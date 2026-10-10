//! `SidebarTagListSettingsField`: the collapsed "Tag filter list" disclosure with Add tag and
//! Reset to Default, the drag-to-reorder rows of tags and separators (switch, visibility, and a
//! trash button on custom tags), and the New tag form (`CustomSessionTagEditorForm` with its
//! `CommandIconPicker`).
//!
//! The filter list is a setting (`sidebarSessionTagListItems`); the custom tag catalog is the
//! local daemon's (`customSessionTags` of the hydrate) and every create, delete or reorder is
//! written straight through (`updateCustomSessionTags`), never through the settings draft
//! (CDXC:Sessions 2026-09-11 WHY in packages/core-ui/settings-modal.tsx (deleted 2026-10-01)).
use super::super::super::native_modal_kit::*;
use super::super::super::space_editor_modal::{SPACE_EDITOR_ICONS, command_score};
use super::super::catalog::{module, settings_catalog};
use super::super::palette::{SETTINGS_FONT, SettingsPalette};
use super::controls::{ButtonVariant, settings_button, settings_icon_button, switch_control};
use super::row::{PageAction, settings_icon, tooltip_text};
use super::text::settings_text_input;
use super::{FieldStates, SettingsPage, icon};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, Pixels,
    Render, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _, Window, anchored,
    deferred, div, point, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::rc::Rc;

const KEY: &str = "sidebarSessionTagListItems";
const MAX_TAG_NAME: usize = 40;
const DEFAULT_TAG_ICON: &str = "sparkles";
/// The drag-to-reorder list id of the tag rows (fields/reorder.rs).
const TAG_REORDER_LIST: &str = "sidebar-tags";

/// One row of the filter list (`SidebarSessionTagListItem`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TagListItem {
    pub(crate) id: String,
    /// `tag`, `separator` or `untagged`.
    pub(crate) kind: String,
    pub(crate) enabled: bool,
    pub(crate) visible: bool,
}

/// One custom tag of the daemon catalog.
#[derive(Clone, Debug)]
pub(crate) struct CustomTag {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) color: String,
}

pub(crate) struct TagListState {
    pub(crate) expanded: bool,
    pub(crate) creating: bool,
    name_input: Entity<InputState>,
    icon: String,
    color: String,
    picker_open: bool,
    picker_search: Entity<InputState>,
    picker_highlight: Option<usize>,
    picker_trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: Option<usize>,
}

