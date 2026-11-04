use std::path::PathBuf;

use gpui::Entity;

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_terminal_markdown_image_reference_path(value: &str) -> Option<&str> {
    if !value.starts_with("[Image #") || !value.ends_with(')') {
        return None;
    }
    let open_paren = value.find('(')?;
    let path = value.get(open_paren + 1..value.len() - 1)?.trim();
    let path = path
        .strip_prefix('<')
        .and_then(|path| path.strip_suffix('>'))
        .unwrap_or(path)
        .trim();
    (!path.is_empty()).then_some(path)
}

/// A clicked `file://` link opens here only when its host is this computer (`ls --hyperlink` writes `file://<host>/<path>`); another computer's link is not a local path.
pub(crate) fn gpui_terminal_file_link_path(link: &str) -> Option<PathBuf> {
    let decoded_file_url;
    let path = if link
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file://"))
    {
        decoded_file_url = crate::terminal_model::file_url_local_path(
            link,
            crate::terminal_model::terminal_local_hostname().as_deref(),
        )?;
        decoded_file_url.as_str()
    } else {
        if gpui_terminal_link_has_scheme(link) && !gpui_terminal_link_is_windows_drive_path(link) {
            return None;
        }
        link
    };
    let path = gpui_terminal_file_link_path_without_coordinates(path);
    #[cfg(target_os = "windows")]
    let path = path
        .strip_prefix('/')
        .filter(|candidate| gpui_terminal_link_is_windows_drive_path(candidate))
        .unwrap_or(path);
    (!path.is_empty()).then(|| gpui_expand_terminal_link_path(path))
}

/// The `:line` or `:line:column` a terminal file link ends with (`src/main.rs:304:7`), the
/// numbers `gpui_terminal_file_link_path_without_coordinates` strips off the path.
pub(crate) fn gpui_terminal_file_link_coordinates(link: &str) -> (Option<u32>, Option<u32>) {
    let path = gpui_terminal_file_link_path_without_coordinates(link);
    let mut numbers = link[path.len()..]
        .split(':')
        .skip(1)
        .map(|number| number.parse::<u32>().ok().filter(|number| *number > 0));
    let line = numbers.next().flatten();
    let column = line.and(numbers.next().flatten());
    (line, column)
}

pub(crate) fn gpui_terminal_file_link_path_without_coordinates(path: &str) -> &str {
    let mut path = path;
    for _ in 0..2 {
        let Some((candidate, coordinate)) = path.rsplit_once(':') else {
            break;
        };
        if coordinate.is_empty() || !coordinate.bytes().all(|byte| byte.is_ascii_digit()) {
            break;
        }
        path = candidate;
    }
    path
}

/// RFC 3986 scheme prefix (alpha, then alphanumeric/`+`/`-`/`.`, then `:`)
/// splits URLs from file paths the way the macOS host does; path matches
/// like `src/file.rs:12` fail it because `/` appears before the colon.
pub(crate) fn gpui_terminal_link_has_scheme(link: &str) -> bool {
    let Some(colon) = link.find(':') else {
        return false;
    };
    let mut chars = link[..colon].chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub(crate) fn gpui_terminal_link_is_windows_drive_path(link: &str) -> bool {
    let bytes = link.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// Expand a leading `~/` to the user's home directory so home-relative
/// path links resolve like the macOS host's standardizing conversion.
pub(crate) fn gpui_expand_terminal_link_path(link: &str) -> PathBuf {
    if let Some(rest) = link.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(link)
}

/// Per-terminal runtime state reported by Ghostty OSC sequences and runtime
/// actions (window title, working directory, bell, hovered link, search).
/// Runtime-only: keyed by runtime session identity and never persisted into
/// shell-layout state.
#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiTerminalRuntimeOscState {
    pub(crate) title: Option<String>,
    pub(crate) pwd: Option<String>,
    pub(crate) bell_count: u64,
    pub(crate) hovered_link_url: Option<String>,
    pub(crate) search: Option<GpuiTerminalSearchState>,
}

pub(crate) struct PendingGpuiTerminalPasteConfirmation {
    pub(crate) text: String,
    pub(crate) view: Entity<terminal_element::TerminalView>,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_keyboard_owner_uses_docs_editor_hotkeys(owner: GpuiKeyboardOwner) -> bool {
    matches!(
        owner,
        GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
            FirstResponderCefSurface::ProjectWorkarea(ProjectWorkareaCefSurfaceSlotKey::Manage)
        ))
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_first_responder_programmatic_depth(
    gpui_root_view: *mut std::ffi::c_void,
) -> u32 {
    GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
        depths
            .borrow()
            .get(&(gpui_root_view as usize))
            .copied()
            .unwrap_or(0)
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_first_responder_transition(
    gpui_root_view: *mut std::ffi::c_void,
    responder: *mut std::ffi::c_void,
) {
    /*
    CDXC:FocusRouting 2026-07-11:
    `responder` arrives +1 retained from the AppKit KVO hook
    (GpuiCefAppKitHooks.m): responder churn is often caused by the teardown
    that deallocates the outgoing responder view, so a raw pointer would be
    dangling by the time the deferred classification below walks its
    superview chain (use-after-free on the main thread). Every path out of
    this function must balance the retain via
    GhostexGpuiReleaseRetainedResponder — after classification in the
    deferred task, or immediately when no callback target exists yet.
    */
    unsafe extern "C" {
        fn GhostexGpuiReleaseRetainedResponder(responder: *mut std::ffi::c_void);
    }
    let Some(target) = gpui_first_responder_callback_target(gpui_root_view) else {
        unsafe { GhostexGpuiReleaseRetainedResponder(responder) };
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let responder = responder as usize;
    let suppressed_by_programmatic_focus =
        gpui_first_responder_programmatic_depth(gpui_root_view) > 0;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.receive_first_responder_transition(
                    responder as *mut std::ffi::c_void,
                    suppressed_by_programmatic_focus,
                    window,
                    cx,
                );
            });
            unsafe { GhostexGpuiReleaseRetainedResponder(responder as *mut std::ffi::c_void) };
        })
        .detach();
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiFirstResponderDidChange(
    gpui_root_view: *mut std::ffi::c_void,
    responder: *mut std::ffi::c_void,
) {
    queue_gpui_first_responder_transition(gpui_root_view, responder);
}

/*
CDXC:Sidebar 2026-08-02:
The sidebar renderer cannot observe the pointer once it crosses into a native
sibling (GPUI chrome, a Ghostty terminal host, another CEF pane), so Chromium
keeps the last hovered row's :hover state — which is what pinned the hover-only
Close button on a session row after the pointer had already left — and an open
sidebar context menu never learns about clicks that land outside its document.
The AppKit sendEvent observer reports both facts here; both are forwarded into
the page through the sidebar's existing app-owned script boundary.
*/
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSidebarPointerInsideChanged(inside: bool) {
    let Some(target) = gpui_sidebar_pointer_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update(&mut async_app, |this, cx| {
                this.dispatch_gpui_sidebar_pointer_inside(inside, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSidebarOutsideMouseDown() {
    let Some(target) = gpui_sidebar_pointer_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update(&mut async_app, |this, cx| {
                this.dispatch_gpui_sidebar_dismiss_context_menus(cx);
            });
        })
        .detach();
}
