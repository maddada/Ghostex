// Cluster: zmx client visibility announcements for Agents GPUI-engine
// terminals (displayed, chat or parked), including the resting-width grid a
// chat client keeps locally.

use std::collections::HashSet;

use crate::app::model::*;
use crate::terminal_model::ZMX_RESTING_GRID_COLS;
use crate::terminal_model::{zmx_client_chat_sequence, zmx_client_hidden_sequence};
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiEngineTerminalZmxVisibility {
    Visible,
    Chat,
    Parked,
}

/// Last visibility claim made to the zmx attach client behind one Agents
/// engine terminal, plus the grid that claim carried. Runtime-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiEngineTerminalAnnouncedVisibility {
    pub(crate) view_id: gpui::EntityId,
    pub(crate) visibility: GpuiEngineTerminalZmxVisibility,
    pub(crate) rows: u16,
    pub(crate) cols: u16,
}

impl GhostexGpuiApp {
    /*
    CDXC:Terminal 2026-09-05 WHY:
    Agent sessions run inside zmx. Viewers pinned by pending input or native
    operations can remain attached while hidden; other viewers release through
    gpui_engine_terminal_viewers, with visible Chat retaining a lightweight claim.
    A parked client used to keep its last
    narrow width and stay the daemon's sizing client, so the agent CLI kept
    rendering narrow and the chat view, which reads the zmx screen, showed
    truncated lines. This pass runs after every engine reconcile (once per
    render through `prepare_focus_bounds_for_render`) and tells each client
    whether its rendered slot is a terminal, chat, or absent:

    - Displayed = the session is the active session of a rendered mount slot
      in the current titlebar mode: an Agents pane slot
      (`rendered_terminal_body_mount_slots`) while Agents mode is active, or a
      project-editor companion slot
      (`current_project_editor_companion_terminal_body_mount_slots`) while
      that editor mode is active. Chat-mode sessions are never displayed: the
      chat CEF body replaces the terminal mount slot. Popped-out sessions are
      `PoppedOutPlaceholder`, never Running, so they own no engine record here.
    - Chat: a rendered chat slot holds a wide-grid claim with `ZMX_CHAT`.
    - Parked: a session outside the rendered slots holds no chat claim.
    - Both non-visible states: a chat client's local emulator is resized to
      `ZMX_RESTING_GRID_COLS` x its current rows, the width its claim widens
      the daemon to; a parked client keeps its grid (CDXC:Zmx 2026-09-25 below).
      Then the matching `ZMX_CHAT` or `ZMX_HIDDEN` is written into the PTY input
      (the same `write_input` path pastes use; the attach client consumes the
      sequence and never forwards it to the shell).
    - Visible: the claim is written from the element's prepaint, after the
      real grid resize, so `ZMX_VISIBLE=<rows>,<cols>` always carries the grid
      the slot actually has. Newly created records (created for rendered
      slots) announce the same way after their first prepaint resize.
    - Tab and split drags keep the last announced state: a drag transiently
      hides surfaces and must not flap the daemon grid.

    Command-pane terminals are untouched: no reader depends on their width.

    `refresh-if-stale` (click, focus change, resize debounce) conforms the
    daemon grid to the size it is handed, so it may only ever carry a grid the
    user is looking at. `agents_gpui_engine_terminal_zmx_grid_is_displayed`
    gates every Agents/companion refresh: a parked record's model is the
    resting width, and on the surfacing frame the focus-change refresh runs at
    render start, before the prepaint that resizes the model back, so it would
    hand the daemon the resting width right after the visible claim and undo
    it. The visible claim itself is the surfacing refresh: the daemon repaints
    every client when that claim changes the grid.
    */
    pub(crate) fn sync_agents_gpui_engine_terminal_zmx_visibility(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        {
            let records = &self.agents_gpui_engine_terminals;
            self.agents_gpui_engine_terminal_zmx_visibility
                .retain(|session_id, announced| {
                    records
                        .get(session_id)
                        .is_some_and(|record| record.view.entity_id() == announced.view_id)
                });
        }
        if self.agents_gpui_engine_terminal_visibility_drag_in_progress() {
            return;
        }
        // CDXC:Zmx 2026-09-19 WHY: every claim makes that session's daemon resize, and a held "next tab" key that passes mounted terminals sent one Visible and one Hidden claim per step. While the selection is still moving nothing is claimed (gx_store/burst.rs): this pass is skipped, and a displayed viewer that has not announced Visible is told to keep its parked grid, because its own prepaint would otherwise resize and claim (terminal_element.rs, `zmx_grid_claim_held`). The pass compares what is displayed with what each viewer last announced, so the repaint at the settle claims the tab the user landed on and releases the tab the hold started from, each exactly once; tabs only passed were never claimed.
        if self.gx_store_selection_is_settling() {
            let displayed = self.displayed_agents_gpui_engine_terminal_sessions();
            for session_id in displayed {
                let announced_visible = self
                    .agents_gpui_engine_terminal_zmx_visibility
                    .get(&session_id)
                    .is_some_and(|announced| {
                        announced.visibility == GpuiEngineTerminalZmxVisibility::Visible
                    });
                if announced_visible || !self.agents_gpui_engine_terminal_is_zmx_client(session_id)
                {
                    continue;
                }
                if let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) {
                    record
                        .view
                        .update(cx, |view, _cx| view.set_zmx_grid_claim_held(true));
                }
            }
            return;
        }

