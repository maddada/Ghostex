//! Native GPUI Create ticket dialog: a work-mode project's "…" menu → Create Linear Ticket… (or
//! Create GitHub Issue…), or the Work page's New ticket button (with a Project picker when the page
//! shows several work-mode projects). It creates the ticket in the workspace's primary tracker (a
//! Linear ticket, or a GitHub issue in the project's repo) and, when Start work now is on, starts an
//! agent on the ticket's branch in a new worktree, linked to the ticket.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: the workspace's primary tracker is "Linear Tickets & Projects or Github Issues & Projects -
//! Need to pick just 1". This one dialog follows it: for GitHub it has no Team or Linear project
//! fields and makes a GitHub issue assigned to `@me` (server/src/work_mode/github_tickets.rs).
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! The dialog stays open while Linear and gxserver work, so a refused key, a missing team or a
//! failed worktree shows here next to the fields instead of as a toast after the dialog is gone. A
//! ticket that was created but whose work did not start is not created twice: the button turns
//! into Start Work for that ticket.
//! SEE-ALSO: apps/desktop/src/app/create_linear_ticket_modal_lifecycle.rs (open, the new session's
//! focus), server/src/server/route_http/work_tickets.rs (`/api/listLinearTeams`,
//! `/api/listLinearProjects`, `/api/createLinearIssue`, `/api/startWorkOnTicket`),
//! packages/gx-core/src/sidebar_actions/open.rs (the `createLinearTicket` action that opens it).
use super::native_modal_kit::*;
use crate::app::gx_store::{gx_rpc, gx_rpc_with_timeout};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, Focusable as _,
    FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::rc::Rc;
use std::time::Duration;

pub(crate) const CREATE_LINEAR_TICKET_MODAL_WIDTH: f32 = 540.0;
pub(crate) const CREATE_LINEAR_TICKET_MODAL_INITIAL_HEIGHT: f32 = 660.0;

/// Starting work fetches the ticket's branch from origin (up to two minutes), cuts the worktree,
/// runs the project's worktree setup command and starts the agent.
const START_WORK_TIMEOUT: Duration = Duration::from_secs(180);

const TITLE: &str = "Create Linear Ticket";
const GITHUB_TITLE: &str = "Create GitHub Issue";
const TITLE_PLACEHOLDER: &str = "e.g. Add Copy link to the share menu";
const DESCRIPTION_PLACEHOLDER: &str = "What needs doing (Markdown)";
const NO_LINEAR_PROJECT: &str = "No Linear project";

/// What the dialog asks its host to do. The dialog removes its own window before sending it.
pub(crate) enum CreateLinearTicketModalCommand {
    /// The ticket exists and an agent started on it: open that session.
    Started {
        project_id: String,
        session_id: String,
    },
    /// The ticket exists and nothing was started.
    Created {
        identifier: String,
    },
    Cancel,
}

pub(crate) type CreateLinearTicketModalHost = Rc<dyn Fn(CreateLinearTicketModalCommand, &mut App)>;

pub(crate) struct CreateLinearTicketModalConfig {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    /// `(projectId, name)` of the work-mode projects to pick from, when it was opened from the Work
    /// page showing several; empty for a project's own "…" menu, which names its project.
    pub(crate) projects: Vec<(String, String)>,
    /// `(agentId, name)`, the New session launcher's agents.
    pub(crate) agents: Vec<(String, String)>,
    pub(crate) selected_agent: usize,
    pub(crate) palette: ModalPalette,
}

/// `(id, label)` of a team or a Linear project.
type Choice = (String, String);

/// Where the ticket goes: the project workspace's primary tracker (`/api/readWorkTracker`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tracker {
    Loading,
    Linear,
    Github,
}

