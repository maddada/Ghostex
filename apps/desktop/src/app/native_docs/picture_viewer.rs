//! The full-window picture viewer Files opens a picture in: the chat's image preview, over the
//! whole main window.
//!
//! CDXC:Docs 2026-10-24 SEE-ALSO:
//! The chat's viewer (apps/desktop/src/app/native_chat/image_viewer/) belongs to a chat pane and reads its pictures through the session, so Files draws the same viewer here from the same sizing code (`fitted_size`, `painted_size`, `zoom_widths` and `stage_inset`): the picture opens centred, a click steps through three zoom levels up to its own pixels and back to fitted, dragging pans, and Escape, the close button or a click around the picture closes it. Its copy buttons are the chat's Copy path and Copy image. A change to how the chat's viewer looks or zooms belongs in both.

use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, DispatchPhase, FocusHandle, InteractiveElement as _, IntoElement,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render,
    ScrollHandle, Size, StatefulInteractiveElement as _, Styled as _, StyledImage as _, WeakEntity,
    Window, WindowBounds, WindowOptions, canvas, div, img, point, px, svg,
};
use gpui_component::Root;
use gpui_component::tooltip::Tooltip;

use crate::GhostexGpuiApp;
use crate::app::native_chat::appearance::ChatAppearance;
use crate::app::native_chat::image_viewer::pan::stage_inset;
use crate::app::native_chat::image_viewer::render::{ZOOM_LEVEL_COUNT, painted_size, zoom_widths};

/// How far a press on the picture travels before it is a drag instead of a zoom click.
const DRAG_THRESHOLD: f32 = 4.0;

struct ViewerPress {
    start: Point<Pixels>,
    offset: Point<Pixels>,
    panning: bool,
}

struct ZoomAnchor {
    fraction: Point<f32>,
    at: Point<Pixels>,
}

pub(crate) struct DocsPictureViewer {
    app: WeakEntity<GhostexGpuiApp>,
    picture: Arc<gpui::Image>,
    /// The file's full path, for Copy path.
    path: String,
    natural: Option<Size<Pixels>>,
    /// 0 is the fitted size; every step after it enlarges, and the last step returns to fitted.
    zoom: usize,
    scroll: ScrollHandle,
    press: Option<ViewerPress>,
    zoom_anchor: Option<ZoomAnchor>,
    painted: Option<Size<Pixels>>,
    completed: Option<&'static str>,
    focus: FocusHandle,
}

impl super::picture_zoom::DocsPictureHost for GhostexGpuiApp {
    fn docs_active_picture(&self) -> Option<(String, Arc<gpui::Image>)> {
        let document = self.native_docs.active_document()?;
        if document.kind != super::state::DocsFileKind::Image || document.svg_source {
            return None;
        }
        Some((document.path.to_string(), document.image.clone()?))
    }

    fn docs_picture_view(
        &mut self,
        path: &str,
    ) -> Option<&mut super::picture_zoom::DocsPictureView> {
        self.native_docs
            .document_mut(path)
            .map(|document| &mut document.picture)
    }

    fn docs_open_picture_viewer(&mut self, path: &str, cx: &mut Context<Self>) {
        self.native_docs_open_picture_viewer(path, cx);
    }
}

