//! Settings > Boosts: Boosts on or off, every Boost's prompt (edit and reset), and the user's own
//! Boosts (new, edit, move, delete). Everything here is gxserver's (`/api/boosts`,
//! server/src/boosts.rs), read when the page opens and after each change. The chat's Boost menu,
//! a Boost pill's double click and Save as Boost open this page (`initialSection: "boost:<id>"`
//! opens that Boost's editor, `"newBoost"` with `initialBoostText` a new Boost holding the text).
//!
//! CDXC:Boosts 2026-11-20 DECISION:
//! User, with the menu and hint answers: "a new Settings page for Boosts, where editing happens
//! (every Boost's prompt, reset, custom Boosts: add/edit/delete/reorder, maybe turn Boosts
//! on/off); the menu's edit actions open it". Edits are made in a sheet over the page, as the
//! coordinator playbook's are.
//! SEE-ALSO: packages/gx-chat-core/src/extras/boosts.rs (the Boosts and the user's decisions),
//! apps/desktop/src/app/native_chat/boosts.rs (what opens this page).

use super::super::fields::notice::{is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SettingsPage, setting_row, settings_button,
    settings_dialog_sheet, settings_section, settings_text_input, settings_textarea, static_note,
    toggle_field_with,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, store_gxserver_rpc};
use crate::app::window::native_modal_kit::hsla;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const ENDPOINT: &str = "/api/boosts";
const LABEL_FIELD: &str = "boost-sheet-label";
const PROMPT_FIELD: &str = "boost-sheet-prompt";

pub(crate) fn boosts_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| BoostsTab::new(store.clone(), cx)).into()
}

/// The open editor: one of Ghostex's Boosts (`builtin`), one of the user's, or a new one (`id`
/// `None`).
struct BoostSheet {
    focus: FocusHandle,
    id: Option<String>,
    builtin: bool,
    title: String,
    label: String,
    icon: String,
    prompt: String,
    default_prompt: String,
    edited: bool,
    saving: bool,
    error: Option<String>,
    /// Delete was pressed once and waits for its confirming press.
    confirm_delete: bool,
}

pub(crate) struct BoostsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The last `/api/boosts` answer.
    view: Option<Value>,
    error: Option<String>,
    sheet: Option<BoostSheet>,
    was_active: bool,
}

impl SettingsPage for BoostsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// A static icon path for one the core or gxserver names (the row and sheet icons take one).
fn static_icon(path: &str) -> &'static str {
    ghostex_gx_chat_core::extras::boosts::BOOSTS
        .iter()
        .map(|boost| boost.icon)
        .chain(ghostex_gx_chat_core::extras::boosts::CUSTOM_ICONS)
        .find(|icon| *icon == path)
        .unwrap_or(ghostex_gx_chat_core::extras::boosts::CUSTOM_ICONS[0])
}

