//! The Actions page (packages/core-ui/settings-modal/tabs/actions.tsx (deleted 2026-10-01)): the "set frequently used
//! commands here" note while nothing is configured, Global Actions and Project Actions (each a
//! drag-to-reorder list with Terminal Action / Browser Action buttons, or an empty state). The Tab
//! Strip Buttons switches moved to General > Sidebar, because this page hides while the Actions
//! extension is off. Adding or editing an action replaces the lists with the one editor
//! (editor.rs), so a second edit can never start while a draft is open.
mod editor;
mod model;

use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, FieldStates, SearchableList, SettingsPage, card_inset, command_icon, icon,
    move_index, reorder_handle, reorder_order, reorder_row, reorder_scroll_container,
    settings_button, settings_icon, settings_icon_button, settings_section,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, post_store_message};
use editor::{ActionEditor, render_editor};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use model::{
    ActionType, CommandButton, CommandScope, commands_from_hud, ordered_commands,
    reconcile_draft_ids, reorder_request_id,
};
use serde_json::{Value, json};
use std::collections::HashMap;

const ICON_PENCIL: &str = "modals/settings/pencil.svg";

/// Creates the Actions page view.
pub(crate) fn actions_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| ActionsTab::new(store.clone(), window, cx))
        .into()
}

pub(crate) struct ActionsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The open editor (`editorState`).
    pub(crate) editor: Option<ActionEditor>,
    /// `draftCommandIds` per list: the order just posted, until the hydrate carries it.
    draft_order: HashMap<CommandScope, Vec<String>>,
    pub(crate) icon_picker: SearchableList,
    preview_applied: bool,
}

impl super::open_forms::RemembersOpenForm for ActionsTab {
    /// The Add or Edit action form.
    fn open_form(&self, cx: &App) -> Option<Value> {
        let project_id = self.store.read(cx).request().initial_project_id.clone();
        Some(self.editor.as_ref()?.to_form(project_id.as_deref()))
    }
}

