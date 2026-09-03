// C1 wave-3 re-cluster: the Agents terminal body presentation/click-action state and mount-candidate selection, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:Terminal 2026-06-22-20:14:
Phase 2 libghostty parity records the pane id and selected session id for each rendered Agents terminal body before any real surface exists. Keep the boundary explicit so sleeping, mounting, failed-startup, restored/unmounted, popped-out, missing-session, inactive running, and non-focused running tabs stay classified instead of receiving fake or fallback surfaces.

CDXC:Terminal 2026-06-22-20:14:
The future libghostty mount slot is the normal WorkspaceLeaf body child below the Agents tab bar. Keep that body as a non-overlapping layout sibling with no hidden hit regions, transparent overlays, root hit-test routing, or synthetic coordinate routing.

CDXC:Terminal 2026-06-22-22:45:
All rendered visible Agents leaves whose selected session is Running are real terminal mount slots. Focus mode limits the rendered leaf set naturally, inactive tabs stay hidden, and sleeping/restored/mounting/failed-startup/popped-out/missing selections keep placeholder bodies without fake surfaces or extra hit regions.

CDXC:Terminal 2026-06-23-05:03:
Command-pane terminal mount slots are runtime-only group/session body identities, separate from Agents pane/session ids and shell persistence. Only expanded visible active command bodies may record bounds or mount surfaces; inactive command tabs, collapsed panes, missing sessions, titles, status labels, project paths, commands, env, input, and terminal content must not become launch payload or durable state.

CDXC:Terminal 2026-06-23-08:32:
Running Agents terminal mouse input is accepted only through the current body mount slot and the recorded body rectangle. Missing or stale bounds produce a no-op so focus and placeholder activation semantics stay intact without adding overlays, hidden hit regions, root/window hit-test routing, synthetic coordinate routing, logs, persistence, or raw input storage.

CDXC:Terminal 2026-06-23-09:32:
Running Agents body scroll input uses the same current-slot and recorded-body-boundary gate as mouse movement and buttons. Forwarding updates Ghostty's body-relative pointer position first, then sends the wheel delta only to the exact current surface without overlays, hidden hit regions, root/window routing, coordinate rerouting, logging, persistence, or placeholder changes.

CDXC:Terminal 2026-06-23-09:41:
Mounted command-pane terminal bodies mirror Running Agents mouse and scroll forwarding through the normal body element only. Current command mount slots use recorded body bounds, body-relative pointer coordinates, mapped mouse modifier bits, exact Ghostty surface identity, and runtime-only state while preserving command-pane focus/drop ownership without overlays, hidden hit regions, input routing, logging, persistence, or raw input storage.

CDXC:Terminal 2026-06-23-12:43:
Ghostty owns terminal selection state; GPUI must not store selection text, raw drag coordinates, or per-terminal drag state. Mounted Agents and command terminal body selection is represented by the normal body-scoped Ghostty event stream: button press, body-relative pointer moves including press-held moves delivered by the body, and button release. Outside-body release stays capture-gated only, with no transparent overlay, hidden hit region, broad hit-test routing, root/window pre-dispatch routing, global capture, or synthetic coordinate routing.

CDXC:Terminal 2026-06-23-09:51:
Mounted Agents and command terminal pressure input uses the same current-slot, recorded-body-bounds, exact-surface, and macOS gates as pointer and scroll forwarding. Forward body-relative pointer position with mapped modifiers first, then pass the GPUI pressure value and mapped pressure stage to Ghostty without clamping, fallback behavior, logging, persistence, coordinate routing, or placeholder changes.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsTerminalBodyPresentation {
    MountSlot,
    RunningPlaceholder,
    LifecyclePlaceholder,
    MissingSessionPlaceholder,
    EmptyWorkspacePlaceholder,
}

