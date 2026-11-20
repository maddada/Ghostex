//! Which of the header's controls are pinned, and the ⋯ menu that holds the rest.
//!
//! CDXC:Titlebar 2026-11-19 DECISION:
//! User (header page, Q4196 C): "too many buttons here, and some of the icons visually look bigger than others". The header keeps the chat's and project's own controls (Start, Open, Commit, Work, the context ring, the two panel toggles) and a ⋯ menu holds the rest (Grok Bot, Claude, Voice, Share, Open in a popup). Then: "when i hover on one of these pls show the pin icon and i should be able to pin some of these", "please allow unpinning any of the buttons that exist now by right clicking on it", "pleas make the 3 dots dropdown organized well". So every control here can be pinned from its ⋯ row (the pin shows on hover) and unpinned by right-clicking it in the header; the choice is `workareaHeaderPins` (control key → pinned) in this computer's settings file, beside the other header settings, so it stays on this computer. A control that is in use (its bot page on, a voice call open, the chat shared) stays in the header while it is, pinned or not, so it can be turned off where it is seen. The ⋯ menu groups its rows under This chat, Project, Agents and Panels, each with what it is doing now. The trailing ⋯ removed on 2026-11-11 (CDXC:Titlebar 2026-11-11 in titlebar/menu_panels.rs) held app menus that stay in the sidebar's menu; this one holds only the header's own controls. This supersedes the 2026-11-10 placement of the bot buttons left of Voice and the 2026-10-20 Voice-first rule while those controls are unpinned; pinned, they keep those places, and Voice keeps its outlined split look ("please keep the voice as it is now when it's pinned").
//! SEE-ALSO: action_buttons.rs (what each slot gates), titlebar/popup_lifecycle.rs (the popup).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::AnyElement;
use gpui::Bounds;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::Side;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use serde_json::{Map, Value};

use crate::app::claude_web::{HeaderControl, WebPlace};
use crate::app::consts::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

pub(crate) use crate::app::helpers::{
    HEADER_ICON_BOX, HeaderMoreRow, HeaderSlot, PinWorkareaHeaderSlot, RunWorkareaHeaderSlot,
    header_icon, header_icon_button, header_more_row,
};

/// The shared setting: an object of control key → pinned. A control it does not name takes its
/// default (`HeaderSlot::pinned_by_default`).
pub(crate) const WORKAREA_HEADER_PINS_SETTINGS_KEY: &str = "workareaHeaderPins";

/// Whether `slot` is pinned in `settings` (the shared settings object).
pub(crate) fn header_slot_pinned_in(settings: &Map<String, Value>, slot: HeaderSlot) -> bool {
    settings
        .get(WORKAREA_HEADER_PINS_SETTINGS_KEY)
        .and_then(|pins| pins.get(slot.key()))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| slot.pinned_by_default())
}

/// `settings` with `slot` pinned or not.
pub(crate) fn with_header_slot_pinned(
    mut settings: Map<String, Value>,
    slot: HeaderSlot,
    pinned: bool,
) -> Map<String, Value> {
    let pins = settings
        .entry(WORKAREA_HEADER_PINS_SETTINGS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !pins.is_object() {
        *pins = Value::Object(Map::new());
    }
    if let Some(pins) = pins.as_object_mut() {
        pins.insert(slot.key().to_string(), Value::Bool(pinned));
    }
    settings
}

/// What a control is doing now, as its ⋯ row says it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HeaderSlotState {
    /// The control would be drawn if it were pinned.
    pub(crate) available: bool,
    /// Its bot page is on, its call is open, or its chat is shared: it shows even unpinned.
    pub(crate) in_use: bool,
    /// The row's trailing words ("On", "Live · 2", "2 waiting", "40%").
    pub(crate) status: Option<String>,
    /// The status is a live state (drawn with the green dot).
    pub(crate) live: bool,
}

