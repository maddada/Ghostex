//! A dock row's ask in a floating panel over the chat (work_cards.rs `dock_row`), opened the way a
//! Ghostex page opens from the chat: a window of its own over the main window, with the page
//! window's look (window/visual_page_modal.rs): the page's backdrop (the app's softened wallpaper
//! under the work area's tint under window glass, the page's top colour without), its 36pt top
//! band that moves the window with the round glass controls in its corners, and edges that resize
//! it. The chat stays drawn behind it, whole.
//!
//! In it: the row's mark and title whole, its age, the work it belongs to, its whole Markdown
//! description and where it stands, then its item pills and threads at the left of its answers
//! (the recommended one starred) and Reply after a divider, or its Open / Reply when it has no
//! answers. The ⋯ in the band holds the row's other actions (Show in chat, Dismiss, Snooze, Copy
//! as Markdown, Mention in chat box), as the row's ⋯ does.
//!
//! An answer, Open, a link to the work or a pick in the ⋯ closes it as it goes, back to the dock;
//! a Reply sent closes it once the row says it was replied to (the reply box opens over the panel,
//! so the panel stays while it is typed). Escape and the X close it; Enter picks nothing. It closes
//! by itself, whole (its glass goes with its ask), when its row leaves the dock (answered or
//! withdrawn elsewhere, dismissed, snoozed, its item dropped or done) and when the chat is put away
//! for another session; an ask settled elsewhere while it was open says so in a toast, "Answered"
//! or "Withdrawn". It opens over the chat at the ask's own height (it fits itself
//! to its text, up to most of the main window, until the reader resizes it).
//!
//! CDXC:Coordinators 2026-11-14 DECISION:
//! User, of an ask row in the dock opening in place: "i want when i click on a thing like this one
//! to open modal instead of expanding like accordion because accordion is annoying pls fix". Then,
//! of the first modal (drawn inside the chat pane, which stopped painting the chat under glass):
//! "when i said make these appear not as expanded in accordion, then we should do this just like
//! inline html panel that appears, and not hide the chat that's already there. fix pls". The ask
//! is a floating panel like a Ghostex page's, with the chat fully visible behind it. The phone shows
//! the same ask in a sheet (apps/mobile/app/src/chat/native/cards/WorkCards.tsx `DockRow`).

use super::chat_text_look::{PROSE_LINE_HEIGHT, PROSE_SIZE};
use super::work_cards::{OPENED_ROW_GAP, answers_divider, pill_fit, pill_widths, row_mark};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::window::page_band_geometry::{CONTROL_INSET, TOP_BAND_HEIGHT};
use crate::app::window::page_controls::{ControlGlass, control_button};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Subscription, Task, Window, WindowBounds, WindowOptions, div, px, svg,
};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

/// The command a pick in the panel's ⋯ menu is wrapped in, so the panel closes as it goes
/// (`unwrap_dock_ask_command`, from `invoke`).
const THEN_CLOSE: &str = "dockAskModalThen";

/// The panel's width, and the height it opens at before it has measured its text.
const PANEL_WIDTH: f32 = 680.0;
const PANEL_OPEN_HEIGHT: f32 = 420.0;
/// The shortest it fits itself to, and the most of the main window's height it takes.
const PANEL_MIN_HEIGHT: f32 = 200.0;
const PANEL_MAX_SHARE: f32 = 0.85;
const PANEL_PAD_X: f32 = 20.0;
/// The text's indent under the title, past the mark.
const TEXT_LEAD: f32 = 24.0;

/// The page window's backdrop colours without a picture (window/visual_page_modal/glass.rs).
#[cfg(not(ghostex_web))]
use crate::app::window::visual_page_modal::{PAGE_TOP_DARK, PAGE_TOP_LIGHT};
#[cfg(ghostex_web)]
const PAGE_TOP_DARK: u32 = 0x131313;
#[cfg(ghostex_web)]
const PAGE_TOP_LIGHT: u32 = 0xf0f0f0;

