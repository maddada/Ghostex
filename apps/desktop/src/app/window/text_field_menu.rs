//! The right-click menu of every text field: Ghostex's own drawn menu, in the sessions sidebar
//! menu's look, instead of the system's.
//!
//! GPUI Kit hands every right click in an `Input` or `Textarea` without its own menu builder to the
//! presenter installed here (`gpui_component::input::set_context_menu_presenter`), so one change
//! covers every field in the app and in the GPUI web build. A field with something of its own to
//! offer (the Reply popover's Save as Boost) opens the same menu through [`show_text_field_menu`]
//! with its extra rows; the chat box keeps the chat's own drawn menu (`native_chat/boosts.rs`).
//!
//! CDXC:ContextMenus 2026-11-27 DECISION:
//! User, asked "Use Ghostex's own right-click menu in every text box, like the chat box now?",
//! picked "Yes, every text box". Every text field opens this menu on a right click, on macOS,
//! Windows, Linux and the web build alike: Cut, Copy, Paste, a separator, Select All, with the chat
//! box's rules (Cut and Copy only with a selection, Paste always). A read-only field offers only
//! Copy and Select All; a masked one (a password or a secret) never copies. A code editor keeps
//! its own Go to Definition and Show Code Actions above them, as GPUI Kit's menu had. Save as Boost
//! is offered only in boxes that send a prompt to an agent (the chat box and the Reply popover).
//! The browser's address bar keeps its own Ghostex menu.
//!
//! CDXC:ContextMenus 2026-11-27 WHY:
//! The field keeps keyboard focus while the menu is up, as with the system menu: many fields
//! commit or close when they lose it (a sidebar rename ends on blur). So the menu never takes
//! focus. On macOS, Windows and Linux it draws in a frosted host window of its own over the field's
//! window (`FrostedHostKind::TextFieldMenu`), never clipped by a small window such as the Reply
//! popover; the web build has one window and draws it in place. Each window's GPUI Kit `Root`
//! carries [`TextFieldMenuLayer`], which closes the menu on a press anywhere else in that window,
//! and Escape, the arrow keys and Return reach the menu through a keystroke interceptor.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/menus.rs (the menu look),
//! .dependencies/gpui-component/crates/component/src/input/input.rs (`set_context_menu_presenter`).

use std::{cell::RefCell, rc::Rc};

use gpui::{
    Action, AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, FocusHandle,
    InteractiveElement as _, IntoElement, KeystrokeEvent, ParentElement as _, Pixels, Point,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity,
    Window, anchored, deferred, div, prelude::FluentBuilder as _, px, size, svg,
};
use gpui_component::input::{InputContextMenuCapabilities, InputContextMenuRequest};

use crate::app::helpers::{
    popup_menu_surface, titlebar_popup_menu_background, titlebar_popup_menu_border_color,
    titlebar_popup_menu_foreground, titlebar_popup_menu_hover_color,
};
use crate::app::native_sidebar::{SIDEBAR_MENU_ROW_HEIGHT, SIDEBAR_MENU_SEPARATOR_HEIGHT};

/// The menu's width, its inset around the rows and its corner radius (the sidebar menu's).
const MENU_WIDTH: f32 = 220.0;
const MENU_PADDING: f32 = 6.0;
const MENU_RADIUS: f32 = 8.0;
/// Kept from the window's edges.
const MENU_MARGIN: f32 = 8.0;
/// The layer's paint order: above GPUI Kit's popups (`POPUP_PRIORITY`, 100) and dialogs.
const MENU_PRIORITY: usize = 200;

/// What a row does when it is picked.
pub(crate) enum TextFieldMenuAction {
    /// One of the field's own actions (Cut, Copy, Paste, Select All), dispatched from the field as
    /// a key binding would.
    Field(Box<dyn Action>),
    /// Anything else (Save as Boost), run in the field's window.
    Run(Rc<dyn Fn(&mut Window, &mut App)>),
}

/// One row of the menu.
pub(crate) struct TextFieldMenuRow {
    pub(crate) label: SharedString,
    pub(crate) icon: &'static str,
    pub(crate) disabled: bool,
    pub(crate) action: TextFieldMenuAction,
}

