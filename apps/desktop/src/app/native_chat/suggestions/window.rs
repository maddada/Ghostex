use super::super::{appearance::ChatAppearance, state::NativeChatView};
use super::layout::{SPEC, suggestion_panel_height};
use gpui::{
    AppContext as _, Bounds, Context, Entity, Pixels, ScrollHandle, Styled as _, Subscription,
    WindowBounds, WindowOptions, point, px, size,
};
use gpui_component::Root;
use serde_json::json;

#[derive(Default)]
pub(in crate::app::native_chat) struct SuggestionWindowState {
    handle: Option<gpui::WindowHandle<Root>>,
    /// The open card in the chat window's content coordinates, on device pixels.
    pub(super) bounds: Option<Bounds<Pixels>>,
    opening: bool,
    inline: Option<gpui::WeakEntity<SuggestionPanel>>,
}

impl SuggestionWindowState {
    pub(in crate::app::native_chat) fn is_open(&self) -> bool {
        self.bounds.is_some()
    }

    /// The card inside its window, whose frame is `window_frame` of the card.
    pub(super) fn card_in_window(&self) -> Option<Bounds<Pixels>> {
        self.bounds
            .map(|card| Bounds::new(card.origin - window_frame(card).origin, card.size))
    }
}

pub(in crate::app::native_chat) struct SuggestionPanel {
    pub(super) chat: Entity<NativeChatView>,
    pub(super) source: gpui::AnyWindowHandle,
    pub(super) scroll: ScrollHandle,
    pub(super) selected: Option<usize>,
    /// The child window's panel, whose card is inset by `card_in_window`; the maximized
    /// composer's inline panel fills its slot.
    pub(super) windowed: bool,
    _subscription: Subscription,
}

