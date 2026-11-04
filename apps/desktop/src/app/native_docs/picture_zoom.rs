//! Zooming and moving around a picture open in Files: pinch, Cmd/Ctrl with the wheel, the zoom
//! chords, dragging, double-click, and the header's zoom buttons.
//!
//! CDXC:Docs 2026-10-24 DECISION:
//! User: "i can't zoom in here into this png this is very annoying". They picked every option: pinch on a trackpad, Ctrl or Cmd with the mouse wheel and Cmd+= / Cmd+- / Cmd+0 (Ctrl on Windows and Linux) zoom around the pointer, dragging or scrolling moves around, and a double-click switches between the fitted and the actual size; the header carries zoom out, the percentage, zoom in, Fit and 1:1; a picture taller than the view opens at the view's width (never past its own size) and scrolls like a document, while a wide or small one still fits whole, with a "Fit all" button for the whole picture; and a click opens the chat's full-window picture viewer (`picture_viewer.rs`). For keeping the zoom they answered "No, each picture opens fitted": the zoom lives on the open document and is not carried to another picture.
//! SEE-ALSO: apps/mobile/app/src/docs/plainDocument.ts (the phone's picture page: pinch, and the same double-tap).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, DispatchPhase, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    ScrollEvent, ScrollHandle, Size, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, canvas, div, img, point, px, size,
};

use super::files_list::{header_icon, header_tile};
use super::palette::DocsPalette;
use crate::app::helpers::titlebar_tooltip;

/// The air around the picture while it fits, and at its edges once it is bigger than the view.
const PAD: f32 = 16.0;
/// The header's zoom buttons and the chords step through these.
const ZOOM_STEPS: [f32; 15] = [
    0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0,
];
const MAX_SCALE: f32 = 8.0;
/// How far a press travels before it is a drag instead of a click.
const DRAG_THRESHOLD: f32 = 4.0;
/// A click opens the viewer only once a second click could no longer make it a double-click.
const DOUBLE_CLICK_WAIT: Duration = Duration::from_millis(300);

/// How the picture is sized.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(crate) enum DocsPictureZoom {
    /// How it opens: the view's width, never past its own size, so a tall one scrolls.
    #[default]
    Opening,
    /// The whole picture in the view ("Fit all").
    Whole,
    /// Picture pixels per point, 1.0 being the actual size.
    Scale(f32),
}

/// A press on the picture that has not been released yet.
struct PicturePress {
    start: Point<Pixels>,
    offset: Point<Pixels>,
    panning: bool,
}

/// One open picture's zoom and position.
#[derive(Default)]
pub(crate) struct DocsPictureView {
    pub(crate) zoom: DocsPictureZoom,
    pub(crate) scroll: ScrollHandle,
    /// The picture measured last, by its image id, and its own size.
    natural: Option<(u64, Option<Size<Pixels>>)>,
    /// The body's bounds as last laid out.
    viewport: Rc<Cell<Bounds<Pixels>>>,
    press: Option<PicturePress>,
    /// Bumped by every press, so a pending single click's viewer never opens after a second one.
    click_generation: u64,
}

/// The sizes a picture can take in a view of `viewport`.
#[derive(Clone, Copy)]
struct PictureFit {
    natural: Size<Pixels>,
    width: f32,
    whole: f32,
}

impl PictureFit {
    fn new(natural: Size<Pixels>, viewport: Size<Pixels>) -> Self {
        let room_w = (f32::from(viewport.width) - PAD * 2.0).max(1.0);
        let room_h = (f32::from(viewport.height) - PAD * 2.0).max(1.0);
        let (w, h) = (f32::from(natural.width), f32::from(natural.height));
        let width = (room_w / w).min(1.0);
        let whole = width.min(room_h / h);
        Self {
            natural,
            width,
            whole,
        }
    }

    fn scale(&self, zoom: DocsPictureZoom) -> f32 {
        match zoom {
            DocsPictureZoom::Opening => self.width,
            DocsPictureZoom::Whole => self.whole,
            DocsPictureZoom::Scale(scale) => scale,
        }
    }

