//! Favorites are stored as a small set of stable 64-bit keys, one per line in
//! hex, at `$XDG_CONFIG_HOME/zehn/favorites` (falling back to `~/.config`). The
//! key is a hash of agent+text rather than the prompt itself: it keeps the file
//! tiny, avoids newline/encoding hazards from arbitrary prompt text, and stays
//! stable across runs even though the history files are read-only sources we
//! never write back to.
//!
//! The hash is Zig's Wyhash (see `wyhash.rs`) so this Rust build reads and
//! writes exactly the favorites files the previous Zig build produced.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::wyhash::Wyhash;

#[derive(Default, Clone)]
pub struct FavoriteSet {
    keys: BTreeSet<u64>,
}

impl FavoriteSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, k: u64) -> bool {
        self.keys.contains(&k)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Add the key if absent, remove it if present. Returns the new state
    /// (`true` = now a favorite).
    pub fn toggle(&mut self, k: u64) -> bool {
        if self.keys.remove(&k) {
            false
        } else {
            self.keys.insert(k);
            true
        }
    }

    pub fn set(&mut self, k: u64, favorite: bool) {
        if favorite {
            self.keys.insert(k);
        } else {
            self.keys.remove(&k);
        }
    }

    /// Parse the favorites file: one hex u64 per line; blank lines and anything
    /// unparseable are ignored (forward-compatible and hand-edit safe).
    pub fn parse(&mut self, data: &str) {
        for line in data.split('\n') {
            let t = line.trim_matches([' ', '\t', '\r']);
            if t.is_empty() {
                continue;
            }
            if let Ok(k) = u64::from_str_radix(t, 16) {
                self.keys.insert(k);
            }
        }
    }

    /// Serialize to the on-disk form: sorted hex keys, one per line.
    pub fn serialize(&self) -> String {
        let mut out = String::with_capacity(self.keys.len() * 17);
        for k in &self.keys {
            out.push_str(&format!("{k:016x}\n"));
        }
        out
    }

    /// Load the favorites file. A missing file is fine (empty set).
    pub fn load(&mut self, file_path: &Path) {
        if let Ok(data) = std::fs::read_to_string(file_path) {
            self.parse(&data);
        }
    }

    /// Persist to `file_path`, creating the parent directory if needed.
    /// Best-effort: a failure to save must never take down the picker.
    pub fn save(&self, file_path: &Path) {
        if let Some(dir) = file_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file_path, self.serialize());
    }
}

/// Stable key for a prompt: hash of `"<agent>\0<text>"`. Same agent+text always
/// hashes the same, and two different agents with identical text differ — the
/// same distinction dedup makes.
pub fn key(agent_label: &str, text: &str) -> u64 {
    let mut h = Wyhash::new(0);
    h.update(agent_label.as_bytes());
    h.update(&[0]);
    h.update(text.as_bytes());
    h.finish()
}

/// `$XDG_CONFIG_HOME/zehn/favorites`, or `~/.config/zehn/favorites`.
pub fn path_for(home: &str, xdg: Option<&str>) -> PathBuf {
    if let Some(x) = xdg {
        if !x.is_empty() {
            return PathBuf::from(x).join("zehn").join("favorites");
        }
    }
    PathBuf::from(home).join(".config").join("zehn").join("favorites")
}

/// Combined ranking value: higher sorts first. Favorites form a strict tier
/// above non-favorites so a frequently-reused prompt surfaces ahead of an
/// incidental better-scoring match, while score still orders within each tier.
pub fn rank(score: i32, is_fav: bool) -> i64 {
    let tier: i64 = if is_fav { 1 } else { 0 };
    (tier << 40) + score as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_distinguishes_agent() {
        let a1 = key("claude", "fix the bug");
        assert_eq!(a1, key("claude", "fix the bug"));
        assert_ne!(a1, key("codex", "fix the bug"));
        assert_ne!(a1, key("claude", "fix the bugs"));
    }

    #[test]
    fn key_matches_the_zig_build_so_existing_favorites_survive() {
        // Same vector the Zig implementation produced for this pair.
        assert_eq!(key("claude", "fix the bug"), 0x08775375308e09c0);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut s = FavoriteSet::new();
        let k = key("pi", "hello");
        assert!(!s.contains(k));
        assert!(s.toggle(k));
        assert!(s.contains(k));
        assert_eq!(s.len(), 1);
        assert!(!s.toggle(k));
        assert!(!s.contains(k));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn favorites_outrank_non_favorites_regardless_of_score() {
        assert!(rank(-1000, true) > rank(2_000_000, false));
        assert!(rank(50, true) > rank(10, true));
        assert!(rank(50, false) > rank(10, false));
    }

    #[test]
    fn parse_ignores_blanks_and_junk_and_round_trips() {
        let mut s = FavoriteSet::new();
        s.parse("00000000000000ff\n\n  not-hex-garbage\n0000000000000001\n00000000000000ff");
        assert_eq!(s.len(), 2);
        assert!(s.contains(0xff));
        assert!(s.contains(0x1));
        assert_eq!(s.serialize(), "0000000000000001\n00000000000000ff\n");
    }

    #[test]
    fn path_for_honors_xdg_then_falls_back_to_config() {
        assert_eq!(path_for("/home/x", Some("/cfg")), PathBuf::from("/cfg/zehn/favorites"));
        assert_eq!(path_for("/home/x", None), PathBuf::from("/home/x/.config/zehn/favorites"));
        assert_eq!(path_for("/home/x", Some("")), PathBuf::from("/home/x/.config/zehn/favorites"));
    }
}