impl NativeChatView {
    pub(in crate::app::native_chat) fn inline_suggestions(
        &mut self,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SuggestionPanel>> {
        if self.snapshot["suggestions"].is_null() {
            return None;
        }
        let source = window.window_handle();
        if let Some(panel) = self
            .suggestions
            .inline
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
        {
            panel.update(cx, |panel, _| panel.source = source);
            return Some(panel);
        }
        let chat = cx.entity();
        let panel = cx.new(|cx| {
            let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
            SuggestionPanel {
                chat,
                source,
                scroll: Default::default(),
                selected: None,
                windowed: false,
                _subscription: subscription,
            }
        });
        self.suggestions.inline = Some(panel.downgrade());
        Some(panel)
    }

    pub(in crate::app::native_chat) fn update_suggestion_selection(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = &self.input else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let offset = input.read(cx).cursor().min(text.len());
        let caret = text[..offset].encode_utf16().count();
        if self
            .suggestion_selection
            .as_ref()
            .is_some_and(|old| old.0 == text && old.1 == caret)
        {
            return;
        }
        self.suggestion_selection = Some((text.clone(), caret));
        self.invoke(
            json!({"type":"composerSelection","text":text,"caret":caret}),
            cx,
        );
    }

    /// Closes the list's window so the next sync opens a fresh one, attached to the window the chat
    /// is drawn in now (`child_window_parent`).
    pub(in crate::app::native_chat) fn close_suggestion_window(&mut self, cx: &mut Context<Self>) {
        self.suggestions.bounds = None;
        if let Some(handle) = self.suggestions.handle.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.sync_suggestion_window(cx);
    }

    /// CDXC:SessionChat 2026-09-19 WHY:
    /// React draws the `@`, `$` and `/` list inside the composer, so nothing outside the composer
    /// can hide it. This one is a child window and two conditions outside the composer used to be
    /// able to: it opened only while the app shell called the whole pane focused, and it opened
    /// without a display. GPUI reads a window's bounds relative to the display it is on but places
    /// a new window relative to `display_id`, falling back to the display that owns the menu bar,
    /// so on a two-display computer the popup landed on the other screen. Both read to the user as
    /// the feature being missing. Every other chat child window passes its display the same way.
    pub(crate) fn sync_suggestion_window(&mut self, cx: &mut Context<Self>) {
        if self.suggestions.opening {
            return;
        }
        let source = self
            .maximized_window
            .map(|window| window.into())
            .or(self.main_window);
        let Some(source) = source else {
            return;
        };
        let p = ChatAppearance::current(&self.snapshot);
        let anchor = self.composer_bounds.get();
        let projection = &self.snapshot["suggestions"];
        // React shows the picker whenever the trigger sits under the caret, with no pane-level
        // condition of its own. The composer's own keyboard focus is the native equivalent; the
        // app shell's pane-focus flag alone missed every moment it lagged the field (a first
        // responder the shell classifies as `Other`, a companion pane, a pane whose focus border
        // is held elsewhere), which read as the feature being missing.
        let visible = !self.pane_hidden
            && self.maximized_window.is_none()
            && (self.pane_focused || self.composer_focused)
            && projection.is_object()
            && self.snapshot["questionCard"]["visible"] != true
            && anchor.size.width > px(0.0);
        // `anchor` has the composer card's painted left and right edges and, as its top, the top
        // of the panels stacked on the card (composer.rs). React's `inset-x-0 bottom-full mb-2`
        // gives the list those edges and a fixed gap above that top.
        // React draws the picker inside the chat pane, so it can never grow past the pane's top;
        // the window is clamped to the same room.
        let gap = px(SPEC.gap_above_px * p.scale);
        let room = (anchor.top() - gap - self.bounds.get().top()).max(px(0.0));
        let height = suggestion_panel_height(projection, p.scale).min(room);
        let card = Bounds::new(
            point(anchor.left(), anchor.top() - gap - height),
            size(anchor.size.width, height),
        );
        let chat = cx.entity();
        let parent = self.child_window_parent(cx);
        // Under window glass the popup's window blurs what is behind it, rounded to the card.
        let glass = crate::app::helpers::window_glass_active_for(Some(source));
        let corner_radius = px(SPEC.radius_px * p.scale);
        self.suggestions.opening = true;
        cx.defer(move |cx| {
            let placement = visible
                .then(|| {
                    source
                        .update(cx, |_, window, cx| {
                            let origin = super::super::child_window::content_bounds(window).origin;
                            // A picker floating over an app the user has switched away from is
                            // not what React's in-page list does, so an inactive window keeps it
                            // shut.
                            window.is_window_active().then(|| {
                                (
                                    snap_to_device_pixels(card, window.scale_factor()),
                                    origin,
                                    window.display(cx).map(|display| display.id()),
                                )
                            })
                        })
                        .ok()
                        .flatten()
                })
                .flatten()
                .filter(|(card, ..)| card.size.height > px(0.0));
            let wanted = placement.map(|(card, ..)| card);
            if chat.read(cx).suggestions.bounds == wanted {
                chat.update(cx, |chat, _| chat.suggestions.opening = false);
                return;
            }
            let old = chat.update(cx, |chat, _| {
                chat.suggestions.bounds = wanted;
                chat.suggestions.handle.take()
            });
            // An open popup follows the card in place (pane resize, sidebar toggle, a growing
            // draft) instead of being closed and reopened, which flickered on every step.
            let old = match old {
                Some(handle)
                    if wanted.is_some_and(|card| {
                        super::super::child_window::move_child_window(
                            handle.into(),
                            parent,
                            window_frame(card),
                            cx,
                        )
                    }) =>
                {
                    chat.update(cx, |chat, cx| {
                        chat.suggestions.handle = Some(handle);
                        chat.suggestions.opening = false;
                        if chat.composer_bounds.get() != anchor {
                            chat.sync_suggestion_window(cx);
                        }
                    });
                    return;
                }
                old => old,
            };
            if let Some(old) = old {
                let _ = old.update(cx, |_, window, _| window.remove_window());
            }
            let result = placement.map(|(card, origin, display_id)| {
                let frame = window_frame(card);
                let display_id = crate::app::window::popup_frame::display_at(
                    Bounds::new(origin + frame.origin, frame.size).center(),
                    cx,
                )
                .or(display_id);
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            origin + frame.origin,
                            frame.size,
                        ))),
                        display_id,
                        titlebar: None,
                        kind: gpui::WindowKind::PopUp,
                        focus: false,
                        show: true,
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        app_id: crate::gpui_platform_window_app_id(),
                        icon: crate::gpui_platform_window_icon(),
                        window_background: if glass {
                            gpui::WindowBackgroundAppearance::Blurred
                        } else {
                            gpui::WindowBackgroundAppearance::Transparent
                        },
                        ..Default::default()
                    },
                    {
                        let chat = chat.clone();
                        move |window, cx| {
                            window.set_background_corner_radius(corner_radius);
                            crate::app::helpers::apply_frosted_menu_blur(window);
                            attach_suggestion_window(window, parent);
                            let panel = cx.new(|cx| {
                                let subscription = cx.observe(&chat, |_, _, cx| cx.notify());
                                SuggestionPanel {
                                    chat,
                                    source,
                                    scroll: Default::default(),
                                    selected: None,
                                    windowed: true,
                                    _subscription: subscription,
                                }
                            });
                            cx.new(|cx| Root::new(panel, window, cx).bg(gpui::transparent_black()))
                        }
                    },
                )
            });
            chat.update(cx, |chat, cx| {
                chat.suggestions.opening = false;
                match result {
                    Some(Ok(handle)) => chat.suggestions.handle = Some(handle),
                    Some(Err(error)) => {
                        chat.error = Some(error.to_string());
                        chat.suggestions.bounds = None;
                    }
                    None => {}
                }
                // The card may have moved while this deferred step ran; its measurement was
                // dropped by the `opening` guard, so catch up now.
                if chat.composer_bounds.get() != anchor {
                    chat.sync_suggestion_window(cx);
                }
                cx.notify();
            });
        });
    }
}