/// This window's ⋯ menu: kept on the window's app, so two windows never share rows or anchors.
#[derive(Default)]
pub(crate) struct HeaderPinsState {
    /// The ⋯ menu's rows by section, as the header last drew them (the menu is built and measured
    /// outside a render, where the chats cannot be read).
    more_rows: RefCell<Vec<(&'static str, Vec<HeaderMoreRow>)>>,
    /// Where the ⋯ button was last painted: its menu, and the popovers a row opens, hang from it.
    more_anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The Start, Open or Commit menu was opened by right-clicking its button, so it ends with
    /// "Unpin from header".
    pub(crate) unpin_row_in: Cell<Option<GpuiTitlebarPopupKind>>,
    /// The Voice arrow's menu was opened by right-clicking the headset: it ends with the pin row.
    pub(crate) voice_unpin_row: Cell<bool>,
}

impl HeaderPinsState {
    fn anchor(&self) -> Option<Bounds<Pixels>> {
        self.more_anchor.get()
    }
}

/// The slot whose button opens `kind`'s menu.
fn menu_slot(kind: GpuiTitlebarPopupKind) -> Option<HeaderSlot> {
    match kind {
        GpuiTitlebarPopupKind::Actions => Some(HeaderSlot::Start),
        GpuiTitlebarPopupKind::OpenTargets => Some(HeaderSlot::Open),
        GpuiTitlebarPopupKind::Git => Some(HeaderSlot::Commit),
        _ => None,
    }
}

impl GhostexGpuiApp {
    pub(crate) fn header_slot_pinned(&self, slot: HeaderSlot) -> bool {
        header_slot_pinned_in(
            shared_settings::shared_sidebar_settings_snapshot().object(),
            slot,
        )
    }

    pub(crate) fn set_header_slot_pinned(
        &mut self,
        slot: HeaderSlot,
        pinned: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        if let Err(error) = shared_settings::write_shared_sidebar_settings_object(
            with_header_slot_pinned(settings, slot, pinned),
        ) {
            log::error!("Header pins: could not save {}: {error:?}", slot.key());
        }
        cx.notify();
    }

    /// What `slot` is doing now. `available` repeats the checks its button makes before drawing.
    pub(crate) fn header_slot_state(&self, slot: HeaderSlot, cx: &gpui::App) -> HeaderSlotState {
        let shown = |control| self.header_control_shown(control);
        let mut state = HeaderSlotState::default();
        match slot {
            HeaderSlot::Work => {
                if let Some(chat) = self.session_details_target_chat() {
                    let chat = chat.read(cx);
                    state.available = chat.work_panel_available();
                    state.status = chat.work_panel_shown().then(|| "Open".into());
                }
            }
            HeaderSlot::SessionDetails => {
                if shown(HeaderControl::SessionDetails)
                    && self.built_in_extension_enabled(
                        ghostex_settings_catalog::built_in_extensions::SESSION_DETAILS,
                    )
                    && let Some(chat) = self.session_details_target_chat()
                {
                    let chat = chat.read(cx);
                    state.available = chat.session_details_available();
                    let (_, label) = chat.session_details_ring();
                    state.status = (!label.is_empty()).then_some(label);
                }
            }
            HeaderSlot::Share => {
                if let Some(collab) = self.share_button_collab(cx) {
                    state.available = true;
                    if collab["shared"] == true {
                        state.in_use = true;
                        state.live = true;
                        let watching = collab["viewerCount"].as_u64().unwrap_or(0);
                        state.status = Some(if watching > 0 {
                            format!("Live \u{b7} {watching}")
                        } else {
                            "Live".into()
                        });
                    }
                }
            }
            HeaderSlot::Popout => {
                state.available = self
                    .session_popup_focused_target()
                    .and_then(|id| self.agents_workspace.session(id))
                    .is_some_and(|session| session.agent_icon != Some("browser"));
            }
            HeaderSlot::Start => {
                let active = self.active_gpui_titlebar_action();
                state.available = (cfg!(any(target_os = "macos", target_os = "windows"))
                    || active.is_some())
                    && shown(HeaderControl::Actions)
                    && !self.built_in_feature_button_hidden(
                        ghostex_settings_catalog::built_in_extensions::ACTIONS,
                    );
                state.status = active
                    .map(|action| action.name.trim().to_string())
                    .filter(|name| !name.is_empty());
            }
            HeaderSlot::Open => {
                state.available = shown(HeaderControl::OpenIn)
                    && !self.built_in_feature_button_hidden(
                        ghostex_settings_catalog::built_in_extensions::OPEN_IN,
                    );
            }
            HeaderSlot::Commit => {
                state.available = shown(HeaderControl::Commit)
                    && !self.titlebar_button_hidden(
                        GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                        "gitActions",
                    );
                if let Some(git) = self.titlebar_git_menu_state.as_ref() {
                    let summary = super::action_buttons::WorkareaHeaderGitSummary::from_state(git);
                    if summary.has_diff() {
                        state.status = Some(format!(
                            "+{} \u{2212}{}",
                            summary.additions.min(9999),
                            summary.deletions.min(9999)
                        ));
                    }
                }
            }
            HeaderSlot::Claude | HeaderSlot::GrokBot => {
                let place = if slot == HeaderSlot::Claude {
                    WebPlace::Claude
                } else {
                    WebPlace::Grok
                };
                // With the rail on, Claude and Grok Bot are the rail's (app/sidebar_rail/).
                state.available = !self.sidebar_rail_shown()
                    && shown(HeaderControl::WebBots)
                    && place.enabled()
                    && self.titlebar_mode_available(place.mode());
                if self.web_place_on() == Some(place) {
                    state.in_use = true;
                    state.live = true;
                    state.status = Some("On".into());
                }
            }
            HeaderSlot::Voice => {
                state.available =
                    crate::app::voice_agent::voice_agent_on() && shown(HeaderControl::Voice);
                if self.voice_agent.active() {
                    state.in_use = true;
                    state.live = true;
                    state.status = Some("In a call".into());
                }
            }
            HeaderSlot::BottomPanel => {
                state.available = shown(HeaderControl::PanelToggles);
                state.status = self.command_pane.is_expanded().then(|| "Shown".into());
            }
            HeaderSlot::SidePanel => {
                state.available = shown(HeaderControl::PanelToggles);
                state.status = self.view_panel_open().then(|| "Shown".into());
            }
        }
        state
    }

