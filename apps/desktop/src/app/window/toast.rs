// C1 wave-2 extraction: the GpuiAppToastWindow entity, its toast-stack model, and consts moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) const GPUI_APP_TOAST_WIDTH: f32 = 356.0;
pub(crate) const GPUI_APP_TOAST_GAP: f32 = 10.0;
pub(crate) const GPUI_APP_TOAST_BOTTOM_MARGIN: f32 = 47.0;
pub(crate) const GPUI_APP_TOAST_MAX_VISIBLE: usize = 4;
pub(crate) const GPUI_APP_TOAST_DEFAULT_DURATION_MS: u64 = 8_000;
pub(crate) const GPUI_APP_TOAST_CLOSE_SIZE: f32 =
    super::round_close_button::ROUND_CLOSE_BUTTON_SIZE;
pub(crate) const GPUI_APP_TOAST_CLOSE_OUTSET: f32 = GPUI_APP_TOAST_CLOSE_SIZE / 2.0;
pub(crate) const GPUI_APP_TOAST_CLOSE_WINDOW_INSET: f32 = 5.0;
pub(crate) const GPUI_APP_TOAST_CLOSE_TOP_INSET: f32 = 5.0;
pub(crate) const GPUI_APP_TOAST_WINDOW_WIDTH: f32 =
    GPUI_APP_TOAST_WIDTH + GPUI_APP_TOAST_CLOSE_OUTSET;
pub(crate) const GPUI_APP_TOAST_WRAPPER_GAP: f32 = GPUI_APP_TOAST_GAP - GPUI_APP_TOAST_CLOSE_OUTSET;
pub(crate) const GPUI_APP_TOAST_HORIZONTAL_PADDING: f32 = 12.0;
pub(crate) const GPUI_APP_TOAST_VERTICAL_PADDING: f32 = 10.0;
pub(crate) const GPUI_APP_TOAST_CONTENT_GAP: f32 = 2.0;
pub(crate) const GPUI_APP_TOAST_TITLE_LINE_HEIGHT: f32 = 18.0;
pub(crate) const GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT: f32 = 17.0;
pub(crate) const GPUI_APP_TOAST_TITLE_CHARS_PER_LINE: usize = 36;
pub(crate) const GPUI_APP_TOAST_DESCRIPTION_CHARS_PER_LINE: usize = 40;
pub(crate) const GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID: &str = "gpui-session-chat-file-opening";
pub(crate) const GPUI_SWITCH_REQUEST_TOAST_ID: &str = "gpui-switch-request-toast";
pub(crate) const GPUI_SWITCH_REQUEST_TOAST_DURATION_MS: u64 = 8_000;

pub(crate) fn gpui_switch_request_now_ms() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiAppToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl GpuiAppToastLevel {
    pub(crate) fn from_raw(raw: Option<&str>) -> Self {
        match raw {
            Some("success") => Self::Success,
            Some("warning") => Self::Warning,
            Some("error") => Self::Error,
            _ => Self::Info,
        }
    }

    fn container_background(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0x202124f2),
            Self::Success => rgba(0x1d2b22f2),
            Self::Warning => rgba(0x33291af2),
            Self::Error => rgba(0x3a1d20f2),
        }
    }

    fn container_border(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0xffffff1f),
            Self::Success => rgba(0x4ade804d),
            Self::Warning => rgba(0xfbbf244d),
            Self::Error => rgba(0xf8717152),
        }
    }

    fn title_color(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0xe7e7eaff),
            Self::Success => rgba(0xf0fdf4ff),
            Self::Warning => rgba(0xfef3c7ff),
            Self::Error => rgba(0xfff1f2ff),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    fn indicator_color(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0x60a5faff),
            Self::Success => rgba(0x4ade80ff),
            Self::Warning => rgba(0xfbbf24ff),
            Self::Error => rgba(0xf87171ff),
        }
    }
}

/// The colours one toast paints with.
struct GpuiAppToastColors {
    background: Hsla,
    border: Hsla,
    title: Hsla,
    description: Hsla,
}

impl GpuiAppToastColors {
    /*
    CDXC:AppModal 2026-09-23 DECISION:
    User: "let's also please make the toasts that we have in the app also use the same effect that the scroll to bottom has and let's only show the x button when we hover them". Under window glass the toast window blurs what is behind each toast (limited to the toasts themselves, not the gaps between them), and each toast takes the chat composer's frosted wash and border instead of a solid tinted card; success, warning and error keep a faint wash and outline of their own colour. The × stays hidden until the toast is hovered. The opaque window keeps the solid cards.
    */
    fn resolve(level: GpuiAppToastLevel, glass: bool, light: bool) -> Self {
        if !glass {
            if light && level == GpuiAppToastLevel::Info {
                return Self {
                    background: rgba(0xfffffffa).into(),
                    border: rgba(0x0000001c).into(),
                    title: rgb(0x18181b).into(),
                    description: rgba(0x18181ba6).into(),
                };
            }
            return Self {
                background: level.container_background().into(),
                border: level.container_border().into(),
                title: level.title_color().into(),
                description: rgba(0xffffffb8).into(),
            };
        }
        let ink: Hsla = if light { gpui::black() } else { gpui::white() };
        let tint = |alpha: f32| -> Hsla { Hsla::from(level.indicator_color()).opacity(alpha) };
        let (background, border) = match level {
            GpuiAppToastLevel::Info => (
                ink.opacity(if light { 0.04 } else { 0.06 }),
                ink.opacity(0.08),
            ),
            _ => (tint(0.10), level.container_border().into()),
        };
        let title: Hsla = if light {
            match level {
                GpuiAppToastLevel::Info => rgb(0x18181b),
                GpuiAppToastLevel::Success => rgb(0x14532d),
                GpuiAppToastLevel::Warning => rgb(0x713f12),
                GpuiAppToastLevel::Error => rgb(0x7f1d1d),
            }
            .into()
        } else {
            level.title_color().into()
        };
        Self {
            background,
            border,
            title,
            description: ink.opacity(if light { 0.62 } else { 0.72 }),
        }
    }
}

#[derive(Clone)]
pub(crate) struct GpuiAppToast {
    pub(crate) id: String,
    pub(crate) level: GpuiAppToastLevel,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    /// The raw error behind Copy details. A toast with it, or with an `action`, is never drawn as
    /// a toast: it waits in the notice modal (app/notice_modal_host.rs, CDXC:AppModal 2026-11-08
    /// in window/notice_modal.rs).
    pub(crate) copy_text: Option<String>,
    /// A button that takes the user to where the toast's problem is fixed.
    pub(crate) action: Option<GpuiAppToastAction>,
    pub(crate) loading: bool,
    pub(crate) persistent: bool,
    pub(crate) duration_ms: u64,
    pub(crate) epoch: u64,
}

