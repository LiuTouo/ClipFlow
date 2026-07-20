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

    /// Evict over-limit Clips (oldest unpinned first). Returns evicted ids.
    fn enforce_limits(&mut self, config: &AppConfig) -> Vec<String> {
        let mut evicted = Vec::new();

        // Evict oldest unpinned non-image Clips
        loop {
            let text_count = self.clips.iter().filter(|c| c.kind != ClipKind::Image).count();
            if text_count <= config.text_count_limit { break; }
            let idx = (0..self.clips.len()).rev()
                .find(|&i| self.clips[i].kind != ClipKind::Image && !self.clips[i].pinned);
            if let Some(i) = idx { evicted.push(self.clips.remove(i).id); } else { break; }
        }

        // Evict oldest unpinned image Clips by count + memory
        let image_memory_limit = (config.image_memory_budget_mb as u64) * 1024 * 1024;
        loop {
            let image_count = self.clips.iter().filter(|c| c.kind == ClipKind::Image).count();
            let image_memory: u64 = self.clips.iter()
                .filter(|c| c.kind == ClipKind::Image).map(|c| c.byte_size).sum();
            if image_count <= config.image_count_limit && image_memory <= image_memory_limit { break; }
            let idx = (0..self.clips.len()).rev()
                .find(|&i| self.clips[i].kind == ClipKind::Image && !self.clips[i].pinned);
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
