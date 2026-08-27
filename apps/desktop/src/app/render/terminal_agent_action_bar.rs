// The agent action bar that sits directly below an agent session's terminal
// body, replacing the floating top-right overlay cluster that used to be
// painted inside `terminal_element.rs`.
//
// Design: docs/2026-08-25/chat-terminal-buttons/option-1-mockup.html, frame
// "Terminal view — bare bottom bar". It is the chat composer's footer row
// replayed below the terminal with no composer box around it, so flipping a
// session between Chat and Terminal keeps the same controls in the same place.
//
// Layout discipline (AGENTS.md "Native layout and hit-testing"): the bar is a
// normal sibling of the terminal body inside the pane's vertical flex, not an
// overlay. The terminal never renders underneath it, so no transparent shield,
// hit-test override, or synthetic routing is involved. The one element that
// does overlap the terminal is the "More actions" popup menu — a normal
// dismiss-on-outside-press menu surface, like the titlebar's dropdown panels —
// and it occludes only its own visible rectangle.

use gpui::AnyElement;
use gpui::BoxShadow;
use gpui::ClipboardItem;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Rgba;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui::svg;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::Tooltip;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/*
Geometry is derived from the real chat composer so the shared controls land on
the same pixels when a session flips between Chat and Terminal:

  packages/core-ui/chat/session-chat-view.tsx  wrapper `max-w-3xl px-4 pb-3`
  packages/core-ui/chat/session-chat-composer.tsx  box `rounded-3xl border px-4`

so the composer's action row is inset 16 + 1 + 16 = 33px from the pane's side
edges and its centre line sits 12 + 1 + 10 + 16 = 39px above the pane's bottom
edge. A 44px-tall bar therefore needs a 17px bottom inset to put its buttons on
that same centre line.

The bar draws the composer's own rounded border around itself and fills with
the dark composer's #141414 surface so the controls sit on the same raised
pill instead of on the black terminal. It is the same hairline pill, at the
same width and the same side inset — only as tall as the controls inside it,
because there is no message field here to give it height.
*/
/// Tailwind `max-w-3xl` (48rem), the width the chat column centres to.
const TERMINAL_AGENT_BAR_MAX_CONTENT_WIDTH: f32 = 768.0;
/// The chat wrapper's `px-4`, applied outside the centred column just like it is.
const TERMINAL_AGENT_BAR_OUTER_PADDING: f32 = 16.0;
/// The composer box's `px-4`; its 1px border is drawn here too, so the controls
/// keep the same 17px total inset from the pill's outer edge.
const TERMINAL_AGENT_BAR_INNER_PADDING: f32 = 16.0;
const TERMINAL_AGENT_BAR_HEIGHT: f32 = 44.0;
const TERMINAL_AGENT_BAR_BOTTOM_INSET: f32 = 17.0;
/// `size-7` in the chat footer.
const TERMINAL_AGENT_BAR_BUTTON_SIZE: f32 = 28.0;
/// 25% under the chat footer's original `size-8` (32px), matching Send.
const TERMINAL_AGENT_BAR_ACCENT_BUTTON_SIZE: f32 = 24.0;
/// Chat Send's corners, scaled with the 24px control (was 8px on 32px).
const TERMINAL_AGENT_BAR_ACCENT_BUTTON_RADIUS: f32 = 6.0;
/// `gap-1.5` in the chat footer's action group.
const TERMINAL_AGENT_BAR_BUTTON_GAP: f32 = 6.0;
const TERMINAL_AGENT_BAR_ICON_SIZE: f32 = 16.0;
/*
`.ghostex-chat-stash-control .n svg { width: 1.25rem }` in
packages/core-ui/styles/chat.css: the stack-push glyph only inks 16x15 of its
24px box, so at the shared 16px size it reads smaller than the paperclip beside
it. The chat footer renders that one glyph a size up; the bar does the same.
*/
const TERMINAL_AGENT_BAR_STASH_ICON_SIZE: f32 = 20.0;
const TERMINAL_AGENT_BAR_MENU_ICON_SIZE: f32 = 14.0;
const TERMINAL_AGENT_BAR_MENU_SHORTCUT_SIZE: f32 = 11.0;
/// How far the bottom-bar tooltips are lifted above their trigger. Applied as a
/// relative `top` inset, not a margin: the tooltip bubble sits inside a plain
/// block `div`, where block layout collapses child margins through the parent,
/// so a margin never reaches the overlay positioner's measured bounds and the
/// tooltip lands flush on the bar. A negative inset shifts the painted bubble
/// without depending on that measurement. 8px matches the chat composer's
/// tooltip gap (`sideOffset = 8` in packages/core-ui/app-tooltip.tsx).
const TERMINAL_AGENT_BAR_TOOLTIP_LIFT: f32 = 8.0;
/// The Copy affordance next to the session id, sized down from the button icons
/// so it reads as part of the 12px id text rather than as a seventh control.
const TERMINAL_AGENT_BAR_COPY_ICON_SIZE: f32 = 13.0;
const TERMINAL_AGENT_BAR_NOTE_DOT_SIZE: f32 = 5.0;
const TERMINAL_AGENT_BAR_BADGE_HEIGHT: f32 = 10.0;
const TERMINAL_AGENT_BAR_BADGE_MIN_WIDTH: f32 = 10.0;
const TERMINAL_AGENT_BAR_MENU_WIDTH: f32 = 200.0;
/// Distance from the ⋯ button's top edge up to the menu's bottom edge.
const TERMINAL_AGENT_BAR_MENU_GAP: f32 = 6.0;
const TERMINAL_AGENT_BAR_ICON_COLOR: u32 = 0xa6a6a6;
/// Controls the chat footer owns but the terminal has no route to. They keep
/// their slot so the two footers stay column-for-column identical.
const TERMINAL_AGENT_BAR_DISABLED_ICON_COLOR: u32 = 0x5a5a5a;
/*
The chat composer's `border-input`, which the dark chat theme resolves to 8%
white (packages/core-ui/styles/chat.css). That mix is computed against the chat
page's #0e0e0e; the terminal pane behind this bar is black, where the same 8%
lands at #141414 and disappears. This is the app-wide `--input` from
packages/core-ui/styles/theme.css (15% white) composited on black instead, so
the pill reads as the same hairline the composer draws rather than as a fainter
one.
*/
/// Dark chat composer fill (`packages/core-ui/styles/chat.css`).
const TERMINAL_AGENT_BAR_BACKGROUND: u32 = 0x141414;
const TERMINAL_AGENT_BAR_BORDER_COLOR: u32 = 0x262626;
const TERMINAL_AGENT_BAR_HOVER_BACKGROUND: u32 = 0x343434;
const TERMINAL_AGENT_BAR_SESSION_ID_COLOR: u32 = 0x6f6f6f;
const TERMINAL_AGENT_BAR_SESSION_ID_HOVER_COLOR: u32 = 0xc4c4c4;
/// Dark chat `--primary` (`oklch(92.2% 0 0)` → #e5e5e5).
const TERMINAL_AGENT_BAR_ACCENT_BACKGROUND: u32 = 0xe5e5e5;
const TERMINAL_AGENT_BAR_ACCENT_HOVER_BACKGROUND: u32 = 0xffffff;
const TERMINAL_AGENT_BAR_ACCENT_ICON_COLOR: u32 = 0x111111;
const TERMINAL_AGENT_BAR_MENU_BACKGROUND: u32 = 0x151515;
const TERMINAL_AGENT_BAR_MENU_BORDER: u32 = 0x2a2a2a;
const TERMINAL_AGENT_BAR_MENU_SEPARATOR: u32 = 0x222222;
const TERMINAL_AGENT_BAR_MENU_TEXT_COLOR: u32 = 0xd6d6d6;

