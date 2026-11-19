//! Paired computers: the server links a computer keeps (GitHub #674), in a connected machine's
//! Edit dialog and, for this computer, on the Remote page.
//!
//! CDXC:ServerLinks 2026-11-19 DECISION:
//! User (4B on the server link design): "build the Paired computers list in Settings > Remote in this
//! change", so a link two headless servers keep between themselves is seen and ended from the app
//! like any paired device. The list asks that computer's own gxserver (`/api/serverLinks`, through
//! the machine's tunnel for a remote one), so its rows are exactly what that computer keeps. Each
//! row: the other computer, where the link stands, a row-wide Agent access switch (kept on disk
//! there, both ways from that computer) and Remove, which ends the link on both computers. An
//! older gxserver has no such route and the card does not show.
//! SEE-ALSO: server/src/remote_access/server_links.rs, server/src/remote_access/server_link_dialer.rs.

use super::super::super::super::native_modal_kit::*;
use super::super::super::store::store_machine_gxserver_rpc;
use super::RemoteTab;
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const ENDPOINT: &str = "/api/serverLinks";
const TIMEOUT: Duration = Duration::from_secs(20);
/// A removal tells the other computer first, through its tunnel.
const REMOVE_TIMEOUT: Duration = Duration::from_secs(60);

/// One server link, as `/api/serverLinks` lists it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LinkRow {
    pub(super) server_id: String,
    pub(super) name: String,
    pub(super) state: String,
    pub(super) outgoing: bool,
    pub(super) incoming: bool,
    pub(super) agent_access: bool,
    pub(super) error: Option<String>,
    pub(super) words: Vec<String>,
}

/// One computer's list: this computer's (`""`) or a saved machine's (its id).
#[derive(Clone, Debug, Default)]
pub(super) struct LinksView {
    pub(super) rows: Vec<LinkRow>,
    pub(super) loaded: bool,
    pub(super) error: Option<String>,
    /// The row whose Remove waits for its confirmation.
    pub(super) confirming: Option<String>,
    /// The row a switch or Remove is being sent for.
    pub(super) busy: Option<String>,
}

