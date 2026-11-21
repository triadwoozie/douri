//! Phrase caching for rendered vocal audio.
//!
//! Hashes phrase notes, lyrics, tempo, and voicebank state to avoid redundant
//! resynthesis when edits only touch other parts of the project.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// In-memory cache for rendered vocal phrases.
pub struct PhraseCache {
    entries: HashMap<u64, Arc<[f32]>>,
    max_entries: usize,
}

impl Default for PhraseCache {
    fn default() -> Self {
        Self::new(256)
    }
}

impl PhraseCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
        }
    }

    /// Computes a hash key for a vocal phrase.
    pub fn compute_key(voicebank_name: &str, tempo: f64, notes: &[(f64, f64, u8, String)]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        voicebank_name.hash(&mut hasher);
        tempo.to_bits().hash(&mut hasher);
        for (beat, dur, tone, lyric) in notes {
            beat.to_bits().hash(&mut hasher);
            dur.to_bits().hash(&mut hasher);
            tone.hash(&mut hasher);
            lyric.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Retrieves a cached phrase audio buffer.
    pub fn get(&self, key: u64) -> Option<Arc<[f32]>> {
        self.entries.get(&key).cloned()
    }

    /// Stores a phrase audio buffer in cache.
    pub fn insert(&mut self, key: u64, sound: Arc<[f32]>) {
        if self.entries.len() >= self.max_entries {
            // Drop an arbitrary entry
            if let Some(first_key) = self.entries.keys().next().copied() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(key, sound);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_stores_and_retrieves() {
        let mut cache = PhraseCache::new(10);
        let notes = vec![(0.0, 1.0, 60, "か".to_string())];
        let key = PhraseCache::compute_key("Defoko", 120.0, &notes);

        let data: Arc<[f32]> = vec![0.5f32; 100].into();
        cache.insert(key, data.clone());

        let retrieved = cache.get(key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 100);
    }
}
