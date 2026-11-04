//! Mermaid diagrams in the transcript: a finished ```mermaid fence, marked by the core, drawn as
//! the diagram inside the fence's card, with its source a click away and the app's larger diagram
//! viewer (zoom, pan) behind the expand button.
//!
//! CDXC:SessionChat 2026-09-30 DECISION:
//! "In the gpui chat view, we need to render mermaid charts instead of showing them as ascii": a diagram an agent writes is drawn, as the React chat's Mermaid viewer drew it until 2026-09-25.
//! CDXC:SessionChat 2026-09-30 SEE-ALSO:
//! The marks come from packages/gx-chat-core/src/transcript/native_markdown.rs (`NATIVE_MERMAID_OPEN`); the drawing is mermaid_render.rs, which the browser build (apps/gpui-web) replaces with its own; the expand button opens apps/desktop/src/app/window/mermaid_diagram_modal.rs.

use super::{appearance::ChatAppearance, fonts::CHAT_MONO, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderImage, StatefulInteractiveElement as _, Styled as _, div, img, px,
    svg,
};
use serde_json::json;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use web_time::Instant;

pub(super) const OPEN: &str = "\u{E000}mermaid";
pub(super) const CLOSE: &str = "\u{E000}/mermaid";

/// A diagram taller than this is drawn smaller to fit, down to `MIN_FIT` of its size, and scrolls
/// inside a box this tall past that.
const FIT_HEIGHT: f32 = 480.0;
/// CDXC:SessionChat 2026-09-30 WHY: React's viewer fitted every diagram into a 416px frame and let the reader zoom, but the transcript card has no zoom, and a sequence diagram of twenty messages fitted that way prints its labels at 5px. Below three quarters of its size a diagram's text stops being readable, so it scrolls instead; the expand button still opens the zoomable viewer.
const MIN_FIT: f32 = 0.75;
const PADDING: f32 = 12.0;
/// Device pixels per point the diagram is painted with, as the transcript's pictures are.
const PIXEL_RATIO: f32 = 2.0;
const MAX_RASTER_SIDE: f32 = 8192.0;
const MAX_RASTER_PIXELS: f32 = 8_000_000.0;

struct Diagram {
    svg: Arc<str>,
    width: f32,
    height: f32,
    /// The painted copy and the device width it was painted at.
    raster: Option<(u32, Arc<RenderImage>)>,
    raster_pending: bool,
    /// When a row last drew the diagram, which is what the chat's picture budget lets go by
    /// (`images.rs`, `trim`).
    drawn_at: Instant,
}

enum Entry {
    Rendering,
    Ready(Diagram),
    /// Why the diagram is shown as its source instead.
    Source(String),
}

/// The transcript's diagrams, drawn once per source and theme.
///
/// Asked for while a row renders with the view borrowed shared, so the map is behind a cell, as
/// the transcript's pictures are (`images.rs`).
#[derive(Default)]
pub(crate) struct ChatMermaidCache {
    entries: RefCell<HashMap<(String, bool), Entry>>,
    /// The blocks the reader switched to their source.
    showing_source: HashSet<String>,
}

impl ChatMermaidCache {
    /// Each painted copy, with when its diagram was last drawn and how many bytes it holds, for
    /// the chat's picture budget (`images.rs`, `trim`).
    pub(super) fn painted(&self) -> Vec<(Instant, (String, bool), usize)> {
        self.entries
            .borrow()
            .iter()
            .filter_map(|(key, entry)| match entry {
                Entry::Ready(Diagram {
                    raster: Some((_, raster)),
                    drawn_at,
                    ..
                }) => Some((
                    *drawn_at,
                    key.clone(),
                    super::images::rendered_bytes(raster),
                )),
                _ => None,
            })
            .collect()
    }

