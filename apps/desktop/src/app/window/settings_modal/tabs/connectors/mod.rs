//! The Connectors page (experimental): each workspace's connectors, what the `ghostex connectors`
//! CLI does with buttons. Composio (sign in, connect apps), Executor (turn it on, add apps from
//! its catalog or a remote MCP server, sign them in), the rules for calls that change something
//! (Ask, Allow, Never) and the workspace's own gh and gws sign-ins. It reads `/api/readConnectors`
//! when it opens and on every `connectorsChanged` the app forwards, and after each action.
//! SEE-ALSO: server/src/connectors/ (the routes), server/src/connectors/gate.rs (the experimental
//! switch this page follows, `SettingsStore::connectors_visible`).
mod actions;
mod cli;
mod composio;
mod data;
mod executor;
mod rules;
mod signin_panel;
mod widgets;

use super::super::fields::notice::{greyed_out, is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, FieldStates, SettingsPage, settings_button, static_note,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent};
use gpui::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn connectors_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| ConnectorsTab::new(store.clone(), cx)).into()
}

pub(crate) struct ConnectorsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The last `/api/readConnectors` answer.
    connectors: Option<Value>,
    error: Option<String>,
    /// A read is in flight.
    loading: bool,
    /// Actions in flight, keyed `<workspace>:<action>`, so their buttons show the wait.
    busy: HashSet<String>,
    /// A line to show under a workspace's section after an action (a sign-in code, what to do in
    /// the browser), keyed `<workspace>:<section>`.
    notes: HashMap<String, String>,
    /// A browser sign-in waiting to be finished, keyed `<workspace>:<section>` (signin_panel.rs).
    pending: HashMap<String, signin_panel::PendingSignIn>,
    /// Executor catalog results per workspace, from the last search.
    catalog_results: HashMap<String, Vec<Value>>,
    /// The page was the active one at the last store change, to read again when it comes back.
    was_active: bool,
    /// The next read while an Executor downloads or starts.
    pending_reread: Option<gpui::Task<()>>,
}

impl SettingsPage for ConnectorsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl ConnectorsTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Connectors;
            if active && !page.was_active {
                page.reload(cx);
            }
            page.was_active = active;
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &store,
            |page: &mut Self, _, event: &SettingsStoreEvent, cx| {
                let SettingsStoreEvent::HostPayload(kind) = event;
                if kind == "connectorsChanged" {
                    page.reload(cx);
                }
            },
        )
        .detach();
        cx.spawn(async move |page, cx| {
            let _ = page.update(cx, |page, cx| page.reload(cx));
        })
        .detach();
        Self {
            store,
            fields: FieldStates::default(),
            connectors: None,
            error: None,
            loading: false,
            busy: HashSet::new(),
            notes: HashMap::new(),
            pending: HashMap::new(),
            catalog_results: HashMap::new(),
            was_active: true,
            pending_reread: None,
        }
    }

    fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn is_busy(&self, workspace: &str, action: &str) -> bool {
        self.busy.contains(&data::busy_key(workspace, action))
    }
}

impl Render for ConnectorsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Connectors),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Connectors,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Connectors, &p, blocks, cx);
        }
        let intro = static_note(
            &p,
            "Connect your apps so agents can use them. Each workspace has its own sign-ins and apps. Reads run on their own; before an agent sends, posts, changes or deletes anything, Ghostex asks you in that session's chat. Experimental.",
            false,
        );
        let refresh = settings_button(
            &p,
            "connectors-reload",
            if self.loading {
                "Reading…"
            } else {
                "Refresh"
            },
            Some("modals/settings/refresh.svg"),
            ButtonVariant::Outline,
            self.loading || !self.rpc_available(cx),
            None,
            |page: &mut Self, _window, cx| page.reload(cx),
            cx,
        );
        blocks.push(PageBlock::plain(
            h_flex()
                .w_full()
                .gap(px(12.0))
                .items_center()
                .child(div().flex_1().min_w_0().child(intro))
                .child(refresh),
        ));
        let unreachable = self.error.as_deref().is_some_and(is_service_unreachable);
        if unreachable {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "connectors-unreachable",
                |page: &mut Self, _window, cx| page.reload(cx),
                cx,
            )));
        } else if let Some(error) = self.error.clone() {
            blocks.push(PageBlock::plain(static_note(&p, error, true)));
        }
        // What follows needs the service to save: dimmed while it does not answer.
        let first_control_block = blocks.len();
        let workspaces_enabled = self
            .connectors
            .as_ref()
            .and_then(|connectors| connectors.get("workspacesEnabled"))
            .and_then(Value::as_bool)
            == Some(true);
        let section_shown = |section: &str| {
            let result = search.section(section);
            !result.is_searching || result.has_visible()
        };
        for workspace in data::workspaces(self.connectors.as_ref()) {
            let id = data::text(&workspace, "workspaceId");
            let title = |section: &str| {
                if workspaces_enabled {
                    format!("{} · {section}", data::text(&workspace, "name"))
                } else {
                    section.to_string()
                }
            };
            if section_shown("composio")
                && let Some(section) = self.composio_section(&p, &workspace, title("Composio"), cx)
            {
                blocks.push(PageBlock::section(format!("{id}-composio"), section));
            }
            if section_shown("executor")
                && let Some(section) =
                    self.executor_section(&p, &workspace, title("Executor"), window, cx)
            {
                blocks.push(PageBlock::section(format!("{id}-executor"), section));
            }
            if section_shown("rules")
                && let Some(section) =
                    self.rules_section(&p, &workspace, title("When agents change something"), cx)
            {
                blocks.push(PageBlock::section(format!("{id}-rules"), section));
            }
            if section_shown("cli")
                && let Some(section) =
                    self.cli_section(&p, &workspace, title("Command-line sign-ins"), cx)
            {
                blocks.push(PageBlock::section(format!("{id}-cli"), section));
            }
        }
        if unreachable {
            for block in &mut blocks[first_control_block..] {
                let element = std::mem::replace(&mut block.element, gpui::Empty.into_any_element());
                block.element = greyed_out(element);
            }
        }
        settings_page(&self.store, SettingsTabId::Connectors, &p, blocks, cx)
    }
}