    /// The header draws `slot`'s control: it applies here, and it is pinned or in use.
    pub(crate) fn header_slot_drawn(&self, slot: HeaderSlot, cx: &gpui::App) -> bool {
        let state = self.header_slot_state(slot, cx);
        state.available && (state.in_use || self.header_slot_pinned(slot))
    }

    /// The ⋯ button, after the chat's controls: shown while the menu has a row. Its rows are
    /// worked out here, while the chats can be read, and kept for the menu.
    pub(crate) fn render_workarea_header_more_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let mut sections = Vec::new();
        for (title, slots) in HEADER_MORE_SECTIONS {
            let rows: Vec<HeaderMoreRow> = slots
                .iter()
                .filter_map(|&slot| {
                    let state = self.header_slot_state(slot, cx);
                    (state.available && !self.header_slot_pinned(slot)).then(|| HeaderMoreRow {
                        slot,
                        status: state.status,
                        live: state.live,
                    })
                })
                .collect();
            if !rows.is_empty() {
                sections.push((title, rows));
            }
        }
        let empty = sections.is_empty();
        *self.header_pins.more_rows.borrow_mut() = sections;
        if empty {
            return None;
        }
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::HeaderMore);
        let text = titlebar_icon_color();
        let anchor = self.header_pins.more_anchor.clone();
        let work_drawn = self.header_slot_drawn(HeaderSlot::Work, cx);
        let details_drawn = self.header_slot_drawn(HeaderSlot::SessionDetails, cx);
        let work_bounds = self.session_details.work_button_bounds.clone();
        let details_bounds = self.session_details.button_bounds.clone();
        Some(
            header_icon_button("ghostex-gpui-workarea-header-more", open)
                .role(gpui::Role::Button)
                .aria_label("More header controls")
                .aria_expanded(open)
                .relative()
                .child(header_icon(TITLEBAR_ICON_DOTS, text))
                .tooltip(|window, cx| titlebar_tooltip("More", window, cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let open = this.titlebar_popup_menu_open(GpuiTitlebarPopupKind::HeaderMore);
                        this.set_gpui_titlebar_popup_open(
                            GpuiTitlebarPopupKind::HeaderMore,
                            !open,
                            this.header_pins.anchor(),
                            window,
                            cx,
                        );
                    }),
                )
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let app = cx.entity().downgrade();
                    move |_, window, cx| {
                        let _ = app.update(cx, |this, cx| {
                            let open =
                                this.titlebar_popup_menu_open(GpuiTitlebarPopupKind::HeaderMore);
                            this.set_gpui_titlebar_popup_open(
                                GpuiTitlebarPopupKind::HeaderMore,
                                !open,
                                this.header_pins.anchor(),
                                window,
                                cx,
                            );
                        });
                    }
                })
                // A Work or Session details card opened from the chat hangs from ⋯ while its own
                // button is in here.
                .on_prepaint(move |bounds, _, _| {
                    anchor.set(Some(bounds));
                    if !work_drawn {
                        work_bounds.set(bounds);
                    }
                    if !details_drawn {
                        details_bounds.set(bounds);
                    }
                })
                .into_any_element(),
        )
    }

    /// The ⋯ menu's height for the rows the header last drew.
    pub(crate) fn header_more_popup_content_height(&self) -> f32 {
        let label =
            TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT.max(TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT);
        let mut heights = Vec::new();
        for (index, (_, rows)) in self.header_pins.more_rows.borrow().iter().enumerate() {
            if index > 0 {
                heights.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
            }
            heights.push(label);
            heights.extend(std::iter::repeat_n(
                TITLEBAR_POPUP_MENU_ROW_HEIGHT,
                rows.len(),
            ));
        }
        titlebar_popup_menu_height_for_rows(&heights)
    }

    pub(crate) fn build_gpui_header_more_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            crate::app::titlebar::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior(
                menu, width, max_height, scrollable,
            )
            .check_side(Side::Right);
        let sections = self.header_pins.more_rows.borrow().clone();
        for (index, (title, rows)) in sections.into_iter().enumerate() {
            if index > 0 {
                menu = menu.separator();
            }
            menu = menu.item(
                PopupMenuItem::element(move |_, _| titlebar_popup_git_section_label(title))
                    .disabled(true),
            );
            for row in rows {
                let action = RunWorkareaHeaderSlot {
                    slot: row.slot.key().to_string(),
                };
                menu =
                    menu.menu_element(Box::new(action), move |_, _| header_more_row(row.clone()));
            }
        }
        menu
    }

    /// Re-reads the open ⋯ menu after a pin, or closes it once nothing is left in it. Called
    /// deferred from the pin's click (`pin_header_slot_from_menu`): that click runs inside the
    /// menu's own window, which can't be updated again until its event is done.
    pub(crate) fn refresh_open_header_more_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::HeaderMore) {
            return;
        }
        // The pin just moved a row to the header; drop it from the kept rows before measuring.
        {
            let mut sections = self.header_pins.more_rows.borrow_mut();
            for (_, rows) in sections.iter_mut() {
                rows.retain(|row| !self.header_slot_pinned(row.slot));
            }
            sections.retain(|(_, rows)| !rows.is_empty());
        }
        if self.header_pins.more_rows.borrow().is_empty() {
            self.close_gpui_titlebar_popup(Some(GpuiTitlebarPopupKind::HeaderMore), window, cx);
            return;
        }
        let Some(popup) = self.titlebar_popup_window else {
            return;
        };
        let content_height = self.header_more_popup_content_height();
        let _ = popup.update(cx, |popup, window, cx| {
            let bounds = window.bounds();
            let height = px(content_height.min(TITLEBAR_POPUP_MENU_MAX_HEIGHT));
            let width = (bounds.size.width.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let max_height = (height.as_f32() - TITLEBAR_POPUP_MENU_BORDER_CHROME).max(0.0);
            let menu = PopupMenu::build(window, cx, |menu, _, _| {
                self.build_gpui_header_more_popup_menu(menu, width, max_height, false)
            });
            window.resize(gpui::size(bounds.size.width, height));
            popup.replace_menu(menu, window, cx);
        });
    }

    /// A pin or unpin from a menu window: saved now, the ⋯ menu re-read once the menu's own event
    /// is over.
    pub(crate) fn pin_header_slot_from_menu(
        &mut self,
        slot: HeaderSlot,
        pinned: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_header_slot_pinned(slot, pinned, cx);
        cx.defer_in(window, |this, window, cx| {
            this.refresh_open_header_more_popup(window, cx);
        });
    }

    /// A ⋯ row: what the control's own button does, hung from the ⋯ button where it opens a
    /// popover. Start, Open and Voice open the choices their arrows offer (Actions, Open In
    /// targets, All sessions / This session / the GBot call), so none is out of reach unpinned.
    pub(crate) fn run_header_slot(
        &mut self,
        slot: HeaderSlot,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let anchor = self.header_pins.anchor();
        self.header_pins.unpin_row_in.set(None);
        match slot {
            HeaderSlot::Work => {
                if let Some(anchor) = anchor
                    && !self.header_slot_drawn(slot, cx)
                {
                    self.session_details.work_button_bounds.set(anchor);
                }
                self.toggle_work_panel(window, cx);
            }
            HeaderSlot::SessionDetails => {
                if let Some(anchor) = anchor
                    && !self.header_slot_drawn(slot, cx)
                {
                    self.session_details.button_bounds.set(anchor);
                }
                self.toggle_session_details(cx);
            }
            HeaderSlot::Share => {
                if !self.header_slot_drawn(slot, cx) {
                    super::share_button::set_share_anchor(anchor);
                }
                self.toggle_collab_share_popover(window, cx);
            }
            HeaderSlot::Popout => {
                if let Some(session_id) = self.session_popup_focused_target() {
                    self.open_session_popup_for_shell(session_id, cx);
                }
            }
            HeaderSlot::Start => self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::Actions,
                true,
                anchor,
                window,
                cx,
            ),
            HeaderSlot::Open => self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::OpenTargets,
                true,
                anchor,
                window,
                cx,
            ),
            HeaderSlot::Commit => self.show_gpui_titlebar_git_menu(anchor, window, cx),
            HeaderSlot::Claude => self.toggle_web_place(WebPlace::Claude, window, cx),
            HeaderSlot::GrokBot => self.toggle_web_place(WebPlace::Grok, window, cx),
            HeaderSlot::Voice => {
                if !self.header_slot_drawn(slot, cx) {
                    self.voice_agent.menu_anchor.set(anchor);
                }
                self.header_pins.voice_unpin_row.set(false);
                self.toggle_voice_agent_menu(cx);
            }
            HeaderSlot::BottomPanel => self.handle_command_pane_control_action(
                CommandPaneControlAction::ToggleExpanded,
                None,
                window,
                cx,
            ),
            HeaderSlot::SidePanel => self.toggle_view_panel(window, cx),
        }
        cx.notify();
    }

    /// The pin row a control's right-click offers: Unpin, saying the control stays while it is in
    /// use, or Pin for one shown only because it is in use.
    pub(crate) fn header_slot_pin_row(
        &self,
        slot: HeaderSlot,
        cx: &gpui::App,
    ) -> (&'static str, PinWorkareaHeaderSlot) {
        let pinned = self.header_slot_pinned(slot);
        let in_use = self.header_slot_state(slot, cx).in_use;
        let label = match (pinned, in_use) {
            (true, true) => "Unpin (stays while in use)",
            (true, false) => "Unpin from header",
            (false, _) => "Pin to header",
        };
        (
            label,
            PinWorkareaHeaderSlot {
                slot: slot.key().to_string(),
                pinned: !pinned,
            },
        )
    }

    /// The last rows of a Start, Open or Commit menu opened by right-clicking its button.
    pub(crate) fn header_menu_unpin_rows(
        &self,
        kind: GpuiTitlebarPopupKind,
        menu: PopupMenu,
    ) -> PopupMenu {
        let Some(slot) =
            menu_slot(kind).filter(|_| self.header_pins.unpin_row_in.get() == Some(kind))
        else {
            return menu;
        };
        let action = PinWorkareaHeaderSlot {
            slot: slot.key().to_string(),
            pinned: false,
        };
        menu.separator().menu_element(Box::new(action), |_, _| {
            titlebar_popup_standard_menu_row(
                "titlebar/pin-outline.svg",
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                "Unpin from header".to_string(),
                false,
            )
        })
    }

    /// The height `header_menu_unpin_rows` adds to `kind`'s menu.
    pub(crate) fn header_menu_unpin_rows_height(&self, kind: GpuiTitlebarPopupKind) -> f32 {
        if menu_slot(kind).is_some() && self.header_pins.unpin_row_in.get() == Some(kind) {
            TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT
                + TITLEBAR_POPUP_MENU_ROW_HEIGHT
                + 2.0 * TITLEBAR_POPUP_MENU_ITEM_GAP
        } else {
            0.0
        }
    }

    /// Right-click on a header control without a menu of its own: its pin row.
    pub(crate) fn show_header_slot_context_menu(
        &self,
        slot: HeaderSlot,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (label, action) = self.header_slot_pin_row(slot, cx);
        GpuiContextMenu::new()
            .menu(label, Box::new(action))
            .show(position, window, cx);
    }
}

