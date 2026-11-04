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
pub(crate) const GPUI_APP_TOAST_CLOSE_SIZE: f32 = 18.0;
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
    pub(crate) copy_text: Option<String>,
    /// A button that takes the user to where the toast's problem is fixed.
    pub(crate) action: Option<GpuiAppToastAction>,
    pub(crate) loading: bool,
    pub(crate) persistent: bool,
    pub(crate) duration_ms: u64,
    pub(crate) epoch: u64,
}

/// What a toast's action button opens. The host runs it (`run_gpui_app_toast_action`) and the
/// toast closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAppToastAction {
    /// Settings > Accounts, on this agent's accounts (`claude`, `codex`) when one is named.
    OpenAccounts { provider: Option<String> },
    /// Settings > Extensions, where views are turned on.
    OpenViewSettings,
    /// The release page of the update a failed AppImage update was installing (Linux).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
}

impl GpuiAppToastAction {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::OpenAccounts { .. } => "Open Accounts",
            Self::OpenViewSettings => "Open Settings",
            Self::OpenUpdateDownloadPage => "Open download page",
            Self::NativeProjectPathAction { label, .. } | Self::RunInNewTerminal { label, .. } => {
                label
            }
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

/// What the toast says, for telling a repeat from a new toast.
fn gpui_app_toast_repeat_key(toast: &GpuiAppToast) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        toast.level.key(),
        gpui_app_toast_comparable_text(&toast.title),
        toast
            .description
            .as_deref()
            .map(gpui_app_toast_comparable_text)
            .unwrap_or_default(),
    )
}

/// CDXC:AppModal 2026-10-24 DECISION:
/// User: "i see this error appear a lot of times (should appear once, our errors in the app shouldn't keep spamming toasts like this)". A toast that says the same thing as one already on screen (same level, title and description) does not stack a second card: it takes the shown toast's place and restarts its timer. A warning or error raised again within `GPUI_APP_TOAST_REPEAT_QUIET` of the last time it was raised, after it closed or was dismissed, is not shown again. Toasts that update themselves by id (progress, Committing… then Committed) are unaffected. The desktop's toast window and the GPUI web build's toast overlay both pass every toast through here.
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
    if toast.in_progress() || shown.iter().any(|existing| existing.id == toast.id) {
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
        copy_text: None,
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
    GPUI_APP_TOAST_VERTICAL_PADDING * 2.0
        + title_height
        + description_height
        + if toast.copy_text.is_some() || toast.action.is_some() {
            34.0
        } else {
            0.0
        }
}

pub(crate) fn gpui_app_toast_stack_height(toasts: &[GpuiAppToast]) -> f32 {
    let toast_heights: f32 = toasts.iter().map(gpui_app_toast_estimated_height).sum();
    toast_heights
        + GPUI_APP_TOAST_GAP * toasts.len().saturating_sub(1) as f32
        + GPUI_APP_TOAST_CLOSE_OUTSET
        + GPUI_APP_TOAST_CLOSE_WINDOW_INSET
}

#[cfg(target_os = "macos")]
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
#[cfg(target_os = "windows")]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    super::own_gpui_popup_window(window, main_window_native_view);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

/// Makes the app-modal window an AppKit child of the main window (CDXC:Onboarding 2026-09-15 in GpuiAppToastWindowChrome.m).
#[cfg(target_os = "macos")]
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

#[cfg(not(target_os = "macos"))]
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

pub(crate) struct GpuiAppToastWindow {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) toasts: Vec<GpuiAppToast>,
    pub(crate) hovered_toast_id: Option<String>,
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
        cx.notify();
    }

    fn set_hovered_toast(&mut self, toast_id: &str, hovered: bool, cx: &mut gpui::Context<Self>) {
        if hovered {
            if self.hovered_toast_id.as_deref() != Some(toast_id) {
                self.hovered_toast_id = Some(toast_id.to_string());
                cx.notify();
            }
            return;
        }
        if self.hovered_toast_id.as_deref() == Some(toast_id) {
            self.hovered_toast_id = None;
            cx.notify();
        }
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
            .right_0()
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
            .child(titlebar_svg_icon(
                COMMAND_ICON_XMARK,
                8.0,
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

impl GpuiAppToastWindow {
    /// The toast's action button (Open Accounts): runs the action in the app and closes the toast.
    fn render_action_button(
        &self,
        toast_id: String,
        action: GpuiAppToastAction,
        ink: Hsla,
    ) -> AnyElement {
        let app = self.app.clone();
        let label = action.label().to_string();
        div()
            .id(format!("toast-action-{toast_id}"))
            .mt(px(6.0))
            .h(px(26.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(ink.opacity(0.25))
            .text_size(px(12.0))
            .text_color(ink)
            .cursor_pointer()
            .hover(move |button| button.bg(ink.opacity(0.094)))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let app = app.clone();
                let toast_id = toast_id.clone();
                let action = action.clone();
                cx.defer(move |cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.remove_gpui_app_toast(&toast_id, cx);
                        app.run_gpui_app_toast_action(&action, cx);
                    });
                });
            })
            .child(label)
            .into_any_element()
    }
}

