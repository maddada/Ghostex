//! The Agents page: the Agents card (a summary line when something needs fixing, one row per
//! agent with an on/off switch, drag to order, the "More agents" grid of agents that are off and
//! never used, the one-time tidy-up offer and "Add custom agent"), the Defaults card (Default
//! Prompt Agent, Title Generation Agent and its custom command, Agent approvals with the Skip
//! permissions? confirmation) and the Session resume hooks card.
//!
//! With a remote machine picked at the top (../machine_picker.rs) the page shows that machine's
//! agent CLIs instead (remote_machine.rs).
//!
//! Like the React tab panel, the page forgets its view state (open rows, the form, CLI answers,
//! a dragged order) when another page is shown; hook status belongs to the modal and stays.
mod chips;
mod cli;
mod config;
mod editor;
mod hooks_card;
mod inline_input;
mod logos;
mod model;
mod panel;
mod remote_machine;
mod roster;
mod roster_data;
mod select;
mod status;
mod turn_on;

use super::super::fields::{FieldStates, SettingsPage};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::should_show_section;
use super::super::store::{SettingsStore, SettingsStoreEvent};
use super::{machine_picker, windows_link_notice};
use gpui::{
    AnyView, App, AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Styled as _, Window, div,
};
use gpui_component::input::InputState;
use serde_json::json;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// The icon paths the page draws (Tabler icons in assets/modals/settings/).
mod icons {
    pub(super) const ALERT_TRIANGLE: &str = "modals/settings/alert-triangle.svg";
    pub(super) const CHEVRON_DOWN: &str = "modals/settings/chevron-down.svg";
    pub(super) const CIRCLE_CHECK_FILLED: &str = "modals/settings/circle-check-filled.svg";
    pub(super) const CIRCLE_X: &str = "modals/settings/circle-x.svg";
    pub(super) const CODE_DOTS: &str = "modals/settings/code-dots.svg";
    pub(super) const COPY: &str = "modals/settings/copy.svg";
    pub(super) const ARROW_BACK_UP: &str = "modals/settings/arrow-back-up.svg";
    pub(super) const SPARKLES: &str = "modals/settings/sparkles.svg";
    pub(super) const DOTS: &str = "titlebar/dots.svg";
    pub(super) const DOWNLOAD: &str = "modals/settings/download.svg";
    pub(super) const EXTERNAL_LINK: &str = "modals/settings/external-link.svg";
    pub(super) const GRIP_VERTICAL: &str = "modals/settings/grip-vertical.svg";
    pub(super) const INFO_CIRCLE: &str = "modals/settings/info-circle.svg";
    pub(super) const MESSAGE_CIRCLE: &str = "modals/settings/message-circle.svg";
    pub(super) const PENCIL: &str = "modals/settings/pencil.svg";
    pub(super) const PLUS: &str = "modals/settings/plus.svg";
    pub(super) const REFRESH: &str = "modals/settings/refresh.svg";
    pub(super) const SEARCH: &str = "modals/settings/search.svg";
    pub(super) const SELECTOR: &str = "modals/settings/selector.svg";
    pub(super) const TRASH: &str = "modals/settings/trash.svg";
    pub(super) const X: &str = "modals/settings/x.svg";
}

/// The anchor of the Agents roster section (the `agentHooks` deep link lands on it).
const ROSTER_ANCHOR: &str = "agentList";
/// The anchor of the Session resume hooks card.
const HOOKS_ANCHOR: &str = "agentHooks";

/// Creates the Agents page view.
pub(crate) fn agents_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| AgentsTab::new(store.clone(), window, cx))
        .into()
}

