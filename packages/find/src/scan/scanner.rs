use std::collections::HashMap;
use std::path::PathBuf;

use crate::agent::Agent;

use super::*;

#[derive(Default)]
pub struct Scanner {
    pub(super) home: PathBuf,
    pub(super) cache_root: PathBuf,
    pub records: Vec<Record>,
    pub(super) claude_titles: HashMap<String, String>,
    pub(super) codex_titles: HashMap<String, String>,
    pub(super) codex_projects: HashMap<String, String>,
    /// Set when an opencode DB exists but could not be read.
    pub opencode_error: Option<String>,
    /// Set when Empryo's thread index exists but could not be read.
    pub empryo_error: Option<String>,
}

impl Scanner {
    pub fn new(home: impl Into<PathBuf>, cache_root: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            cache_root: cache_root.into(),
            ..Default::default()
        }
    }

    pub(super) fn path(&self, suffix: &str) -> PathBuf {
        self.home.join(suffix)
    }

    /// A file in Codex's own folder, which on Windows sits in the Profile folder whatever HOME
    /// says (`ghostex_paths::agent_homes`).
    pub(super) fn codex_path(&self, suffix: &str) -> PathBuf {
        ghostex_paths::agent_homes::codex_default_home(&self.home).join(suffix)
    }

    /// A file in Claude Code's own folder, which on Windows sits under USERPROFILE whatever HOME
    /// says (`ghostex_paths::agent_homes`).
    pub(super) fn claude_path(&self, suffix: &str) -> PathBuf {
        ghostex_paths::agent_homes::claude_default_home(&self.home).join(suffix)
    }

    pub fn scan_all(&mut self) {
        self.scan_claude();
        self.scan_codex();
        self.scan_pi();
        self.scan_opencode();
        self.scan_cursor();
        self.scan_grok();
        self.scan_empryo();
        self.scan_antigravity();
        self.scan_hermes();
        self.scan_copilot();
        self.scan_droid();
        self.scan_omp();
        self.scan_zcode();
        self.dedup();
    }

    /// Collapse identical (agent, text) prompts, keeping the most recent
    /// occurrence (highest ts). Preserves first-seen ordering otherwise.
    pub fn dedup(&mut self) {
        let mut seen: HashMap<(Agent, String), usize> = HashMap::new();
        let mut out: Vec<Record> = Vec::with_capacity(self.records.len());
        for rec in std::mem::take(&mut self.records) {
            let key = (rec.agent, rec.text.clone());
            match seen.get(&key) {
                Some(&pos) => {
                    if rec.ts > out[pos].ts {
                        out[pos] = rec;
                    }
                }
                None => {
                    seen.insert(key, out.len());
                    out.push(rec);
                }
            }
        }
        self.records = out;
    }
}
