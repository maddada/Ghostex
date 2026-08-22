use std::path::PathBuf;
use std::time::Instant;

use tracing::error;

use super::{App, Mode};
use crate::workspace::Workspace;

impl App {
    pub(crate) fn enable_ghostex_mode(&mut self) {
        // CDXC:GhostexTui2 2026-06-16-22:52: The experimental `gx 2` TUI keeps upstream Herdr's sidebar-first UX while replacing Herdr's local agent inventory with gxserver sessions.
        self.state.ghostex_mode = true;
        self.state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        self.state.sidebar_collapsed = false;
        self.state.mode = Mode::Navigate;
        self.state.ghostex_status = Some("loading Ghostex sessions".to_string());
        self.next_ghostex_session_refresh = Some(Instant::now());
        self.refresh_ghostex_sessions();
    }

    pub(crate) fn maybe_refresh_ghostex_sessions(&mut self, now: Instant) -> bool {
        let Some(next_refresh) = self.next_ghostex_session_refresh else {
            return false;
        };
        if now < next_refresh {
            return false;
        }
        self.refresh_ghostex_sessions();
        true
    }

    pub(crate) fn refresh_ghostex_sessions(&mut self) {
        match crate::ghostex::fetch_sessions() {
            Ok(sessions) => {
                self.state.ghostex_sessions = sessions;
                let max_scroll = crate::ghostex::sidebar_rows(&self.state.ghostex_sessions)
                    .len()
                    .saturating_sub(1);
                self.state.agent_panel_scroll = self.state.agent_panel_scroll.min(max_scroll);
                self.state.ghostex_status = if self.state.ghostex_sessions.is_empty() {
                    Some("no Ghostex sessions found".to_string())
                } else {
                    None
                };
            }
            Err(err) => {
                self.state.ghostex_status = Some(format!("could not load Ghostex sessions: {err}"));
            }
        }
        self.next_ghostex_session_refresh =
            Some(Instant::now() + crate::ghostex::SESSION_LIST_REFRESH);
    }

    pub(crate) fn attach_ghostex_session_by_key(&mut self, key: &str) {
        let Some(session) = self
            .state
            .ghostex_sessions
            .iter()
            .find(|session| crate::ghostex::session_identity_key(session) == key)
            .cloned()
        else {
            self.state.ghostex_status = Some(format!("Ghostex session {key} is no longer listed"));
            self.refresh_ghostex_sessions();
            return;
        };

        if self.focus_existing_ghostex_workspace(key) {
            return;
        }

        if crate::ghostex::session_activity(&session)
            == Some(crate::ghostex::SessionActivity::Attention)
        {
            let _ = crate::ghostex::acknowledge_session_attention(&session);
        }

        let cwd = session
            .project_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        let attach_command = crate::ghostex::attach_shell_command(&session);
        let argv = vec!["/bin/zsh".to_string(), "-lc".to_string(), attach_command];
        let (rows, cols) = self.state.estimate_pane_size();
        match Workspace::new_argv_command_with_extra_env(
            cwd,
            rows,
            cols,
            &argv,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            self.event_tx.clone(),
            self.render_notify.clone(),
            self.render_dirty.clone(),
            // CDXC:GhostexTui 2026-07-01-02:10: Sessions spawned by the promoted GX 2 app should identify as `ghostex-tui`, matching the public binary that bare `gx` and `ghostex` now launch.
            vec![("TERM_PROGRAM".to_string(), "ghostex-tui".to_string())],
        ) {
            Ok((mut workspace, terminal, runtime)) => {
                workspace.set_custom_name(crate::ghostex::workspace_title(&session));
                if let Some(tab) = workspace.tabs.get_mut(0) {
                    tab.set_custom_name("attach".to_string());
                }
                let workspace_id = workspace.id.clone();
                self.terminal_runtimes.insert(terminal.id.clone(), runtime);
                self.state.terminals.insert(terminal.id.clone(), terminal);
                self.state.workspaces.push(workspace);
                let ws_idx = self.state.workspaces.len() - 1;
                self.state.switch_workspace(ws_idx);
                self.state.mode = Mode::Terminal;
                self.state.ghostex_active_session_key = Some(key.to_string());
                self.state
                    .ghostex_session_workspaces
                    .insert(key.to_string(), workspace_id);
                self.state.ghostex_status = None;
            }
            Err(err) => {
                error!(err = %err, "failed to create Ghostex attach workspace");
                self.state.ghostex_status =
                    Some(format!("could not attach Ghostex session: {err}"));
                self.state.mode = Mode::Navigate;
            }
        }
    }

    pub(crate) fn create_ghostex_terminal(&mut self) {
        let source_session = self
            .state
            .ghostex_active_session_key
            .as_deref()
            .and_then(|key| {
                self.state
                    .ghostex_sessions
                    .iter()
                    .find(|session| crate::ghostex::session_identity_key(session) == key)
            })
            .or_else(|| self.state.ghostex_sessions.first());
        let Some(project_id) = source_session
            .and_then(|session| session.project_id.as_deref())
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
        else {
            self.state.ghostex_status =
                Some("select a Ghostex project/session before creating a terminal".to_string());
            return;
        };
        let group_id = source_session.and_then(|session| {
            session
                .group_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
        });

        self.create_ghostex_terminal_for(Some(project_id), group_id);
    }

    pub(crate) fn create_ghostex_terminal_for(
        &mut self,
        project_id: Option<String>,
        group_id: Option<String>,
    ) {
        let Some(project_id) = project_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
        else {
            self.state.ghostex_status =
                Some("select a Ghostex project/session before creating a terminal".to_string());
            return;
        };

        match crate::ghostex::create_terminal(Some(project_id.as_str()), group_id.as_deref()) {
            Ok(created) => {
                self.refresh_ghostex_sessions();
                let created_session_id = created
                    .session
                    .and_then(|session| session.ghostex_id.or(session.session_id));
                if let Some(session_id) = created_session_id {
                    let key = crate::ghostex::session_identity_key_parts(
                        Some(project_id.as_str()),
                        &session_id,
                    );
                    if self
                        .state
                        .ghostex_sessions
                        .iter()
                        .any(|session| crate::ghostex::session_identity_key(session) == key)
                    {
                        self.attach_ghostex_session_by_key(&key);
                        return;
                    }
                }
                self.state.ghostex_status = Some(
                    "created terminal, but it was not found in the refreshed session list"
                        .to_string(),
                );
            }
            Err(err) => {
                self.state.ghostex_status =
                    Some(format!("could not create Ghostex terminal: {err}"));
            }
        }
    }

    fn focus_existing_ghostex_workspace(&mut self, key: &str) -> bool {
        let Some(workspace_id) = self.state.ghostex_session_workspaces.get(key).cloned() else {
            return false;
        };
        let Some(ws_idx) = self
            .state
            .workspaces
            .iter()
            .position(|workspace| workspace.id == workspace_id)
        else {
            self.state.ghostex_session_workspaces.remove(key);
            return false;
        };
        self.state.switch_workspace(ws_idx);
        self.state.mode = Mode::Terminal;
        self.state.ghostex_active_session_key = Some(key.to_string());
        true
    }
}