/*
Every glyph below is the Tabler outline icon the chat footer imports from
`@tabler/icons-react`, shipped here as an asset file under
`apps/desktop/assets/titlebar/` (the crate's established icon pattern; see
`apps/desktop/src/assets.rs`). The mapping is one-for-one with
`packages/core-ui/chat/session-chat-composer-actions.tsx`:

  IconDots        → dots.svg          IconNote        → note.svg
  IconStackPush   → stack-push.svg    IconPaperclip   → paperclip.svg
  IconMaximize    → maximize.svg      IconMinimize    → minimize.svg
  IconEdit        → edit.svg          IconEyeOff      → eye-off.svg
  IconClockCheck  → clock-check.svg   IconPencil      → pencil.svg
  IconGitBranch   → git-branch.svg    IconRefresh     → refresh.svg
  IconFileExport  → file-export.svg   IconCopy        → copy.svg

Two glyphs are deliberately not a Tabler import on the chat side either: Sleep
uses `moon.svg`, which `SleepMoonIcon` in that same file copies verbatim, and
the surface toggle is mirrored — chat shows `IconTerminal2` for "Terminal View",
so the terminal shows the message bubble for "Chat View".
*/
const TERMINAL_AGENT_BAR_MORE_ACTIONS_ICON: &str = "titlebar/dots.svg";
const TERMINAL_AGENT_BAR_SESSION_NOTE_ICON: &str = "titlebar/note.svg";
const TERMINAL_AGENT_BAR_MAXIMIZE_ICON: &str = "titlebar/maximize.svg";
const TERMINAL_AGENT_BAR_EXIT_MAXIMIZE_ICON: &str = "titlebar/minimize.svg";
const TERMINAL_AGENT_BAR_VERBOSE_MODE_ICON: &str = "titlebar/eye-off.svg";
const TERMINAL_AGENT_BAR_PROMPT_EDITOR_ICON: &str = "titlebar/edit.svg";
const TERMINAL_AGENT_BAR_ATTACH_PATH_ICON: &str = "titlebar/paperclip.svg";
const TERMINAL_AGENT_BAR_RENAME_ICON: &str = "titlebar/pencil.svg";
const TERMINAL_AGENT_BAR_SLEEP_ICON: &str = "titlebar/moon.svg";
const TERMINAL_AGENT_BAR_DELAYED_ACTIONS_ICON: &str = "titlebar/clock-check.svg";
const TERMINAL_AGENT_BAR_FORK_ICON: &str = "titlebar/git-branch.svg";
const TERMINAL_AGENT_BAR_FULL_RELOAD_ICON: &str = "titlebar/refresh.svg";
const TERMINAL_AGENT_BAR_STASHED_PROMPTS_ICON: &str = "titlebar/stack-push.svg";
const TERMINAL_AGENT_BAR_CHAT_VIEW_ICON: &str = "titlebar/message-circle.svg";
const TERMINAL_AGENT_BAR_EXPORT_TRANSCRIPT_ICON: &str = "titlebar/file-export.svg";
const TERMINAL_AGENT_BAR_COPY_SESSION_ID_ICON: &str = "titlebar/copy.svg";

/// Which pane surface a bar belongs to. Both show Agents workspace sessions and
/// both resolve into the same shell-session id space, but they focus through
/// different entry points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalAgentBarSurface {
    AgentsPane(WorkspacePaneId),
    ProjectEditorCompanion(TitlebarMode),
}

impl TerminalAgentBarSurface {
    fn element_id_suffix(self, session_id: TerminalSessionId) -> String {
        match self {
            Self::AgentsPane(pane_id) => format!("agents-{}-{}", pane_id.0, session_id.0),
            Self::ProjectEditorCompanion(mode) => {
                format!("companion-{}-{}", mode.element_slug(), session_id.0)
            }
        }
    }
}

