//! Native GPUI New Thread picker (Cmd+Shift+T). Its React twin, `NewThreadPalette` in
//! packages/core-ui/new-thread-palette.tsx, was deleted with the React web app on 2026-09-24.
//!
//! CDXC:AgentLauncher 2026-09-09 DECISION:
//! User: the desktop New Thread picker (Cmd+Shift+T) is drawn natively in GPUI so it opens instantly and is sized to its rows (up to twelve agents plus the Browser and Terminal rows, then it scrolls).
//! It mirrors the project-header agent dropdown: every agent with its account count and chat badge, the last-used agent first and preselected, typing filters, Up/Down move, Enter starts, Tab or Right on Claude or Codex opens that provider's account list (Left, Backspace on an empty query, or Esc goes back), Esc closes. The highlighted row uses the sidebar's focused-session chrome and is never bolded.
//!
//! CDXC:AppModal 2026-09-16 DECISION:
//! User: "please fix this modal, also please ensure that we use the gpui components that we created in the gpui app and we're not using the older modals": the picker takes its colours from the shared native modal kit palette in both appearances. It used dark-only white tints that vanished on the light theme (search border, key hints, divider, white agent logos) while the highlighted row stayed black.
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/agent_launcher_menu.rs (the dropdown this mirrors), apps/desktop/src/app/window/native_modal_kit.rs (the shared palette), apps/desktop/src/app/new_thread_picker_lifecycle.rs (open, close, preload, data), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
//!
//! This module depends only on the kit, gpui and gpui-component so the preview binary can include it with `#[path]`.
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, Div, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Rgba, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px, rgb, size,
};
use gpui_component::input::{
    Backspace, Enter, Escape, IndentInline, Input, InputEvent, InputState, MoveDown, MoveLeft,
    MoveRight, MoveUp,
};
use gpui_component::scroll::Scrollbar;
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::rc::Rc;

/*
CDXC:AgentLauncher 2026-09-09 DECISION:
User: the native New Thread picker is sized to its rows: the search field, the key-hint row, one row per agent up to twelve, the divider, and the Browser and Terminal rows; more agents scroll. The chrome height is the 6px top inset, 36px search field, 32px hint row, 2px list inset, 9px divider, two 36px rows, the 6px bottom inset, and the 2px frame border.
*/
pub(crate) const NEW_THREAD_PICKER_WIDTH: f32 = 420.0;
pub(crate) const NEW_THREAD_PICKER_SEARCH_HEIGHT: f32 = 36.0;
pub(crate) const NEW_THREAD_PICKER_ROW_HEIGHT: f32 = 36.0;
pub(crate) const NEW_THREAD_PICKER_MAX_AGENT_ROWS: usize = 12;
pub(crate) const NEW_THREAD_PICKER_CHROME_HEIGHT: f32 = 165.0;

/// `.quick-access-surface`: Inter at 14px over 20px.
const PICKER_FONT: &str = "Inter Variable";
const ROW_TEXT_SIZE: f32 = 14.0;
const ROW_LINE_HEIGHT: f32 = 20.0;
const SCROLLBAR_WIDTH: f32 = 5.0;

const PLACEHOLDER_AGENTS: &str = "Search agents, browser, terminal...";
const PLACEHOLDER_ACCOUNTS: &str = "Search accounts...";
const HINT_MOVE: &str = "Move";
const HINT_START: &str = "Start";
const HINT_BACK: &str = "Back";
const HINT_ACCOUNTS: &str = "Accounts";
const HINT_CLOSE: &str = "Close";
const ROW_BROWSER: &str = "Browser";
const ROW_TERMINAL: &str = "Terminal";
const ROW_CLI_LOGIN: &str = "Current CLI login";
const ROW_ADD_ACCOUNT: &str = "Add account";
const ROW_TRY_AGAIN: &str = "Try again";
const DEFAULT_SUFFIX: &str = "· Default";
const EMPTY_AGENTS: &str = "Nothing matches.";
const EMPTY_ACCOUNTS: &str = "No accounts found.";
const LOADING_AGENTS: &str = "Loading agents…";
const READING_ACCOUNTS: &str = "Reading accounts…";
const CLI_LOGIN_HINT: &str = "Uses your existing CLI sign-in. No account switcher needed.";
const ADD_ACCOUNT_HINT: &str = "Add your account to see usage and reset times in Ghostex.";