    /// Lets one painted copy go; the diagram keeps its drawing and is painted again the next
    /// time a row draws it.
    pub(super) fn take_painted(&self, key: &(String, bool)) -> Option<Arc<RenderImage>> {
        match self.entries.borrow_mut().get_mut(key) {
            Some(Entry::Ready(diagram)) => diagram.raster.take().map(|(_, raster)| raster),
            _ => None,
        }
    }

    /// Every painted diagram, for a chat view that is going away and must take its atlas tiles
    /// with it (`images.rs`, `release_chat_images_with_view`).
    pub(super) fn take_rasters(&self) -> Vec<Arc<RenderImage>> {
        self.entries
            .borrow_mut()
            .drain()
            .filter_map(|(_, entry)| match entry {
                Entry::Ready(diagram) => diagram.raster.map(|(_, raster)| raster),
                Entry::Rendering | Entry::Source(_) => None,
            })
            .collect()
    }
}

/// The diagram's Mermaid text: the fence's lines without its opening and closing runs.
pub(super) fn fence_source(fence: &str) -> String {
    let lines = fence.lines().collect::<Vec<_>>();
    let indent_of = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let indent = lines.first().map_or(0, |line| indent_of(line));
    lines
        .get(1..lines.len().saturating_sub(1))
        .unwrap_or_default()
        .iter()
        .map(|line| &line[indent_of(line).min(indent)..])
        .collect::<Vec<_>>()
        .join("\n")
}

/// The width a diagram is drawn at in the transcript: its own size at the transcript's zoom, made
/// smaller to fit `FIT_HEIGHT` but never below `MIN_FIT`. The card's width can only shrink it
/// further.
fn display_width(width: f32, height: f32, p: &ChatAppearance) -> f32 {
    let fit = (FIT_HEIGHT / height.max(1.0)).clamp(MIN_FIT, 1.0);
    width * fit * p.scale
}

/// The device width the diagram is painted at for `display_width`, within the raster limits.
fn device_width(width: f32, height: f32, display: f32) -> u32 {
    let ratio = width / height.max(1.0);
    let wanted = display * PIXEL_RATIO;
    let limit = MAX_RASTER_SIDE
        .min(MAX_RASTER_SIDE * ratio)
        .min((MAX_RASTER_PIXELS * ratio).sqrt());
    ((wanted.min(limit) / 8.0).round() * 8.0).max(8.0) as u32
}

pub(super) fn button(
    id: &'static str,
    icon: &'static str,
    label: Option<&'static str>,
    p: &ChatAppearance,
    click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    let s = p.scale;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(3.0 * s))
        .h(px(22.0 * s))
        .min_w(px(22.0 * s))
        .when(label.is_some(), |this| this.px(px(5.0 * s)))
        .rounded(px(6.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.border.opacity(0.7)))
        .child(
            svg()
                .path(icon)
                .size(px(14.0 * s))
                .text_color(p.muted)
                .flex_shrink_0(),
        )
        .when_some(label, |this, label| {
            this.child(
                div()
                    .text_size(px(11.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .child(label),
            )
        })
        // The card's own control consumes the press, so the heading row behind it does not read
        // it as a click on its own text.
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            click(cx)
        })
        .into_any_element()
}

impl NativeChatView {
    /// The drawing for `source`, starting it the first time it is asked for.
    fn mermaid_entry<R>(
        &self,
        source: &str,
        light: bool,
        cx: &Context<Self>,
        read: impl FnOnce(&Entry) -> R,
    ) -> R {
        let key = (source.to_owned(), light);
        if let Some(entry) = self.mermaid.entries.borrow_mut().get_mut(&key) {
            if let Entry::Ready(diagram) = entry {
                diagram.drawn_at = Instant::now();
            }
            return read(entry);
        }
        self.mermaid
            .entries
            .borrow_mut()
            .insert(key.clone(), Entry::Rendering);
        let text = key.0.clone();
        let task = cx
            .background_executor()
            .spawn(async move { super::mermaid_render::svg(&text, light) });
        cx.spawn(async move |this, cx| {
            let drawn = task.await;
            let _ = this.update(cx, |this, cx| {
                this.mermaid.entries.borrow_mut().insert(
                    key,
                    match drawn {
                        Ok((svg, width, height)) => Entry::Ready(Diagram {
                            svg,
                            width,
                            height,
                            raster: None,
                            raster_pending: false,
                            drawn_at: Instant::now(),
                        }),
                        Err(note) => Entry::Source(note),
                    },
                );
                this.list.remeasure();
                cx.notify();
            });
        })
        .detach();
        read(&Entry::Rendering)
    }

