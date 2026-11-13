//! Native GPUI New Thread picker (Cmd+Shift+T). Its React twin, `NewThreadPalette` in
//! packages/core-ui/new-thread-palette.tsx, was deleted with the React web app on 2026-09-24.
//!
//! CDXC:AgentLauncher 2026-09-09 DECISION:
//! User: the desktop New Thread picker (Cmd+Shift+T) is drawn natively in GPUI so it opens instantly and is sized to its rows (up to twelve agents plus the Coordinator, Browser and Terminal rows, then it scrolls).
//! It mirrors the project-header agent dropdown: every agent with its account count and chat badge, the last-used agent first and preselected, typing filters, Up/Down move, Enter starts, Tab or Right on Claude or Codex opens that provider's account list (Left, Backspace on an empty query, or Esc goes back), Esc closes. The highlighted row uses the sidebar's focused-session chrome and is never bolded.
//!
//! CDXC:AppModal 2026-09-16 DECISION:
//! User: "please fix this modal, also please ensure that we use the gpui components that we created in the gpui app and we're not using the older modals": the picker takes its colours from the shared native modal kit palette in both appearances. It used dark-only white tints that vanished on the light theme (search border, key hints, divider, white agent logos) while the highlighted row stayed black.
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/agent_launcher_menu.rs (the dropdown this mirrors), apps/desktop/src/app/window/native_modal_kit/ (the shared palette), apps/desktop/src/app/new_thread_picker_lifecycle.rs (open, close, preload, data), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
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
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::rc::Rc;

#[path = "new_thread_picker_run_on.rs"]
mod run_on;
pub(crate) use run_on::*;
#[path = "new_thread_picker_place.rs"]
mod place;
pub(crate) use place::*;
#[path = "new_thread_picker_input.rs"]
mod input;
#[path = "new_thread_picker_render.rs"]
mod render;

/*
CDXC:AgentLauncher 2026-09-09 DECISION:
User: the native New Thread picker is sized to its rows: the search field, the key-hint row, one row per agent up to twelve, the divider, and the Browser and Terminal rows; more agents scroll. The chrome height is the 6px top inset, 36px search field, 32px hint row, 2px list inset, 9px divider, three 36px rows (Coordinator, Browser, Terminal), the 6px bottom inset, and the 2px frame border.
SEE-ALSO: the 40px project row (new_thread_picker_place.rs) and the 30px Run on row (new_thread_picker_run_on.rs) are added whenever the picker knows its projects, which the app always passes; the preview binary without them is exactly this. Its width is the user's 2026-10-29 decision below (420px became 460px so the key hints keep one line).
*/
/*
CDXC:Coordinators 2026-11-01 DECISION:
User: "please add new coordinator option here" (the picker's bottom section). A Coordinator row sits above Browser and Terminal, with the users-group icon the project launcher's "New Coordinator…" row uses, and typing "coord" finds it. Choosing it opens the existing New Coordinator dialog for the project the picker would have started a session in (the same `coordinator` project action the launcher sends), so there is one create flow. The chrome grows by that third 36px row (165px became 201px).
*/
/*
CDXC:AgentLauncher 2026-10-29 DECISION:
User: "make this wider so the hotkeys don't wrap". At 420px the widest key-hint row (Move, Start, Accounts, the Run on row's Location keys, Close) wrapped "esc Close" onto a second line, which also pushed the Terminal row under the bottom edge. Measured at the default text size, that row is 394px of chips and labels with the macOS `⌘←→` keys and about 6px more with `Alt ←→` elsewhere; with its 16px side padding and the 1px frame it needs about 434px, so 460px keeps one line with room to spare on every platform.
*/
pub(crate) const NEW_THREAD_PICKER_WIDTH: f32 = 460.0;
pub(crate) const NEW_THREAD_PICKER_SEARCH_HEIGHT: f32 = 36.0;
pub(crate) const NEW_THREAD_PICKER_ROW_HEIGHT: f32 = 36.0;
pub(crate) const NEW_THREAD_PICKER_MAX_AGENT_ROWS: usize = 12;
pub(crate) const NEW_THREAD_PICKER_CHROME_HEIGHT: f32 = 201.0;
/// The coordinator step's title row above the search field.
const COORDINATOR_TITLE_HEIGHT: f32 = 34.0;
/// The coordinator step's warning line under the list, kept even while it is empty so the frame
/// does not jump as the highlight moves.
const COORDINATOR_WARNING_HEIGHT: f32 = 40.0;