enum Line {
    Row(TextFieldMenuRow),
    Separator,
}

impl Line {
    fn height(&self) -> f32 {
        match self {
            Line::Row(_) => SIDEBAR_MENU_ROW_HEIGHT,
            Line::Separator => SIDEBAR_MENU_SEPARATOR_HEIGHT,
        }
    }

    fn enabled_row(&self) -> bool {
        matches!(self, Line::Row(row) if !row.disabled)
    }
}

struct OpenMenu {
    /// The window the field is in, which the menu sits over.
    parent: AnyWindowHandle,
    /// The menu's frame in `parent`'s coordinates.
    frame: Bounds<Pixels>,
    lines: Rc<Vec<Line>>,
    /// The field's focus handle: its actions are dispatched from it.
    focus: FocusHandle,
    /// The row the arrow keys are on.
    selected: Option<usize>,
    /// The layer in `parent` that closes the menu on a press elsewhere.
    layer: Option<WeakEntity<TextFieldMenuLayer>>,
    _keys: Subscription,
    /// Closes the menu when the field loses the keyboard (its window closed, or a press elsewhere
    /// that no layer saw, such as in another menu's window).
    _blur: (gpui::Entity<BlurWatch>, Subscription),
}

struct BlurWatch;

thread_local! {
    static OPEN: RefCell<Option<OpenMenu>> = const { RefCell::new(None) };
}

/// Draws every text field's right-click menu as Ghostex's own (see the module comment). Call once
/// at start, after `gpui_component::init` and before the first window opens.
pub(crate) fn install(cx: &mut App) {
    gpui_component::Root::register_plugin::<TextFieldMenuLayer>(cx, TextFieldMenuLayer::new);
    gpui_component::input::set_context_menu_presenter(cx, |request, window, cx| {
        show_text_field_menu(&request, Vec::new(), window, cx);
    });
}

/// The menu for a field that builds its own (`Input::context_menu`), read from the field's state
/// now, at the pointer.
pub(crate) fn request_for(
    focus_handle: FocusHandle,
    capabilities: InputContextMenuCapabilities,
    selected_text: Option<String>,
    window: &Window,
) -> InputContextMenuRequest {
    InputContextMenuRequest {
        position: window.mouse_position(),
        focus_handle,
        selected_text: selected_text
            .filter(|text| !text.is_empty() && !capabilities.is_masked())
            .map(SharedString::from),
        capabilities,
    }
}

/// Opens the menu for `request`: the field's edit rows, then `extra` after a separator.
pub(crate) fn show_text_field_menu(
    request: &InputContextMenuRequest,
    extra: Vec<TextFieldMenuRow>,
    window: &mut Window,
    cx: &mut App,
) {
    let lines = menu_lines(&request.capabilities, extra);
    if lines.is_empty() {
        return;
    }
    close_text_field_menu(cx);
    let height = lines.iter().map(Line::height).sum::<f32>() + 2.0 * MENU_PADDING + 2.0;
    let frame = menu_frame(request.position, height, window.viewport_size());
    let parent = window.window_handle();
    let layer = window
        .root::<gpui_component::Root>()
        .flatten()
        .and_then(|root| root.read(cx).plugin::<TextFieldMenuLayer>())
        .map(|layer| layer.downgrade());
    let keys = cx.intercept_keystrokes(menu_key);
    let watch = cx.new(|_| BlurWatch);
    let blur = watch.update(cx, |_, cx| {
        cx.on_blur(&request.focus_handle, window, |_, _, cx| {
            close_text_field_menu(cx)
        })
    });
    OPEN.with(|open| {
        *open.borrow_mut() = Some(OpenMenu {
            parent,
            frame,
            lines: Rc::new(lines),
            focus: request.focus_handle.clone(),
            selected: None,
            layer,
            _keys: keys,
            _blur: (watch, blur),
        });
    });
    refresh(cx);
}