pub(crate) struct AgentsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The page is the one shown (`isActive`).
    active: bool,
    /// `agentHookStatusLoading` of the modal host: set when a hook request is posted, cleared when
    /// a complete `agentHookStatus` arrives.
    hook_status_loading: bool,
    /// Agent ids whose row is expanded (CDXC:AgentHooks 2026-08-28: every row starts collapsed).
    expanded: Vec<String>,
    /// `draftAgentIds`: the order after a drop, until the synced roster catches up.
    draft_agent_ids: Option<Vec<String>>,
    synced_agent_ids: Vec<String>,
    /// The "Add custom agent" form at the end of the list.
    editor: Option<editor::AgentEditor>,
    /// gxserver's `agentRoster`: every agent, on and off, with when it was last used. `None`
    /// until read (or from an older gxserver), when the page falls back to `hud.agents`.
    roster: Option<Vec<model::AgentButton>>,
    roster_seq: u64,
    /// The `hud.agents` the roster was read against; a change re-reads it.
    roster_hud_signature: Option<String>,
    /// Switches flipped here whose write has not come back yet.
    pending_enabled: HashMap<String, bool>,
    /// The inline step a row shows right after it was turned on (install its CLI, or the
    /// one-time resume hook question).
    turn_on: HashMap<String, turn_on::TurnOnStep>,
    /// CLI agents whose resume hook installs once their CLI install finishes.
    hook_after_cli: HashSet<String>,
    /// The Name and Command inputs of expanded rows, keyed by input id.
    inline_inputs: HashMap<SharedString, Entity<InputState>>,
    /// "More agents" is open.
    more_agents_open: bool,
    /// The Session resume hooks card's ⋯ menu is open, and where its trigger was last drawn.
    hooks_menu_open: bool,
    hooks_menu_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Inline inputs whose text was saved and has not come back from gxserver yet.
    inline_committed: HashMap<SharedString, String>,
    /// The Skip permissions? confirmation is open.
    confirming_bypass: bool,
    /// The Uninstall hooks for all agents? confirmation is open (shares `confirm_focus`).
    confirming_uninstall_hooks: bool,
    confirm_focus: FocusHandle,
    cli: cli::CliModel,
    dropdowns: HashMap<SharedString, select::DropdownState>,
    /// `lastTargetedAgentsSectionRef`.
    last_targeted_section: Option<String>,
    /// The preview binary's page state was applied.
    preview_applied: bool,
    /// A dropdown to open on its next render (the preview binary's dropdown states).
    pending_open_dropdown: Option<String>,
    /// The remote machine the page shows (the machine picker's pick); `None` is this computer.
    shown_machine: Option<String>,
    shown_machine_name: Option<String>,
}