/// `.quick-access-surface`: Inter at 14px over 20px.
const PICKER_FONT: &str = crate::ui_fonts::UI_FONT;
const ROW_TEXT_SIZE: f32 = 14.0;
const ROW_LINE_HEIGHT: f32 = 20.0;
const SCROLLBAR_WIDTH: f32 = 5.0;

const PLACEHOLDER_AGENTS: &str = "Search agents, coordinator, browser, terminal...";
const PLACEHOLDER_COORDINATOR_AGENTS: &str = "Search agents...";
const PLACEHOLDER_ACCOUNTS: &str = "Search accounts...";
/// Above a Claude (cloud) account list.
const CLOUD_ACCOUNT_NOTE: &str =
    "Runs on this account for good: its usage, follow-ups and Bring to this computer stay on it.";
const HINT_MOVE: &str = "Move";
const HINT_START: &str = "Start";
const HINT_BACK: &str = "Back";
const HINT_ACCOUNTS: &str = "Accounts";
const HINT_CLOSE: &str = "Close";
const ROW_COORDINATOR: &str = "Coordinator";
const TITLE_COORDINATOR_STEP: &str = "Pick the coordinator's agent";
const ICON_WARNING: &str = "titlebar/alert-triangle.svg";
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
const ICON_COORDINATOR: &str = "titlebar/users-group.svg";
const ICON_BROWSER: &str = "modals/new-thread-picker/world.svg";
const ICON_TERMINAL: &str = "modals/new-thread-picker/terminal-2.svg";
const ICON_BACK: &str = "modals/new-thread-picker/chevron-left.svg";
const ICON_SEARCH: &str = "modals/new-thread-picker/search.svg";
const ICON_CLEAR: &str = "modals/new-thread-picker/x.svg";

/// Search field, key hints, list insets, the divider, and the Coordinator,
/// Browser and Terminal rows, plus one row per agent up to the visible maximum; longer
/// agent lists scroll inside that frame. `place` adds the project row, `run_on` the Run on row.
pub(crate) fn new_thread_picker_window_height(
    agent_count: usize,
    place: bool,
    run_on: bool,
) -> f32 {
    NEW_THREAD_PICKER_CHROME_HEIGHT
        + if place {
            NEW_THREAD_PICKER_PLACE_HEIGHT
        } else {
            0.0
        }
        + if run_on {
            NEW_THREAD_PICKER_RUN_ON_HEIGHT
        } else {
            0.0
        }
        + agent_count.min(NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32 * NEW_THREAD_PICKER_ROW_HEIGHT
}

/// The coordinator step's frame: the title row and the warning line instead of the divider and the
/// Coordinator, Browser and Terminal rows, and no project or Run on row.
pub(crate) fn new_thread_picker_coordinator_height(agent_count: usize) -> f32 {
    NEW_THREAD_PICKER_CHROME_HEIGHT - 3.0 * NEW_THREAD_PICKER_ROW_HEIGHT - 9.0
        + COORDINATOR_TITLE_HEIGHT
        + COORDINATOR_WARNING_HEIGHT
        + agent_count.min(NEW_THREAD_PICKER_MAX_AGENT_ROWS) as f32 * NEW_THREAD_PICKER_ROW_HEIGHT
}

/// The picker as the New Coordinator form's second step: the agent (and account) the coordinator
/// runs on, in the copy the form picked.
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// User (Q2980 "Build it: auto-send + remind on drift"): the New Coordinator form drops its agent and model fields; when it is done, the normal New Thread picker opens titled "Pick the coordinator's agent", with every agent and its accounts on Tab, and the pick starts the coordinator and sends its first message. It shows only agents (no Coordinator, Browser or Terminal row, no project list, no Run on row: the form picked where it runs), and a warning line under the list when the highlighted agent would start on a model a coordinator works worse on (gx-protocol `coordinator_quality_concern`).
/// SEE-ALSO: apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (opens it, creates the pick), apps/gpui-web/src/app/web_host/new_thread_picker.rs (the page's picker).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NewThreadPickerCoordinatorStep {
    /// The sidebar group of the copy the coordinator runs in.
    pub(crate) group_id: String,
    /// The name the form was given (empty: the placeholder "Coordinator").
    pub(crate) name: String,
    /// "Coordinator quality may be degraded on <model>" by agent id, for the agents whose start
    /// model or effort gives a concern.
    pub(crate) warnings: std::collections::BTreeMap<String, String>,
}

