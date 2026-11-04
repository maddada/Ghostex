//! The floating button: the app icon with a tray of counts beside it, or a thin tab of stacked
//! counts while it is docked against a screen edge.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: the button is the app's icon with the numbers in a dark tray outside the icon on its
//! right ("make the area for numbers appear on the right outside of the icon itself"), flipped to
//! the left when there is no room; dragged against a screen edge it tucks away and shows only the
//! three numbers stacked on top of each other until hovered.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, DispatchPhase, ImageSource, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, RenderImage, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, img, point,
    px, rgb, rgba,
};

use super::glass::{TrayPalette, button_glass, one_side_rounded};
use super::model::*;
use super::persistence;
use super::placement::{self, Screen};
use super::platform;
use crate::GhostexGpuiApp;

/// How long the pointer may be off a slid-out docked button before it tucks back.
const DOCK_CLOSE_DELAY: Duration = Duration::from_millis(350);

/// The app icon, trimmed to its visible pixels, as GPUI draws it (BGRA).
fn app_icon_image() -> Option<Arc<RenderImage>> {
    static IMAGE: OnceLock<Option<Arc<RenderImage>>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let decoded = image::load_from_memory_with_format(
                crate::assets::APP_ICON_PNG,
                image::ImageFormat::Png,
            )
            .ok()?
            .into_rgba8();
            let (mut left, mut top, mut right, mut bottom) =
                (decoded.width(), decoded.height(), 0, 0);
            for (x, y, pixel) in decoded.enumerate_pixels() {
                if pixel.0[3] > 8 {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x);
                    bottom = bottom.max(y);
                }
            }
            if right <= left || bottom <= top {
                return None;
            }
            let mut cropped =
                image::imageops::crop_imm(&decoded, left, top, right - left + 1, bottom - top + 1)
                    .to_image();
            for pixel in cropped.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Some(Arc::new(RenderImage::new(vec![image::Frame::new(cropped)])))
        })
        .clone()
}

pub(crate) struct CaptureIconView {
    app: WeakEntity<GhostexGpuiApp>,
    _observe: Subscription,
    /// The window's background and blur region as last set; the button stays up while window
    /// glass is turned on or off and while its tray changes size (`glass.rs`).
    background: WindowBackgroundAppearance,
    blur_region: Vec<(Bounds<Pixels>, Pixels)>,
}

impl GhostexGpuiApp {
    pub(crate) fn ghostex_capture_counts(&self) -> CaptureCounts {
        let counts = self.sidebar_session_status_indicators.section_counts;
        CaptureCounts {
            working: counts.working,
            attention: counts.attention,
            question: counts.question,
        }
    }

    /// The button's scale factor, for reading the pointer on the backends that report it in device
    /// pixels.
    pub(super) fn ghostex_capture_scale(&self) -> f32 {
        self.ghostex_capture
            .icon
            .as_ref()
            .map(|icon| icon.scale)
            .unwrap_or(1.0)
    }

    /// The screen the button lives on and its saved spot there.
    pub(super) fn ghostex_capture_home(
        &self,
        cx: &App,
    ) -> Option<(Screen, persistence::SavedPlacement)> {
        let saved = &self.ghostex_capture.saved;
        let screen = placement::home_screen(saved.display.as_deref(), cx)?;
        let spot = saved
            .placements
            .get(&screen.key)
            .copied()
            .unwrap_or_else(|| placement::default_placement(&screen));
        Some((screen, spot))
    }

    /// The layout and frame the button should have now.
    fn ghostex_capture_icon_target(
        &self,
        cx: &App,
    ) -> Option<(IconLayout, gpui::Bounds<gpui::Pixels>)> {
        let (screen, spot) = self.ghostex_capture_home(cx)?;
        let counts = self.ghostex_capture_counts();
        let mut layout = placement::resting_layout(&screen, spot, counts);
        if let IconLayout::Docked(edge) = layout
            && (self.ghostex_capture.hovered
                || self.ghostex_capture.panel.is_some()
                || self.ghostex_capture.panel_opening)
        {
            layout = IconLayout::DockedOpen(edge);
        }
        Some((
            layout,
            placement::window_frame(&screen, spot, layout, counts),
        ))
    }