impl ActionsTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let icon_picker = SearchableList::new("Search icons", window, cx);
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            editor: None,
            draft_order: HashMap::new(),
            icon_picker,
            preview_applied: false,
        };
        // The Add or Edit action form the last visit left open (settings_modal/restore.rs).
        if let Some(form) = page.store.update(cx, |store, _| {
            store.open_forms_mut().take(SettingsTabId::Actions)
        }) {
            page.editor = page.restorable_editor(&form, cx);
        }
        page
    }

    /// The remembered editor when it still fits: a project action only on its own project, and
    /// the edit of an action that still exists.
    fn restorable_editor(&self, form: &Value, cx: &App) -> Option<ActionEditor> {
        let editor = ActionEditor::from_form(form)?;
        let store = self.store.read(cx);
        if editor.scope == CommandScope::Project
            && form.get("projectId").and_then(Value::as_str)
                != store.request().initial_project_id.as_deref()
        {
            return None;
        }
        if let Some(command_id) = editor.command_id.as_deref() {
            let key = match editor.scope {
                CommandScope::Global => "globalCommands",
                CommandScope::Project => "commands",
            };
            if !commands_from_hud(store.hud(), key)
                .iter()
                .any(|command| command.command_id == command_id)
            {
                return None;
            }
        }
        Some(editor)
    }

    pub(crate) fn post(&self, message: Value, cx: &mut App) {
        post_store_message(&self.store, message, cx);
    }

    /// `deleteCommand(scope, commandId)`.
    pub(crate) fn delete_command(
        &mut self,
        scope: CommandScope,
        command_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.post(
            json!({ "commandId": command_id, "type": scope.delete_type() }),
            cx,
        );
        self.editor = None;
        cx.notify();
    }

    /// `reorderCommands(scope, nextCommandIds)`.
    fn reorder(&mut self, scope: CommandScope, next: Vec<String>, cx: &mut Context<Self>) {
        self.draft_order.insert(scope, next.clone());
        self.post(
            json!({
                "commandIds": next,
                "requestId": reorder_request_id(scope),
                "type": scope.order_type(),
            }),
            cx,
        );
        cx.notify();
    }

    /// The list in its draft order; a draft the hydrate caught up with is dropped
    /// (`reconcileDraftIds`).
    fn ordered(&mut self, scope: CommandScope, commands: &[CommandButton]) -> Vec<CommandButton> {
        let synced: Vec<String> = commands
            .iter()
            .map(|command| command.command_id.clone())
            .collect();
        let draft = self.draft_order.get(&scope).map(Vec::as_slice);
        match reconcile_draft_ids(draft, &synced) {
            Some(next) => {
                self.draft_order.insert(scope, next);
            }
            None => {
                self.draft_order.remove(&scope);
            }
        }
        ordered_commands(commands, self.draft_order.get(&scope).map(Vec::as_slice))
    }

    /// The preview binary's page states.
    fn apply_preview(
        &mut self,
        commands: &[CommandButton],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_applied {
            return;
        }
        self.preview_applied = true;
        let Some(state) = self.store.read(cx).request().preview_state.clone() else {
            return;
        };
        let first = commands.first().cloned();
        match state.as_str() {
            "actions-editor-new-terminal" => {
                self.editor = Some(ActionEditor::new_action(
                    CommandScope::Project,
                    ActionType::Terminal,
                ));
            }
            "actions-editor-new-browser" => {
                self.editor = Some(ActionEditor::new_action(
                    CommandScope::Global,
                    ActionType::Browser,
                ));
            }
            "actions-editor-edit"
            | "actions-editor-links"
            | "actions-icon-picker"
            | "actions-icon-search" => {
                if let Some(command) = first {
                    self.editor = Some(ActionEditor::edit(CommandScope::Project, &command));
                }
                if state.starts_with("actions-icon") {
                    self.icon_picker.open(false, window, cx);
                    if state == "actions-icon-search" {
                        self.icon_picker
                            .search
                            .update(cx, |input, cx| input.set_value("git", window, cx));
                    }
                }
            }
            "actions-duplicate" => {
                if let Some(command) = commands.get(1) {
                    let mut editor = ActionEditor::edit(CommandScope::Project, command);
                    editor.name = commands
                        .first()
                        .map(|command| command.name.clone())
                        .unwrap_or_default();
                    self.editor = Some(editor);
                }
            }
            _ => {}
        }
    }
}

impl SettingsPage for ActionsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// `bg-muted` behind a management row's glyph.
fn icon_tile_background(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        rgb(0xefefef)
    } else {
        rgb(0x2a2a2a)
    }
}

/// `SettingsActionIcon` in its 36px tile.
fn action_icon_tile(p: &SettingsPalette, icon_id: Option<&str>) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.92))
        .bg(hsla(icon_tile_background(p)))
        .child(command_icon(
            icon_id.unwrap_or(super::super::fields::DEFAULT_COMMAND_ICON),
            16.0,
            p.foreground,
        ))
        .into_any_element()
}

/// The ghost button look (`variant='ghost'`): transparent, the ghost hover fill.
fn ghost_hover(p: &SettingsPalette) -> gpui::Rgba {
    if p.light {
        rgb(0xf1f1f1)
    } else {
        css_fade(rgb(0x262626), 0.5)
    }
}

/// `Empty` inside a section card: a centered title and description.
fn empty_state(p: &SettingsPalette, title: &str, description: &str) -> AnyElement {
    card_inset(
        v_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .line_height(px(24.9))
                    .text_color(hsla(p.foreground))
                    .text_center()
                    .child(title.to_string()),
            )
            .child(
                div()
                    .max_w(px(640.0))
                    .text_size(px(13.0))
                    .line_height(px(21.1))
                    .text_color(hsla(p.muted))
                    .text_center()
                    .child(description.to_string()),
            ),
    )
}