/// One control on the bar or one row in its ⋯ menu. Every enabled variant routes
/// into the pre-existing `TerminalViewEvent` flow, so the actions the old
/// overlay cluster performed are unchanged.
///
/// `VerboseMode` is the one variant with no terminal behaviour at all. It is
/// here so the ⋯ menu is row-for-row the chat composer's menu, and it renders
/// disabled (see [`GhostexGpuiApp::terminal_agent_bar_action_disabled_reason`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalAgentBarAction {
    ToggleMenu,
    SessionNote,
    StashedPrompts,
    AttachPath,
    Maximize,
    ToggleChatView,
    PromptEditor,
    VerboseMode,
    DelayedActions,
    Rename,
    Sleep,
    Fork,
    FullReload,
    ExportTranscript,
}

impl TerminalAgentBarAction {
    fn id_slug(self) -> &'static str {
        match self {
            Self::ToggleMenu => "more-actions",
            Self::SessionNote => "session-note",
            Self::StashedPrompts => "stashed-prompts",
            Self::AttachPath => "attach-path",
            Self::Maximize => "maximize",
            Self::ToggleChatView => "chat-view",
            Self::PromptEditor => "prompt-editor",
            Self::VerboseMode => "verbose-mode",
            Self::DelayedActions => "delayed-actions",
            Self::Rename => "rename",
            Self::Sleep => "sleep",
            Self::Fork => "fork",
            Self::FullReload => "full-reload",
            Self::ExportTranscript => "export-transcript",
        }
    }

    /// Label plus the hotkey action id the shared tooltip resolver reads, so the
    /// bar shows the same "Label (⌘R)" strings the overlay cluster did.
    fn label_and_hotkey_action_id(self) -> (&'static str, &'static str) {
        match self {
            Self::ToggleMenu => ("More actions", "toggleAgentActions"),
            Self::SessionNote => ("Session note", "sessionNote"),
            Self::StashedPrompts => ("Saved prompts", "stashedPrompts"),
            Self::AttachPath => ("Attach a file or folder", "attachFileOrFolder"),
            Self::Maximize => (
                "Maximize pane",
                terminal_element::TERMINAL_OVERLAY_FOCUS_MODE_HOTKEY_ACTION_ID,
            ),
            Self::ToggleChatView => ("Chat View", "toggleChatView"),
            Self::PromptEditor => ("Prompt editor", "promptEditor"),
            Self::VerboseMode => ("Verbose mode", ""),
            Self::DelayedActions => ("Delayed actions", "delayedSend"),
            Self::Rename => ("Rename", "renameActiveSession"),
            Self::Sleep => ("Sleep", "sleepFocusedSession"),
            Self::Fork => ("Fork", "forkSession"),
            Self::FullReload => ("Full reload", "reloadSession"),
            Self::ExportTranscript => ("Export transcript", "exportTranscript"),
        }
    }

    fn icon_path(self) -> &'static str {
        match self {
            Self::ToggleMenu => TERMINAL_AGENT_BAR_MORE_ACTIONS_ICON,
            Self::SessionNote => TERMINAL_AGENT_BAR_SESSION_NOTE_ICON,
            Self::StashedPrompts => TERMINAL_AGENT_BAR_STASHED_PROMPTS_ICON,
            Self::AttachPath => TERMINAL_AGENT_BAR_ATTACH_PATH_ICON,
            Self::Maximize => TERMINAL_AGENT_BAR_MAXIMIZE_ICON,
            Self::ToggleChatView => TERMINAL_AGENT_BAR_CHAT_VIEW_ICON,
            Self::PromptEditor => TERMINAL_AGENT_BAR_PROMPT_EDITOR_ICON,
            Self::VerboseMode => TERMINAL_AGENT_BAR_VERBOSE_MODE_ICON,
            Self::DelayedActions => TERMINAL_AGENT_BAR_DELAYED_ACTIONS_ICON,
            Self::Rename => TERMINAL_AGENT_BAR_RENAME_ICON,
            Self::Sleep => TERMINAL_AGENT_BAR_SLEEP_ICON,
            Self::Fork => TERMINAL_AGENT_BAR_FORK_ICON,
            Self::FullReload => TERMINAL_AGENT_BAR_FULL_RELOAD_ICON,
            Self::ExportTranscript => TERMINAL_AGENT_BAR_EXPORT_TRANSCRIPT_ICON,
        }
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::StashedPrompts => TERMINAL_AGENT_BAR_STASH_ICON_SIZE,
            Self::PromptEditor => 13.5,
            _ => TERMINAL_AGENT_BAR_ICON_SIZE,
        }
    }

    fn request(self) -> Option<terminal_element::TerminalAgentActionRequest> {
        use terminal_element::TerminalAgentActionRequest as Request;

        match self {
            Self::SessionNote => Some(Request::SessionNote),
            Self::StashedPrompts => Some(Request::StashedPrompts),
            Self::ToggleChatView => Some(Request::ToggleChatView),
            Self::DelayedActions => Some(Request::DelayedActions),
            Self::Rename => Some(Request::Rename),
            Self::Sleep => Some(Request::Sleep),
            Self::Fork => Some(Request::Fork),
            Self::FullReload => Some(Request::FullReload),
            Self::ExportTranscript => Some(Request::ExportTranscript),
            Self::ToggleMenu
            | Self::AttachPath
            | Self::Maximize
            | Self::PromptEditor
            | Self::VerboseMode => None,
        }
    }
}

/*
The ⋯ menu, top to bottom. It is the chat composer's menu row for row, in the
same order, with the same icons and the same shortcut column — see
`packages/core-ui/chat/session-chat-composer-actions.tsx`, whose expanded menu
renders a "Chat" group of Verbose mode and Delayed actions, then the host's
"Agent" group, then the host's remaining actions in host-list order under no
heading at all. The headings themselves come from
[`terminal_agent_bar_menu_group_heading`], keyed off the row that opens each
block. Prompt editor is intentionally absent because its accent button is always
visible immediately beside the menu.
*/
const TERMINAL_AGENT_BAR_MENU_ROWS: &[Option<TerminalAgentBarAction>] = &[
    Some(TerminalAgentBarAction::VerboseMode),
    Some(TerminalAgentBarAction::DelayedActions),
    None,
    Some(TerminalAgentBarAction::Rename),
    Some(TerminalAgentBarAction::Sleep),
    Some(TerminalAgentBarAction::Fork),
    Some(TerminalAgentBarAction::FullReload),
    None,
    Some(TerminalAgentBarAction::ExportTranscript),
];