pub(crate) struct GpuiCreateLinearTicketModalWindow {
    host: CreateLinearTicketModalHost,
    palette: ModalPalette,
    project_id: String,
    project_name: String,
    projects: Vec<Choice>,
    project_select: ModalSelect,
    tracker: Tracker,
    /// `owner/repo` the GitHub issue goes to, when known.
    repo: Option<String>,
    title_input: Entity<InputState>,
    description_input: Entity<TextareaState>,
    title: String,
    description: String,
    teams: Vec<Choice>,
    team_index: Option<usize>,
    team_select: ModalSelect,
    teams_loading: bool,
    /// The selected team's open Linear projects; the menu's first row is "No Linear project".
    linear_projects: Vec<Choice>,
    linear_project_index: Option<usize>,
    linear_project_select: ModalSelect,
    assign_to_me: bool,
    start_work: bool,
    agents: Vec<(String, String)>,
    agent_index: usize,
    agent_select: ModalSelect,
    /// A ticket this dialog already created, whose work has not started yet.
    created: Option<String>,
    busy: Option<&'static str>,
    error: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiCreateLinearTicketModalWindow {
    pub(crate) fn new(
        config: CreateLinearTicketModalConfig,
        host: CreateLinearTicketModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder(TITLE_PLACEHOLDER));
        let description_input =
            cx.new(|cx| TextareaState::new(window, cx).placeholder(DESCRIPTION_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe_in(
                &title_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.title = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &description_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.description = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
        ];
        title_input.update(cx, |input, cx| input.focus(window, cx));
        let agent_index = config
            .selected_agent
            .min(config.agents.len().saturating_sub(1));
        let this = Self {
            host,
            palette: config.palette,
            project_id: config.project_id,
            project_name: config.project_name,
            projects: config.projects,
            project_select: ModalSelect::new(),
            tracker: Tracker::Loading,
            repo: None,
            title_input,
            description_input,
            title: String::new(),
            description: String::new(),
            teams: Vec::new(),
            team_index: None,
            team_select: ModalSelect::new(),
            teams_loading: true,
            linear_projects: Vec::new(),
            linear_project_index: None,
            linear_project_select: ModalSelect::new(),
            assign_to_me: true,
            start_work: true,
            agents: config.agents,
            agent_index,
            agent_select: ModalSelect::new(),
            created: None,
            busy: None,
            error: None,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.load_tracker(window, cx);
        this
    }

    /// Asks which tracker the project's workspace uses, then loads Linear's teams when it is Linear.
    fn load_tracker(&self, window: &mut Window, cx: &mut Context<Self>) {
        let project_id = self.project_id.clone();
        let params = json!({ "projectId": project_id });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/readWorkTracker", params).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.project_id != project_id {
                    return;
                }
                match result {
                    Ok(answer) if answer["tracker"].as_str() == Some("github") => {
                        // No Team / Linear project row: the window shrinks to the fields left.
                        this.fit.refit();
                        this.tracker = Tracker::Github;
                        this.repo = answer["repo"].as_str().map(str::to_string);
                        this.teams_loading = false;
                    }
                    Ok(_) => {
                        this.fit.refit();
                        this.tracker = Tracker::Linear;
                        this.load_teams(window, cx);
                    }
                    Err(error) => {
                        this.teams_loading = false;
                        this.error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_teams(&self, window: &mut Window, cx: &mut Context<Self>) {
        let project_id = self.project_id.clone();
        let params = json!({ "projectId": project_id });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/listLinearTeams", params).await;
            let _ = this.update_in(cx, |this, window, cx| {
                // A project picked again while this was loading has its own load.
                if this.project_id != project_id {
                    return;
                }
                this.teams_loading = false;
                match result {
                    Ok(answer) => {
                        this.teams = choices(&answer["teams"], |team| {
                            let name = team["name"].as_str()?;
                            Some(match team["key"].as_str() {
                                Some(key) => format!("{name} ({key})"),
                                None => name.to_string(),
                            })
                        });
                        let default = answer["defaultTeamId"].as_str();
                        this.team_index = this
                            .teams
                            .iter()
                            .position(|(id, _)| Some(id.as_str()) == default)
                            .or((!this.teams.is_empty()).then_some(0));
                        this.load_linear_projects(window, cx);
                    }
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_linear_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.linear_projects.clear();
        self.linear_project_index = None;
        let Some(team_id) = self.team_id() else {
            return;
        };
        let params = json!({ "projectId": self.project_id, "teamId": team_id });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/listLinearProjects", params).await;
            let _ = this.update(cx, |this, cx| {
                // A team picked again while this was loading has its own load.
                if this.team_id().as_deref() != Some(team_id.as_str()) {
                    return;
                }
                if let Ok(answer) = result {
                    this.linear_projects = choices(&answer["projects"], |project| {
                        project["name"].as_str().map(str::to_string)
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn team_id(&self) -> Option<String> {
        self.team_index
            .and_then(|index| self.teams.get(index))
            .map(|(id, _)| id.clone())
    }

    fn can_create(&self) -> bool {
        self.busy.is_none()
            && (self.created.is_some()
                || (!self.title.trim().is_empty()
                    && match self.tracker {
                        Tracker::Loading => false,
                        Tracker::Github => true,
                        Tracker::Linear => !self.teams_loading && self.team_id().is_some(),
                    }))
    }

    fn close_selects(&mut self) {
        self.project_select.close();
        self.team_select.close();
        self.linear_project_select.close();
        self.agent_select.close();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        let command = match self.created.take() {
            Some(identifier) => CreateLinearTicketModalCommand::Created { identifier },
            None => CreateLinearTicketModalCommand::Cancel,
        };
        (self.host)(command, cx);
    }

    /// Creates the ticket (unless this dialog already did), then starts work on it.
    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create() {
            return;
        }
        self.close_selects();
        self.error = None;
        let project_id = self.project_id.clone();
        let created = self.created.clone();
        let create_params = json!({
            "projectId": project_id,
            "title": self.title.trim(),
            "description": self.description.trim(),
            "teamId": self.team_id(),
            "linearProjectId": self
                .linear_project_index
                .and_then(|index| self.linear_projects.get(index))
                .map(|(id, _)| id.clone()),
            "assignToMe": self.assign_to_me,
        });
        let start_work = self.start_work;
        let agent_id = self.agents.get(self.agent_index).map(|(id, _)| id.clone());
        let github = self.tracker == Tracker::Github;
        let (create_path, create_params) = if github {
            (
                "/api/createGithubIssue",
                json!({
                    "projectId": project_id,
                    "title": self.title.trim(),
                    "description": self.description.trim(),
                    "assignToMe": self.assign_to_me,
                }),
            )
        } else {
            ("/api/createLinearIssue", create_params)
        };
        self.busy = Some(if created.is_some() {
            "Starting work…"
        } else {
            "Creating the ticket…"
        });
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let identifier = match created {
                Some(identifier) => identifier,
                None => match gx_rpc(None, create_path, create_params).await {
                    // A GitHub issue is named `#218` from here on, a Linear ticket by its ID.
                    Ok(ticket) => match ticket["identifier"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| ticket["number"].as_u64().map(|number| format!("#{number}")))
                    {
                        Some(identifier) => identifier,
                        None => {
                            let _ = this.update(cx, |this, cx| {
                                this.busy = None;
                                this.error =
                                    Some("Ghostex was not told which ticket was created.".to_string());
                                cx.notify();
                            });
                            return;
                        }
                    },
                    Err(error) => {
                        let _ = this.update(cx, |this, cx| {
                            this.busy = None;
                            this.error = Some(error.message);
                            cx.notify();
                        });
                        return;
                    }
                },
            };
            if !start_work {
                let _ = this.update_in(cx, |this, window, cx| {
                    window.remove_window();
                    (this.host)(CreateLinearTicketModalCommand::Created { identifier }, cx);
                });
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.created = Some(identifier.clone());
                this.busy = Some("Starting work…");
                cx.notify();
            });
            let mut start_params = match identifier.strip_prefix('#') {
                Some(number) => json!({ "projectId": project_id, "githubIssue": number }),
                None => json!({ "projectId": project_id, "linearIssue": identifier }),
            };
            if let Some(agent_id) = agent_id {
                start_params["agentId"] = json!(agent_id);
            }
            let result = gx_rpc_with_timeout(
                None,
                "/api/startWorkOnTicket",
                start_params,
                START_WORK_TIMEOUT,
            )
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = None;
                match result {
                    Ok(started) => {
                        let session_id = started["sessionId"].as_str().unwrap_or_default();
                        let project_id = started["projectId"]
                            .as_str()
                            .unwrap_or(&this.project_id)
                            .to_string();
                        this.created = None;
                        window.remove_window();
                        (this.host)(
                            CreateLinearTicketModalCommand::Started {
                                project_id,
                                session_id: session_id.to_string(),
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.error = Some(format!(
                            "{identifier} was created, but work did not start: {}",
                            error.message
                        ));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Enter creates from the title; in the description it is a newline, and Cmd/Ctrl+Enter
    /// creates from anywhere.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        let in_description = self
            .description_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        if in_description && !action.secondary {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.create(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.project_select.open
            || self.team_select.open
            || self.linear_project_select.open
            || self.agent_select.open
        {
            self.close_selects();
            cx.notify();
            return;
        }
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let lists = [
            (0, self.teams.len()),
            (1, self.linear_projects.len() + 1),
            (2, self.agents.len()),
            (3, self.projects.len()),
        ];
        for (which, count) in lists {
            let select = match which {
                0 => &mut self.team_select,
                1 => &mut self.linear_project_select,
                2 => &mut self.agent_select,
                _ => &mut self.project_select,
            };
            match select.handle_key(key, count) {
                ModalSelectKey::Consumed => {
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                ModalSelectKey::Choose(index) => {
                    self.choose(which, index, window, cx);
                    cx.stop_propagation();
                    return;
                }
                ModalSelectKey::Ignored => {}
            }
        }
        match key {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.create(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// `which`: 0 team, 1 Linear project (row 0 is "No Linear project"), 2 agent, 3 project.
    fn choose(&mut self, which: u8, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.close_selects();
        match which {
            3 => {
                if let Some((project_id, name)) = self.projects.get(index).cloned()
                    && project_id != self.project_id
                {
                    // The tracker, teams and Linear projects come from the project's own
                    // workspace and Linear key.
                    self.project_id = project_id;
                    self.project_name = name;
                    self.tracker = Tracker::Loading;
                    self.repo = None;
                    self.teams.clear();
                    self.team_index = None;
                    self.teams_loading = true;
                    self.linear_projects.clear();
                    self.linear_project_index = None;
                    self.error = None;
                    self.load_tracker(window, cx);
                }
            }
            0 => {
                if self.team_index != Some(index) {
                    self.team_index = Some(index);
                    self.load_linear_projects(window, cx);
                }
            }
            1 => self.linear_project_index = index.checked_sub(1),
            _ => self.agent_index = index,
        }
        cx.notify();
    }

    fn toggle(&mut self, which: u8, window: &mut Window, cx: &mut Context<Self>) {
        let (select, selected) = match which {
            3 => (
                &self.project_select,
                self.projects
                    .iter()
                    .position(|(id, _)| *id == self.project_id),
            ),
            0 => (&self.team_select, self.team_index),
            1 => (
                &self.linear_project_select,
                Some(self.linear_project_index.map_or(0, |index| index + 1)),
            ),
            _ => (&self.agent_select, Some(self.agent_index)),
        };
        let was_open = select.open;
        self.close_selects();
        if !was_open {
            self.focus_handle.focus(window, cx);
            match which {
                0 => self.team_select.toggle(selected),
                1 => self.linear_project_select.toggle(selected),
                2 => self.agent_select.toggle(selected),
                _ => self.project_select.toggle(selected),
            }
        }
        cx.notify();
    }

    fn select_field(
        &self,
        label: &'static str,
        select: &ModalSelect,
        id: &'static str,
        value: Option<String>,
        placeholder: &'static str,
        which: u8,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let trigger = modal_select_trigger(
            &p,
            select,
            id,
            value,
            placeholder,
            self.busy.is_some(),
            move |this: &mut Self, window, cx| this.toggle(which, window, cx),
            cx,
        );
        v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, label))
            .child(
                h_flex()
                    .w_full()
                    .on_children_prepainted(capture_child_bounds(select.trigger_bounds.clone(), 0))
                    .child(trigger),
            )
            .into_any_element()
    }

    fn switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        on_toggle: impl Fn(&mut Self) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        h_flex()
            .id(id)
            .role(gpui::Role::Switch)
            .aria_toggled(a11y_toggled(checked))
            .aria_label(label)
            .w_full()
            .justify_between()
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                if this.busy.is_none() {
                    on_toggle(this);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(label),
            )
            .child(modal_switch(&p, checked, self.busy.is_some()))
            .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let busy = self.busy.is_some();
        let team_value = self
            .team_index
            .and_then(|index| self.teams.get(index))
            .map(|(_, label)| label.clone());
        let linear_project_value = Some(
            self.linear_project_index
                .and_then(|index| self.linear_projects.get(index))
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| NO_LINEAR_PROJECT.to_string()),
        );
        let mut body = v_flex().w_full().gap(px(16.0));
        // CDXC:WorkMode 2026-10-09 DECISION:
        // User: a Linear ticket can be created from the project header's "…" menu "(also can be created from the 'Work' page)". The Work page opens this same dialog; when it shows several work-mode projects and is not filtered to one, the dialog asks which project the ticket's work belongs to.
        if self.projects.len() > 1 {
            let project_value = self
                .projects
                .iter()
                .find(|(id, _)| *id == self.project_id)
                .map(|(_, name)| name.clone());
            body = body.child(h_flex().w_full().child(self.select_field(
                "Project",
                &self.project_select,
                "create-linear-ticket-project",
                project_value,
                "Choose a project",
                3,
                cx,
            )));
        }
        body = body.child(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(modal_section_title(&p, "Title"))
                .child(modal_text_input(&p, &self.title_input, busy, window, cx)),
        );
        body = body.child(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(modal_section_title(&p, "Description (optional)"))
                .child(modal_text_area(
                    &p,
                    &self.description_input,
                    Some(96.0),
                    busy,
                    window,
                    cx,
                )),
        );
        if self.tracker != Tracker::Github {
            body = body.child(
                h_flex()
                    .w_full()
                    .gap(px(12.0))
                    .child(self.select_field(
                        "Team",
                        &self.team_select,
                        "create-linear-ticket-team",
                        team_value,
                        if self.teams_loading {
                            "Loading teams…"
                        } else {
                            "Choose a team"
                        },
                        0,
                        cx,
                    ))
                    .child(self.select_field(
                        "Linear project (optional)",
                        &self.linear_project_select,
                        "create-linear-ticket-linear-project",
                        linear_project_value,
                        NO_LINEAR_PROJECT,
                        1,
                        cx,
                    )),
            );
        }
        body = body
            .child(self.switch_row(
                "create-linear-ticket-assign",
                "Assign to me",
                self.assign_to_me,
                |this| this.assign_to_me = !this.assign_to_me,
                cx,
            ))
            .child(self.switch_row(
                "create-linear-ticket-start",
                "Start work now",
                self.start_work,
                |this| {
                    this.start_work = !this.start_work;
                    this.fit.refit();
                },
                cx,
            ));
        if self.start_work && !self.agents.is_empty() {
            body = body.child(
                h_flex().w_full().child(
                    self.select_field(
                        "Agent",
                        &self.agent_select,
                        "create-linear-ticket-agent",
                        self.agents
                            .get(self.agent_index)
                            .map(|(_, name)| name.clone()),
                        "Choose an agent",
                        2,
                        cx,
                    ),
                ),
            );
            body = body.child(modal_hint(
                &p,
                "Starts the agent in a new worktree on the ticket's branch and links the session to the ticket. Nothing is sent to the agent.",
            ));
        }
        if let Some(status) = self.busy {
            body = body.child(modal_hint(&p, status));
        }
        if let Some(error) = self.error.clone() {
            body = body.child(modal_error(&p, error));
        }
        body.into_any_element()
    }

    fn render_open_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let (select, id, labels, selected, which) = if self.project_select.open {
            (
                &self.project_select,
                "create-linear-ticket-project-menu",
                self.projects
                    .iter()
                    .map(|(_, name)| name.clone())
                    .collect::<Vec<_>>(),
                self.projects
                    .iter()
                    .position(|(id, _)| *id == self.project_id),
                3u8,
            )
        } else if self.team_select.open {
            (
                &self.team_select,
                "create-linear-ticket-team-menu",
                self.teams.iter().map(|(_, label)| label.clone()).collect(),
                self.team_index,
                0,
            )
        } else if self.linear_project_select.open {
            let mut labels = vec![NO_LINEAR_PROJECT.to_string()];
            labels.extend(self.linear_projects.iter().map(|(_, label)| label.clone()));
            (
                &self.linear_project_select,
                "create-linear-ticket-linear-project-menu",
                labels,
                Some(self.linear_project_index.map_or(0, |index| index + 1)),
                1,
            )
        } else {
            (
                &self.agent_select,
                "create-linear-ticket-agent-menu",
                self.agents.iter().map(|(_, name)| name.clone()).collect(),
                Some(self.agent_index),
                2,
            )
        };
        modal_select_menu(
            &p,
            select,
            id,
            &labels,
            selected,
            move |this: &mut Self, index, window, cx| this.choose(which, index, window, cx),
            |this: &mut Self, _window, cx| {
                this.close_selects();
                cx.notify();
            },
            window,
            cx,
        )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "create-linear-ticket-cancel",
            if self.created.is_some() {
                "Close"
            } else {
                "Cancel"
            },
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let label: SharedString = match (&self.created, self.start_work) {
            (Some(_), _) => "Start Work".into(),
            (None, true) => "Create and Start".into(),
            (None, false) => "Create".into(),
        };
        let create = modal_action_button(
            &p,
            "create-linear-ticket-create",
            label,
            None,
            ModalButtonTone::Primary,
            !self.can_create(),
            |this, window, cx| this.create(window, cx),
            cx,
        );
        modal_footer(vec![cancel, create])
    }
}

/// `[{ id, … }]` → `(id, label)`, skipping rows without an id or a label.
fn choices(list: &Value, label: impl Fn(&Value) -> Option<String>) -> Vec<Choice> {
    list.as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| Some((row["id"].as_str()?.to_string(), label(row)?)))
                .collect()
        })
        .unwrap_or_default()
}

impl Render for GpuiCreateLinearTicketModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let (title, description) = match self.tracker {
            Tracker::Github => (
                GITHUB_TITLE,
                format!(
                    "A new GitHub issue in {}. Start work now opens an agent on the issue's own branch.",
                    self.repo
                        .clone()
                        .unwrap_or_else(|| format!("{}'s repo", self.project_name))
                ),
            ),
            _ => (
                TITLE,
                format!(
                    "A new Linear ticket for {}. Start work now opens an agent on the ticket's own branch.",
                    self.project_name
                ),
            ),
        };
        let content = vec![
            modal_header(&p, title, Some(description)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        let menu = self.render_open_menu(window, cx);
        modal_shell(
            &p,
            "ghostex-gpui-create-linear-ticket-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            menu,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiCreateLinearTicketModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