/// Closes the menu wherever it is open.
pub(crate) fn close_text_field_menu(cx: &mut App) {
    let Some(menu) = OPEN.with(|open| open.borrow_mut().take()) else {
        return;
    };
    #[cfg(not(ghostex_web))]
    crate::app::window::frosted_host::hide_frosted_host(
        crate::app::window::frosted_host::FrostedHostKind::TextFieldMenu,
        cx,
    );
    notify_layer(menu.parent, menu.layer.clone(), cx);
    // The keystroke interceptor may be the caller; it is dropped once it has returned.
    cx.defer(move |_| drop(menu));
}

/// Redraws the layer in the field's window, after whatever is being updated now (the layer itself,
/// when its window lost the keyboard). A window without a GPUI Kit `Root` draws it from its own
/// view ([`rootless_layer`]), so the whole window is drawn again.
fn notify_layer(
    parent: AnyWindowHandle,
    layer: Option<WeakEntity<TextFieldMenuLayer>>,
    cx: &mut App,
) {
    cx.defer(move |cx| match layer {
        Some(layer) => {
            let _ = layer.update(cx, |_, cx| cx.notify());
        }
        None => {
            let _ = parent.update(cx, |_, window, _| window.refresh());
        }
    });
}

/// The rows for what the field allows: a code editor's own items first, then Cut, Copy, Paste,
/// Select All (only Copy and Select All in a read-only field), then `extra`.
fn menu_lines(
    capabilities: &InputContextMenuCapabilities,
    extra: Vec<TextFieldMenuRow>,
) -> Vec<Line> {
    let field = |label: &str, icon: &'static str, disabled: bool, action: Box<dyn Action>| {
        Line::Row(TextFieldMenuRow {
            label: SharedString::from(label.to_string()),
            icon,
            disabled,
            action: TextFieldMenuAction::Field(action),
        })
    };
    let editable = capabilities.is_editable();
    let copyable = capabilities.is_copyable();
    let mut lines = Vec::with_capacity(8 + extra.len());
    if capabilities.is_code_editor() {
        lines.push(field(
            "Go to Definition",
            "titlebar/arrow-right.svg",
            !capabilities.has_definition(),
            Box::new(gpui_component::input::GoToDefinition),
        ));
        lines.push(field(
            "Show Code Actions",
            "titlebar/bulb.svg",
            !(editable && capabilities.has_code_actions()),
            Box::new(gpui_component::input::ToggleCodeActions),
        ));
        lines.push(Line::Separator);
    }
    if editable {
        lines.push(field(
            "Cut",
            "titlebar/scissors.svg",
            !copyable,
            Box::new(gpui_component::input::Cut),
        ));
    }
    lines.push(field(
        "Copy",
        "titlebar/copy.svg",
        !copyable,
        Box::new(gpui_component::input::Copy),
    ));
    // Paste is offered without peeking at the clipboard, as the chat box's menu does.
    if editable {
        lines.push(field(
            "Paste",
            "titlebar/clipboard.svg",
            false,
            Box::new(gpui_component::input::Paste),
        ));
    }
    lines.push(Line::Separator);
    lines.push(field(
        "Select All",
        "titlebar/select-all.svg",
        false,
        Box::new(gpui_component::input::SelectAll),
    ));
    if !extra.is_empty() {
        lines.push(Line::Separator);
        lines.extend(extra.into_iter().map(Line::Row));
    }
    lines
}

/// The menu's frame: opening down and right from the click, kept inside the window, and opening
/// upward where it would pass the window's bottom.
fn menu_frame(at: Point<Pixels>, height: f32, viewport: gpui::Size<Pixels>) -> Bounds<Pixels> {
    let size = size(px(MENU_WIDTH), px(height));
    let margin = px(MENU_MARGIN);
    let x = at.x.min(viewport.width - size.width - margin).max(margin);
    let y = if at.y + size.height + margin > viewport.height {
        (at.y - size.height).max(margin)
    } else {
        at.y
    };
    Bounds::new(gpui::point(x, y), size)
}