impl GhostexGpuiApp {
    fn agents_sidebar_session_for_terminal(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&GpuiSidebarWorkspaceTabSession> {
        let key = match self.agents_chat_local_key_for_session(session_id) {
            Some(key) => GpuiWorkspaceTerminalSessionKey::Local(key),
            None => GpuiWorkspaceTerminalSessionKey::Remote(
                self.agents_chat_remote_key_for_session(session_id)?,
            ),
        };
        self.sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_deref()?
            .iter()
            .find(|session| session.key == key)
    }

    pub(crate) fn agents_session_agent_session_id(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&str> {
        self.agents_sidebar_session_for_terminal(session_id)?
            .agent_session_id
            .as_deref()
            .map(str::trim)
            .filter(|agent_session_id| !agent_session_id.is_empty())
    }

    /// The Agents-view pane bar. `None` for panes that are not showing an agent
    /// session's terminal, which is also every pane whose body is a chat
    /// surface (the chat composer carries these controls itself) or a native
    /// libghostty view (a GPUI-painted popup cannot draw above an AppKit host).
    pub(crate) fn render_agents_pane_terminal_agent_action_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        self.render_terminal_agent_action_bar(
            TerminalAgentBarSurface::AgentsPane(leaf.pane_id),
            session_id,
            cx,
        )
    }

    /// The project-editor companion terminal shows the same Agents sessions in
    /// its own top/bottom split, and had the same overlay cluster, so it gets
    /// the same bar.
    pub(crate) fn render_project_editor_companion_terminal_agent_action_bar(
        &self,
        mode: TitlebarMode,
        session_id: Option<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = session_id?;
        self.render_terminal_agent_action_bar(
            TerminalAgentBarSurface::ProjectEditorCompanion(mode),
            session_id,
            cx,
        )
    }

    fn render_terminal_agent_action_bar(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return None;
        }
        if !self.agents_gpui_engine_terminals.contains_key(&session_id) {
            return None;
        }
        let presentation_session = self.agents_sidebar_session_for_terminal(session_id)?;
        let agent_name = terminal_agent_bar_agent_name(presentation_session);
        let agent_session_id = presentation_session
            .agent_session_id
            .as_deref()
            .map(str::trim)
            .filter(|agent_session_id| !agent_session_id.is_empty());
        if agent_name.is_none()
            && presentation_session.agent_icon.is_none()
            && agent_session_id.is_none()
        {
            return None;
        }
        let has_session_note = presentation_session.has_session_note;
        let stashed_prompt_count = presentation_session.stashed_prompt_count;
        let full_session_id = agent_session_id.map(str::to_string);
        let menu_open = self.agents_terminal_action_bar_menu_session == Some(session_id);
        let suffix = surface.element_id_suffix(session_id);

        Some(
            h_flex()
                .id(format!("ghostex-gpui-terminal-agent-bar-{suffix}"))
                .flex_shrink_0()
                .w_full()
                .justify_center()
                .px(px(TERMINAL_AGENT_BAR_OUTER_PADDING))
                .pb(px(TERMINAL_AGENT_BAR_BOTTOM_INSET))
                .child(
                    h_flex()
                        .w_full()
                        .max_w(px(TERMINAL_AGENT_BAR_MAX_CONTENT_WIDTH
                            - 2.0 * TERMINAL_AGENT_BAR_OUTER_PADDING))
                        .h(px(TERMINAL_AGENT_BAR_HEIGHT))
                        .px(px(TERMINAL_AGENT_BAR_INNER_PADDING))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(TERMINAL_AGENT_BAR_BORDER_COLOR))
                        .bg(rgb(TERMINAL_AGENT_BAR_BACKGROUND))
                        .items_center()
                        .gap(px(TERMINAL_AGENT_BAR_BUTTON_GAP))
                        .when_some(full_session_id, |this, full_session_id| {
                            this.child(terminal_agent_bar_session_id(full_session_id, &suffix, cx))
                        })
                        .child(div().flex_1().min_w_0())
                        .child(self.render_terminal_agent_bar_menu_anchor(
                            surface, session_id, menu_open, &suffix, cx,
                        ))
                        .child(self.render_terminal_agent_bar_icon_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::SessionNote,
                            has_session_note,
                            0,
                            &suffix,
                            None,
                            cx,
                        ))
                        .child(self.render_terminal_agent_bar_icon_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::StashedPrompts,
                            false,
                            stashed_prompt_count,
                            &suffix,
                            None,
                            cx,
                        ))
                        .child(self.render_terminal_agent_bar_icon_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::AttachPath,
                            false,
                            0,
                            &suffix,
                            None,
                            cx,
                        ))
                        .child(self.render_terminal_agent_bar_icon_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::Maximize,
                            false,
                            0,
                            &suffix,
                            None,
                            cx,
                        ))
                        .child(self.render_terminal_agent_bar_icon_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::ToggleChatView,
                            false,
                            0,
                            &suffix,
                            agent_name.as_deref(),
                            cx,
                        ))
                        .child(terminal_agent_bar_accent_button(
                            surface,
                            session_id,
                            TerminalAgentBarAction::PromptEditor,
                            &suffix,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    /*
    What a control looks like right now. Maximize varies with pane state, and
    Chat View varies with agent support and session readiness; the rest resolve
    to their static label/icon. Keeping all of them on one path means a control
    can never be drawn enabled while its handler refuses to run it, which is the
    failure the old hard-coded `is_enabled` list invited.

    In Agents, Maximize is the workspace's reversible pane Focus mode. In a
    project-editor companion it is a route into Agents focused on that exact
    session; the matching Agents button becomes Minimize and restores the
    remembered view and companion slot.
    */
    fn terminal_agent_bar_action_state(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        action: TerminalAgentBarAction,
        agent_name: Option<&str>,
    ) -> TerminalAgentBarActionState {
        let (label, hotkey_action_id) = action.label_and_hotkey_action_id();
        let mut state = TerminalAgentBarActionState {
            disabled_reason: None,
            hotkey_action_id,
            icon_path: action.icon_path(),
            label,
            tooltip_override: None,
        };
        match action {
            TerminalAgentBarAction::VerboseMode => {
                // Verbose mode chooses how much of a turn the chat transcript
                // renders. A terminal shows the agent's own output verbatim, so
                // there is nothing for it to expand or collapse.
                state.disabled_reason = Some("chat view only".to_string());
            }
            TerminalAgentBarAction::ToggleChatView => {
                if self
                    .agents_session_chat_transcript_agent(session_id)
                    .is_none()
                {
                    let agent_name = agent_name.unwrap_or("This agent");
                    state.disabled_reason = Some(format!(
                        "{agent_name} isn't supported by Ghostex Chat View yet\nOnly Claude, Codex, Pi, Omp, and Grok are supported\nPlease request other agents on X or the Discord"
                    ));
                } else if !self.agents_session_chat_eligible(session_id) {
                    let agent_name = agent_name.unwrap_or("this agent");
                    state.tooltip_override = Some(format!(
                        "Install hooks for {agent_name} to enable Chat View\nClick to open Settings > Agents"
                    ));
                }
            }
            TerminalAgentBarAction::Maximize => {
                if let TerminalAgentBarSurface::AgentsPane(pane_id) = surface {
                    let restoring_companion = self
                        .terminal_agent_bar_companion_focus_return
                        .is_some_and(|target| target.session_id == session_id);
                    if restoring_companion || self.agents_workspace.focus_mode_pane == Some(pane_id)
                    {
                        state.icon_path = TERMINAL_AGENT_BAR_EXIT_MAXIMIZE_ICON;
                        state.label = "Exit maximize";
                    } else if self.agents_workspace.focus_mode_eligible_leaf_count() <= 1 {
                        state.disabled_reason = Some("Session is not in a split pane".to_string());
                    }
                } else {
                    // The global Agents Focus-mode chord is inert while a
                    // project view owns the workarea; only advertise the
                    // companion button's click action here.
                    state.hotkey_action_id = "";
                }
            }
            _ => {}
        }
        state
    }

    fn terminal_agent_bar_action_enabled(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        action: TerminalAgentBarAction,
    ) -> bool {
        let agent_name = self
            .agents_sidebar_session_for_terminal(session_id)
            .and_then(terminal_agent_bar_agent_name);
        self.terminal_agent_bar_action_state(surface, session_id, action, agent_name.as_deref())
            .disabled_reason
            .is_none()
    }

    /// The ⋯ button plus, when open, its popup menu. The menu is an absolutely
    /// positioned child of this 28px anchor, exactly like the design
    /// reference's `.menu-anchor`, so it opens upward from the button with no
    /// coordinate arithmetic and stays clipped to the pane.
    fn render_terminal_agent_bar_menu_anchor(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        menu_open: bool,
        suffix: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .flex_shrink_0()
            .size(px(TERMINAL_AGENT_BAR_BUTTON_SIZE))
            .child(self.render_terminal_agent_bar_icon_button(
                surface,
                session_id,
                TerminalAgentBarAction::ToggleMenu,
                false,
                0,
                suffix,
                None,
                cx,
            ))
            .when(menu_open, |this| {
                // Dismissal lives on the anchor, not the menu: the ⋯ button is
                // inside the anchor, so its own click never counts as an
                // outside press and the toggle keeps working. Anything else,
                // including a menu row, closes the menu.
                this.on_mouse_down_out(cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    this.close_terminal_agent_action_bar_menu(cx);
                }))
                .child(self.render_terminal_agent_bar_menu(surface, session_id, suffix, cx))
            })
            .into_any_element()
    }

    fn render_terminal_agent_bar_menu(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        suffix: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let mut menu = div()
            .id(format!("ghostex-gpui-terminal-agent-bar-menu-{suffix}"))
            .absolute()
            .right_0()
            .bottom(px(
                TERMINAL_AGENT_BAR_BUTTON_SIZE + TERMINAL_AGENT_BAR_MENU_GAP
            ))
            .w(px(TERMINAL_AGENT_BAR_MENU_WIDTH))
            .flex()
            .flex_col()
            .p(px(5.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(TERMINAL_AGENT_BAR_MENU_BORDER))
            .bg(rgb(TERMINAL_AGENT_BAR_MENU_BACKGROUND))
            .shadow(vec![
                BoxShadow::new(
                    px(0.0),
                    px(10.0),
                    Rgba {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.45,
                    }
                    .into(),
                )
                .blur_radius(px(22.0)),
            ])
            .occlude();

        for row in TERMINAL_AGENT_BAR_MENU_ROWS {
            match row {
                Some(action) => {
                    if let Some(heading) = terminal_agent_bar_menu_group_heading(*action) {
                        menu = menu.child(terminal_agent_bar_menu_group_label(heading));
                    }
                    menu = menu.child(self.render_terminal_agent_bar_menu_item(
                        surface, session_id, *action, suffix, cx,
                    ));
                }
                None => {
                    menu = menu.child(terminal_agent_bar_menu_separator());
                }
            }
        }

        menu.into_any_element()
    }

    pub(crate) fn toggle_terminal_agent_action_bar_menu(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_terminal_action_bar_menu_session =
            (self.agents_terminal_action_bar_menu_session != Some(session_id))
                .then_some(session_id);
        cx.notify();
    }

    pub(crate) fn close_terminal_agent_action_bar_menu(&mut self, cx: &mut gpui::Context<Self>) {
        if self
            .agents_terminal_action_bar_menu_session
            .take()
            .is_some()
        {
            cx.notify();
        }
    }

    /// Every bar control runs through here. The pane is focused first because
    /// each downstream handler is scoped to the focused Agents session, and the
    /// action itself is emitted on the session's own `TerminalView` so it takes
    /// the exact route the removed overlay buttons took.
    fn perform_terminal_agent_bar_action(
        &mut self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        action: TerminalAgentBarAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if action == TerminalAgentBarAction::ToggleMenu {
            self.toggle_terminal_agent_action_bar_menu(session_id, cx);
            return;
        }
        self.close_terminal_agent_action_bar_menu(cx);
        if action == TerminalAgentBarAction::ToggleChatView
            && self
                .agents_session_chat_transcript_agent(session_id)
                .is_some()
            && !self.agents_session_chat_eligible(session_id)
        {
            let agent_name = self
                .agents_sidebar_session_for_terminal(session_id)
                .and_then(terminal_agent_bar_agent_name)
                .unwrap_or_else(|| "this agent".to_string());
            self.open_gpui_settings_agent_hooks_page(window, cx);
            self.dispatch_gpui_app_modal_toast(
                "warning",
                &format!("Install hooks for {agent_name}"),
                &format!(
                    "Install and approve {agent_name} hooks in order for Chat View to work correctly. Resuming and working/done indicators also require hooks."
                ),
                cx,
            );
            return;
        }
        if !self.terminal_agent_bar_action_enabled(surface, session_id, action) {
            return;
        }

        // Focus mode is a workspace-layout action rather than something the
        // session's terminal performs, so it never becomes a `TerminalViewEvent`.
        if action == TerminalAgentBarAction::Maximize {
            self.perform_terminal_agent_bar_maximize_action(surface, session_id, window, cx);
            return;
        }

        match surface {
            TerminalAgentBarSurface::AgentsPane(pane_id) => {
                self.focus_agents_terminal_mount_slot(
                    AgentsTerminalBodyMountSlotId {
                        pane_id,
                        session_id,
                    },
                    window,
                    cx,
                );
            }
            TerminalAgentBarSurface::ProjectEditorCompanion(mode) => {
                self.focus_project_editor_companion_terminal_session(mode, session_id, window, cx);
            }
        }

        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        let event = match action {
            TerminalAgentBarAction::AttachPath => {
                terminal_element::TerminalViewEvent::AttachPathsRequested
            }
            TerminalAgentBarAction::PromptEditor => {
                terminal_element::TerminalViewEvent::PromptEditorShortcutRequested
            }
            other => {
                let Some(request) = other.request() else {
                    return;
                };
                terminal_element::TerminalViewEvent::AgentActionRequested(request)
            }
        };
        view.update(cx, |_view, cx| cx.emit(event));
        cx.notify();
    }

    fn perform_terminal_agent_bar_maximize_action(
        &mut self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match surface {
            TerminalAgentBarSurface::ProjectEditorCompanion(mode) => {
                let slot = if self.project_editor_companion_terminal_session_id == Some(session_id)
                {
                    ProjectEditorCompanionTerminalSlot::Top
                } else if self.project_editor_companion_secondary_terminal_session_id
                    == Some(session_id)
                {
                    ProjectEditorCompanionTerminalSlot::Bottom
                } else {
                    return;
                };
                let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
                    return;
                };

                self.terminal_agent_bar_companion_focus_return =
                    Some(TerminalAgentBarCompanionFocusReturn {
                        mode,
                        session_id,
                        slot,
                    });
                self.agents_workspace.select_tab(pane_id, session_id);
                if self.agents_workspace.focus_mode_pane != Some(pane_id) {
                    if self.agents_workspace.focus_mode_pane.is_some() {
                        let _ = self.agents_workspace.toggle_focus_mode();
                    }
                    let _ = self.agents_workspace.toggle_focus_mode();
                }
                if self.set_active_mode(TitlebarMode::Agents, window, cx) {
                    self.focus_agents_pane(pane_id, cx);
                    cx.notify();
                } else {
                    self.terminal_agent_bar_companion_focus_return = None;
                }
            }
            TerminalAgentBarSurface::AgentsPane(pane_id) => {
                if self.restore_terminal_agent_bar_companion_focus(session_id, window, cx) {
                    return;
                }
                self.toggle_agents_focus_mode_for_pane(pane_id, cx);
            }
        }
    }

    fn restore_terminal_agent_bar_companion_focus(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(target) = self
            .terminal_agent_bar_companion_focus_return
            .filter(|target| target.session_id == session_id)
        else {
            return false;
        };
        if !self.titlebar_mode_available(target.mode)
            || !self.agents_workspace.has_session(session_id)
        {
            self.terminal_agent_bar_companion_focus_return = None;
            return false;
        }

        self.terminal_agent_bar_companion_focus_return = None;
        if self.agents_workspace.focus_mode_pane.is_some() {
            let _ = self.agents_workspace.toggle_focus_mode();
        }
        match target.slot {
            ProjectEditorCompanionTerminalSlot::Top => {
                self.project_editor_companion_terminal_session_id = Some(session_id);
            }
            ProjectEditorCompanionTerminalSlot::Bottom => {
                self.project_editor_shell.left_companion_split_enabled = true;
                self.project_editor_companion_secondary_terminal_session_id = Some(session_id);
            }
        }
        self.project_editor_companion_focused_terminal_slot = target.slot;
        if !self.set_active_mode(target.mode, window, cx) {
            return false;
        }
        self.focus_project_editor_companion_terminal_session(target.mode, session_id, window, cx);
        cx.notify();
        true
    }
}