/// Payload carried by a switch-request toast (`GPUI_SWITCH_REQUEST_TOAST_ID`), drawn inline in the
/// bottom-center toast stack instead of the centered notice modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSwitchRequestToast {
    pub(crate) request: ghostex_gx_protocol::SwitchRequest,
    /// Remote computer id when the request came from a connected remote computer; `None` for local.
    pub(crate) machine_id: Option<String>,
    pub(crate) primary_label: String,
    pub(crate) external_url: Option<String>,
    /// The External browser switch (#762): Open and Always open the page in the default browser.
    pub(crate) external_browser: bool,
    pub(crate) icon: Option<String>,
    pub(crate) started_at_ms: u64,
    pub(crate) remaining_ms: u64,
    pub(crate) paused: bool,
}

/// Undo on a chat's toast (Seen, Not needed or Clear Inbox in the Inbox): the core command that
/// takes the press back, run in the chat that raised it (native_chat/work_cards.rs
/// `sync_inbox_undo_toast`). Drawn inline in the toast stack with its Undo button, never in the
/// notice modal (`GpuiAppToast::has_buttons`).
#[derive(Clone)]
pub(crate) struct GpuiChatUndoToast {
    pub(crate) chat: gpui::WeakEntity<crate::app::native_chat::state::NativeChatView>,
    pub(crate) command: serde_json::Value,
}

impl GpuiChatUndoToast {
    pub(crate) fn run(&self, cx: &mut gpui::App) {
        let command = self.command.clone();
        let _ = self.chat.update(cx, |chat, cx| chat.invoke(command, cx));
    }
}

impl std::fmt::Debug for GpuiChatUndoToast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuiChatUndoToast")
            .field("chat", &self.chat.entity_id())
            .field("command", &self.command)
            .finish()
    }
}

impl PartialEq for GpuiChatUndoToast {
    fn eq(&self, other: &Self) -> bool {
        self.chat.entity_id() == other.chat.entity_id() && self.command == other.command
    }
}

impl Eq for GpuiChatUndoToast {}

/// A toast from a chat (`{"type":"toast", …}`) whose `chatUndo` command makes it an Undo toast
/// for `chat`; `None` when the message has no title.
pub(crate) fn gpui_chat_undo_toast_from_bridge_message(
    message: &serde_json::Value,
    generated_id: String,
    chat: gpui::WeakEntity<crate::app::native_chat::state::NativeChatView>,
) -> Option<GpuiAppToast> {
    let mut toast = gpui_app_toast_from_bridge_message(message, generated_id)?;
    let command = message
        .get("chatUndo")
        .filter(|command| command.is_object())?;
    toast.action = Some(GpuiAppToastAction::ChatUndo(Box::new(GpuiChatUndoToast {
        chat,
        command: command.clone(),
    })));
    Some(toast)
}

/// What a notice's button opens. The host runs it (`run_gpui_app_toast_action`) and the notice
/// closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAppToastAction {
    /// A clickable bottom-center switch-request toast (`Q4705`-`Q4711`). Never routed into the
    /// centered notice modal (`GpuiAppToast::has_buttons` returns `false` for this variant).
    SwitchRequest(Box<GpuiSwitchRequestToast>),
    /// Undo on a chat's Inbox toast (`GpuiChatUndoToast`), drawn inline like `SwitchRequest`.
    ChatUndo(Box<GpuiChatUndoToast>),
    /// Settings > Accounts, on this agent's accounts (`claude`, `codex`) when one is named.
    OpenAccounts { provider: Option<String> },
    /// Settings > Extensions, where views are turned on.
    OpenViewSettings,
    /// Settings > Agents, where an agent that isn't installed is installed or another one picked
    /// (gx_store/create/agent.rs `show_agent_not_installed_notice`).
    OpenAgentSettings,
    /// The release page of the update a failed AppImage update was installing (Linux).
    #[cfg_attr(not(all(target_os = "linux", not(ghostex_web))), allow(dead_code))]
    OpenUpdateDownloadPage,
    /// A fixed native project-path bridge payload a planned toast carries
    /// (`ghostex_gx_core::ToastButton`), such as Copy Path on a remote project.
    NativeProjectPathAction {
        label: String,
        payload: serde_json::Value,
    },
    /// A command run in a new terminal split right of `pane_id`, such as a terminal extension's
    /// Install with Homebrew. `pane_id` is the Agents workspace pane (`WorkspacePaneId`).
    RunInNewTerminal {
        label: String,
        title: String,
        command: String,
        pane_id: u64,
    },
    /// Undo on a settings sync note: this computer's value of a synced setting comes back on every
    /// computer (`id` is the note's, server/src/settings_sync/).
    UndoSyncedChange { id: String },
    /// Set to 100% on the one-time Linux note that Ghostex now follows the desktop's display
    /// scale: Interface scale back to 100% (app/display_scale_offer.rs).
    #[cfg_attr(not(all(target_os = "linux", not(ghostex_web))), allow(dead_code))]
    ResetInterfaceScale,
    /// Repair on "SpaceO is stuck": SpaceO's operator recovery, run because the user clicked it
    /// (app/os_integration/spaceo_heal.rs).
    RepairSpaceo,
    /// An approval notice's Approve (`deny: false`) or Deny: GitHub #495,
    /// app/owner_security/ui.rs.
    SecurityApproval {
        request: Box<ghostex_gx_protocol::approval::ApprovalRequest>,
        deny: bool,
        /// The remote computer that asked; `None` for this computer.
        machine_id: Option<String>,
        /// That computer's name as the notice shows it; `None` for this computer.
        computer: Option<String>,
        /// Approve asks for Touch ID, Windows Hello or the password first (that computer's
        /// Settings > Security switch, or a gxserver from before plain approvals).
        require_presence: bool,
    },
    /// "Let <this computer> manage <that computer>" on a connected computer's approval notice this
    /// app can't answer yet: asks that computer to let this app in, then shows `request` again
    /// (app/owner_security/ui.rs).
    AskToManage {
        label: String,
        machine_id: String,
        request: Box<ghostex_gx_protocol::approval::ApprovalRequest>,
    },
}