    pub(super) fn open_ghostex_capture_icon(&mut self, cx: &mut Context<Self>) {
        if self.ghostex_capture.icon.is_some() || self.ghostex_capture.icon_opening {
            return;
        }
        let Some((layout, frame)) = self.ghostex_capture_icon_target(cx) else {
            return;
        };
        self.ghostex_capture.icon_opening = true;
        let app = cx.weak_entity();
        // GPUI draws a new window's root synchronously; opening it inside this entity's update
        // would lease the app twice (the floating sidebar's September 9 crash).
        App::defer(cx, move |cx| {
            let (bounds, display_id) = crate::app::window::popup_frame::place_global(frame, cx);
            let observed = app.clone();
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: false,
                    show: true,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    // Without this macOS rounds the window's own corners, which turned the narrow
                    // docked tab into a pointed lens.
                    crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                    cx.new(|cx| CaptureIconView {
                        _observe: match observed.upgrade() {
                            Some(app) => cx.observe(&app, |_, _, cx| cx.notify()),
                            None => Subscription::new(|| {}),
                        },
                        app: observed.clone(),
                        background: WindowBackgroundAppearance::Transparent,
                        blur_region: Vec::new(),
                    })
                },
            );
            let Some(app) = app.upgrade() else {
                return;
            };
            let Ok(handle) = result else {
                app.update(cx, |app, _| app.ghostex_capture.icon_opening = false);
                return;
            };
            let native = handle
                .update(cx, |_, window, _| {
                    crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| (view as usize, window.scale_factor()))
                })
                .ok()
                .flatten();
            let Some((native, scale)) = native else {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                app.update(cx, |app, _| app.ghostex_capture.icon_opening = false);
                return;
            };
            platform::prepare_floating_window(native, false);
            platform::set_window_frame(native, frame, scale);
            app.update(cx, |app, cx| {
                app.ghostex_capture.icon_opening = false;
                app.ghostex_capture.icon = Some(IconWindow {
                    handle,
                    native,
                    scale,
                    frame,
                    layout,
                });
                app.sync_ghostex_capture_icon(cx);
            });
        });
    }

    pub(super) fn close_ghostex_capture_icon(&mut self, cx: &mut Context<Self>) {
        self.ghostex_capture.press = None;
        self.ghostex_capture.hovered = false;
        if let Some(icon) = self.ghostex_capture.icon.take() {
            let handle = icon.handle;
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// Moves or resizes the button to where its state says it belongs.
    pub(super) fn sync_ghostex_capture_icon(&mut self, cx: &mut Context<Self>) {
        if self
            .ghostex_capture
            .press
            .as_ref()
            .is_some_and(|press| press.dragging)
        {
            return;
        }
        let Some((layout, frame)) = self.ghostex_capture_icon_target(cx) else {
            return;
        };
        self.apply_ghostex_capture_icon_frame(layout, frame, cx);
    }

    fn apply_ghostex_capture_icon_frame(
        &mut self,
        layout: IconLayout,
        frame: gpui::Bounds<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(icon) = self.ghostex_capture.icon.as_mut() else {
            return;
        };
        let resized = icon.frame.size != frame.size;
        if icon.frame == frame && icon.layout == layout {
            return;
        }
        icon.frame = frame;
        icon.layout = layout;
        platform::set_window_frame(icon.native, frame, icon.scale);
        if resized {
            // GPUI learns a new size only from the platform's resize callback, which gives up
            // while the app is borrowed; re-read the bounds once this update has ended.
            let handle = icon.handle;
            App::defer(cx, move |cx| {
                let _ = handle.update(cx, |_, window, cx| window.bounds_changed(cx));
            });
        }
        cx.notify();
    }

    fn ghostex_capture_icon_pressed(&mut self, cx: &mut Context<Self>) {
        let Some(icon) = self.ghostex_capture.icon.as_ref() else {
            return;
        };
        let Some(pointer) = platform::pointer(icon.scale) else {
            return;
        };
        let counts = self.ghostex_capture_counts();
        self.ghostex_capture.press = Some(IconPress {
            pointer,
            icon_origin: icon.frame.origin + placement::icon_offset(icon.layout, counts),
            dragging: false,
        });
        // Remember the app the user is in before anything of ours can take the keyboard.
        self.ghostex_capture.frontmost = platform::frontmost_app();
        let _ = cx;
    }

    fn ghostex_capture_icon_dragged(&mut self, cx: &mut Context<Self>) {
        let Some(icon) = self.ghostex_capture.icon.as_ref() else {
            return;
        };
        let scale = icon.scale;
        let Some(pointer) = platform::pointer(scale) else {
            return;
        };
        let Some(press) = self.ghostex_capture.press.as_mut() else {
            return;
        };
        let delta = pointer - press.pointer;
        if !press.dragging
            && f32::from(delta.x).abs() < DRAG_THRESHOLD
            && f32::from(delta.y).abs() < DRAG_THRESHOLD
        {
            return;
        }
        press.dragging = true;
        let icon_now = press.icon_origin + delta;
        let counts = self.ghostex_capture_counts();
        // While dragging, the button is drawn floating with the tray on the side it will rest on.
        let layout = match placement::screen_at(icon_now, cx) {
            Some(screen) => {
                let spot = placement::placement_for(&screen, icon_now, None);
                placement::resting_layout(&screen, spot, counts)
            }
            None => IconLayout::Floating { tray_left: false },
        };
        let size = placement::window_size(layout, counts);
        let frame = gpui::Bounds::new(icon_now - placement::icon_offset(layout, counts), size);
        self.apply_ghostex_capture_icon_frame(layout, frame, cx);
    }

    fn ghostex_capture_icon_released(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.ghostex_capture.press.take() else {
            return;
        };
        if !press.dragging {
            self.run_ghostex_capture_action(CaptureAction::TogglePanel, cx);
            return;
        }
        let Some(icon) = self.ghostex_capture.icon.as_ref() else {
            return;
        };
        let counts = self.ghostex_capture_counts();
        let icon_now = icon.frame.origin + placement::icon_offset(icon.layout, counts);
        let Some(screen) = placement::screen_at(icon_now, cx)
            .or_else(|| self.ghostex_capture_home(cx).map(|(screen, _)| screen))
        else {
            return;
        };
        let dock = placement::dock_edge_for(&screen, icon_now);
        let spot = placement::placement_for(&screen, icon_now, dock);
        let saved = &mut self.ghostex_capture.saved;
        saved.display = Some(screen.key.clone());
        saved.placements.insert(screen.key.clone(), spot);
        persistence::save(saved);
        self.ghostex_capture.hovered = dock.is_some();
        self.sync_ghostex_capture_icon(cx);
        self.sync_ghostex_capture_panel_frame(cx);
    }

    fn ghostex_capture_icon_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.ghostex_capture.hover_left_at = None;
            if !self.ghostex_capture.hovered {
                self.ghostex_capture.hovered = true;
                self.sync_ghostex_capture_icon(cx);
            }
            return;
        }
        let left_at = Instant::now();
        self.ghostex_capture.hover_left_at = Some(left_at);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DOCK_CLOSE_DELAY).await;
            let _ = this.update(cx, |app, cx| {
                if app.ghostex_capture.hover_left_at == Some(left_at)
                    && app.ghostex_capture.press.is_none()
                {
                    app.ghostex_capture.hovered = false;
                    app.sync_ghostex_capture_icon(cx);
                }
            });
        })
        .detach();
    }
}