    /// Taller than the view at its opening size, so "Fit all" differs from how it opened.
    fn tall(&self) -> bool {
        self.width > self.whole + 0.0005
    }

    fn min_scale(&self) -> f32 {
        self.whole.min(ZOOM_STEPS[0])
    }

    fn painted(&self, scale: f32) -> Size<Pixels> {
        size(self.natural.width * scale, self.natural.height * scale)
    }
}

/// The picture's own size: its pixels, or an SVG's declared size.
pub(crate) fn natural_size(picture: &gpui::Image) -> Option<Size<Pixels>> {
    if picture.format == gpui::ImageFormat::Svg {
        let text = std::str::from_utf8(&picture.bytes).ok()?;
        let (width, height) = super::blocks::svg_natural_size(text)?;
        return (width > 0.0 && height > 0.0).then(|| size(px(width), px(height)));
    }
    crate::app::native_chat::image_viewer::render::header_size(picture)
}

/// Where the stage puts a picture of `painted` size at scroll offset zero, on one axis.
fn inset(view: Pixels, painted: Pixels) -> Pixels {
    ((view - painted) / 2.0).max(px(PAD))
}

fn next_step(scale: f32, up: bool) -> f32 {
    if up {
        ZOOM_STEPS
            .into_iter()
            .find(|step| *step > scale * 1.01)
            .unwrap_or(MAX_SCALE)
    } else {
        ZOOM_STEPS
            .into_iter()
            .rev()
            .find(|step| *step < scale * 0.99)
            .unwrap_or(0.0)
    }
}

impl DocsPictureView {
    fn fit(&mut self, picture: &gpui::Image) -> Option<PictureFit> {
        let natural = match self.natural {
            Some((id, natural)) if id == picture.id => natural,
            _ => {
                let natural = natural_size(picture);
                self.natural = Some((picture.id, natural));
                natural
            }
        }?;
        let viewport = self.viewport.get().size;
        (viewport.width > px(0.0) && viewport.height > px(0.0))
            .then(|| PictureFit::new(natural, viewport))
    }