/// The picker never covers more than this share of the window it opens over.
const NEW_THREAD_PICKER_MAX_WINDOW_SHARE: f32 = 0.7;
/// The room the picker keeps above and below itself inside that window.
const NEW_THREAD_PICKER_WINDOW_MARGIN: f32 = 16.0;
/// The shortest the cap goes, so a very short window still shows the header and a few rows.
const NEW_THREAD_PICKER_MIN_HEIGHT: f32 = 240.0;

/// `height` held to the window the picker opens over (`room`, the app window's bounds, or the
/// page in the web build): at most 70 percent of its height, and never past its bottom edge less a
/// margin when the picker's `top` is known (resizing keeps the top where it is). The list scrolls
/// inside what is left; with no `room` (the preview) the height is unchanged.
///
/// CDXC:AgentLauncher 2026-11-11 DECISION:
/// User ("are u serious man? where's the max height here??"): the project list was sized to twelve rows plus the second line of every row with computers, about 870px, and grew down from where the short agent list had centred it, so on a normal window it ran past the bottom edge with no scrolling. The picker's height is now capped to the window it opens over, whatever mode it is in.
pub(crate) fn new_thread_picker_capped_height(
    height: f32,
    room: Option<gpui::Bounds<gpui::Pixels>>,
    top: Option<f32>,
) -> f32 {
    let Some(room) = room else {
        return height;
    };
    let room_height = f32::from(room.size.height);
    let mut cap = (room_height * NEW_THREAD_PICKER_MAX_WINDOW_SHARE)
        .min(room_height - 2.0 * NEW_THREAD_PICKER_WINDOW_MARGIN);
    if let Some(top) = top {
        cap = cap.min(f32::from(room.bottom()) - NEW_THREAD_PICKER_WINDOW_MARGIN - top);
    }
    height.min(cap.max(NEW_THREAD_PICKER_MIN_HEIGHT.min(height)))
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
        let colors = Self::resolve_solid(p);
        if !p.glass {
            return colors;
        }
        // On the popover glass the selected row, the filter field and the dividers take the
        // dialogs' stronger washes (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`).
        let washes = frosted_dialog_washes(p.light);
        Self {
            selected_background: washes.raised,
            input_background: washes.raised,
            divider: rgba_of(washes.hairline, washes.hairline.a * 0.7),
            ..colors
        }
    }

    fn resolve_solid(p: &ModalPalette) -> Self {
        if p.light {
            let foreground = rgb(0x262626);
            Self {
                // Solid: under glass the app thins it into its frosted fill (`surface_color`).
                surface: p.solid_surface,
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
                surface: p.solid_surface,
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
    /// The HUD's `usesOwnLogin`: the agent's command picks its own login, so it lists no accounts.
    pub(crate) uses_own_login: bool,
}

impl NewThreadPickerAgent {
    fn family(&self) -> &str {
        self.icon.as_deref().unwrap_or(&self.agent_id)
    }

    /// Claude and Codex are the providers with Ghostex-managed accounts, unless the agent's command
    /// picks its own login.
    pub(crate) fn provider(&self) -> Option<&'static str> {
        if self.uses_own_login {
            return None;
        }
        match self.family() {
            // A Claude cloud session runs on a saved Claude account too (CDXC:ClaudeCloud).
            "claude" | "claude-cloud" => Some("claude"),
            "codex" => Some("codex"),
            _ => None,
        }
    }

    /// Port of `resolveSessionChatTranscriptAgent` in packages/shared/session-chat-agents.ts.
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
    /// The agentbox boxes a thread can run in, as last read.
    pub(crate) boxes: NewThreadPickerBoxes,
    /// Settings' default location: `local` or a box's `runLocation`.
    pub(crate) default_run_location: String,
    /// Offer the Run on row's Cloud Box chip (Cloud Boxes on, project on this computer).
    pub(crate) cloud_box: bool,
    /// The app closes the picker when its window stops being key; the preview keeps it open.
    pub(crate) close_when_inactive: bool,
    /// The account of the active project's last Claude cloud session: the Claude (cloud) account
    /// list highlights it first (gx-core `project_last_cloud_account`).
    pub(crate) cloud_default_account: Option<String>,
    /// The projects a thread can start in and where the picker starts; `None` (the preview) draws
    /// no project row and starts in the active project.
    pub(crate) place: Option<NewThreadPickerPlace>,
    /// The window the picker opens over, which caps its height; `None` (the preview) leaves the
    /// height uncapped.
    pub(crate) room: Option<gpui::Bounds<gpui::Pixels>>,
    /// The New Coordinator form's agent step; `None` is the ordinary picker.
    pub(crate) coordinator: Option<NewThreadPickerCoordinatorStep>,
}

/// What the picker asks its host to do. The picker removes its own window
/// before sending any command except `RetryAccounts`.
pub(crate) enum NewThreadPickerCommand {
    /// `runSidebarAgent { agentId, accountId?, runLocation? }`.
    LaunchAgent {
        agent_id: String,
        account_id: Option<String>,
        /// A box's `runLocation`; `None` runs on the picked machine.
        run_location: Option<String>,
        /// The picked project and machine; `None` is the active project.
        target: Option<NewThreadPickerTarget>,
    },
    /// The coordinator step's pick: create the coordinator on this agent and account.
    PickCoordinatorAgent {
        step: NewThreadPickerCoordinatorStep,
        agent_id: String,
        account_id: Option<String>,
    },
    /// The New Coordinator dialog (`projectAction` `coordinator`).
    NewCoordinator {
        target: Option<NewThreadPickerTarget>,
    },
    /// `openBrowserPaneInGroup`.
    OpenBrowser {
        target: Option<NewThreadPickerTarget>,
    },
    /// `createSession`.
    CreateTerminal {
        target: Option<NewThreadPickerTarget>,
    },
    /// Settings on its Accounts page.
    AddAccount,
    /// Settings on its Cloud Boxes page (the Run on row's set-up link).
    OpenCloudBoxesSettings,
    /// Read the accounts list again after a failure.
    RetryAccounts,
    /// Escape, or the window lost activation.
    Closed,
}

pub(crate) type NewThreadPickerHost = Rc<dyn Fn(NewThreadPickerCommand, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerRow {
    Agent(usize),
    Coordinator,
    Browser,
    Terminal,
    Account(usize),
    CliLogin,
    AddAccount,
    Retry,
    /// A project of the project list.
    Project(usize),
    /// A project the search found, started with the first agent (new_thread_picker_place.rs).
    StartIn(usize),
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
    run_on: RunOnState,
    /// The Run on chips' horizontal scroll (new_thread_picker_run_on.rs).
    run_on_scroll: ScrollHandle,
    query: String,
    selected: usize,
    scope: Option<usize>,
    scroll: ScrollHandle,
    /// Set once the window has been key; a preloaded hidden window is never
    /// active, so losing activation only closes a window that was shown.
    was_active: bool,
    close_when_inactive: bool,
    /// See `NewThreadPickerConfig::cloud_default_account`.
    cloud_default_account: Option<String>,
    /// See `NewThreadPickerConfig::place`.
    place: Option<NewThreadPickerPlace>,
    /// See `NewThreadPickerConfig::room`.
    room: Option<gpui::Bounds<gpui::Pixels>>,
    /// The list shows the projects (new_thread_picker_place.rs).
    choosing_project: bool,
    /// A project row is highlighted; the list opens with none, so the first Down is its top row.
    project_cursor: bool,
    /// A place picked with Left or Right on a project row, by project.
    row_places: std::collections::BTreeMap<usize, usize>,
    /// See `NewThreadPickerConfig::coordinator`.
    coordinator: Option<NewThreadPickerCoordinatorStep>,
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
        let placeholder = if config.coordinator.is_some() {
            PLACEHOLDER_COORDINATOR_AGENTS
        } else {
            PLACEHOLDER_AGENTS
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let change_subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = input.read(cx).value().to_string();
                    this.selected = 0;
                    // Typing highlights the best match, as the agent list does.
                    this.project_cursor = !this.query.trim().is_empty();
                    this.scroll.scroll_to_item(0);
                    // The project list is as tall as the projects the search leaves.
                    if this.choosing_project {
                        this.fit_window_height(window);
                    }
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
                super::popup_dismissal::dismiss_on_focus_loss(
                    this,
                    window,
                    cx,
                    |this, window, cx| this.close(window, cx),
                );
            }
        });
        let press_subscription = super::popup_dismissal::observe_main_window_press(
            window,
            cx,
            |this: &mut Self, window, cx| {
                // A hidden preload is not on screen for a press to click away from.
                if this.was_active && this.close_when_inactive {
                    this.close(window, cx);
                }
            },
        );
        input.update(cx, |input, cx| input.focus(window, cx));
        let start_machine = config.place.as_ref().map(|place| place.machine);
        let mut picker = Self {
            glass: false,
            frosted_fill: None,
            host,
            colors: PickerColors::resolve(&config.palette),
            input,
            agents: config.agents,
            agents_loaded: config.agents_loaded,
            accounts: config.accounts,
            accounts_error: None,
            cloud_default_account: config.cloud_default_account.clone(),
            run_on: RunOnState::new(config.boxes, config.default_run_location)
                .with_cloud_box(config.cloud_box),
            run_on_scroll: ScrollHandle::new(),
            query: String::new(),
            selected: 0,
            scope: None,
            scroll: ScrollHandle::new(),
            was_active: window.is_window_active(),
            close_when_inactive: config.close_when_inactive,
            place: config.place,
            room: config.room,
            choosing_project: false,
            project_cursor: false,
            row_places: Default::default(),
            coordinator: config.coordinator,
            _subscriptions: vec![
                change_subscription,
                activation_subscription,
                press_subscription,
            ],
        };
        if let Some(machine) = start_machine {
            picker.apply_place_machines(machine);
        }
        picker.reveal_selected_run_location();
        picker
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
        self.run_on = RunOnState::new(config.boxes, config.default_run_location)
            .with_cloud_box(config.cloud_box);
        self.cloud_default_account = config.cloud_default_account;
        let start_machine = config.place.as_ref().map(|place| place.machine);
        self.place = config.place;
        self.room = config.room;
        self.coordinator = config.coordinator;
        self.choosing_project = false;
        self.set_placeholder(self.agents_placeholder(), window, cx);
        if let Some(machine) = start_machine {
            self.apply_place_machines(machine);
        }
        self.fit_window_height(window);
        self.reveal_selected_run_location();
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
        // The coordinator step lists the agents of the copy's computer it opened with.
        if self.coordinator.is_some() {
            return;
        }
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
        if let Some(scope_agent_id) = self.scope.and(selected_agent_id.as_deref()) {
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
        self.fit_window_height(window);
        cx.notify();
    }

    /// The frame follows the agent count and whether the project and Run on rows are drawn, or
    /// the project list while it is open.
    pub(super) fn fit_window_height(&self, window: &mut Window) {
        let height = if self.choosing_project {
            self.project_list_height()
        } else if self.coordinator.is_some() {
            new_thread_picker_coordinator_height(self.agents.len())
        } else {
            new_thread_picker_window_height(
                self.agents.len(),
                self.place.is_some(),
                self.place.is_none() && self.run_on.shown(),
            )
        };
        let height = new_thread_picker_capped_height(
            height,
            self.room,
            Some(f32::from(window.bounds().origin.y)),
        );
        window.resize(size(px(NEW_THREAD_PICKER_WIDTH), px(height)));
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

    /// The account list is the Claude (cloud) agent's.
    fn cloud_scope(&self) -> bool {
        self.scope_agent()
            .is_some_and(|agent| ghostex_gx_core::is_claude_cloud_agent(&agent.agent_id))
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
        if self.choosing_project {
            return self.project_rows();
        }
        let query = self.normalized_query();
        match self.scope_agent() {
            None => {
                // A box runs only agentbox's agents; a coordinator, a browser or a terminal never runs in one.
                let in_box = self.run_on.in_box();
                let mut rows: Vec<PickerRow> = self
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, agent)| !in_box || runs_in_box(agent))
                    .filter(|(_, agent)| matches_query(&agent.name, &query))
                    .map(|(index, _)| PickerRow::Agent(index))
                    .collect();
                // The coordinator step picks an agent only.
                if self.coordinator.is_some() {
                    return rows;
                }
                if !in_box && matches_query(ROW_COORDINATOR, &query) {
                    rows.push(PickerRow::Coordinator);
                }
                if !in_box && matches_query(ROW_BROWSER, &query) {
                    rows.push(PickerRow::Browser);
                }
                if !in_box && matches_query(ROW_TERMINAL, &query) {
                    rows.push(PickerRow::Terminal);
                }
                rows.extend(self.start_rows());
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

    /// The search field's words over the agent list: the coordinator step lists agents only.
    fn agents_placeholder(&self) -> &'static str {
        if self.coordinator.is_some() {
            PLACEHOLDER_COORDINATOR_AGENTS
        } else {
            PLACEHOLDER_AGENTS
        }
    }

    /// The coordinator step's warning for the highlighted agent, or the one whose accounts show.
    fn coordinator_warning(&self) -> Option<&str> {
        let step = self.coordinator.as_ref()?;
        let agent = match self.scope_agent() {
            Some(agent) => agent,
            None => match self.selected_row()? {
                PickerRow::Agent(index) => self.agents.get(index)?,
                _ => return None,
            },
        };
        step.warnings.get(&agent.agent_id).map(String::as_str)
    }

    fn selected_row(&self) -> Option<PickerRow> {
        self.rows().get(self.selected).copied()
    }

    /// Child index inside the scroll container, which also holds the divider
    /// between the agents and the Coordinator, Browser and Terminal rows.
    fn scroll_child_index(&self, rows: &[PickerRow], selected: usize) -> usize {
        if self.choosing_project {
            return self.project_scroll_child_index(rows, selected);
        }
        let agent_rows = rows
            .iter()
            .filter(|row| matches!(row, PickerRow::Agent(_)))
            .count();
        let past_divider = self.scope.is_none() && agent_rows > 0 && selected >= agent_rows;
        selected + usize::from(past_divider) + self.start_heading_before(rows, selected)
    }
}
