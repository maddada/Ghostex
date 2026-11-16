//! Native GPUI Mermaid diagram popup: the larger view of a ```mermaid block, opened from the Docs
//! Markdown page, from Markdown in Settings > Extensions, and from a diagram in the chat transcript
//! (apps/desktop/src/app/native_chat/mermaid.rs).
//!
//! The diagram is drawn by the Rust renderer the native Docs view uses (`mermaid_svg` in
//! apps/desktop/src/app/native_docs/blocks.rs, mermaid-rs-renderer then resvg), in the app's
//! appearance over the native modal surface, which is frosted under window glass. It keeps the
//! React viewer's controls: Diagram/Source, zoom out, fit, zoom in, copy source, and dragging to pan.
//! SEE-ALSO: apps/desktop/src/app/mermaid_diagram_modal_lifecycle.rs (open and close),
//! packages/core-ui/mermaid/mermaid-diagram.tsx (deleted 2026-10-01) (the React viewer the Docs page and chat Markdown
//! still draw inline).
use super::native_modal_kit::*;
use crate::app::native_docs::blocks::{mermaid_svg, rasterize_svg_scaled, svg_natural_size};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, CursorStyle, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement as _, Pixels, Point, Render, RenderImage, ScrollHandle, Size,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, point, px,
};
use gpui_component::{h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const ICON_ZOOM_OUT: &str = "modals/mermaid/minus.svg";
const ICON_FIT: &str = "modals/mermaid/focus-centered.svg";
const ICON_ZOOM_IN: &str = "titlebar/plus.svg";
const ICON_COPY: &str = "titlebar/copy.svg";
const ICON_COPIED: &str = "titlebar/check.svg";
const ICON_CLOSE: &str = "titlebar/x.svg";

const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 4.0;
const ZOOM_STEP: f32 = 1.25;
/// The viewport's padding around the diagram, the React viewer's 12px.
const VIEWPORT_PADDING: f32 = 12.0;
/// Past these the bitmap is drawn smaller and the GPU scales it up, so a 4x zoom of a large
/// diagram stays within memory.
const MAX_RASTER_SIDE: f32 = 8192.0;
const MAX_RASTER_PIXELS: f32 = 32_000_000.0;
const COPIED_FOR: Duration = Duration::from_millis(1200);

/// CDXC:SessionChat 2026-09-27 WHY:
/// mermaid-rs-renderer draws every Mermaid type the React viewer drew, but two come out unusable: kanban cards lose their text after the first word (`[Port`), and a timeline in the dark theme puts white text on pastel boxes. Those open on the source with a note instead of a broken picture. Checked against mermaid.js 11.17 on flowchart, sequence, class, state, ER, gantt, pie, mindmap, gitGraph, journey, timeline, quadrant, xychart, sankey, block, requirement, C4, kanban, architecture, packet, radar and treemap samples.
pub(crate) fn unsupported_note(source: &str, light: bool) -> Option<&'static str> {
    match diagram_keyword(source) {
        "kanban" => Some("Kanban diagrams can’t be drawn here yet. This is the diagram’s source."),
        "timeline" if !light => {
            Some("Timelines can’t be drawn in the dark theme yet. This is the diagram’s source.")
        }
        _ => None,
    }
}

/// The diagram type: the first word of the first line that is not front matter or a comment.
fn diagram_keyword(source: &str) -> &str {
    let mut lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut in_front_matter = false;
    for line in lines.by_ref() {
        if line == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter || line.starts_with("%%") {
            continue;
        }
        return line.split_whitespace().next().unwrap_or_default();
    }
    ""
}

pub(crate) enum MermaidDiagramModalCommand {
    /// Escape or the close button.
    Close,
}

pub(crate) type MermaidDiagramModalHost = Rc<dyn Fn(MermaidDiagramModalCommand, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Diagram,
    Source,
}

enum Diagram {
    Rendering,
    /// The SVG and its own size in logical px.
    Ready {
        svg: Arc<str>,
        width: f32,
        height: f32,
    },
    /// Drawn as the source with this note: the renderer refused the text, or draws its type badly.
    Unavailable(String),
}