impl AgentsTab {
    fn new(store: Entity<SettingsStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_in(&store, window, |page: &mut Self, _, window, cx| {
            page.sync_active(window, cx);
            page.follow_machine(cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &store,
            |page: &mut Self, store, event: &SettingsStoreEvent, cx| {
                match event {
                    SettingsStoreEvent::HostPayload(kind) if kind == "agentHookStatus" => {}
                    _ => return,
                }
                {
                    // A partial post of the host's provider walk says `complete: false`.
                    let complete = store
                        .read(cx)
                        .host_payload("agentHookStatus")
                        .and_then(|payload| payload.get("complete"))
                        .and_then(serde_json::Value::as_bool)
                        != Some(false);
                    if complete {
                        page.hook_status_loading = false;
                    }
                    cx.notify();
                }
            },
        )
        .detach();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            active: false,
            hook_status_loading: false,
            expanded: Vec::new(),
            draft_agent_ids: None,
            synced_agent_ids: Vec::new(),
            editor: None,
            roster: None,
            roster_seq: 0,
            roster_hud_signature: None,
            pending_enabled: HashMap::new(),
            turn_on: HashMap::new(),
            hook_after_cli: HashSet::new(),
            inline_inputs: HashMap::new(),
            inline_committed: HashMap::new(),
            more_agents_open: false,
            hooks_menu_open: false,
            hooks_menu_trigger: Rc::new(Cell::new(None)),
            confirming_bypass: false,
            confirming_uninstall_hooks: false,
            confirm_focus: cx.focus_handle(),
            cli: cli::CliModel::default(),
            dropdowns: HashMap::new(),
            last_targeted_section: None,
            preview_applied: false,
            pending_open_dropdown: None,
            shown_machine: None,
            shown_machine_name: None,
        };
        page.follow_machine(cx);
        page.sync_active(window, cx);
        // The Add custom agent form the last visit left open (settings_modal/restore.rs).
        if let Some(form) = page.store.update(cx, |store, _| {
            store.open_forms_mut().take(SettingsTabId::Agents)
        }) {
            page.restore_editor(&form, window, cx);
        }
        page
    }

    /// Follows the shown page: entering it asks for hook status (once per modal) and the CLI list;
    /// leaving it drops the page's view state but the Add custom agent form, as the unmounted
    /// React tab panel did.
    fn sync_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.store.read(cx).active_tab() == SettingsTabId::Agents;
        if active == self.active {
            if active {
                self.follow_deep_link(cx);
            }
            return;
        }
        self.active = active;
        if !active {
            // The Add custom agent form stays: only its Cancel or Add agent forgets it
            // (CDXC:Settings 2026-10-16 in settings_modal/restore.rs).
            self.expanded.clear();
            self.draft_agent_ids = None;
            self.roster_hud_signature = None;
            self.pending_enabled.clear();
            self.turn_on.clear();
            self.inline_inputs.clear();
            self.inline_committed.clear();
            self.confirming_bypass = false;
            self.confirming_uninstall_hooks = false;
            self.hooks_menu_open = false;
            self.cli_reset();
            self.dropdowns.clear();
            self.last_targeted_section = None;
            return;
        }
        // Like the React effects, the requests go out after the page is mounted.
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |page, cx| {
                if !page.active {
                    return;
                }
                let has_status = page
                    .store
                    .read(cx)
                    .host_payload("agentHookStatus")
                    .is_some();
                if !has_status && !page.hook_status_loading {
                    page.request_hook_status(cx);
                }
                page.cli_list_refresh(cx);
                page.roster_refresh(cx);
                machine_picker::request_machine_states(&page.store, cx);
            });
        })
        .detach();
        self.apply_preview_state(window, cx);
        self.follow_deep_link(cx);
    }

    /// Follows the machine picker: another machine drops every CLI answer and open row of the
    /// last one and reads the new machine's list.
    fn follow_machine(&mut self, cx: &mut Context<Self>) {
        let choice = machine_picker::picked_machine_choice(&self.store, cx);
        let machine = choice.as_ref().and_then(|choice| choice.machine_id.clone());
        self.shown_machine_name = choice.map(|choice| choice.name);
        if machine == self.shown_machine {
            return;
        }
        self.shown_machine = machine;
        self.expanded.clear();
        self.cli_reset();
        self.cli_list_refresh(cx);
    }

    /// `initialAgentsSection`: close an open editor first, then scroll the roster into view once
    /// per visit.
    fn follow_deep_link(&mut self, cx: &mut Context<Self>) {
        let Some(section) = self.store.read(cx).request().initial_agents_section.clone() else {
            return;
        };
        if self.last_targeted_section.as_deref() == Some(section.as_str()) {
            return;
        }
        if self.editor.is_some() {
            self.editor = None;
            cx.notify();
        }
        self.last_targeted_section = Some(section);
        self.store.update(cx, |store, cx| {
            store.scroll_to_section(SettingsTabId::Agents, ROSTER_ANCHOR, cx)
        });
    }

    /// `onRequestAgentHookStatus`.
    fn request_hook_status(&mut self, cx: &mut Context<Self>) {
        self.hook_status_loading = true;
        self.post(json!({ "type": "requestAgentHookStatus" }), cx);
    }

    /// `onInstallAgentHooks(agentIds)`; `None` installs every hook.
    fn install_hooks(&mut self, agent_ids: Option<Vec<String>>, cx: &mut Context<Self>) {
        self.hook_status_loading = true;
        self.post(
            json!({ "agentIds": agent_ids, "type": "installAgentHooks" }),
            cx,
        );
    }

    /// `onUninstallAgentHooks(agentIds)`; `None` removes every Ghostex hook.
    fn uninstall_hooks(&mut self, agent_ids: Option<Vec<String>>, cx: &mut Context<Self>) {
        self.hook_status_loading = true;
        self.post(
            json!({ "agentIds": agent_ids, "type": "uninstallAgentHooks" }),
            cx,
        );
    }

    fn post(&self, message: serde_json::Value, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.post_message(message, cx));
    }

    fn save(&self, key: &str, value: serde_json::Value, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.update_setting(key, value, cx));
    }

    /// The preview binary's page states (`preview_state`): open rows, the editor, the confirmation.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        self.preview_applied = true;
        let Some(state) = self.store.read(cx).request().preview_state.clone() else {
            return;
        };
        match state.as_str() {
            "agents-expanded" => self.expanded = vec!["codex".into(), "claude".into()],
            "agents-clis" => {
                self.expanded = vec!["zcode".into(), "claude".into(), "pi".into(), "grok".into()]
            }
            "agents-cli-running" => self.expanded = vec!["gemini".into()],
            "agents-cli-failed" => self.expanded = vec!["grok".into()],
            "agents-editor-new" => self.open_editor(None, window, cx),
            "agents-editor-edit" => self.expanded = vec!["codex".into()],
            "agents-skip-confirm" => self.confirming_bypass = true,
            "agents-prompt-dropdown" => {
                self.pending_open_dropdown = Some("defaultPromptAgent".to_string())
            }
            "agents-type-dropdown" => {
                self.open_editor(None, window, cx);
                self.pending_open_dropdown = Some("agent-editor-type".to_string());
            }
            _ => {}
        }
    }
}