/// A right-click listener for `slot`'s control.
pub(crate) fn header_slot_right_click(
    slot: HeaderSlot,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
        window.prevent_default();
        cx.stop_propagation();
        this.show_header_slot_context_menu(slot, event.position, window, cx);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_take_their_defaults_until_the_user_pins_them() {
        let settings = Map::new();
        assert!(header_slot_pinned_in(&settings, HeaderSlot::Start));
        assert!(header_slot_pinned_in(&settings, HeaderSlot::SidePanel));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::Claude));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::Voice));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::Share));
    }

    #[test]
    fn pinning_and_unpinning_is_kept_per_control_and_leaves_other_settings_alone() {
        let mut settings = Map::new();
        settings.insert("theme".into(), Value::String("dark".into()));
        let settings = with_header_slot_pinned(settings, HeaderSlot::Voice, true);
        let settings = with_header_slot_pinned(settings, HeaderSlot::Commit, false);
        assert!(header_slot_pinned_in(&settings, HeaderSlot::Voice));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::Commit));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::Claude));
        assert_eq!(settings["theme"], "dark");
    }

    #[test]
    fn a_broken_pins_value_is_replaced_not_trusted() {
        let mut settings = Map::new();
        settings.insert(WORKAREA_HEADER_PINS_SETTINGS_KEY.into(), Value::Bool(true));
        assert!(!header_slot_pinned_in(&settings, HeaderSlot::GrokBot));
        let settings = with_header_slot_pinned(settings, HeaderSlot::GrokBot, true);
        assert!(header_slot_pinned_in(&settings, HeaderSlot::GrokBot));
    }

    #[test]
    fn every_control_has_one_key_and_one_menu_row() {
        for slot in HeaderSlot::ALL {
            assert_eq!(HeaderSlot::from_key(slot.key()), Some(slot));
            let rows = HEADER_MORE_SECTIONS
                .iter()
                .flat_map(|(_, slots)| slots.iter())
                .filter(|candidate| **candidate == slot)
                .count();
            assert_eq!(rows, 1, "{slot:?}");
        }
    }

    #[test]
    fn only_start_open_and_commit_menus_carry_the_unpin_row() {
        assert_eq!(
            menu_slot(GpuiTitlebarPopupKind::Actions),
            Some(HeaderSlot::Start)
        );
        assert_eq!(
            menu_slot(GpuiTitlebarPopupKind::OpenTargets),
            Some(HeaderSlot::Open)
        );
        assert_eq!(
            menu_slot(GpuiTitlebarPopupKind::Git),
            Some(HeaderSlot::Commit)
        );
        assert_eq!(menu_slot(GpuiTitlebarPopupKind::HeaderMore), None);
    }

    #[test]
    fn solid_logos_are_drawn_smaller_than_outline_icons() {
        assert_eq!(header_icon_size(WebPlace::Claude.icon()), 11.0);
        assert_eq!(
            WebPlace::Claude.icon(),
            crate::app::helpers::HEADER_CLAUDE_ICON
        );
        assert_eq!(WebPlace::Grok.icon(), crate::app::helpers::HEADER_GROK_ICON);
        assert_eq!(header_icon_size(WebPlace::Grok.icon()), 12.0);
        assert_eq!(header_icon_size("titlebar/users.svg"), HEADER_ICON_BOX);
    }
}