    /// Sizes the picture by `zoom`, keeping the spot under `at` (window coordinates; the view's
    /// centre when `None`) where it is.
    fn zoom_to(&mut self, fit: PictureFit, zoom: DocsPictureZoom, at: Option<Point<Pixels>>) {
        let zoom = match zoom {
            DocsPictureZoom::Scale(scale) => {
                DocsPictureZoom::Scale(scale.clamp(fit.min_scale(), MAX_SCALE))
            }
            other => other,
        };
        let view = self.viewport.get();
        let at = at.unwrap_or_else(|| view.center());
        let before = fit.painted(fit.scale(self.zoom));
        let after = fit.painted(fit.scale(zoom));
        let offset = self.scroll.offset();
        let axis = |at: Pixels,
                    origin: Pixels,
                    offset: Pixels,
                    view: Pixels,
                    before: Pixels,
                    after: Pixels| {
            let start = origin + offset + inset(view, before);
            let fraction = if before > px(0.0) {
                (f32::from(at - start) / f32::from(before)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let overflow = (after + px(PAD * 2.0) - view).max(px(0.0));
            (at - origin - inset(view, after) - after * fraction).clamp(-overflow, px(0.0))
        };
        self.scroll.set_offset(point(
            axis(
                at.x,
                view.origin.x,
                offset.x,
                view.size.width,
                before.width,
                after.width,
            ),
            axis(
                at.y,
                view.origin.y,
                offset.y,
                view.size.height,
                before.height,
                after.height,
            ),
        ));
        self.zoom = zoom;
    }
}

/// What a zoom request asks for.
#[derive(Clone, Copy)]
pub(crate) enum PictureZoomCommand {
    In,
    Out,
    /// Back to how the picture opened (Cmd+0).
    Opening,
    Whole,
    Actual,
    /// Multiplies the scale (a pinch or a wheel turn) around a point.
    By(f32, Point<Pixels>),
    /// The double-click: actual size at the pointer, or back to how it opened.
    Toggle(Point<Pixels>),
}

/// The view a picture is drawn in: the app's Files view, or the Files demo window
/// (`bin/native_docs_demo/picture.rs`). Everything else here is shared.
pub(crate) trait DocsPictureHost: Sized + 'static {
    /// The active document's path and picture, while it shows as a picture.
    fn docs_active_picture(&self) -> Option<(String, std::sync::Arc<gpui::Image>)>;
    /// The zoom and position of the open picture at `path`.
    fn docs_picture_view(&mut self, path: &str) -> Option<&mut DocsPictureView>;
    /// Opens `path`'s picture in the full-window viewer (`picture_viewer.rs`).
    fn docs_open_picture_viewer(&mut self, path: &str, cx: &mut Context<Self>);

    /// Zooms the active picture; false when there is none to zoom, so the key goes on.
    fn native_docs_zoom_picture(
        &mut self,
        command: PictureZoomCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((path, picture)) = self.docs_active_picture() else {
            return false;
        };
        let Some(view) = self.docs_picture_view(&path) else {
            return false;
        };
        let Some(fit) = view.fit(&picture) else {
            return false;
        };
        let scale = fit.scale(view.zoom);
        let (zoom, at) = match command {
            PictureZoomCommand::In => (DocsPictureZoom::Scale(next_step(scale, true)), None),
            PictureZoomCommand::Out => (
                DocsPictureZoom::Scale(next_step(scale, false).max(fit.min_scale())),
                None,
            ),
            PictureZoomCommand::Opening => (DocsPictureZoom::Opening, None),
            PictureZoomCommand::Whole => (DocsPictureZoom::Whole, None),
            PictureZoomCommand::Actual => (DocsPictureZoom::Scale(1.0), None),
            PictureZoomCommand::By(factor, at) => {
                (DocsPictureZoom::Scale(scale * factor), Some(at))
            }
            PictureZoomCommand::Toggle(at) => {
                if (scale - 1.0).abs() < 0.001 {
                    (DocsPictureZoom::Opening, Some(at))
                } else {
                    (DocsPictureZoom::Scale(1.0), Some(at))
                }
            }
        };
        view.zoom_to(fit, zoom, at);
        cx.notify();
        true
    }

    /// The picture body: the picture at its zoom inside a scrolling stage.
    fn render_native_docs_picture(
        &mut self,
        path: &str,
        picture: std::sync::Arc<gpui::Image>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(view) = self.docs_picture_view(path) else {
            return div().into_any_element();
        };
        let viewport = view.viewport.clone();
        let laid_out = viewport.get();
        let probe = canvas(
            move |bounds, window, _| {
                if viewport.replace(bounds) != bounds {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let Some(fit) = view.fit(&picture) else {
            // Not laid out yet, or a picture with no size of its own: fitted whole, as before.
            return div()
                .relative()
                .size_full()
                .child(probe)
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(PAD))
                        .child(
                            img(picture)
                                .max_w_full()
                                .max_h_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                )
                .into_any_element();
        };
        let painted = fit.painted(fit.scale(view.zoom));
        let room = laid_out.size;
        let pannable = painted.width + px(PAD * 2.0) > room.width + px(0.5)
            || painted.height + px(PAD * 2.0) > room.height + px(0.5);
        let panning = view.press.as_ref().is_some_and(|press| press.panning);
        let scroll = view.scroll.clone();
        let app = cx.weak_entity();
        let listeners = {
            let app = app.clone();
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if panning {
                        window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
                    }
                    let moved = app.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moved
                                .update(cx, |this, cx| this.native_docs_drag_picture(event, cx));
                        }
                    });
                    let released = app.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                            let _ = released
                                .update(cx, |this, cx| this.native_docs_release_picture(event, cx));
                        }
                    });
                    // Cmd or Ctrl with the wheel zooms instead of scrolling; captured so the
                    // scroll box under it never moves too.
                    let wheel = app.clone();
                    window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture
                            || !(event.modifiers.platform || event.modifiers.control)
                            || !bounds.contains(&event.position)
                        {
                            return;
                        }
                        let factor = match event.delta {
                            gpui::ScrollDelta::Lines(lines) => 1.12_f32.powf(lines.y),
                            gpui::ScrollDelta::Pixels(delta) => (f32::from(delta.y) / 200.0).exp(),
                        };
                        let zoomed = wheel
                            .update(cx, |this, cx| {
                                this.native_docs_zoom_picture(
                                    PictureZoomCommand::By(factor, event.position),
                                    cx,
                                )
                            })
                            .unwrap_or(false);
                        if zoomed {
                            cx.stop_propagation();
                            window.prevent_default();
                        }
                    });
                },
            )
            .absolute()
            .size_full()
        };
        let picture_path = path.to_string();
        let element = div()
            .id("native-docs-picture")
            .flex_shrink_0()
            .w(painted.width)
            .h(painted.height)
            .map(|element| {
                if pannable {
                    element.cursor(gpui::CursorStyle::OpenHand)
                } else {
                    element.cursor_pointer()
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.native_docs_press_picture(&picture_path, event);
                }),
            )
            .child(img(picture).size_full().object_fit(gpui::ObjectFit::Fill));
        div()
            .relative()
            .size_full()
            .on_pinch(cx.listener(|this, event: &gpui::PinchEvent, _, cx| {
                this.native_docs_zoom_picture(
                    PictureZoomCommand::By(1.0 + event.delta, event.position),
                    cx,
                );
            }))
            .child(probe)
            .child(
                div()
                    .id("native-docs-image")
                    .track_scroll(&scroll)
                    .size_full()
                    .overflow_scroll()
                    .child(
                        // At least the view's size and growing with the picture, which centres a
                        // small picture and starts a big one at its edge, so every part of it can
                        // be scrolled to (GPUI only scrolls toward the right and the bottom).
                        div()
                            .flex_shrink_0()
                            .min_w(room.width)
                            .min_h(room.height)
                            .p(px(PAD))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(element),
                    ),
            )
            .child(gpui_component::scroll::Scrollbar::new(&scroll))
            .child(listeners)
            .into_any_element()
    }

    fn native_docs_press_picture(&mut self, path: &str, event: &MouseDownEvent) {
        let Some(view) = self.docs_picture_view(path) else {
            return;
        };
        view.click_generation += 1;
        view.press = Some(PicturePress {
            start: event.position,
            offset: view.scroll.offset(),
            panning: false,
        });
    }

    fn native_docs_drag_picture(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((path, _)) = self.docs_active_picture() else {
            return;
        };
        let Some(view) = self.docs_picture_view(&path) else {
            return;
        };
        let Some(press) = &mut view.press else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            view.press = None;
            cx.notify();
            return;
        }
        let travel = event.position - press.start;
        if !press.panning {
            if f32::from(travel.x).hypot(f32::from(travel.y)) < DRAG_THRESHOLD {
                return;
            }
            press.panning = true;
            cx.notify();
        }
        let max = view.scroll.max_offset();
        let next = press.offset + travel;
        let next = point(next.x.clamp(-max.x, px(0.0)), next.y.clamp(-max.y, px(0.0)));
        if next != view.scroll.offset() {
            view.scroll.set_offset(next);
            cx.notify();
        }
    }

    fn native_docs_release_picture(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some((path, _)) = self.docs_active_picture() else {
            return;
        };
        let Some(view) = self.docs_picture_view(&path) else {
            return;
        };
        let Some(press) = view.press.take() else {
            return;
        };
        let generation = view.click_generation;
        if press.panning {
            cx.notify();
            return;
        }
        if event.click_count >= 2 {
            self.native_docs_zoom_picture(PictureZoomCommand::Toggle(event.position), cx);
            return;
        }
        // A single click opens the full-window viewer, unless it turns out to be a double-click.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DOUBLE_CLICK_WAIT).await;
            let _ = this.update(cx, |this, cx| {
                let current = this
                    .docs_picture_view(&path)
                    .map(|view| view.click_generation);
                let active = this.docs_active_picture().map(|(active, _)| active);
                if current == Some(generation) && active.as_deref() == Some(path.as_str()) {
                    this.docs_open_picture_viewer(&path, cx);
                }
            });
        })
        .detach();
    }

    /// The header's zoom buttons for the active picture, once it is laid out.
    fn render_native_docs_picture_tiles(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some((path, picture)) = self.docs_active_picture() else {
            return Vec::new();
        };
        let Some(view) = self.docs_picture_view(&path) else {
            return Vec::new();
        };
        let Some(fit) = view.fit(&picture) else {
            return Vec::new();
        };
        let scale = fit.scale(view.zoom);
        let near = |a: f32, b: f32| (a - b).abs() < 0.001;
        let at_min = scale <= fit.min_scale() + 0.0005;
        let at_max = scale >= MAX_SCALE - 0.0005;
        let tall = fit.tall();
        let primary = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl+"
        };
        let zoom_out_tip: gpui::SharedString = format!("Zoom out ({primary}−)").into();
        let zoom_in_tip: gpui::SharedString = format!("Zoom in ({primary}+)").into();
        let mut tiles = Vec::new();
        let tile = |id: &'static str,
                    icon: AnyElement,
                    active: bool,
                    disabled: bool,
                    tip: gpui::SharedString,
                    command: PictureZoomCommand,
                    cx: &mut Context<Self>| {
            header_tile(id, icon, active, disabled, p)
                .tooltip(move |window, cx| titlebar_tooltip(tip.clone(), window, cx))
                .when(!disabled, |tile| {
                    tile.on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_zoom_picture(command, cx);
                    }))
                })
                .into_any_element()
        };
        tiles.push(tile(
            "native-docs-picture-zoom-out",
            header_icon("modals/mermaid/minus.svg", at_min, p),
            false,
            at_min,
            zoom_out_tip,
            PictureZoomCommand::Out,
            cx,
        ));
        tiles.push(
            div()
                .flex_none()
                .min_w(px(40.0))
                .text_center()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.toolbar_icon)
                .child(format!("{}%", (scale * 100.0).round() as i32))
                .into_any_element(),
        );
        tiles.push(tile(
            "native-docs-picture-zoom-in",
            header_icon("titlebar/plus.svg", at_max, p),
            false,
            at_max,
            zoom_in_tip,
            PictureZoomCommand::In,
            cx,
        ));
        if tall {
            tiles.push(tile(
                "native-docs-picture-fit-width",
                header_icon("files-view/t-arrow-autofit-width.svg", false, p),
                near(scale, fit.scale(DocsPictureZoom::Opening)),
                false,
                "Fit to width".into(),
                PictureZoomCommand::Opening,
                cx,
            ));
        }
        tiles.push(tile(
            "native-docs-picture-fit",
            header_icon("modals/mermaid/focus-centered.svg", false, p),
            near(scale, fit.scale(DocsPictureZoom::Whole)),
            false,
            if tall { "Fit all" } else { "Fit" }.into(),
            PictureZoomCommand::Whole,
            cx,
        ));
        tiles.push(tile(
            "native-docs-picture-actual",
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.toolbar_icon)
                .child("1:1")
                .into_any_element(),
            near(scale, 1.0),
            false,
            "Actual size".into(),
            PictureZoomCommand::Actual,
            cx,
        ));
        let open_path = path.clone();
        tiles.push(
            header_tile(
                "native-docs-picture-viewer",
                header_icon("titlebar/arrows-diagonal.svg", false, p),
                false,
                false,
                p,
            )
            .tooltip(|window, cx| titlebar_tooltip("Open in picture viewer", window, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.docs_open_picture_viewer(&open_path, cx);
            }))
            .into_any_element(),
        );
        tiles
    }
}