impl super::HoldsUnsavedInput for AgentsTab {
    /// The Add custom agent form or the Skip permissions? confirmation is open.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.editor.is_some() || self.confirming_bypass || self.confirming_uninstall_hooks
    }
}

impl super::open_forms::RemembersOpenForm for AgentsTab {
    /// The Add custom agent form (not the Skip permissions? or uninstall confirmations).
    fn open_form(&self, cx: &gpui::App) -> Option<serde_json::Value> {
        self.open_editor_form(cx)
    }
}

impl SettingsPage for AgentsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }

    /// CDXC:AgentLauncher 2026-10-14 DECISION:
    /// User: "let's not allow dragging to reorder on any agent here in the list that's expanded. when trying to drag we first collapse it then allow dragging". A drag that starts on an expanded agent row closes the row, and the drag then carries the closed row; the drag list waits one measured frame so the lifted row and its slot are the closed height. Other open rows stay open.
    fn reorder_prepare_lift(&mut self, list: &str, index: usize, cx: &mut Context<Self>) -> bool {
        if list != roster::LIST {
            return false;
        }
        let Some(agent_id) = self
            .ordered_agents(cx)
            .get(index)
            .map(|agent| agent.agent_id.clone())
        else {
            return false;
        };
        if !self.expanded.contains(&agent_id) {
            return false;
        }
        self.toggle_expanded(&agent_id, cx);
        true
    }
}

impl Render for AgentsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching_pages) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Agents),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Agents,
                &matching_pages,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        }
        if let Some(picker) = machine_picker::render_machine_picker(
            self,
            &p,
            &self.store.clone(),
            "agent CLIs",
            window,
            cx,
        ) {
            blocks.push(PageBlock::section("machinePicker", picker));
        }
        // A remote machine's own CLI answers say its platform; this computer's is the build's.
        let windows = match &self.shown_machine {
            Some(_) => self
                .cli
                .list
                .values()
                .any(|state| windows_link_notice::is_windows(Some(&state.platform))),
            None => cfg!(target_os = "windows"),
        };
        if windows {
            let remote_name = self
                .shown_machine
                .as_ref()
                .map(|_| self.cli_machine_label());
            blocks.push(PageBlock::section(
                "windowsLinkNotice",
                windows_link_notice::render_windows_link_notice(
                    &p,
                    &self.store.clone(),
                    remote_name.as_deref(),
                    cx,
                ),
            ));
        }
        if self.shown_machine.is_some() {
            if let Some(section) = self.render_remote_agents(&p, window, cx) {
                blocks.push(PageBlock::section(ROSTER_ANCHOR, section));
            }
            return div().size_full().child(settings_page(
                &self.store,
                SettingsTabId::Agents,
                &p,
                blocks,
                cx,
            ));
        }
        // CDXC:AgentLauncher 2026-10-06 DECISION: User: "ok implement the plan": the agents come first, the Defaults card (today's Config) below them, and the bulk hook tools in their own card at the bottom.
        if (self.editor.is_some() || should_show_section(&search.section("agentList"), None))
            && let Some(section) = self.render_roster(&p, window, cx)
        {
            blocks.push(PageBlock::section(ROSTER_ANCHOR, section));
        }
        if should_show_section(&search.section("config"), None)
            && let Some(section) = self.render_config(&p, &search, window, cx)
        {
            blocks.push(PageBlock::section("config", section));
        }
        if should_show_section(&search.section(HOOKS_ANCHOR), None)
            && let Some(section) = self.render_hooks_card(&p, &search, window, cx)
        {
            blocks.push(PageBlock::section(HOOKS_ANCHOR, section));
        }
        let dialog = self
            .render_bypass_dialog(&p, window, cx)
            .or_else(|| self.render_uninstall_hooks_dialog(&p, window, cx));
        div()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::Agents,
                &p,
                blocks,
                cx,
            ))
            .children(dialog)
    }
}