struct Raster {
    image: Arc<RenderImage>,
    device_width: u32,
}

pub(crate) struct GpuiMermaidDiagramModalWindow {
    host: MermaidDiagramModalHost,
    palette: ModalPalette,
    source: String,
    mode: Mode,
    zoom: f32,
    diagram: Diagram,
    raster: Option<Raster>,
    raster_pending: Option<u32>,
    /// The diagram viewport's size, measured each frame; the fitted size is derived from it.
    viewport: Rc<Cell<Option<Size<Pixels>>>>,
    scroll: ScrollHandle,
    source_scroll: ScrollHandle,
    /// The pointer position and scroll offset a drag started from.
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    copied: bool,
    copied_generation: u64,
    focus_handle: FocusHandle,
    _click_away: Vec<gpui::Subscription>,
}

impl GpuiMermaidDiagramModalWindow {
    pub(crate) fn new(
        source: String,
        palette: ModalPalette,
        host: MermaidDiagramModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let light = palette.light;
        let (mode, diagram) = match unsupported_note(&source, light) {
            Some(note) => (Mode::Source, Diagram::Unavailable(note.to_string())),
            None => {
                let text = source.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            let svg = mermaid_svg(&text, light)?;
                            let (width, height) = svg_natural_size(&svg).ok_or_else(|| {
                                "The diagram image could not be displayed.".to_string()
                            })?;
                            Ok::<_, String>((svg, width, height))
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        match result {
                            Ok((svg, width, height)) => {
                                this.diagram = Diagram::Ready {
                                    svg: svg.into(),
                                    width,
                                    height,
                                };
                            }
                            Err(error) => {
                                this.mode = Mode::Source;
                                this.diagram = Diagram::Unavailable(format!(
                                    "Couldn’t draw this diagram: {error}"
                                ));
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
                (Mode::Diagram, Diagram::Rendering)
            }
        };
        Self {
            host,
            palette,
            source,
            mode,
            zoom: 1.0,
            diagram,
            raster: None,
            raster_pending: None,
            viewport: Rc::new(Cell::new(None)),
            scroll: ScrollHandle::new(),
            source_scroll: ScrollHandle::new(),
            drag: None,
            copied: false,
            copied_generation: 0,
            focus_handle,
            _click_away: super::popup_dismissal::close_app_modal_on_click_away(window, cx),
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        (self.host)(MermaidDiagramModalCommand::Close, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.prevent_default();
            cx.stop_propagation();
            self.close(cx);
        }
    }

    fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        cx.notify();
    }

    fn fit(&mut self, cx: &mut Context<Self>) {
        self.zoom = 1.0;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    fn copy_source(&mut self, cx: &mut Context<Self>) {
        crate::app::helpers::gpui_copy_to_clipboard(
            ClipboardItem::new_string(self.source.clone()),
            cx,
        );
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Draws the SVG for `display_width` logical px in the background, unless that bitmap exists or
    /// is on its way. One draw runs at a time; the next frame asks again for the latest size.
    fn request_raster(
        &mut self,
        svg: Arc<str>,
        natural: (f32, f32),
        display_width: f32,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        let (natural_width, natural_height) = natural;
        let mut scale = display_width * scale_factor / natural_width;
        let limit = (MAX_RASTER_SIDE / (natural_width * scale))
            .min(MAX_RASTER_SIDE / (natural_height * scale))
            .min((MAX_RASTER_PIXELS / (natural_width * natural_height * scale * scale)).sqrt())
            .min(1.0);
        scale *= limit;
        // Whole multiples of 8 device px, so a sub-pixel layout change does not redraw.
        let device_width = ((natural_width * scale / 8.0).round() * 8.0).max(8.0) as u32;
        if self.raster.as_ref().map(|raster| raster.device_width) == Some(device_width)
            || self.raster_pending.is_some()
        {
            return;
        }
        self.raster_pending = Some(device_width);
        let scale = device_width as f32 / natural_width;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let image = background
                .spawn(async move { rasterize_svg_scaled(&svg, scale) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.raster_pending = None;
                match image {
                    Some(image) => {
                        if let Some(old) = this.raster.replace(Raster {
                            image,
                            device_width,
                        }) {
                            cx.drop_image(old.image, None);
                        }
                    }
                    // A bitmap that cannot be drawn is not asked for again every frame.
                    None => {
                        this.mode = Mode::Source;
                        this.diagram = Diagram::Unavailable(
                            "The diagram image could not be displayed.".to_string(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toolbar_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        enabled: bool,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        div()
            .id(id)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(26.0))
            .rounded(px(6.0))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.accent)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        on_click(this, cx);
                    }))
            })
            .when(!enabled, |this| this.opacity(0.4))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(modal_icon(icon, 15.0, p.foreground))
            .into_any_element()
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let ready = matches!(self.diagram, Diagram::Ready { .. }) && self.raster.is_some();
        let zoom = self.zoom;
        let mode = modal_segmented_control(
            &p,
            "mermaid-diagram-mode",
            &[
                ModalSegmentedItem {
                    icon: None,
                    label: "Diagram",
                },
                ModalSegmentedItem {
                    icon: None,
                    label: "Source",
                },
            ],
            usize::from(self.mode == Mode::Source),
            |this: &mut Self, index, _window, cx| {
                this.mode = if index == 0 {
                    Mode::Diagram
                } else {
                    Mode::Source
                };
                cx.notify();
            },
            cx,
        );
        let mut actions = h_flex().items_center().gap(px(2.0));
        if self.mode == Mode::Diagram {
            actions = actions
                .child(self.toolbar_button(
                    "mermaid-zoom-out",
                    ICON_ZOOM_OUT,
                    "Zoom out",
                    ready && zoom > MIN_ZOOM,
                    |this, cx| this.set_zoom(this.zoom / ZOOM_STEP, cx),
                    cx,
                ))
                .child(self.toolbar_button(
                    "mermaid-fit",
                    ICON_FIT,
                    "Fit diagram",
                    ready,
                    |this, cx| this.fit(cx),
                    cx,
                ))
                .child(self.toolbar_button(
                    "mermaid-zoom-in",
                    ICON_ZOOM_IN,
                    "Zoom in",
                    ready && zoom < MAX_ZOOM,
                    |this, cx| this.set_zoom(this.zoom * ZOOM_STEP, cx),
                    cx,
                ));
        }
        actions = actions.child(self.toolbar_button(
            "mermaid-copy",
            if self.copied { ICON_COPIED } else { ICON_COPY },
            if self.copied { "Copied" } else { "Copy source" },
            true,
            |this, cx| this.copy_source(cx),
            cx,
        ));
        let close = self.toolbar_button(
            "mermaid-close",
            ICON_CLOSE,
            "Close",
            true,
            |this, cx| this.close(cx),
            cx,
        );
        // The X leads the toolbar on macOS (CDXC:AppModal 2026-11-10 in
        // window/round_close_button.rs), and ends it elsewhere.
        let (leading_close, trailing_close) = if super::round_close_button::close_buttons_on_left()
        {
            (Some(close), None)
        } else {
            (None, Some(close))
        };
        h_flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            // 9pt at the sides: the 32pt mode switch makes the row taller than the 26pt X, which
            // then sits 3pt lower, so this keeps the X as far from the window's side as from its
            // top (CDXC:SessionChat 2026-11-11 in window/page_band_geometry.rs).
            .px(px(9.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .children(leading_close)
                    .child(div().w(px(180.0)).child(mode)),
            )
            .child(actions.children(trailing_close))
            .into_any_element()
    }

    fn status(&self, lines: Vec<(String, bool)>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .p(px(VIEWPORT_PADDING))
            .gap(px(6.0))
            .children(lines.into_iter().map(|(text, strong)| {
                div()
                    .text_color(hsla(if strong { p.foreground } else { p.muted }))
                    .when(strong, |this| this.font_weight(gpui::FontWeight::MEDIUM))
                    .child(text)
            }))
            .into_any_element()
    }

    fn render_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (svg, natural) = match &self.diagram {
            Diagram::Rendering => {
                return self.status(vec![("Rendering diagram…".to_string(), false)]);
            }
            Diagram::Unavailable(note) => {
                return self.status(vec![
                    (note.clone(), true),
                    (
                        "Open Source to inspect the Mermaid text.".to_string(),
                        false,
                    ),
                ]);
            }
            Diagram::Ready { svg, width, height } => (svg.clone(), (*width, *height)),
        };
        let ratio = natural.0 / natural.1.max(1.0);
        let display = self.viewport.get().map(|size| {
            let frame_width = (f32::from(size.width) - VIEWPORT_PADDING * 2.0).max(1.0);
            let frame_height = (f32::from(size.height) - VIEWPORT_PADDING * 2.0).max(1.0);
            let width = self.zoom * frame_width.min(frame_height * ratio);
            (width, width / ratio)
        });
        if let Some((width, _)) = display {
            self.request_raster(svg, natural, width, window.scale_factor(), cx);
        }
        let image: AnyElement = match (display, &self.raster) {
            (Some((width, height)), Some(raster)) => img(raster.image.clone())
                .flex_none()
                .w(px(width))
                .h(px(height))
                .mx_auto()
                .into_any_element(),
            _ => self.status(vec![("Rendering diagram…".to_string(), false)]),
        };
        let viewport = self.viewport.clone();
        let view = cx.weak_entity();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .on_children_prepainted(move |bounds, _window, cx| {
                let size = bounds.first().map(|bounds| bounds.size);
                if viewport.get() != size {
                    viewport.set(size);
                    let view = view.clone();
                    cx.defer(move |cx| {
                        let _ = view.update(cx, |_, cx| cx.notify());
                    });
                }
            })
            .child(
                div()
                    .id("mermaid-diagram-viewport")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.scroll)
                    .p(px(VIEWPORT_PADDING))
                    .cursor(if self.drag.is_some() {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.drag = Some((event.position, this.scroll.offset()));
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        let Some((start, offset)) = this.drag else {
                            return;
                        };
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.drag = None;
                            cx.notify();
                            return;
                        }
                        let max = this.scroll.max_offset();
                        let next = offset + (event.position - start);
                        this.scroll.set_offset(point(
                            next.x.clamp(-max.x, px(0.0)),
                            next.y.clamp(-max.y, px(0.0)),
                        ));
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _window, cx| {
                            this.drag = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _window, cx| {
                            this.drag = None;
                            cx.notify();
                        }),
                    )
                    .child(image),
            )
            .into_any_element()
    }

    fn render_source(&self) -> AnyElement {
        let p = self.palette;
        let note = match &self.diagram {
            Diagram::Unavailable(note) => Some(note.clone()),
            _ => None,
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .children(note.map(|note| {
                div()
                    .flex_shrink_0()
                    .px(px(VIEWPORT_PADDING))
                    .pt(px(VIEWPORT_PADDING))
                    .text_color(hsla(p.muted))
                    .child(note)
            }))
            .child(
                div()
                    .id("mermaid-diagram-source")
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .track_scroll(&self.source_scroll)
                    .p(px(VIEWPORT_PADDING))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(19.2))
                    .text_color(hsla(p.foreground))
                    .children(self.source.lines().map(|line| {
                        div().whitespace_nowrap().child(if line.is_empty() {
                            " ".to_string()
                        } else {
                            line.to_string()
                        })
                    })),
            )
            .into_any_element()
    }
}

impl Render for GpuiMermaidDiagramModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let toolbar = self.render_toolbar(cx);
        let body = match self.mode {
            Mode::Diagram => self.render_diagram(window, cx),
            Mode::Source => self.render_source(),
        };
        div()
            .id("ghostex-gpui-mermaid-diagram-modal")
            .track_focus(&self.focus_handle)
            .size_full()
            .p(px(12.0))
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(13.0))
            .line_height(px(19.5))
            .text_color(hsla(p.foreground))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                v_flex()
                    .size_full()
                    .overflow_hidden()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .child(toolbar)
                    .child(body),
            )
    }
}

impl ModalCornerClose for GpuiMermaidDiagramModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
    }

    /// It draws its own close button in that corner.
    fn shows_corner_close(&self, _cx: &App) -> bool {
        false
    }
}