/// A menu's rows with each command wrapped in `THEN_CLOSE`, a submenu's (Snooze's lengths) too.
fn wrap_menu_rows(rows: Vec<Value>) -> Vec<Value> {
    rows.into_iter()
        .map(|mut row| {
            if row["command"].is_object() {
                row["command"] = json!({ "type": THEN_CLOSE, "command": row["command"].take() });
            }
            if let Some(children) = row["children"].as_array().cloned() {
                row["children"] = Value::Array(wrap_menu_rows(children));
            }
            row
        })
        .collect()
}

/// The height the panel fits itself to: what it measured of its band, title, text and foot, kept
/// between `PANEL_MIN_HEIGHT` and `PANEL_MAX_SHARE` of the main window's height.
fn fitted_height(natural: f32, main_height: f32) -> f32 {
    let most = (main_height * PANEL_MAX_SHARE).max(PANEL_MIN_HEIGHT);
    natural.clamp(PANEL_MIN_HEIGHT, most)
}

/// The toast an ask settled while its panel was open shows (`sync_dock_ask_modal`), from how
/// the core says it went (`workAsksSettled`): its level and title.
fn settled_toast(how: &str) -> Option<(&'static str, &'static str)> {
    match how {
        "answered" => Some(("success", "Answered")),
        "withdrawn" => Some(("info", "Withdrawn")),
        _ => None,
    }
}

/// The picture the panel paints behind itself under window glass (`panel_backdrop`).
struct PanelBackdrop {
    image: std::sync::Arc<gpui::Image>,
    rect: gpui::Bounds<Pixels>,
    tint: gpui::Rgba,
}

/// The panel window's view: the chat draws what is in it (`dock_ask_panel`).
struct DockAskPanel {
    chat: Entity<NativeChatView>,
    focus: FocusHandle,
    backdrop: Option<PanelBackdrop>,
    /// The natural heights measured at the last paint: band and title, the text, the foot.
    measured: Rc<Cell<(f32, f32, f32)>>,
    /// The height the panel last fitted itself to; once the reader resizes it, it stops fitting.
    fitted: Option<f32>,
    /// The main window's height when the panel opened, which caps how tall it fits itself.
    main_height: f32,
    _chat: Subscription,
    _glass: Option<Task<()>>,
    _bounds: Option<Subscription>,
}

impl DockAskPanel {
    /// Follows the main window's glass picture, as the page window does: once a second, and at
    /// once on a move or resize.
    fn follow_backdrop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_backdrop(window, cx);
        self._glass = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.sync_backdrop(window, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
        self._bounds = Some(cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_backdrop(window, cx);
        }));
    }

    #[cfg(not(ghostex_web))]
    fn sync_backdrop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = crate::app::window::visual_page_modal::panel_backdrop(window, cx).map(
            |(jpeg, rect, tint)| PanelBackdrop {
                image: match &self.backdrop {
                    Some(old) if old.image.bytes() == jpeg.as_slice() => old.image.clone(),
                    _ => {
                        std::sync::Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Jpeg, jpeg))
                    }
                },
                rect,
                tint,
            },
        );
        let changed = match (&self.backdrop, &next) {
            (Some(old), Some(new)) => {
                old.rect != new.rect
                    || old.tint != new.tint
                    || !std::sync::Arc::ptr_eq(&old.image, &new.image)
            }
            (None, None) => false,
            _ => true,
        };
        self.backdrop = next;
        if changed {
            cx.notify();
        }
    }

    /// The web build has no window glass: the panel keeps the page's own colour.
    #[cfg(ghostex_web)]
    fn sync_backdrop(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Fits the window to its ask's height about its centre, until the reader resizes it.
    fn fit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (head, text, foot) = self.measured.get();
        if head <= 0.0 {
            return;
        }
        let height = f32::from(window.bounds().size.height);
        if self
            .fitted
            .is_some_and(|fitted| (fitted - height).abs() > 1.0)
        {
            // The reader resized it: their height stands.
            return;
        }
        let main_height = self.main_height;
        let want = fitted_height(head + text + foot, main_height);
        self.fitted = Some(want);
        if (want - height).abs() <= 1.0 {
            return;
        }
        let width = window.bounds().size.width;
        // Moved outside the update, as the page window's settle is: AppKit reports the new frame
        // synchronously, and GPUI drops that report while the app is borrowed.
        #[cfg(all(target_os = "macos", not(ghostex_web)))]
        {
            let Ok(native) = crate::app::helpers::cef_parent_native_view(window) else {
                return;
            };
            let native = native as usize;
            let frame = crate::app::window::visual_page_modal::frame::shared_frame(window);
            // About the centre it opened at, over the chat.
            let to = gpui::Bounds::new(
                gpui::point(frame.origin.x, frame.center().y - px(want / 2.0)),
                gpui::size(width, px(want)),
            );
            let scale = window.scale_factor();
            cx.spawn_in(window, async move |_, _| {
                crate::app::ghostex_capture::platform::set_window_frame(native, to, scale);
            })
            .detach();
        }
        #[cfg(not(all(target_os = "macos", not(ghostex_web))))]
        window.resize(gpui::size(width, px(want)));
    }
}