const ICON_CODE: &str = "modals/new-thread-picker/code.svg";
const ICON_ACCOUNTS: &str = "modals/new-thread-picker/user.svg";
const ICON_CHAT: &str = "modals/new-thread-picker/message-circle.svg";
const ICON_BROWSER: &str = "modals/new-thread-picker/world.svg";
const ICON_TERMINAL: &str = "modals/new-thread-picker/terminal-2.svg";
const ICON_BACK: &str = "modals/new-thread-picker/chevron-left.svg";
const ICON_SEARCH: &str = "modals/new-thread-picker/search.svg";
const ICON_CLEAR: &str = "modals/new-thread-picker/x.svg";

/// Search field, key hints, list insets, the divider, and the Browser and
/// Terminal rows, plus one row per agent up to the visible maximum; longer
/// agent lists scroll inside that frame.
pub(crate) fn new_thread_picker_window_height(agent_count: usize) -> f32 {
    NEW_THREAD_PICKER_CHROME_HEIGHT
        + agent_count.min(NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32 * NEW_THREAD_PICKER_ROW_HEIGHT
}

/// The `.quick-access-surface` tokens the React palette layers over the app
/// theme, resolved from the kit palette.
#[derive(Clone, Copy)]
struct PickerColors {
    /// `--app-dropdown-background`.
    surface: Rgba,
    /// The titlebar popup chrome every floating native menu uses (#3f3f3f dark, #d4d4d4 light).
    frame_border: Rgba,
    /// `--quick-access-item-color`: #b4b8bf dark, #404040 light.
    item: Rgba,
    /// `--app-foreground` on the surface: #b4b8bf dark, #262626 light.
    foreground: Rgba,
    /// `--app-muted`.
    muted: Rgba,
    /// `.group-agent-menu-label`: foreground 86%, muted 14%.
    label: Rgba,
    /// `.group-agent-launcher-icon`, `.new-thread-palette-glyph`: foreground 62%, muted 38%.
    glyph: Rgba,
    /// `.new-thread-palette-row[data-selected='true']`: the composer fill in the dark
    /// sidebar, the active card fill in the light one.
    selected_background: Rgba,
    selected_ring: Rgba,
    selected_label: Rgba,
    /// `[data-slot='command-input-wrapper'] [data-slot='input-group']` on the surface.
    input_background: Rgba,
    /// `--input`.
    input_border: Rgba,
    /// `[data-slot='command-separator']`: `--app-border` at 70%.
    divider: Rgba,
    light: bool,
}

impl PickerColors {
    fn resolve(p: &ModalPalette) -> Self {
        if p.light {
            let foreground = rgb(0x262626);
            Self {
                surface: p.surface,
                frame_border: p.hairline,
                item: rgb(0x404040),
                foreground,
                muted: p.muted,
                label: css_mix(foreground, 0.86, p.muted),
                glyph: css_mix(foreground, 0.62, p.muted),
                selected_background: modal_rgba(0x000000, 0.05),
                selected_ring: modal_rgba(0x000000, 0.06),
                selected_label: foreground,
                input_background: p.panel,
                input_border: modal_rgba(0x000000, 0.12),
                divider: modal_rgba(0x000000, 0.12 * 0.7),
                light: true,
            }
        } else {
            let foreground = rgb(0xb4b8bf);
            Self {
                surface: p.surface,
                frame_border: modal_rgba(0xffffff, 0.12),
                item: foreground,
                foreground,
                muted: p.muted,
                label: css_mix(foreground, 0.86, p.muted),
                glyph: css_mix(foreground, 0.62, p.muted),
                selected_background: modal_rgba(0xffffff, 0.06),
                selected_ring: modal_rgba(0xffffff, 0.05),
                selected_label: rgb(0xd8d8d8),
                input_background: modal_rgba(0xffffff, 0.04),
                input_border: modal_rgba(0xffffff, 0.15),
                divider: modal_rgba(0xffffff, 0.10 * 0.7),
                light: false,
            }
        }
    }

    /// `getBrandAgentLogoStyle`: white and near-white brand marks take
    /// `--ghostex-light-icon-color` (the foreground) on the light theme.
    fn brand(&self, accent: u32) -> Rgba {
        if self.light && matches!(accent, 0xffffff | 0xedecec) {
            self.foreground
        } else {
            rgb(accent)
        }
    }
}

/// One sidebar HUD agent button, with its logo resolved by the host.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewThreadPickerAgent {
    pub(crate) agent_id: String,
    pub(crate) name: String,
    /// The HUD icon id (`claude`, `codex`, ...), which is also the provider family.
    pub(crate) icon: Option<String>,
    /// The logo asset, its rendered size, and its brand colour; `None` draws the generic code glyph.
    pub(crate) icon_path: Option<&'static str>,
    pub(crate) icon_svg_size: f32,
    pub(crate) icon_accent: u32,
}