impl SuggestionPanel {
    pub(super) fn choose(&mut self, command: serde_json::Value, cx: &mut Context<Self>) {
        let source = self.source;
        let chat = self.chat.clone();
        cx.defer(move |cx| {
            let _ = source.update(cx, |_, window, cx| {
                chat.update(cx, |chat, cx| {
                    chat.invoke(command, cx);
                    chat.focus_requested = true;
                    chat.ensure_input(window, cx);
                });
            });
        });
    }
}

#[cfg(target_os = "macos")]
fn attach_suggestion_window(window: &mut gpui::Window, parent: *mut std::ffi::c_void) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn GhostexGpuiAttachComposerSuggestionsWindow(
            view: *mut std::ffi::c_void,
            parent: *mut std::ffi::c_void,
        );
    }
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
            unsafe {
                GhostexGpuiAttachComposerSuggestionsWindow(handle.ns_view.as_ptr(), parent);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn attach_suggestion_window(_: &mut gpui::Window, _: *mut std::ffi::c_void) {}

/// Each edge on its nearest device pixel, all four taken from the same rect.
fn snap_to_device_pixels(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    let snap = |value: Pixels| px((value.as_f32() * scale).round() / scale);
    Bounds::from_corners(
        point(snap(bounds.left()), snap(bounds.top())),
        point(snap(bounds.right()), snap(bounds.bottom())),
    )
}

/// CDXC:SessionChat 2026-09-19 WHY:
/// AppKit places a window on whole points, so at a fractional zoom a card edge on a half point landed one device pixel off whichever edge the frame's rounding moved. The window takes the whole points around the card and the card is inset inside it by the remainder, so both edges sit on the composer card's own device pixels.
fn window_frame(card: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(card.left().floor(), card.top().floor()),
        point(card.right().ceil(), card.bottom().ceil()),
    )
}
