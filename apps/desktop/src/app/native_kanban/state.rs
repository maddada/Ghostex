//! The native Kanban's state, kept on the app like the native sidebar's so the board can call the
//! app's own Beads bridge, conversation routing and toasts directly.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Bounds, Entity, FocusHandle, Pixels, Subscription, Task};
use gpui_component::input::{InputState, TextareaState};

use super::filters::{KanbanCardView, KanbanViewPreferences};
use super::model::{BeadsIssue, BoardColumn, BoardTicket, KanbanConversationState};

/// The project a board belongs to. A change of any part reloads the board from scratch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KanbanProjectKey {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) remote_machine_id: Option<String>,
}

/// What the render resolves from the live sidebar project snapshot each frame.
#[derive(Clone, Debug)]
pub(crate) struct KanbanProject {
    pub(crate) key: KanbanProjectKey,
    pub(crate) display_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KanbanLoadState {
    Idle,
    Loading,
    Ready,
    Error,
}

/// `BoardRefreshMode`: initial and manual loads also reconcile the issue prefix and the required
/// statuses; background and post-mutation loads only read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KanbanRefreshMode {
    Initial,
    Manual,
    Mutation,
    Background,
}

impl KanbanRefreshMode {
    pub(crate) fn reconciles(self) -> bool {
        matches!(self, Self::Initial | Self::Manual)
    }

    /// Which of two queued refreshes to keep: anything beats a background poll.
    pub(crate) fn stronger(self, other: Self) -> Self {
        if self == Self::Background {
            other
        } else {
            self
        }
    }
}

/// A lane move shown before bd confirmed it, reapplied over refreshes until it lands.
#[derive(Clone, Debug)]
pub(crate) struct KanbanPendingMove {
    pub(crate) beads_status: String,
    pub(crate) token: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KanbanFormMode {
    New,
    Edit { ticket_id: String },
}

/// The ticket panel's editable draft: New ticket or Edit ticket.
pub(crate) struct KanbanTicketForm {
    pub(crate) mode: KanbanFormMode,
    /// The ticket as last loaded, for the read-only parts (ids, comments, assignee).
    pub(crate) ticket: Option<BoardTicket>,
    pub(crate) title: Entity<InputState>,
    pub(crate) description: Entity<TextareaState>,
    pub(crate) comment: Entity<TextareaState>,
    pub(crate) label_input: Entity<InputState>,
    pub(crate) status: String,
    pub(crate) priority: String,
    pub(crate) tshirt: Option<&'static str>,
    pub(crate) labels: Vec<String>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

pub(crate) struct KanbanColumnsForm {
    pub(crate) name: Entity<InputState>,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
    pub(crate) _subscription: Subscription,
}

pub(crate) enum KanbanPanel {
    Ticket(Box<KanbanTicketForm>),
    Columns(KanbanColumnsForm),
}

/// Which board request a pending sidebar-runtime conversation answer belongs to.
#[derive(Clone, Debug)]
pub(crate) enum KanbanConversationRequest {
    State,
    StartWork { ticket_id: String },
    Jump,
}

/// Trigger bounds for the toolbar menus, so a menu opens below its button.
#[derive(Clone, Default)]
pub(crate) struct KanbanMenuAnchors {
    pub(crate) filters: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) card_view: Rc<Cell<Bounds<Pixels>>>,
}

#[derive(Default)]
pub(crate) struct NativeKanbanState {
    pub(crate) project: Option<KanbanProjectKey>,
    /// The project name the board's title shows.
    pub(crate) display_name: String,
    pub(crate) display_key: String,
    pub(crate) issue_prefix: String,
    /// Bumped on every project change; answers from an older board are dropped.
    pub(crate) generation: u64,
    pub(crate) load_state: Option<KanbanLoadState>,
    pub(crate) initial_load_done: bool,
    pub(crate) error: Option<String>,
    pub(crate) refreshing: bool,
    pub(crate) queued_refresh: Option<KanbanRefreshMode>,
    /// The raw `status.custom` value, kept so column edits preserve bd's category suffixes.
    pub(crate) column_config: String,
    pub(crate) columns: Vec<BoardColumn>,
    pub(crate) issues: Vec<BeadsIssue>,
    pub(crate) tickets: Vec<BoardTicket>,
    pub(crate) issues_signature: String,
    pub(crate) pending_moves: HashMap<String, KanbanPendingMove>,
    pub(crate) move_serial: u64,
    pub(crate) conversation: KanbanConversationState,
    pub(crate) conversation_requests: HashMap<String, KanbanConversationRequest>,
    /// The ticket whose Start work or session jump is in flight.
    pub(crate) busy_ticket: Option<String>,
    pub(crate) create_in_flight: bool,
    pub(crate) search: Option<Entity<InputState>>,
    pub(crate) search_query: String,
    pub(crate) view: KanbanViewPreferences,
    pub(crate) card_view: KanbanCardView,
    pub(crate) panel: Option<KanbanPanel>,
    pub(crate) focus: Option<FocusHandle>,
    pub(crate) anchors: KanbanMenuAnchors,
    pub(crate) refresh_timer: Option<Task<()>>,
    pub(crate) search_subscription: Option<Subscription>,
    /// The cached view the board draws in (`view.rs`).
    pub(crate) board_view: Option<Entity<super::view::NativeKanbanView>>,
    /// Bumped whenever tickets, columns, search or filters change; keys `derived`.
    pub(crate) revision: u64,
    pub(crate) derived: Option<super::derived::KanbanDerived>,
    /// Built from settings once per appearance change rather than every frame.
    pub(crate) palette: Option<super::palette::KanbanPalette>,
    /// Window glass and light chrome as last drawn; a change redraws the board.
    pub(crate) appearance_signature: Option<(bool, bool)>,
}

impl NativeKanbanState {
    /// Drops everything that belongs to the previous project; the toolbar's view choices, the
    /// card-detail toggles and the search box carry over like the React board's app-wide ones.
    pub(crate) fn reset_for_project(&mut self, project: KanbanProjectKey) {
        let generation = self.generation.wrapping_add(1);
        let view = std::mem::take(&mut self.view);
        let card_view = self.card_view;
        let search = self.search.take();
        let search_query = std::mem::take(&mut self.search_query);
        let search_subscription = self.search_subscription.take();
        let focus = self.focus.take();
        let anchors = std::mem::take(&mut self.anchors);
        let refresh_timer = self.refresh_timer.take();
        let board_view = self.board_view.take();
        let palette = self.palette.take();
        let appearance_signature = self.appearance_signature;
        *self = Self {
            project: Some(project),
            generation,
            columns: super::model::build_board_columns(""),
            view,
            card_view,
            search,
            search_query,
            search_subscription,
            focus,
            anchors,
            refresh_timer,
            board_view,
            palette,
            appearance_signature,
            ..Self::default()
        };
    }

    /// The board's first load has not answered yet, so its lanes show skeleton cards.
    pub(crate) fn loading_first(&self) -> bool {
        !self.initial_load_done
            && matches!(
                self.load_state,
                Some(KanbanLoadState::Loading | KanbanLoadState::Idle)
            )
    }

    pub(crate) fn ticket(&self, ticket_id: &str) -> Option<&BoardTicket> {
        self.tickets
            .iter()
            .find(|ticket| ticket.issue.id == ticket_id)
    }
}
