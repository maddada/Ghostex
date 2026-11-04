//! `SessionCardHoverActionsField`: the session hover buttons as 32px icon toggles (dashed and
//! faded when off, a raised fill with a 30% edge when on); a click toggles one, dragging reorders
//! the strip, the chevron included (CDXC:Sessions 2026-09-12 DECISION in
//! packages/core-ui/settings-modal/session-card-hover-actions-field.tsx (deleted 2026-10-01)).
use super::super::super::native_modal_kit::*;
use super::super::catalog::{module, settings_catalog};
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::row::{PageAction, RowSpec, setting_row, settings_icon, tooltip_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};

/// One `{ id, enabled }` entry of `sessionCardHoverButtons`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HoverButtonItem {
    pub(crate) id: String,
    pub(crate) enabled: bool,
}

/// Kept for `FieldStates`; the drag state lives in the reorder list (fields/reorder.rs).
#[derive(Default)]
pub(crate) struct HoverActionsState;

/// Which sidebar's strip the field edits: each keeps its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverStripKind {
    Project,
    Unified,
}

impl HoverStripKind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Project => "sessionCardHoverButtons",
            Self::Unified => "unifiedSessionCardHoverButtons",
        }
    }

    pub(crate) fn other(self) -> Self {
        match self {
            Self::Project => Self::Unified,
            Self::Unified => Self::Project,
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Project => {
                "Buttons a session card shows when you hover it in the Project sidebar. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default the strip is Tag, Park, Sleep, chevron, Close. The button beside the strip copies the Unified sidebar's. Shown while the Project sidebar is selected."
            }
            Self::Unified => {
                "Buttons a session card shows when you hover it in the Unified sidebar, which keeps its own strip. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked, and opening the chevron opens it on every card. By default the strip is Tag, Sleep, chevron, Snooze, Park, Close. The button beside the strip copies the Project sidebar's. Shown while the Unified sidebar is selected."
            }
        }
    }

    fn copy_label(self) -> &'static str {
        match self {
            Self::Project => "Copy the Unified sidebar's buttons",
            Self::Unified => "Copy the Project sidebar's buttons",
        }
    }

    fn default_name(self) -> &'static str {
        match self {
            Self::Project => "DEFAULT_SESSION_CARD_HOVER_BUTTONS",
            Self::Unified => "DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS",
        }
    }
}

fn default_items() -> Vec<HoverButtonItem> {
    default_items_of(HoverStripKind::Project)
}

fn default_items_of(strip: HoverStripKind) -> Vec<HoverButtonItem> {
    settings_catalog()
        .module_value(module::SESSION_CARD_HOVER_ACTIONS, strip.default_name())
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(HoverButtonItem {
                        id: item.get("id")?.as_str()?.to_string(),
                        enabled: item.get("enabled")?.as_bool()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `normalizeSessionCardHoverButtons`.
pub(crate) fn normalize_hover_buttons(candidate: &Value) -> Vec<HoverButtonItem> {
    let defaults = default_items();
    let Some(entries) = candidate.as_array() else {
        return defaults;
    };
    let known: Vec<String> = defaults.iter().map(|item| item.id.clone()).collect();
    if entries.iter().all(Value::is_string) {
        let enabled: Vec<&str> = entries.iter().filter_map(Value::as_str).collect();
        return defaults
            .into_iter()
            .map(|item| {
                if item.id == "chevron" {
                    item
                } else {
                    let on = enabled.contains(&item.id.as_str());
                    HoverButtonItem {
                        id: item.id,
                        enabled: on,
                    }
                }
            })
            .collect();
    }
    let mut items: Vec<HoverButtonItem> = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !known.iter().any(|known| known == id) || items.iter().any(|item| item.id == id) {
            continue;
        }
        items.push(HoverButtonItem {
            id: id.to_string(),
            enabled: entry.get("enabled").and_then(Value::as_bool) == Some(true),
        });
    }
    for item in defaults {
        if !items.iter().any(|existing| existing.id == item.id) {
            items.push(HoverButtonItem {
                id: item.id,
                enabled: false,
            });
        }
    }
    items
}

/// `areSessionCardHoverButtonsEqual(value, DEFAULT)`.
pub(crate) fn hover_buttons_are_default(items: &[HoverButtonItem], strip: HoverStripKind) -> bool {
    items == default_items_of(strip).as_slice()
}

fn items_json(items: &[HoverButtonItem]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| json!({ "enabled": item.enabled, "id": item.id }))
            .collect(),
    )
}