/// The frames a toast window's frosted region is built from, collected while it draws.
type ToastFrames = std::rc::Rc<std::cell::RefCell<Vec<(Bounds<Pixels>, Pixels)>>>;

/// The toast's round × is a see-through light gray in both appearances, with a dark gray ×.
///
/// CDXC:Theming 2026-11-04 DECISION:
/// User, of the toast's close button: solid black stood out against the frosted toasts; it should be light gray so it fits the see-through UI. The fill keeps some of what is behind it in dark and light alike, its hover is a step stronger, and the × is dark gray so it still reads on the light fill.
pub(crate) fn gpui_app_toast_close_button_color(light: bool) -> Hsla {
    if light {
        rgb(0xe2e2e2).opacity(0.80).into()
    } else {
        rgb(0xc9c9c9).opacity(0.72).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_hover_color(light: bool) -> Hsla {
    if light {
        rgb(0xd2d2d2).opacity(0.92).into()
    } else {
        rgb(0xdedede).opacity(0.88).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_border_color(light: bool) -> Hsla {
    if light {
        rgb(0x000000).opacity(0.10).into()
    } else {
        rgb(0xffffff).opacity(0.28).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_icon_color(light: bool) -> Hsla {
    if light {
        rgb(0x4a4a4a).into()
    } else {
        rgb(0x2b2b2b).into()
    }
}

/// Whether toasts sit on their own blurred window.
///
/// CDXC:Theming 2026-09-27 DECISION:
/// User: "same for ones that should be glass on windows but they're not". Toasts are frosted on Windows too. Their window's blur is limited to the cards (`set_background_blur_region`): macOS masks the blur, and Windows, which can only confine a window's blur by clipping the window itself, clips the window to the cards plus the × of a hovered toast, so the close button stays visible and clickable. Supersedes the 2026-09-25 rule that kept solid cards on Windows.
pub(crate) fn toast_window_glass() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows")) && window_glass_active()
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
                let close_region =
                    (glass && cfg!(target_os = "windows")).then(|| card_frames.clone());
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
                            .mr(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                            .px(px(GPUI_APP_TOAST_HORIZONTAL_PADDING))
                            .py(px(GPUI_APP_TOAST_VERTICAL_PADDING))
                            .relative()
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
                            .child(
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
                                            .when_some(description, |text_column, description| {
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
                                            })
                                            .when_some(toast.copy_text.clone(), |column, report| {
                                                column.child(
                                                    div()
                                                        .id(format!(
                                                            "toast-copy-diagnostics-{}",
                                                            toast.id
                                                        ))
                                                        .mt(px(6.0))
                                                        .h(px(26.0))
                                                        .px(px(8.0))
                                                        .flex()
                                                        .items_center()
                                                        .rounded(px(4.0))
                                                        .border_1()
                                                        .border_color(colors.title.opacity(0.25))
                                                        .text_size(px(12.0))
                                                        .text_color(colors.title)
                                                        .cursor_pointer()
                                                        .hover(move |button| {
                                                            button.bg(colors.title.opacity(0.094))
                                                        })
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            gpui_copy_to_clipboard(
                                                                ClipboardItem::new_string(
                                                                    report.clone(),
                                                                ),
                                                                cx,
                                                            );
                                                        })
                                                        .child("Copy diagnostics"),
                                                )
                                            })
                                            .when_some(toast.action.clone(), |column, action| {
                                                column.child(self.render_action_button(
                                                    toast.id.clone(),
                                                    action,
                                                    colors.title,
                                                ))
                                            }),
                                    ),
                            ),
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