impl GpuiAppToastAction {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::SwitchRequest(request) => request.primary_label.as_str(),
            Self::OpenAccounts { .. } => "Open Accounts",
            Self::OpenViewSettings => "Open Settings",
            Self::OpenAgentSettings => "Install or pick another agent",
            Self::OpenUpdateDownloadPage => "Open download page",
            Self::UndoSyncedChange { .. } | Self::ChatUndo(_) => "Undo",
            Self::ResetInterfaceScale => "Set to 100%",
            Self::RepairSpaceo => "Repair",
            Self::SecurityApproval { deny: false, .. } => "Approve",
            Self::SecurityApproval { deny: true, .. } => "Deny",
            Self::NativeProjectPathAction { label, .. }
            | Self::RunInNewTerminal { label, .. }
            | Self::AskToManage { label, .. } => label,
        }
    }
}

/// How long a warning or error toast stays quiet when the same one is raised again after it
/// closed. Measured from the last time it was raised, so a refusal repeated every few seconds
/// stays quiet for as long as it keeps repeating.
pub(crate) const GPUI_APP_TOAST_REPEAT_QUIET: std::time::Duration =
    std::time::Duration::from_secs(10);

thread_local! {
    /// The toasts raised in the last `GPUI_APP_TOAST_REPEAT_QUIET`, by `gpui_app_toast_repeat_key`.
    static RECENT_APP_TOASTS: std::cell::RefCell<Vec<(String, web_time::Instant)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A toast id that starts with this names what the toast is about after it (`<prefix><scope>`),
/// and the scope is part of the toast's repeat key.
///
/// CDXC:AppModal 2026-11-11 WHY:
/// Two toasts can say the same thing about different things: the Beads hooks warning of two
/// repositories whose folders share a name. Their text is kept short, so the repeat rule below
/// would hide the second one. A toast whose id carries a scope (the repository's full path and
/// computer, hashed) is a repeat only of a toast with the same text and the same scope. Toasts
/// without one compare by their text alone, exactly as before.
pub(crate) const GPUI_APP_TOAST_REPEAT_SCOPE_PREFIX: &str = "repeat-scope:";

/// What the toast says, and what it is about when its id names a scope, for telling a repeat
/// from a new toast.
fn gpui_app_toast_repeat_key(toast: &GpuiAppToast) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        toast.level.key(),
        gpui_app_toast_comparable_text(&toast.title),
        toast
            .description
            .as_deref()
            .map(gpui_app_toast_comparable_text)
            .unwrap_or_default(),
        toast
            .id
            .strip_prefix(GPUI_APP_TOAST_REPEAT_SCOPE_PREFIX)
            .unwrap_or_default(),
    )
}

/// CDXC:AppModal 2026-10-24 DECISION:
/// User: "i see this error appear a lot of times (should appear once, our errors in the app shouldn't keep spamming toasts like this)". A toast that says the same thing as one already on screen (same level, title and description) does not stack a second card: it takes the shown toast's place and restarts its timer. A warning or error raised again within `GPUI_APP_TOAST_REPEAT_QUIET` of the last time it was raised, after it closed or was dismissed, is not shown again. Toasts that update themselves by id (progress, Committing… then Committed) are unaffected. The desktop's toast window and the GPUI web build's toast overlay both pass every toast through here.
/// CDXC:AppModal 2026-11-11 WHY (PR #569):
/// A toast may opt into a repeat scope through a `repeat-scope:` id (`GPUI_APP_TOAST_REPEAT_SCOPE_PREFIX`); then only a toast with the same text and the same scope counts as the same toast, so the same Beads hooks warning about two different repositories shows twice while each repository's repeat stays quiet. Toasts without a scope follow the rule above unchanged.
/// SEE-ALSO: apps/desktop/src/app/modals/modal_messages_and_toasts.rs and apps/gpui-web/src/app/web_host/toasts.rs (`upsert_gpui_app_toast`).
///
/// The toast to show, with the id of the shown toast it repeats, or `None` when it stays quiet.
pub(crate) fn gpui_app_toast_without_repeats(
    shown: &[GpuiAppToast],
    toast: GpuiAppToast,
) -> Option<GpuiAppToast> {
    RECENT_APP_TOASTS.with(|recent| {
        gpui_app_toast_without_repeats_at(
            &mut recent.borrow_mut(),
            shown,
            toast,
            web_time::Instant::now(),
        )
    })
}

pub(crate) fn gpui_app_toast_without_repeats_at(
    recent: &mut Vec<(String, web_time::Instant)>,
    shown: &[GpuiAppToast],
    mut toast: GpuiAppToast,
    now: web_time::Instant,
) -> Option<GpuiAppToast> {
    if matches!(
        toast.action,
        Some(GpuiAppToastAction::SwitchRequest(_) | GpuiAppToastAction::ChatUndo(_))
    ) || toast.in_progress()
        || shown.iter().any(|existing| existing.id == toast.id)
    {
        return Some(toast);
    }
    recent.retain(|(_, at)| now.saturating_duration_since(*at) < GPUI_APP_TOAST_REPEAT_QUIET);
    let key = gpui_app_toast_repeat_key(&toast);
    let raised_recently = recent.iter().any(|(recent_key, _)| *recent_key == key);
    recent.retain(|(recent_key, _)| *recent_key != key);
    recent.push((key.clone(), now));
    if let Some(existing) = shown
        .iter()
        .find(|existing| !existing.in_progress() && gpui_app_toast_repeat_key(existing) == key)
    {
        toast.id = existing.id.clone();
        return Some(toast);
    }
    let quiet_level = matches!(
        toast.level,
        GpuiAppToastLevel::Warning | GpuiAppToastLevel::Error
    );
    (!(quiet_level && raised_recently)).then_some(toast)
}

impl GpuiAppToast {
    /// CDXC:AppModal 2026-10-20 DECISION:
    /// User: "make the blue dot here animate to show that it's working and add ... after commiting. do this for all similar ones please". A toast posted while work runs (an Info toast that stays until its result replaces it: Committing…, Pushing…, Creating worktree…, Loading sessions…) pulses its dot like the chat's working agents, and its title ends with "…" where the title is written. Finished, failed and plain info toasts keep a still dot and no ellipsis.
    pub(crate) fn in_progress(&self) -> bool {
        self.level == GpuiAppToastLevel::Info && (self.persistent || self.loading)
    }
}

pub(crate) fn gpui_app_toast_comparable_text(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '.' | '!' | '?')
        })
        .to_lowercase()
}

pub(crate) fn gpui_normalized_app_toast_description(
    title: &str,
    description: Option<&str>,
) -> Option<String> {
    let description = description?.trim();
    if description.is_empty() {
        return None;
    }
    if gpui_app_toast_comparable_text(title) == gpui_app_toast_comparable_text(description) {
        return None;
    }
    Some(description.to_string())
}