fn count_text(value: u64, color: u32, size: f32) -> gpui::Div {
    div()
        .text_size(px(size))
        .font_weight(gpui::FontWeight::BOLD)
        .font_family("Menlo")
        .text_color(rgb(color))
        .child(if value > 99 {
            "99+".to_string()
        } else {
            value.to_string()
        })
}

impl CaptureIconView {
    /// Blurs what is behind the tray or the docked tab, and only there: `region` is their shape,
    /// or `None` when they are not frosted or not shown (an empty region would blur the whole
    /// window, the icon's clear surroundings included).
    fn sync_glass(&mut self, window: &mut Window, region: Option<Vec<(Bounds<Pixels>, Pixels)>>) {
        let background = if region.is_some() {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Transparent
        };
        if let Some(region) = region
            && region != self.blur_region
        {
            // Set before the blur is, so the blur never covers the whole window.
            window.set_background_blur_region(region.clone());
            self.blur_region = region;
        }
        if background != self.background {
            window.set_background_appearance(background);
            self.background = background;
        }
        // The button stays up, so it follows the Menu blur slider here rather than at open
        // (CDXC:Theming 2026-10-15 on `apply_frosted_menu_blur`).
        #[cfg(target_os = "macos")]
        if background == WindowBackgroundAppearance::Blurred {
            crate::app::helpers::apply_frosted_menu_blur(window);
        }
    }

    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut GhostexGpuiApp, &mut Context<GhostexGpuiApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }
}

/// Moves the button with a held left button, wherever the pointer is.
///
/// CDXC:GhostexCapture 2026-10-24 WHY:
/// The drag listens window-wide instead of through the button's own `on_mouse_move`: GPUI calls an
/// element's move listener only while the pointer is over its hitbox, so a fast drag left the
/// small window behind and it stopped until the pointer came back over it. Every backend keeps
/// sending a held drag to the window it started in (AppKit routes `mouseDragged:` to the pressed
/// view, Windows has GPUI's `SetCapture`, X11 its implicit grab), and the frame is computed from
/// the OS pointer, so the button stays under the pointer at any speed until the release. A canvas
/// registers no hitbox, so nothing under it loses input.
fn drag_tracker(view: WeakEntity<CaptureIconView>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture
                    || event.pressed_button != Some(MouseButton::Left)
                {
                    return;
                }
                let _ = view.update(cx, |this, cx| {
                    this.with_app(cx, |app, cx| app.ghostex_capture_icon_dragged(cx));
                });
            });
        },
    )
    .absolute()
    .inset_0()
}