impl ActionsTab {
    /// `SettingsCommandRow`: grip, the edit button (glyph tile, title, meta), and the edit and
    /// delete buttons revealed while the row is hovered.
    ///
    /// CDXC:Settings 2026-09-09 SEE-ALSO: the hover fill covers the whole management row, grip and buttons included (packages/core-ui/styles.css (deleted 2026-11-06)).
    fn command_row(
        &mut self,
        p: &SettingsPalette,
        scope: CommandScope,
        command: &CommandButton,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = scope.list_id();
        let group: SharedString = format!("{list}-row-{index}").into();
        let title = command.title();
        let grip_hover = ghost_hover(p);
        let grip = div()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .hover(move |this| this.bg(hsla(grip_hover)))
            .child(settings_icon(icon::GRIP_VERTICAL, 16.0, p.foreground))
            .into_any_element();
        let edit_command = command.clone();
        let edit_hover = ghost_hover(p);
        let edit_button = h_flex()
            .id(SharedString::from(format!("{list}-edit-{index}")))
            .flex_1()
            .min_w_0()
            .px(px(8.0))
            .py(px(8.0))
            .gap(px(12.0))
            .items_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(edit_hover)))
            .on_click(cx.listener(move |tab, _: &ClickEvent, _window, cx| {
                tab.editor = Some(ActionEditor::edit(scope, &edit_command));
                cx.notify();
            }))
            .child(action_icon_tile(p, command.icon.as_deref()))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(p.foreground))
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .line_height(px(18.57))
                            .text_color(hsla(p.muted))
                            .child(command.meta()),
                    ),
            );
        let pencil_command = command.clone();
        let delete_id = command.command_id.clone();
        let actions = h_flex()
            .flex_shrink_0()
            .gap(px(4.0))
            .opacity(0.0)
            .group_hover(group.clone(), |this| this.opacity(1.0))
            .child(settings_icon_button(
                p,
                SharedString::from(format!("{list}-pencil-{index}")),
                ICON_PENCIL,
                16.0,
                28.0,
                ButtonVariant::Ghost,
                None,
                false,
                move |tab: &mut Self, _window, cx| {
                    tab.editor = Some(ActionEditor::edit(scope, &pencil_command));
                    cx.notify();
                },
                cx,
            ))
            .child(settings_icon_button(
                p,
                SharedString::from(format!("{list}-delete-{index}")),
                icon::TRASH,
                16.0,
                28.0,
                ButtonVariant::Destructive,
                None,
                false,
                move |tab: &mut Self, _window, cx| {
                    tab.delete_command(scope, &delete_id, cx);
                },
                cx,
            ));
        let row_hover = p.raised_hover;
        h_flex()
            .id(SharedString::from(format!("{list}-row-{index}")))
            .group(group)
            .w_full()
            .min_h(px(56.0))
            .px(px(16.0))
            .py(px(6.0))
            .gap(px(10.0))
            .items_center()
            .hover(move |this| this.bg(hsla(row_hover)))
            .child(reorder_handle(p, list, index, title, grip))
            .child(edit_button)
            .child(actions)
            .into_any_element()
    }

    /// `ActionsSettingsSection`.
    #[allow(clippy::too_many_arguments)]
    fn actions_section(
        &mut self,
        p: &SettingsPalette,
        scope: CommandScope,
        commands: &[CommandButton],
        title: &'static str,
        description: &'static str,
        empty_title: &'static str,
        empty_description: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let list = scope.list_id();
        let add = |label: &'static str, action_type: ActionType, cx: &mut Context<Self>| {
            settings_button(
                p,
                SharedString::from(format!("{list}-add-{}", action_type.id())),
                label,
                Some(icon::PLUS),
                ButtonVariant::Outline,
                false,
                Some("Adding actions needs the Ghostex app connection.".into()),
                move |tab: &mut Self, _window, cx| {
                    tab.editor = Some(ActionEditor::new_action(scope, action_type));
                    cx.notify();
                },
                cx,
            )
        };
        let actions = h_flex()
            .items_center()
            .gap(px(8.0))
            .child(add("Terminal Action", ActionType::Terminal, cx))
            .child(add("Browser Action", ActionType::Browser, cx))
            .into_any_element();
        let rows: Vec<AnyElement> = if commands.is_empty() {
            vec![empty_state(p, empty_title, empty_description)]
        } else {
            let handle = self
                .store
                .update(cx, |store, _| store.scroll_handle(SettingsTabId::Actions));
            reorder_scroll_container(self, list, handle);
            let order = reorder_order(self, list, commands.len(), cx);
            let ids: Vec<String> = commands
                .iter()
                .map(|command| command.command_id.clone())
                .collect();
            order
                .iter()
                .enumerate()
                .map(|(slot, index)| {
                    let index = *index;
                    let row = self.command_row(p, scope, &commands[index], index, cx);
                    let ids = ids.clone();
                    reorder_row(
                        self,
                        list,
                        index,
                        slot,
                        row,
                        move |tab: &mut Self, from, to, _window, cx| {
                            tab.reorder(scope, move_index(&ids, from, to), cx);
                        },
                        cx,
                    )
                })
                .collect()
        };
        settings_section(p, title, Some(description.into()), Some(actions), rows)
            .map(IntoElement::into_any_element)
    }
}