fn label_of(id: &str) -> String {
    settings_catalog()
        .module_value(
            module::SESSION_CARD_HOVER_ACTIONS,
            "SESSION_CARD_HOVER_BUTTON_LABELS",
        )
        .and_then(|labels| labels.get(id))
        .and_then(Value::as_str)
        .unwrap_or(id)
        .to_string()
}

fn icon_of(id: &str) -> &'static str {
    match id {
        "chevron" => "modals/settings/chevron-left.svg",
        "close" => "modals/settings/x.svg",
        "closeAfterDone" => "modals/settings/clock-x.svg",
        "note" => "modals/settings/note.svg",
        "park" => "modals/settings/archive.svg",
        "pin" => "modals/settings/pin.svg",
        "rename" => "modals/settings/pencil.svg",
        "sleep" => "modals/settings/moon.svg",
        "snooze" => "modals/settings/alarm.svg",
        _ => "modals/settings/tag.svg",
    }
}

fn save_items<V: SettingsPage>(
    page: &mut V,
    strip: HoverStripKind,
    items: &[HoverButtonItem],
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    let value = items_json(items);
    store.update(cx, |store, cx| store.update_setting(strip.key(), value, cx));
}

/// The drag-to-reorder list of the hover buttons (fields/reorder.rs).
const LIST: &str = "session-card-hover-buttons";

/// Puts a button the strip left out (Note, while Session notes is off) back where it was, so a
/// click or a drag never drops it or its on/off state from the saved list.
fn with_hidden(
    mut items: Vec<HoverButtonItem>,
    hidden: &Option<(usize, HoverButtonItem)>,
) -> Vec<HoverButtonItem> {
    if let Some((at, item)) = hidden {
        items.insert((*at).min(items.len()), item.clone());
    }
    items
}

