//! CDXC:AgentLauncher 2026-09-09 DECISION:
//! User: the desktop New Thread picker (Cmd+Shift+T) is drawn natively in GPUI so it opens instantly and is sized to its rows (up to twelve agents plus the Browser and Terminal rows, then it scrolls); the web app keeps the React palette because it is React-based.
//! It mirrors the project-header agent dropdown: every agent with its account count and chat badge, the last-used agent first and preselected, typing filters, Up/Down move, Enter starts, Tab or Right on Claude or Codex opens that provider's account list (Left, Backspace on an empty query, or Esc goes back), Esc closes. The highlighted row uses the sidebar's focused-session chrome and is never bolded.
//! SEE-ALSO: apps/desktop/src/app/new_thread_picker_lifecycle.rs (open, close, data), packages/core-ui/new-thread-palette.tsx (web), packages/core-ui/accounts/agent-launcher-menu.tsx (the dropdown this mirrors).
use crate::app::helpers::*;
use crate::app::titlebar::account_usage::{account_display_name, account_display_text};
use crate::*;
use gpui::{ScrollHandle, SharedString};
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::input::{
    Backspace, Enter, Escape, IndentInline, Input, InputEvent, InputState, MoveDown, MoveLeft,
    MoveRight, MoveUp,
};
use serde_json::{Value, json};

const ROW_TEXT_SIZE: f32 = 13.0;
const ICON_PATH_BROWSER: &str = "titlebar/world.svg";
const ICON_PATH_TERMINAL: &str = "titlebar/terminal-2.svg";
const ICON_PATH_ACCOUNTS: &str = "titlebar/user-circle.svg";
const ICON_PATH_CHAT: &str = "titlebar/message-circle.svg";
const ICON_PATH_SEARCH: &str = "titlebar/search.svg";
const ICON_PATH_BACK: &str = "titlebar/chevron-left.svg";
const ICON_PATH_AGENT_FALLBACK: &str = "titlebar/terminal-2.svg";

/// Search field, key hints, list insets, the divider, and the Browser and
/// Terminal rows, plus one row per agent up to the visible maximum; longer
/// agent lists scroll inside that frame.
pub(crate) fn new_thread_picker_window_height(agent_count: usize) -> f32 {
    NEW_THREAD_PICKER_CHROME_HEIGHT
        + agent_count.min(NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32 * NEW_THREAD_PICKER_ROW_HEIGHT
}

#[derive(Clone, Debug)]
pub(crate) struct NewThreadPickerAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
}

impl NewThreadPickerAgent {
    fn from_hud(value: &Value) -> Option<Self> {
        let agent_id = value.get("agentId")?.as_str()?.trim();
        let name = value.get("name")?.as_str()?.trim();
        if agent_id.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self {
            agent_id: agent_id.to_string(),
            name: name.to_string(),
            icon: value
                .get("icon")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|icon| !icon.is_empty())
                .map(str::to_string),
        })
    }

    fn family(&self) -> &str {
        self.icon.as_deref().unwrap_or(&self.agent_id)
    }

    /// Claude and Codex are the providers with Ghostex-managed accounts.
    pub(crate) fn provider(&self) -> Option<&'static str> {
        match self.family() {
            "claude" => Some("claude"),
            "codex" => Some("codex"),
            _ => None,
        }
    }

    /// Port of `resolveSessionChatTranscriptAgent` in packages/shared/session-chat.ts.
    fn supports_chat(&self) -> bool {
        [self.agent_id.as_str(), self.family()]
            .into_iter()
            .map(|candidate| candidate.trim().to_ascii_lowercase())
            .any(|candidate| {
                matches!(
                    candidate.as_str(),
                    "antigravity"
                        | "antigravity-cli"
                        | "antigravity cli"
                        | "agy"
                        | "claude"
                        | "openclaude"
                        | "codex"
                        | "cursor"
                        | "cursor-agent"
                        | "cursor cli"
                        | "grok"
                        | "grok-build"
                        | "hermes"
                        | "hermes-agent"
                        | "hermes agent"
                        | "pi"
                        | "omp"
                )
            })
    }

    fn icon_path(&self) -> &'static str {
        self.icon
            .as_deref()
            .and_then(workspace_tab_agent_icon_path)
            .unwrap_or(ICON_PATH_AGENT_FALLBACK)
    }

    fn icon_svg_size(&self) -> f32 {
        self.icon
            .as_deref()
            .map(workspace_tab_agent_svg_size)
            .unwrap_or(12.0)
    }

    fn icon_color(&self) -> Hsla {
        match self.icon.as_deref() {
            Some(icon) => rgb(workspace_tab_agent_icon_accent_color(icon)).into(),
            None => rgb(0xffffff).opacity(0.62).into(),
        }
    }
}