impl NewThreadPickerAgent {
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
}

/// One registered Claude or Codex account, already masked and summarised by the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewThreadPickerAccount {
    pub(crate) id: String,
    pub(crate) provider: String,
    pub(crate) name: String,
    /// `AccountLauncherUsage`: the usage figures line, when the account has any.
    pub(crate) usage: Option<String>,
    pub(crate) ready: bool,
    pub(crate) is_default: bool,
}

pub(crate) struct NewThreadPickerConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) agents: Vec<NewThreadPickerAgent>,
    pub(crate) agents_loaded: bool,
    /// `None` until the accounts list has been read once.
    pub(crate) accounts: Option<Vec<NewThreadPickerAccount>>,
    /// The app closes the picker when its window stops being key; the preview keeps it open.
    pub(crate) close_when_inactive: bool,
}

/// What the picker asks its host to do. The picker removes its own window
/// before sending any command except `RetryAccounts`.
pub(crate) enum NewThreadPickerCommand {
    /// `runSidebarAgent { agentId, accountId? }`.
    LaunchAgent {
        agent_id: String,
        account_id: Option<String>,
    },
    /// `openBrowserPaneInGroup`.
    OpenBrowser,
    /// `createSession`.
    CreateTerminal,
    /// Settings on its Accounts page.
    AddAccount,
    /// Read the accounts list again after a failure.
    RetryAccounts,
    /// Escape, or the window lost activation.
    Closed,
}

pub(crate) type NewThreadPickerHost = Rc<dyn Fn(NewThreadPickerCommand, &mut App)>;

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

pub(crate) struct GpuiNewThreadPickerWindow {
    host: NewThreadPickerHost,
    colors: PickerColors,
    /// Set by the app when the picker's window is blurred under window glass.
    pub(crate) glass: bool,
    /// The app's frosted menu fill for this picker's surface, set with `glass`.
    pub(crate) frosted_fill: Option<gpui::Hsla>,
    input: Entity<InputState>,
    agents: Vec<NewThreadPickerAgent>,
    agents_loaded: bool,
    accounts: Option<Vec<NewThreadPickerAccount>>,
    accounts_error: Option<String>,
    query: String,
    selected: usize,
    scope: Option<usize>,
    scroll: ScrollHandle,
    /// Set once the window has been key; a preloaded hidden window is never
    /// active, so losing activation only closes a window that was shown.
    was_active: bool,
    close_when_inactive: bool,
    _subscriptions: Vec<Subscription>,
}

impl GpuiNewThreadPickerWindow {
    /// The picker's own surface colour, which the app thins into its frosted menu fill.
    pub(crate) fn surface_color(&self) -> gpui::Hsla {
        hsla(self.colors.surface)
    }