/// `SessionCardHoverActionsField` (a wide row). Each button is its own drag handle: a click
/// toggles it; a hold (250ms) or an 8px move (`getSidebarReorderActivationConstraints`) lifts it,
/// at the `[data-dragging='true']` opacity of 0.55.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hover_actions_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &Value,
    strip: HoverStripKind,
    other: &Value,
    cx: &mut Context<V>,
) -> AnyElement {
    let mut items = normalize_hover_buttons(value);
    // CDXC:SessionNotes 2026-10-30 WHY: while Session notes is off (Settings > Extensions) its Note button leaves the strip, and the saved list keeps it, on or off, for when it comes back.
    let notes_on = {
        let store = page.settings_store().read(cx);
        ghostex_settings_catalog::built_in_extensions::enabled_with(
            ghostex_settings_catalog::built_in_extensions::SESSION_NOTES,
            |key| Some(store.bool(key)),
        )
    };
    let hidden = (!notes_on)
        .then(|| items.iter().position(|item| item.id == "note"))
        .flatten()
        .map(|at| (at, items.remove(at)));
    let palette = *p;
    super::reorder::reorder_options(
        page,
        LIST,
        super::reorder::ReorderOptions {
            lifted_opacity: 0.55,
            activation_distance: 8.0,
            scroll: None,
            fill: false,
        },
    );
    let order = super::reorder::reorder_order(page, LIST, items.len(), cx);
    let mut buttons: Vec<AnyElement> = Vec::new();
    for (slot, index) in order.iter().copied().enumerate() {
        let item = &items[index];
        let enabled = item.enabled;
        let icon = icon_of(&item.id);
        let label = label_of(&item.id);
        let toggle_items = items.clone();
        let toggle_hidden = hidden.clone();
        let hover_on = css_mix(p.foreground, 0.12, p.raised_hover);
        let button = div()
            .id(SharedString::from(format!("hover-button-{}", item.id)))
            .flex_shrink_0()
            .size(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .when(enabled, |this| {
                this.border_color(hsla(css_fade(p.foreground, 0.3)))
                    .bg(hsla(p.raised_hover))
                    .hover(move |this| this.bg(hsla(hover_on)))
            })
            .when(!enabled, |this| {
                this.border_dashed()
                    .border_color(hsla(css_fade(p.hairline, 0.8)))
                    .opacity(0.5)
                    .hover(move |this| this.bg(hsla(palette.raised_hover)).opacity(0.85))
            })
            .cursor_grab()
            .tooltip(tooltip_text(label.clone()))
            .on_click(
                cx.listener(move |page: &mut V, _: &ClickEvent, _window, cx| {
                    // A finished drag also clicks the button it moved (`didDragRef`).
                    if super::reorder::reorder_take_just_dragged(page, LIST) {
                        return;
                    }
                    let mut next = toggle_items.clone();
                    next[index].enabled = !enabled;
                    save_items(page, strip, &with_hidden(next, &toggle_hidden), cx);
                }),
            )
            .child(settings_icon(
                icon,
                16.0,
                if enabled { p.foreground } else { p.muted },
            ))
            .into_any_element();
        let handle = super::reorder::reorder_handle(p, LIST, index, label, button);
        let move_items = items.clone();
        let move_hidden = hidden.clone();
        buttons.push(super::reorder::reorder_row(
            page,
            LIST,
            index,
            slot,
            handle,
            move |page: &mut V, from, to, _window, cx| {
                let next = super::reorder::move_index(&move_items, from, to);
                save_items(page, strip, &with_hidden(next, &move_hidden), cx);
            },
            cx,
        ));
    }
    let other_items = normalize_hover_buttons(other);
    let copy_label = strip.copy_label();
    let copy = div()
        .id("hover-buttons-copy")
        .role(gpui::Role::Button)
        .aria_label(copy_label)
        .flex_shrink_0()
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(palette.raised_hover)))
        .tooltip(tooltip_text(copy_label.to_string()))
        .on_click(
            cx.listener(move |page: &mut V, _: &ClickEvent, _window, cx| {
                save_items(page, strip, &other_items, cx);
            }),
        )
        .child(settings_icon("modals/settings/copy.svg", 16.0, p.muted))
        .into_any_element();
    let control = h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .children(buttons)
        .child(div().w(px(6.0)))
        .child(copy)
        .into_any_element();
    setting_row(p, strip.key(), spec.wide(), on_reset, control, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids_on(items: &[HoverButtonItem]) -> Vec<&str> {
        items
            .iter()
            .filter(|item| item.enabled)
            .map(|item| item.id.as_str())
            .collect()
    }

    #[test]
    fn each_sidebar_has_its_own_strip_and_key() {
        assert_eq!(HoverStripKind::Project.key(), "sessionCardHoverButtons");
        assert_eq!(
            HoverStripKind::Unified.key(),
            "unifiedSessionCardHoverButtons"
        );
        assert_eq!(HoverStripKind::Project.other(), HoverStripKind::Unified);
        assert_eq!(
            ids_on(&default_items_of(HoverStripKind::Project)),
            ["tag", "park", "sleep", "chevron", "close"]
        );
        assert_eq!(
            ids_on(&default_items_of(HoverStripKind::Unified)),
            ["tag", "sleep", "chevron", "snooze", "park", "close"]
        );
    }

    #[test]
    fn the_catalogs_unified_default_is_gx_cores() {
        assert_eq!(
            items_json(&default_items_of(HoverStripKind::Unified))
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| (item["id"].clone(), item["enabled"].clone()))
                        .collect::<Vec<_>>()
                }),
            ghostex_gx_core::unified_default_hover_buttons()
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| (item["id"].clone(), item["enabled"].clone()))
                        .collect::<Vec<_>>()
                }),
        );
    }

    #[test]
    fn copying_makes_the_strip_the_others_and_leaves_the_others_default_check_honest() {
        // What the copy button saves: the other strip, as this field reads it.
        let project = items_json(&default_items_of(HoverStripKind::Project));
        let copied = normalize_hover_buttons(&project);
        assert!(!hover_buttons_are_default(&copied, HoverStripKind::Unified));
        assert!(hover_buttons_are_default(&copied, HoverStripKind::Project));
    }
}
