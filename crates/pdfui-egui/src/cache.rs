//! The pictures of pages kept for drawing: a few at a time, the least recently used dropped first,
//! so a long document never holds more than a fixed amount of memory.

use std::collections::HashMap;

/// Most pictures kept.
pub const MAX_ENTRIES: usize = 16;
/// Most pixels kept over all pictures (48 million: about 190 MB of RGBA).
pub const MAX_PIXELS: u64 = 48_000_000;

struct Entry<T> {
    value: T,
    pixels: u64,
    used: u64,
}

/// Pictures by (page, width in pixels).
pub struct Cache<T> {
    entries: HashMap<(usize, u32), Entry<T>>,
    clock: u64,
    pixels: u64,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self { entries: HashMap::new(), clock: 0, pixels: 0 }
    }
}

impl<T> Cache<T> {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.pixels = 0;
    }

    /// The picture of `page` at exactly `width`, marked as used.
    pub fn exact(&mut self, page: usize, width: u32) -> Option<&T> {
        self.clock += 1;
        let clock = self.clock;
        let entry = self.entries.get_mut(&(page, width))?;
        entry.used = clock;
        Some(&entry.value)
    }

    /// The picture of `page` whose width is nearest `width` (a stand-in while the right one is
    /// drawn), marked as used.
    pub fn nearest(&mut self, page: usize, width: u32) -> Option<&T> {
        self.clock += 1;
        let clock = self.clock;
        let key = self.entries.keys().filter(|(p, _)| *p == page).min_by_key(|(_, w)| w.abs_diff(width)).copied()?;
        let entry = self.entries.get_mut(&key)?;
        entry.used = clock;
        Some(&entry.value)
    }

    /// Keep a picture, then drop the least recently used ones (never this one) until the cache is
    /// within its limits.
    pub fn insert(&mut self, page: usize, width: u32, value: T, pixels: u64) {
        self.clock += 1;
        if let Some(old) = self.entries.insert((page, width), Entry { value, pixels, used: self.clock }) {
            self.pixels = self.pixels.saturating_sub(old.pixels);
        }
        self.pixels = self.pixels.saturating_add(pixels);
        while self.entries.len() > MAX_ENTRIES || self.pixels > MAX_PIXELS {
            let Some(oldest) = self.entries.iter().filter(|(k, _)| **k != (page, width)).min_by_key(|(_, e)| e.used).map(|(k, _)| *k) else { break };
            if let Some(gone) = self.entries.remove(&oldest) {
                self.pixels = self.pixels.saturating_sub(gone.pixels);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_found_exactly_or_by_the_nearest_width() {
        let mut c = Cache::default();
        c.insert(3, 400, "a", 10);
        c.insert(3, 800, "b", 10);
        c.insert(4, 500, "c", 10);
        assert_eq!(c.exact(3, 800), Some(&"b"));
        assert_eq!(c.exact(3, 700), None);
        assert_eq!(c.nearest(3, 700), Some(&"b"));
        assert_eq!(c.nearest(3, 500), Some(&"a"));
        assert_eq!(c.nearest(9, 500), None);
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn the_least_recently_used_goes_first_and_never_the_new_one() {
        let mut c = Cache::default();
        for p in 0..MAX_ENTRIES {
            c.insert(p, 100, p, 1);
        }
        assert_eq!(c.len(), MAX_ENTRIES);
        // Page 0 is used again, so page 1 is the oldest.
        assert!(c.exact(0, 100).is_some());
        c.insert(99, 100, 99, 1);
        assert_eq!(c.len(), MAX_ENTRIES);
        assert!(c.exact(0, 100).is_some() && c.exact(99, 100).is_some());
        assert!(c.exact(1, 100).is_none(), "the oldest was dropped");
    }

    #[test]
    fn pixels_are_limited_too_and_one_big_picture_stays() {
        let mut c = Cache::default();
        c.insert(0, 100, 0, MAX_PIXELS / 2);
        c.insert(1, 100, 1, MAX_PIXELS / 2);
        c.insert(2, 100, 2, MAX_PIXELS / 2);
        assert_eq!(c.len(), 2);
        assert!(c.exact(0, 100).is_none());
        // A picture bigger than the whole budget is still kept (it is the one being shown).
        c.insert(5, 100, 5, MAX_PIXELS * 2);
        assert!(c.exact(5, 100).is_some());
        assert_eq!(c.len(), 1);
        // Replacing a picture doesn't count it twice.
        c.clear();
        c.insert(0, 100, 0, MAX_PIXELS / 2);
        c.insert(0, 100, 1, MAX_PIXELS / 2);
        c.insert(1, 100, 2, MAX_PIXELS / 2);
        assert_eq!(c.len(), 2);
    }
}