/// The label, glyph and availability of one control at render time.
#[derive(Clone)]
struct TerminalAgentBarActionState {
    /// `None` when the control works here; otherwise the reason or complete
    /// disabled tooltip, so an inert control never leaves the user guessing.
    disabled_reason: Option<String>,
    hotkey_action_id: &'static str,
    icon_path: &'static str,
    label: &'static str,
    tooltip_override: Option<String>,
}

impl GhostexGpuiApp {
    /// A `size-7` round ghost button, the chat footer's `icon-sm` shape. A
    /// disabled action keeps the slot and the tooltip but gets no hover
    /// background and no click handler at all, so it cannot be pressed and
    /// cannot be reached from a stray propagation path either.
    fn render_terminal_agent_bar_icon_button(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        action: TerminalAgentBarAction,
        show_note_dot: bool,
        stashed_prompt_count: u64,
        suffix: &str,
        agent_name: Option<&str>,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) -> AnyElement {
        let state = self.terminal_agent_bar_action_state(surface, session_id, action, agent_name);
        let enabled = state.disabled_reason.is_none();
        terminal_agent_bar_button_base(action, &state, suffix)
            .size(px(TERMINAL_AGENT_BAR_BUTTON_SIZE))
            .when(enabled, |this| {
                this.hover(|this| this.bg(rgb(TERMINAL_AGENT_BAR_HOVER_BACKGROUND)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.perform_terminal_agent_bar_action(
                                surface, session_id, action, window, cx,
                            );
                        }),
                    )
            })
            .child(terminal_agent_bar_icon(
                state.icon_path,
                action.icon_size(),
                if enabled {
                    TERMINAL_AGENT_BAR_ICON_COLOR
                } else {
                    TERMINAL_AGENT_BAR_DISABLED_ICON_COLOR
                },
            ))
            .when(show_note_dot, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(-1.0))
                        .right(px(-1.0))
                        .size(px(TERMINAL_AGENT_BAR_NOTE_DOT_SIZE))
                        .rounded_full()
                        .bg(rgb(TERMINAL_AGENT_BAR_ACCENT_BACKGROUND)),
                )
            })
            .when(stashed_prompt_count > 0, |this| {
                this.child(terminal_agent_bar_stashed_prompt_count_badge(
                    stashed_prompt_count,
                ))
            })
            .into_any_element()
    }

    /// One ⋯ menu row: glyph, label, and the action's effective shortcut in a
    /// right-aligned column, the way the chat composer's `DropdownMenuShortcut`
    /// renders it.
    fn render_terminal_agent_bar_menu_item(
        &self,
        surface: TerminalAgentBarSurface,
        session_id: TerminalSessionId,
        action: TerminalAgentBarAction,
        suffix: &str,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) -> AnyElement {
        let state = self.terminal_agent_bar_action_state(surface, session_id, action, None);
        let enabled = state.disabled_reason.is_none();
        let shortcut = enabled
            .then(|| terminal_element::terminal_overlay_hotkey_label(state.hotkey_action_id))
            .flatten();
        let icon_color = if enabled {
            TERMINAL_AGENT_BAR_ICON_COLOR
        } else {
            TERMINAL_AGENT_BAR_DISABLED_ICON_COLOR
        };
        h_flex()
            .id(format!(
                "ghostex-gpui-terminal-agent-bar-menu-{}-{suffix}",
                action.id_slug()
            ))
            .w_full()
            .items_center()
            .gap(px(9.0))
            .px(px(9.0))
            .py(px(6.0))
            .rounded(px(7.0))
            .cursor_default()
            .text_size(px(13.0))
            .text_color(rgb(if enabled {
                TERMINAL_AGENT_BAR_MENU_TEXT_COLOR
            } else {
                TERMINAL_AGENT_BAR_DISABLED_ICON_COLOR
            }))
            .when(enabled, |this| {
                this.hover(|this| this.bg(rgb(TERMINAL_AGENT_BAR_HOVER_BACKGROUND)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.perform_terminal_agent_bar_action(
                                surface, session_id, action, window, cx,
                            );
                        }),
                    )
            })
            .child(terminal_agent_bar_icon(
                state.icon_path,
                TERMINAL_AGENT_BAR_MENU_ICON_SIZE,
                icon_color,
            ))
            .child(state.label)
            .child(div().flex_1().min_w_0())
            .when_some(shortcut, |this, shortcut| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(TERMINAL_AGENT_BAR_MENU_SHORTCUT_SIZE))
                        .text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_COLOR))
                        .child(shortcut),
                )
            })
            .into_any_element()
    }
}

