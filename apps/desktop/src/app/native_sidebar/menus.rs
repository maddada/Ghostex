use super::{
    actions::NativeSidebarAction,
    hover_actions::{SESSION_HOVER_BUTTON, SESSION_HOVER_ICON},
    menu_state::{
        SIDEBAR_MENU_ARM_TIMEOUT, SIDEBAR_MENU_SEARCH_HEIGHT, SidebarMenuPanel,
        SidebarMenuPlacement, SidebarMenuState,
    },
};
use crate::app::window::native_modal_kit::modal_typed_filter_field;
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Bounds, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, Point, StatefulInteractiveElement, Styled, Window, deferred, div, px,
};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::{Root, h_flex, v_flex};
use serde_json::Value;

pub(crate) use super::menu_state::menu_left_at_button;

/// The gap between a sidebar row or button and the menu hung from it.
pub(crate) const SIDEBAR_MENU_OWNER_GAP: f32 = 3.0;

/// The sidebar menu button's trigger id.
pub(crate) const SIDEBAR_MORE_TRIGGER: &str = "sidebar-more";

/// The hover group of a menu row whose button shows while the pointer is on it.
const MENU_ROW_GROUP: &str = "native-menu-row";

/// What a sidebar menu hangs from, and the button that toggles it.
pub(crate) struct SidebarMenuOpener {
    /// The row or button the menu opens from; it stays uncovered.
    pub(crate) owner: Bounds<Pixels>,
    /// What the menu lines up with across: the owner, the button inside it that opened the menu,
    /// or a zero-width span at a right-click's pointer.
    pub(crate) from: Bounds<Pixels>,
    /// Line the menu's right edge up with the owner's instead of its left edge.
    pub(crate) align_right: bool,
    /// The gap to the owner, unscaled.
    pub(crate) gap: f32,
    /// A button that opened the menu, by a stable id, so pressing it again closes the menu.
    /// `None` for a menu that a row's right-click opened.
    pub(crate) trigger: Option<String>,
    /// Where the press that opened it went down (`press_position`), to tell the press that
    /// dismissed an open menu from a new one.
    pub(crate) press: Option<Point<Pixels>>,
    /// Makes it a dropdown no taller than this (unscaled), always below the owner, its rows
    /// scrolling inside it.
    pub(crate) max_height: Option<f32>,
}

impl SidebarMenuOpener {
    /// A menu hung `SIDEBAR_MENU_OWNER_GAP` below `owner`, left-aligned with it.
    pub(crate) fn below(owner: Bounds<Pixels>) -> Self {
        Self {
            owner,
            from: owner,
            align_right: false,
            gap: SIDEBAR_MENU_OWNER_GAP,
            trigger: None,
            press: None,
            max_height: None,
        }
    }

    /// The same, lined up with `button`, a button inside the owner: its left edge on the button's,
    /// or its right edge when the button sits in the owner's right half.
    pub(crate) fn from_button(mut self, button: Bounds<Pixels>) -> Self {
        self.align_right = button.center().x > self.owner.center().x;
        self.from = button;
        self
    }

    /// The same, with its left edge at a right-click's pointer (its right edge there when the
    /// sidebar has no room to the pointer's right).
    pub(crate) fn at_pointer(mut self, pointer: Point<Pixels>) -> Self {
        self.align_right = false;
        self.from = Bounds {
            origin: Point::new(pointer.x, self.owner.top()),
            size: gpui::size(px(0.0), self.owner.size.height),
        };
        self
    }

    /// The same, toggled by the button `trigger`, opened by the press at `press`.
    pub(crate) fn toggled_by(
        mut self,
        trigger: impl Into<String>,
        press: Option<Point<Pixels>>,
    ) -> Self {
        self.trigger = Some(trigger.into());
        self.press = press;
        self
    }

    /// The same, as a dropdown capped at `max_height` (unscaled).
    pub(crate) fn capped(mut self, max_height: f32) -> Self {
        self.max_height = Some(max_height);
        self
    }
}

/// Where a row or button that opens a menu was last painted, for the menu to hang from.
pub(crate) type MenuOwner = std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>;

/// An `on_prepaint` callback that records the element's bounds into `owner`.
pub(crate) fn record_menu_owner(
    owner: &MenuOwner,
) -> impl FnOnce(Bounds<Pixels>, &mut Window, &mut gpui::App) + 'static {
    let owner = owner.clone();
    move |bounds, _, _| owner.set(Some(bounds))
}

/// Where the press behind a click went down, `None` for a keyboard click.
pub(crate) fn press_position(event: &gpui::ClickEvent) -> Option<Point<Pixels>> {
    match event {
        gpui::ClickEvent::Mouse(event) => Some(event.down.position),
        gpui::ClickEvent::Touch(event) => Some(event.position),
        gpui::ClickEvent::Keyboard(_) => None,
    }
}

impl GhostexGpuiApp {
    pub(crate) fn show_native_sidebar_menu(
        items: &Value,
        position: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        Self::open_native_sidebar_menu(
            items,
            position,
            SidebarMenuPlacement::Pointer,
            None,
            None,
            scale,
            window,
            cx,
        );
    }