impl AgentsTerminalBodyPresentation {
    pub(crate) fn element_slug(
        self,
        presentation_state: Option<TerminalSessionPresentationState>,
    ) -> &'static str {
        match self {
            Self::MountSlot => "libghostty-mount-slot",
            Self::RunningPlaceholder => "running-black-placeholder",
            Self::LifecyclePlaceholder => presentation_state
                .map(TerminalSessionPresentationState::element_slug)
                .unwrap_or("lifecycle-placeholder"),
            Self::MissingSessionPlaceholder => "missing-session-placeholder",
            Self::EmptyWorkspacePlaceholder => "empty-workspace-placeholder",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsTerminalBodyClickAction {
    FocusRunningMountSlot(AgentsTerminalBodyMountSlotId),
    ActivatePlaceholder {
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    },
    None,
}

pub(crate) fn agents_terminal_body_click_action(
    mount_candidate: AgentsTerminalBodyMountCandidate,
    fallback_session_id: TerminalSessionId,
) -> AgentsTerminalBodyClickAction {
    /*
    CDXC:FocusRouting 2026-06-22-23:11:
    Clicking an eligible running Agents terminal body must focus the shell pane and then the real Ghostty/AppKit host surface for that mounted slot. Non-running placeholders keep the existing activation path so sleeping/restored/popped-out/failed-startup tabs remain explicit wake/materialize/reattach/retry actions and Mounting/missing states do not fabricate a terminal.
    */
    if mount_candidate.active_session_id.is_none() {
        AgentsTerminalBodyClickAction::None
    } else if let Some(slot_id) = mount_candidate.mount_slot_id() {
        AgentsTerminalBodyClickAction::FocusRunningMountSlot(slot_id)
    } else {
        AgentsTerminalBodyClickAction::ActivatePlaceholder {
            pane_id: mount_candidate.pane_id,
            session_id: fallback_session_id,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalBodyMountCandidate {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) active_session_id: Option<TerminalSessionId>,
    pub(crate) presentation: AgentsTerminalBodyPresentation,
}

impl AgentsTerminalBodyMountCandidate {
    pub(crate) fn eligible_for_terminal_surface(self) -> bool {
        matches!(self.presentation, AgentsTerminalBodyPresentation::MountSlot)
    }

    pub(crate) fn mount_slot_id(self) -> Option<AgentsTerminalBodyMountSlotId> {
        if !self.eligible_for_terminal_surface() {
            return None;
        }

        Some(AgentsTerminalBodyMountSlotId {
            pane_id: self.pane_id,
            session_id: self.active_session_id?,
        })
    }

    pub(crate) fn renders_placeholder_child(self) -> bool {
        matches!(
            self.presentation,
            AgentsTerminalBodyPresentation::LifecyclePlaceholder
                | AgentsTerminalBodyPresentation::MissingSessionPlaceholder
        )
    }
}

pub(crate) fn selected_agents_terminal_body_mount_candidate(
    leaf: &WorkspaceLeaf,
    terminal_sessions: &[TerminalSession],
    rendered_leaf_order: &[WorkspacePaneId],
) -> AgentsTerminalBodyMountCandidate {
    let active_session_id = leaf.tab_group.active_session_id();
    let active_session = active_session_id.and_then(|session_id| {
        terminal_sessions
            .iter()
            .find(|session| session.id == session_id)
    });
    let presentation_state = active_session.map(|session| session.presentation_state);
    let pane_is_rendered = rendered_leaf_order.contains(&leaf.pane_id);
    let presentation = match presentation_state {
        Some(TerminalSessionPresentationState::Running) if pane_is_rendered => {
            AgentsTerminalBodyPresentation::MountSlot
        }
        Some(TerminalSessionPresentationState::Running) => {
            AgentsTerminalBodyPresentation::RunningPlaceholder
        }
        Some(_) => AgentsTerminalBodyPresentation::LifecyclePlaceholder,
        None if active_session_id.is_none() => {
            AgentsTerminalBodyPresentation::EmptyWorkspacePlaceholder
        }
        None => AgentsTerminalBodyPresentation::MissingSessionPlaceholder,
    };

    AgentsTerminalBodyMountCandidate {
        pane_id: leaf.pane_id,
        active_session_id,
        presentation,
    }
}