/// Draws the open menu again: its host window (desktop) and the layer in its field's window.
fn refresh(cx: &mut App) {
    let Some((parent, frame, layer)) = OPEN.with(|open| {
        open.borrow()
            .as_ref()
            .map(|menu| (menu.parent, menu.frame, menu.layer.clone()))
    }) else {
        return;
    };
    #[cfg(not(ghostex_web))]
    crate::app::window::frosted_host::show_frosted_host(
        crate::app::window::frosted_host::FrostedHostKind::TextFieldMenu,
        parent,
        frame,
        None,
        Rc::new(|_, _| menu_panel(true)),
        None,
        cx,
    );
    #[cfg(ghostex_web)]
    let _ = frame;
    notify_layer(parent, layer, cx);
}

/// Escape closes the menu, the arrow keys move between its rows and Return picks one; any other
/// key closes it and reaches the field.
fn menu_key(event: &KeystrokeEvent, window: &mut Window, cx: &mut App) {
    let keystroke = &event.keystroke;
    let plain = !keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" if plain => {
            cx.stop_propagation();
            close_text_field_menu(cx);
        }
        "up" | "down" if plain => {
            cx.stop_propagation();
            let down = keystroke.key == "down";
            OPEN.with(|open| {
                if let Some(menu) = open.borrow_mut().as_mut() {
                    menu.selected = step(&menu.lines, menu.selected, down);
                }
            });
            refresh(cx);
        }
        // As in a system menu, Return with no row picked only closes the menu.
        "enter" if plain => {
            cx.stop_propagation();
            match OPEN.with(|open| open.borrow().as_ref().and_then(|menu| menu.selected)) {
                Some(index) => activate(index, window, cx),
                None => close_text_field_menu(cx),
            }
        }
        _ => close_text_field_menu(cx),
    }
}

/// The next enabled row from `from`, down or up, wrapping around.
fn step(lines: &[Line], from: Option<usize>, down: bool) -> Option<usize> {
    let count = lines.len();
    (1..=count)
        .map(|offset| match (from, down) {
            (None, true) => offset - 1,
            (None, false) => count - offset,
            (Some(at), true) => (at + offset) % count,
            (Some(at), false) => (at + count - offset % count) % count,
        })
        .find(|&index| lines[index].enabled_row())
        .or(from)
}

/// Picks row `index`: closes the menu and runs the row in its field's window. `window` is the
/// window the pick came from (the host's, or the field's own on the web).
fn activate(index: usize, window: &mut Window, cx: &mut App) {
    let Some((parent, focus, lines)) = OPEN.with(|open| {
        open.borrow()
            .as_ref()
            .map(|menu| (menu.parent, menu.focus.clone(), menu.lines.clone()))
    }) else {
        return;
    };
    let Some(Line::Row(row)) = lines.get(index) else {
        return;
    };
    if row.disabled {
        return;
    }
    close_text_field_menu(cx);
    let run = |window: &mut Window, cx: &mut App| match &row.action {
        TextFieldMenuAction::Field(action) => {
            let action = action.boxed_clone();
            focus.focus(window, cx);
            // Deferred: the field (or a view around it whose Cut and Copy handlers update it) may
            // be the one being updated right now.
            let focus = focus.clone();
            window.defer(cx, move |window, cx| {
                focus.dispatch_action(action.as_ref(), window, cx);
            });
        }
        TextFieldMenuAction::Run(run) => run(window, cx),
    };
    if window.window_handle() == parent {
        run(window, cx);
    } else {
        let _ = parent.update(cx, |_, window, cx| run(window, cx));
    }
}

