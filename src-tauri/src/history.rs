use crate::models::{AppConfig, Clip, ClipKind};

pub struct HistoryStore {
    pub clips: Vec<Clip>,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self {
            clips: Vec::with_capacity(128),
        }
    }

    /// Insert a Clip, deduplicating by content hash. Returns the stored Clip
    /// plus the ids of any Clips evicted by capacity limits (so callers can
    /// keep persistence in sync).
    pub fn insert(&mut self, clip: Clip, config: &AppConfig) -> (Clip, Vec<String>) {
        if let Some(existing) = self
            .clips
            .iter_mut()
            .find(|c| c.content_hash == clip.content_hash)
        {
            existing.captured_at = clip.captured_at;
            existing.source_exe = clip.source_exe.clone();
            existing.source_title = clip.source_title.clone();
            let result = existing.clone();
            self.move_to_front(&clip.content_hash);
            return (result, Vec::new());
        }

        let result = clip.clone();
        self.clips.push(clip);
        let evicted = self.enforce_limits(config);
        (result, evicted)
    }

    fn move_to_front(&mut self, content_hash: &str) {
        if let Some(pos) = self
            .clips
            .iter()
            .position(|c| c.content_hash == content_hash)
        {
            if pos != 0 {
                let item = self.clips.remove(pos);
                self.clips.insert(0, item);
            }
        }
    }

    /// Evict over-limit Clips (oldest unpinned first). Returns evicted ids.
    /// Eviction goes by true age, not vec position: new Clips are pushed to
    /// the back of the vec, so evicting from the back would discard the fresh
    /// Clip and keep the oldest one forever.
    fn enforce_limits(&mut self, config: &AppConfig) -> Vec<String> {
        let refs: Vec<&Clip> = self.clips.iter().collect();
        let victims = Self::plan_evictions(&refs, config);
        let evicted = victims
            .iter()
            .map(|&i| self.clips[i].id.clone())
            .collect::<Vec<_>>();
        for &i in victims.iter().rev() {
            self.clips.remove(i);
        }
        evicted
    }

    /// Indices (into `clips`, which includes the just-pushed candidate at the
    /// end) of the Clips to evict under `config`, oldest unpinned first —
    /// non-image Clips by count, then image Clips by count + byte budget.
    /// Pure: no mutation, so `insert` and `preview_evictions` both route
    /// through it and can never drift apart.
    fn plan_evictions(clips: &[&Clip], config: &AppConfig) -> Vec<usize> {
        let mut victims: Vec<usize> = Vec::new();

        // Evict oldest unpinned non-image Clips by count
        loop {
            let text_count = clips
                .iter()
                .enumerate()
                .filter(|(i, c)| !victims.contains(i) && c.kind != ClipKind::Image)
                .count();
            if text_count <= config.text_count_limit {
                break;
            }
            let idx = clips
                .iter()
                .enumerate()
                .filter(|(i, c)| !victims.contains(i) && c.kind != ClipKind::Image && !c.pinned)
                .min_by_key(|(_, c)| c.captured_at)
                .map(|(i, _)| i);
            match idx {
                Some(i) => victims.push(i),
                None => break,
            }
        }

        // Evict oldest unpinned image Clips by count + memory
        let image_memory_limit = config.image_memory_budget_mb * 1024 * 1024;
        loop {
            let images: Vec<(usize, u64)> = clips
                .iter()
                .enumerate()
                .filter(|(i, c)| !victims.contains(i) && c.kind == ClipKind::Image)
                .map(|(i, c)| (i, c.byte_size))
                .collect();
            let image_count = images.len();
            let image_memory: u64 = images.iter().map(|(_, b)| *b).sum();
            if image_count <= config.image_count_limit && image_memory <= image_memory_limit {
                break;
            }
            let idx = clips
                .iter()
                .enumerate()
                .filter(|(i, c)| !victims.contains(i) && c.kind == ClipKind::Image && !c.pinned)
                .min_by_key(|(_, c)| c.captured_at)
                .map(|(i, _)| i);
            match idx {
                Some(i) => victims.push(i),
                None => break,
            }
        }

        victims
    }

    /// Ids an `insert` of `clip` would evict, computed without mutating the
    /// store. Parity with `insert` is exact by construction: both run the
    /// same planner over the same clip set — `insert` after pushing the
    /// candidate, this over the current Clips plus the candidate. A
    /// content-hash dedup hit evicts nothing in both paths.
    pub fn preview_evictions(&self, clip: &Clip, config: &AppConfig) -> Vec<String> {
        if self
            .clips
            .iter()
            .any(|c| c.content_hash == clip.content_hash)
        {
            return Vec::new();
        }
        let mut refs: Vec<&Clip> = self.clips.iter().collect();
        refs.push(clip);
        Self::plan_evictions(&refs, config)
            .iter()
            .map(|&i| refs[i].id.clone())
            .collect()
    }

    pub fn get_all(&self) -> Vec<Clip> {
        let mut all: Vec<Clip> = self.clips.clone();
        Self::sort_display(&mut all);
        all
    }

    /// All Clips in display order with raw image bytes stripped (they never
    /// cross IPC — see models::Clip::image_data).
    pub fn get_all_for_ipc(&self) -> Vec<Clip> {
        let mut all: Vec<Clip> = self.clips.iter().map(Clip::meta_clone).collect();
        Self::sort_display(&mut all);
        all
    }

    fn sort_display(clips: &mut [Clip]) {
        clips.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.captured_at.cmp(&a.captured_at))
        });
    }

    /// One Clip by id, cloned (full, including image bytes) — for a single
    /// entry's preview. A narrow clone: never go through get_all() to locate
    /// one Clip's content.
    pub fn get_clip(&self, id: &str) -> Option<Clip> {
        self.clips.iter().find(|c| c.id == id).cloned()
    }

    /// Raw image bytes for an Image Clip, checked by kind: a Text/File id
    /// gets "not an image" instead of a misleading "not found".
    pub fn get_clip_image(&self, id: &str) -> Result<Vec<u8>, String> {
        match self.clips.iter().find(|c| c.id == id) {
            Some(c) if c.kind == ClipKind::Image => c
                .image_data
                .clone()
                .ok_or_else(|| "Image data missing".to_string()),
            Some(_) => Err("Clip is not an image".to_string()),
            None => Err("Clip not found".to_string()),
        }
    }

    pub fn delete(&mut self, id: &str) -> Option<Clip> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            Some(self.clips.remove(pos))
        } else {
            None
        }
    }

    /// (source_exe, source_title) of the Clip with this content hash — used
    /// to preserve the original source app when the monitor re-captures
    /// content Mnemark itself wrote. Returns only strings: cloning a whole
    /// Clip here would copy up to 10 MB of image bytes for two fields.
    pub fn source_by_hash(&self, content_hash: &str) -> Option<(String, String)> {
        self.clips
            .iter()
            .find(|c| c.content_hash == content_hash)
            .map(|c| (c.source_exe.clone(), c.source_title.clone()))
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> Result<(), String> {
        if pinned {
            let pin_count = self.clips.iter().filter(|c| c.pinned).count();
            if pin_count >= 10 {
                return Err("Maximum 10 pinned Clips".to_string());
            }
        }
        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) {
            clip.pinned = pinned;
            Ok(())
        } else {
            Err("Clip not found".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: &str, kind: ClipKind, captured_at: u64, byte_size: u64) -> Clip {
        Clip {
            id: id.to_string(),
            kind,
            text_content: None,
            file_paths: None,
            image_data: None,
            thumbnail_base64: None,
            content_hash: format!("hash-{id}"),
            preview: id.to_string(),
            truncated: false,
            source_exe: "test.exe".to_string(),
            source_title: String::new(),
            source_icon: None,
            captured_at,
            pinned: false,
            byte_size,
        }
    }

    fn text_clip(id: &str, captured_at: u64) -> Clip {
        clip(id, ClipKind::Text, captured_at, 1)
    }

    #[test]
    fn over_limit_evicts_oldest_not_the_new_clip() {
        // Regression: eviction used to scan from the back of the vec — where
        // push() had just placed the new Clip — so a full history discarded
        // every fresh capture and kept the oldest Clips forever.
        let mut h = HistoryStore::new();
        let cfg = AppConfig {
            text_count_limit: 3,
            ..AppConfig::default()
        };
        for i in 1..=3 {
            h.insert(text_clip(&format!("c{i}"), i), &cfg);
        }
        let (_, evicted) = h.insert(text_clip("c4", 4), &cfg);
        assert_eq!(evicted, vec!["c1".to_string()]);
        assert!(h.clips.iter().any(|c| c.id == "c4"));
        assert_eq!(h.clips.len(), 3);
    }

    #[test]
    fn pinned_clips_are_never_evicted() {
        let mut h = HistoryStore::new();
        let cfg = AppConfig {
            text_count_limit: 3,
            ..AppConfig::default()
        };
        for i in 1..=3 {
            h.insert(text_clip(&format!("c{i}"), i), &cfg);
        }
        h.set_pinned("c1", true).unwrap();
        let (_, evicted) = h.insert(text_clip("c4", 4), &cfg);
        assert_eq!(evicted, vec!["c2".to_string()]);
        assert!(h.clips.iter().any(|c| c.id == "c1"));
    }

    #[test]
    fn source_by_hash_returns_only_source_fields() {
        let mut h = HistoryStore::new();
        let cfg = AppConfig::default();
        let mut c = text_clip("c1", 1);
        c.source_exe = "Code.exe".to_string();
        c.source_title = "main.rs".to_string();
        h.insert(c, &cfg);
        assert_eq!(
            h.source_by_hash("hash-c1"),
            Some(("Code.exe".to_string(), "main.rs".to_string()))
        );
        assert_eq!(h.source_by_hash("nope"), None);
    }

    #[test]
    fn get_all_for_ipc_strips_image_bytes_but_keeps_order() {
        let mut h = HistoryStore::new();
        let cfg = AppConfig::default();
        let mut c = clip("i1", ClipKind::Image, 1, 100);
        c.image_data = Some(vec![1u8; 1024]);
        h.insert(c, &cfg);
        h.insert(text_clip("t1", 2), &cfg);

        let all = h.get_all_for_ipc();
        assert_eq!(all.len(), 2);
        // Display order preserved (newest first).
        assert_eq!(all[0].id, "t1");
        assert_eq!(all[1].id, "i1");
        assert_eq!(all[1].image_data, None);

        // Full get_all keeps the bytes (the persistence dump needs them).
        assert_eq!(h.get_all()[1].image_data, Some(vec![1u8; 1024]));
    }

    #[test]
    fn get_clip_image_distinguishes_missing_wrong_kind_and_ok() {
        let mut h = HistoryStore::new();
        let cfg = AppConfig::default();
        let mut img = clip("i1", ClipKind::Image, 1, 4);
        img.image_data = Some(vec![1u8; 4]);
        h.insert(img, &cfg);
        h.insert(text_clip("t1", 2), &cfg);

        assert_eq!(h.get_clip_image("i1").unwrap(), vec![1u8; 4]);
        assert_eq!(h.get_clip_image("t1").unwrap_err(), "Clip is not an image");
        assert_eq!(h.get_clip_image("nope").unwrap_err(), "Clip not found");
    }

    #[test]
    fn image_memory_budget_evicts_oldest_image_first() {
        let mut h = HistoryStore::new();
        let cfg = AppConfig {
            image_count_limit: 10,
            image_memory_budget_mb: 1, // 1,048,576 bytes
            ..AppConfig::default()
        };
        h.insert(clip("i1", ClipKind::Image, 1, 600_000), &cfg);
        let (_, evicted) = h.insert(clip("i2", ClipKind::Image, 2, 600_000), &cfg);
        assert_eq!(evicted, vec!["i1".to_string()]);
        let (_, evicted) = h.insert(clip("i3", ClipKind::Image, 3, 600_000), &cfg);
        assert_eq!(evicted, vec!["i2".to_string()]);
        assert_eq!(h.clips.len(), 1);
        assert_eq!(h.clips[0].id, "i3");
    }

    /// preview_evictions must equal what insert actually evicts — the undo
    /// DB-first path plans its transaction from the preview, so any drift
    /// would desync SQLite from memory. Table-driven across every limit kind.
    #[test]
    fn preview_evictions_matches_insert_across_all_limit_kinds() {
        struct Case {
            name: &'static str,
            config: AppConfig,
            // (id, kind, captured_at, byte_size, pinned) rows pre-seeded.
            seed: Vec<(&'static str, ClipKind, u64, u64, bool)>,
            candidate: (&'static str, ClipKind, u64, u64),
        }
        let cases = vec![
            Case {
                name: "text count limit",
                config: AppConfig {
                    text_count_limit: 2,
                    ..AppConfig::default()
                },
                seed: vec![
                    ("c1", ClipKind::Text, 1, 1, false),
                    ("c2", ClipKind::Text, 2, 1, false),
                ],
                candidate: ("c3", ClipKind::Text, 3, 1),
            },
            Case {
                name: "image count limit",
                config: AppConfig {
                    image_count_limit: 1,
                    ..AppConfig::default()
                },
                seed: vec![("i1", ClipKind::Image, 1, 10, false)],
                candidate: ("i2", ClipKind::Image, 2, 10),
            },
            Case {
                name: "image byte budget",
                config: AppConfig {
                    image_memory_budget_mb: 1,
                    ..AppConfig::default()
                },
                seed: vec![("i1", ClipKind::Image, 1, 600_000, false)],
                candidate: ("i2", ClipKind::Image, 2, 600_000),
            },
            Case {
                name: "pinned exempt from eviction",
                config: AppConfig {
                    text_count_limit: 2,
                    ..AppConfig::default()
                },
                seed: vec![
                    ("c1", ClipKind::Text, 1, 1, true),
                    ("c2", ClipKind::Text, 2, 1, false),
                ],
                candidate: ("c3", ClipKind::Text, 3, 1),
            },
            Case {
                name: "combined text and image limits",
                config: AppConfig {
                    text_count_limit: 1,
                    image_count_limit: 1,
                    ..AppConfig::default()
                },
                seed: vec![
                    ("c1", ClipKind::Text, 1, 1, false),
                    ("i1", ClipKind::Image, 2, 100, false),
                ],
                candidate: ("c2", ClipKind::Text, 3, 1),
            },
            Case {
                name: "no limits hit",
                config: AppConfig::default(),
                seed: vec![("c1", ClipKind::Text, 1, 1, false)],
                candidate: ("c2", ClipKind::Text, 2, 1),
            },
        ];
        for case in cases {
            let cfg = case.config;
            let mut seeded = HistoryStore::new();
            for (id, kind, at, size, pinned) in &case.seed {
                let mut c = clip(id, kind.clone(), *at, *size);
                c.pinned = *pinned;
                seeded.insert(c, &cfg);
            }
            let (cid, ckind, cat, csize) = case.candidate;
            let candidate = clip(cid, ckind, cat, csize);

            let preview = seeded.preview_evictions(&candidate, &cfg);
            let (_, applied) = seeded.insert(candidate, &cfg);
            assert_eq!(preview, applied, "parity failed: {}", case.name);
        }
    }

    #[test]
    fn preview_evictions_empty_on_content_hash_dedup() {
        let cfg = AppConfig::default();
        let mut h = HistoryStore::new();
        let c1 = text_clip("c1", 1);
        h.insert(c1.clone(), &cfg);
        // Same content hash, different id: insert takes the dedup path.
        let mut dup = text_clip("other-id", 2);
        dup.content_hash = c1.content_hash.clone();
        assert!(h.preview_evictions(&dup, &cfg).is_empty());
    }
}