/// The sidebar HUD agent buttons in dropdown order, with the last-used agent
/// moved to the front so it is the preselected row.
pub(crate) fn order_new_thread_picker_agents(
    hud_agents: &[Value],
    primary_agent_id: Option<&str>,
) -> Vec<NewThreadPickerAgent> {
    let mut agents: Vec<NewThreadPickerAgent> = hud_agents
        .iter()
        .filter_map(NewThreadPickerAgent::from_hud)
        .collect();
    if let Some(primary_index) = primary_agent_id
        .and_then(|primary| agents.iter().position(|agent| agent.agent_id == primary))
    {
        let primary = agents.remove(primary_index);
        agents.insert(0, primary);
    }
    agents
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerRow {
    Agent(usize),
    Browser,
    Terminal,
    Account(usize),
    CliLogin,
    AddAccount,
    Retry,
}

fn matches_query(text: &str, normalized_query: &str) -> bool {
    if normalized_query.is_empty() {
        return true;
    }
    let haystack = text.to_lowercase();
    if haystack.contains(normalized_query) {
        return true;
    }
    let mut haystack_chars = haystack.chars();
    normalized_query
        .chars()
        .all(|needle| haystack_chars.any(|candidate| candidate == needle))
}

/// Port of `accountUsageLabel` in packages/shared/account-usage-label.ts.
fn usage_window_label(window: &Value) -> Option<String> {
    let seconds = window["limitWindowSeconds"].as_i64().unwrap_or(0);
    let duration = if seconds > 0 {
        if seconds % 86_400 == 0 {
            Some(format!("{}d", seconds / 86_400))
        } else if seconds % 3_600 == 0 {
            Some(format!("{}h", seconds / 3_600))
        } else {
            Some(format!("{}m", seconds / 60))
        }
    } else if window["id"].as_str() == Some("fiveHour") {
        Some("5h".to_string())
    } else if window["id"].as_str() == Some("sevenDay") || window["model"].is_string() {
        Some("7d".to_string())
    } else {
        None
    };
    match duration {
        Some(duration) => Some(match window["model"].as_str() {
            Some(model) => format!("{model} {duration}"),
            None => duration,
        }),
        None => window["label"].as_str().map(str::to_string),
    }
}

/// Port of `AccountLauncherUsage` in packages/core-ui/accounts/agent-launcher-menu.tsx:
/// Claude shows the weekly and five-hour windows, Codex the weekly window and available resets.
fn account_usage_line(account: &Value) -> Option<String> {
    let windows = account["usage"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let main: Vec<&Value> = windows.iter().filter(|w| w["model"].is_null()).collect();
    let weekly = main.iter().copied().find(|w| {
        w["id"].as_str() == Some("sevenDay")
            || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604_800
    });
    let five_hour = main.iter().copied().find(|w| {
        w["id"].as_str() == Some("fiveHour") || w["limitWindowSeconds"].as_i64() == Some(18_000)
    });
    let percent = |w: &Value| -> Option<String> {
        let label = usage_window_label(w)?;
        let used = w["usedPercent"].as_f64()?;
        Some(format!("{label}: {}%", used.round() as i64))
    };
    let values: Vec<String> = if account["provider"].as_str() == Some("claude") {
        [weekly, five_hour]
            .into_iter()
            .flatten()
            .filter_map(percent)
            .collect()
    } else {
        let mut values: Vec<String> = weekly.and_then(percent).into_iter().collect();
        if let Some(resets) = account["resetCredits"].as_u64() {
            values.push(format!("{resets}rs"));
        }
        values
    };
    (!values.is_empty()).then(|| values.join(" · "))
}

pub(crate) struct GpuiNewThreadPickerWindow {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    input: Entity<InputState>,
    agents: Vec<NewThreadPickerAgent>,
    agents_loaded: bool,
    accounts: Option<Value>,
    accounts_error: Option<String>,
    query: String,
    selected: usize,
    scope: Option<usize>,
    scroll: ScrollHandle,
    /// Set once the window has been key; a preloaded hidden window is never
    /// active, so losing activation only closes a window that was shown.
    was_active: bool,
    _subscriptions: Vec<gpui::Subscription>,
}

impl GpuiNewThreadPickerWindow {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        agents: Vec<NewThreadPickerAgent>,
        agents_loaded: bool,
        accounts: Option<Value>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search agents, browser, terminal...")
            });
            let change_subscription = cx.subscribe_in(
                &input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.query = input.read(cx).value().to_string();
                        this.selected = 0;
                        this.scroll.scroll_to_item(0);
                        cx.notify();
                    }
                },
            );
            let activation_subscription =
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.close(window, cx);
                    }
                });
            input.update(cx, |input, cx| input.focus(window, cx));
            Self {
                main_app,
                input,
                agents,
                agents_loaded,
                accounts,
                accounts_error: None,
                query: String::new(),
                selected: 0,
                scope: None,
                scroll: ScrollHandle::new(),
                was_active: false,
                _subscriptions: vec![change_subscription, activation_subscription],
            }
        })
    }

    /// Reuses a preloaded window for a new open: fresh agent order, cached
    /// accounts, empty query, agent list scope, first row selected, input focused.
    pub(crate) fn reset(
        &mut self,
        agents: Vec<NewThreadPickerAgent>,
        agents_loaded: bool,
        accounts: Option<Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents = agents;
        self.agents_loaded = agents_loaded;
        if accounts.is_some() {
            self.accounts = accounts;
            self.accounts_error = None;
        }
        self.scope = None;
        self.selected = 0;
        self.clear_query(window, cx);
        self.scroll.scroll_to_item(0);
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn set_agents(
        &mut self,
        agents: Vec<NewThreadPickerAgent>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let selected_agent_id = self
            .scope
            .or(match self.rows().get(self.selected) {
                Some(PickerRow::Agent(index)) => Some(*index),
                _ => None,
            })
            .and_then(|index| self.agents.get(index))
            .map(|agent| agent.agent_id.clone());
        self.agents = agents;
        self.agents_loaded = true;
        if let Some(scope_agent_id) = self.scope.and_then(|_| selected_agent_id.as_deref()) {
            self.scope = self
                .agents
                .iter()
                .position(|agent| agent.agent_id == scope_agent_id);
        }
        if self.scope.is_none() {
            let rows = self.rows();
            self.selected = selected_agent_id
                .and_then(|agent_id| {
                    rows.iter().position(|row| {
                        matches!(row, PickerRow::Agent(index) if self.agents[*index].agent_id == agent_id)
                    })
                })
                .unwrap_or(0);
        }
        window.resize(size(
            px(NEW_THREAD_PICKER_WIDTH),
            px(new_thread_picker_window_height(self.agents.len())),
        ));
        cx.notify();
    }

    pub(crate) fn set_accounts(
        &mut self,
        accounts: Result<Value, String>,
        cx: &mut gpui::Context<Self>,
    ) {
        match accounts {
            Ok(state) => {
                self.accounts = Some(state);
                self.accounts_error = None;
            }
            Err(error) => {
                if self.accounts.is_none() {
                    self.accounts_error = Some(error);
                }
            }
        }
        self.selected = self.selected.min(self.rows().len().saturating_sub(1));
        cx.notify();
    }

    fn normalized_query(&self) -> String {
        self.query.trim().to_lowercase()
    }

    fn scope_agent(&self) -> Option<&NewThreadPickerAgent> {
        self.scope.and_then(|index| self.agents.get(index))
    }

    fn provider_accounts(&self, provider: &str) -> Vec<usize> {
        self.accounts
            .as_ref()
            .and_then(|state| state["accounts"].as_array())
            .map(|accounts| {
                accounts
                    .iter()
                    .enumerate()
                    .filter(|(_, account)| {
                        account["registered"].as_bool() == Some(true)
                            && account["provider"].as_str() == Some(provider)
                    })
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn provider_account_count(&self, provider: &str) -> Option<usize> {
        self.accounts
            .as_ref()
            .map(|_| self.provider_accounts(provider).len())
    }

    fn account(&self, index: usize) -> Option<&Value> {
        self.accounts
            .as_ref()
            .and_then(|state| state["accounts"].as_array())
            .and_then(|accounts| accounts.get(index))
    }

    fn rows(&self) -> Vec<PickerRow> {
        let query = self.normalized_query();
        match self.scope_agent() {
            None => {
                let mut rows: Vec<PickerRow> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, agent)| matches_query(&agent.name, &query))
                    .map(|(index, _)| PickerRow::Agent(index))
                    .collect();
                if matches_query("Browser", &query) {
                    rows.push(PickerRow::Browser);
                }
                if matches_query("Terminal", &query) {
                    rows.push(PickerRow::Terminal);
                }
                rows
            }
            Some(agent) => {
                let Some(provider) = agent.provider() else {
                    return Vec::new();
                };
                if self.accounts.is_none() {
                    return if self.accounts_error.is_some() {
                        vec![PickerRow::Retry]
                    } else {
                        Vec::new()
                    };
                }
                let accounts = self.provider_accounts(provider);
                if accounts.is_empty() {
                    return vec![PickerRow::CliLogin, PickerRow::AddAccount];
                }
                accounts
                    .into_iter()
                    .filter(|index| {
                        self.account(*index)
                            .and_then(|account| account["name"].as_str())
                            .is_some_and(|name| matches_query(name, &query))
                    })
                    .map(PickerRow::Account)
                    .collect()
            }
        }
    }

    fn selected_row(&self) -> Option<PickerRow> {
        self.rows().get(self.selected).copied()
    }

    /// Child index inside the scroll container, which also holds the divider
    /// between the agents and the Browser and Terminal rows.
    fn scroll_child_index(&self, rows: &[PickerRow], selected: usize) -> usize {
        let agent_rows = rows
            .iter()
            .filter(|row| matches!(row, PickerRow::Agent(_)))
            .count();
        if self.scope.is_none() && agent_rows > 0 && selected >= agent_rows {
            selected + 1
        } else {
            selected
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let count = rows.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(count) as usize;
        self.selected = next;
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, next));
        cx.notify();
    }

    fn clear_query(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.query.clear();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn enter_scope(
        &mut self,
        agent_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .agents
            .get(agent_index)
            .and_then(NewThreadPickerAgent::provider)
            .is_none()
        {
            return;
        }
        self.scope = Some(agent_index);
        self.selected = 0;
        self.clear_query(window, cx);
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn leave_scope(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(agent_index) = self.scope.take() else {
            return;
        };
        self.clear_query(window, cx);
        let rows = self.rows();
        self.selected = rows
            .iter()
            .position(|row| *row == PickerRow::Agent(agent_index))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, self.selected));
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.was_active = false;
        let _ = self.main_app.update(cx, |app, cx| {
            app.release_gpui_new_thread_picker_window(cx);
        });
        window.remove_window();
    }

    fn launch_agent(
        &mut self,
        agent: &NewThreadPickerAgent,
        account_id: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut message = json!({
            "agentId": agent.agent_id,
            "type": "runSidebarAgent",
        });
        if let Some(account_id) = account_id {
            message["accountId"] = json!(account_id);
        }
        let agent_id = agent.agent_id.clone();
        let _ = self.main_app.update(cx, |app, cx| {
            app.sidebar_primary_agent_launcher_id = Some(agent_id);
            app.dispatch_gpui_sidebar_host_message(message, cx);
        });
        self.close(window, cx);
    }

    fn dispatch_sidebar_and_close(
        &mut self,
        message: Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.dispatch_gpui_sidebar_host_message(message, cx);
        });
        self.close(window, cx);
    }

    fn activate(&mut self, row: PickerRow, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match row {
            PickerRow::Agent(index) => {
                if let Some(agent) = self.agents.get(index).cloned() {
                    self.launch_agent(&agent, None, window, cx);
                }
            }
            PickerRow::Account(index) => {
                let Some(agent) = self.scope_agent().cloned() else {
                    return;
                };
                let Some(account) = self.account(index) else {
                    return;
                };
                if account["status"].as_str() != Some("ready") {
                    return;
                }
                let account_id = account["id"].as_str().map(str::to_string);
                self.launch_agent(&agent, account_id, window, cx);
            }
            PickerRow::CliLogin => {
                if let Some(agent) = self.scope_agent().cloned() {
                    self.launch_agent(&agent, None, window, cx);
                }
            }
            PickerRow::AddAccount => {
                let _ = self.main_app.update(cx, |app, cx| {
                    app.open_gpui_settings_accounts_from_new_thread_picker(cx);
                });
                self.close(window, cx);
            }
            PickerRow::Retry => {
                self.accounts_error = None;
                let _ = self.main_app.update(cx, |app, cx| {
                    app.refresh_gpui_new_thread_picker_accounts(cx);
                });
                cx.notify();
            }
            PickerRow::Browser => {
                self.dispatch_sidebar_and_close(
                    json!({ "type": "openBrowserPaneInGroup" }),
                    window,
                    cx,
                );
            }
            PickerRow::Terminal => {
                self.dispatch_sidebar_and_close(json!({ "type": "createSession" }), window, cx);
            }
        }
    }

    /*
    CDXC:AgentLauncher 2026-09-09 WHY:
    gpui dispatches key bindings before key-down listeners, and the focused
    search field binds Up, Down, Tab, Left, Right, and Backspace to its own
    input actions, which swallow the keystroke in single-line mode. The picker
    therefore captures those actions on its root (capture phase runs before
    the input's own handlers) instead of watching raw key events; Left, Right,
    and Backspace propagate to the field whenever the picker has no use for
    them, so cursor editing still works.
    */
    fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.move_selection(-1, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.move_selection(1, cx);
    }

    fn on_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(row) = self.selected_row() {
            self.activate(row, window, cx);
        }
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.scope.is_some() {
            self.leave_scope(window, cx);
        } else {
            self.close(window, cx);
        }
    }

    /// Tab: open the highlighted Claude or Codex agent's account list.
    fn on_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.scope.is_none() {
            if let Some(PickerRow::Agent(index)) = self.selected_row() {
                self.enter_scope(index, window, cx);
            }
        }
    }

    fn on_move_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.scope.is_none() {
            if let Some(PickerRow::Agent(index)) = self.selected_row() {
                if self.agents[index].provider().is_some() {
                    self.enter_scope(index, window, cx);
                    return;
                }
            }
        }
        cx.propagate();
    }

    fn on_move_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.scope.is_some() {
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.scope.is_some() && self.query.is_empty() {
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }

    fn muted_text_color() -> Hsla {
        rgb(0xffffff).opacity(0.50).into()
    }

    fn render_kbd(label: &'static str) -> impl IntoElement {
        div()
            .flex()
            .h(px(16.0))
            .min_w(px(16.0))
            .px(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(rgb(0xffffff).opacity(0.08))
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.14))
            .text_size(px(10.0))
            .line_height(px(14.0))
            .child(label)
    }

    fn render_hint(keys: &[&'static str], label: &'static str) -> impl IntoElement {
        h_flex()
            .items_center()
            .gap(px(4.0))
            .children(keys.iter().map(|key| Self::render_kbd(key)))
            .child(label)
    }

    fn render_hints(&self) -> impl IntoElement {
        let in_accounts = self.scope.is_some();
        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(6.0))
            .pb(px(4.0))
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(Self::muted_text_color())
            .child(Self::render_hint(&["↑", "↓"], "Move"))
            .child(Self::render_hint(&["↵"], "Start"))
            .child(if in_accounts {
                Self::render_hint(&["←"], "Back")
            } else {
                Self::render_hint(&["⇥"], "Accounts")
            })
            .child(Self::render_hint(
                &["esc"],
                if in_accounts { "Back" } else { "Close" },
            ))
    }

    fn render_search(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let scope_chip = self.scope_agent().map(|agent| {
            let icon_path = agent.icon_path();
            let icon_size = agent.icon_svg_size();
            let icon_color = agent.icon_color();
            h_flex()
                .id("ghostex-gpui-new-thread-picker-scope")
                .flex_shrink_0()
                .h(px(22.0))
                .items_center()
                .gap(px(5.0))
                .pl(px(4.0))
                .pr(px(7.0))
                .rounded(px(4.0))
                .bg(rgb(0xffffff).opacity(0.08))
                .hover(|this| this.bg(rgb(0xffffff).opacity(0.14)))
                .text_size(px(12.0))
                .text_color(titlebar_text_color())
                .child(
                    svg()
                        .path(ICON_PATH_BACK)
                        .size(px(12.0))
                        .text_color(Self::muted_text_color()),
                )
                .child(
                    div()
                        .flex()
                        .size(px(14.0))
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path(icon_path)
                                .size(px(icon_size))
                                .text_color(icon_color),
                        ),
                )
                .child(agent.name.clone())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.leave_scope(window, cx);
                }))
        });
        h_flex()
            .flex_shrink_0()
            .h(px(NEW_THREAD_PICKER_SEARCH_HEIGHT))
            .mx(px(6.0))
            .mt(px(6.0))
            .px(px(10.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0xffffff).opacity(0.10))
            .bg(rgb(0xffffff).opacity(0.05))
            .children(scope_chip)
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.input)
                        .with_size(ComponentSize::XSmall)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(ROW_TEXT_SIZE))
                        .text_color(titlebar_text_color()),
                ),
            )
            .child(
                svg()
                    .path(ICON_PATH_SEARCH)
                    .size(px(14.0))
                    .text_color(Self::muted_text_color()),
            )
    }

    fn row_shell(
        &self,
        id: impl Into<ElementId>,
        row: PickerRow,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .flex_shrink_0()
            .w_full()
            .min_h(px(NEW_THREAD_PICKER_ROW_HEIGHT))
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(ROW_TEXT_SIZE))
            .text_color(titlebar_text_color())
            .when(selected, |this| {
                this.bg(rgb(0x141414))
                    .border_color(rgb(0xffffff).opacity(0.05))
                    .text_color(rgb(0xd8d8d8))
            })
            .when(!selected, |this| {
                this.hover(|this| this.bg(rgb(0xffffff).opacity(0.05)))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate(row, window, cx);
            }))
    }

    fn render_agent_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let agent = &self.agents[index];
        let provider = agent.provider();
        let account_count = provider.and_then(|provider| self.provider_account_count(provider));
        self.row_shell(
            ElementId::Name(format!("new-thread-agent-{}", agent.agent_id).into()),
            PickerRow::Agent(index),
            selected,
            cx,
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .size(px(14.0))
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(agent.icon_path())
                        .size(px(agent.icon_svg_size()))
                        .text_color(agent.icon_color()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(agent.name.clone()),
        )
        .when(provider.is_some(), |this| {
            this.child(
                h_flex()
                    .id(ElementId::Name(
                        format!("new-thread-accounts-{}", agent.agent_id).into(),
                    ))
                    .flex_shrink_0()
                    .h(px(24.0))
                    .min_w(px(28.0))
                    .items_center()
                    .justify_center()
                    .gap(px(3.0))
                    .px(px(4.0))
                    .rounded(px(5.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xffffff).opacity(0.5))
                    .hover(|this| {
                        this.bg(rgb(0xffffff).opacity(0.10))
                            .text_color(rgb(0xffffff).opacity(0.8))
                    })
                    .child(
                        svg()
                            .path(ICON_PATH_ACCOUNTS)
                            .size(px(14.0))
                            .text_color(gpui::Hsla::from(rgb(0xffffff)).opacity(0.5)),
                    )
                    .children(account_count.map(|count| count.to_string()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.enter_scope(index, window, cx);
                    })),
            )
        })
        .when(agent.supports_chat(), |this| {
            this.child(
                div().flex().flex_shrink_0().ml(px(6.0)).mr(px(5.0)).child(
                    svg()
                        .path(ICON_PATH_CHAT)
                        .size(px(14.0))
                        .text_color(gpui::Hsla::from(rgb(0xffffff)).opacity(0.5)),
                ),
            )
        })
        .into_any_element()
    }

    fn render_plain_row(
        &self,
        id: &'static str,
        row: PickerRow,
        icon_path: &'static str,
        label: &'static str,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.row_shell(id, row, selected, cx)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .size(px(14.0))
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path(icon_path)
                            .size(px(14.0))
                            .text_color(gpui::Hsla::from(rgb(0xffffff)).opacity(0.62)),
                    ),
            )
            .child(div().flex_1().min_w_0().child(label))
            .into_any_element()
    }

    fn render_account_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(account) = self.account(index) else {
            return div().into_any_element();
        };
        let provider = account["provider"].as_str().unwrap_or("claude");
        let ready = account["status"].as_str() == Some("ready");
        let is_default = self
            .accounts
            .as_ref()
            .and_then(|state| state["defaultAccounts"][provider].as_str())
            .is_some_and(|default_id| Some(default_id) == account["id"].as_str());
        let display_name = account_display_name(account);
        let usage = account_usage_line(account);
        let icon_path = workspace_tab_agent_icon_path(provider).unwrap_or(ICON_PATH_AGENT_FALLBACK);
        let icon_color: Hsla = rgb(workspace_tab_agent_icon_accent_color(provider)).into();
        self.row_shell(
            ElementId::Name(format!("new-thread-account-{index}").into()),
            PickerRow::Account(index),
            selected,
            cx,
        )
        .py(px(6.0))
        .when(!ready, |this| this.opacity(0.42))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .size(px(16.0))
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(icon_path)
                        .size(px(workspace_tab_agent_svg_size(provider) + 1.5))
                        .text_color(icon_color),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(display_name),
                        )
                        .when(is_default, |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(Self::muted_text_color())
                                    .child("· Default"),
                            )
                        }),
                )
                .children(usage.map(|usage| {
                    div()
                        .font_family(ACCOUNT_INDICATOR_FONT_FAMILY)
                        .text_size(px(10.5))
                        .line_height(px(13.0))
                        .text_color(Self::muted_text_color())
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(usage)
                })),
        )
        .into_any_element()
    }

    fn render_hint_text(text: impl Into<SharedString>) -> AnyElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(Self::muted_text_color())
            .child(text.into())
            .into_any_element()
    }

    fn render_divider() -> AnyElement {
        div()
            .flex_shrink_0()
            .h(px(1.0))
            .mx(px(4.0))
            .my(px(4.0))
            .bg(rgb(0xffffff).opacity(0.10))
            .into_any_element()
    }

    fn render_list(&self, cx: &mut gpui::Context<Self>) -> Vec<AnyElement> {
        let rows = self.rows();
        let mut children: Vec<AnyElement> = Vec::new();
        match self.scope_agent() {
            None => {
                let agent_rows = rows
                    .iter()
                    .filter(|row| matches!(row, PickerRow::Agent(_)))
                    .count();
                for (position, row) in rows.iter().enumerate() {
                    let selected = position == self.selected;
                    if agent_rows > 0 && position == agent_rows {
                        children.push(Self::render_divider());
                    }
                    children.push(match *row {
                        PickerRow::Agent(index) => self.render_agent_row(index, selected, cx),
                        PickerRow::Browser => self.render_plain_row(
                            "new-thread-browser",
                            PickerRow::Browser,
                            ICON_PATH_BROWSER,
                            "Browser",
                            selected,
                            cx,
                        ),
                        PickerRow::Terminal => self.render_plain_row(
                            "new-thread-terminal",
                            PickerRow::Terminal,
                            ICON_PATH_TERMINAL,
                            "Terminal",
                            selected,
                            cx,
                        ),
                        _ => continue,
                    });
                }
                if rows.is_empty() {
                    children.push(Self::render_hint_text(if self.agents_loaded {
                        "Nothing matches."
                    } else {
                        "Loading agents…"
                    }));
                }
            }
            Some(agent) => {
                let agent = agent.clone();
                if self.accounts.is_none() && self.accounts_error.is_none() {
                    children.push(Self::render_hint_text("Reading accounts…"));
                }
                if let Some(error) = &self.accounts_error {
                    if self.accounts.is_none() {
                        children.push(Self::render_hint_text(account_display_text(error)));
                    }
                }
                for (position, row) in rows.iter().enumerate() {
                    let selected = position == self.selected;
                    match *row {
                        PickerRow::Account(index) => {
                            children.push(self.render_account_row(index, selected, cx));
                        }
                        PickerRow::CliLogin => {
                            children.push(
                                self.row_shell(
                                    "new-thread-cli-login",
                                    PickerRow::CliLogin,
                                    selected,
                                    cx,
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_shrink_0()
                                        .size(px(14.0))
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            svg()
                                                .path(agent.icon_path())
                                                .size(px(agent.icon_svg_size()))
                                                .text_color(agent.icon_color()),
                                        ),
                                )
                                .child(div().flex_1().min_w_0().child("Current CLI login"))
                                .into_any_element(),
                            );
                            children.push(Self::render_hint_text(
                                "Uses your existing CLI sign-in. No account switcher needed.",
                            ));
                            children.push(Self::render_divider());
                            children.push(Self::render_hint_text(
                                "Add your account to see usage and reset times in Ghostex.",
                            ));
                        }
                        PickerRow::AddAccount => {
                            children.push(
                                self.row_shell(
                                    "new-thread-add-account",
                                    PickerRow::AddAccount,
                                    selected,
                                    cx,
                                )
                                .child(div().flex_1().min_w_0().child("Add account"))
                                .into_any_element(),
                            );
                        }
                        PickerRow::Retry => {
                            children.push(
                                self.row_shell(
                                    "new-thread-retry-accounts",
                                    PickerRow::Retry,
                                    selected,
                                    cx,
                                )
                                .child(div().flex_1().min_w_0().child("Try again"))
                                .into_any_element(),
                            );
                        }
                        _ => {}
                    }
                }
                if self.accounts.is_some()
                    && rows.is_empty()
                    && agent
                        .provider()
                        .is_some_and(|provider| !self.provider_accounts(provider).is_empty())
                {
                    children.push(Self::render_hint_text("No accounts found."));
                }
            }
        }
        children
    }
}

impl Render for GpuiNewThreadPickerWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let list_children = self.render_list(cx);
        v_flex()
            .id("ghostex-gpui-new-thread-picker")
            .size_full()
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(titlebar_popup_menu_border_color())
            .bg(titlebar_popup_menu_background())
            .font_family("Inter Variable")
            .text_color(titlebar_text_color())
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_move_right))
            .capture_action(cx.listener(Self::on_move_left))
            .capture_action(cx.listener(Self::on_backspace))
            .on_action(
                cx.listener(|this, action: &RunConfiguredGhostexHotkey, window, cx| {
                    if action.action_id == "openNewThreadPalette" {
                        this.close(window, cx);
                    }
                }),
            )
            .child(self.render_search(cx))
            .child(self.render_hints())
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(
                        v_flex()
                            .id("ghostex-gpui-new-thread-picker-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .px(px(6.0))
                            .pt(px(2.0))
                            .pb(px(6.0))
                            .children(list_children),
                    )
                    .child(
                        Scrollbar::vertical(&self.scroll)
                            .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                    ),
            )
    }
}
