//! Opening and editing the side panel: New ticket, Edit ticket and Board columns.

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, InputState, TextareaState};

use super::beads::show_issue;
use super::model::{estimate_to_tshirt, priority_select_value};
use super::state::{KanbanColumnsForm, KanbanFormMode, KanbanPanel, KanbanTicketForm};
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    fn native_kanban_input(
        &mut self,
        placeholder: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<InputState>, Subscription) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder.to_string())
                .default_value(value.to_string())
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| match event {
                InputEvent::Focus => this.native_kanban_input_focused(),
                InputEvent::Change => this.native_kanban_notify(cx),
                InputEvent::PressEnter { .. } => {
                    this.native_kanban_input_enter(input, window, cx);
                }
                InputEvent::Blur => {}
            },
        );
        (input, subscription)
    }

    /// A text area of the side panel; unlike a single-line field, its Enter is a newline.
    fn native_kanban_text_area(
        &mut self,
        placeholder: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextareaState>, Subscription) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder.to_string())
                .default_value(value.to_string())
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                InputEvent::Focus => this.native_kanban_input_focused(),
                InputEvent::Change => this.native_kanban_notify(cx),
                InputEvent::PressEnter { .. } | InputEvent::Blur => {}
            },
        );
        (input, subscription)
    }

    fn native_kanban_input_enter(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.native_kanban.panel.as_ref() {
            Some(KanbanPanel::Ticket(form)) if form.label_input == *input => {
                self.native_kanban_add_form_label(window, cx);
            }
            Some(KanbanPanel::Columns(form)) if form.name == *input => {
                self.native_kanban_add_column(window, cx);
            }
            _ => {}
        }
    }

    fn native_kanban_ticket_form(
        &mut self,
        mode: KanbanFormMode,
        status: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KanbanTicketForm {
        let ticket = match &mode {
            KanbanFormMode::Edit { ticket_id } => self.native_kanban.ticket(ticket_id).cloned(),
            KanbanFormMode::New => None,
        };
        let issue = ticket.as_ref().map(|ticket| &ticket.issue);
        let title_value = issue.map(|issue| issue.title.clone()).unwrap_or_default();
        let description_value = issue
            .map(|issue| issue.description.clone())
            .unwrap_or_default();
        let title_placeholder = if mode == KanbanFormMode::New {
            "Auto-generated from prompt when left empty"
        } else {
            "Title"
        };
        let (title, title_subscription) =
            self.native_kanban_input(title_placeholder, &title_value, window, cx);
        let (description, description_subscription) = self.native_kanban_text_area(
            "Write the full prompt for this ticket.",
            &description_value,
            window,
            cx,
        );
        let (comment, comment_subscription) =
            self.native_kanban_text_area("Add a note for the team.", "", window, cx);
        let (label_input, label_subscription) =
            self.native_kanban_input("Add a label and press Enter", "", window, cx);
        KanbanTicketForm {
            priority: priority_select_value(issue.and_then(|issue| issue.priority)).to_string(),
            tshirt: estimate_to_tshirt(issue.and_then(|issue| issue.estimate)),
            labels: issue.map(|issue| issue.labels.clone()).unwrap_or_default(),
            status: ticket
                .as_ref()
                .map(|ticket| ticket.board_status.clone())
                .unwrap_or(status),
            mode,
            ticket,
            title,
            description,
            comment,
            label_input,
            _subscriptions: vec![
                title_subscription,
                description_subscription,
                comment_subscription,
                label_subscription,
            ],
        }
    }

    /// `openNewTicket`, from `+ Ticket` or a lane's `+` (which creates in that lane).
    pub(crate) fn native_kanban_open_new_ticket(
        &mut self,
        status: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form =
            self.native_kanban_ticket_form(KanbanFormMode::New, status.to_string(), window, cx);
        let description = form.description.clone();
        self.native_kanban.panel = Some(KanbanPanel::Ticket(Box::new(form)));
        description.update(cx, |input, cx| input.focus(window, cx));
        self.native_kanban_notify(cx);
    }

    /// `openTicket`: the panel opens on what the board already has, then takes the comments and
    /// details from bd's `show`.
    pub(crate) fn native_kanban_open_ticket(
        &mut self,
        ticket_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_kanban.ticket(ticket_id).is_none() {
            return;
        }
        let mode = KanbanFormMode::Edit {
            ticket_id: ticket_id.to_string(),
        };
        let form = self.native_kanban_ticket_form(mode.clone(), String::new(), window, cx);
        self.native_kanban.panel = Some(KanbanPanel::Ticket(Box::new(form)));
        let issue_id = ticket_id.to_string();
        self.native_kanban_spawn(
            move |context| show_issue(context, &issue_id),
            move |this, result, cx| {
                let Ok(Some(shown)) = result else {
                    return;
                };
                let Some(KanbanPanel::Ticket(form)) = this.native_kanban.panel.as_mut() else {
                    return;
                };
                if form.mode != mode {
                    return;
                }
                if let Some(ticket) = form.ticket.as_mut() {
                    ticket.issue.comments = shown.comments;
                    ticket.issue.comment_count = shown.comment_count.or(ticket.issue.comment_count);
                    if ticket.issue.assignee.is_none() {
                        ticket.issue.assignee = shown.assignee;
                    }
                    if ticket.issue.created_by.is_none() {
                        ticket.issue.created_by = shown.created_by;
                    }
                }
                this.native_kanban_notify(cx);
            },
            cx,
        );
        self.native_kanban_notify(cx);
    }

    pub(crate) fn native_kanban_open_columns(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (name, subscription) = self.native_kanban_input("New column name", "", window, cx);
        self.native_kanban.panel = Some(KanbanPanel::Columns(KanbanColumnsForm {
            name,
            busy: false,
            error: None,
            _subscription: subscription,
        }));
        self.native_kanban_notify(cx);
    }

    pub(crate) fn native_kanban_close_panel(&mut self, cx: &mut Context<Self>) {
        self.native_kanban.panel = None;
        self.native_kanban_notify(cx);
    }

    pub(crate) fn native_kanban_form_mut(&mut self) -> Option<&mut KanbanTicketForm> {
        match self.native_kanban.panel.as_mut() {
            Some(KanbanPanel::Ticket(form)) => Some(form),
            _ => None,
        }
    }

    /// Adds the label field's text (comma separated) to the ticket's labels.
    pub(crate) fn native_kanban_add_form_label(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.native_kanban_form_mut() else {
            return;
        };
        let input = form.label_input.clone();
        let text = input.read(cx).value().to_string();
        for label in text
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
        {
            if !form.labels.iter().any(|existing| existing == label) {
                form.labels.push(label.to_string());
            }
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.native_kanban_notify(cx);
    }

    pub(crate) fn native_kanban_toggle_form_label(&mut self, label: &str, cx: &mut Context<Self>) {
        let Some(form) = self.native_kanban_form_mut() else {
            return;
        };
        if let Some(index) = form.labels.iter().position(|existing| existing == label) {
            form.labels.remove(index);
        } else {
            form.labels.push(label.to_string());
        }
        self.native_kanban_notify(cx);
    }

    /// Board columns: add the typed name when bd would accept it.
    pub(crate) fn native_kanban_add_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(KanbanPanel::Columns(form)) = self.native_kanban.panel.as_mut() else {
            return;
        };
        if form.busy {
            return;
        }
        let name_input = form.name.clone();
        let name = name_input.read(cx).value().trim().to_string();
        let config = self.native_kanban.column_config.clone();
        if let Some(error) = super::model::board_column_name_error(&name, &config) {
            form.error = Some(error);
            self.native_kanban_notify(cx);
            return;
        }
        name_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.native_kanban_write_column_config(super::model::add_board_column(&config, &name), cx);
    }
}