fn is_custom_tag_id(value: &str) -> bool {
    value.strip_prefix("custom-").is_some_and(|rest| {
        (4..=40).contains(&rest.len())
            && rest
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

fn builtin_tags() -> Vec<String> {
    settings_catalog().string_list(module::SESSION_TAGS, "SIDEBAR_SESSION_TAGS", None)
}

fn tag_color_presets() -> Vec<(String, String)> {
    settings_catalog()
        .module_value(module::SESSION_TAGS, "SESSION_TAG_COLOR_PRESETS")
        .and_then(Value::as_array)
        .map(|presets| {
            presets
                .iter()
                .filter_map(|preset| {
                    Some((
                        preset.get("label")?.as_str()?.to_string(),
                        preset.get("value")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The custom tags of a `customSessionTags` document, in catalog order.
pub(crate) fn custom_tags(state: Option<&Value>) -> Vec<CustomTag> {
    let Some(state) = state else {
        return Vec::new();
    };
    let tags = state.get("tags").and_then(Value::as_object);
    let order: Vec<&str> = state
        .get("order")
        .and_then(Value::as_array)
        .map(|order| order.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut ids: Vec<String> = order.iter().map(|id| id.to_string()).collect();
    if let Some(tags) = tags {
        for id in tags.keys() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    ids.into_iter()
        .filter(|id| is_custom_tag_id(id))
        .filter_map(|id| {
            let tag = tags?.get(&id)?;
            Some(CustomTag {
                name: tag
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Tag")
                    .to_string(),
                icon: tag
                    .get("icon")
                    .and_then(Value::as_str)
                    .filter(|icon| !icon.trim().is_empty())
                    .unwrap_or(DEFAULT_TAG_ICON)
                    .to_string(),
                color: tag
                    .get("color")
                    .and_then(Value::as_str)
                    .unwrap_or("#8e949d")
                    .to_string(),
                id,
            })
        })
        .collect()
}

fn default_items() -> Vec<TagListItem> {
    settings_catalog()
        .module_value(
            module::SESSION_TAGS,
            "DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS",
        )
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_item).collect())
        .unwrap_or_default()
}

fn parse_item(value: &Value) -> Option<TagListItem> {
    let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(true);
    let tag = value.get("tag").and_then(Value::as_str).unwrap_or(id);
    let builtin = builtin_tags();
    if builtin.iter().any(|known| known == tag) || is_custom_tag_id(tag) {
        return Some(TagListItem {
            id: tag.to_string(),
            kind: "tag".to_string(),
            enabled: flag("enabled"),
            visible: flag("visible"),
        });
    }
    if id == "untagged" || value.get("type").and_then(Value::as_str) == Some("untagged") {
        return Some(TagListItem {
            id: "untagged".to_string(),
            kind: "untagged".to_string(),
            enabled: flag("enabled"),
            visible: flag("visible"),
        });
    }
    let separators = settings_catalog().string_list(
        module::SESSION_TAGS,
        "SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS",
        None,
    );
    separators
        .iter()
        .any(|separator| separator == id)
        .then(|| TagListItem {
            id: id.to_string(),
            kind: "separator".to_string(),
            enabled: flag("enabled"),
            visible: flag("visible"),
        })
}

/// `normalizeSidebarSessionTagListItems(items, customTags)`.
pub(crate) fn normalize_tag_list_items(
    value: &Value,
    custom: Option<&[CustomTag]>,
) -> Vec<TagListItem> {
    let mut items: Vec<TagListItem> = Vec::new();
    for entry in value.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(item) = parse_item(entry) else {
            continue;
        };
        if items.iter().any(|existing| existing.id == item.id) {
            continue;
        }
        if let Some(custom) = custom
            && item.kind == "tag"
            && is_custom_tag_id(&item.id)
            && !custom.iter().any(|tag| tag.id == item.id)
        {
            continue;
        }
        items.push(item);
    }
    for item in default_items() {
        if !items.iter().any(|existing| existing.id == item.id) {
            items.push(item);
        }
    }
    if let Some(custom) = custom {
        let missing: Vec<TagListItem> = custom
            .iter()
            .filter(|tag| !items.iter().any(|item| item.id == tag.id))
            .map(|tag| TagListItem {
                id: tag.id.clone(),
                kind: "tag".to_string(),
                enabled: true,
                visible: true,
            })
            .collect();
        if !missing.is_empty() {
            let insert_at = items
                .iter()
                .position(|item| item.id == "separator-type-untagged")
                .or_else(|| items.iter().position(|item| item.kind == "untagged"))
                .unwrap_or(items.len());
            for (offset, item) in missing.into_iter().enumerate() {
                items.insert(insert_at + offset, item);
            }
        }
    }
    items
}

fn items_json(items: &[TagListItem]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                let mut entry = Map::new();
                entry.insert("enabled".into(), json!(item.enabled));
                entry.insert("id".into(), json!(item.id));
                if item.kind == "tag" {
                    entry.insert("tag".into(), json!(item.id));
                }
                entry.insert("type".into(), json!(item.kind));
                entry.insert("visible".into(), json!(item.visible));
                Value::Object(entry)
            })
            .collect(),
    )
}

/// `areSidebarSessionTagListItemsEqual(items, DEFAULT)`.
pub(crate) fn tag_list_is_default(items: &[TagListItem]) -> bool {
    items == default_items().as_slice()
}

fn builtin_label(tag: &str) -> Option<String> {
    settings_catalog()
        .module_value(module::SESSION_TAGS, "SIDEBAR_SESSION_TAG_OPTIONS")
        .and_then(Value::as_array)
        .and_then(|options| {
            options
                .iter()
                .find(|option| option.get("value").and_then(Value::as_str) == Some(tag))
        })
        .and_then(|option| option.get("label"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `getSidebarSessionTagListItemLabel`.
fn item_label(item: &TagListItem, custom: &[CustomTag]) -> String {
    match item.kind.as_str() {
        "untagged" => "No tag".to_string(),
        "separator" => "Separator".to_string(),
        _ if is_custom_tag_id(&item.id) => custom
            .iter()
            .find(|tag| tag.id == item.id)
            .map(|tag| tag.name.clone())
            .unwrap_or_else(|| "Custom tag".to_string()),
        _ => builtin_label(&item.id).unwrap_or_else(|| item.id.clone()),
    }
}

/// The glyph and colour of a built-in tag (packages/core-ui/session-tag-ui.tsx (deleted 2026-10-01), tag-presentation).
fn builtin_presentation(tag: &str) -> (&'static str, u32) {
    match tag {
        "favorite" => ("modals/settings/star-filled.svg", 0xf3cd5f),
        "high-priority" => ("modals/settings/alert-triangle.svg", 0xff8b6b),
        "low-priority" => ("modals/settings/arrow-down.svg", 0x8e949d),
        "research" => ("modals/settings/microscope.svg", 0x8fb8ff),
        "todo" => ("modals/settings/checkbox.svg", 0xd9dee6),
        "in-progress" => ("modals/settings/player-play.svg", 0x4ee6b8),
        "testing" => ("modals/settings/test-pipe.svg", 0x59d9ff),
        "blocked" => ("modals/settings/barrier-block.svg", 0xff5f73),
        "on-hold" => ("modals/settings/player-pause.svg", 0xd2a7ff),
        "done" => ("modals/settings/circle-check.svg", 0x95d7f6),
        "bug" => ("modals/settings/bug.svg", 0xa54646),
        "feature" => ("modals/settings/puzzle.svg", 0xf0c66e),
        "design" => ("modals/settings/palette.svg", 0xff9ee7),
        _ => ("modals/settings/tag-off.svg", 0xacb6c0),
    }
}

/// The outline glyph of a sidebar command icon id (`SidebarCommandIconGlyph`).
pub(crate) fn command_icon_asset(id: &str) -> SharedString {
    if id == "terminal" {
        return "titlebar/terminal-2.svg".into();
    }
    let mut kebab = String::new();
    for character in id.chars() {
        if character.is_ascii_uppercase() {
            kebab.push('-');
            kebab.push(character.to_ascii_lowercase());
        } else {
            kebab.push(character);
        }
    }
    format!("titlebar/{kebab}.svg").into()
}

fn tag_glyph(item: &TagListItem, custom: &[CustomTag], muted: Rgba) -> AnyElement {
    if item.kind == "separator" {
        return settings_icon(icon::MINUS, 16.0, muted).into_any_element();
    }
    if item.kind == "untagged" {
        let (path, color) = builtin_presentation("untagged");
        return settings_icon(path, 15.0, gpui::rgb(color)).into_any_element();
    }
    if is_custom_tag_id(&item.id) {
        let tag = custom.iter().find(|tag| tag.id == item.id);
        let icon_id = tag.map(|tag| tag.icon.as_str()).unwrap_or(DEFAULT_TAG_ICON);
        let color = tag
            .map(|tag| super::color::hex_rgba(&tag.color))
            .unwrap_or(gpui::rgb(0x8e949d));
        let known = SPACE_EDITOR_ICONS.iter().any(|icon| icon.id == icon_id);
        let path: SharedString = if known {
            command_icon_asset(icon_id)
        } else {
            "modals/settings/tag.svg".into()
        };
        return gpui::svg()
            .path(path)
            .size(px(15.0))
            .text_color(hsla(color))
            .into_any_element();
    }
    let (path, color) = builtin_presentation(&item.id);
    settings_icon(path, 15.0, gpui::rgb(color)).into_any_element()
}

#[derive(Clone)]
struct TagRowDrag {
    index: usize,
    label: SharedString,
}

struct TagRowDragView {
    label: SharedString,
    palette: SettingsPalette,
}

impl Render for TagRowDragView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised_hover))
            .text_size(px(14.0))
            .text_color(hsla(p.foreground))
            .opacity(0.85)
            .child(self.label.clone())
    }
}

impl FieldStates {
    fn tag_list_state<V: SettingsPage>(
        page: &mut V,
        creating: bool,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        if page.field_states().tag_list.is_some() {
            return;
        }
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Tag name"));
        let picker_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search icons"));
        let name_subscription = cx.subscribe_in(
            &name_input,
            window,
            |page: &mut V, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let text = input.read(cx).value().to_string();
                    if text.chars().count() > MAX_TAG_NAME {
                        let clipped: String = text.chars().take(MAX_TAG_NAME).collect();
                        input.update(cx, |input, cx| input.set_value(clipped, window, cx));
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => submit_new_tag(page, window, cx),
                _ => {}
            },
        );
        let search_subscription = cx.subscribe_in(
            &picker_search,
            window,
            |page: &mut V, _input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(state) = page.field_states().tag_list.as_mut()
                {
                    state.picker_highlight = Some(0);
                    cx.notify();
                }
            },
        );
        let states = page.field_states();
        states.subscriptions.push(name_subscription);
        states.subscriptions.push(search_subscription);
        states.tag_list = Some(TagListState {
            expanded: creating,
            creating,
            name_input,
            icon: DEFAULT_TAG_ICON.to_string(),
            color: String::new(),
            picker_open: false,
            picker_search,
            picker_highlight: None,
            picker_trigger: Rc::new(Cell::new(None)),
            dragging: None,
        });
    }
}

fn current_custom_tags<V: SettingsPage>(page: &V, cx: &gpui::App) -> Option<Value> {
    page.settings_store()
        .read(cx)
        .sidebar_state()
        .get("customSessionTags")
        .cloned()
        .filter(Value::is_object)
}

/// Writes the daemon catalog: the hydrate is updated first, then `updateCustomSessionTags`.
fn write_custom_tags<V: SettingsPage>(page: &mut V, state: Value, cx: &mut Context<V>) {
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| {
        let mut sidebar_state = store.sidebar_state().clone();
        sidebar_state["customSessionTags"] = state.clone();
        store.receive_sidebar_state(sidebar_state, cx);
        store.post_message(
            json!({ "state": state, "type": "updateCustomSessionTags" }),
            cx,
        );
    });
}

fn random_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut value = (nanos as u64) ^ 0x9e37_79b9_7f4a_7c15;
    let mut token = String::new();
    for _ in 0..6 {
        let digit = (value % 36) as u32;
        value /= 36;
        token.push(std::char::from_digit(digit, 36).unwrap_or('0'));
    }
    token
}

fn base36(mut value: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(std::char::from_digit((value % 36) as u32, 36).unwrap_or('0'));
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

/// `createCustomSessionTag`.
fn created_catalog(current: Option<&Value>, name: &str, icon: &str, color: &str) -> Value {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let tag_id = format!("custom-{}{}", base36(millis), random_token());
    let mut order: Vec<Value> = current
        .and_then(|state| state.get("order"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tags = current
        .and_then(|state| state.get("tags"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    order.push(json!(tag_id));
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TAG_NAME)
        .collect();
    tags.insert(
        tag_id.clone(),
        json!({ "color": color.to_lowercase(), "icon": icon, "name": name, "tagId": tag_id }),
    );
    json!({ "order": order, "tags": tags })
}

fn submit_new_tag<V: SettingsPage>(page: &mut V, window: &mut Window, cx: &mut Context<V>) {
    let Some(state) = page.field_states().tag_list.as_mut() else {
        return;
    };
    let name = state.name_input.read(cx).value().trim().to_string();
    if name.is_empty() {
        return;
    }
    let (icon, color) = (state.icon.clone(), state.color.clone());
    state.creating = false;
    state.picker_open = false;
    let input = state.name_input.clone();
    input.update(cx, |input, cx| input.set_value("", window, cx));
    let current = current_custom_tags(page, cx);
    let next = created_catalog(current.as_ref(), &name, &icon, &color);
    write_custom_tags(page, next, cx);
}

fn save_items<V: SettingsPage>(
    page: &mut V,
    items: &[TagListItem],
    custom: Option<&Value>,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    store.update(cx, |store, cx| {
        store.update_setting(KEY, items_json(items), cx)
    });
    // CDXC:Sessions 2026-09-11 DECISION: User: custom tags are added and sorted from the existing Sidebar Tags list. Custom rows sit in the same drag list as the built-in rows (hide, disable, reorder work unchanged) and gain a delete action; the relative order of the custom rows is written back to the daemon catalog so the phone's Tag as menu lists them in the same order.
    // The custom rows' relative order is the daemon catalog's order.
    if let Some(custom) = custom {
        let next_order: Vec<String> = items
            .iter()
            .filter(|item| item.kind == "tag" && is_custom_tag_id(&item.id))
            .map(|item| item.id.clone())
            .collect();
        let current_order: Vec<String> = custom
            .get("order")
            .and_then(Value::as_array)
            .map(|order| {
                order
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if next_order != current_order {
            let mut next = custom.clone();
            next["order"] = json!(next_order);
            write_custom_tags(page, next, cx);
        }
    }
}

fn icon_results(query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..SPACE_EDITOR_ICONS.len()).collect();
    }
    let mut scored: Vec<(usize, f64)> = SPACE_EDITOR_ICONS
        .iter()
        .enumerate()
        .map(|(index, icon)| (index, command_score(icon.label, query)))
        .filter(|(_, score)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(index, _)| index).collect()
}

fn choose_icon<V: SettingsPage>(
    page: &mut V,
    index: usize,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if let Some(state) = page.field_states().tag_list.as_mut() {
        state.icon = SPACE_EDITOR_ICONS[index].id.to_string();
        state.picker_open = false;
        let input = state.name_input.clone();
        input.update(cx, |input, cx| input.focus(window, cx));
    }
    cx.notify();
}

/// CDXC:Sessions 2026-09-12 DECISION:
/// User: the New tag form lives only in Settings > Sidebar Tags, and it has to be built from the same rounded controls as the rest of Settings. It is laid out as one more row of the tag list, and the icon trigger doubles as the live preview by drawing the chosen glyph in the chosen colour.
/// The New tag form: the compact icon picker (the glyph in the chosen colour), the name box,
/// the colour swatches, Cancel and Create tag.
fn new_tag_form<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let presets = tag_color_presets();
    let Some(state) = page.field_states().tag_list.as_mut() else {
        return div().into_any_element();
    };
    if state.color.is_empty() {
        state.color = presets
            .first()
            .map(|preset| preset.1.clone())
            .unwrap_or_default();
    }
    let (name_input, icon_id, color, picker_trigger, picker_open) = (
        state.name_input.clone(),
        state.icon.clone(),
        state.color.clone(),
        state.picker_trigger.clone(),
        state.picker_open,
    );
    let name_empty = name_input.read(cx).value().trim().is_empty();
    let chevron = p.muted;
    let trigger = h_flex()
        .id("tag-icon-trigger")
        .flex_shrink_0()
        .h(px(32.0))
        .px(px(8.0))
        .gap(px(4.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .cursor_pointer()
        .hover(|this| this.bg(hsla(css_fade(p.hairline, 0.3))))
        .tooltip(tooltip_text("Tag icon"))
        .on_click(cx.listener(|page: &mut V, _: &ClickEvent, window, cx| {
            if let Some(state) = page.field_states().tag_list.as_mut() {
                state.picker_open = !state.picker_open;
                state.picker_highlight = Some(0);
                let search = state.picker_search.clone();
                search.update(cx, |search, cx| {
                    search.set_value("", window, cx);
                    search.focus(window, cx);
                });
            }
            cx.notify();
        }))
        .child(
            gpui::svg()
                .path(command_icon_asset(&icon_id))
                .size(px(16.0))
                .text_color(hsla(super::color::hex_rgba(&color))),
        )
        .child(settings_icon(icon::CHEVRON_DOWN, 13.0, chevron));
    let swatches: Vec<AnyElement> = presets
        .iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let selected = *value == color;
            let value = value.clone();
            div()
                .id(("tag-color", index))
                .flex_shrink_0()
                .size(px(28.0))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(if selected {
                    hsla(p.ring)
                } else {
                    hsla(css_fade(p.hairline, 0.8))
                })
                .when(selected, |this| {
                    this.shadow(vec![gpui::BoxShadow {
                        color: hsla(css_fade(p.ring, 0.45)),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(2.0),
                        inset: false,
                    }])
                })
                .bg(hsla(super::color::hex_rgba(&value)))
                .cursor_pointer()
                .tooltip(tooltip_text(label.clone()))
                .on_click(
                    cx.listener(move |page: &mut V, _: &ClickEvent, _window, cx| {
                        if let Some(state) = page.field_states().tag_list.as_mut() {
                            state.color = value.clone();
                        }
                        cx.notify();
                    }),
                )
                .into_any_element()
        })
        .collect();
    let picker = picker_open
        .then(|| icon_picker_popover(page, p, &icon_id, picker_trigger.clone(), window, cx))
        .flatten();
    v_flex()
        .w_full()
        .gap(px(10.0))
        .on_key_down(
            cx.listener(|page: &mut V, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    if let Some(state) = page.field_states().tag_list.as_mut() {
                        if state.picker_open {
                            state.picker_open = false;
                        } else {
                            state.creating = false;
                        }
                    }
                    cx.notify();
                }
            }),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .on_children_prepainted(capture_child_bounds(picker_trigger, 0))
                        .child(trigger),
                )
                .child(settings_text_input(p, &name_input, None, false, window, cx)),
        )
        .child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .children(swatches),
                )
                .child(
                    h_flex()
                        .ml_auto()
                        .items_center()
                        .gap(px(8.0))
                        .child(settings_button(
                            p,
                            "tag-cancel",
                            "Cancel",
                            None,
                            ButtonVariant::Ghost,
                            false,
                            None,
                            |page: &mut V, _window, cx| {
                                if let Some(state) = page.field_states().tag_list.as_mut() {
                                    state.creating = false;
                                    state.picker_open = false;
                                }
                                cx.notify();
                            },
                            cx,
                        ))
                        .child(settings_button(
                            p,
                            "tag-create",
                            "Create tag",
                            None,
                            ButtonVariant::Outline,
                            name_empty,
                            None,
                            |page: &mut V, window, cx| submit_new_tag(page, window, cx),
                            cx,
                        )),
                ),
        )
        .children(picker)
        .into_any_element()
}

