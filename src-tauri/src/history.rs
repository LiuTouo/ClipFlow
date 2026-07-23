use crate::models::{Clip, ClipKind, AppConfig};

pub struct HistoryStore {
    pub clips: Vec<Clip>,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self { clips: Vec::with_capacity(128) }
    }

    /// Insert a Clip, deduplicating by content hash. Returns the stored Clip
    /// plus the ids of any Clips evicted by capacity limits (so callers can
    /// keep persistence in sync).
    pub fn insert(&mut self, clip: Clip, config: &AppConfig) -> (Clip, Vec<String>) {
        if let Some(existing) = self.clips.iter_mut().find(|c| c.content_hash == clip.content_hash) {
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
        if let Some(pos) = self.clips.iter().position(|c| c.content_hash == content_hash) {
            if pos != 0 {
                let item = self.clips.remove(pos);
                self.clips.insert(0, item);
            }
        }
    }

    /// Index of the oldest (smallest `captured_at`) unpinned Clip matching
    /// `pred`. Eviction must go by true age, not vec position: new Clips are
    /// pushed to the back of the vec, so evicting from the back would discard
    /// the fresh Clip and keep the oldest one forever.
    fn oldest_unpinned(clips: &[Clip], pred: impl Fn(&Clip) -> bool) -> Option<usize> {
        clips
            .iter()
            .enumerate()
            .filter(|(_, c)| pred(c) && !c.pinned)
            .min_by_key(|(_, c)| c.captured_at)
            .map(|(i, _)| i)
    }

    /// Evict over-limit Clips (oldest unpinned first). Returns evicted ids.
    fn enforce_limits(&mut self, config: &AppConfig) -> Vec<String> {
        let mut evicted = Vec::new();

        // Evict oldest unpinned non-image Clips
        loop {
            let text_count = self.clips.iter().filter(|c| c.kind != ClipKind::Image).count();
            if text_count <= config.text_count_limit { break; }
            let idx = Self::oldest_unpinned(&self.clips, |c| c.kind != ClipKind::Image);
            if let Some(i) = idx { evicted.push(self.clips.remove(i).id); } else { break; }
        }

        // Evict oldest unpinned image Clips by count + memory
        let image_memory_limit = (config.image_memory_budget_mb as u64) * 1024 * 1024;
        loop {
            let image_count = self.clips.iter().filter(|c| c.kind == ClipKind::Image).count();
            let image_memory: u64 = self.clips.iter()
                .filter(|c| c.kind == ClipKind::Image).map(|c| c.byte_size).sum();
            if image_count <= config.image_count_limit && image_memory <= image_memory_limit { break; }
            let idx = Self::oldest_unpinned(&self.clips, |c| c.kind == ClipKind::Image);
            if let Some(i) = idx { evicted.push(self.clips.remove(i).id); } else { break; }
        }

        evicted
    }

    pub fn get_all(&self) -> Vec<Clip> {
        let mut all: Vec<Clip> = self.clips.clone();
        all.sort_by(|a, b| {
            b.pinned.cmp(&a.pinned).then(b.captured_at.cmp(&a.captured_at))
        });
        all
    }

    pub fn delete(&mut self, id: &str) -> Option<Clip> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            Some(self.clips.remove(pos))
        } else {
            None
        }
    }

    /// Look up a Clip by content hash (used to preserve the original source
    /// app when the monitor re-captures content ClipFlow itself wrote).
    pub fn find_by_hash(&self, content_hash: &str) -> Option<Clip> {
        self.clips.iter().find(|c| c.content_hash == content_hash).cloned()
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
        let cfg = AppConfig { text_count_limit: 3, ..AppConfig::default() };
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
        let cfg = AppConfig { text_count_limit: 3, ..AppConfig::default() };
        for i in 1..=3 {
            h.insert(text_clip(&format!("c{i}"), i), &cfg);
        }
        h.set_pinned("c1", true).unwrap();
        let (_, evicted) = h.insert(text_clip("c4", 4), &cfg);
        assert_eq!(evicted, vec!["c2".to_string()]);
        assert!(h.clips.iter().any(|c| c.id == "c1"));
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
}
