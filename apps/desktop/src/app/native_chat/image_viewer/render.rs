use super::super::images::ChatImageSource;
use super::super::{appearance::ChatAppearance, transcript::text};
use super::window::ImageViewerWindow;
use crate::app::native_chat::cursor::ChatCursor as _;
use base64::Engine as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window, div, img, px, svg,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement, Tooltip};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

/**
 * Three steps between the fitted size and the picture's own pixels, spaced geometrically so the
 * first click is a modest zoom and the last one is exactly 1:1: React's `zoomWidthsForImage`.
 */
pub(super) const ZOOM_LEVEL_COUNT: usize = 3;
/// What a zoom step enlarges the fitted box by when the picture itself cannot be measured.
const UNMEASURED_ZOOM_STEPS: [f32; ZOOM_LEVEL_COUNT] = [1.5, 2.25, 3.375];
/// React caps the fitted picture at 75% of the window height; the same cap keeps the toolbar clear.
const FIT_HEIGHT: f32 = 0.75;
const FIT_WIDTH: f32 = 0.9;
/// The app-bridge image transfer moves already-base64 bytes in ordered 256 KiB messages.
const SAVE_CHUNK_CHARS: usize = 256 * 1024;

/**
 * The picture's own pixel size, read from the header of the bytes the viewer already holds.
 *
 * GPUI has the decoded frame but does not hand it out, and the pixels are not wanted here anyway:
 * only the width and height decide the box the picture is painted in.
 */
fn header_size(picture: &gpui::Image) -> Option<gpui::Size<gpui::Pixels>> {
    let reader = image::ImageReader::new(std::io::Cursor::new(picture.bytes.as_slice()))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    (width > 0 && height > 0).then(|| gpui::size(px(width as f32), px(height as f32)))
}

/**
 * The whole picture, capped at the window box and never enlarged past its own pixels, which is
 * what React's `max-height` and `max-width` caps do to an `<img>`.
 */
fn fitted_size(
    natural: gpui::Size<gpui::Pixels>,
    viewport: gpui::Size<gpui::Pixels>,
) -> gpui::Size<gpui::Pixels> {
    let (width, height) = (f32::from(natural.width), f32::from(natural.height));
    let scale = (f32::from(viewport.width) * FIT_WIDTH / width)
        .min(f32::from(viewport.height) * FIT_HEIGHT / height)
        .min(1.0);
    gpui::size(px(width * scale), px(height * scale))
}

/**
 * The width each zoom step paints, geometric from the fitted width up to the picture's own pixels,
 * or `None` when 1:1 shows no more than the fitted size already does and clicking is therefore
 * pretending to zoom. React's `zoomWidthsForImage`, step for step.
 */
fn zoom_widths(
    natural: gpui::Size<gpui::Pixels>,
    viewport: gpui::Size<gpui::Pixels>,
) -> Option<[f32; ZOOM_LEVEL_COUNT]> {
    let natural_width = f32::from(natural.width);
    let fitted_width = f32::from(fitted_size(natural, viewport).width);
    if fitted_width <= 0.0 || natural_width <= fitted_width + 1.0 {
        return None;
    }
    let ratio = (natural_width / fitted_width).powf(1.0 / ZOOM_LEVEL_COUNT as f32);
    // The last step is the picture at 1:1; the ones before it are spaced evenly up to it.
    let mut widths = [natural_width; ZOOM_LEVEL_COUNT];
    for (level, width) in widths.iter_mut().enumerate().take(ZOOM_LEVEL_COUNT - 1) {
        *width = fitted_width * ratio.powi(level as i32 + 1);
    }
    Some(widths)
}

/// The size this zoom level paints the picture at.
fn painted_size(
    natural: gpui::Size<gpui::Pixels>,
    viewport: gpui::Size<gpui::Pixels>,
    zoom: usize,
) -> gpui::Size<gpui::Pixels> {
    let fitted = fitted_size(natural, viewport);
    let Some(widths) = zoom_widths(natural, viewport).filter(|_| zoom > 0) else {
        return fitted;
    };
    let width = widths[(zoom - 1).min(ZOOM_LEVEL_COUNT - 1)];
    let height = width * f32::from(natural.height) / f32::from(natural.width);
    gpui::size(px(width), px(height))
}