impl Render for CaptureIconView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .id("ghostex-capture-button")
            .size_full()
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.with_app(cx, |app, cx| app.ghostex_capture_icon_pressed(cx));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.with_app(cx, |app, cx| app.ghostex_capture_icon_released(cx));
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.with_app(cx, |app, cx| app.ghostex_capture_icon_released(cx));
                }),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                let hovered = *hovered;
                this.with_app(cx, move |app, cx| {
                    app.ghostex_capture_icon_hover(hovered, cx)
                });
            }))
            .child(drag_tracker(cx.weak_entity()));
        let Some(app) = self.app.upgrade() else {
            self.sync_glass(window, None);
            return root;
        };
        let app = app.read(cx);
        let Some(icon) = app.ghostex_capture.icon.as_ref() else {
            self.sync_glass(window, None);
            return root;
        };
        let glass = button_glass();
        let palette = TrayPalette::resolve(glass);
        let layout = icon.layout;
        let counts = app.ghostex_capture_counts();
        let active = app.ghostex_capture.panel.is_some();
        let shown = placement::shown_counts(counts);

        if let IconLayout::Docked(edge) = layout {
            let tab = div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(1.0))
                .bg(palette.tab_fill(glass))
                .border_1()
                .border_color(palette.tab_edge);
            let region = glass.then(|| {
                one_side_rounded(
                    Bounds::new(point(px(0.0), px(0.0)), window.viewport_size()),
                    px(10.0),
                    edge == DockEdge::Left,
                )
            });
            self.sync_glass(window, region);
            let tab = match edge {
                DockEdge::Right => tab.rounded_l(px(10.0)).border_r_0(),
                DockEdge::Left => tab.rounded_r(px(10.0)).border_l_0(),
            };
            let tab = if shown.is_empty() {
                tab.child(match app_icon_image() {
                    Some(image) => img(ImageSource::Render(image))
                        .size(px(14.0))
                        .into_any_element(),
                    None => div().into_any_element(),
                })
            } else {
                tab.children(
                    shown
                        .iter()
                        .map(|(value, color)| count_text(*value, *color, 11.0).into_any_element()),
                )
            };
            return root.child(tab);
        }

        let offset = placement::icon_offset(layout, counts);
        let tray_left = placement::tray_on_left(layout);
        let tray_width = placement::tray_width(counts);
        let mut root = root;
        let tray_x = if tray_left {
            offset.x + px(TRAY_TUCK) - px(tray_width)
        } else {
            offset.x + px(ICON_SIZE - TRAY_TUCK)
        };
        // The tray is rounded on its outer side; the side tucked under the icon is square.
        let region = (glass && tray_width > 0.0).then(|| {
            one_side_rounded(
                Bounds::new(
                    point(tray_x, px(ICON_PAD)),
                    gpui::size(px(tray_width), px(ICON_SIZE)),
                ),
                px(12.0),
                !tray_left,
            )
        });
        self.sync_glass(window, region);
        if tray_width > 0.0 {
            let rows = shown.len() as f32;
            let tray = div()
                .absolute()
                .top(px(ICON_PAD))
                .h(px(ICON_SIZE))
                .w(px(tray_width))
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap(px(if rows >= 3.0 { 0.0 } else { 2.0 }))
                .left(tray_x)
                .bg(palette.fill)
                .border_1()
                .border_color(if active {
                    rgba(0x86d3f859).into()
                } else {
                    palette.edge
                })
                .children(shown.iter().map(|(value, color)| {
                    count_text(*value, *color, if rows >= 3.0 { 11.0 } else { 12.5 })
                        .line_height(px(if rows >= 3.0 { 13.0 } else { 15.0 }))
                        .into_any_element()
                }));
            let tray = if tray_left {
                tray.pr(px(TRAY_TUCK)).rounded_l(px(12.0)).border_r_0()
            } else {
                tray.pl(px(TRAY_TUCK)).rounded_r(px(12.0)).border_l_0()
            };
            root = root.child(tray);
        }
        let mut icon = div()
            .absolute()
            .left(offset.x)
            .top(offset.y)
            .size(px(ICON_SIZE))
            .rounded(px(ICON_SIZE * 0.23))
            .shadow_md();
        if let Some(image) = app_icon_image() {
            icon = icon.child(img(ImageSource::Render(image)).size(px(ICON_SIZE)));
        }
        if active {
            icon = icon.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(ICON_SIZE * 0.23))
                    .border(px(1.5))
                    .border_color(rgb(0x86d3f8)),
            );
        }
        root.child(icon)
    }
}