impl GhostexGpuiApp {
    /// Opens `path`'s picture in the full-window viewer.
    pub(crate) fn native_docs_open_picture_viewer(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(picture) = self
            .native_docs
            .document(path)
            .and_then(|document| document.image.clone())
        else {
            return;
        };
        let Some(main) = self.main_window_handle else {
            return;
        };
        self.native_docs_close_picture_viewer(cx);
        let full_path = {
            let file = std::path::Path::new(path);
            match &self.native_docs.project {
                Some(project) if !file.is_absolute() => project
                    .project_path
                    .join(file)
                    .to_string_lossy()
                    .into_owned(),
                _ => path.to_string(),
            }
        };
        let parent = self.parent_ns_view;
        let glass = crate::app::helpers::window_glass_active();
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let result = main
                .update(cx, |_, window, cx| {
                    let origin = window.bounds().origin
                        + point(
                            px(0.0),
                            (window.bounds().size.height - window.viewport_size().height)
                                .max(px(0.0)),
                        );
                    (
                        gpui::Bounds::new(origin, window.viewport_size()),
                        crate::app::window::popup_frame::PopupOwner::of(window, cx),
                    )
                })
                .and_then(|(bounds, owner)| {
                    let display_id = owner.display_for(bounds, cx);
                    let app = app.clone();
                    cx.open_window(
                        WindowOptions {
                            kind: crate::app::window::popup_frame::child_window_kind(),
                            window_decorations:
                                crate::app::window::popup_frame::child_window_decorations(),
                            #[cfg(target_os = "linux")]
                            x11_parent: Some(main),
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            display_id,
                            app_id: crate::gpui_platform_window_app_id(),
                            icon: crate::gpui_platform_window_icon(),
                            focus: true,
                            show: true,
                            is_resizable: false,
                            is_minimizable: false,
                            is_movable: false,
                            titlebar: None,
                            window_background: if glass {
                                gpui::WindowBackgroundAppearance::Blurred
                            } else {
                                gpui::WindowBackgroundAppearance::Transparent
                            },
                            ..Default::default()
                        },
                        move |window, cx| {
                            crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                            if glass {
                                crate::app::helpers::apply_frosted_menu_blur(window);
                            }
                            crate::app::window::attach_gpui_app_modal_window_to_main_window(
                                window, parent,
                            );
                            #[cfg(target_os = "windows")]
                            crate::app::window::own_gpui_popup_window(window, parent);
                            let natural = super::picture_zoom::natural_size(&picture);
                            let view = cx.new(|cx| {
                                let focus = cx.focus_handle();
                                focus.focus(window, cx);
                                window.activate_window();
                                DocsPictureViewer {
                                    app,
                                    picture,
                                    path: full_path,
                                    natural,
                                    zoom: 0,
                                    scroll: ScrollHandle::new(),
                                    press: None,
                                    zoom_anchor: None,
                                    painted: None,
                                    completed: None,
                                    focus,
                                }
                            });
                            cx.new(|cx| {
                                Root::new(view, window, cx)
                                    .bordered(false)
                                    .bg(gpui::transparent_black())
                            })
                        },
                    )
                });
            if let Ok(handle) = result {
                let _ = app.update(cx, |app, _| app.native_docs.picture_viewer = Some(handle));
            }
        });
    }

    pub(crate) fn native_docs_close_picture_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.native_docs.picture_viewer.take() else {
            return;
        };
        let main = self.main_window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            if let Some(main) = main {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            }
        });
    }
}

impl DocsPictureViewer {
    fn close(&mut self, cx: &mut Context<Self>) {
        let _ = self
            .app
            .update(cx, |app, cx| app.native_docs_close_picture_viewer(cx));
    }

    fn press_picture(&mut self, position: Point<Pixels>) {
        self.press = Some(ViewerPress {
            start: position,
            offset: self.scroll.offset(),
            panning: false,
        });
    }

    fn drag_picture(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(press) = &mut self.press else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
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
        let max = self.scroll.max_offset();
        let next = press.offset + travel;
        let next = point(next.x.clamp(-max.x, px(0.0)), next.y.clamp(-max.y, px(0.0)));
        if next != self.scroll.offset() {
            self.scroll.set_offset(next);
            cx.notify();
        }
    }

    fn release_picture(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        match self.press.take() {
            Some(press) if press.panning => cx.notify(),
            Some(_) => self.zoom_at(event.position, cx),
            None => {}
        }
    }

    /// Steps the zoom, keeping the spot of the picture under `at` there.
    fn zoom_at(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.zoom_anchor = self.painted.map(|painted| {
            let scroll = self.scroll.bounds();
            let origin = scroll.origin + self.scroll.offset() + stage_inset(scroll, painted);
            let fraction = |from: Pixels, size: Pixels| {
                if size > px(0.0) {
                    (f32::from(from) / f32::from(size)).clamp(0.0, 1.0)
                } else {
                    0.5
                }
            };
            ZoomAnchor {
                fraction: point(
                    fraction(at.x - origin.x, painted.width),
                    fraction(at.y - origin.y, painted.height),
                ),
                at,
            }
        });
        self.zoom = if self.zoom >= ZOOM_LEVEL_COUNT {
            0
        } else {
            self.zoom + 1
        };
        cx.notify();
    }

    fn follow_zoom_anchor(&mut self, painted: Option<Size<Pixels>>) {
        if let (Some(anchor), Some(painted)) = (self.zoom_anchor.take(), painted) {
            let scroll = self.scroll.bounds();
            let inset = stage_inset(scroll, painted);
            let offset =
                |at: Pixels, start: Pixels, inset: Pixels, spot: Pixels, overflow: Pixels| {
                    (at - start - inset - spot).clamp(-overflow.max(px(0.0)), px(0.0))
                };
            self.scroll.set_offset(point(
                offset(
                    anchor.at.x,
                    scroll.origin.x,
                    inset.x,
                    painted.width * anchor.fraction.x,
                    painted.width - scroll.size.width,
                ),
                offset(
                    anchor.at.y,
                    scroll.origin.y,
                    inset.y,
                    painted.height * anchor.fraction.y,
                    painted.height - scroll.size.height,
                ),
            ));
        }
        self.painted = painted;
    }

