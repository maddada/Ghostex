//! The Sync page: what this computer keeps the same on the user's other computers, which computers
//! those are, and the recent changes two computers made apart (with Undo). It reads
//! `/api/readSettingsSync` when it opens, on every `settingsSyncChanged` the app forwards, and after
//! each action. The switches are this computer's own settings (`syncSpaces` and the rest).
//! SEE-ALSO: server/src/settings_sync/ (the routes), gx-core settings_sync (`SYNC_CATEGORIES`, the
//! rules and CDXC:SettingsSync 2026-11-06).
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, RowSpec, SettingsPage, setting_row,
    settings_button, settings_list_item, settings_section, static_note, switch_control,
    toggle_field,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, SettingsStoreEvent, store_gxserver_rpc};
use ghostex_gx_core::settings_sync::SYNC_CATEGORIES;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn sync_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| SyncTab::new(store.clone(), cx)).into()
}

pub(crate) struct SyncTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The last `/api/readSettingsSync` answer.
    status: Option<Value>,
    error: Option<String>,
    loading: bool,
    /// The page was the active one at the last store change, to read again when it comes back.
    was_active: bool,
}

impl SettingsPage for SyncTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl SyncTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Sync;
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
                if kind == "settingsSyncChanged" {
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
            status: None,
            error: None,
            loading: false,
            was_active: true,
        }
    }

    fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.call("/api/readSettingsSync", json!({}), cx);
    }

    /// Calls one of the page's routes; each answers (or is followed by) the page's new state.
    fn call(&mut self, path: &'static str, params: Value, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        self.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            path,
            params,
            READ_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.loading = false;
                    match result {
                        Ok(result) if result.get("computers").is_some() => {
                            page.status = Some(result);
                            page.error = None;
                        }
                        Ok(_) => page.reload(cx),
                        Err(error) => page.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn what_syncs(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let values = self.store.read(cx).values();
        let rows: Vec<AnyElement> = SYNC_CATEGORIES
            .iter()
            .map(|category| {
                let key = category.setting_key;
                let spec = RowSpec::new(category.label).description(describe(category.prefix));
                if category.available {
                    toggle_field(
                        self,
                        p,
                        key,
                        spec.modified(values.is_modified(key)),
                        values.bool(key),
                        cx,
                    )
                } else {
                    let reason: SharedString = "Coming soon".into();
                    let control = switch_control(
                        p,
                        SharedString::from(format!("{key}-switch")),
                        category.label,
                        false,
                        true,
                        Some(reason.clone()),
                        |_: &mut Self, _, _, _| {},
                        cx,
                    );
                    setting_row(
                        p,
                        key,
                        spec.disabled_reason(Some(reason)),
                        None,
                        control,
                        cx,
                    )
                }
            })
            .collect();
        settings_section(
            p,
            "What syncs",
            Some("Each computer has its own switches: something syncs between two computers when it is on in both.".into()),
            None,
            rows,
        )
    }

    fn computers(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let status = self.status.clone()?;
        let now = status.get("nowMs").and_then(Value::as_u64).unwrap_or(0);
        let disabled = self.loading || !self.rpc_available(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for computer in list(&status, "computers") {
            let server_id = text(&computer, "serverId");
            let this_computer = flag(&computer, "thisComputer");
            let removed = flag(&computer, "removed");
            let refused = computer
                .get("refusedHere")
                .and_then(Value::as_str)
                .map(str::to_string);
            let (dot, detail) = if this_computer {
                (ListItemStatus::Neutral, "This computer".to_string())
            } else if removed {
                (
                    ListItemStatus::Warning,
                    "Removed: doesn't sync with your computers".to_string(),
                )
            } else if let Some(refused) = refused {
                (ListItemStatus::Warning, refused)
            } else {
                let last_sync = computer.get("lastSyncMs").and_then(Value::as_u64);
                let synced = last_sync
                    .map(|at| format!("Synced {}", ago(now, at)))
                    .unwrap_or_else(|| "Not synced yet".to_string());
                let connected = flag(&computer, "connected");
                let dot = sync_dot(connected, last_sync, now);
                // A phone carries changes between computers each time it reaches one.
                if flag(&computer, "phone") {
                    (dot, format!("Phone · {synced}"))
                } else if connected {
                    (dot, format!("Connected · {synced}"))
                } else {
                    (dot, format!("Not connected · {synced}"))
                }
            };
            let control = (!this_computer).then(|| {
                let (label, path) = if removed {
                    ("Add back", "/api/restoreSettingsSyncComputer")
                } else {
                    ("Remove", "/api/removeSettingsSyncComputer")
                };
                let id = server_id.clone();
                settings_button(
                    p,
                    SharedString::from(format!("sync-computer-{server_id}")),
                    label,
                    None,
                    ButtonVariant::Outline,
                    disabled,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.call(path, json!({ "serverId": id }), cx)
                    },
                    cx,
                )
            });
            rows.push(settings_list_item(
                p,
                Some(dot),
                None,
                text(&computer, "name"),
                Some(div().child(detail).into_any_element()),
                control,
            ));
        }
        settings_section(
            p,
            "Your computers",
            Some("Every computer connected to this one joins (Remote machines, Easy Connect, Tailscale). Nothing goes through a server, and sign-ins and secrets never sync.".into()),
            None,
            rows,
        )
    }

    fn changes(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let status = self.status.clone()?;
        let now = status.get("nowMs").and_then(Value::as_u64).unwrap_or(0);
        let disabled = self.loading || !self.rpc_available(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for note in list(&status, "notes").into_iter().take(10) {
            let id = text(&note, "id");
            let undone = flag(&note, "undone");
            let at = note.get("atMs").and_then(Value::as_u64).unwrap_or(now);
            let undo_id = id.clone();
            let control = settings_button(
                p,
                SharedString::from(format!("sync-undo-{id}")),
                if undone { "Undone" } else { "Undo" },
                None,
                ButtonVariant::Outline,
                disabled || undone,
                None,
                move |page: &mut Self, _window, cx| {
                    page.call("/api/undoSettingsSyncChange", json!({ "id": undo_id }), cx)
                },
                cx,
            );
            rows.push(settings_list_item(
                p,
                None,
                None,
                text(&note, "text"),
                Some(
                    div()
                        .child(format!(
                            "Changed on {} · {}",
                            text(&note, "keptOn"),
                            ago(now, at)
                        ))
                        .into_any_element(),
                ),
                Some(control),
            ));
        }
        settings_section(
            p,
            "Changed on two computers",
            Some("When two computers changed the same thing while apart, the later change wins on both. Undo puts the other one back everywhere.".into()),
            None,
            rows,
        )
    }
}

impl Render for SyncTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Sync),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Sync,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Sync, &p, blocks, cx);
        }
        let intro = static_note(
            &p,
            "Your computers keep what you pick here the same whenever they are connected to each other, and each works on its own while apart.",
            false,
        );
        let refresh = settings_button(
            &p,
            "sync-reload",
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
        if let Some(error) = self.error.clone() {
            blocks.push(PageBlock::plain(static_note(&p, error, true)));
        }
        let shown = |section: &str| {
            let result = search.section(section);
            !result.is_searching || result.has_visible()
        };
        if shown("whatSyncs")
            && let Some(section) = self.what_syncs(&p, cx)
        {
            blocks.push(PageBlock::section("whatSyncs", section));
        }
        if shown("syncComputers")
            && let Some(section) = self.computers(&p, cx)
        {
            blocks.push(PageBlock::section("syncComputers", section));
        }
        if shown("syncChanges")
            && let Some(section) = self.changes(&p, cx)
        {
            blocks.push(PageBlock::section("syncChanges", section));
        }
        settings_page(&self.store, SettingsTabId::Sync, &p, blocks, cx)
    }
}

