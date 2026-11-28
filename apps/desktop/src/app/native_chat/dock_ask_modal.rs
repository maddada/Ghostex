//! A dock row's ask in a floating panel over the chat (work_cards.rs `dock_row`), opened the way a
//! Ghostex page opens from the chat: a window of its own over the main window, with the page
//! window's look (window/visual_page_modal.rs): its 36pt top band that moves the window with the
//! round glass controls in its corners, and edges that resize it. Its surface is the frosted glass
//! of the form dialogs under window glass (the Rename dialog's), the page's top colour without. The chat stays drawn behind it, whole.
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
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User, of the dock's question pop-up drawn as a near-opaque black panel: "make this more transparent like we did for other parts of the app pls it's not good now. the rename one is good example". Under window glass the panel takes the Rename dialog's material: its window blurs what is behind it and it paints only the app's frosted fill (`ChatAppearance::menu_surface`, the same `frosted_menu_fill` the form dialogs use) over that blur, in dark and light. It no longer paints the opaque page-top colour with the wallpaper and tint over it, which hid the blur. Glass off keeps the solid page colour.

use super::chat_text_look::{PROSE_LINE_HEIGHT, PROSE_SIZE};
use super::work_cards::{OPENED_ROW_GAP, answers_divider, pill_fit, pill_widths, row_mark};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::window::page_band_geometry::{CONTROL_INSET, TOP_BAND_HEIGHT};
use crate::app::window::page_controls::{ControlGlass, control_button};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, WindowBounds, WindowOptions, div, px, svg,
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

/// The panel's colours with window glass off (window/visual_page_modal/glass.rs).
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