impl Render for DockAskPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(all(target_os = "macos", not(ghostex_web)))]
        crate::app::helpers::apply_frosted_menu_blur(window);
        self.fit(window, cx);
        let measured = self.measured.clone();
        let content = self.chat.update(cx, |chat, cx| {
            let glass = crate::app::helpers::window_glass_active_for(chat.main_window);
            let p = ChatAppearance::current(&chat.snapshot).on_window_glass(glass);
            chat.dock_ask_panel(&p, measured, window, cx)
                .map(|content| (content, p.light, p.font.clone()))
        });
        let Some((content, light, font)) = content else {
            // Its row left the dock, or the panel is closing: nothing is left to show, so the
            // window goes with it. The backdrop is this window's own (its blur and picture), so a
            // panel with no ask removes itself here instead of waiting on the chat to close it;
            // an empty glass window must never stay over the chat.
            window.remove_window();
            return div().size_full().into_any_element();
        };
        let chat = self.chat.clone();
        div()
            .id("dock-ask-panel")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(font)
            .bg(gpui::rgb(if light {
                PAGE_TOP_LIGHT
            } else {
                PAGE_TOP_DARK
            }))
            .on_key_down(move |event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && !event.is_held {
                    chat.update(cx, |chat, cx| chat.close_dock_ask_modal(cx));
                    cx.stop_propagation();
                }
            })
            .when_some(self.backdrop.as_ref(), |this, backdrop| {
                this.child(
                    div()
                        .absolute()
                        .left(backdrop.rect.origin.x)
                        .top(backdrop.rect.origin.y)
                        .w(backdrop.rect.size.width)
                        .h(backdrop.rect.size.height)
                        .child(
                            gpui::img(backdrop.image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Fill),
                        ),
                )
                .child(div().absolute().inset_0().bg(backdrop.tint))
            })
            .child(content)
            .into_any_element()
    }
}

impl NativeChatView {
    /// The row the panel shows, as the dock has it now, or `None` when it has left the dock.
    fn dock_ask_row(&self) -> Option<Value> {
        let key = self.board.dock_modal.as_deref()?;
        self.snapshot["workDock"]["rows"]
            .as_array()?
            .iter()
            .find(|row| row["key"].as_str() == Some(key))
            .cloned()
    }