/// The session id the agent runs under, plus a Copy affordance. Both halves are
/// one click target — the id text is the obvious thing to aim at, and the icon
/// is the obvious thing to aim at, so neither may be dead.
fn terminal_agent_bar_session_id(
    full_session_id: String,
    suffix: &str,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> AnyElement {
    let group = format!("ghostex-gpui-terminal-agent-bar-id-group-{suffix}");
    let copy_shortcut = terminal_element::terminal_overlay_hotkey_label("copyAgentSessionId");
    let tooltip = match copy_shortcut {
        Some(shortcut) => format!("{full_session_id}\nClick to copy ({shortcut})"),
        None => format!("{full_session_id}\nClick to copy"),
    };
    let copy_session_id = full_session_id.clone();
    h_flex()
        .id(format!("ghostex-gpui-terminal-agent-bar-id-{suffix}"))
        .group(group.clone())
        .min_w_0()
        .items_center()
        .gap(px(5.0))
        .overflow_hidden()
        .cursor_default()
        .managed_tooltip(move |window, cx| {
            Tooltip::new(tooltip.clone())
                .text_center()
                .top(px(-TERMINAL_AGENT_BAR_TOOLTIP_LIFT))
                .build(window, cx)
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(copy_session_id.clone()));
            }),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_size(px(12.0))
                .text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_COLOR))
                .group_hover(group.clone(), |this| {
                    this.text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_HOVER_COLOR))
                })
                .child(full_session_id),
        )
        .child(
            svg()
                .flex_shrink_0()
                .size(px(TERMINAL_AGENT_BAR_COPY_ICON_SIZE))
                .path(TERMINAL_AGENT_BAR_COPY_SESSION_ID_ICON)
                .text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_COLOR))
                .group_hover(group, |this| {
                    this.text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_HOVER_COLOR))
                }),
        )
        .into_any_element()
}