    fn action_button(
        &self,
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        done: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .size(px(28.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(p.input))
            .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            .child(
                svg()
                    .path(if done { "titlebar/check.svg" } else { icon })
                    .size(px(15.0))
                    .text_color(crate::app::helpers::titlebar_popup_menu_foreground()),
            )
            .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            .into_any_element()
    }
}

impl Render for DocsPictureViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = ChatAppearance::current(&serde_json::Value::Null);
        if window.focused(cx).is_none() {
            self.focus.focus(window, cx);
        }
        let viewport = window.viewport_size();
        let painted = self
            .natural
            .map(|natural| painted_size(natural, viewport, self.zoom));
        self.follow_zoom_anchor(painted);
        let zooms = self
            .natural
            .is_some_and(|natural| zoom_widths(natural, viewport).is_some());
        let picture = img(self.picture.clone()).object_fit(gpui::ObjectFit::Contain);
        let picture = match painted {
            Some(size) => picture.w(size.width).h(size.height),
            None => picture
                .max_w(viewport.width * 0.9)
                .max_h(viewport.height * 0.75),
        };
        let content = div()
            .id("docs-picture-viewer-picture")
            .map(|element| match (zooms, self.zoom >= ZOOM_LEVEL_COUNT) {
                (false, _) => element.cursor_pointer(),
                (true, false) => element.cursor(gpui::CursorStyle::ZoomIn),
                (true, true) => element.cursor(gpui::CursorStyle::ZoomOut),
            })
            .flex_shrink_0()
            .when_some(painted, |element, size| {
                element.w(size.width).h(size.height)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.press_picture(event.position);
                }),
            )
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(picture);
        let body = div()
            .id("docs-picture-viewer-scroll")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_scroll()
            .flex()
            .items_start()
            .child(
                div()
                    .flex_shrink_0()
                    .min_w(viewport.width)
                    .min_h(viewport.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            );
        // The sidebar tooltip bubble's fill: these float over the picture (CDXC:Tooltips 2026-10-25
        // on `in_window_bubble`).
        let surface = crate::app::native_sidebar::link_chips::in_window_bubble(
            crate::app::helpers::window_glass_active(),
        )
        .fill;
        let surface_hover =
            surface.blend(crate::app::helpers::titlebar_popup_menu_foreground().opacity(0.1));
        let completed = self.completed;
        let toolbar = div()
            .absolute()
            .top(px(12.0))
            .right(px(56.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.control_border)
            .bg(surface)
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.action_button(
                "docs-picture-copy-path",
                "Copy path",
                "titlebar/link.svg",
                completed == Some("Path copied"),
                &p,
                cx,
                |this, cx| {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(this.path.clone()),
                        cx,
                    );
                    this.completed = Some("Path copied");
                    cx.notify();
                },
            ))
            .child(self.action_button(
                "docs-picture-copy",
                "Copy image",
                "titlebar/copy.svg",
                completed == Some("Image copied"),
                &p,
                cx,
                |this, cx| {
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_image(&this.picture),
                        cx,
                    );
                    this.completed = Some("Image copied");
                    cx.notify();
                },
            ));
        let close = div()
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("docs-picture-viewer-close")
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(surface)
                    .when(p.light, |this| this.border_1().border_color(p.border))
                    .cursor_pointer()
                    .hover(move |style| style.bg(surface_hover))
                    .child(
                        svg()
                            .path("titlebar/x.svg")
                            .size(px(18.0))
                            .text_color(crate::app::helpers::titlebar_popup_menu_foreground()),
                    )
                    .tooltip(|window, cx| {
                        Tooltip::new("Close (or click outside the image)").build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            );
        let view = cx.weak_entity();
        let panning = self.press.as_ref().is_some_and(|press| press.panning);
        let pan_listeners = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if panning {
                    window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
                }
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |this, cx| this.drag_picture(event, cx));
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        let _ = released.update(cx, |this, cx| this.release_picture(event, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let dim = if crate::app::helpers::window_glass_active() {
            gpui::black().opacity(0.45)
        } else {
            gpui::black().opacity(0.7)
        };
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .font_family(p.font.clone())
            .flex()
            .flex_col()
            .bg(dim)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close(cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.close(cx)),
            )
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let handled = match event.keystroke.key.as_str() {
                    "escape" => {
                        this.close(cx);
                        true
                    }
                    "enter" | "space" => {
                        this.zoom_at(this.scroll.bounds().center(), cx);
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
            .child(pan_listeners)
    }
}