/// The note above the lists while no action has a command or URL.
///
/// CDXC:Projects 2026-06-15-15:29 SEE-ALSO: with nothing configured, the page explains that frequent commands can be set here for one click or a hotkey (packages/core-ui/settings-modal/tabs/actions.tsx (deleted 2026-10-01)).
fn unconfigured_note(p: &SettingsPalette) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(12.0))
        .px(px(4.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(hsla(p.muted))
        .child(
            div()
                .flex_shrink_0()
                .mt(px(2.0))
                .child(settings_icon(icon::INFO_CIRCLE, 16.0, p.muted)),
        )
        .child(
            div().min_w_0().child(
                "Set frequently used terminal or browser commands here so you can run them with one click or a hotkey.",
            ),
        )
        .into_any_element()
}

impl Render for ActionsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, commands, global_commands, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Actions),
                commands_from_hud(store.hud(), "commands"),
                commands_from_hud(store.hud(), "globalCommands"),
                matching,
            )
        };
        self.apply_preview(&commands, window, cx);
        let ordered = self.ordered(CommandScope::Project, &commands);
        let ordered_global = self.ordered(CommandScope::Global, &global_commands);
        let mut blocks: Vec<PageBlock> = Vec::new();
        if let Some(scope) = self.editor.as_ref().map(|editor| editor.scope) {
            let existing = match scope {
                CommandScope::Global => ordered_global.clone(),
                CommandScope::Project => commands.clone(),
            };
            if let Some(section) = render_editor(self, &p, &existing, window, cx) {
                blocks.push(PageBlock::section("actionEditor", section));
            }
        } else if search.tab.is_searching && !search.has_matches() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Actions,
                &matching,
                move |tab, _window, cx| {
                    store.update(cx, |store, cx| store.set_active_tab(tab, cx));
                },
            )));
        } else {
            let configured = ordered_global
                .iter()
                .chain(ordered.iter())
                .any(CommandButton::is_configured);
            if !configured {
                blocks.push(PageBlock::plain(unconfigured_note(&p)));
            }
            if let Some(section) = self.actions_section(
                &p,
                CommandScope::Global,
                &ordered_global,
                "Global Actions",
                "Global actions apply to every project and are stored by the Ghostex daemon, so they follow you to every app that connects to it. They appear in the tab strip above your tabs.",
                "No global actions configured",
                "Add a terminal or browser action that should be available in every project.",
                cx,
            ) {
                blocks.push(PageBlock::section("globalActions", section));
            }
            // CDXC:AgentLauncher 2026-06-15-14:00 SEE-ALSO: the header copy explains quick command terminals, browser panes, worktree sharing and the right-click list.
            if let Some(section) = self.actions_section(
                &p,
                CommandScope::Project,
                &ordered,
                "Project Actions",
                "Actions are custom shortcuts for repeat work. Add terminal actions to run saved commands in quick command terminals, or browser actions to open saved URLs in browser panes. These actions are shared between a main project and its worktrees, and you can right-click the action button to show all configured actions for that project.",
                "No actions configured",
                "Add a terminal or browser action.",
                cx,
            ) {
                blocks.push(PageBlock::section("actions", section));
            }
        }
        settings_page(&self.store, SettingsTabId::Actions, &p, blocks, cx)
    }
}