/// The menu's panel, built as the sessions sidebar's right-click menu is (`native_sidebar/menus.rs`):
/// its outline, ink and hover from the shared popup menu colours, its 34px rows, 6px inset, 8px
/// corners and 16px icons. `hosted` draws it for its frosted host window, filling it, on the
/// frosted menus' surface over that window's blur.
fn menu_panel(hosted: bool) -> AnyElement {
    let Some((lines, selected)) = OPEN.with(|open| {
        open.borrow()
            .as_ref()
            .map(|menu| (menu.lines.clone(), menu.selected))
    }) else {
        return div().into_any_element();
    };
    let foreground = titlebar_popup_menu_foreground();
    let hover = titlebar_popup_menu_hover_color();
    let border = titlebar_popup_menu_border_color();
    let mut panel = div()
        .id("text-field-menu")
        .role(gpui::Role::Menu)
        .aria_label("Edit")
        .font_family(crate::ui_fonts::UI_FONT)
        .when(hosted, |panel| panel.size_full())
        .when(!hosted, |panel| panel.w(px(MENU_WIDTH)))
        .flex()
        .flex_col()
        .p(px(MENU_PADDING))
        .rounded(px(MENU_RADIUS))
        .border_1()
        .border_color(border)
        .bg(if hosted {
            popup_menu_surface()
        } else {
            titlebar_popup_menu_background()
        })
        .text_color(foreground)
        .text_size(px(13.0))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation());
    for (index, line) in lines.iter().enumerate() {
        let row = match line {
            Line::Separator => {
                panel = panel.child(
                    div()
                        .h(px(SIDEBAR_MENU_SEPARATOR_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(div().h(px(1.0)).w_full().bg(border)),
                );
                continue;
            }
            Line::Row(row) => row,
        };
        let disabled = row.disabled;
        panel = panel.child(
            div()
                .id(("text-field-menu-row", index))
                .role(gpui::Role::MenuItem)
                .aria_label(row.label.clone())
                .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                    activate(index, window, cx);
                })
                .w_full()
                .h(px(SIDEBAR_MENU_ROW_HEIGHT))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(10.0))
                .gap(px(8.0))
                .rounded(px(6.0))
                .when(selected == Some(index), |row| row.bg(hover))
                .when(!disabled, |row| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            activate(index, window, cx);
                        })
                })
                .when(disabled, |row| row.opacity(0.42))
                .child(
                    svg()
                        .path(row.icon)
                        .size(px(16.0))
                        .flex_shrink_0()
                        .text_color(foreground),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(row.label.clone()),
                ),
        );
    }
    panel.into_any_element()
}

/// Every window's piece of the menu, carried by its GPUI Kit `Root`: while the menu is open over
/// this window, the frame it covers, which closes it on a press anywhere else in the window (and
/// on the web, where there are no host windows, the menu itself).
pub(crate) struct TextFieldMenuLayer {
    window: AnyWindowHandle,
    _activation: Subscription,
}

impl TextFieldMenuLayer {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Another window or app taking the keyboard closes the menu, as it closes a system menu.
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active()
                && OPEN.with(|open| {
                    open.borrow()
                        .as_ref()
                        .is_some_and(|menu| menu.parent == this.window)
                })
            {
                close_text_field_menu(cx);
            }
        });
        Self {
            window: window.window_handle(),
            _activation: activation,
        }
    }
}

impl gpui_component::RootPlugin for TextFieldMenuLayer {}

impl Render for TextFieldMenuLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        layer_element(self.window).unwrap_or_else(|| div().into_any_element())
    }
}

/// The layer for a window whose view is not inside a GPUI Kit `Root` (the voice agent's panel, the
/// capture editor): that view draws it among its own children.
pub(crate) fn rootless_layer(window: &Window) -> Option<AnyElement> {
    layer_element(window.window_handle())
}

/// While the menu is open over `window`: the frame it covers, which closes it on a press anywhere
/// else in the window (and on the web, where there are no host windows, the menu itself).
fn layer_element(window: AnyWindowHandle) -> Option<AnyElement> {
    let frame = OPEN.with(|open| {
        open.borrow()
            .as_ref()
            .filter(|menu| menu.parent == window)
            .map(|menu| menu.frame)
    })?;
    let place = div()
        .id("text-field-menu-place")
        .w(frame.size.width)
        .h(frame.size.height)
        .occlude()
        .on_mouse_down_out(|_, _, cx| close_text_field_menu(cx));
    // The desktop draws the menu in its host window above this one; this window keeps only the
    // frame it covers, so nothing under it lights up or takes a click.
    #[cfg(ghostex_web)]
    let place = place.child(menu_panel(false));
    Some(
        deferred(anchored().position(frame.origin).child(place))
            .with_priority(MENU_PRIORITY)
            .into_any_element(),
    )
}