/// The Send button's twin: same 24px filled shape, 6px corners, dark glyph.
/// Prompt editor takes that slot because it is the terminal's "put text into
/// the session" control, so chat muscle memory stays harmless.
fn terminal_agent_bar_accent_button(
    surface: TerminalAgentBarSurface,
    session_id: TerminalSessionId,
    action: TerminalAgentBarAction,
    suffix: &str,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> AnyElement {
    let (label, hotkey_action_id) = action.label_and_hotkey_action_id();
    let state = TerminalAgentBarActionState {
        disabled_reason: None,
        hotkey_action_id,
        icon_path: action.icon_path(),
        label,
        tooltip_override: None,
    };
    terminal_agent_bar_button_base(action, &state, suffix)
        .size(px(TERMINAL_AGENT_BAR_ACCENT_BUTTON_SIZE))
        .rounded(px(TERMINAL_AGENT_BAR_ACCENT_BUTTON_RADIUS))
        .bg(rgb(TERMINAL_AGENT_BAR_ACCENT_BACKGROUND))
        .hover(|this| this.bg(rgb(TERMINAL_AGENT_BAR_ACCENT_HOVER_BACKGROUND)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                this.perform_terminal_agent_bar_action(surface, session_id, action, window, cx);
            }),
        )
        .child(
            svg()
                .flex_shrink_0()
                .size(px(action.icon_size()))
                .path(action.icon_path())
                .text_color(rgb(TERMINAL_AGENT_BAR_ACCENT_ICON_COLOR)),
        )
        .into_any_element()
}