/// Parses the shared `createAppToastRequest` bridge payload. Action buttons are
/// not parsed because no GPUI-side producer sends them yet; add routing back to
/// the Rust store when one does.
pub(crate) fn gpui_app_toast_from_bridge_message(
    message: &serde_json::Value,
    generated_id: String,
) -> Option<GpuiAppToast> {
    let title = message
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())?
        .to_string();
    let description = gpui_normalized_app_toast_description(
        &title,
        message
            .get("description")
            .and_then(serde_json::Value::as_str),
    );
    let duration_ms = message
        .get("durationMs")
        .and_then(serde_json::Value::as_f64)
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| duration as u64)
        .unwrap_or(GPUI_APP_TOAST_DEFAULT_DURATION_MS);
    Some(GpuiAppToast {
        // The raw error behind Copy details (CDXC:Notices 2026-11-06, Q1411).
        copy_text: message
            .get("copyText")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        action: None,
        id: message
            .get("toastId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or(generated_id),
        level: GpuiAppToastLevel::from_raw(
            message.get("level").and_then(serde_json::Value::as_str),
        ),
        title,
        description,
        loading: false,
        persistent: message
            .get("persistent")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        duration_ms,
        epoch: 0,
    })
}

pub(crate) fn gpui_app_toast_wrapped_line_count(text: &str, chars_per_line: usize) -> usize {
    text.split('\n')
        .map(|line| line.chars().count().div_ceil(chars_per_line).max(1))
        .sum::<usize>()
        .max(1)
}

pub(crate) fn gpui_app_toast_estimated_height(toast: &GpuiAppToast) -> f32 {
    if let Some(GpuiAppToastAction::SwitchRequest(switch_request)) = &toast.action {
        let chars_per_line = if switch_request.external_url.is_some() {
            32
        } else {
            21
        };
        let title_height = gpui_app_toast_wrapped_line_count(&toast.title, chars_per_line) as f32
            * GPUI_APP_TOAST_TITLE_LINE_HEIGHT;
        let description_height = toast
            .description
            .as_deref()
            .map(|_| GPUI_APP_TOAST_CONTENT_GAP + GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT)
            .unwrap_or(0.0);
        let text_height = (title_height + description_height).max(26.0);
        let actions_row_height = if switch_request.external_url.is_some() {
            8.0 + 26.0
        } else {
            0.0
        };
        return GPUI_APP_TOAST_VERTICAL_PADDING * 2.0 + text_height + actions_row_height + 4.0;
    }
    let title_height =
        gpui_app_toast_wrapped_line_count(&toast.title, GPUI_APP_TOAST_TITLE_CHARS_PER_LINE) as f32
            * GPUI_APP_TOAST_TITLE_LINE_HEIGHT;
    let description_height = toast
        .description
        .as_deref()
        .map(|description| {
            GPUI_APP_TOAST_CONTENT_GAP
                + if toast.id == GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID {
                    1.0
                } else {
                    gpui_app_toast_wrapped_line_count(
                        description,
                        GPUI_APP_TOAST_DESCRIPTION_CHARS_PER_LINE,
                    ) as f32
                } * GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT
        })
        .unwrap_or(0.0);
    GPUI_APP_TOAST_VERTICAL_PADDING * 2.0 + title_height + description_height
}

pub(crate) fn gpui_app_toast_stack_height(toasts: &[GpuiAppToast]) -> f32 {
    let toast_heights: f32 = toasts.iter().map(gpui_app_toast_estimated_height).sum();
    toast_heights
        + GPUI_APP_TOAST_GAP * toasts.len().saturating_sub(1) as f32
        + GPUI_APP_TOAST_CLOSE_OUTSET
        + GPUI_APP_TOAST_CLOSE_WINDOW_INSET
}

#[cfg(all(target_os = "macos", not(ghostex_web)))]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe {
            GhostexGpuiRemoveToastPopupWindowChrome(handle.ns_view.as_ptr());
            GhostexGpuiAttachToastPopupToMainWindow(
                handle.ns_view.as_ptr(),
                main_window_native_view,
            );
        }
    }
}

/// A toast can open while Ghostex is in the background (an agent finished), when GPUI would leave
/// its pop-up unowned and topmost over the app in front; it belongs to the main window instead.
#[cfg(all(target_os = "windows", not(ghostex_web)))]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    super::own_gpui_popup_window(window, main_window_native_view);
}

#[cfg(not(any(
    all(target_os = "macos", not(ghostex_web)),
    all(target_os = "windows", not(ghostex_web))
)))]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

/// Makes the app-modal window an AppKit child of the main window (CDXC:Onboarding 2026-09-15 in GpuiAppToastWindowChrome.m).
#[cfg(all(target_os = "macos", not(ghostex_web)))]
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe {
            GhostexGpuiAttachAppModalWindowToMainWindow(
                handle.ns_view.as_ptr(),
                main_window_native_view,
            );
        }
    }
}

#[cfg(not(all(target_os = "macos", not(ghostex_web))))]
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

pub(crate) struct GpuiAppToastWindow {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) toasts: Vec<GpuiAppToast>,
    pub(crate) hovered_toast_id: Option<String>,
    /// Which button on a switch-request toast is currently hovered (`"primary"` or `"external"`),
    /// so its hotkey hint appears only while hovered.
    pub(crate) hovered_switch_button: Option<&'static str>,
    /// The toast cards the window's blurred background was last limited to.
    pub(crate) blur_region: Vec<(Bounds<Pixels>, Pixels)>,
}

impl GpuiAppToastWindow {
    pub(crate) fn set_toasts(&mut self, toasts: Vec<GpuiAppToast>, cx: &mut gpui::Context<Self>) {
        self.toasts = toasts;
        if self.hovered_toast_id.as_ref().is_some_and(|hovered_id| {
            !self
                .toasts
                .iter()
                .any(|toast| toast.id.as_str() == hovered_id.as_str())
        }) {
            self.hovered_toast_id = None;
        }
        if !self
            .toasts
            .iter()
            .any(|toast| toast.id == GPUI_SWITCH_REQUEST_TOAST_ID)
        {
            self.hovered_switch_button = None;
        }
        cx.notify();
    }