impl BoostsTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Boosts;
            if active && !page.was_active {
                page.call(json!({ "op": "list" }), cx);
            }
            page.was_active = active;
            cx.notify();
        })
        .detach();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            view: None,
            error: None,
            sheet: None,
            was_active: true,
        };
        page.call(json!({ "op": "list" }), cx);
        page
    }

    /// One `/api/boosts` call; its answer is the page's new state. A change made from the sheet
    /// closes it once saved, or shows why it could not be.
    fn call(&mut self, params: Value, cx: &mut Context<Self>) {
        if !self.store.read(cx).request().gxserver_rpc_available {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        let from_sheet = params["op"] != "list" && self.sheet.is_some();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            ENDPOINT,
            params,
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => {
                            page.view = Some(answer);
                            page.error = None;
                            if from_sheet {
                                page.sheet = None;
                            }
                        }
                        Err(error) => match page.sheet.as_mut().filter(|_| from_sheet) {
                            Some(sheet) => {
                                sheet.saving = false;
                                sheet.error = Some(error);
                            }
                            None => page.error = Some(error),
                        },
                    }
                    page.open_deep_link(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// A deep link from the chat (`initialSection`): a Boost's editor, or a new Boost.
    fn open_deep_link(&mut self, cx: &mut Context<Self>) {
        if self.view.is_none() {
            return;
        }
        let (section, text) = self.store.update(cx, |store, _| {
            let request = store.request_mut();
            let section = request
                .initial_section
                .take_if(|section| section == "newBoost" || section.starts_with("boost:"));
            let text = section
                .is_some()
                .then(|| request.initial_boost_text.take())
                .flatten();
            (section, text)
        });
        match section.as_deref() {
            Some("newBoost") => self.open_new(text.unwrap_or_default(), cx),
            Some(link) => {
                if let Some(id) = link.strip_prefix("boost:") {
                    self.open_editor(id, cx);
                }
            }
            None => {}
        }
    }

    fn rows(&self, key: &str) -> Vec<Value> {
        self.view
            .as_ref()
            .and_then(|view| view[key].as_array().cloned())
            .unwrap_or_default()
    }

    fn open_new(&mut self, prompt: String, cx: &mut Context<Self>) {
        let label = prompt
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join(" ");
        self.sheet = Some(BoostSheet {
            focus: cx.focus_handle(),
            id: None,
            builtin: false,
            title: "New Boost".to_string(),
            label,
            icon: ghostex_gx_chat_core::extras::boosts::CUSTOM_ICONS[0].to_string(),
            prompt: prompt.trim().to_string(),
            default_prompt: String::new(),
            edited: false,
            saving: false,
            error: None,
            confirm_delete: false,
        });
        cx.notify();
    }

    fn open_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.rows("boosts").into_iter().find(|row| row["id"] == id) {
            self.sheet = Some(BoostSheet {
                focus: cx.focus_handle(),
                id: Some(id.to_string()),
                builtin: true,
                title: format!("{} Boost", text(&row, "label")),
                label: text(&row, "label"),
                icon: text(&row, "iconPath"),
                prompt: text(&row, "prompt"),
                default_prompt: text(&row, "default"),
                edited: row["edited"] == true,
                saving: false,
                error: None,
                confirm_delete: false,
            });
        } else if let Some(row) = self.rows("custom").into_iter().find(|row| row["id"] == id) {
            self.sheet = Some(BoostSheet {
                focus: cx.focus_handle(),
                id: Some(id.to_string()),
                builtin: false,
                title: "Your Boost".to_string(),
                label: text(&row, "label"),
                icon: text(&row, "icon"),
                prompt: text(&row, "prompt"),
                default_prompt: String::new(),
                edited: false,
                saving: false,
                error: None,
                confirm_delete: false,
            });
        }
        cx.notify();
    }

    fn save_sheet(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        sheet.saving = true;
        sheet.error = None;
        let params = match (&sheet.id, sheet.builtin) {
            (Some(id), true) => json!({ "op": "set", "id": id, "prompt": sheet.prompt }),
            (Some(id), false) => json!({
                "op": "updateCustom", "id": id, "label": sheet.label,
                "icon": sheet.icon, "prompt": sheet.prompt,
            }),
            (None, _) => json!({
                "op": "addCustom", "label": sheet.label, "icon": sheet.icon, "prompt": sheet.prompt,
            }),
        };
        self.call(params, cx);
    }

    fn switch_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let on = self.view.as_ref().is_some_and(|view| view["on"] != false);
        settings_section(
            p,
            "",
            None,
            None,
            vec![toggle_field_with(
                p,
                "boostsOn",
                RowSpec::new("Boosts").description(
                    "The Boost pill in the chat box, and * at the start of a word, add a Boost to your message: Ghostex puts its prompt under that one message for the agent. Off hides them in every chat on this computer.",
                ),
                on,
                None,
                |page: &mut Self, next, _window, cx| {
                    if let Some(view) = page.view.as_mut() {
                        view["on"] = json!(next);
                    }
                    page.call(json!({ "op": "setOn", "on": next }), cx);
                },
                cx,
            )],
        )
    }

    fn custom_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let custom = self.rows("custom");
        let count = custom.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, row) in custom.iter().enumerate() {
            let id = text(row, "id");
            let first_line: String = text(row, "prompt")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let edit_id = id.clone();
            let up_id = id.clone();
            let down_id = id.clone();
            let controls = h_flex()
                .gap(px(6.0))
                .when(index > 0, |row| {
                    row.child(settings_button(
                        p,
                        SharedString::from(format!("boost-up-{id}")),
                        "Up",
                        Some("modals/settings/chevron-up.svg"),
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.call(
                                json!({ "op": "moveCustom", "id": up_id, "to": index - 1 }),
                                cx,
                            )
                        },
                        cx,
                    ))
                })
                .when(index + 1 < count, |row| {
                    row.child(settings_button(
                        p,
                        SharedString::from(format!("boost-down-{id}")),
                        "Down",
                        Some("modals/settings/chevron-down.svg"),
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.call(
                                json!({ "op": "moveCustom", "id": down_id, "to": index + 1 }),
                                cx,
                            )
                        },
                        cx,
                    ))
                })
                .child(settings_button(
                    p,
                    SharedString::from(format!("boost-edit-{id}")),
                    "Edit",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| page.open_editor(&edit_id, cx),
                    cx,
                ))
                .into_any_element();
            rows.push(setting_row(
                p,
                SharedString::from(format!("boost-custom-{id}")),
                RowSpec::new(text(row, "label"))
                    .description(first_line)
                    .icon(static_icon(&text(row, "icon"))),
                None,
                controls,
                cx,
            ));
        }
        if rows.is_empty() {
            rows.push(
                div()
                    .px(px(16.0))
                    .py(px(14.0))
                    .child(static_note(
                        p,
                        "None yet. Select words in a message or the chat box and choose Save as Boost, or press New Boost.",
                        false,
                    ))
                    .into_any_element(),
            );
        }
        let new_button = settings_button(
            p,
            "boost-new",
            "New Boost",
            Some("modals/settings/plus.svg"),
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.open_new(String::new(), cx),
            cx,
        );
        settings_section(
            p,
            "Your Boosts",
            Some("They show in the Boost menu's Custom tab, in this order.".into()),
            Some(new_button),
            rows,
        )
    }

    fn category_section(
        &mut self,
        p: &SettingsPalette,
        category: &str,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let mut rows: Vec<AnyElement> = Vec::new();
        for row in self
            .rows("boosts")
            .into_iter()
            .filter(|row| row["category"] == category)
        {
            let id = text(&row, "id");
            let edited = row["edited"] == true;
            let edit_id = id.clone();
            let reset_id = id.clone();
            let controls = h_flex()
                .gap(px(6.0))
                .when(edited, |controls| {
                    controls.child(settings_button(
                        p,
                        SharedString::from(format!("boost-reset-{id}")),
                        "Reset",
                        None,
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.call(json!({ "op": "reset", "id": reset_id }), cx)
                        },
                        cx,
                    ))
                })
                .child(settings_button(
                    p,
                    SharedString::from(format!("boost-edit-{id}")),
                    "Edit prompt",
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| page.open_editor(&edit_id, cx),
                    cx,
                ))
                .into_any_element();
            let mut spec = RowSpec::new(text(&row, "label"))
                .description(text(&row, "description"))
                .icon(static_icon(&text(&row, "iconPath")));
            if edited {
                spec = spec.readout("Edited prompt");
            }
            rows.push(setting_row(
                p,
                SharedString::from(format!("boost-{id}")),
                spec,
                None,
                controls,
                cx,
            ));
        }
        settings_section(p, title.to_string(), None, None, rows)
    }

    /// The editor sheet, while it is open.
    fn render_sheet(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = &p.nested_dialog();
        let sheet = self.sheet.as_ref()?;
        let (builtin, saving, title, focus) = (
            sheet.builtin,
            sheet.saving,
            sheet.title.clone(),
            sheet.focus.clone(),
        );
        let (label, prompt, icon) = (
            sheet.label.clone(),
            sheet.prompt.clone(),
            sheet.icon.clone(),
        );
        let (edited, default_prompt, error, confirm_delete, has_id) = (
            sheet.edited,
            sheet.default_prompt.clone(),
            sheet.error.clone(),
            sheet.confirm_delete,
            sheet.id.is_some(),
        );
        let label_state = FieldStates::text_state(
            self,
            &SharedString::from(LABEL_FIELD),
            &label,
            Some("Its name in the menu"),
            |page: &mut Self, value, _window, _cx| {
                if let Some(sheet) = page.sheet.as_mut() {
                    sheet.label = value;
                }
            },
            window,
            cx,
        );
        let prompt_state = FieldStates::textarea_state(
            self,
            &SharedString::from(PROMPT_FIELD),
            &prompt,
            Some("What the agent is told when this Boost is in a message"),
            (5, 14),
            |page: &mut Self, value, _window, cx| {
                if let Some(sheet) = page.sheet.as_mut() {
                    sheet.prompt = value;
                }
                cx.notify();
            },
            window,
            cx,
        );
        let label_text = |text: &str| {
            div()
                .text_size(px(12.5))
                .text_color(hsla(p.muted))
                .child(text.to_string())
                .into_any_element()
        };
        let mut body: Vec<AnyElement> = Vec::new();
        if !builtin {
            body.push(label_text("Name"));
            body.push(settings_text_input(
                p,
                &label_state,
                None,
                false,
                window,
                cx,
            ));
            body.push(label_text("Icon"));
            let icons = h_flex().gap(px(6.0)).children(
                ghostex_gx_chat_core::extras::boosts::CUSTOM_ICONS
                    .iter()
                    .map(|path| {
                        let chosen = *path == icon;
                        let path = *path;
                        div()
                            .id(SharedString::from(format!("boost-icon-{path}")))
                            .size(px(30.0))
                            .rounded(px(7.0))
                            .border_1()
                            .border_color(hsla(if chosen { p.focus_border } else { p.hairline }))
                            .when(chosen, |tile| tile.bg(hsla(p.foreground_alpha(0.06))))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(svg().path(path).size(px(15.0)).text_color(hsla(if chosen {
                                p.foreground
                            } else {
                                p.muted
                            })))
                            .on_click(cx.listener(move |page: &mut Self, _, _, cx| {
                                if let Some(sheet) = page.sheet.as_mut() {
                                    sheet.icon = path.to_string();
                                }
                                cx.notify();
                            }))
                    }),
            );
            body.push(icons.into_any_element());
            body.push(label_text("What the agent is told"));
        }
        body.push(settings_textarea(
            p,
            &prompt_state,
            140.0,
            false,
            saving,
            window,
            cx,
        ));
        body.push(label_text(if builtin {
            "Saved on this computer for every chat and every device that uses it. Reset goes back to Ghostex's prompt."
        } else {
            "Saved on this computer for every chat and every device that uses it."
        }));
        if let Some(error) = error {
            body.push(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(p.destructive))
                    .child(error)
                    .into_any_element(),
            );
        }
        let mut footer = h_flex().w_full().gap(px(8.0));
        if builtin && edited {
            footer = footer.child(settings_button(
                p,
                "boost-sheet-reset",
                "Reset to default",
                None,
                ButtonVariant::Outline,
                saving,
                None,
                move |page: &mut Self, window, cx| {
                    let default_prompt = default_prompt.clone();
                    if let Some(sheet) = page.sheet.as_mut() {
                        sheet.prompt = default_prompt.clone();
                        sheet.saving = true;
                        let id = sheet.id.clone().unwrap_or_default();
                        let _ = window;
                        page.call(json!({ "op": "reset", "id": id }), cx);
                    }
                },
                cx,
            ));
        }
        if !builtin && has_id {
            footer = footer.child(settings_button(
                p,
                "boost-sheet-delete",
                if confirm_delete {
                    "Delete it"
                } else {
                    "Delete"
                },
                None,
                if confirm_delete {
                    ButtonVariant::DestructiveDialog
                } else {
                    ButtonVariant::Outline
                },
                saving,
                None,
                |page: &mut Self, _window, cx| {
                    let Some(sheet) = page.sheet.as_mut() else {
                        return;
                    };
                    if !sheet.confirm_delete {
                        sheet.confirm_delete = true;
                        cx.notify();
                        return;
                    }
                    sheet.saving = true;
                    let id = sheet.id.clone().unwrap_or_default();
                    page.call(json!({ "op": "deleteCustom", "id": id }), cx);
                },
                cx,
            ));
        }
        let cancel = settings_button(
            p,
            "boost-sheet-cancel",
            "Cancel",
            None,
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.sheet = None;
                cx.notify();
            },
            cx,
        );
        let save = settings_button(
            p,
            "boost-sheet-save",
            if saving { "Saving\u{2026}" } else { "Save" },
            None,
            ButtonVariant::Primary,
            saving || prompt.trim().is_empty() || (!builtin && label.trim().is_empty()),
            None,
            |page: &mut Self, _window, cx| page.save_sheet(cx),
            cx,
        );
        let footer = footer
            .child(div().flex_1())
            .child(cancel)
            .child(save)
            .into_any_element();
        let max_height = f32::from(window.viewport_size().height) * 0.92;
        Some(settings_dialog_sheet(
            p,
            "boost-sheet",
            &focus,
            title,
            Some(
                div()
                    .child(if builtin {
                        "What the agent is told when this Boost is in a message."
                    } else {
                        "Your Boost shows in the Boost menu's Custom tab and in the * picker."
                    })
                    .into_any_element(),
            ),
            560.0,
            Some(max_height),
            24.0,
            12.0,
            false,
            body,
            Some(footer),
            |page: &mut Self, _window, cx| {
                page.sheet = None;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}

impl Render for BoostsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.store.read(cx).palette();
        let mut blocks: Vec<PageBlock> = Vec::new();
        if self.error.as_deref().is_some_and(is_service_unreachable) {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "boosts-unreachable",
                |page: &mut Self, _window, cx| page.call(json!({ "op": "list" }), cx),
                cx,
            )));
        } else if let Some(error) = self.error.clone() {
            blocks.push(PageBlock::plain(static_note(&p, error, true)));
        }
        if self.view.is_none() {
            if self.error.is_none() {
                blocks.push(PageBlock::plain(static_note(
                    &p,
                    "Reading Boosts\u{2026}",
                    false,
                )));
            }
        } else {
            if let Some(section) = self.switch_section(&p, cx) {
                blocks.push(PageBlock::section("boostsOn", section));
            }
            let on = self.view.as_ref().is_some_and(|view| view["on"] != false);
            // CDXC:Settings 2026-11-14 DECISION (AGENTS.md, Q3107): while Boosts are off the page
            // is that one switch.
            if on {
                if let Some(section) = self.custom_section(&p, cx) {
                    blocks.push(PageBlock::section("customBoosts", section));
                }
                for (category, title) in [("show", "Show"), ("do", "Actions"), ("work", "Work")] {
                    if let Some(section) = self.category_section(&p, category, title, cx) {
                        blocks.push(PageBlock::section(category, section));
                    }
                }
            }
        }
        let sheet = self.render_sheet(&p, window, cx);
        v_flex()
            .size_full()
            .child(settings_page(
                &self.store,
                SettingsTabId::Boosts,
                &p,
                blocks,
                cx,
            ))
            .children(sheet)
    }
}
