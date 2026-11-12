//! The board's menus (Filters, card details, a card's right-click menu) and the one action their
//! rows dispatch. Menus use the shared GPUI context menu, so they look and dismiss like every other
//! menu in the app.

use gpui::{Context, Pixels, Point, Window};
use serde_json::{Value, json};

use super::filters::{CARD_VIEW_FIELDS, KanbanSort, tag_options};
use super::host::GpuiContextMenu;
use super::model::{PRIORITY_OPTIONS, TSHIRT_OPTIONS};
use crate::GhostexGpuiApp;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(crate) struct NativeKanbanAction {
    pub(crate) command: Value,
}

fn action(command: Value) -> Box<dyn gpui::Action> {
    Box::new(NativeKanbanAction { command })
}

impl GhostexGpuiApp {
    pub(crate) fn handle_native_kanban_action(
        &mut self,
        action: &NativeKanbanAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = &action.command;
        let text = |key: &str| command[key].as_str().unwrap_or_default().to_string();
        match command["type"].as_str().unwrap_or_default() {
            "setFilter" => {
                let value = text("value");
                let view = &mut self.native_kanban.view;
                match text("field").as_str() {
                    "priority" => view.priority = value,
                    "estimate" => view.estimate = value,
                    "tag" => view.tag = value,
                    "sort" => view.sort = KanbanSort::from_id(&value),
                    _ => {}
                }
                self.native_kanban.invalidate_derived();
            }
            "openFilterField" => {
                self.show_native_kanban_filter_field_menu(&text("field"), window, cx)
            }
            "resetFilters" => {
                self.native_kanban.view = Default::default();
                self.native_kanban.invalidate_derived();
            }
            "toggleCardField" => self.native_kanban.card_view.toggle(&text("key")),
            "startWork" => self.native_kanban_start_work(&text("ticketId"), cx),
            "jumpToSession" => self.native_kanban_jump_to_session(&text("ticketId"), cx),
            "openTicket" => self.native_kanban_open_ticket(&text("ticketId"), window, cx),
            "deleteTicket" => self.native_kanban_confirm_delete(&text("ticketId"), window, cx),
            _ => {}
        }
        self.native_kanban_notify(cx);
    }