    /// Closes the panel once its ask has left the dock or a Reply sent from it was taken, as soon
    /// as the chat's snapshot says so (`NativeChatView::apply_output`), whether or not the panel's
    /// own window draws again. An ask the coordinator, the phone or another computer settled while
    /// it was open says how it went in a toast: "Answered" (an answer, a Reply or Mark done) or
    /// "Withdrawn". A row that left for another reason (snoozed elsewhere, a refresh) closes it
    /// quietly.
    pub(super) fn sync_dock_ask_modal(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.board.dock_modal.clone() else {
            return;
        };
        match self.dock_ask_row() {
            Some(row) if text(&row, "replied") == self.board.dock_modal_replied => {}
            // The Reply sent from the panel was taken: the reader did it, no toast.
            Some(_) => self.close_dock_ask_modal(cx),
            None => {
                let how = self.snapshot["workAsksSettled"][key.as_str()]
                    .as_str()
                    .and_then(settled_toast);
                self.close_dock_ask_modal(cx);
                if let Some((level, title)) = how {
                    cx.emit(super::state::NativeChatEvent::Host(
                        json!({ "type": "toast", "level": level, "title": title }),
                    ));
                }
            }
        }
    }

    /// Shows the row `key`'s ask in the panel, opening the panel over the chat when it is not up.
    pub(super) fn open_dock_ask_modal(
        &mut self,
        key: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.board.dock_modal = Some(key.to_owned());
        self.board.dock_modal_replied = self
            .dock_ask_row()
            .map(|row| text(&row, "replied"))
            .unwrap_or_default();
        cx.notify();
        if let Some(handle) = self.board.dock_modal_window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }
        if self.board.dock_modal_opening {
            return;
        }
        let Some(main) = self.open_main_window(cx) else {
            return;
        };
        self.board.dock_modal_opening = true;
        let pane = self.bounds.get();
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let parent_native_view = self.child_window_parent(cx);
        let chat = cx.entity();
        #[cfg(target_os = "linux")]
        let owner = main;
        // The click arrives while the main window is mid-update: its frame is read in the deferred
        // pass (maximized.rs `sync_maximized_window`).
        cx.defer(move |cx| {
            let Ok((bounds, popup_owner, main_height)) = main.update(cx, |_, window, cx| {
                let content = super::child_window::content_bounds(window);
                let area = if pane.size.width > px(0.0) {
                    gpui::Bounds::new(content.origin + pane.origin, pane.size)
                } else {
                    content
                };
                let width = px(PANEL_WIDTH * scale).min(area.size.width - px(32.0));
                let height = px(PANEL_OPEN_HEIGHT * scale).min(area.size.height - px(32.0));
                (
                    gpui::Bounds::centered_at(area.center(), gpui::size(width, height)),
                    crate::app::window::popup_frame::PopupOwner::of(window, cx),
                    f32::from(content.size.height),
                )
            }) else {
                chat.update(cx, |this, _| this.board.dock_modal_opening = false);
                return;
            };
            let display_id = popup_owner.display_for(bounds, cx);
            let result = cx.open_window(
                WindowOptions {
                    kind: crate::app::window::popup_frame::child_window_kind(),
                    window_decorations: crate::app::window::popup_frame::child_window_decorations(),
                    #[cfg(target_os = "linux")]
                    x11_parent: Some(owner),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    focus: true,
                    show: true,
                    is_resizable: true,
                    window_min_size: Some(gpui::size(px(360.0), px(PANEL_MIN_HEIGHT))),
                    is_minimizable: false,
                    titlebar: None,
                    // The page window's own: blurred under window glass, opaque without.
                    window_background: if crate::app::helpers::window_glass_active() {
                        gpui::WindowBackgroundAppearance::Blurred
                    } else {
                        gpui::WindowBackgroundAppearance::Opaque
                    },
                    ..Default::default()
                },
                {
                    let chat = chat.clone();
                    move |window, cx| {
                        // Windows draws this border around the window: 16% ink over the
                        // page's colour, as the app modals' `window_border`.
                        let border = gpui::rgb(
                            if crate::app::helpers::chrome_uses_light_appearance() {
                                0xcacaca
                            } else {
                                0x3a3a3a
                            },
                        );
                        crate::app::window::popup_frame::frame_app_modal_window(window, border);
                        crate::app::window::attach_gpui_app_modal_window_to_main_window(
                            window,
                            parent_native_view,
                        );
                        #[cfg(all(target_os = "macos", not(ghostex_web)))]
                        if let Ok(native) = crate::app::helpers::cef_parent_native_view(window) {
                            crate::app::ghostex_capture::platform::make_resizable(
                                native as usize,
                                gpui::size(px(360.0), px(PANEL_MIN_HEIGHT)),
                            );
                        }
                        window.activate_window();
                        let view = cx.new(|cx| {
                            let observed = cx.observe(&chat, |_, _, cx| cx.notify());
                            let focus = cx.focus_handle();
                            focus.focus(window, cx);
                            let mut panel = DockAskPanel {
                                chat,
                                focus,
                                backdrop: None,
                                measured: Rc::new(Cell::new((0.0, 0.0, 0.0))),
                                fitted: None,
                                main_height,
                                _chat: observed,
                                _glass: None,
                                _bounds: None,
                            };
                            panel.follow_backdrop(window, cx);
                            panel
                        });
                        cx.new(|cx| {
                            Root::new(view, window, cx)
                                .bordered(false)
                                .bg(gpui::transparent_black())
                        })
                    }
                },
            );
            chat.update(cx, |this, cx| {
                this.board.dock_modal_opening = false;
                match result {
                    // Closed while the window was opening.
                    Ok(handle) if this.board.dock_modal.is_none() => {
                        cx.defer(move |cx| {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        });
                    }
                    Ok(handle) => {
                        let handle: gpui::AnyWindowHandle = handle.into();
                        this.board.dock_modal_window = Some(handle);
                        let chat = cx.weak_entity();
                        this.board.dock_modal_closed = Some(cx.on_window_closed(move |cx, id| {
                            let _ = chat.update(cx, |chat, cx| {
                                if chat
                                    .board
                                    .dock_modal_window
                                    .is_some_and(|handle| handle.window_id() == id)
                                {
                                    chat.board.dock_modal_window = None;
                                    chat.close_dock_ask_modal(cx);
                                }
                            });
                        }));
                    }
                    Err(error) => {
                        this.board.dock_modal = None;
                        this.error = Some(error.to_string());
                        cx.notify();
                    }
                }
            });
        });
    }

    /// Closes the panel, back to the dock; the chat box takes the keyboard back.
    pub(super) fn close_dock_ask_modal(&mut self, cx: &mut Context<Self>) {
        let shown = self.board.dock_modal.take().is_some();
        self.board.dock_modal_closed = None;
        let Some(handle) = self.board.dock_modal_window.take() else {
            if shown {
                cx.notify();
            }
            return;
        };
        let main_window = self.main_window.filter(|_| !self.pane_hidden);
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            if let Some(main_window) = main_window {
                let _ = main_window.update(cx, |_, window, cx| {
                    window.activate_window();
                    let _ = chat.update(cx, |chat, cx| {
                        chat.focus_requested = true;
                        cx.notify();
                    });
                });
            }
        });
        cx.notify();
    }

    /// A command from the panel's ⋯ menu closes the panel and goes on as the command it wraps; any
    /// other passes as it is.
    pub(super) fn unwrap_dock_ask_command(
        &mut self,
        action: Value,
        cx: &mut Context<Self>,
    ) -> Value {
        if action["type"] != THEN_CLOSE {
            return action;
        }
        self.close_dock_ask_modal(cx);
        action["command"].clone()
    }

    /// `element`, closing the panel once a click on it has done what it does (an answer, Open, a
    /// link to the work): the press is seen on its way in and the close runs after it.
    fn closes_panel(&self, element: AnyElement, cx: &Context<Self>) -> AnyElement {
        div()
            .flex_shrink_0()
            .capture_any_mouse_up(cx.listener(|_, _: &gpui::MouseUpEvent, window, cx| {
                cx.defer_in(window, |this, _, cx| this.close_dock_ask_modal(cx));
            }))
            .child(element)
            .into_any_element()
    }

    /// The panel's content, drawn in its window (`DockAskPanel`): the band with the controls, the
    /// title, the text and the foot. `None` closes the panel: its row left the dock, or a Reply sent
    /// from it was taken.
    fn dock_ask_panel(
        &mut self,
        p: &ChatAppearance,
        measured: Rc<Cell<(f32, f32, f32)>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.board.dock_modal.as_ref()?;
        let row = self.dock_ask_row();
        let replied = row.as_ref().map(|row| text(row, "replied"));
        if row.is_none() || replied.is_some_and(|now| now != self.board.dock_modal_replied) {
            // The snapshot's own check (`sync_dock_ask_modal`) normally closes it first; this one
            // covers a row that went without a new snapshot, with the same toast.
            let chat = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = chat.update(cx, |chat, cx| chat.sync_dock_ask_modal(cx));
            });
            return None;
        }
        let row = row?;
        let s = p.scale;
        let key = text(&row, "key");
        let (icon, color) = row_mark(&row, p);
        let body = p.strengthened(p.prose);
        let busy = row["busy"] == true;
        let title = text(&row, "title");
        let age = text(&row, "age");
        let controls = ControlGlass::for_theme(p.light);

        // The band: the page window's, its middle moving the window, the round controls in its
        // corners (the X leading on macOS, window/round_close_button.rs), the age before them.
        let menu_row = row.clone();
        let more = control_button(
            "dock-ask-more",
            "titlebar/dots.svg",
            "More actions",
            controls,
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                let rows = wrap_menu_rows(this.row_menu(&menu_row, cx));
                this.show_dock_row_menu(rows, event.position(), window, cx);
            }),
        );
        let close = control_button("dock-ask-close", "titlebar/x.svg", "Close", controls).on_click(
            cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_dock_ask_modal(cx);
            }),
        );
        let close_on_left = crate::app::window::round_close_button::close_buttons_on_left();
        let mut close = Some(close);
        let leading = close_on_left.then(|| close.take()).flatten();
        let band = div()
            .flex()
            .flex_shrink_0()
            .w_full()
            .h(px(TOP_BAND_HEIGHT))
            .items_start()
            .when_some(leading, |band, close| {
                band.child(
                    div()
                        .flex_shrink_0()
                        .pt(px(CONTROL_INSET))
                        .pl(px(CONTROL_INSET))
                        .child(close),
                )
            })
            .child(
                crate::app::render::window_drag_region::window_drag_region(
                    div().id("dock-ask-drag-band"),
                )
                .flex_1()
                .min_w_0()
                .h_full(),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pt(px(CONTROL_INSET))
                    .pr(px(CONTROL_INSET))
                    .child(
                        div()
                            .pr(px(4.0))
                            .text_size(px(12.0 * s))
                            .text_color(p.muted)
                            .child(age.to_string()),
                    )
                    .child(more)
                    .children(close),
            );
        // The title under the band, whole.
        let title_size = PROSE_SIZE + 2.0;
        let title_line = PROSE_LINE_HEIGHT + 3.0;
        let head = div()
            .flex()
            .flex_shrink_0()
            .items_start()
            .gap(px(10.0 * s))
            .px(px(PANEL_PAD_X * s))
            .pb(px(10.0 * s))
            .child(
                div()
                    .h(px(title_line * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .child(svg().path(icon).size(px(14.0 * s)).text_color(color)),
            )
            .child(
                div()
                    .id("dock-ask-title")
                    .role(gpui::Role::Heading)
                    .flex_1()
                    .min_w_0()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.ask_title(
                        &key,
                        &title,
                        &row["titleRuns"],
                        false,
                        body,
                        title_size,
                        title_line,
                        p,
                        cx,
                    )),
            );

        // The text: the work it belongs to, the whole description, where it stands. It scrolls
        // when the panel is shorter than it.
        let (thread_chips, other_chips): (Vec<Value>, Vec<Value>) = if !row["waitedBy"].is_array() {
            row["chips"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .partition(|chip| chip["kind"] == "thread")
        } else {
            (Vec::new(), Vec::new())
        };
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s));
        let mut has_text = false;
        if row["waitedBy"].is_array() {
            // An ask's own item, then the other items that wait on its answer: one line. With item
            // pills in the foot, only its other parts (the steps) are left here.
            let parts: Vec<AnyElement> = self
                .item_line(&key, &row, p, cx)
                .into_iter()
                .map(|part| self.closes_panel(part, cx))
                .collect();
            if !parts.is_empty() {
                has_text = true;
                column = column.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(6.0 * s))
                        .line_height(px(20.0 * s))
                        .children(parts),
                );
            }
        } else {
            let chips: Vec<AnyElement> = other_chips
                .iter()
                .enumerate()
                .map(|(index, chip)| {
                    let part = self.work_card_part(chip, &format!("dock-ask:{key}:{index}"), p, cx);
                    self.closes_panel(part, cx)
                })
                .collect();
            if !chips.is_empty() {
                has_text = true;
                column = column.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x(px(14.0 * s))
                        .gap_y(px(2.0 * s))
                        .children(chips),
                );
            }
        }
        let body_lead = if has_text { OPENED_ROW_GAP - 2.0 } else { 0.0 };
        let detail = text(&row, "body");
        let mut first_body = true;
        if !detail.is_empty() {
            first_body = false;
            column = column.child(div().mt(px(body_lead * s)).child(self.ask_markdown(
                &format!("dock-ask:{key}"),
                &detail,
                p.card_muted,
                PROSE_SIZE,
                PROSE_LINE_HEIGHT,
                p,
                cx,
            )));
        }
        for line in row["detail"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|line| !line.is_empty())
        {
            column = column.child(
                div()
                    .min_w_0()
                    .when(first_body, |line| line.mt(px(body_lead * s)))
                    .whitespace_normal()
                    .text_color(p.card_muted)
                    .child(line.to_string()),
            );
            first_body = false;
        }
        let has_body = has_text || !first_body;

        // The foot: item pills and threads at the left, the answers (never folded: the panel has
        // room to wrap them) and Reply after a divider at the right; Open / Reply for a row that
        // asks with no answers. Reply stays out of `closes_panel`: its box opens over the panel,
        // which closes once the reply is taken.
        let answers: Vec<Value> = row["buttons"].as_array().cloned().unwrap_or_default();
        let action = answers.is_empty() && row["needs"] == true;
        let reply_answer = ((!answers.is_empty() || (action && row["open"].is_object()))
            && row["reply"].is_object())
        .then(|| Self::reply_answer(&row["reply"], &text(&row, "replied"), busy));
        let mut buttons: Vec<AnyElement> = if action {
            let opens = row["open"].is_object();
            self.work_row_action(&key, &row, p, cx)
                .into_iter()
                .map(|button| {
                    if opens {
                        self.closes_panel(button, cx)
                    } else {
                        button
                    }
                })
                .collect()
        } else {
            self.work_answer_buttons(&format!("dock-ask:{key}"), &answers, busy, p, cx)
                .into_iter()
                .map(|button| self.closes_panel(button, cx))
                .collect()
        };
        if let Some(answer) = &reply_answer {
            if !buttons.is_empty() {
                buttons.push(answers_divider(p));
            }
            buttons.extend(self.work_answer_buttons(
                &format!("dock-ask:{key}:reply"),
                std::slice::from_ref(answer),
                busy,
                p,
                cx,
            ));
        }
        let pills: Vec<Value> = row["itemPills"].as_array().cloned().unwrap_or_default();
        let mut lead: Vec<AnyElement> = self
            .item_pill_elements(
                &format!("dock-ask:{key}"),
                &pills,
                pill_fit(pill_widths(&pills, p, cx), PANEL_WIDTH / 2.0),
                p,
                cx,
            )
            .into_iter()
            .map(|pill| self.closes_panel(pill, cx))
            .collect();
        lead.extend(thread_chips.iter().enumerate().map(|(index, chip)| {
            let logo = self.work_thread_logo(chip, &format!("dock-ask:{key}:t{index}"), p, cx);
            self.closes_panel(logo, cx)
        }));
        let has_foot = !buttons.is_empty() || !lead.is_empty();
        let foot = has_foot.then(|| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(10.0 * s))
                .px(px(PANEL_PAD_X * s))
                .py(px(12.0 * s))
                .border_t_1()
                .border_color(p.foreground.opacity(0.08))
                .text_size(px(PROSE_SIZE * s))
                .line_height(px(PROSE_LINE_HEIGHT * s))
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .children(lead),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .items_center()
                        .gap(px(6.0 * s))
                        .children(buttons),
                )
        });

        // Each part's natural height, for the panel to fit itself to (`DockAskPanel::fit`).
        let measure = |part: usize, measured: Rc<Cell<(f32, f32, f32)>>| {
            gpui::canvas(
                move |bounds, _, _| {
                    let mut now = measured.get();
                    let height = f32::from(bounds.size.height);
                    match part {
                        0 => now.0 = height,
                        1 => now.1 = height,
                        _ => now.2 = height,
                    }
                    measured.set(now);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        };
        let text_pad = 16.0 * s;
        if !has_body {
            measured.set((measured.get().0, 0.0, measured.get().2));
        }
        if !has_foot {
            measured.set((measured.get().0, measured.get().1, 0.0));
        }
        Some(
            div()
                .id("dock-ask-content")
                .relative()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .text_color(p.primary)
                .role(gpui::Role::Dialog)
                .aria_label(title.to_string())
                .child(
                    div()
                        .relative()
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .child(band)
                        .child(head)
                        .child(measure(0, measured.clone())),
                )
                .when(has_body, |panel| {
                    panel.child(
                        div()
                            .id("dock-ask-text")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .relative()
                                    .pl(px((PANEL_PAD_X + TEXT_LEAD) * s))
                                    .pr(px(PANEL_PAD_X * s))
                                    .pb(px(text_pad))
                                    .child(column)
                                    .child(measure(1, measured.clone())),
                            ),
                    )
                })
                // With no text, the foot keeps to the bottom of whatever height the reader gives.
                .when(!has_body, |panel| panel.child(div().flex_1()))
                .when_some(foot, |panel, foot| {
                    panel.child(div().relative().child(foot).child(measure(2, measured)))
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_pick_closes_the_panel_snooze_lengths_included() {
        let rows = wrap_menu_rows(vec![
            json!({ "label": "Dismiss", "command": { "type": "dismiss" } }),
            json!({ "label": "Snooze", "children": [
                { "label": "1 hour", "command": { "type": "snooze", "for": 3600 } },
            ] }),
        ]);
        assert_eq!(rows[0]["command"]["type"], THEN_CLOSE);
        assert_eq!(rows[0]["command"]["command"]["type"], "dismiss");
        assert_eq!(rows[1]["children"][0]["command"]["type"], THEN_CLOSE);
        assert_eq!(rows[1]["children"][0]["command"]["command"]["for"], 3600);
        assert!(rows[1]["command"].is_null());
    }

    #[test]
    fn the_panel_fits_its_ask_between_its_shortest_and_most_of_the_main_window() {
        assert_eq!(fitted_height(120.0, 900.0), PANEL_MIN_HEIGHT);
        assert_eq!(fitted_height(480.0, 900.0), 480.0);
        assert_eq!(fitted_height(2000.0, 900.0), 900.0 * PANEL_MAX_SHARE);
    }
}