/**
 * CDXC:SessionChat 2026-09-19 DECISION:
 * User: right-clicking the previewed image closes the preview, and the close button's tooltip says so, shown below and to the left so it stays inside the overlay. Matches React's session-chat-image-viewer.tsx.
 */
const IMAGE_VIEWER_CLOSE_TOOLTIP: &str = "Close (or right-click the image)";

static SAVE_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl super::window::ImageViewerRequest {
    pub(super) fn current(&self) -> &Value {
        &self.images[self.index.min(self.images.len().saturating_sub(1))]
    }
}

impl ImageViewerWindow {
    /// One segment of React's joined image-actions ButtonGroup: Copy path, Save image, Copy image,
    /// each swapping its glyph for a tick once it has run.
    fn action_button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        done: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
        action: impl Fn(&mut ImageViewerWindow, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .size(px(28.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.input))
            .focus_visible(|style| style.bg(p.input))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(
                svg()
                    .path(if done { "titlebar/check.svg" } else { icon })
                    .size(px(15.0))
                    .text_color(if p.light { p.foreground } else { p.primary }),
            )
            .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            .into_any_element()
    }

    /**
     * The picture's own pixel size, or `None` when nothing here can know it.
     *
     * CDXC:SessionChat 2026-09-20 WHY:
     * GPUI lays an `img` out at the natural size of its bytes and then clamps the width and the
     * height at `max_w`/`max_h` independently, so a picture shaped differently from the fitted box
     * keeps the leftover as empty element around the painting. That margin still takes clicks: it
     * zoomed instead of dismissing, and a reader had to aim far outside the picture to close the
     * viewer. Measuring the picture here lets the element be exactly the picture.
     */
    fn natural_size(
        &mut self,
        source: &ChatImageSource,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Option<gpui::Size<gpui::Pixels>> {
        match source {
            // Vector bytes have no pixel size of their own: GPUI draws them into whatever box they
            // are given, so the fitted box is the right one and there is nothing to measure.
            ChatImageSource::Bytes(picture) if picture.format != gpui::ImageFormat::Svg => {
                if let Some((id, measured)) = self.measured
                    && id == picture.id
                {
                    return measured;
                }
                let measured = header_size(picture);
                self.measured = Some((picture.id, measured));
                measured
            }
            // An address GPUI fetches itself: its decoded frame carries the size, and asking the
            // asset cache the `img` element reads fetches nothing twice.
            ChatImageSource::Uri(url) => {
                let path = url.split(['?', '#']).next().unwrap_or(url);
                if path.to_ascii_lowercase().ends_with(".svg") {
                    return None;
                }
                let gpui::ImageSource::Resource(resource) = gpui::ImageSource::from(url.clone())
                else {
                    return None;
                };
                let data = window
                    .use_asset::<gpui::ImgResourceLoader>(&resource, cx)?
                    .ok()?;
                let size = data.size(0);
                let (width, height) = (i32::from(size.width), i32::from(size.height));
                (width > 0 && height > 0).then(|| gpui::size(px(width as f32), px(height as f32)))
            }
            _ => None,
        }
    }

    fn current_image(&self, cx: &Context<Self>) -> Option<Value> {
        self.chat
            .read(cx)
            .image_viewer
            .request
            .as_ref()
            .map(|request| request.current().clone())
    }

    fn copy_image(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.current_image(cx) else {
            return;
        };
        let bytes = self
            .chat
            .update(cx, |chat, cx| match chat.chat_image(&image, cx) {
                ChatImageSource::Bytes(bytes) => Some(bytes),
                _ => None,
            });
        if let Some(bytes) = bytes {
            cx.write_to_clipboard(gpui::ClipboardItem::new_image(&bytes));
            crate::app::helpers::gpui_play_copy_sound();
            self.chat.update(cx, |chat, cx| {
                chat.note_image_viewer_action("Image copied", cx)
            });
        }
    }

    fn copy_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .current_image(cx)
            .map(|image| text(&image, "copyPath"))
            .filter(|path| !path.is_empty())
        else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path));
        crate::app::helpers::gpui_play_copy_sound();
        self.chat.update(cx, |chat, cx| {
            chat.note_image_viewer_action("Path copied", cx)
        });
    }

    /// Hands the bytes to the host's Downloads writer, the route React's Save image also takes.
    fn save_image(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.current_image(cx) else {
            return;
        };
        self.chat.update(cx, |chat, cx| {
            let ChatImageSource::Bytes(bytes) = chat.chat_image(&image, cx) else {
                return;
            };
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes.bytes);
            let request_id = format!(
                "native-image-{}",
                SAVE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let name = text(&image, "fileName");
            chat.host(
                "saveImageStart",
                json!({
                    "requestId": request_id,
                    "suggestedName": if name.is_empty() { "image.png".to_string() } else { name },
                }),
                cx,
            );
            for (index, chunk) in encoded.as_bytes().chunks(SAVE_CHUNK_CHARS).enumerate() {
                chat.host(
                    "saveImageChunk",
                    json!({
                        "requestId": request_id,
                        "chunkIndex": index,
                        "base64Chunk": String::from_utf8_lossy(chunk),
                    }),
                    cx,
                );
            }
            chat.host("saveImageFinish", json!({ "requestId": request_id }), cx);
            chat.note_image_viewer_action("Saving image", cx);
        });
    }
}