fn read_rows(answer: &Value) -> Vec<LinkRow> {
    let text = |row: &Value, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let flag = |row: &Value, key: &str| row.get(key).and_then(Value::as_bool) == Some(true);
    answer
        .get("links")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| LinkRow {
            server_id: text(row, "serverId"),
            name: text(row, "name"),
            state: text(row, "state"),
            outgoing: flag(row, "outgoing"),
            incoming: flag(row, "incoming"),
            agent_access: row.get("agentAccess").and_then(Value::as_bool) != Some(false),
            error: row
                .get("error")
                .and_then(Value::as_str)
                .filter(|error| !error.is_empty())
                .map(str::to_string),
            words: row
                .get("words")
                .and_then(Value::as_array)
                .map(|words| {
                    words
                        .iter()
                        .filter_map(|word| word.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .filter(|row| !row.server_id.is_empty())
        .collect()
}

/// Where the link stands, in a line.
fn row_detail(row: &LinkRow) -> String {
    match row.state.as_str() {
        "pairing" => format!(
            "Waiting for approval on {}. Words: {}",
            row.name,
            row.words.join(" · ")
        ),
        "ended" => format!(
            "Not linked: {}",
            row.error.clone().unwrap_or_else(|| "it ended.".to_string())
        ),
        _ if !row.agent_access => {
            "Agent access is off: neither computer's agents use the other.".to_string()
        }
        _ => match (row.outgoing, row.incoming) {
            (true, true) => "Linked both ways: the agents on each can use the other.".to_string(),
            (true, false) => "Linked one way so far: waiting for it to link back.".to_string(),
            (false, true) => {
                "Its agents can use this computer; it isn't linked back yet.".to_string()
            }
            (false, false) => "Not linked.".to_string(),
        },
    }
}

fn key(machine: Option<&str>) -> String {
    machine.unwrap_or_default().to_string()
}

impl RemoteTab {
    /// Asks `machine`'s gxserver (this computer's for `None`) for its server links.
    pub(super) fn refresh_server_links(&mut self, machine: Option<String>, cx: &mut Context<Self>) {
        self.server_link_call(machine, json!({ "op": "list" }), TIMEOUT, None, cx);
    }

    fn server_link_call(
        &mut self,
        machine: Option<String>,
        params: Value,
        timeout: Duration,
        busy: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let slot = key(machine.as_deref());
        self.server_links.entry(slot.clone()).or_default().busy = busy;
        let weak = cx.weak_entity();
        store_machine_gxserver_rpc(
            &self.store,
            machine.as_deref(),
            ENDPOINT,
            params,
            timeout,
            move |result, cx| {
                let _ = weak.update(cx, |tab, cx| {
                    let view = tab.server_links.entry(slot).or_default();
                    view.busy = None;
                    view.loaded = true;
                    match result {
                        Ok(answer) => {
                            view.rows = read_rows(&answer);
                            view.error = None;
                        }
                        // An older gxserver has no such route: no card. Anything else says so.
                        Err(error) if !view.rows.is_empty() => view.error = Some(error),
                        Err(_) => {}
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn set_server_link_access(
        &mut self,
        machine: Option<String>,
        server_id: String,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        self.server_link_call(
            machine,
            json!({ "op": "setAgentAccess", "serverId": server_id, "on": on }),
            TIMEOUT,
            Some(server_id),
            cx,
        );
    }

    fn remove_server_link(
        &mut self,
        machine: Option<String>,
        server_id: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.server_links.get_mut(&key(machine.as_deref())) {
            view.confirming = None;
        }
        self.server_link_call(
            machine,
            json!({ "op": "remove", "serverId": server_id }),
            REMOVE_TIMEOUT,
            Some(server_id),
            cx,
        );
    }
}

/// The Paired computers card for `machine` (this computer for `None`), or `None` when it keeps no
/// server link (or its gxserver is older than server links).
pub(super) fn server_links_panel(
    tab: &RemoteTab,
    t: &RemoteTokens,
    machine: Option<String>,
    computer_name: &str,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let view = tab.server_links.get(&key(machine.as_deref()))?;
    if view.rows.is_empty() {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    for row in &view.rows {
        rows.push(link_row(t, view, row, machine.clone(), cx));
    }
    let mut card = v_flex()
        .id(SharedString::from(format!(
            "remote-server-links-{}",
            key(machine.as_deref())
        )))
        .w_full()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(gpui::Hsla::from(t.foreground))
                .child("Paired computers"),
        )
        .child(detail(
            t,
            format!(
                "Computers linked with {computer_name} as servers: their agents and {computer_name}'s reach each other with no app open. Remove ends a link on both computers."
            ),
            true,
        ))
        .child(super::paired_devices::rows_frame(t, rows));
    if let Some(error) = &view.error {
        card = card.child(
            div()
                .text_size(px(13.0))
                .text_color(gpui::Hsla::from(t.p.destructive))
                .child(error.clone()),
        );
    }
    Some(card.into_any_element())
}

fn link_row(
    t: &RemoteTokens,
    view: &LinksView,
    row: &LinkRow,
    machine: Option<String>,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let id = format!("{}-{}", key(machine.as_deref()), row.server_id);
    let busy = view.busy.as_deref() == Some(row.server_id.as_str());
    let confirming = view.confirming.as_deref() == Some(row.server_id.as_str());
    let settled = row.state == "linked" || row.state == "incomingOnly";
    let on = row.agent_access;
    // The text and the switch are one press target (row-wide switch, AGENTS.md); Remove sits
    // beside it, outside that target.
    let toggle_machine = machine.clone();
    let toggle_id = row.server_id.clone();
    let switch_area = h_flex()
        .id(SharedString::from(format!(
            "remote-server-link-{id}-access"
        )))
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(12.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .switch_row_press(
            switch_row_wash(t.p.foreground),
            busy || !settled,
            RowToggle {
                press: bind_press(cx, move |tab: &mut RemoteTab, _window, cx| {
                    tab.set_server_link_access(toggle_machine.clone(), toggle_id.clone(), !on, cx);
                }),
                switch: Some(RowSwitchA11y {
                    label: format!("Agent access with {}", row.name).into(),
                    checked: on,
                    accessibility_id: None,
                }),
            },
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(gpui::Hsla::from(t.foreground))
                        .child(row.name.clone()),
                )
                .child(detail(t, row_detail(row), true)),
        )
        .when(settled, |this| this.child(small_switch(&t.p, on)));
    let remove_machine = machine.clone();
    let remove_id = row.server_id.clone();
    let confirm_machine = machine.clone();
    let confirm_id = row.server_id.clone();
    let actions = if confirming {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .child(compact_button(
                &t.p,
                SharedString::from(format!("remote-server-link-{id}-confirm")),
                "Remove both ways",
                None,
                Look::Destructive,
                24.0,
                14.0,
                None,
                busy,
                None,
                move |tab: &mut RemoteTab, _window, cx| {
                    tab.remove_server_link(remove_machine.clone(), remove_id.clone(), cx);
                },
                cx,
            ))
            .child(compact_button(
                &t.p,
                SharedString::from(format!("remote-server-link-{id}-cancel")),
                "Cancel",
                None,
                Look::Filled,
                24.0,
                14.0,
                Some(t.muted),
                false,
                None,
                move |tab: &mut RemoteTab, _window, cx| {
                    if let Some(view) = tab.server_links.get_mut(&key(confirm_machine.as_deref())) {
                        view.confirming = None;
                    }
                    cx.notify();
                },
                cx,
            ))
            .into_any_element()
    } else {
        compact_button(
            &t.p,
            SharedString::from(format!("remote-server-link-{id}-remove")),
            "Remove",
            None,
            Look::Filled,
            24.0,
            14.0,
            Some(t.muted),
            busy,
            None,
            move |tab: &mut RemoteTab, _window, cx| {
                if let Some(view) = tab.server_links.get_mut(&key(machine.as_deref())) {
                    view.confirming = Some(confirm_id.clone());
                }
                let _ = &confirm_machine;
                cx.notify();
            },
            cx,
        )
    };
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .px(px(4.0))
        .py(px(4.0))
        .child(switch_area)
        .child(actions)
        .into_any_element()
}