    /// CDXC:Sidebar 2026-10-13 DECISION:
    /// User: "make the context menus that appear below the session card not appear aligned to the left always, should be related to where the button that spawned them or the right click that i did is". A menu opened from a sidebar row, such as a session card's tag menu or the project header's new-agent menu, still opens 3px below that row, never covering it, and above it only when there is no room below; the workspace menu likewise opens above its button, 3px away. Across, a right-click's menu puts its left edge at the pointer, and a button's menu lines up with that button (its right edge on the button's when the button sits in the row's right half), switching sides or shifting to stay inside the sidebar. The sidebar menu button keeps its 2026-09-21 right-aligned drop. This supersedes the 2026-10-11 rule that left-aligned every such menu with its row.
    /// CDXC:Sidebar 2026-10-11 DECISION:
    /// User: clicking the button that opened a menu while it is open closes it, "as should happen for all menus like this in the app". The outside press dismisses the menu first and records the trigger and where the press went down; the click that same press then makes on the trigger (`press_position`) is the closing half and opens nothing. This supersedes the 2026-09-21 250ms rule that did this for the sidebar menu button alone.
    pub(crate) fn show_native_sidebar_menu_from(
        items: &Value,
        opener: SidebarMenuOpener,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let placement = SidebarMenuPlacement::Attached {
            owner: opener.owner,
            from: opener.from,
            align_right: opener.align_right,
            gap: px(opener.gap * scale),
        };
        let anchor = Point::new(opener.owner.left(), opener.owner.bottom());
        let toggle = opener.trigger.map(|trigger| (trigger, opener.press));
        let max_height = opener.max_height.map(|height| px(height * scale));
        Self::open_native_sidebar_menu(
            items, anchor, placement, toggle, max_height, scale, window, cx,
        );
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: the sidebar menu drops down from its trigger button rather than opening where the pointer was: its right edge lines up with the button's right edge, and it sits below the button with a gap (4px since 2026-09-25, `HEADER_MENU_TRIGGER_GAP`). Clicking the button again closes it.
    /// The More menu with its Collapse All row saying what the next press does to the Unified
    /// groups, which fold without the menu being rebuilt.
    pub(crate) fn native_sidebar_more_menu_now(&self, items: &Value) -> Value {
        let mut items = items.clone();
        let all_folded = self.native_sidebar.unified.as_ref().is_some_and(|unified| {
            let saved = &self.gx_store.unified_sidebar.saved;
            !unified.groups.is_empty()
                && unified
                    .groups
                    .iter()
                    .all(|group| saved.folded_groups.contains(&group.repo_key))
        });
        if all_folded && let Some(rows) = items.as_array_mut() {
            for row in rows {
                if row["command"]["action"] == ghostex_gx_core::UNIFIED_TOGGLE_GROUPS_ACTION {
                    row["label"] = Value::from("Expand All");
                }
            }
        }
        items
    }

    pub(crate) fn toggle_native_sidebar_more_menu(
        items: &Value,
        trigger: Bounds<Pixels>,
        press: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        Self::show_native_sidebar_menu_from(
            items,
            SidebarMenuOpener {
                owner: trigger,
                from: trigger,
                align_right: true,
                gap: crate::app::consts::HEADER_MENU_TRIGGER_GAP,
                trigger: None,
                press: None,
                max_height: None,
            }
            .toggled_by(SIDEBAR_MORE_TRIGGER, Some(press)),
            scale,
            window,
            cx,
        );
    }

    /// A row's right-click menu: hung below the row (`owner`) when it was painted, its left edge
    /// at the pointer; else where the pointer is.
    pub(crate) fn show_native_sidebar_context_menu(
        items: &Value,
        owner: Option<Bounds<Pixels>>,
        pointer: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        match owner {
            Some(owner) => Self::show_native_sidebar_menu_from(
                items,
                SidebarMenuOpener::below(owner).at_pointer(pointer),
                scale,
                window,
                cx,
            ),
            None => Self::show_native_sidebar_menu(items, pointer, scale, window, cx),
        }
    }

    /// A menu hung below `owner` (a row or a button) when it was painted, else where the pointer
    /// is; `button` is the button inside the row that opened it, for the menu to line up with,
    /// and `trigger` names the button that toggles it and the press that opened it.
    pub(crate) fn show_native_sidebar_menu_hung(
        items: &Value,
        owner: Option<Bounds<Pixels>>,
        button: Option<Bounds<Pixels>>,
        trigger: Option<(String, Option<Point<Pixels>>)>,
        fallback: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        match owner {
            Some(owner) => {
                let mut opener = SidebarMenuOpener::below(owner);
                if let Some(button) = button {
                    opener = opener.from_button(button);
                }
                if let Some((trigger, press)) = trigger {
                    opener = opener.toggled_by(trigger, press);
                }
                Self::show_native_sidebar_menu_from(items, opener, scale, window, cx);
            }
            None => Self::open_native_sidebar_menu(
                items,
                fallback,
                SidebarMenuPlacement::Pointer,
                trigger,
                None,
                scale,
                window,
                cx,
            ),
        }
    }

    /// Closes the menu for a press outside it, remembering which button opened it and where the
    /// press went down, so that press's click on the same button does not open it again.
    pub(crate) fn dismiss_native_sidebar_menu_by_press(
        &mut self,
        press: Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // The sidebar root and the menu's layer both see one outside press; only the first, which
        // still finds the menu open, records it.
        if let Some(menu) = &self.native_sidebar.menu {
            self.native_sidebar.menu_dismissed_by_press =
                menu.trigger.clone().map(|trigger| (trigger, press));
        }
        self.close_native_sidebar_menu(window, cx);
    }

    #[allow(clippy::too_many_arguments)]
    fn open_native_sidebar_menu(
        items: &Value,
        position: Point<Pixels>,
        placement: SidebarMenuPlacement,
        toggle: Option<(String, Option<Point<Pixels>>)>,
        max_height: Option<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let Some(mut items) = items.as_array().filter(|items| !items.is_empty()).cloned() else {
            return;
        };
        let app = window.root::<Root>().flatten().and_then(|root| {
            root.read(cx)
                .view()
                .clone()
                .downcast::<GhostexGpuiApp>()
                .ok()
        });
        // A menu opened from the floating panel belongs to the same app entity; the panel's own
        // window root is what the lookup above cannot see. Every platform floats now, so this is
        // no longer a macOS-only branch.
        let app = app.or_else(|| {
            window
                .root::<Root>()
                .flatten()
                .and_then(|root| {
                    root.read(cx)
                        .view()
                        .clone()
                        .downcast::<crate::app::floating_reveal::window::FloatingRevealWindow>()
                        .ok()
                })
                .and_then(|view| view.read(cx).app.upgrade())
        });
        let Some(app) = app else {
            return;
        };
        let source = window.window_handle();
        cx.defer(move |cx| {
            let _ = source.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| {
                    let dismissed = app.native_sidebar.menu_dismissed_by_press.take();
                    if let Some((trigger, press)) = &toggle {
                        if app
                            .native_sidebar
                            .menu
                            .as_ref()
                            .is_some_and(|menu| menu.opened_by(trigger))
                        {
                            app.close_native_sidebar_menu(window, cx);
                            return;
                        }
                        if press.is_some()
                            && dismissed.as_ref().is_some_and(|(dismissed, at)| {
                                dismissed == trigger && Some(*at) == *press
                            })
                        {
                            return;
                        }
                    }
                    let previous_focus = window.focused(cx);
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    if items
                        .first()
                        .is_some_and(|item| item["onOpen"]["type"] == "sessionMenu")
                    {
                        app.native_sidebar.next_menu_request += 1;
                        let owner =
                            format!("session-menu:{}", app.native_sidebar.next_menu_request);
                        items[0]["menuOwner"] = Value::String(owner.clone());
                        items[0]["onOpen"]["ownerId"] = Value::String(owner);
                    }
                    let account_panel = items
                        .first()
                        .and_then(|item| item["menuOwner"].as_str())
                        .map(|owner| (owner.to_owned(), 0));
                    let on_open = items.first().and_then(|item| item.get("onOpen")).cloned();
                    app.native_sidebar.menu = Some(SidebarMenuState {
                        window: source,
                        account_panel,
                        panels: vec![SidebarMenuPanel {
                            placement,
                            max_height,
                            ..SidebarMenuPanel::new(items, position).with_requested_search()
                        }],
                        focus,
                        previous_focus,
                        trigger: toggle.map(|(trigger, _)| trigger),
                        scale,
                        // CDXC:Tooltips 2026-10-17 DECISION: User (2026-09-15): no tooltip may show while a context menu is open; the suppression is held for as long as the menu is. The one exception, the user's choice on 2026-10-17 ("real tooltip"): the (i) beside New Orchestrator… shows its benefits as a real tooltip while the menu is open, so the suppression is lifted only while the pointer is on that (i) (`set_native_sidebar_menu_info_hover` in agent_launcher_menu.rs). Every other tooltip stays hidden while a menu is open.
                        tooltips: Some(Root::suppress_tooltips(window, cx)),
                    });
                    if let Some(command) = on_open {
                        app.dispatch_native_sidebar_ui(command, cx);
                    }
                    cx.notify();
                })
            });
        });
    }

    pub(crate) fn dismiss_native_sidebar_menu(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(menu) = self.native_sidebar.menu.take() {
            cx.defer(move |cx| {
                let _ = menu.window.update(cx, |_, window, cx| {
                    if let Some(focus) = menu.previous_focus {
                        focus.focus(window, cx);
                    }
                });
            });
            cx.notify();
        }
    }

    pub(crate) fn close_native_sidebar_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(menu) = self.native_sidebar.menu.take() {
            if let Some(focus) = menu.previous_focus {
                focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    /// CDXC:Sidebar 2026-10-31 DECISION:
    /// User: "click on a button on the right side twice to remove a project", then, of the first version: "should be when i hover on the project, not shown always", "more on the right", "never show [a word], it should always just be a red bg x icon button (same button but with red bg)". A row's `secondary` with `confirm` takes two clicks. It shows only on the row the pointer is over, the row the keyboard is on and an armed row, in the column where other rows have their tick; a ticked row's tick gives way to it there. The first click arms it: the same button in the same place, on a red fill. The second runs it, and the row leaves. It stands down after `SIDEBAR_MENU_ARM_TIMEOUT`, when the pointer leaves the row, when the keyboard moves or the query changes, and when the menu closes. With the keyboard, Right arms and Right again runs; Left or Escape stands it down.
    pub(super) fn activate_native_sidebar_menu_secondary(
        &mut self,
        panel_index: usize,
        item_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let arming = self.native_sidebar.next_menu_request + 1;
        let Some(panel) = self
            .native_sidebar
            .menu
            .as_mut()
            .and_then(|menu| menu.panels.get_mut(panel_index))
        else {
            return;
        };
        let Some(item) = panel.items.get(item_index).cloned() else {
            return;
        };
        let Some(command) = item["secondary"].get("command").cloned() else {
            return;
        };
        if !panel.row_is_armed(&item) && panel.arm_row(&item, arming) {
            self.native_sidebar.next_menu_request = arming;
            cx.spawn(async move |app, cx| {
                cx.background_executor()
                    .timer(SIDEBAR_MENU_ARM_TIMEOUT)
                    .await;
                let _ = app.update(cx, |app, cx| {
                    app.disarm_native_sidebar_menu_row(panel_index, Some(arming), cx);
                });
            })
            .detach();
            cx.notify();
            return;
        }
        panel.drop_confirmed_row(&item);
        self.dispatch_native_sidebar_ui(command, cx);
        cx.notify();
    }

    /// Stands an armed row down. `only` keeps a later arming armed, for an earlier one's timer.
    fn disarm_native_sidebar_menu_row(
        &mut self,
        panel_index: usize,
        only: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(panel) = self
            .native_sidebar
            .menu
            .as_mut()
            .and_then(|menu| menu.panels.get_mut(panel_index))
        else {
            return;
        };
        let due = panel
            .armed
            .as_ref()
            .is_some_and(|(_, arming)| only.is_none_or(|only| only == *arming));
        if due {
            panel.armed = None;
            cx.notify();
        }
    }

    /// CDXC:DesignSystem 2026-09-15 DECISION:
    /// User: Switch Account and other submenus must stay open when the pointer crosses another menu item so their contents remain easy to reach. Submenus open on a click here, never on hover, so passing over another row leaves them open.
    /// CDXC:ContextMenus 2026-09-19 DECISION:
    /// User: "please do the same for all context menus that have sub menus in the gpui app. when i
    /// click on the menu item that opens the sub menu again then close the sub menu if it's open.
    /// example in the sidebar's context menu there's a few like park and tag etc". Every sidebar
    /// menu, submenu, and flyout goes through this one activation, so the toggle lives here rather
    /// than in the per-menu builders. `toggle` is set by pointer clicks only: the keyboard's
    /// Enter/Space/Right must keep opening a submenu, and Left keeps closing it, so those callers
    /// pass `false` and a repeated Right leaves the open submenu alone. `child_item` records which
    /// row owns the panel stacked above, because `selected` also moves with the arrow keys and
    /// cannot say whether a submenu is open.
    pub(super) fn activate_native_sidebar_menu_item(
        &mut self,
        panel_index: usize,
        item_index: usize,
        row_bounds: Bounds<Pixels>,
        toggle: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // A row clicked in the frosted host window acts in the window the menu belongs to: its
        // actions open dialogs and move focus there, not in the host.
        if let Some(source) = self.native_sidebar.menu.as_ref().map(|menu| menu.window)
            && source != window.window_handle()
        {
            let app = cx.entity();
            cx.defer(move |cx| {
                let _ = source.update(cx, |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.activate_native_sidebar_menu_item(
                            panel_index,
                            item_index,
                            row_bounds,
                            toggle,
                            window,
                            cx,
                        );
                    });
                });
            });
            return;
        }
        let Some(menu) = self.native_sidebar.menu.as_mut() else {
            return;
        };
        let child_open = menu.panels.len() > panel_index + 1;
        let Some(panel) = menu.panels.get_mut(panel_index) else {
            return;
        };
        let Some(item) = panel.items.get(item_index).cloned() else {
            return;
        };
        if item["disabled"] == true || item["heading"] == true {
            return;
        }
        // This row already owns the submenu on screen, so a click on it is a request to shut it.
        let owns_open_child = child_open && panel.child_item == Some(item_index);
        if item["back"] == true {
            if let Some(items) = panel.pages.pop() {
                panel.replace_items(items);
            }
        } else if let Some(children) = item["children"].as_array() {
            if item["presentation"] == "page" {
                panel.pages.push(panel.items.clone());
                panel.replace_items(
                    std::iter::once(
                        serde_json::json!({"label": "Back", "icon": "chevron-left", "back": true}),
                    )
                    .chain(std::iter::once(serde_json::json!({"separator": true})))
                    .chain(children.clone())
                    .collect(),
                );
                truncate_panels(menu, panel_index + 1);
            } else if owns_open_child {
                if toggle {
                    close_child_panel(menu, panel_index, item_index);
                }
            } else {
                panel.selected = Some(item_index);
                panel.child_item = Some(item_index);
                truncate_panels(menu, panel_index + 1);
                menu.panels.push(SidebarMenuPanel::new(
                    children.clone(),
                    Point::new(
                        row_bounds.left(),
                        row_bounds.bottom() + px(4.0 * menu.scale),
                    ),
                ));
            }
        } else if let Some(command) = item.get("command") {
            if command["type"] == "sessionAccounts" && command["action"] == "load" {
                if owns_open_child {
                    // The accounts flyout is already up; this click only takes it back down.
                    if toggle {
                        close_child_panel(menu, panel_index, item_index);
                        cx.notify();
                    }
                    return;
                }
                panel.selected = Some(item_index);
                panel.child_item = Some(item_index);
                truncate_panels(menu, panel_index + 1);
                menu.account_panel = command["sessionId"]
                    .as_str()
                    .map(|id| (id.to_owned(), panel_index + 1));
                menu.panels.push(SidebarMenuPanel::new(
                    vec![serde_json::json!({"label": "Loading accounts…", "disabled": true})],
                    Point::new(
                        row_bounds.left(),
                        row_bounds.bottom() + px(4.0 * menu.scale),
                    ),
                ));
            }
            let action = NativeSidebarAction {
                command: command.clone(),
            };
            if item["keepOpen"] != true {
                self.close_native_sidebar_menu(window, cx);
            }
            self.handle_native_sidebar_action(&action, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn render_native_sidebar_menu(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        use crate::app::window::frosted_host::{
            FrostedHostKind, SIDEBAR_MENU_HOST_LEVELS, frosted_hosting_active, show_frosted_host,
        };
        let Some(menu) = self.native_sidebar.menu.as_ref() else {
            hide_sidebar_menu_hosts(0, window.window_handle(), cx);
            return None;
        };
        let sidebar = self.native_sidebar.bounds;
        let hosted = frosted_hosting_active();
        let (panels, union) = self.native_sidebar_menu_panels(hosted, None, cx)?;
        let layers = div()
            .id("native-sidebar-menu-layers")
            .absolute()
            .left(union.left() - sidebar.left())
            .top(union.top() - sidebar.top())
            .w(union.size.width)
            .h(union.size.height)
            .track_focus(&menu.focus)
            .on_mouse_down_out(
                cx.listener(|app, event: &gpui::MouseDownEvent, window, cx| {
                    let position = window.mouse_position();
                    if app
                        .native_sidebar
                        .more_button_bounds
                        .get()
                        .is_some_and(|bounds| {
                            bounds.contains(&position) || bounds.contains(&event.position)
                        })
                    {
                        return;
                    }
                    app.dismiss_native_sidebar_menu_by_press(event.position, window, cx);
                }),
            )
            .on_key_down(cx.listener(Self::native_sidebar_menu_key_down));
        if hosted {
            // The panels draw in the frosted host above the sidebar; this window keeps only the
            // menu's keyboard focus and its click-outside dismissal. It still owns the menu's
            // frame: as the key window it goes on getting pointer moves under the host, and the
            // rows beneath must not light up there (the same frame its panels occluded unhosted).
            // CDXC:Sidebar 2026-09-25 DECISION: User approved ("Yes, keep it") this occluding region: while a hosted sidebar menu is open, the main window's area under it takes no hover or clicks, so only the menu reacts, as before menus were frosted.
            let layers = layers.occlude();
            // One window per panel, so a submenu stacked over its parent blurs the parent the way
            // a header dropdown blurs whatever is under it.
            let sidebar = self.native_sidebar.bounds;
            let scale = menu.scale;
            let count = menu.panels.len().min(usize::from(SIDEBAR_MENU_HOST_LEVELS));
            for (index, panel) in menu.panels.iter().take(count).enumerate() {
                let app = cx.entity();
                show_frosted_host(
                    FrostedHostKind::SidebarMenu(index as u8),
                    menu.window,
                    panel.bounds(sidebar, scale, index > 0),
                    None,
                    std::rc::Rc::new(move |_, cx| {
                        app.update(cx, |app, cx| {
                            app.native_sidebar_menu_panels(true, Some(index), cx)
                                .map(|(panel, _)| panel.into_any_element())
                                .unwrap_or_else(|| div().into_any_element())
                        })
                    }),
                    Some(cx.entity()),
                    cx,
                );
            }
            hide_sidebar_menu_hosts(count, menu.window, cx);
            return Some(deferred(layers).with_priority(20).into_any_element());
        }
        hide_sidebar_menu_hosts(0, menu.window, cx);
        Some(
            deferred(layers.child(panels))
                .with_priority(20)
                .into_any_element(),
        )
    }

    /// The menu's panels, positioned inside the box that holds them all (returned with it, in
    /// window coordinates). `frosted` draws them for a frosted host window, with the thinned fill
    /// over its blur; `only` draws just that panel, at the origin of its own window.
    fn native_sidebar_menu_panels(
        &self,
        frosted: bool,
        only: Option<usize>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(gpui::Div, Bounds<Pixels>)> {
        let menu = self.native_sidebar.menu.as_ref()?;
        let sidebar = self.native_sidebar.bounds;
        let scale = menu.scale;
        let light = chrome_uses_light_appearance();
        let panels = menu.panels.clone();
        let background = titlebar_popup_menu_background();
        let foreground = titlebar_popup_menu_foreground();
        let hover = titlebar_popup_menu_hover_color();
        let panel_bounds: Vec<_> = panels
            .iter()
            .enumerate()
            .map(|(index, panel)| panel.bounds(sidebar, scale, index > 0))
            .collect();
        let left = panel_bounds
            .iter()
            .map(|bounds| bounds.left())
            .min()
            .unwrap_or(sidebar.left());
        let top = panel_bounds
            .iter()
            .map(|bounds| bounds.top())
            .min()
            .unwrap_or(sidebar.top());
        let right = panel_bounds
            .iter()
            .map(|bounds| bounds.right())
            .max()
            .unwrap_or(left);
        let bottom = panel_bounds
            .iter()
            .map(|bounds| bounds.bottom())
            .max()
            .unwrap_or(top);
        // A panel alone in its frosted host fills the host window where the window can come out
        // taller than the panel (`hosted_menu_fills_window`).
        let fill_window =
            only.is_some() && crate::app::window::frosted_host::hosted_menu_fills_window();
        let mut layers = div()
            .relative()
            .w(right - left)
            .h(bottom - top)
            .when(fill_window, |layers| layers.size_full());
        let view = cx.entity();
        for (panel_index, panel) in panels.iter().enumerate() {
            if only.is_some_and(|only| only != panel_index) {
                continue;
            }
            let bounds = panel.bounds(sidebar, scale, panel_index > 0);
            let relative = if only.is_some() {
                Point::default()
            } else {
                bounds.origin - Point::new(left, top)
            };
            if panel.is_agent_launcher() {
                layers = layers.child(self.render_agent_launcher_menu_panel(
                    panel_index,
                    panel,
                    bounds,
                    relative,
                    scale,
                    view.clone(),
                    cx,
                ));
                continue;
            }
            let surface = if frosted {
                popup_window_surface(background)
            } else {
                background
            };
            let search_height = panel
                .search
                .as_ref()
                .map_or(0.0, |_| SIDEBAR_MENU_SEARCH_HEIGHT);
            let searchable = panel.search.is_some();
            let row_height = panel.row_height();
            let row_gap = panel.row_gap();
            let mut content = v_flex()
                .on_children_prepainted(measure_menu_panel(
                    view.clone(),
                    panel_index,
                    px((12.0 + search_height) * scale + 2.0),
                ))
                .id(format!("native-sidebar-menu-panel-{panel_index}"))
                .map(|content| {
                    if searchable {
                        // Under the field, whose wrapper draws the panel's edge and fill.
                        content.w_full().flex_1().min_h_0()
                    } else if fill_window {
                        content.w(bounds.size.width).h_full()
                    } else {
                        content.w(bounds.size.width).max_h(bounds.size.height)
                    }
                })
                .overflow_y_scroll()
                .track_scroll(&panel.scroll)
                .occlude()
                .p(px(6.0 * scale))
                .gap(px(row_gap * scale))
                .when(!searchable, |content| {
                    content
                        .rounded(px(8.0 * scale))
                        .border_1()
                        .border_color(titlebar_popup_menu_border_color())
                        .bg(surface)
                })
                .text_color(foreground)
                .font_weight(FontWeight::NORMAL)
                .text_size(px(13.0 * scale))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
            let mut offset = 6.0 + search_height;
            for (item_index, item) in panel.items.iter().enumerate() {
                if item["separator"] == true {
                    content = content.child(
                        div()
                            .h(px(13.0 * scale))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .h(px(1.0))
                                    .w_full()
                                    .bg(titlebar_popup_menu_border_color()),
                            ),
                    );
                    offset += 13.0 + row_gap;
                    continue;
                }
                let label = item["label"].as_str().unwrap_or_default().to_owned();
                let icon = item["icon"]
                    .as_str()
                    .map(|icon| gpui_sidebar_command_icon_asset_path(Some(icon)).to_string());
                let icon_color = item["iconColor"]
                    .as_str()
                    .and_then(|color| u32::from_str_radix(color.trim_start_matches('#'), 16).ok())
                    .map(|color| {
                        if !light {
                            return color;
                        }
                        if item["icon"] == "star-filled" {
                            return 0xf6c944;
                        }
                        let channel =
                            |shift| ((color >> shift & 255u32) as f32 * 0.42).round() as u32;
                        channel(16) << 16 | channel(8) << 8 | channel(0)
                    })
                    .map(gpui::rgb)
                    .map(gpui::Hsla::from)
                    .unwrap_or(foreground);
                let image = item["imageDataUrl"].as_str().and_then(|value| {
                    super::images::agent_image(value, item["agentIcon"].as_str(), light)
                });
                let color = item["color"]
                    .as_str()
                    .and_then(|color| u32::from_str_radix(color.trim_start_matches('#'), 16).ok())
                    .map(gpui::rgb);
                let disabled = item["disabled"] == true;
                let heading = item["heading"] == true;
                let selected = panel.selected == Some(item_index);
                let confirm = item["secondary"]["confirm"] == true;
                let row_bounds = Bounds {
                    origin: Point::new(bounds.left(), bounds.top() + px(offset * scale)),
                    size: gpui::size(bounds.size.width, px(row_height * scale)),
                };
                content = content.child(
                    h_flex()
                        .id(format!("native-menu-item-{panel_index}-{item_index}"))
                        .w_full()
                        .min_h(px(if heading { 24.0 } else { row_height } * scale))
                        .flex_shrink_0()
                        .px(px(10.0 * scale))
                        .gap(px(8.0 * scale))
                        .rounded(px(6.0 * scale))
                        .when(selected, |row| row.bg(hover))
                        .when(!disabled && !heading, |row| row.hover(|row| row.bg(hover)))
                        .when(disabled, |row| row.opacity(0.42))
                        .when(heading, |row| row.text_size(px(11.0 * scale)).opacity(0.6))
                        .when(item["danger"] == true, |row| {
                            row.text_color(chrome_color(0xff7b72, 0xd32f2f))
                        })
                        .when_some(icon, |row, icon| {
                            row.child(
                                gpui::svg()
                                    .path(icon)
                                    .text_color(icon_color)
                                    .size(px(16.0 * scale))
                                    .flex_shrink_0(),
                            )
                        })
                        .when_some(image, |row, image| {
                            row.child(gpui::img(image).size(px(16.0 * scale)).flex_shrink_0())
                        })
                        .when_some(color, |row, color| {
                            row.child(div().size(px(14.0 * scale)).rounded_full().bg(color))
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .gap(px(4.0 * scale))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                // A long suffix (Link to's "@owner's untitled
                                                // project") gives way before the label does.
                                                .when(item["suffix"].is_string(), |label| {
                                                    label.flex_shrink_0()
                                                })
                                                .child(label),
                                        )
                                        .when_some(item["suffix"].as_str(), |row, suffix| {
                                            row.child(
                                                div()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .text_size(px(10.0 * scale))
                                                    .opacity(0.6)
                                                    .child(suffix.to_owned()),
                                            )
                                        }),
                                )
                                .when_some(
                                    item["detail"].as_str().filter(|detail| !detail.is_empty()),
                                    |column, detail| {
                                        column.child(
                                            div()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .text_size(px(10.0 * scale))
                                                .opacity(0.6)
                                                .child(detail.to_owned()),
                                        )
                                    },
                                ),
                        )
                        .when(item["supportsChat"] == true, |row| {
                            row.child(
                                gpui::svg()
                                    .path("titlebar/message-circle.svg")
                                    .size(px(14.0 * scale))
                                    .text_color(foreground.opacity(0.5)),
                            )
                        })
                        .when_some(item.get("secondary"), |row, secondary| {
                            let label = secondary["label"].as_str().unwrap_or_default().to_owned();
                            let icon_only = label.is_empty();
                            let icon_path =
                                gpui_sidebar_command_icon_asset_path(secondary["icon"].as_str());
                            let armed = panel.row_is_armed(item);
                            // Painted over its column rather than added and removed, so no row
                            // changes size as it appears.
                            let revealed = !confirm || armed || selected;
                            let overlay = || {
                                div()
                                    .absolute()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                            };
                            row.child(
                                h_flex()
                                    .id(format!("native-menu-secondary-{panel_index}-{item_index}"))
                                    .role(gpui::Role::Button)
                                    .when_some(secondary["ariaLabel"].as_str(), |button, name| {
                                        button.aria_label(if armed {
                                            format!("Click again: {name}")
                                        } else {
                                            name.to_owned()
                                        })
                                    })
                                    .relative()
                                    .flex_shrink_0()
                                    .rounded(px(4.0 * scale))
                                    .hover(|button| button.bg(hover))
                                    .map(|button| {
                                        // A two-click button is a session row's hover button, its
                                        // icon centred on the tick's column: half the difference
                                        // between it and the 14px tick hangs past the row's padding.
                                        if confirm {
                                            button
                                                .size(px(SESSION_HOVER_BUTTON * scale))
                                                .justify_center()
                                                .mr(px((14.0 - SESSION_HOVER_BUTTON) / 2.0 * scale))
                                        } else {
                                            button.h(px(28.0 * scale))
                                        }
                                    })
                                    // An icon-only button (the workspace menu's new-window button)
                                    // is a square whose icon centre lines up with the checkmark
                                    // other rows show at the same right edge (14px wide, flush).
                                    .when(icon_only && !confirm, |button| {
                                        button
                                            .w(px(22.0 * scale))
                                            .justify_center()
                                            .mr(px(-4.0 * scale))
                                    })
                                    .when(!icon_only, |button| {
                                        button.px(px(5.0 * scale)).gap(px(3.0 * scale))
                                    })
                                    .when(armed, |button| {
                                        let red = gpui::rgb(0xdc2626);
                                        button.bg(red).hover(move |button| button.bg(red))
                                    })
                                    .map(|button| {
                                        let icon_size =
                                            if confirm { SESSION_HOVER_ICON } else { 14.0 };
                                        let icon =
                                            gpui::svg().path(icon_path).size(px(icon_size * scale));
                                        if confirm {
                                            button
                                                .when(item["checked"] == true, |button| {
                                                    button.child(
                                                        overlay()
                                                            .when(revealed, |tick| {
                                                                tick.opacity(0.0)
                                                            })
                                                            .group_hover(MENU_ROW_GROUP, |tick| {
                                                                tick.opacity(0.0)
                                                            })
                                                            .child(titlebar_svg_icon(
                                                                "titlebar/check.svg",
                                                                14.0 * scale,
                                                                foreground,
                                                            )),
                                                    )
                                                })
                                                .child(
                                                    overlay()
                                                        .when(!revealed, |mark| mark.opacity(0.0))
                                                        .group_hover(MENU_ROW_GROUP, |mark| {
                                                            mark.opacity(1.0)
                                                        })
                                                        .child(icon.text_color(if armed {
                                                            gpui::white()
                                                        } else {
                                                            foreground.opacity(0.55)
                                                        })),
                                                )
                                        } else {
                                            button.child(icon.text_color(foreground))
                                        }
                                    })
                                    .when(!icon_only, |button| button.child(label))
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.activate_native_sidebar_menu_secondary(
                                            panel_index,
                                            item_index,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        // A button that takes two clicks stands down as the pointer leaves, and
                        // shows while the pointer is on its row.
                        .when(confirm, |row| {
                            row.group(MENU_ROW_GROUP).on_hover(cx.listener(
                                move |app, hovered: &bool, _, cx| {
                                    if !*hovered {
                                        app.disarm_native_sidebar_menu_row(panel_index, None, cx);
                                    }
                                },
                            ))
                        })
                        .when(item["checked"] == true && !confirm, |row| {
                            row.child(titlebar_svg_icon(
                                "titlebar/check.svg",
                                14.0 * scale,
                                foreground,
                            ))
                        })
                        .when(
                            item.get("children").is_some()
                                || item["command"]["type"] == "sessionAccounts"
                                    && item["command"]["action"] == "load",
                            |row| {
                                row.child(titlebar_svg_icon(
                                    "titlebar/chevron-right.svg",
                                    14.0 * scale,
                                    foreground,
                                ))
                            },
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            cx.stop_propagation();
                            app.activate_native_sidebar_menu_item(
                                panel_index,
                                item_index,
                                row_bounds,
                                true,
                                window,
                                cx,
                            );
                        })),
                );
                offset += if heading { 24.0 } else { row_height } + row_gap;
            }
            // The bar shows only while the rows scroll, like the agent picker's (new_thread_picker.rs).
            let scrollbar = Scrollbar::vertical(&panel.scroll)
                .mode(ScrollbarMode::Scrolling)
                .thickness(px(crate::app::consts::TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH));
            layers = layers.child(
                div()
                    .absolute()
                    .left(relative.x)
                    .top(relative.y)
                    .when(fill_window, |panel| panel.size_full())
                    .map(|layer| match &panel.search {
                        Some(search) => layer.child(
                            v_flex()
                                .id(format!("native-sidebar-menu-search-{panel_index}"))
                                .w(bounds.size.width)
                                .map(|column| {
                                    if fill_window {
                                        column.h_full()
                                    } else {
                                        column.h(bounds.size.height)
                                    }
                                })
                                .occlude()
                                .rounded(px(8.0 * scale))
                                .border_1()
                                .border_color(titlebar_popup_menu_border_color())
                                .bg(surface)
                                .overflow_hidden()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .h(px(SIDEBAR_MENU_SEARCH_HEIGHT * scale))
                                        .flex_shrink_0()
                                        .px(px(6.0 * scale))
                                        .pt(px(6.0 * scale))
                                        .pb(px(2.0 * scale))
                                        .child(
                                            modal_typed_filter_field(
                                                light,
                                                surface.into(),
                                                &search.query,
                                                &search.placeholder,
                                                foreground.into(),
                                                foreground.opacity(0.6).into(),
                                                14.0 * scale,
                                                16.0 * scale,
                                            )
                                            .h(px(32.0 * scale))
                                            .text_size(px(13.0 * scale)),
                                        ),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_h_0()
                                        .child(content)
                                        .child(scrollbar),
                                ),
                        ),
                        None => layer.child(content).child(scrollbar),
                    }),
            );
        }
        Some((
            layers,
            Bounds::from_corners(Point::new(left, top), Point::new(right, bottom)),
        ))
    }

    fn native_sidebar_menu_key_down(
        app: &mut Self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let Some(menu) = app.native_sidebar.menu.as_mut() else {
            return;
        };
        // A searchable menu types what isn't a movement key, a space included (`SidebarMenuSearch`).
        if let Some(panel) = menu.panels.last_mut()
            && panel.search_key(&event.keystroke)
        {
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let searching = menu
            .panels
            .last()
            .is_some_and(|panel| panel.search.is_some());
        // An armed button stands down before Escape closes anything, and as the keyboard moves.
        if let Some(panel) = menu.panels.last_mut()
            && panel.armed.is_some()
            && matches!(key, "escape" | "left" | "up" | "down" | "home" | "end")
        {
            panel.armed = None;
            if matches!(key, "escape" | "left") {
                window.prevent_default();
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        // The agent account page returns on Escape or Left, like the React launcher.
        let back = menu
            .panels
            .last()
            .and_then(|panel| panel.items.first())
            .and_then(|item| item.get("command"))
            .filter(|command| command["type"] == "agentAccounts" && command["action"] == "root")
            .cloned();
        match key {
            "escape" => {
                if let Some(command) = back {
                    app.dispatch_native_sidebar_ui(command, cx);
                } else {
                    app.close_native_sidebar_menu(window, cx);
                }
            }
            "left" => {
                if menu.panels.len() > 1 {
                    let len = menu.panels.len() - 1;
                    truncate_panels(menu, len);
                    // The parent row keeps the keyboard cursor but no longer owns a panel,
                    // so Right reopens the submenu instead of finding it already open.
                    if let Some(panel) = menu.panels.last_mut() {
                        panel.child_item = None;
                    }
                } else if let Some(items) = menu.panels[0].pages.pop() {
                    menu.panels[0].replace_items(items);
                } else if let Some(command) = back {
                    app.dispatch_native_sidebar_ui(command, cx);
                }
            }
            "up" | "down" | "home" | "end" => {
                if let Some(panel) = menu.panels.last_mut() {
                    panel.move_selection(key);
                }
            }
            "enter" | "space" | "right" => {
                let panel_index = menu.panels.len() - 1;
                let panel = &menu.panels[panel_index];
                if let Some(index) = panel.selected {
                    if key == "right" && panel.items[index].get("secondary").is_some() {
                        app.activate_native_sidebar_menu_secondary(panel_index, index, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    // In a search only Enter picks: Right means the button above or nothing.
                    if searching && key == "right" {
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    let bounds =
                        panel.bounds(app.native_sidebar.bounds, menu.scale, panel_index > 0);
                    let y = panel.items[..index]
                        .iter()
                        .map(|item| {
                            if item["separator"] == true {
                                13.0
                            } else {
                                panel.row_height()
                            }
                        })
                        .sum::<f32>();
                    let row_bounds = Bounds {
                        origin: Point::new(
                            bounds.left(),
                            bounds.top() + px((6.0 + y) * menu.scale),
                        ),
                        size: gpui::size(bounds.size.width, px(panel.row_height() * menu.scale)),
                    };
                    app.activate_native_sidebar_menu_item(
                        panel_index,
                        index,
                        row_bounds,
                        false,
                        window,
                        cx,
                    );
                }
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}

/// Drops the panel that `item_index` opened on top of `panel_index`, and everything stacked above
/// it, then clears the row's expanded highlight so the parent menu no longer looks open.
fn close_child_panel(menu: &mut SidebarMenuState, panel_index: usize, item_index: usize) {
    if let Some(panel) = menu.panels.get_mut(panel_index) {
        panel.child_item = None;
        if panel.selected == Some(item_index) {
            panel.selected = None;
        }
    }
    truncate_panels(menu, panel_index + 1);
}

/// Every panel stack cut goes through here: an answer still on its way to a panel that is gone
/// (closed, popped with Left, or replaced by another row's submenu at the same index) would
/// otherwise land in whatever panel sits at that index now.
fn truncate_panels(menu: &mut SidebarMenuState, len: usize) {
    menu.panels.truncate(len);
    if menu
        .account_panel
        .as_ref()
        .is_some_and(|(_, index)| *index >= len)
    {
        menu.account_panel = None;
    }
}

/// Records the rendered rows' height (plus `chrome`: vertical padding and border) so the panel's next frame fits them exactly.
pub(super) fn measure_menu_panel(
    view: gpui::Entity<GhostexGpuiApp>,
    panel_index: usize,
    chrome: Pixels,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut gpui::App) + 'static {
    move |rows, _, cx| {
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
            return;
        };
        let height = last.bottom() - first.top() + chrome;
        view.update(cx, |app, cx| {
            let Some(panel) = app
                .native_sidebar
                .menu
                .as_mut()
                .and_then(|menu| menu.panels.get_mut(panel_index))
            else {
                return;
            };
            if panel
                .measured_height
                .is_none_or(|measured| (measured - height).abs() > px(0.5))
            {
                panel.measured_height = Some(height);
                // GPUI suppresses invalidation during prepaint. Apply the new
                // bounds on a fresh frame even if the user stops interacting.
                let view = cx.weak_entity();
                cx.defer(move |cx| {
                    let _ = view.update(cx, |_, cx| cx.notify());
                });
            }
        });
    }
}

/// Hides the sidebar menu's panel windows from `from` up, but only those sitting over `over`.
///
/// CDXC:Sidebar 2026-10-09 WHY:
/// The panel host windows are shared by every workspace window, while each workspace window has its own sidebar and menu. Hiding them unconditionally let a second window without a menu hide the first window's open menu on each of its frames, and the first window showed it again on its next one, so the menu flickered while two windows were open.
fn hide_sidebar_menu_hosts(from: usize, over: gpui::AnyWindowHandle, cx: &mut gpui::App) {
    use crate::app::window::frosted_host::{
        FrostedHostKind, SIDEBAR_MENU_HOST_LEVELS, hide_frosted_host_over,
    };
    for level in from..usize::from(SIDEBAR_MENU_HOST_LEVELS) {
        hide_frosted_host_over(FrostedHostKind::SidebarMenu(level as u8), over, cx);
    }
}
