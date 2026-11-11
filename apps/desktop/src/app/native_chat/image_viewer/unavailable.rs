//! What the image preview shows in place of a picture it cannot show.

use super::super::images::ChatImageFailure;
use super::super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    transcript::text,
};
use super::window::ImageViewerWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::Value;

/// The card never grows past this, however wide the window.
const CARD_MAX_WIDTH: f32 = 460.0;
/// The air kept between the card and the window's edges on a narrow window.
const CARD_GUTTER: f32 = 24.0;

/**
 * The title and sentence for one failure reason.
 *
 * CDXC:SessionChat 2026-10-01 SEE-ALSO:
 * The reasons come from `image_read_failure_reason` in packages/gx-chat-core/src/transcript/images.rs
 * plus the renderer's own `unsupported`, `damaged` and `unavailable`; the phone draws the same words
 * in apps/mobile/app/src/chat/native/cards/ImageUnavailable.tsx.
 */
pub(in crate::app::native_chat) fn failure_copy(
    failure: &ChatImageFailure,
) -> (&'static str, String) {
    match failure.reason.as_str() {
        "missing" => (
            "Image not found",
            "The file was moved or deleted.".to_string(),
        ),
        "tooLarge" => (
            "Can't preview this image",
            "The file is empty or larger than 20 MB.".to_string(),
        ),
        "notImage" => (
            "Can't open this image",
            "The file isn't a recognized image.".to_string(),
        ),
        // The type the read said, so a picture refused for its type says which one arrived: a
        // PNG named here would mean the answer, not the file, was wrong.
        "unsupported" => (
            "Can't open this image",
            match failure.error.trim() {
                "" => "Ghostex got no picture type for this file, so it can't show it.".to_string(),
                media_type => {
                    format!("Ghostex got this file as {media_type}, which it can't show.")
                }
            },
        ),
        "damaged" => (
            "Can't open this image",
            "The file looks damaged or incomplete.".to_string(),
        ),
        "unavailable" => (
            "Image unavailable",
            "There is no file or address to load it from.".to_string(),
        ),
        _ => (
            "Can't open this image",
            if failure.error.trim().is_empty() {
                "The file couldn't be read.".to_string()
            } else {
                failure.error.trim().to_string()
            },
        ),
    }
}

impl ImageViewerWindow {
    /// One labelled neutral filled button in the card's action row.
    fn card_button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        p: &ChatAppearance,
        cx: &Context<Self>,
        action: impl Fn(&mut ImageViewerWindow, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .rounded(px(7.0))
            .border_1()
            .chat_filled(p.neutral_button_fills(), true)
            .text_size(px(12.5))
            .text_color(p.foreground)
            .chat_cursor_pointer()
            .focus_visible(|style| style.border_color(p.ring))
            .child(
                svg()
                    .path(icon)
                    .size(px(13.0))
                    .flex_shrink_0()
                    .text_color(p.foreground),
            )
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            .into_any_element()
    }