/// The `CommandIconPicker` popover: a search field over the 59 command icons, ranked by cmdk's
/// scorer while searching.
fn icon_picker_popover<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    current: &str,
    trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let bounds = trigger.get()?;
    let state = page.field_states().tag_list.as_ref()?;
    let search = state.picker_search.clone();
    let highlight = state.picker_highlight;
    let query = search.read(cx).value().to_string();
    let results = icon_results(&query);
    let highlighted = hsla(p.foreground_alpha(0.08));
    let checked_bg = if p.light {
        gpui::rgb(0xefefef)
    } else {
        gpui::rgb(0x202020)
    };
    let rows = results.iter().enumerate().map(|(position, index)| {
        let index = *index;
        let icon = &SPACE_EDITOR_ICONS[index];
        let checked = icon.id == current;
        h_flex()
            .id(("tag-icon-option", index))
            .w_full()
            .min_h(px(32.0))
            .px(px(10.0))
            .py(px(6.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .text_size(px(13.0))
            .text_color(hsla(p.foreground))
            .when(checked, |this| this.bg(hsla(checked_bg)))
            .when(highlight == Some(position), |this| this.bg(highlighted))
            .hover(move |this| this.bg(highlighted))
            .on_click(
                cx.listener(move |page: &mut V, _: &ClickEvent, window, cx| {
                    choose_icon(page, index, window, cx);
                }),
            )
            .child(
                gpui::svg()
                    .path(command_icon_asset(icon.id))
                    .size(px(16.0))
                    .text_color(hsla(p.foreground)),
            )
            .child(icon.label)
    });
    let search_results = results.clone();
    let list: AnyElement = if results.is_empty() {
        div()
            .p(px(10.0))
            .text_size(px(12.0))
            .text_center()
            .text_color(hsla(p.muted))
            .child("No matching icons")
            .into_any_element()
    } else {
        v_flex()
            .id("tag-icon-list")
            .w_full()
            .max_h(px(288.0))
            .p(px(4.0))
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    };
    let popup = v_flex()
        .id("tag-icon-picker")
        .occlude()
        .w(px(240.0))
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(hsla(p.popup_border))
        .bg(hsla(p.popup_background))
        .shadow_lg()
        .font_family(SETTINGS_FONT)
        .on_mouse_down_out(
            cx.listener(move |page: &mut V, event: &MouseDownEvent, _window, cx| {
                if bounds.contains(&event.position) {
                    return;
                }
                if let Some(state) = page.field_states().tag_list.as_mut() {
                    state.picker_open = false;
                }
                cx.notify();
            }),
        )
        .on_key_down(
            cx.listener(move |page: &mut V, event: &KeyDownEvent, window, cx| {
                let Some(state) = page.field_states().tag_list.as_mut() else {
                    return;
                };
                let count = search_results.len();
                match event.keystroke.key.as_str() {
                    "down" if count > 0 => {
                        state.picker_highlight =
                            Some((state.picker_highlight.unwrap_or(0) + 1) % count);
                    }
                    "up" if count > 0 => {
                        state.picker_highlight =
                            Some((state.picker_highlight.unwrap_or(0) + count - 1) % count);
                    }
                    "enter" => {
                        if let Some(index) = state
                            .picker_highlight
                            .and_then(|position| search_results.get(position))
                        {
                            let index = *index;
                            cx.stop_propagation();
                            choose_icon(page, index, window, cx);
                        }
                        return;
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            div()
                .p(px(8.0))
                .border_b_1()
                .border_color(hsla(p.popup_border))
                .child(settings_text_input(p, &search, None, false, window, cx)),
        )
        .child(list);
    Some(
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(
                    bounds.origin.x,
                    bounds.origin.y + bounds.size.height + px(4.0),
                ))
                .snap_to_window_with_margin(px(5.0))
                .child(popup),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// `SidebarTagListSettingsField` (the whole Sidebar Tags card body).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tag_list_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    description: &str,
    value: &Value,
    creating_on_open: bool,
    on_reset: PageAction<V>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    FieldStates::tag_list_state(page, creating_on_open, window, cx);
    let custom_state = current_custom_tags(page, cx);
    let custom = custom_tags(custom_state.as_ref());
    let items = normalize_tag_list_items(value, custom_state.as_ref().map(|_| custom.as_slice()));
    let modified = !tag_list_is_default(&items);
    let can_add = custom_state.is_some();
    let (expanded, creating) = page
        .field_states()
        .tag_list
        .as_ref()
        .map(|state| (state.expanded, state.creating && can_add))
        .unwrap_or_default();
    let muted = p.muted;
    let group: SharedString = "settings-row-tag-list".into();
    let summary = h_flex()
        .id("tag-list-summary")
        .group(group.clone())
        .w_full()
        .py(px(4.0))
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .cursor_pointer()
        .on_click(cx.listener(|page: &mut V, _: &ClickEvent, _window, cx| {
            if let Some(state) = page.field_states().tag_list.as_mut() {
                state.expanded = !state.expanded;
            }
            cx.notify();
        }))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(10.0))
                .child(settings_icon(
                    if expanded {
                        icon::CHEVRON_DOWN
                    } else {
                        icon::CHEVRON_RIGHT
                    },
                    16.0,
                    muted,
                ))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.9))
                                .text_color(hsla(p.foreground))
                                .child("Tag filter list"),
                        )
                        .child(
                            div()
                                .id("tag-list-info")
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .opacity(0.0)
                                .group_hover(group, |this| this.opacity(1.0))
                                .tooltip(tooltip_text(description.to_string()))
                                .child(settings_icon(icon::INFO_CIRCLE, 15.0, muted)),
                        ),
                ),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(8.0))
                .when(can_add, |this| {
                    this.child(settings_button(
                        p,
                        "tag-list-add",
                        "Add tag",
                        None,
                        ButtonVariant::Outline,
                        false,
                        None,
                        |page: &mut V, window, cx| {
                            if let Some(state) = page.field_states().tag_list.as_mut() {
                                state.creating = true;
                                state.expanded = true;
                                let input = state.name_input.clone();
                                input.update(cx, |input, cx| input.focus(window, cx));
                            }
                            cx.notify();
                        },
                        cx,
                    ))
                })
                .child(settings_button(
                    p,
                    "tag-list-reset",
                    "Reset to Default",
                    None,
                    ButtonVariant::Outline,
                    !modified,
                    Some("These tag settings already match the defaults.".into()),
                    move |page: &mut V, window, cx| on_reset(page, window, cx),
                    cx,
                )),
        );
    let mut body = v_flex().w_full().child(summary);
    if expanded {
        let form = creating.then(|| new_tag_form(page, p, window, cx));
        let row_bg = hsla(css_fade(
            if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                gpui::rgb(0x262626)
            },
            0.2,
        ));
        // CDXC:Settings 2026-09-29 SEE-ALSO: the rows sort live like the dnd-kit sortable list of
        // fields.tsx; the drag itself is fields/reorder.rs, shared with Agents, Actions and Arrange views.
        let order = super::reorder::reorder_order(page, TAG_REORDER_LIST, items.len(), cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        for (slot, index) in order.iter().copied().enumerate() {
            let item = &items[index];
            let label = item_label(item, &custom);
            let dimmed = !item.enabled || !item.visible;
            let text_color = if dimmed { p.muted } else { p.foreground };
            let toggle_items = items.clone();
            let visible_items = items.clone();
            let delete_state = custom_state.clone();
            let item_id = item.id.clone();
            let is_custom = item.kind == "tag" && is_custom_tag_id(&item.id);
            let grip = div()
                .id(("tag-list-grip", index))
                .flex_shrink_0()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .hover(|this| this.bg(hsla(css_fade(gpui::rgb(0x262626), 0.5))))
                .child(settings_icon(icon::GRIP_VERTICAL, 16.0, p.foreground))
                .into_any_element();
            let row = h_flex()
                .id(("tag-list-row", index))
                .w_full()
                .p(px(8.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(row_bg)
                .child(super::reorder::reorder_handle(
                    p,
                    TAG_REORDER_LIST,
                    index,
                    label.clone(),
                    grip,
                ))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(8.0))
                        .py(px(8.0))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(32.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(7.0))
                                .bg(hsla(if p.light {
                                    gpui::rgb(0xf1f1f1)
                                } else {
                                    gpui::rgb(0x262626)
                                }))
                                .child(tag_glyph(item, &custom, p.muted)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(14.0))
                                .text_color(hsla(if item.kind == "separator" {
                                    p.muted
                                } else {
                                    text_color
                                }))
                                .when(item.kind == "separator", |this| this.italic())
                                .child(label.clone()),
                        ),
                )
                // CDXC:Sessions 2026-09-12 DECISION: User: place the custom tag trash button leftmost, before the enabled switch and visibility button.
                .when(is_custom, |this| {
                    this.child(settings_icon_button(
                        p,
                        SharedString::from(format!("tag-delete-{item_id}")),
                        icon::TRASH,
                        16.0,
                        32.0,
                        ButtonVariant::Ghost,
                        Some("Delete tag".into()),
                        false,
                        move |page: &mut V, _window, cx| {
                            let Some(mut state) = delete_state.clone() else {
                                return;
                            };
                            if let Some(tags) = state.get_mut("tags").and_then(Value::as_object_mut)
                            {
                                tags.remove(&item_id);
                            }
                            if let Some(order) =
                                state.get_mut("order").and_then(Value::as_array_mut)
                            {
                                order.retain(|id| id.as_str() != Some(item_id.as_str()));
                            }
                            write_custom_tags(page, state, cx);
                        },
                        cx,
                    ))
                })
                .child(switch_control(
                    p,
                    SharedString::from(format!("tag-enabled-{index}")),
                    label.clone(),
                    item.enabled,
                    false,
                    None,
                    move |page: &mut V, enabled, _window, cx| {
                        let mut next = toggle_items.clone();
                        next[index].enabled = enabled;
                        next[index].visible = enabled;
                        let store = page.settings_store().clone();
                        store.update(cx, |store, cx| {
                            store.update_setting(KEY, items_json(&next), cx)
                        });
                    },
                    cx,
                ))
                .child(settings_icon_button(
                    p,
                    SharedString::from(format!("tag-visible-{index}")),
                    if item.visible {
                        icon::EYE
                    } else {
                        icon::EYE_OFF
                    },
                    16.0,
                    32.0,
                    ButtonVariant::Ghost,
                    Some(if item.visible {
                        "Hide".into()
                    } else {
                        "Show".into()
                    }),
                    false,
                    move |page: &mut V, _window, cx| {
                        let mut next = visible_items.clone();
                        let visible = !next[index].visible;
                        next[index].enabled = visible;
                        next[index].visible = visible;
                        let store = page.settings_store().clone();
                        store.update(cx, |store, cx| {
                            store.update_setting(KEY, items_json(&next), cx)
                        });
                    },
                    cx,
                ))
                .into_any_element();
            let move_items = items.clone();
            let move_custom = custom_state.clone();
            rows.push(super::reorder::reorder_row(
                page,
                TAG_REORDER_LIST,
                index,
                slot,
                row,
                move |page: &mut V, from, to, _window, cx| {
                    let next = super::reorder::move_index(&move_items, from, to);
                    save_items(page, &next, move_custom.as_ref(), cx);
                },
                cx,
            ));
        }
        body = body.child(
            v_flex()
                .w_full()
                .pt(px(12.0))
                .children(form.map(|form| {
                    div()
                        .w_full()
                        .mb(px(8.0))
                        .p(px(10.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(row_bg)
                        .child(form)
                }))
                .child(v_flex().w_full().gap(px(8.0)).children(rows)),
        );
    }
    body.into_any_element()
}