    fn set_hovered_toast(&mut self, toast_id: &str, hovered: bool, cx: &mut gpui::Context<Self>) {
        if hovered {
            if self.hovered_toast_id.as_deref() != Some(toast_id) {
                self.hovered_toast_id = Some(toast_id.to_string());
                if toast_id == GPUI_SWITCH_REQUEST_TOAST_ID {
                    let app = self.app.clone();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.pause_gpui_switch_request_toast(cx);
                        });
                    });
                }
                cx.notify();
            }
            return;
        }
        if self.hovered_toast_id.as_deref() == Some(toast_id) {
            self.hovered_toast_id = None;
            if toast_id == GPUI_SWITCH_REQUEST_TOAST_ID {
                self.hovered_switch_button = None;
                let app = self.app.clone();
                cx.defer(move |cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.resume_gpui_switch_request_toast(cx);
                    });
                });
            }
            cx.notify();
        }
    }

    fn set_hovered_switch_button(
        &mut self,
        button: &'static str,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let next = if hovered {
            Some(button)
        } else if self.hovered_switch_button == Some(button) {
            None
        } else {
            self.hovered_switch_button
        };
        if self.hovered_switch_button != next {
            self.hovered_switch_button = next;
            cx.notify();
        }
    }

    fn render_switch_request_toast_content(
        &self,
        toast: &GpuiAppToast,
        sr: &GpuiSwitchRequestToast,
        colors: &GpuiAppToastColors,
        light: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let icon_slug = sr.icon.as_deref().unwrap_or("");
        let icon_badge_bg: Hsla = if light {
            rgba(0x18181b0f).into()
        } else {
            rgba(0xffffff14).into()
        };
        let leading_icon = if let Some(path) = workspace_tab_agent_icon_path(icon_slug) {
            let accent = rgb(workspace_tab_agent_icon_accent_color(icon_slug));
            let icon_color: Hsla =
                if light && workspace_tab_agent_icon_accent_color(icon_slug) == 0xffffff {
                    rgb(0x18181b).into()
                } else {
                    accent.into()
                };
            let svg_size = (workspace_tab_agent_svg_size(icon_slug) + 2.0).min(15.0);
            div()
                .flex_shrink_0()
                .mt(px(1.0))
                .size(px(22.0))
                .rounded(px(6.0))
                .bg(icon_badge_bg)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    gpui::svg()
                        .path(path)
                        .size(px(svg_size))
                        .text_color(icon_color),
                )
                .into_any_element()
        } else {
            div()
                .flex_shrink_0()
                .mt(px(5.0))
                .size(px(8.0))
                .rounded(px(4.0))
                .bg(toast.level.indicator_color())
                .into_any_element()
        };

        let primary_hotkey = switch_request_hotkey_label("acceptSwitchRequestToast");
        let external_hotkey = switch_request_hotkey_label("openSwitchRequestExternal");
        let show_primary_hotkey = self.hovered_switch_button == Some("primary");
        let show_external_hotkey = self.hovered_switch_button == Some("external");

        let primary_bg: Hsla = if light {
            rgba(0x00000029).into()
        } else {
            rgba(0xffffff33).into()
        };
        let primary_hover_bg: Hsla = if light {
            rgba(0x00000036).into()
        } else {
            rgba(0xffffff42).into()
        };
        let primary_fg: Hsla = if light {
            rgb(0x18181b).into()
        } else {
            rgb(0xf4f4f5).into()
        };
        let secondary_bg: Hsla = if light {
            rgba(0x18181b0d).into()
        } else {
            rgba(0xffffff14).into()
        };
        let secondary_hover_bg: Hsla = if light {
            rgba(0x18181b1a).into()
        } else {
            rgba(0xffffff22).into()
        };
        let secondary_border: Hsla = if light {
            rgba(0x18181b1f).into()
        } else {
            rgba(0xffffff24).into()
        };

        let app_primary = self.app.clone();
        let primary_tooltip = match &primary_hotkey {
            Some(hotkey) => format!("{} ({hotkey})", sr.primary_label),
            None => sr.primary_label.clone(),
        };
        let primary_button = div()
            .id("ghostex-gpui-switch-request-accept")
            .flex_shrink_0()
            .h(px(26.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(primary_bg)
            .text_color(primary_fg)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .hover(move |button| button.bg(primary_hover_bg))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.set_hovered_switch_button("primary", *hovered, cx);
            }))
            .tooltip(move |window, cx| Tooltip::new(primary_tooltip.clone()).build(window, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let app = app_primary.clone();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.accept_gpui_switch_request_toast(None, cx);
                        });
                    });
                }),
            )
            .child(sr.primary_label.clone())
            .when(show_primary_hotkey, |button| {
                button.when_some(primary_hotkey.clone(), |button, hotkey| {
                    button.child(
                        div()
                            .text_size(px(10.5))
                            .text_color(primary_fg.opacity(0.72))
                            .child(hotkey),
                    )
                })
            });

        let text_column = div()
            .flex()
            .flex_col()
            .gap(px(GPUI_APP_TOAST_CONTENT_GAP))
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(GPUI_APP_TOAST_TITLE_LINE_HEIGHT))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.title)
                    .child(toast.title.clone()),
            )
            .when_some(toast.description.clone(), |column, description| {
                column.child(
                    div()
                        .w_full()
                        .text_size(px(12.0))
                        .line_height(px(GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT))
                        .text_color(colors.description)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(description),
                )
            });

        // CDXC:SwitchRequests 2026-11-26 DECISION: User (Q5021 = B, Q5022 = A, #762): an
        // agent's page open gets "Always" after Open: it opens the page as Open does and turns
        // on "Let agents open and switch pages without asking" (Settings > Browser), so later
        // page opens come on screen without this toast. An extension's view counts as a page here
        // (CDXC:Extensions 2026-11-26 in server/src/extensions/open_view.rs, #772). Neutral; Open stays the one strong
        // button. Not on a remote computer's toast: the setting is read by the
        // gxserver the agent runs on, and this one only saves this computer's settings.
        let always_button = (sr.machine_id.is_none()
            && matches!(
                sr.request.action.as_str(),
                "openBrowser" | "openBrowserPane" | "openExtensionView"
            ))
        .then(|| {
            let app_always = self.app.clone();
            div()
                .id("ghostex-gpui-switch-request-always")
                .flex_shrink_0()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(secondary_border)
                .bg(secondary_bg)
                .text_color(colors.title)
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(move |button| button.bg(secondary_hover_bg))
                .tooltip(|window, cx| {
                    Tooltip::new("Open, and open agents' pages without asking from now on")
                        .build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |_this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let app = app_always.clone();
                        cx.defer(move |cx| {
                            let _ = app.update(cx, |app, cx| {
                                app.always_accept_gpui_switch_request_toast(cx);
                            });
                        });
                    }),
                )
                .child("Always")
        });
        if sr.external_url.is_some() {
            // CDXC:SwitchRequests 2026-11-26 DECISION: User (Q5026 = A, Q5027 = A, #762): the
            // Open external button became an External browser switch at the left of the row
            // (toast-toggle.html). It is "Agents open pages in" (on = My default browser, off =
            // Ghostex Browser), saved as it flips, and Open and Always honour it. A remote
            // computer's toast flips it for that toast only: this app saves this computer's
            // settings. The `openSwitchRequestExternal` hotkey flips it.
            let app_external = self.app.clone();
            let external_on = sr.external_browser;
            let external_tooltip = match &external_hotkey {
                Some(hotkey) => format!("Open agents' pages in your default browser ({hotkey})"),
                None => "Open agents' pages in your default browser".to_string(),
            };
            let (track_bg, track_border, knob_bg): (Hsla, Hsla, Hsla) = match (external_on, light) {
                (true, true) => (
                    rgb(0x262626).into(),
                    gpui::transparent_black(),
                    rgb(0xffffff).into(),
                ),
                (true, false) => (
                    rgb(0xe5e5e5).into(),
                    gpui::transparent_black(),
                    rgb(0x171717).into(),
                ),
                (false, _) => (secondary_bg, secondary_border, colors.description),
            };
            let external_switch = div()
                .id("ghostex-gpui-switch-request-external")
                .flex_shrink_0()
                .h(px(26.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .text_color(colors.title)
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    this.set_hovered_switch_button("external", *hovered, cx);
                }))
                .tooltip(move |window, cx| Tooltip::new(external_tooltip.clone()).build(window, cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |_this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let app = app_external.clone();
                        cx.defer(move |cx| {
                            let _ = app.update(cx, |app, cx| {
                                app.toggle_gpui_switch_request_external_browser(cx);
                            });
                        });
                    }),
                )
                .child(
                    div()
                        .relative()
                        .flex_shrink_0()
                        .w(px(26.0))
                        .h(px(16.0))
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(track_border)
                        .bg(track_bg)
                        .child(
                            div()
                                .absolute()
                                .top(px(1.0))
                                .left(px(if external_on { 11.0 } else { 1.0 }))
                                .size(px(12.0))
                                .rounded(px(3.5))
                                .bg(knob_bg),
                        ),
                )
                .child("External browser")
                .when(show_external_hotkey, |button| {
                    button.when_some(external_hotkey.clone(), |button, hotkey| {
                        button.child(
                            div()
                                .text_size(px(10.5))
                                .text_color(colors.description)
                                .child(hotkey),
                        )
                    })
                });
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(9.0))
                        .child(leading_icon)
                        .child(text_column),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(6.0))
                        .child(external_switch)
                        .child(div().flex_1())
                        .child(primary_button)
                        .children(always_button),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(9.0))
                .child(leading_icon)
                .child(text_column)
                .child(primary_button)
                .children(always_button)
                .into_any_element()
        }
    }

    fn render_switch_request_countdown_bar(
        toast: &GpuiAppToast,
        sr: &GpuiSwitchRequestToast,
        light: bool,
    ) -> AnyElement {
        let total = toast.duration_ms.max(1) as f32;
        let start_fraction = (sr.remaining_ms as f32 / total).clamp(0.0, 1.0);
        let track_color: Hsla = if light {
            rgba(0x18181b12).into()
        } else {
            rgba(0xffffff14).into()
        };
        let fill_color: Hsla = if light {
            rgba(0x2563ebcc).into()
        } else {
            rgba(0x60a5fae0).into()
        };
        let fill = if sr.paused {
            div()
                .h_full()
                .w(relative(start_fraction))
                .bg(fill_color)
                .into_any_element()
        } else {
            div()
                .h_full()
                .bg(fill_color)
                .with_throttled_animation(
                    gpui::ElementId::Name(
                        format!(
                            "switch-request-countdown-{}-{}",
                            toast.epoch, sr.remaining_ms
                        )
                        .into(),
                    ),
                    std::time::Duration::from_millis(sr.remaining_ms.max(1)),
                    move |bar, phase| {
                        let fraction = (start_fraction * (1.0 - phase)).clamp(0.0, 1.0);
                        bar.w(relative(fraction))
                    },
                )
                .into_any_element()
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(2.0))
            .bg(track_color)
            .child(fill)
            .into_any_element()
    }

    /// The Undo at the end of a chat's Undo toast (`GpuiChatUndoToast`): runs the chat's undo and
    /// closes the toast. A small neutral filled button (CDXC:AppModal 2026-10-14 on
    /// `neutral_button_fills`), as the switch-request toast's buttons sit inline.
    fn render_chat_undo_button(
        &self,
        toast_id: String,
        undo: GpuiChatUndoToast,
        light: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let app = self.app.clone();
        let fills = super::native_modal_kit::palette::neutral_button_fills(light);
        let ink: gpui::Hsla = if light {
            gpui::rgb(0x18181b).into()
        } else {
            gpui::rgb(0xfcfcfc).into()
        };
        div()
            .id(format!("ghostex-gpui-app-toast-undo-{toast_id}"))
            .flex_shrink_0()
            .h(px(GPUI_APP_TOAST_TITLE_LINE_HEIGHT + 4.0))
            .mt(px(-2.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .bg(fills.rest)
            .hover(move |style| style.bg(fills.hover))
            .active(move |style| style.bg(fills.pressed))
            .cursor_pointer()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ink)
            .child("Undo")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                cx.stop_propagation();
                let app = app.clone();
                let undo = undo.clone();
                let toast_id = toast_id.clone();
                cx.defer(move |cx| {
                    undo.run(cx);
                    let _ = app.update(cx, |app, cx| {
                        app.remove_gpui_app_toast(&toast_id, cx);
                    });
                });
            }))
            .into_any_element()
    }

    /// `region` collects the button's frame into the window's frosted region, where the window is
    /// clipped to that region (Windows, see `toast_window_glass`), so the × stays visible and
    /// clickable beside its card.
    fn render_close_button(
        &self,
        toast_id: String,
        region: Option<ToastFrames>,
        light: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let app = self.app.clone();
        div()
            .id(format!("ghostex-gpui-app-toast-dismiss-{toast_id}"))
            .absolute()
            .map(|button| {
                if super::round_close_button::close_buttons_on_left() {
                    button.left_0()
                } else {
                    button.right_0()
                }
            })
            .top(px(GPUI_APP_TOAST_CLOSE_TOP_INSET))
            .flex()
            .size(px(GPUI_APP_TOAST_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(gpui_app_toast_close_button_border_color(light))
            .bg(gpui_app_toast_close_button_color(light))
            .text_color(gpui_app_toast_close_button_icon_color(light))
            .cursor_default()
            .hover(move |this| this.bg(gpui_app_toast_close_button_hover_color(light)))
            .tooltip(|window, cx| Tooltip::new("Dismiss toast").build(window, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let app = app.clone();
                    let toast_id = toast_id.clone();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.remove_gpui_app_toast(&toast_id, cx);
                        });
                    });
                }),
            )
            .child(super::round_close_button::round_close_x_icon(
                1.0,
                gpui_app_toast_close_button_icon_color(light),
            ))
            .when_some(region, |button, region| {
                button.child(
                    canvas(
                        move |bounds, _, _| {
                            region.borrow_mut().push((
                                bounds.dilate(px(1.0)),
                                px(GPUI_APP_TOAST_CLOSE_SIZE / 2.0 + 1.0),
                            ));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .into_any_element()
    }
}

/// Live formatted hotkey label for a switch-request toast button, read from settings and the
/// hotkey catalog on both desktop and gpui-web.
pub(crate) fn switch_request_hotkey_chord(action_id: &str) -> Option<String> {
    let default_key = ghostex_settings_catalog::hotkey_definition(action_id)?.default_key;
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let key = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .and_then(|hotkeys| hotkeys.get(action_id))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default_key)
        .trim();
    (!key.is_empty()).then(|| key.to_string())
}

fn switch_request_hotkey_label(action_id: &str) -> Option<String> {
    let chord = switch_request_hotkey_chord(action_id)?;
    let platform = {
        #[cfg(ghostex_web)]
        {
            if crate::terminal_element::uses_macos_text_editing() {
                ghostex_settings_catalog::Platform::MacOs
            } else {
                ghostex_settings_catalog::Platform::Linux
            }
        }
        #[cfg(not(ghostex_web))]
        {
            ghostex_settings_catalog::Platform::current()
        }
    };
    let label = ghostex_settings_catalog::hotkey_label(&chord, platform);
    (!label.trim().is_empty()).then_some(label)
}

/// The frames a toast window's frosted region is built from, collected while it draws.
type ToastFrames = std::rc::Rc<std::cell::RefCell<Vec<(Bounds<Pixels>, Pixels)>>>;

pub(crate) use super::round_close_button::{
    gpui_app_toast_close_button_border_color, gpui_app_toast_close_button_color,
    gpui_app_toast_close_button_hover_color, gpui_app_toast_close_button_icon_color,
};

/// Whether toasts sit on their own blurred window.
///
/// CDXC:Theming 2026-09-27 DECISION:
/// User: "same for ones that should be glass on windows but they're not". Toasts are frosted on Windows too. Their window's blur is limited to the cards (`set_background_blur_region`): macOS masks the blur, and Windows, which can only confine a window's blur by clipping the window itself, clips the window to the cards plus the × of a hovered toast, so the close button stays visible and clickable. Supersedes the 2026-09-25 rule that kept solid cards on Windows.
pub(crate) fn toast_window_glass() -> bool {
    cfg!(any(
        all(target_os = "macos", not(ghostex_web)),
        all(target_os = "windows", not(ghostex_web))
    )) && window_glass_active()
}

impl Render for GpuiAppToastWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hovered_toast_id = self.hovered_toast_id.clone();
        let glass = toast_window_glass();
        let light = chrome_uses_light_appearance();
        // Each card reports its painted frame here so the blurred background covers the cards
        // themselves and not the gaps or the close buttons' outset.
        let card_frames: std::rc::Rc<std::cell::RefCell<Vec<(Bounds<Pixels>, Pixels)>>> =
            Default::default();
        let region_frames = card_frames.clone();
        let toast_window = cx.entity().downgrade();
        // The × overhangs the card's top-left corner on macOS (CDXC:AppModal 2026-11-10 in
        // window/round_close_button.rs), so the card leaves its outset on that side.
        let close_on_left = super::round_close_button::close_buttons_on_left();

        div()
            .flex()
            .flex_col()
            .justify_end()
            .gap(px(GPUI_APP_TOAST_WRAPPER_GAP))
            .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
            .h_full()
            .children(self.toasts.iter().map(|toast| {
                let toast_id = toast.id.clone();
                let hover_toast_id = toast.id.clone();
                let show_close_button = hovered_toast_id.as_deref() == Some(toast.id.as_str());
                let description = toast.description.clone();
                let colors = GpuiAppToastColors::resolve(toast.level, glass, light);
                let card_frames = card_frames.clone();
                // Windows clips the window to its region, so a shown × joins it (see
                // `render_close_button`); macOS only masks the blur and leaves it out.
                let close_region = (glass && cfg!(all(target_os = "windows", not(ghostex_web))))
                    .then(|| card_frames.clone());
                let truncate_description_from_start =
                    toast.id == GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID;
                let dot = div()
                    .flex_shrink_0()
                    .mt(px((GPUI_APP_TOAST_TITLE_LINE_HEIGHT - 8.0) / 2.0))
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(toast.level.indicator_color());
                let indicator = if toast.in_progress() {
                    // The chat agent fleet's working pulse (native_chat/agent_fleet.rs).
                    dot.with_throttled_animation(
                        gpui::ElementId::Name(format!("app-toast-working-{toast_id}").into()),
                        std::time::Duration::from_millis(1600),
                        |dot, phase| {
                            let progress = if phase < 0.5 {
                                phase * 2.0
                            } else {
                                (1.0 - phase) * 2.0
                            };
                            dot.opacity(1.0 - 0.65 * progress)
                        },
                    )
                    .into_any_element()
                } else {
                    dot.into_any_element()
                };
                div()
                    .id(format!("ghostex-gpui-app-toast-wrapper-{toast_id}"))
                    .relative()
                    .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        this.set_hovered_toast(&hover_toast_id, *hovered, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(GPUI_APP_TOAST_WIDTH))
                            .mt(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                            .map(|card| {
                                if close_on_left {
                                    card.ml(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                                } else {
                                    card.mr(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                                }
                            })
                            .px(px(GPUI_APP_TOAST_HORIZONTAL_PADDING))
                            .py(px(GPUI_APP_TOAST_VERTICAL_PADDING))
                            .relative()
                            .overflow_hidden()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.background)
                            .when(glass, |card| {
                                card.child(
                                    canvas(
                                        move |bounds, _, _| {
                                            // Pinned to the padding box, so grow by the 1px
                                            // border to cover the whole card.
                                            card_frames
                                                .borrow_mut()
                                                .push((bounds.dilate(px(1.0)), px(10.0)));
                                        },
                                        |_, _, _, _| {},
                                    )
                                    // Without insets an absolute child sits at the card's
                                    // content origin, inside its padding, which put the blur
                                    // off the card by the padding.
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0(),
                                )
                            })
                            .map(|card| {
                                if let Some(GpuiAppToastAction::SwitchRequest(sr)) = &toast.action {
                                    card.child(self.render_switch_request_toast_content(
                                        toast, sr, &colors, light, cx,
                                    ))
                                    .child(Self::render_switch_request_countdown_bar(
                                        toast, sr, light,
                                    ))
                                } else {
                                    let undo = match &toast.action {
                                        Some(GpuiAppToastAction::ChatUndo(undo)) => {
                                            Some(self.render_chat_undo_button(
                                                toast.id.clone(),
                                                (**undo).clone(),
                                                light,
                                                cx,
                                            ))
                                        }
                                        _ => None,
                                    };
                                    card.child(
                                        div()
                                            .flex()
                                            .items_start()
                                            .gap(px(7.0))
                                            .child(indicator)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(GPUI_APP_TOAST_CONTENT_GAP))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .line_height(px(
                                                                GPUI_APP_TOAST_TITLE_LINE_HEIGHT,
                                                            ))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(colors.title)
                                                            .child(toast.title.clone()),
                                                    )
                                                    .when_some(
                                                        description,
                                                        |text_column, description| {
                                                            text_column.child(
                                                                div()
                                                                    .w_full()
                                                                    .text_size(px(12.0))
                                                                    .line_height(px(
                                                                        GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT,
                                                                    ))
                                                                    .text_color(colors.description)
                                                                    .when(
                                                                        truncate_description_from_start,
                                                                        |description| {
                                                                            description
                                                                                .overflow_hidden()
                                                                                .whitespace_nowrap()
                                                                                .text_ellipsis_start()
                                                                        },
                                                                    )
                                                                    .child(description),
                                                            )
                                                        },
                                                    ),
                                            )
                                            .children(undo),
                                    )
                                }
                            }),
                    )
                    .when(show_close_button, |toast_element| {
                        toast_element.child(self.render_close_button(
                            toast_id,
                            close_region,
                            light,
                            cx,
                        ))
                    })
            }))
            .when(glass, |stack| {
                // Painted after every card, so it sees all of this frame's card frames.
                stack.child(
                    canvas(
                        move |_, window, cx| {
                            let frames = region_frames.borrow().clone();
                            let changed = toast_window
                                .update(cx, |toast_window, _| {
                                    if toast_window.blur_region == frames {
                                        return false;
                                    }
                                    toast_window.blur_region = frames.clone();
                                    true
                                })
                                .unwrap_or(false);
                            if changed {
                                window.set_background_blur_region(frames);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}

#[cfg(test)]
mod repeat_tests {
    use super::*;

    fn toast(id: &str, level: GpuiAppToastLevel, title: &str, description: &str) -> GpuiAppToast {
        GpuiAppToast {
            id: id.to_string(),
            level,
            title: title.to_string(),
            description: Some(description.to_string()),
            copy_text: None,
            action: None,
            loading: false,
            persistent: false,
            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
            epoch: 0,
        }
    }

    const TITLE: &str = "Account unavailable";
    const TEXT: &str =
        "“Fix login” (Claude) is on Claude account 3 (Work), which was removed from Ghostex.";

    #[test]
    fn the_same_warning_raised_while_shown_takes_the_shown_card() {
        let mut recent = Vec::new();
        let start = web_time::Instant::now();
        let first = gpui_app_toast_without_repeats_at(
            &mut recent,
            &[],
            toast("a", GpuiAppToastLevel::Warning, TITLE, TEXT),
            start,
        )
        .expect("the first one shows");
        let shown = vec![first];
        let repeat = gpui_app_toast_without_repeats_at(
            &mut recent,
            &shown,
            toast(
                "b",
                GpuiAppToastLevel::Warning,
                TITLE,
                &format!("  {TEXT} "),
            ),
            start + std::time::Duration::from_secs(2),
        )
        .expect("a repeat refreshes the card");
        assert_eq!(
            repeat.id, "a",
            "it replaces the shown toast instead of stacking"
        );
    }

    #[test]
    fn a_warning_repeated_after_it_closed_stays_quiet_until_it_stops_repeating() {
        let mut recent = Vec::new();
        let start = web_time::Instant::now();
        let at = |seconds| start + std::time::Duration::from_secs(seconds);
        let raise = |recent: &mut Vec<_>, id: &str, seconds| {
            gpui_app_toast_without_repeats_at(
                recent,
                &[],
                toast(id, GpuiAppToastLevel::Warning, TITLE, TEXT),
                at(seconds),
            )
        };
        assert!(raise(&mut recent, "a", 0).is_some());
        // Dismissed; the same refusal every 3 seconds stays quiet for as long as it repeats.
        for (index, seconds) in [3, 6, 9, 12, 15, 18].into_iter().enumerate() {
            assert!(
                raise(&mut recent, &format!("r{index}"), seconds).is_none(),
                "repeat at {seconds}s"
            );
        }
        // Raised again long after the last repeat: the user sees it once more.
        assert!(raise(&mut recent, "later", 60).is_some());
        // A different message is never held back.
        assert!(
            gpui_app_toast_without_repeats_at(
                &mut recent,
                &[],
                toast(
                    "other",
                    GpuiAppToastLevel::Warning,
                    TITLE,
                    "Something else."
                ),
                at(61),
            )
            .is_some()
        );
    }

    #[test]
    fn info_and_progress_toasts_are_not_held_back() {
        let mut recent = Vec::new();
        let start = web_time::Instant::now();
        let copied = || toast("c", GpuiAppToastLevel::Success, "Copied", "");
        assert!(gpui_app_toast_without_repeats_at(&mut recent, &[], copied(), start).is_some());
        assert!(
            gpui_app_toast_without_repeats_at(
                &mut recent,
                &[],
                GpuiAppToast {
                    id: "c2".to_string(),
                    ..copied()
                },
                start + std::time::Duration::from_secs(1),
            )
            .is_some()
        );
        let mut progress = toast("p", GpuiAppToastLevel::Info, "Committing", "");
        progress.persistent = true;
        let shown = vec![progress.clone()];
        let again = gpui_app_toast_without_repeats_at(
            &mut recent,
            &shown,
            GpuiAppToast {
                id: "p2".to_string(),
                ..progress
            },
            start,
        )
        .expect("progress toasts update by their own id");
        assert_eq!(again.id, "p2");
    }
}