    /**
     * A left-aligned notice card centred in the preview: the crossed photo beside a title that
     * says why, the sentence under it, the file's name and its folder shortened to one line (the
     * full path in its tooltip), and the actions that can still help along the bottom right.
     *
     * CDXC:SessionChat 2026-10-01 DECISION:
     * User: a picture that cannot be shown no longer gets one window-wide line ("<path> could not
     * be shown here."). It is a card like the app's notice cards, nothing centre-aligned: the icon
     * on the left beside the title, at most 460px wide, the path shortened rather than shown
     * whole. The title names the reason (Image not found for a missing file, Can't open this image
     * for one that is there but unreadable), and it offers Copy path and, only when the file
     * exists, Open image and Show in Finder. Escape and a click outside still close the preview.
     * Supersedes the same day's centred card with the full path wrapping.
     */
    pub(super) fn unavailable_card(
        &self,
        image: &Value,
        failure: &ChatImageFailure,
        completed: Option<&'static str>,
        viewport_width: gpui::Pixels,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (title, detail) = failure_copy(failure);
        let path = text(image, "path");
        let copy_path = text(image, "copyPath");
        let shown_path = if copy_path.is_empty() {
            path.clone()
        } else {
            copy_path.clone()
        };
        let (folder, file) = split_path(&shown_path);
        let name = if file.is_empty() {
            text(image, "label")
        } else {
            file.to_string()
        };
        let folder = short_folder(folder);
        let has_file = !name.is_empty() || !folder.is_empty();
        let openable = !path.is_empty() && failure.file_exists();
        // An address on the session's machine (`is_loopback_http_url` in gx-chat-core) has no
        // folder to show.
        let locatable = openable && !path.starts_with("http://") && !path.starts_with("https://");
        let width = (f32::from(viewport_width) - CARD_GUTTER * 2.0).clamp(0.0, CARD_MAX_WIDTH);
        let surface = if p.light {
            gpui::white().opacity(0.97)
        } else {
            p.card_background.opacity(0.96)
        };
        let copied = completed == Some("Path copied");
        let mut actions = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(6.0))
            .pt(px(12.0));
        if !copy_path.is_empty() {
            actions = actions.child(self.card_button(
                "chat-image-missing-copy-path",
                if copied { "Copied" } else { "Copy path" },
                if copied {
                    "titlebar/check.svg"
                } else {
                    "titlebar/copy.svg"
                },
                p,
                cx,
                |this, cx| this.chat.update(cx, |chat, cx| chat.copy_viewer_path(cx)),
            ));
        }
        if locatable {
            actions = actions.child(self.card_button(
                "chat-image-missing-locate",
                if cfg!(target_os = "macos") {
                    "Show in Finder"
                } else {
                    "Show in folder"
                },
                "titlebar/folder-open.svg",
                p,
                cx,
                |this, cx| {
                    this.chat
                        .update(cx, |chat, cx| chat.open_viewer_file(true, cx))
                },
            ));
        }
        if openable {
            actions = actions.child(self.card_button(
                "chat-image-missing-open",
                "Open image",
                "titlebar/external-link.svg",
                p,
                cx,
                |this, cx| {
                    this.chat
                        .update(cx, |chat, cx| chat.open_viewer_file(false, cx))
                },
            ));
        }
        let full_path = gpui::SharedString::from(shown_path.clone());
        let file_block = div()
            .id("chat-image-missing-file")
            .mt(px(12.0))
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.input.opacity(0.5))
            .when(!shown_path.is_empty(), |block| {
                block.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(full_path.clone()).build(window, cx)
                })
            })
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(p.foreground)
                    .truncate()
                    .child(name),
            )
            .when(!folder.is_empty(), |block| {
                block.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .min_w_0()
                        .child(
                            svg()
                                .path("titlebar/folder.svg")
                                .size(px(12.0))
                                .flex_shrink_0()
                                .text_color(p.muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .truncate()
                                .child(folder),
                        ),
                )
            });
        div()
            .id("chat-image-viewer-unavailable")
            .w(px(width))
            .flex()
            .flex_col()
            .p(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(p.control_border)
            .bg(surface)
            .shadow_lg()
            // Clicks inside the card are the card's; only the surround dismisses.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.0))
                            .bg(p.input)
                            .child(
                                svg()
                                    .path("titlebar/photo-off.svg")
                                    .size(px(18.0))
                                    .text_color(p.muted),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(p.foreground)
                                    .child(title),
                            )
                            .child(div().text_size(px(12.5)).text_color(p.muted).child(detail)),
                    ),
            )
            .when(has_file, |card| card.child(file_block))
            .when(!copy_path.is_empty() || openable, |card| {
                card.child(actions)
            })
            .into_any_element()
    }
}

/// The folder and the file name of a path written with either separator.
fn split_path(path: &str) -> (&str, &str) {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(at) => (&trimmed[..at], &trimmed[at + 1..]),
        None => ("", trimmed),
    }
}

/// A folder name nobody reads: a content hash or a generated id.
fn is_opaque_segment(segment: &str) -> bool {
    segment.len() >= 24 && segment.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/**
 * A folder shortened to one readable line: the home folder as `~`, hash-named folders as `…`, and
 * only the first segment and the last three kept when it is still long, so
 * `/Users/me/Library/Application Support/ClipBook Rust/attachments/4e6e…` reads
 * `~/…/ClipBook Rust/attachments/…`.
 *
 * CDXC:SessionChat 2026-10-01 SEE-ALSO:
 * The phone shortens the same way in apps/mobile/app/src/chat/native/cards/ImageUnavailable.tsx
 * (`shortFolder`).
 */
pub(in crate::app::native_chat) fn short_folder(folder: &str) -> String {
    let separator = if folder.contains('\\') && !folder.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mut segments: Vec<&str> = folder.split(['/', '\\']).collect();
    // `/Users/<name>`, `/home/<name>` and `C:\Users\<name>` are the home folder on every host.
    let home_at = segments
        .iter()
        .position(|segment| matches!(*segment, "Users" | "home"))
        .filter(|at| *at <= 1 && segments.len() > at + 1);
    if let Some(at) = home_at {
        segments.splice(..at + 2, ["~"]);
    }
    let mut shortened: Vec<&str> = Vec::with_capacity(segments.len());
    for segment in segments {
        let segment = if is_opaque_segment(segment) {
            "…"
        } else {
            segment
        };
        if segment == "…" && shortened.last() == Some(&"…") {
            continue;
        }
        shortened.push(segment);
    }
    if shortened.len() > 5 {
        let tail = shortened.split_off(shortened.len() - 3);
        shortened.truncate(1);
        if tail.first() != Some(&"…") {
            shortened.push("…");
        }
        shortened.extend(tail);
    }
    shortened.join(&separator.to_string())
}