/// Rough upfront height for `row` at `width` (scaled px), so the window opens already close to (or
/// at) its fitted height and vertically centred before the first prepaint measures it exactly.
fn estimated_panel_height(row: &Value, width: f32, scale: f32, main_height: f32) -> f32 {
    let wrapped_lines = |content: &str, char_width: f32, col_width: f32| -> f32 {
        let col = col_width.max(120.0);
        content
            .split('\n')
            .map(|line| {
                let count = line.chars().count();
                if count == 0 {
                    0.6
                } else {
                    (count as f32 * char_width / col).ceil().max(1.0)
                }
            })
            .sum::<f32>()
    };
    let unscaled_width = (width / scale).max(320.0);
    let title_line = PROSE_LINE_HEIGHT + 3.0;
    let title_col = unscaled_width - 2.0 * PANEL_PAD_X - 24.0;
    let title_lines = wrapped_lines(&text(row, "title"), 7.8, title_col).max(1.0);
    let head = TOP_BAND_HEIGHT + (title_lines * title_line + 10.0) * scale;

    let body_col = unscaled_width - 2.0 * PANEL_PAD_X - TEXT_LEAD;
    let mut body_unscaled = 0.0;
    let has_chips = row["waitedBy"].is_array()
        || row["chips"]
            .as_array()
            .is_some_and(|chips| chips.iter().any(|chip| chip["kind"] != "thread"));
    if has_chips {
        body_unscaled += 22.0;
    }
    let detail_body = text(row, "body");
    if !detail_body.is_empty() {
        if has_chips {
            body_unscaled += OPENED_ROW_GAP;
        }
        body_unscaled += wrapped_lines(&detail_body, 6.8, body_col) * PROSE_LINE_HEIGHT;
    }
    for line in row["detail"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|line| !line.is_empty())
    {
        body_unscaled += 2.0 + wrapped_lines(line, 6.8, body_col) * PROSE_LINE_HEIGHT;
    }
    let text_height = if body_unscaled > 0.0 {
        (body_unscaled + 16.0) * scale
    } else {
        0.0
    };

    let has_answers = row["buttons"].as_array().is_some_and(|b| !b.is_empty());
    let has_action = !has_answers && row["needs"] == true;
    let has_reply = row["reply"].is_object();
    let has_pills = row["itemPills"].as_array().is_some_and(|p| !p.is_empty());
    let has_threads = !row["waitedBy"].is_array()
        && row["chips"]
            .as_array()
            .is_some_and(|chips| chips.iter().any(|chip| chip["kind"] == "thread"));
    let foot = if has_answers || has_action || has_reply || has_pills || has_threads {
        (24.0 + 30.0 + 1.0) * scale
    } else {
        0.0
    };
    fitted_height(head + text_height + foot, main_height)
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

/// The panel window's view: the chat draws what is in it (`dock_ask_panel`).
struct DockAskPanel {
    chat: Entity<NativeChatView>,
    focus: FocusHandle,
    /// The natural heights measured at the last prepaint: band and title, the text, the foot.
    measured: Rc<Cell<(f32, f32, f32)>>,
    /// Held transparent until first layout fits the window to its measured height, then shown
    /// atomically at its final size so the reader never sees a size flash.
    hold: crate::app::window::popup_frame::PopupHoldState,
    /// The height the panel last fitted itself to; once the reader resizes it, it stops fitting.
    fitted: Rc<Cell<Option<f32>>>,
    /// Target height of an in-flight resize, preventing duplicate resize triggers while waiting for
    /// the platform resize callback (e.g. `ResizeObserver` on `gpui-web`).
    resizing_to: Rc<Cell<Option<f32>>>,
    /// Number of initial fit resizes issued while held; caps retries if the OS clamps the height.
    fit_attempts: Rc<Cell<u8>>,
    /// The Windows DWM border colour applied when the panel reveals.
    border: gpui::Rgba,
    /// The main window's height when the panel opened, which caps how tall it fits itself.
    main_height: f32,
    _chat: Subscription,
}

impl DockAskPanel {
    /// Trailing prepaint hook that runs after all three measured sections have laid out: resizes
    /// the window if needed, and reveals a held panel once its window matches its measured height.
    fn fit_tracker(&self) -> AnyElement {
        let measured = self.measured.clone();
        let hold = self.hold.clone();
        let fitted = self.fitted.clone();
        let resizing_to = self.resizing_to.clone();
        let fit_attempts = self.fit_attempts.clone();
        let border = self.border;
        let main_height = self.main_height;
        gpui::canvas(
            move |_, window, cx| {
                let (head, text, foot) = measured.get();
                if head <= 0.0 {
                    return;
                }
                let current = f32::from(window.bounds().size.height);
                let current_hold = hold.get();
                if current_hold == crate::app::window::popup_frame::PopupHold::Shown
                    && fitted.get().is_some_and(|f| (f - current).abs() > 1.0)
                    && resizing_to.get().is_none()
                {
                    // The reader resized it: their height stands.
                    return;
                }
                let want = fitted_height(head + text + foot, main_height);
                let is_held = current_hold == crate::app::window::popup_frame::PopupHold::Held;
                if (want - current).abs() > 1.0 && (!is_held || fit_attempts.get() < 2) {
                    if resizing_to.get().is_some_and(|t| (t - want).abs() <= 0.5) {
                        return;
                    }
                    resizing_to.set(Some(want));
                    fitted.set(Some(want));
                    if is_held {
                        fit_attempts.set(fit_attempts.get().saturating_add(1));
                    }
                    let width = window.bounds().size.width;
                    // Moved outside the update, as the page window's settle is: AppKit reports the
                    // new frame synchronously, and GPUI drops that report while the app is borrowed.
                    #[cfg(all(target_os = "macos", not(ghostex_web)))]
                    {
                        let Ok(native) = crate::app::helpers::cef_parent_native_view(window) else {
                            return;
                        };
                        let native = native as usize;
                        let frame =
                            crate::app::window::visual_page_modal::frame::shared_frame(window);
                        // About the centre it opened at, over the chat.
                        let to = gpui::Bounds::new(
                            gpui::point(frame.origin.x, frame.center().y - px(want / 2.0)),
                            gpui::size(width, px(want)),
                        );
                        let scale = window.scale_factor();
                        let handle = window.window_handle();
                        let resizing_done = resizing_to.clone();
                        cx.spawn(async move |cx| {
                            crate::app::ghostex_capture::platform::set_window_frame(
                                native, to, scale,
                            );
                            let _ = handle.update(cx, |_, window, _| {
                                // AppKit applied the frame synchronously; if the display clamped
                                // the height, clear the in-flight target and refresh so the next
                                // prepaint can still reveal instead of waiting forever.
                                if (f32::from(window.bounds().size.height) - want).abs() > 1.0 {
                                    resizing_done.set(None);
                                }
                                window.refresh();
                            });
                        })
                        .detach();
                    }
                    #[cfg(not(all(target_os = "macos", not(ghostex_web))))]
                    {
                        let _ = cx;
                        window.on_next_frame(move |window, _| {
                            window.resize(gpui::size(width, px(want)));
                        });
                    }
                    return;
                }
                resizing_to.set(None);
                fitted.set(Some(current));
                if is_held {
                    hold.set(crate::app::window::popup_frame::PopupHold::Revealing);
                    let hold = hold.clone();
                    let handle = window.window_handle();
                    cx.defer(move |cx| {
                        hold.set(crate::app::window::popup_frame::PopupHold::Shown);
                        let _ = handle.update(cx, |_, window, _| {
                            window.refresh();
                            window.on_next_frame(move |window, _| {
                                crate::app::window::popup_frame::reveal_held_panel_window(
                                    window, border,
                                );
                            });
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }
}

impl Render for DockAskPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let held = self.hold.get() == crate::app::window::popup_frame::PopupHold::Held;
        #[cfg(all(target_os = "macos", not(ghostex_web)))]
        if !held {
            crate::app::helpers::apply_frosted_menu_blur(window);
        }
        let measured = self.measured.clone();
        let content = self.chat.update(cx, |chat, cx| {
            let glass = crate::app::helpers::window_glass_active_for(chat.main_window);
            let p = ChatAppearance::current(&chat.snapshot).on_window_glass(glass);
            chat.dock_ask_panel(&p, measured, window, cx)
                .map(|content| {
                    // Under window glass the panel is the same frosted surface as the form dialogs
                    // (the Rename dialog): the app's frosted fill over the window's own blur.
                    let fill = glass.then(|| p.menu_surface());
                    (content, p.light, p.font.clone(), fill)
                })
        });
        let Some((content, light, font, glass_fill)) = content else {
            // Its row left the dock, or the panel is closing: nothing is left to show, so the
            // window goes with it. The backdrop is this window's own (its blur and picture), so a
            // panel with no ask removes itself here instead of waiting on the chat to close it;
            // an empty glass window must never stay over the chat.
            window.remove_window();
            return div().size_full().into_any_element();
        };
        let chat = self.chat.clone();
        let press = self.chat.read(cx).board.dock_modal_press.clone();
        let focus = self.focus.clone();
        let panel = div()
            .id("dock-ask-panel")
            // Its copy keys beyond Ctrl+C / Cmd+C (main.rs: Ctrl+Insert, and Ctrl+Shift+C on
            // Linux, which Omarchy's Super+C sends), answered by `Root`'s copy.
            .key_context("DockAskPanel")
            .track_focus(&self.focus)
            .capture_any_mouse_down(move |_, window, cx| {
                // Every press starts as one on no control (`closes_panel` marks its own after this).
                press.set(false);
                // A press in the panel gives it the keyboard, so Cmd+C (Ctrl+C off macOS) copies
                // what is selected in it rather than reaching the chat (voice_agent/panel_copy.rs
                // `take_keyboard` does the same for its non-activating panel).
                if !window.is_window_active() {
                    window.activate_window();
                }
                focus.focus(window, cx);
            });
        // Edit › Copy (Cmd+C on macOS goes through the app menu) copies the panel's selection, as
        // Cmd+C does through `Root`.
        #[cfg(not(ghostex_web))]
        let panel = panel.on_action(|_: &crate::app::actions::GpuiEditMenuCopy, window, cx| {
            window.dispatch_action(Box::new(gpui_component::input::Copy), cx);
        });
        panel
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(font)
            .bg(if held {
                gpui::transparent_black()
            } else {
                glass_fill.unwrap_or_else(|| {
                    gpui::rgb(if light { PAGE_TOP_LIGHT } else { PAGE_TOP_DARK }).into()
                })
            })
            .when(held, |panel| panel.opacity(0.0))
            .on_key_down(move |event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && !event.is_held {
                    chat.update(cx, |chat, cx| chat.close_dock_ask_modal(cx));
                    cx.stop_propagation();
                }
            })
            .child(content)
            .child(self.fit_tracker())
            .into_any_element()
    }
}

impl NativeChatView {
    /// The row the panel shows, as the dock has it now, or `None` when it has left the dock.
    fn dock_ask_row(&self) -> Option<Value> {
        let key = self.board.dock_modal.as_deref()?;
        // Snoozed and Recent drawer rows open it too (with legacy "done" fallback).
        let dock = &self.snapshot["workDock"];
        ["rows", "snoozed", "recent", "done"]
            .into_iter()
            .flat_map(|list| dock[list].as_array().into_iter().flatten())
            .find(|row| row["key"].as_str() == Some(key))
            .cloned()
    }

    /// First open ask row in `workDock.rows` that carries details to show when a notification
    /// opens this session (`request_open_first_ask_details`).
    fn first_ask_details_key(&self) -> Option<String> {
        self.snapshot["workDock"]["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| {
                matches!(row["kind"].as_str(), Some("ask" | "question"))
                    && (row["details"] == true || !text(row, "body").trim().is_empty())
                    && row["settled"] != true
            })
            .and_then(|row| row["key"].as_str().map(str::to_string))
    }

    /// Opens the first open ask's details panel right away when its dock row is already loaded,
    /// or arms a short-lived pending request so it opens as soon as the session's snapshot arrives
    /// (called when clicking a desktop notification for a session that needs input).
    pub(crate) fn request_open_first_ask_details(&mut self, cx: &mut Context<Self>) {
        if self.board.dock_modal.is_some() {
            return;
        }
        if let Some(key) = self.first_ask_details_key() {
            self.board.pending_open_ask_details = None;
            self.open_dock_ask_modal_cx(&key, cx);
        } else {
            self.board.pending_open_ask_details = Some(web_time::Instant::now());
        }
    }

    /// Closes the panel once its ask has left the dock or a Reply sent from it was taken, as soon
    /// as the chat's snapshot says so (`NativeChatView::apply_output`), whether or not the panel's
    /// own window draws again. An ask the coordinator, the phone or another computer settled while
    /// it was open says how it went in a toast: "Answered" (an answer, a Reply or Mark done) or
    /// "Withdrawn". A row that left for another reason (snoozed elsewhere, a refresh) closes it
    /// quietly.
    pub(super) fn sync_dock_ask_modal(&mut self, cx: &mut Context<Self>) {
        if let Some(at) = self.board.pending_open_ask_details {
            if at.elapsed().as_secs() > 8 || self.board.dock_modal.is_some() {
                self.board.pending_open_ask_details = None;
            } else if let Some(key) = self.first_ask_details_key() {
                self.board.pending_open_ask_details = None;
                self.open_dock_ask_modal_cx(&key, cx);
            }
        }
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
        self.open_dock_ask_modal_cx(key, cx);
    }

    pub(super) fn open_dock_ask_modal_cx(&mut self, key: &str, cx: &mut Context<Self>) {
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
        let initial_row = self.dock_ask_row();
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
                let main_height = f32::from(content.size.height);
                let width = px(PANEL_WIDTH * scale).min(area.size.width - px(32.0));
                let open_height = initial_row
                    .as_ref()
                    .map(|row| estimated_panel_height(row, f32::from(width), scale, main_height))
                    .unwrap_or(PANEL_OPEN_HEIGHT * scale);
                let height = px(open_height).min(area.size.height - px(32.0));
                (
                    gpui::Bounds::centered_at(area.center(), gpui::size(width, height)),
                    crate::app::window::popup_frame::PopupOwner::of(window, cx),
                    main_height,
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
                    // Opened transparent (no blur/opaque fill, no macOS shadow/rim) while the
                    // first prepaint measures and settles its height; `reveal_held_panel_window`
                    // turns on glass/opaque and shadow atomically on the first visible frame.
                    window_background: gpui::WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                {
                    let chat = chat.clone();
                    move |window, cx| {
                        // Windows draws this border around the window once revealed: 16% ink over
                        // the page's colour, as the app modals' `window_border`.
                        let border =
                            gpui::rgb(if crate::app::helpers::chrome_uses_light_appearance() {
                                0xcacaca
                            } else {
                                0x3a3a3a
                            });
                        crate::app::window::popup_frame::hide_held_panel_window(window);
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
                        let hold: crate::app::window::popup_frame::PopupHoldState =
                            Default::default();
                        let fallback_hold = hold.clone();
                        let handle = window.window_handle();
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(180))
                                .await;
                            if fallback_hold.get()
                                == crate::app::window::popup_frame::PopupHold::Held
                            {
                                fallback_hold
                                    .set(crate::app::window::popup_frame::PopupHold::Shown);
                                let _ = handle.update(cx, |_, window, _| {
                                    window.refresh();
                                    window.on_next_frame(move |window, _| {
                                        crate::app::window::popup_frame::reveal_held_panel_window(
                                            window, border,
                                        );
                                    });
                                });
                            }
                        })
                        .detach();
                        let view = cx.new(|cx| {
                            let observed = cx.observe(&chat, |_, _, cx| cx.notify());
                            let focus = cx.focus_handle();
                            focus.focus(window, cx);
                            DockAskPanel {
                                chat,
                                focus,
                                measured: Rc::new(Cell::new((0.0, 0.0, 0.0))),
                                hold,
                                fitted: Rc::new(Cell::new(None)),
                                resizing_to: Rc::new(Cell::new(None)),
                                fit_attempts: Rc::new(Cell::new(0)),
                                border,
                                main_height,
                                _chat: observed,
                            }
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
                        this.error = Some(
                            crate::app::native_chat::child_window::window_open_failed(error),
                        );
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
                    // The panel also closes on its own when its ask leaves the dock (answered or
                    // withdrawn elsewhere), which must not pull Ghostex in front of another app.
                    crate::app::window_activation::activate_window_if_frontmost(
                        window,
                        cx,
                        "dock ask panel closed",
                    );
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
        let pressed = self.board.dock_modal_press.clone();
        let released = pressed.clone();
        div()
            .flex_shrink_0()
            // Only a press that began here closes it: a drag across the text to select it that
            // ends on an answer or a link leaves the panel as it is (the panel's own press, in
            // `DockAskPanel::render`, clears this first).
            .capture_any_mouse_down(move |event: &gpui::MouseDownEvent, _, _| {
                if event.button == gpui::MouseButton::Left {
                    pressed.set(true);
                }
            })
            .capture_any_mouse_up(cx.listener(move |_, _: &gpui::MouseUpEvent, window, cx| {
                if !released.replace(false) {
                    return;
                }
                cx.defer_in(window, |this, _, cx| this.close_dock_ask_modal(cx));
            }))
            .child(element)
            .into_any_element()
    }

    /// A right press on the panel's title or text: Copy for what is selected in it, as the
    /// transcript's menu has it (transcript_menu.rs). Its Add to Chat and Reply are left out:
    /// the panel is about its ask, and the chat box is behind it.
    fn panel_text_menu(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = gpui_component::TextSelection::selected_text(window, cx)
            .trim()
            .to_owned();
        if selection.is_empty() {
            return;
        }
        let rows: Vec<Value> = self
            .transcript_menu_rows(None, &selection)
            .into_iter()
            .filter(|row| row["command"]["type"] == "copyText")
            .collect();
        if rows.is_empty() {
            return;
        }
        cx.stop_propagation();
        self.show_chat_menu_in_window_at(rows, event.position, 180.0, window, cx);
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
                crate::app::render::window_drag_region::child_window_drag_region(
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
                    .child(self.text_view_look(
                        format!("dock-ask-title:{key}"),
                        title_markdown(&title, &row["titleRuns"]),
                        &Value::Null,
                        p,
                        super::rich_markdown::TextLook {
                            color: body,
                            semibold: false,
                            size: title_size,
                            line_height: title_line,
                            arrow: true,
                            quiet_links: true,
                        },
                        cx,
                    )),
            )
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(Self::panel_text_menu));

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
        for (index, line) in row["detail"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|line| !line.is_empty())
            .enumerate()
        {
            column = column.child(
                div()
                    .min_w_0()
                    .when(first_body, |line| line.mt(px(body_lead * s)))
                    .child(self.text_view_look(
                        format!("dock-ask-detail:{key}:{index}"),
                        title_markdown(line, &Value::Null),
                        &Value::Null,
                        p,
                        super::rich_markdown::TextLook {
                            color: p.card_muted,
                            semibold: false,
                            size: PROSE_SIZE,
                            line_height: PROSE_LINE_HEIGHT,
                            arrow: true,
                            quiet_links: false,
                        },
                        cx,
                    )),
            );
            first_body = false;
        }
        let command_strip = self.needs_you_command_strip(&key, &row, false, p, cx);
        let has_command_strip = command_strip.is_some();
        if let Some(strip) = command_strip {
            column = column.child(
                div()
                    .w_full()
                    .min_w_0()
                    .mt(px(if first_body { body_lead } else { 8.0 } * s))
                    .child(strip),
            );
            first_body = false;
        }
        let has_body = has_text || !first_body || has_command_strip;

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
            // A pick-several ask's Choices only opens its menu, so the panel stays; its Send
            // closes the panel (work_actions.rs `send_ask_picks`).
            let multiple = super::work_actions::answers_multiple(&answers);
            self.work_answer_buttons(&format!("dock-ask:{key}"), &answers, busy, p, cx)
                .into_iter()
                .map(|button| {
                    if multiple {
                        button
                    } else {
                        self.closes_panel(button, cx)
                    }
                })
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
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(Self::panel_text_menu),
                            )
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

/// The ask's title (or a line of where it stands) as Markdown for the chat's selectable text view: its words as written (every
/// Markdown character escaped, so `*`, `_` or a leading `1.` show as they are, the way the dock
/// row's words do), its links (the core's `titleRuns`) as links, and a `#562` left for the view to
/// link to the project's GitHub (`issue_linked`).
///
/// CDXC:Coordinators 2026-11-19 DECISION:
/// User, of a to-do whose title held an IP address in this panel: they could not select its text
/// to copy it. The panel's title and text are selectable the way a chat message's are, with
/// Cmd+C, Edit › Copy and a right-click Copy; its title is drawn by the chat's Markdown view
/// rather than word by word (link_words.rs), so its links are underlined in the title's own colour
/// there instead of dotted. A press that ends on an answer or a link closes the panel only when it
/// began there, so a drag to select never does.
pub(super) fn title_markdown(title: &str, runs: &Value) -> String {
    /// `words` with every Markdown character escaped, except its code spans (`` `npm test` ``),
    /// kept whole so the chat's view draws them as code, a command among them with its Copy and
    /// Paste in terminal (`ask_markdown` below).
    fn escape(words: &str, out: &mut String) {
        let mut rest = words;
        while let Some(start) = rest.find('`') {
            escape_plain(&rest[..start], out);
            let ticks = rest[start..].len() - rest[start..].trim_start_matches('`').len();
            let after = start + ticks;
            match closing_ticks(&rest[after..], ticks) {
                Some(close) => {
                    let end = after + close + ticks;
                    out.push_str(&rest[start..end].replace('\n', " "));
                    rest = &rest[end..];
                }
                None => {
                    for _ in 0..ticks {
                        out.push_str("\\`");
                    }
                    rest = &rest[after..];
                }
            }
        }
        escape_plain(rest, out);
    }
    /// Where a run of exactly `ticks` backticks starts in `text`.
    fn closing_ticks(text: &str, ticks: usize) -> Option<usize> {
        let mut at = 0;
        while let Some(found) = text[at..].find('`') {
            let start = at + found;
            let run = text[start..].len() - text[start..].trim_start_matches('`').len();
            if run == ticks {
                return Some(start);
            }
            at = start + run;
        }
        None
    }
    fn escape_plain(words: &str, out: &mut String) {
        for c in words.chars() {
            // `:` too, so words the core left plain never become an address the view links
            // itself with the escapes kept in it.
            if c.is_ascii_punctuation() && c != '#' && c != '/' {
                out.push('\\');
            }
            if c == '\n' {
                out.push(' ');
            } else {
                out.push(c);
            }
        }
    }
    let mut out = String::new();
    // A row with no runs of its own (a to-do, a blocked or finished row, a form or page) gets
    // them from the core here, so a bare `http://…` in it is a link as an ask title's is.
    let found;
    let runs = if runs.is_array() {
        runs
    } else {
        found = ghostex_gx_chat_core::questions::form_links::word_runs(title).unwrap_or_default();
        &found
    };
    match runs.as_array() {
        None => escape(title, &mut out),
        Some(runs) => {
            for run in runs {
                let words = run["text"].as_str().unwrap_or_default();
                match run["href"].as_str() {
                    Some(href) => {
                        out.push('[');
                        escape(words, &mut out);
                        out.push_str("](<");
                        out.push_str(&href.replace('<', "%3C").replace('>', "%3E"));
                        out.push_str(">)");
                    }
                    None => escape(words, &mut out),
                }
            }
        }
    }
    // A `#` that starts the title is a heading unless a number follows it (an issue).
    if out.starts_with('#') && !out[1..].starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '\\');
    }
    // The core's marks for an ask's text, as its description gets them: a command in a code span
    // becomes the chat's command chip, a file in one its pill.
    ghostex_gx_chat_core::transcript::native_markdown::ask_markdown(&out)
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

    #[test]
    fn the_title_is_its_words_as_written_with_its_links_as_links() {
        assert_eq!(
            title_markdown("1. enter 192.168.1.177 *now*", &Value::Null),
            r"1\. enter 192\.168\.1\.177 \*now\*"
        );
        assert_eq!(
            title_markdown("# not a heading", &Value::Null),
            r"\# not a heading"
        );
        assert_eq!(
            title_markdown("#562 stays an issue", &Value::Null),
            "#562 stays an issue"
        );
        // A bare address in a row with no runs is a link, its address whole.
        assert_eq!(
            title_markdown("Open http://192.168.1.177:8080/admin now", &Value::Null),
            r"Open [http\://192\.168\.1\.177\:8080/admin](<http://192.168.1.177:8080/admin>) now"
        );
        assert_eq!(title_markdown("note: done", &Value::Null), r"note\: done");
        // A code span stays code, unescaped; a command in one is the chat's command chip.
        assert_eq!(
            title_markdown("Set `a.b` to 1.", &Value::Null),
            r"Set `a.b` to 1\."
        );
        let command = title_markdown("Run `npm install` then", &Value::Null);
        assert!(command.contains("](ghostex-run:"), "{command}");
        assert_eq!(title_markdown("a ` tick", &Value::Null), r"a \` tick");
        let runs = json!([
            { "text": "see " },
            { "text": "the [docs]", "href": "https://example.com/a>b" },
            { "text": " and " },
            { "text": "#7", "issue": "7" },
        ]);
        assert_eq!(
            title_markdown("ignored", &runs),
            r"see [the \[docs\]](<https://example.com/a%3Eb>) and #7"
        );
    }
}