    pub(crate) fn new(
        config: NewThreadPickerConfig,
        host: NewThreadPickerHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER_AGENTS));
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
        let activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            // CDXC:AgentLauncher 2026-09-11 WHY:
            // A hidden preload has never owned activation, so an inactive notification must not close it and create another preload.
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active && this.close_when_inactive {
                this.close(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            glass: false,
            frosted_fill: None,
            host,
            colors: PickerColors::resolve(&config.palette),
            input,
            agents: config.agents,
            agents_loaded: config.agents_loaded,
            accounts: config.accounts,
            accounts_error: None,
            query: String::new(),
            selected: 0,
            scope: None,
            scroll: ScrollHandle::new(),
            was_active: window.is_window_active(),
            close_when_inactive: config.close_when_inactive,
            _subscriptions: vec![change_subscription, activation_subscription],
        }
    }

    /// Reuses a preloaded window for a new open: current palette, fresh agent
    /// order, cached accounts, empty query, agent list scope, first row
    /// selected, input focused.
    pub(crate) fn reset(
        &mut self,
        config: NewThreadPickerConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.colors = PickerColors::resolve(&config.palette);
        self.agents = config.agents;
        self.agents_loaded = config.agents_loaded;
        if config.accounts.is_some() {
            self.accounts = config.accounts;
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
        cx: &mut Context<Self>,
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
        accounts: Result<Vec<NewThreadPickerAccount>, String>,
        cx: &mut Context<Self>,
    ) {
        match accounts {
            Ok(accounts) => {
                self.accounts = Some(accounts);
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

    /// Preview hook: opens the account list of the agent at `agent_index`.
    #[allow(dead_code)]
    pub(crate) fn preview_enter_accounts(
        &mut self,
        agent_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.enter_scope(agent_index, window, cx);
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
            .map(|accounts| {
                accounts
                    .iter()
                    .enumerate()
                    .filter(|(_, account)| account.provider == provider)
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

    fn account(&self, index: usize) -> Option<&NewThreadPickerAccount> {
        self.accounts
            .as_ref()
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
                if matches_query(ROW_BROWSER, &query) {
                    rows.push(PickerRow::Browser);
                }
                if matches_query(ROW_TERMINAL, &query) {
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
                            .is_some_and(|account| matches_query(&account.name, &query))
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

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
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

    fn clear_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn set_placeholder(
        &self,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });
    }

    fn enter_scope(&mut self, agent_index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
        self.set_placeholder(PLACEHOLDER_ACCOUNTS, window, cx);
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn leave_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent_index) = self.scope.take() else {
            return;
        };
        self.clear_query(window, cx);
        self.set_placeholder(PLACEHOLDER_AGENTS, window, cx);
        let rows = self.rows();
        self.selected = rows
            .iter()
            .position(|row| *row == PickerRow::Agent(agent_index))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, self.selected));
        cx.notify();
    }

    fn remove_window(&mut self, window: &mut Window) {
        self.was_active = false;
        window.remove_window();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remove_window(window);
        (self.host)(NewThreadPickerCommand::Closed, cx);
    }

    fn finish(
        &mut self,
        command: NewThreadPickerCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_window(window);
        (self.host)(command, cx);
    }

    fn activate(&mut self, row: PickerRow, window: &mut Window, cx: &mut Context<Self>) {
        match row {
            PickerRow::Agent(index) => {
                if let Some(agent) = self.agents.get(index) {
                    let agent_id = agent.agent_id.clone();
                    self.finish(
                        NewThreadPickerCommand::LaunchAgent {
                            agent_id,
                            account_id: None,
                        },
                        window,
                        cx,
                    );
                }
            }
            PickerRow::Account(index) => {
                let Some(agent_id) = self.scope_agent().map(|agent| agent.agent_id.clone()) else {
                    return;
                };
                let Some(account) = self.account(index) else {
                    return;
                };
                if !account.ready {
                    return;
                }
                let account_id = Some(account.id.clone());
                self.finish(
                    NewThreadPickerCommand::LaunchAgent {
                        agent_id,
                        account_id,
                    },
                    window,
                    cx,
                );
            }
            PickerRow::CliLogin => {
                if let Some(agent_id) = self.scope_agent().map(|agent| agent.agent_id.clone()) {
                    self.finish(
                        NewThreadPickerCommand::LaunchAgent {
                            agent_id,
                            account_id: None,
                        },
                        window,
                        cx,
                    );
                }
            }
            PickerRow::AddAccount => {
                self.finish(NewThreadPickerCommand::AddAccount, window, cx);
            }
            PickerRow::Retry => {
                self.accounts_error = None;
                (self.host)(NewThreadPickerCommand::RetryAccounts, cx);
                cx.notify();
            }
            PickerRow::Browser => {
                self.finish(NewThreadPickerCommand::OpenBrowser, window, cx);
            }
            PickerRow::Terminal => {
                self.finish(NewThreadPickerCommand::CreateTerminal, window, cx);
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
    /*
    CDXC:AgentLauncher 2026-09-24 WHY:
    A capture-phase action keeps propagating unless its listener stops it, and the single-line field registers no Up, Down, or Tab handler. An unstopped Up or Down was therefore re-dispatched through every other matching binding and the retried native keystroke (the selection jumped several rows) and finally reported unhandled, so AppKit beeped. Every key the picker uses stops propagation.
    */
    fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.move_selection(-1, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.move_selection(1, cx);
    }

    fn on_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if let Some(row) = self.selected_row() {
            self.activate(row, window, cx);
        }
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.scope.is_some() {
            self.leave_scope(window, cx);
        } else {
            self.close(window, cx);
        }
    }

    /// Tab: open the highlighted Claude or Codex agent's account list.
    fn on_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.scope.is_none() {
            if let Some(PickerRow::Agent(index)) = self.selected_row() {
                self.enter_scope(index, window, cx);
            }
        }
    }

    fn on_move_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope.is_none() {
            if let Some(PickerRow::Agent(index)) = self.selected_row() {
                if self.agents[index].provider().is_some() {
                    cx.stop_propagation();
                    self.enter_scope(index, window, cx);
                    return;
                }
            }
        }
        cx.propagate();
    }

    fn on_move_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope.is_some() {
            cx.stop_propagation();
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope.is_some() && self.query.is_empty() {
            cx.stop_propagation();
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }

    /// `.new-thread-palette-hints kbd`: 16px chips tinted from the foreground.
    fn render_kbd(&self, label: &'static str) -> impl IntoElement {
        let c = &self.colors;
        div()
            .flex()
            .h(px(16.0))
            .min_w(px(16.0))
            .px(px(4.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(hsla(rgba_of(c.foreground, 0.08)))
            .border_1()
            .border_color(hsla(rgba_of(c.foreground, 0.14)))
            .text_size(px(10.0))
            .line_height(px(14.0))
            .child(label)
    }

    fn render_hint(&self, keys: &[&'static str], label: &'static str) -> impl IntoElement {
        h_flex()
            .items_center()
            .gap(px(4.0))
            .children(keys.iter().map(|key| self.render_kbd(key)))
            .child(label)
    }

    /// `.new-thread-palette-hints`: 11px muted at 80%, centered, 9px 16px 7px.
    /// CDXC:AgentLauncher 2026-09-16 DECISION:
    /// User: "center the controls please in this modal and add 3px margin from top/bottom of the controls": the key-hint chips sit centered under the search field with 3px more room above and below them than the React palette first had; the CSS twin carries the same values.
    fn render_hints(&self) -> impl IntoElement {
        let in_accounts = self.scope.is_some();
        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(16.0))
            .pt(px(9.0))
            .pb(px(7.0))
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(hsla(rgba_of(self.colors.muted, 0.8)))
            .child(self.render_hint(&["↑", "↓"], HINT_MOVE))
            .child(self.render_hint(&["↵"], HINT_START))
            .child(if in_accounts {
                self.render_hint(&["←"], HINT_BACK)
            } else {
                self.render_hint(&["⇥"], HINT_ACCOUNTS)
            })
            .child(self.render_hint(&["esc"], if in_accounts { HINT_BACK } else { HINT_CLOSE }))
    }

    /// `.new-thread-palette-scope`: the provider chip inside the search field
    /// while its accounts are listed; clicking it goes back.
    fn render_scope_chip(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let c = self.colors;
        self.scope_agent().map(|agent| {
            h_flex()
                .id("ghostex-gpui-new-thread-picker-scope")
                .flex_shrink_0()
                .h(px(22.0))
                .items_center()
                .gap(px(5.0))
                .pl(px(4.0))
                .pr(px(7.0))
                .rounded(px(4.0))
                .bg(hsla(rgba_of(c.foreground, 0.08)))
                .hover(|this| this.bg(hsla(rgba_of(c.foreground, 0.14))))
                .text_size(px(12.0))
                .text_color(hsla(c.foreground))
                .child(modal_icon(ICON_BACK, 12.0, c.foreground))
                .child(self.render_agent_icon(agent, 14.0, 0.0))
                .child(agent.name.clone())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.leave_scope(window, cx);
                }))
        })
    }

    /// `CommandInput`: the 36px input group with the search glyph at the end
    /// while empty and a clear button once something is typed.
    fn render_search(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors;
        let has_query = !self.query.is_empty();
        h_flex()
            .flex_shrink_0()
            .h(px(NEW_THREAD_PICKER_SEARCH_HEIGHT))
            .mx(px(6.0))
            .mt(px(6.0))
            .pl(px(12.0))
            .pr(px(12.0))
            .gap(px(10.0))
            .items_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(c.input_border))
            .bg(hsla(c.input_background))
            .children(self.render_scope_chip(cx))
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
                        .text_color(hsla(c.item)),
                ),
            )
            .child(if has_query {
                div()
                    .id("ghostex-gpui-new-thread-picker-clear")
                    .flex()
                    .flex_shrink_0()
                    .size(px(24.0))
                    .items_center()
                    .justify_center()
                    .text_color(hsla(c.muted))
                    .hover(|this| this.text_color(hsla(c.foreground)))
                    .child(modal_icon(ICON_CLEAR, 16.0, c.muted))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_query(window, cx);
                        this.selected = 0;
                        this.input.update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                modal_icon(ICON_SEARCH, 16.0, rgba_of(c.muted, 0.5)).into_any_element()
            })
    }

    /// `ProjectAgentLauncherIcon` in brand colour: the provider logo in its
    /// 14px box, or the code glyph for agents without one.
    fn render_agent_icon(
        &self,
        agent: &NewThreadPickerAgent,
        box_size: f32,
        grow: f32,
    ) -> AnyElement {
        let c = self.colors;
        div()
            .flex()
            .flex_shrink_0()
            .size(px(box_size))
            .items_center()
            .justify_center()
            .child(match agent.icon_path {
                Some(path) => {
                    modal_icon(path, agent.icon_svg_size + grow, c.brand(agent.icon_accent))
                }
                None => modal_icon(ICON_CODE, 14.0, c.glyph),
            })
            .into_any_element()
    }

    /// `.new-thread-palette-row`: 36px, 10px inset, 5px radius; the selected row
    /// carries the composer fill and its 1px ring.
    fn row_shell(
        &self,
        id: impl Into<ElementId>,
        row: PickerRow,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = self.colors;
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
            .border_color(transparent())
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(c.label))
            .when(selected, |this| {
                this.bg(hsla(c.selected_background))
                    .border_color(hsla(c.selected_ring))
                    .text_color(hsla(c.selected_label))
            })
            .when(!selected, |this| {
                this.hover(|this| this.bg(hsla(rgba_of(c.foreground, 0.05))))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate(row, window, cx);
            }))
    }

    fn render_agent_row(&self, index: usize, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors;
        let agent = &self.agents[index];
        let provider = agent.provider();
        let account_count = provider.and_then(|provider| self.provider_account_count(provider));
        self.row_shell(
            ElementId::Name(format!("new-thread-agent-{}", agent.agent_id).into()),
            PickerRow::Agent(index),
            selected,
            cx,
        )
        .child(self.render_agent_icon(agent, 14.0, 0.0))
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
            // `.group-agent-menu-accounts` + `button.group-agent-menu-account-button`:
            // muted at 58%, 24px tall, its own 5px hover surface and outline.
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
                    .border_1()
                    .border_color(transparent())
                    .text_size(px(12.0))
                    .text_color(hsla(rgba_of(c.muted, 0.58)))
                    .hover(|this| {
                        this.bg(hsla(rgba_of(c.foreground, 0.10)))
                            .border_color(hsla(rgba_of(c.foreground, 0.18)))
                            .text_color(hsla(rgba_of(c.muted, 0.9)))
                    })
                    .child(modal_icon(ICON_ACCOUNTS, 14.0, rgba_of(c.muted, 0.58)))
                    .children(account_count.map(|count| count.to_string()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.enter_scope(index, window, cx);
                    })),
            )
        })
        .when(agent.supports_chat(), |this| {
            // `.group-agent-menu-chat-support`: muted at 58%, 6px before and 5px after.
            this.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .ml(px(6.0))
                    .mr(px(5.0))
                    .child(modal_icon(ICON_CHAT, 14.0, rgba_of(c.muted, 0.58))),
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        self.row_shell(id, row, selected, cx)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .size(px(14.0))
                    .items_center()
                    .justify_center()
                    .child(modal_icon(icon_path, 14.0, c.glyph)),
            )
            .child(div().flex_1().min_w_0().child(label))
            .into_any_element()
    }

    /// `.new-thread-palette-account-row`: the 16px provider logo, the name with
    /// its Default marker, and the usage figures underneath.
    fn render_account_row(
        &self,
        index: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = self.colors;
        let (Some(agent), Some(account)) = (self.scope_agent(), self.account(index)) else {
            return div().into_any_element();
        };
        let display_name = account.name.clone();
        let usage = account.usage.clone();
        let is_default = account.is_default;
        let ready = account.ready;
        self.row_shell(
            ElementId::Name(format!("new-thread-account-{index}").into()),
            PickerRow::Account(index),
            selected,
            cx,
        )
        .py(px(6.0))
        .when(!ready, |this| this.opacity(0.5))
        .child(self.render_agent_icon(agent, 16.0, 1.5))
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
                                    .text_color(hsla(c.muted))
                                    .child(DEFAULT_SUFFIX),
                            )
                        }),
                )
                .children(usage.map(|usage| {
                    div()
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(10.5))
                        .line_height(px(13.0))
                        .text_color(hsla(c.muted))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(usage)
                })),
        )
        .into_any_element()
    }

    /// `.gx-account-launcher-hint` inside the palette: 11px muted, 6px 10px.
    fn render_hint_text(&self, text: impl Into<SharedString>) -> AnyElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .line_height(px(16.5))
            .text_color(hsla(self.colors.muted))
            .child(text.into())
            .into_any_element()
    }

    /// `CommandEmpty`: centered 14px text with 24px above and below.
    fn render_empty(&self, text: &'static str) -> AnyElement {
        div()
            .w_full()
            .py(px(24.0))
            .text_center()
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(self.colors.item))
            .child(text)
            .into_any_element()
    }

    fn render_divider(&self) -> AnyElement {
        div()
            .flex_shrink_0()
            .h(px(1.0))
            .mx(px(4.0))
            .my(px(4.0))
            .bg(hsla(self.colors.divider))
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
                        children.push(self.render_divider());
                    }
                    children.push(match *row {
                        PickerRow::Agent(index) => self.render_agent_row(index, selected, cx),
                        PickerRow::Browser => self.render_plain_row(
                            "new-thread-browser",
                            PickerRow::Browser,
                            ICON_BROWSER,
                            ROW_BROWSER,
                            selected,
                            cx,
                        ),
                        PickerRow::Terminal => self.render_plain_row(
                            "new-thread-terminal",
                            PickerRow::Terminal,
                            ICON_TERMINAL,
                            ROW_TERMINAL,
                            selected,
                            cx,
                        ),
                        _ => continue,
                    });
                }
                if rows.is_empty() {
                    children.push(if self.agents_loaded {
                        self.render_empty(EMPTY_AGENTS)
                    } else {
                        self.render_hint_text(LOADING_AGENTS)
                    });
                }
            }
            Some(agent) => {
                let agent = agent.clone();
                if self.accounts.is_none() && self.accounts_error.is_none() {
                    children.push(self.render_hint_text(READING_ACCOUNTS));
                }
                if let Some(error) = &self.accounts_error {
                    if self.accounts.is_none() {
                        children.push(self.render_hint_text(error.clone()));
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
                                .child(self.render_agent_icon(&agent, 14.0, 0.0))
                                .child(div().flex_1().min_w_0().child(ROW_CLI_LOGIN))
                                .into_any_element(),
                            );
                            children.push(self.render_hint_text(CLI_LOGIN_HINT));
                            children.push(self.render_divider());
                            children.push(self.render_hint_text(ADD_ACCOUNT_HINT));
                        }
                        PickerRow::AddAccount => {
                            children.push(
                                self.row_shell(
                                    "new-thread-add-account",
                                    PickerRow::AddAccount,
                                    selected,
                                    cx,
                                )
                                .child(div().flex_1().min_w_0().child(ROW_ADD_ACCOUNT))
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
                                .child(div().flex_1().min_w_0().child(ROW_TRY_AGAIN))
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
                    children.push(self.render_empty(EMPTY_ACCOUNTS));
                }
            }
        }
        children
    }
}

impl Render for GpuiNewThreadPickerWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.colors;
        let list_children = self.render_list(cx);
        v_flex()
            .id("ghostex-gpui-new-thread-picker")
            .size_full()
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(c.frame_border))
            // Under window glass the picker's window blurs what is behind it, so its fill thins.
            // Under glass it takes the app's frosted menu fill, passed in by the app because this
            // file also builds into the demo binary (`frosted_menu_fill` in helpers/window_glass.rs).
            .bg(self.frosted_fill.unwrap_or_else(|| hsla(c.surface)))
            .font_family(PICKER_FONT)
            .text_size(px(ROW_TEXT_SIZE))
            .line_height(px(ROW_LINE_HEIGHT))
            .text_color(hsla(c.item))
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_move_right))
            .capture_action(cx.listener(Self::on_move_left))
            .capture_action(cx.listener(Self::on_backspace))
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
                    .child(Scrollbar::vertical(&self.scroll).thickness(px(SCROLLBAR_WIDTH))),
            )
    }
}
