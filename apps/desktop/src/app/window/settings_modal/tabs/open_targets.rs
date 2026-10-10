//! The Open In page (packages/core-ui/settings-modal/tabs/open-targets.tsx (deleted 2026-10-01)): a switch per built-in
//! editor target (disabled while it is not installed) and the custom targets with their editor.
//! The targets come from `BUILT_IN_WORKSPACE_OPEN_TARGETS` in the generated catalog
//! (packages/shared/workspace-open-targets.ts (deleted 2026-10-01)); saves are whole-settings saves, as React made them.
use super::super::super::native_modal_kit::*;
use super::super::catalog::settings_catalog;
use super::super::fields::{
    ButtonVariant, FieldStates, SettingsPage, card_inset, settings_button, settings_icon,
    settings_icon_button, settings_list_item, settings_section, settings_text_input,
    switch_control,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::SettingsStore;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, Focusable as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, img, linear_color_stop,
    linear_gradient, px,
};
use gpui_component::input::{InputState, Textarea, TextareaState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

const MODULE: &str = super::super::catalog::module::OPEN_TARGETS;
const ICON_FOLDER_OPEN: &str = "modals/settings/folder-open.svg";
const ICON_CODE_DOTS: &str = "modals/settings/code-dots.svg";
const ICON_PENCIL: &str = "modals/settings/pencil.svg";
const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_PLUS: &str = "modals/settings/plus.svg";

/// One `BUILT_IN_WORKSPACE_OPEN_TARGETS` entry.
struct BuiltInTarget {
    id: String,
    label: String,
    commands: Option<Vec<String>>,
}

fn built_in_targets() -> Vec<BuiltInTarget> {
    settings_catalog()
        .module_value(MODULE, "BUILT_IN_WORKSPACE_OPEN_TARGETS")
        .and_then(Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|target| {
                    Some(BuiltInTarget {
                        id: target.get("id")?.as_str()?.to_string(),
                        label: target.get("label")?.as_str()?.to_string(),
                        commands: target
                            .get("commands")
                            .and_then(Value::as_array)
                            .map(|list| {
                                list.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            }),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX`.
fn custom_id_prefix() -> String {
    let prefix = settings_catalog().text(MODULE, "CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX");
    if prefix.is_empty() {
        "custom:".to_string()
    } else {
        prefix
    }
}

/// `createWorkspaceOpenTargetSlug`.
fn target_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for character in label.trim().to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if dash && !slug.is_empty() {
                slug.push('-');
            }
            dash = false;
            slug.push(character);
        } else {
            dash = true;
        }
    }
    if slug.is_empty() {
        "target".to_string()
    } else {
        slug
    }
}

/// `Date.now().toString(36)`.
fn now_base36() -> String {
    let mut value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(digits[(value % 36) as usize]);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// A custom target as stored (`CustomWorkspaceOpenTarget`).
#[derive(Clone)]
struct CustomTarget {
    id: String,
    label: String,
    command: String,
    args: Vec<String>,
}

impl CustomTarget {
    fn to_json(&self) -> Value {
        json!({
            "args": self.args,
            "command": self.command,
            "id": self.id,
            "label": self.label,
        })
    }
}

/// `normalizeCustomWorkspaceOpenTargets`: trimmed labels and commands, prefixed unique ids.
fn normalize_custom_targets(candidate: &Value) -> Vec<CustomTarget> {
    let Some(entries) = candidate.as_array() else {
        return Vec::new();
    };
    let prefix = custom_id_prefix();
    let mut seen: Vec<String> = Vec::new();
    let mut normalized = Vec::new();
    for entry in entries {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .map(|text| text.trim().to_string())
                .unwrap_or_default()
        };
        let (label, command) = (text("label"), text("command"));
        if label.is_empty() || command.is_empty() {
            continue;
        }
        let requested = text("id");
        let base = if requested.starts_with(&prefix) {
            requested
        } else {
            format!("{prefix}{}", target_slug(&label))
        };
        let mut id = base.clone();
        let mut suffix = 2;
        while seen.contains(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        seen.push(id.clone());
        normalized.push(CustomTarget {
            id,
            label,
            command,
            args: entry
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    normalized
}

/// The add / edit form (`SettingsOpenTargetEditorState`).
struct TargetEditor {
    id: Option<String>,
    label: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<TextareaState>,
}

pub(crate) fn open_targets_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let _ = window;
    cx.new(|cx| OpenTargetsTab::new(store.clone(), cx)).into()
}

pub(crate) struct OpenTargetsTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    editor: Option<TargetEditor>,
    /// The preview binary's `open-in-editor` state opens Add target on the first frame.
    preview_editor: bool,
}

impl OpenTargetsTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let preview_editor =
            store.read(cx).request().preview_state.as_deref() == Some("open-in-editor");
        if preview_editor {
            store.update(cx, |store, cx| {
                store.scroll_to_section(SettingsTabId::OpenTargets, "customOpenTargets", cx)
            });
        }
        Self {
            store,
            fields: FieldStates::default(),
            editor: None,
            preview_editor,
        }
    }

    /// `onChange({ ...settings, [key]: value })`: a whole-settings save (`applySettings`).
    fn save_whole(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let store = self.store.clone();
        store.update(cx, |store, cx| {
            let mut settings: Map<String, Value> = store.settings().clone();
            settings.insert(key.to_string(), value);
            store.apply_settings(settings, "settings:bulk", cx);
        });
    }

    /// `updateHiddenTarget`.
    ///
    /// CDXC:Titlebar 2026-05-11 WHY:
    /// Installed built-ins are toggleable and unavailable ones are disabled rows; turning an installed target off writes only hidden ids, so the startup scan can refresh availability without undoing that choice.
    fn set_target_visible(&mut self, target_id: &str, visible: bool, cx: &mut Context<Self>) {
        let known: Vec<String> = built_in_targets()
            .into_iter()
            .map(|target| target.id)
            .collect();
        let mut hidden: Vec<String> = Vec::new();
        if let Some(ids) = self
            .store
            .read(cx)
            .value("workspaceOpenTargetHiddenIds")
            .as_array()
        {
            for id in ids.iter().filter_map(Value::as_str) {
                if !hidden.iter().any(|existing| existing == id) {
                    hidden.push(id.to_string());
                }
            }
        }
        if visible {
            hidden.retain(|id| id != target_id);
        } else if !hidden.iter().any(|id| id == target_id) {
            hidden.push(target_id.to_string());
        }
        // `normalizeWorkspaceOpenTargetHiddenIds`: built-in ids only.
        hidden.retain(|id| known.contains(id));
        self.save_whole("workspaceOpenTargetHiddenIds", json!(hidden), cx);
    }

    fn open_editor(
        &mut self,
        target: Option<CustomTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, label, command, args) = match target {
            Some(target) => (
                Some(target.id),
                target.label,
                target.command,
                target.args.join("\n"),
            ),
            None => (None, String::new(), String::new(), String::new()),
        };
        let label = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Name")
                .default_value(label)
        });
        let command = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Command")
                .default_value(command)
        });
        let args = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 12)
                .placeholder("Optional arguments, one per line")
                .default_value(args)
        });
        self.editor = Some(TargetEditor {
            id,
            label,
            command,
            args,
        });
        cx.notify();
    }

    /// `saveCustomTarget`.
    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let label = editor.label.read(cx).value().trim().to_string();
        let command = editor.command.read(cx).value().trim().to_string();
        if label.is_empty() || command.is_empty() {
            return;
        }
        let args: Vec<String> = editor
            .args
            .read(cx)
            .value()
            .split('\n')
            .map(|arg| arg.trim().to_string())
            .filter(|arg| !arg.is_empty())
            .collect();
        let id = editor.id.clone().unwrap_or_else(|| {
            format!(
                "{}{}-{}",
                custom_id_prefix(),
                target_slug(&label),
                now_base36()
            )
        });
        let editing = editor.id.clone();
        let next = CustomTarget {
            id,
            label,
            command,
            args,
        };
        let existing =
            normalize_custom_targets(&self.store.read(cx).value("customWorkspaceOpenTargets"));
        let mut list: Vec<Value> = existing
            .iter()
            .filter(|target| Some(&target.id) != editing.as_ref())
            .map(CustomTarget::to_json)
            .collect();
        list.push(next.to_json());
        let normalized: Vec<Value> = normalize_custom_targets(&Value::Array(list))
            .iter()
            .map(CustomTarget::to_json)
            .collect();
        self.save_whole("customWorkspaceOpenTargets", Value::Array(normalized), cx);
        self.editor = None;
        cx.notify();
    }

    /// `removeCustomTarget`.
    fn remove_target(&mut self, target_id: &str, cx: &mut Context<Self>) {
        let remaining: Vec<Value> =
            normalize_custom_targets(&self.store.read(cx).value("customWorkspaceOpenTargets"))
                .iter()
                .filter(|target| target.id != target_id)
                .map(CustomTarget::to_json)
                .collect();
        self.save_whole("customWorkspaceOpenTargets", Value::Array(remaining), cx);
    }
}

impl super::HoldsUnsavedInput for OpenTargetsTab {
    /// The Add or Edit target form is open.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.editor.is_some()
    }
}

impl SettingsPage for OpenTargetsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// `OpenTargetSettingsIcon`: the folder for the file manager, the editor's brand logo, or the
/// code glyph for a target without one (a 20px square with the adaptive radius).
///
/// CDXC:Titlebar 2026-05-16 WHY:
/// Settings shows the same Open In editor icons as the titlebar dropdown so users can scan Cursor, VS Code variants, Zed, Antigravity, VSCodium, and JetBrains-family targets by brand; the logos are the canonical SVGL marks the React page used (packages/core-ui/assets/editor-icons).
fn open_target_icon(p: &super::super::palette::SettingsPalette, target_id: &str) -> AnyElement {
    let brand = match target_id {
        "antigravity" | "cursor" | "phpstorm" | "pycharm" | "rider" | "rubymine" | "vscodium"
        | "webstorm" | "zed" => Some(target_id),
        "idea" => Some("intellijidea"),
        "vscode" | "vscode-insiders" => Some("vscode"),
        "aqua" | "clion" | "datagrip" | "dataspell" | "goland" | "rustrover" => Some("jetbrains"),
        _ => None,
    };
    let frame = div()
        .relative()
        .flex_shrink_0()
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center();
    match (target_id, brand) {
        ("finder", _) => frame
            .child(settings_icon(ICON_FOLDER_OPEN, 20.0, p.muted))
            .into_any_element(),
        (_, Some(brand)) => frame
            .child(
                img(SharedString::from(format!(
                    "modals/settings/open-in/{brand}.svg"
                )))
                .size_full(),
            )
            .when(target_id == "vscode-insiders", |this| {
                // `.editor-brand-icon-insiders-badge`: a small green square over the corner.
                this.child(
                    div()
                        .absolute()
                        .right(px(-1.6))
                        .bottom(px(-1.6))
                        .size(px(8.0))
                        .border_1()
                        .border_color(hsla(modal_rgba(0x181818, 0.88)))
                        .bg(linear_gradient(
                            135.0,
                            linear_color_stop(hsla(gpui::rgb(0x16a34a)), 0.0),
                            linear_color_stop(hsla(gpui::rgb(0x7dd3fc)), 1.0),
                        )),
                )
            })
            .into_any_element(),
        _ => frame
            .child(settings_icon(ICON_CODE_DOTS, 20.0, p.muted))
            .into_any_element(),
    }
}

impl Render for OpenTargetsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.preview_editor) {
            self.open_editor(None, window, cx);
        }
        let (p, values, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.values(),
                store.tab_search(SettingsTabId::OpenTargets),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::OpenTargets,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        let hidden: Vec<String> = values
            .value("workspaceOpenTargetHiddenIds")
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let available: Vec<String> = values
            .value("workspaceOpenTargetAvailability")
            .get("availableTargetIds")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if search.section("openIn").has_visible() {
            let rows: Vec<AnyElement> = built_in_targets()
                .into_iter()
                .filter(|target| search.row_visible("openIn", &format!("builtin:{}", target.id)))
                .map(|target| {
                    let is_available = target.id == "finder" || available.contains(&target.id);
                    let detail = if !is_available {
                        "Not installed".to_string()
                    } else if target.id == "finder" {
                        "Built-in".to_string()
                    } else {
                        target
                            .commands
                            .as_ref()
                            .map(|commands| commands.join(", "))
                            .unwrap_or_else(|| "Installed app".to_string())
                    };
                    let target_id = target.id.clone();
                    let switch = switch_control(
                        &p,
                        SharedString::from(format!("open-target-{}", target.id)),
                        target.label.clone(),
                        is_available && !hidden.contains(&target.id),
                        !is_available,
                        Some(format!("Install {} to enable this option.", target.label).into()),
                        move |page: &mut Self, checked, _window, cx| {
                            page.set_target_visible(&target_id, checked, cx);
                        },
                        cx,
                    );
                    settings_list_item(
                        &p,
                        None,
                        Some(open_target_icon(&p, &target.id)),
                        target.label.clone(),
                        Some(div().child(detail).into_any_element()),
                        Some(switch),
                    )
                })
                .collect();
            blocks.extend(
                settings_section(&p, "Open In", None, None, rows)
                    .map(|section| PageBlock::section("openIn", section)),
            );
        }
        if search.section("customOpenTargets").has_visible() {
            let custom = normalize_custom_targets(&values.value("customWorkspaceOpenTargets"));
            let mut rows: Vec<AnyElement> = custom
                .iter()
                .map(|target| {
                    let detail = std::iter::once(target.command.clone())
                        .chain(target.args.iter().cloned())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let edit_target = target.clone();
                    let remove_id = target.id.clone();
                    let controls = h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(4.0))
                        .child(settings_icon_button(
                            &p,
                            SharedString::from(format!("open-target-edit-{}", target.id)),
                            ICON_PENCIL,
                            14.0,
                            24.0,
                            ButtonVariant::Ghost,
                            None,
                            false,
                            move |page: &mut Self, window, cx| {
                                page.open_editor(Some(edit_target.clone()), window, cx);
                            },
                            cx,
                        ))
                        .child(settings_icon_button(
                            &p,
                            SharedString::from(format!("open-target-remove-{}", target.id)),
                            ICON_TRASH,
                            14.0,
                            24.0,
                            ButtonVariant::Ghost,
                            None,
                            false,
                            move |page: &mut Self, _window, cx| page.remove_target(&remove_id, cx),
                            cx,
                        ))
                        .into_any_element();
                    settings_list_item(
                        &p,
                        None,
                        None,
                        target.label.clone(),
                        Some(div().child(detail).into_any_element()),
                        Some(controls),
                    )
                })
                .collect();
            match self.editor.as_ref() {
                Some(editor) => {
                    let (label, command, args) = (
                        editor.label.clone(),
                        editor.command.clone(),
                        editor.args.clone(),
                    );
                    let args_focused = args.read(cx).focus_handle(cx).is_focused(window);
                    let form = v_flex()
                        .w_full()
                        .gap(px(12.0))
                        .child(settings_text_input(&p, &label, None, false, window, cx))
                        .child(settings_text_input(&p, &command, None, false, window, cx))
                        .child(
                            div()
                                .w_full()
                                .min_h(px(64.0))
                                .px(px(12.0))
                                .py(px(12.0))
                                .rounded(px(MODAL_RADIUS_CONTROL))
                                .border_1()
                                .border_color(hsla(if args_focused {
                                    p.focus_border
                                } else {
                                    p.hairline
                                }))
                                .bg(hsla(p.input_background()))
                                .child(
                                    Textarea::new(&args)
                                        .with_size(ComponentSize::Small)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .px(px(0.0))
                                        .py(px(0.0))
                                        .text_size(px(14.0))
                                        .text_color(hsla(p.foreground)),
                                ),
                        )
                        .child(
                            h_flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(settings_button(
                                    &p,
                                    "open-target-cancel",
                                    "Cancel",
                                    None,
                                    ButtonVariant::Ghost,
                                    false,
                                    None,
                                    |page: &mut Self, _window, cx| {
                                        page.editor = None;
                                        cx.notify();
                                    },
                                    cx,
                                ))
                                .child(settings_button(
                                    &p,
                                    "open-target-save",
                                    "Save",
                                    None,
                                    ButtonVariant::Default,
                                    false,
                                    None,
                                    |page: &mut Self, _window, cx| page.save_editor(cx),
                                    cx,
                                )),
                        );
                    rows.push(card_inset(form));
                }
                None => {
                    rows.push(card_inset(div().child(settings_button(
                        &p,
                        "open-target-add",
                        "Add target",
                        Some(ICON_PLUS),
                        ButtonVariant::Outline,
                        false,
                        None,
                        |page: &mut Self, window, cx| page.open_editor(None, window, cx),
                        cx,
                    ))));
                }
            }
            blocks.extend(
                settings_section(&p, "Custom Open Targets", None, None, rows)
                    .map(|section| PageBlock::section("customOpenTargets", section)),
            );
        }
        settings_page(&self.store, SettingsTabId::OpenTargets, &p, blocks, cx)
    }
}