impl Render for ImageViewerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.chat.read(cx).snapshot.clone();
        let p = ChatAppearance::current(&snapshot);
        let Some((image, zoom, completed)) = self
            .chat
            .read(cx)
            .image_viewer
            .request
            .as_ref()
            .map(|request| (request.current().clone(), request.zoom, request.completed))
        else {
            return div().size_full().into_any_element();
        };
        // The overlay owns the window's keyboard: without a focused element Escape would reach
        // nothing, which is how a viewer can look stuck open.
        if window.focused(cx).is_none() {
            self.focus.focus(window, cx);
        }
        let label = text(&image, "label");
        let copyable = !text(&image, "copyPath").is_empty();
        let source = self.chat.update(cx, |chat, cx| chat.chat_image(&image, cx));
        let loading = matches!(source, ChatImageSource::Loading);
        let viewport = window.viewport_size();
        let natural = self.natural_size(&source, window, cx);
        // Exactly the box the picture fills, whenever its own size is known.
        let painted = natural.map(|natural| painted_size(natural, viewport, zoom));
        // A picture painted at its own pixels already shows everything it has, so clicking it is
        // not a zoom; one nothing here can measure is enlarged by the steps below instead.
        let zooms = natural.is_none_or(|natural| zoom_widths(natural, viewport).is_some());
        // The caps stand in for the pictures that cannot be measured (vector bytes, an address
        // still being fetched), and a zoom step enlarges the fitted box itself.
        let step = if zoom == 0 {
            1.0
        } else {
            UNMEASURED_ZOOM_STEPS[(zoom - 1).min(ZOOM_LEVEL_COUNT - 1)]
        };
        let max_height = viewport.height * FIT_HEIGHT * step;
        let max_width = viewport.width * FIT_WIDTH * step;
        let fitted = |picture: gpui::Img| {
            match painted {
                Some(size) => picture.w(size.width).h(size.height),
                None => picture.max_h(max_height).max_w(max_width),
            }
            .object_fit(gpui::ObjectFit::Contain)
            .into_any_element()
        };
        let picture = match source {
            ChatImageSource::Uri(url) => Some(fitted(img(url))),
            ChatImageSource::Bytes(bytes) => Some(fitted(img(bytes))),
            _ => None,
        };
        let content = match picture {
            Some(picture) => div()
                .id("chat-image-viewer-picture")
                /*
                CDXC:SessionChat 2026-09-20 DECISION:
                User: the previewed picture shows a zoom cursor, zoom-in while a click enlarges it
                and zoom-out on the step that returns it to the fitted size. It is the one place in
                the GPUI chat view that changes the cursor at all, so the arrow stays on a picture
                that has nothing left to show, on the thumbnails, and everywhere else.
                SEE-ALSO: apps/desktop/src/app/native_chat/cursor.rs,
                packages/core-ui/styles/chat.css `.ghostex-chat-image-preview[data-zoom]`.
                */
                .map(|element| match (zooms, zoom >= ZOOM_LEVEL_COUNT) {
                    (false, _) => element.chat_cursor_pointer(),
                    (true, false) => element.cursor(gpui::CursorStyle::ZoomIn),
                    (true, true) => element.cursor(gpui::CursorStyle::ZoomOut),
                })
                .flex_shrink_0()
                .when_some(painted, |element, size| {
                    element.w(size.width).h(size.height)
                })
                // Clicking the picture itself steps the zoom; only the surround dismisses.
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.chat.update(cx, |chat, cx| chat.zoom_image_viewer(cx));
                    }),
                )
                .child(picture)
                .into_any_element(),
            None => div()
                .px(px(16.0))
                .py(px(12.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(p.control_border)
                .text_color(p.muted)
                .child(if loading {
                    "Loading image…".to_string()
                } else {
                    format!("{label} could not be shown here.")
                })
                .into_any_element(),
        };
        let body = div()
            .id("chat-image-viewer-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_scroll()
            .flex()
            .items_center()
            .justify_center()
            .child(content);
        let stop = |element: gpui::Div, cx: &Context<Self>| {
            element.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
        };
        // React's three actions in one joined group (`.ghostex-chat-image-preview-actions`), inset
        // from the right so the round close button sits beside them, both outside the scrolling
        // layer so zooming and panning never move them.
        let surface = if p.light {
            gpui::white().opacity(0.9)
        } else {
            gpui::black().opacity(0.5)
        };
        let toolbar = stop(
            div()
                .absolute()
                .top(px(12.0))
                .right(px(56.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(p.control_border)
                .bg(surface)
                .overflow_hidden(),
            cx,
        )
        .when(copyable, |bar| {
            bar.child(self.action_button(
                "chat-image-copy-path",
                "Copy path",
                "titlebar/link.svg",
                completed == Some("Path copied"),
                &p,
                cx,
                |this, cx| this.copy_path(cx),
            ))
        })
        .child(self.action_button(
            "chat-image-save",
            "Save image",
            "titlebar/download.svg",
            completed == Some("Saving image"),
            &p,
            cx,
            |this, cx| this.save_image(cx),
        ))
        .child(self.action_button(
            "chat-image-copy",
            "Copy image",
            "titlebar/copy.svg",
            completed == Some("Image copied"),
            &p,
            cx,
            |this, cx| this.copy_image(cx),
        ));
        let close = stop(
            div()
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .flex()
                .items_center(),
            cx,
        )
        .child(
            div()
                .id("chat-image-close")
                .role(gpui::Role::Button)
                .aria_label("Close image preview")
                .tab_index(0)
                .size(px(32.0))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(surface)
                .when(p.light, |this| this.border_1().border_color(p.border))
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.input))
                .focus_visible(|style| style.border_1().border_color(p.ring))
                .child(
                    svg()
                        .path("titlebar/x.svg")
                        .size(px(18.0))
                        .text_color(if p.light { p.foreground } else { p.primary }),
                )
                .managed_tooltip_with_placement(ManagedTooltipPlacement::BelowLeft, |window, cx| {
                    Tooltip::new(IMAGE_VIEWER_CLOSE_TOOLTIP).build(window, cx)
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.chat.update(cx, |chat, cx| chat.close_image_viewer(cx));
                })),
        );
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .font_family(p.font.clone())
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(gpui::Hsla::from(gpui::rgb(0x000000)).opacity(0.7))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.chat.update(cx, |chat, cx| chat.close_image_viewer(cx));
                }),
            )
            // Right-clicking anywhere in the preview, the picture included, closes it.
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.chat.update(cx, |chat, cx| chat.close_image_viewer(cx));
                }),
            )
            // Captured, not bubbled: a focused action button must never swallow Escape.
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let handled = match event.keystroke.key.as_str() {
                    "escape" => {
                        this.chat.update(cx, |chat, cx| chat.close_image_viewer(cx));
                        true
                    }
                    "left" | "up" => {
                        this.chat
                            .update(cx, |chat, cx| chat.step_image_viewer(-1, cx));
                        true
                    }
                    "right" | "down" => {
                        this.chat
                            .update(cx, |chat, cx| chat.step_image_viewer(1, cx));
                        true
                    }
                    "enter" | "space" => {
                        this.chat.update(cx, |chat, cx| chat.zoom_image_viewer(cx));
                        true
                    }
                    _ => false,
                };
                if handled {
                    cx.stop_propagation();
                    window.prevent_default();
                }
            }))
            .child(body)
            .child(toolbar)
            .child(close)
            .into_any_element()
    }
}