    /// Paints the diagram at `device` pixels wide unless that copy is there or on its way.
    fn request_mermaid_raster(&self, source: &str, light: bool, device: u32, cx: &Context<Self>) {
        let key = (source.to_owned(), light);
        let (svg, scale) = {
            let mut entries = self.mermaid.entries.borrow_mut();
            let Some(Entry::Ready(diagram)) = entries.get_mut(&key) else {
                return;
            };
            if diagram.raster_pending
                || diagram.raster.as_ref().map(|(width, _)| *width) == Some(device)
            {
                return;
            }
            diagram.raster_pending = true;
            (diagram.svg.clone(), device as f32 / diagram.width)
        };
        let task = cx
            .background_executor()
            .spawn(async move { super::mermaid_render::raster(&svg, scale) });
        cx.spawn(async move |this, cx| {
            let image = task.await;
            let _ = this.update(cx, |this, cx| {
                let old = {
                    let mut entries = this.mermaid.entries.borrow_mut();
                    let Some(entry) = entries.get_mut(&key) else {
                        return;
                    };
                    match (image, &mut *entry) {
                        (Some(image), Entry::Ready(diagram)) => {
                            diagram.raster_pending = false;
                            diagram.drawn_at = Instant::now();
                            diagram.raster.replace((device, image)).map(|(_, old)| old)
                        }
                        _ => {
                            *entry = Entry::Source(
                                "The diagram image could not be displayed.".to_string(),
                            );
                            None
                        }
                    }
                };
                if let Some(old) = old {
                    cx.drop_image(old, None);
                }
                this.trim_chat_images(cx);
                this.list.remeasure();
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_mermaid_source(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.mermaid.showing_source.remove(&key) {
            self.mermaid.showing_source.insert(key);
        }
        self.list.remeasure();
        cx.notify();
    }

    /// One marked ```mermaid fence, drawn in the card a fenced block sits in.
    pub(super) fn mermaid_card(
        &self,
        key: String,
        fence: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let source = fence_source(fence);
        let light = p.light;
        let showing_source = self.mermaid.showing_source.contains(&key);
        // What the diagram looks like right now: drawn (its image and display size), still being
        // drawn, or shown as its source with the reason.
        enum Body {
            Pending,
            Picture(Arc<RenderImage>, f32, f32),
            Source(Option<String>),
        }
        let body = self.mermaid_entry(&source, light, cx, |entry| match entry {
            Entry::Rendering => Err(None),
            Entry::Source(note) => Err(Some(note.clone())),
            Entry::Ready(diagram) => {
                let display = display_width(diagram.width, diagram.height, p);
                Ok((
                    diagram.width,
                    diagram.height,
                    display,
                    diagram.raster.as_ref().map(|(_, image)| image.clone()),
                ))
            }
        });
        let drawable = !matches!(body, Err(Some(_)));
        let body = match body {
            _ if showing_source && drawable => Body::Source(None),
            Err(Some(note)) => Body::Source(Some(note)),
            Err(None) => Body::Pending,
            Ok((width, height, display, raster)) => {
                self.request_mermaid_raster(
                    &source,
                    light,
                    device_width(width, height, display),
                    cx,
                );
                match raster {
                    Some(image) => Body::Picture(image, display, width / height.max(1.0)),
                    None => Body::Pending,
                }
            }
        };
        let chat = cx.weak_entity();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0 * s))
            .w_full()
            .pl(px(12.0 * s))
            .pr(px(6.0 * s))
            .py(px(2.0 * s))
            .border_b(px(1.0))
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(5.0 * s))
                    .text_size(px(11.0 * s))
                    .text_color(p.muted)
                    .child(
                        svg()
                            .path("modals/settings/sitemap.svg")
                            .size(px(13.0 * s))
                            .text_color(p.muted)
                            .flex_shrink_0(),
                    )
                    .child(div().min_w_0().truncate().child("mermaid")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(2.0 * s))
                    .when(drawable, |this| {
                        let chat = chat.clone();
                        let key = key.clone();
                        let (icon, label) = if showing_source {
                            ("modals/settings/sitemap.svg", "Diagram")
                        } else {
                            ("titlebar/code.svg", "Source")
                        };
                        this.child(button(
                            "mermaid-toggle-source",
                            icon,
                            Some(label),
                            p,
                            move |cx| {
                                let key = key.clone();
                                let _ =
                                    chat.update(cx, |chat, cx| chat.toggle_mermaid_source(key, cx));
                            },
                        ))
                    })
                    .when(
                        super::mermaid_render::HAS_VIEWER && matches!(body, Body::Picture(..)),
                        |this| {
                            let chat = chat.clone();
                            let source = source.clone();
                            this.child(button(
                                "mermaid-expand",
                                "files-view/t-arrows-maximize-2.svg",
                                None,
                                p,
                                move |cx| {
                                    let source = source.clone();
                                    let _ = chat.update(cx, |_, cx| {
                                        cx.emit(super::state::NativeChatEvent::Host(json!({
                                            "type": "open",
                                            "modal": "mermaidDiagram",
                                            "source": source,
                                        })));
                                    });
                                },
                            ))
                        },
                    )
                    .child({
                        let source = source.clone();
                        button("mermaid-copy", "titlebar/copy.svg", None, p, move |cx| {
                            crate::app::helpers::gpui_copy_to_clipboard(
                                ClipboardItem::new_string(source.clone()),
                                cx,
                            );
                        })
                    }),
            );
        let status = |text: String| {
            div()
                .p(px(PADDING * s))
                .text_size(px(12.0 * s))
                .text_color(p.muted)
                .child(text)
        };
        let content: AnyElement = match body {
            Body::Pending => status("Drawing diagram…".to_string()).into_any_element(),
            Body::Picture(image, display, ratio) => self.nested_scroll(
                format!("mermaid-diagram:{key}"),
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .w_full()
                    .max_h(px((FIT_HEIGHT + PADDING * 2.0) * s))
                    .p(px(PADDING * s))
                    .child(
                        div()
                            .flex_none()
                            .w_full()
                            .max_w(px(display))
                            .aspect_ratio(ratio)
                            .child(img(image).size_full()),
                    ),
            ),
            Body::Source(note) => div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .when_some(note, |this, note| this.child(status(note).pb(px(0.0))))
                .child(
                    self.nested_scroll(
                        format!("mermaid-source:{key}"),
                        div()
                            .max_h(px((FIT_HEIGHT + PADDING * 2.0) * s))
                            .p(px(PADDING * s))
                            .font_family(CHAT_MONO)
                            .text_size(px(12.0 * s))
                            .line_height(px(19.2 * s))
                            .text_color(p.prose)
                            .children(source.lines().map(|line| {
                                div().child(if line.is_empty() {
                                    " ".to_string()
                                } else {
                                    line.to_string()
                                })
                            })),
                    ),
                )
                .into_any_element(),
        };
        div()
            .id(gpui::SharedString::from(format!("mermaid:{key}")))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .bg(p.input)
            .border_1()
            .border_color(p.border)
            .rounded(px(12.0 * s))
            .child(header)
            .child(content)
            .into_any_element()
    }
}