fn terminal_agent_bar_button_base(
    action: TerminalAgentBarAction,
    state: &TerminalAgentBarActionState,
    suffix: &str,
) -> gpui::Stateful<gpui::Div> {
    // A disabled control names why it is inert instead of leaving the user to
    // guess, and drops the shortcut: naming a chord that would do nothing here
    // is worse than naming none.
    let tooltip = match state.tooltip_override.as_deref() {
        Some(tooltip) => tooltip.to_string(),
        None => match state.disabled_reason.as_deref() {
            Some(reason)
                if action == TerminalAgentBarAction::Maximize
                    || action == TerminalAgentBarAction::ToggleChatView =>
            {
                reason.to_string()
            }
            Some(reason) => format!("{} ({reason})", state.label),
            None => terminal_element::terminal_overlay_tooltip(state.label, state.hotkey_action_id),
        },
    };
    div()
        .id(format!(
            "ghostex-gpui-terminal-agent-bar-{}-{suffix}",
            action.id_slug()
        ))
        .relative()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_default()
        .managed_tooltip(move |window, cx| {
            Tooltip::new(tooltip.clone())
                .top(px(-TERMINAL_AGENT_BAR_TOOLTIP_LIFT))
                .build(window, cx)
        })
}

fn terminal_agent_bar_agent_name(session: &GpuiSidebarWorkspaceTabSession) -> Option<String> {
    if let Some(icon) = session.agent_icon
        && let Some(agent) = gpui_default_sidebar_agent_by_icon(icon)
    {
        return Some(agent.name.to_string());
    }
    let agent_name = session.agent_name.as_deref()?.trim();
    if agent_name.is_empty() {
        return None;
    }
    let normalized_agent_name = agent_name.to_ascii_lowercase();
    Some(
        gpui_default_sidebar_agent_by_id(normalized_agent_name.as_str())
            .map(|agent| agent.name.to_string())
            .unwrap_or_else(|| agent_name.to_string()),
    )
}

fn terminal_agent_bar_stashed_prompt_count_badge(count: u64) -> AnyElement {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    h_flex()
        .absolute()
        .top(px(-1.0))
        .right(px(-1.0))
        .h(px(TERMINAL_AGENT_BAR_BADGE_HEIGHT))
        .min_w(px(TERMINAL_AGENT_BAR_BADGE_MIN_WIDTH))
        .px(px(3.0))
        .justify_center()
        .rounded_full()
        .bg(rgb(TERMINAL_AGENT_BAR_ACCENT_BACKGROUND))
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(TERMINAL_AGENT_BAR_ACCENT_ICON_COLOR))
        .child(label)
        .into_any_element()
}

/// Which heading, if any, introduces the block this row starts.
///
/// The chat composer's dots menu names exactly two of its blocks — "Chat" over
/// the chat-surface toggles (Verbose mode, Delayed actions) and "Agent" over the
/// host's session actions — and leaves its trailing host-action block unnamed;
/// see `packages/core-ui/chat/session-chat-composer-actions.tsx`. This menu
/// mirrors that, so Export transcript stays under a bare separator here too.
fn terminal_agent_bar_menu_group_heading(action: TerminalAgentBarAction) -> Option<&'static str> {
    match action {
        TerminalAgentBarAction::VerboseMode => Some("Chat"),
        TerminalAgentBarAction::Rename => Some("Agent"),
        _ => None,
    }
}

/// A heading row: text only, in the menu's flex column beside the item rows. It
/// carries no `id`, no hover style and no mouse handler, so it can never be
/// hovered, focused or clicked the way an item row can.
///
/// Sentence case at 11px against the menu's 13px rows keeps the same quiet
/// relationship `DropdownMenuLabel`'s `text-xs text-muted-foreground` has to the
/// web menu's `text-sm` rows.
fn terminal_agent_bar_menu_group_label(label: &'static str) -> AnyElement {
    div()
        .px(px(9.0))
        .pt(px(5.0))
        .pb(px(3.0))
        .text_size(px(11.0))
        .text_color(rgb(TERMINAL_AGENT_BAR_SESSION_ID_COLOR))
        .child(label)
        .into_any_element()
}

fn terminal_agent_bar_menu_separator() -> AnyElement {
    div()
        .h(px(1.0))
        .mx(px(4.0))
        .my(px(5.0))
        .bg(rgb(TERMINAL_AGENT_BAR_MENU_SEPARATOR))
        .into_any_element()
}

fn terminal_agent_bar_icon(path: &'static str, icon_size: f32, color: u32) -> AnyElement {
    svg()
        .flex_shrink_0()
        .size(px(icon_size))
        .path(path)
        .text_color(rgb(color))
        .into_any_element()
}