        let displayed = self.displayed_agents_gpui_engine_terminal_sessions();
        self.sync_agents_gpui_engine_terminal_zmx_visibility_for_displayed(&displayed, cx);
    }

    /// CDXC:Terminal 2026-09-06 WHY:
    /// A project switch releases claims before detaching unused viewers or parking direct PTY owners and pinned viewers; incoming terminals must announce their current layout.
    pub(crate) fn park_agents_gpui_engine_terminal_zmx_clients(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.sync_agents_gpui_engine_terminal_zmx_visibility_for_displayed(&HashSet::new(), cx);
        self.agents_gpui_engine_terminal_zmx_visibility.clear();
    }

    fn sync_agents_gpui_engine_terminal_zmx_visibility_for_displayed(
        &mut self,
        displayed: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        let session_ids = self
            .agents_gpui_engine_terminals
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for session_id in session_ids {
            // Only a `zmx attach` client consumes the claim; a plain shell
            // (Debug Action tabs, default-shell startups, PowerShell tabs
            // moved in from the command pane) would receive it as keystrokes.
            if !self.agents_gpui_engine_terminal_is_zmx_client(session_id) {
                self.agents_gpui_engine_terminal_zmx_visibility
                    .remove(&session_id);
                continue;
            }
            let Some(view) = self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone())
            else {
                continue;
            };
            let view_id = view.entity_id();
            let previous = self
                .agents_gpui_engine_terminal_zmx_visibility
                .get(&session_id)
                .filter(|announced| announced.view_id == view_id)
                .map(|announced| announced.visibility);
            if previous.is_none() {
                view.update(cx, |view, _cx| view.enable_zmx_visibility_claims());
            }
            let visibility = if !displayed.contains(&session_id) {
                GpuiEngineTerminalZmxVisibility::Parked
            } else if self.agents_chat_mode_sessions.contains(&session_id) {
                GpuiEngineTerminalZmxVisibility::Chat
            } else {
                GpuiEngineTerminalZmxVisibility::Visible
            };
            let is_displayed = visibility == GpuiEngineTerminalZmxVisibility::Visible;
            view.update(cx, |view, _cx| {
                view.set_displayed(is_displayed);
                // The selection has settled (or never moved): the prepaint may resize and claim.
                view.set_zmx_grid_claim_held(false);
            });
            if is_displayed {
                let (cols, rows) = view.read(cx).model().size();
                if previous != Some(GpuiEngineTerminalZmxVisibility::Visible) {
                    view.update(cx, |view, _cx| view.request_zmx_visible_announce());
                }
                self.agents_gpui_engine_terminal_zmx_visibility.insert(
                    session_id,
                    GpuiEngineTerminalAnnouncedVisibility {
                        view_id,
                        visibility: GpuiEngineTerminalZmxVisibility::Visible,
                        rows,
                        cols,
                    },
                );
            } else if previous != Some(visibility) {
                // CDXC:Zmx 2026-09-25 WHY:
                // A parked viewer keeps its grid instead of reflowing to the resting width (supersedes parking both non-visible states at 200 columns). With no visible or chat claim the daemon retains the grid the viewer last claimed (loop.zig electLeader), so the mirror at that grid takes the streamed output exactly and a redisplay at the same size needs no reflow, no claim round trip and no reflow hold. The 200-column park re-wrapped that output against a grid the daemon was not using, and every switch back resized, reflowed and held.
                // If another client changes the daemon grid meanwhile, the redisplay's claim changes it back, and the daemon repaints every client from its own screen.
                // Chat keeps the resting width, because the chat claim itself widens the daemon to at least 200 columns.
                let (current_cols, rows) = view.read(cx).model().size();
                let cols = if visibility == GpuiEngineTerminalZmxVisibility::Chat {
                    ZMX_RESTING_GRID_COLS
                } else {
                    current_cols
                };
                view.update(cx, |view, cx| {
                    view.resize_grid(cols, rows, cx);
                    let sequence = if visibility == GpuiEngineTerminalZmxVisibility::Chat {
                        zmx_client_chat_sequence(rows, cols)
                    } else {
                        zmx_client_hidden_sequence(rows, cols)
                    };
                    let _ = view.model().write_input(sequence.as_bytes());
                });
                self.agents_gpui_engine_terminal_zmx_visibility.insert(
                    session_id,
                    GpuiEngineTerminalAnnouncedVisibility {
                        view_id,
                        visibility,
                        rows,
                        cols,
                    },
                );
            }
        }
    }

    /// Sessions whose engine terminal is rendered as a terminal body in the
    /// current titlebar mode, including slots rendering chat instead.
    fn displayed_agents_gpui_engine_terminal_sessions(&self) -> HashSet<TerminalSessionId> {
        let mut displayed = HashSet::new();
        if self.agents_workspace_visible() {
            displayed.extend(
                self.agents_workspace
                    .rendered_terminal_body_mount_slots()
                    .into_iter()
                    .map(|slot_id| slot_id.session_id),
            );
        }
        displayed
    }

    /// Whether the engine terminal behind this session runs a `zmx attach`
    /// client: a local daemon session carries its zmx name on the workspace
    /// session, a remote one is tracked by the SSH attach map (its attach
    /// command runs `zmx attach` on the remote machine).
    pub(crate) fn agents_gpui_engine_terminal_is_zmx_client(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        self.agents_workspace
            .session(session_id)
            .is_some_and(|session| session.zmx_session_name.is_some())
            || self
                .remote_attach_sessions
                .iter()
                .any(|(key, remote_session_id)| {
                    *remote_session_id == session_id
                        && self.agents_workspace_project_id.as_deref()
                            == Some(
                                gpui_remote_scoped_project_id(
                                    &key.remote_machine_id,
                                    &key.project_id,
                                )
                                .as_str(),
                            )
                })
    }

    /// Whether a `refresh-if-stale` may carry this engine terminal's grid:
    /// false while the client is announced chat or parked (its model rests at
    /// `ZMX_RESTING_GRID_COLS`) or while a visible announce is still pending
    /// (the next prepaint re-establishes and claims the real grid). Records
    /// outside the visibility protocol (non-zmx shells, command pane) are
    /// never gated here.
    pub(crate) fn agents_gpui_engine_terminal_zmx_grid_is_displayed(
        &self,
        session_id: TerminalSessionId,
        cx: &gpui::Context<Self>,
    ) -> bool {
        let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) else {
            return true;
        };
        if self
            .agents_gpui_engine_terminal_zmx_visibility
            .get(&session_id)
            .is_some_and(|announced| {
                announced.visibility != GpuiEngineTerminalZmxVisibility::Visible
            })
        {
            return false;
        }
        !record.view.read(cx).zmx_visible_announce_pending()
    }

    fn agents_gpui_engine_terminal_visibility_drag_in_progress(&self) -> bool {
        self.workspace_tab_drag_active
            || self.command_tab_drag_active
            || self.workspace_split_drag.is_some()
            || self.workarea_split_drag.is_some()
    }
}
