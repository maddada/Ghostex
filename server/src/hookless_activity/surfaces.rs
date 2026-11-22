//! The resume-repair entries (`agent-hooks/claude-hook-sessions.json`) the observer writes for
//! every conversation a pane shows, as Claude's hooks do (see `write_claude_surface_record`).

use std::collections::HashMap;

use super::claude::{expected_transcript_path, transcript_path, ClaudeObserver};
use crate::paths::GxserverPaths;

#[derive(Default)]
pub(super) struct SurfaceRecords {
    /// The conversation last written for each pane.
    written: HashMap<(String, String), String>,
}

impl SurfaceRecords {
    pub(super) fn poll(&mut self, paths: &GxserverPaths, observer: &ClaudeObserver) {
        let hook_state_dir = paths.app_state_dir.join("agent-hooks");
        let mut live = HashMap::new();
        for (pane, state) in observer.pane_states() {
            let Some(conversation) = state.conversation.as_deref() else {
                continue;
            };
            live.insert(pane.clone(), conversation.to_string());
            if self.written.get(pane).map(String::as_str) == Some(conversation) {
                continue;
            }
            // Like the hooks, the path Claude will write the conversation to, written yet or not:
            // the repair only takes an entry whose transcript is on disk.
            let transcript = transcript_path(&state.config_dir, state.cwd.as_deref(), conversation)
                .or_else(|| {
                    state
                        .cwd
                        .as_deref()
                        .map(|cwd| expected_transcript_path(&state.config_dir, cwd, conversation))
                });
            crate::agent_hooks::write_claude_surface_record(
                &hook_state_dir,
                conversation,
                &pane.0,
                &state.surface,
                state.cwd.as_deref(),
                transcript.as_deref().and_then(|path| path.to_str()),
                state.pid,
            );
        }
        self.written = live;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_repair_reads_what_the_observer_writes() {
        let home = std::env::temp_dir().join(format!("gx-surfaces-{}", uuid::Uuid::new_v4()));
        let paths = crate::paths::get_gxserver_paths(Some(home.clone()));
        let dir = paths.app_state_dir.join("agent-hooks");
        let transcript = home.join("t.jsonl");
        let write = |conversation: &str| {
            crate::agent_hooks::write_claude_surface_record(
                &dir,
                conversation,
                "P1",
                "S1-P1-G1",
                Some("/w"),
                transcript.to_str(),
                42,
            )
        };
        write("first");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write("second");
        let records = crate::agent_hooks::read_claude_hook_surface_records(&paths, "S1-P1-G1");
        let ids: Vec<_> = records
            .iter()
            .map(|record| record.agent_session_id.as_str())
            .collect();
        assert_eq!(ids, vec!["first", "second"]);
        assert_eq!(
            records[1].agent_session_path.as_deref(),
            transcript.to_str()
        );
        assert!(
            crate::agent_hooks::read_claude_hook_surface_records(&paths, "S1-P1-G2").is_empty()
        );
        std::fs::remove_dir_all(&home).unwrap();
    }
}