/// A category's line under its switch.
fn describe(prefix: &str) -> &'static str {
    match prefix {
        "spaces" => {
            "Spaces and project groups: their names, icons, colours, order and which projects are in them."
        }
        "appearance" => {
            "Theme, colours, fonts, the chat's look and the glass. The glass picture or video, the interface scale and zoom stay on each computer."
        }
        "agents" => {
            "The default agent, how new sessions open, the title agent, Accept all, and each agent's default model and effort."
        }
        "agentRoster" => {
            "Which agents are on in Settings > Agents, their names, commands and flags, and their order. An agent this computer doesn't have shows Not installed here, with Install."
        }
        "skills" => {
            "Your own skills in ~/.agents/skills and the Ghostex skills you installed. Skills over 512 KB stay on each computer."
        }
        "accounts" => {
            "Your Claude and Codex account list: names, numbers, Use this account and the sidebar pins. Logins stay on each computer, because Claude and Codex sign one of two computers out when they share a login."
        }
        "sounds" => {
            "Completion and copy sounds, attention notifications and the finished-work border."
        }
        "keybindings" => {
            "Keyboard shortcuts, between computers on the same system (a Mac with a Mac, Windows with Windows)."
        }
        "coordinator" => "Coordinator settings and your playbook overrides.",
        _ => "",
    }
}

/// How recent a sync still counts as "in touch" for the green dot.
const RECENT_SYNC_MS: u64 = 10 * 60_000;

/// CDXC:SettingsSync 2026-11-07 WHY:
/// One rule colours every device's dot, phones and computers alike: green while the device is in touch with this one (connected now, or synced in the last ten minutes), grey otherwise. Before, only a connected computer could be green, so a phone that had "Synced just now" sat grey beside a green desktop.
fn sync_dot(connected: bool, last_sync: Option<u64>, now: u64) -> ListItemStatus {
    let recent = last_sync.is_some_and(|at| now.saturating_sub(at) < RECENT_SYNC_MS);
    if connected || recent {
        ListItemStatus::Success
    } else {
        ListItemStatus::Neutral
    }
}

fn ago(now: u64, at: u64) -> String {
    let minutes = now.saturating_sub(at) / 60_000;
    match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes} min ago"),
        60..=1439 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1440),
    }
}

fn list(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}