    /// Focuses the board first, so the menu row's action is dispatched through the board.
    fn native_kanban_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.native_kanban.focus.as_ref() {
            focus.focus(window, cx);
        }
    }

    /// The choices of one filter field, with the current one checked.
    fn native_kanban_filter_choices(&self, field: &str) -> Vec<(String, String)> {
        let tags = tag_options(&self.native_kanban.tickets);
        let owned = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(label, value)| ((*label).to_string(), (*value).to_string()))
                .collect::<Vec<_>>()
        };
        match field {
            "priority" => {
                let mut choices = owned(&[("All priorities", "all")]);
                choices.extend(owned(&PRIORITY_OPTIONS));
                choices
            }
            "estimate" => {
                let mut choices = owned(&[("All estimates", "all"), ("None", "none")]);
                choices.extend(
                    TSHIRT_OPTIONS
                        .iter()
                        .map(|(label, _)| ((*label).to_string(), (*label).to_string())),
                );
                choices
            }
            "tag" => {
                let mut choices = owned(&[("All tags", "all")]);
                choices.extend(tags.into_iter().map(|tag| (tag.clone(), tag)));
                choices
            }
            _ => KanbanSort::ALL
                .iter()
                .map(|sort| (sort.label().to_string(), sort.id().to_string()))
                .collect(),
        }
    }

    fn native_kanban_filter_value(&self, field: &str) -> String {
        let view = &self.native_kanban.view;
        match field {
            "priority" => view.priority.clone(),
            "estimate" => view.estimate.clone(),
            "tag" => view
                .active_tag(&tag_options(&self.native_kanban.tickets))
                .to_string(),
            _ => view.sort.id().to_string(),
        }
    }

    /// `Filters`: one row per field showing its current choice; picking a row opens that field's
    /// choices in the same place. Reset appears while any filter or sort is set.
    ///
    /// CDXC:ProjectBoard 2026-09-23 WHY:
    /// The shared menu's nested submenus are sized to the parent row, which clipped the long Sort
    /// labels, so each field opens its own menu instead of a submenu.
    pub(crate) fn show_native_kanban_filters_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_kanban_focus(window, cx);
        let tags = tag_options(&self.native_kanban.tickets);
        let mut menu = GpuiContextMenu::new();
        for (field, name) in [
            ("priority", "Priority"),
            ("estimate", "Estimate"),
            ("tag", "Tags"),
            ("sort", "Sort"),
        ] {
            let current = self.native_kanban_filter_value(field);
            let label = self
                .native_kanban_filter_choices(field)
                .into_iter()
                .find(|(_, value)| *value == current)
                .map(|(label, _)| label)
                .unwrap_or(current);
            menu = menu.menu(
                format!("{name}: {label}"),
                action(json!({ "type": "openFilterField", "field": field })),
            );
        }
        if self.native_kanban.view.active_count(&tags) > 0 {
            menu = menu
                .separator()
                .menu("Reset filters", action(json!({ "type": "resetFilters" })));
        }
        menu.toggle_below(self.native_kanban.anchors.filters.get(), window, cx);
    }

    fn show_native_kanban_filter_field_menu(
        &mut self,
        field: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_kanban_focus(window, cx);
        let current = self.native_kanban_filter_value(field);
        let menu = self.native_kanban_filter_choices(field).into_iter().fold(
            GpuiContextMenu::new(),
            |menu, (label, value)| {
                let checked = value == current;
                menu.menu_with_check(
                    label,
                    checked,
                    action(json!({ "type": "setFilter", "field": field, "value": value })),
                )
            },
        );
        // Hung from the Filters button like the Filters menu it replaced, so a click on Filters
        // while it is open closes it instead of reopening the Filters menu.
        menu.toggle_below(self.native_kanban.anchors.filters.get(), window, cx);
    }

    /// `View`: which details the cards show.
    pub(crate) fn show_native_kanban_card_view_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_kanban_focus(window, cx);
        let card_view = self.native_kanban.card_view;
        let menu = CARD_VIEW_FIELDS
            .iter()
            .fold(GpuiContextMenu::new(), |menu, (key, label)| {
                menu.menu_with_check(
                    *label,
                    card_view.field(key),
                    action(json!({ "type": "toggleCardField", "key": key })),
                )
            });
        menu.toggle_below(self.native_kanban.anchors.card_view.get(), window, cx);
    }

    /// A card's right-click menu: Start work (or open the linked session) and Delete.
    pub(crate) fn show_native_kanban_card_menu(
        &mut self,
        ticket_id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_kanban_focus(window, cx);
        let state = &self.native_kanban;
        let busy = state.busy_ticket.is_some();
        let link = state.conversation.primary_link_for(ticket_id);
        let (label, command) = match link {
            Some(link) if link.openable => ("Go to Session", "jumpToSession"),
            Some(_) => ("Resume Session", "jumpToSession"),
            None => ("Start work", "startWork"),
        };
        let start_disabled = busy || (link.is_none() && state.conversation.agents.is_empty());
        GpuiContextMenu::new()
            .menu_with_icon(
                label,
                "titlebar/player-play.svg",
                start_disabled,
                action(json!({ "type": command, "ticketId": ticket_id })),
            )
            .menu_with_icon(
                "Edit",
                "titlebar/pencil.svg",
                false,
                action(json!({ "type": "openTicket", "ticketId": ticket_id })),
            )
            .separator()
            .menu_with_icon(
                "Delete",
                "titlebar/trash.svg",
                false,
                action(json!({ "type": "deleteTicket", "ticketId": ticket_id })),
            )
            .show(position, window, cx);
    }
}
