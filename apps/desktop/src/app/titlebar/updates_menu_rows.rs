//! What the top bar's Updates list and its button draw for each state: an item updating shows a
//! spinner and its step, a failed one a red icon whose tooltip says why and a dismiss button, and
//! the button itself turns red while a failure stands and spins while an update runs.
//!
//! CDXC:Updates 2026-11-06 DECISION:
//! User, of the top bar's Updates list: "when i click update on one of these please show a spinner next to it and show spinner on the download button and show red update button if failed to update until i dismiss that failure notice (should be icon with tooltip for the one that failed". A row's click runs (or retries) its update and the list stays open on its spinner; a dismissed failure stays hidden, in every window and on the Settings page, until that item's next update.
//! SEE-ALSO: app/updates/mod.rs (the list, dismissals), server/src/updates/store.rs (gxserver's dismissals).
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Action, AnyElement, ElementId, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

use crate::app::actions::{
    DismissGpuiTitlebarUpdateFailure, RunGpuiTitlebarUpdateItem, RunGpuiTitlebarUpdates,
};
use crate::app::consts::{
    TITLEBAR_ICON_DOWNLOAD, TITLEBAR_POPUP_MENU_ITEM_PADDING_X, TITLEBAR_POPUP_MENU_ROW_HEIGHT,
    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE, TITLEBAR_UPDATE_ICON_SIZE,
};
use crate::app::helpers::{
    titlebar_active_text_color, titlebar_button_hover_color, titlebar_inactive_text_color,
    titlebar_popup_menu_destructive, titlebar_popup_menu_foreground,
    titlebar_popup_standard_menu_row, titlebar_tooltip, titlebar_update_available_color,
};
use crate::app::window::native_modal_kit::modal_spinner;

pub(crate) const UPDATES_MENU_HEADER_HEIGHT: f32 = 52.0;
const ICON_FAILED: &str = "titlebar/alert-circle.svg";
const ICON_DISMISS: &str = "modals/kit/x.svg";
/// A failure's reason in its tooltip is cut here; the Settings page shows all of it.
const REASON_MAX_CHARS: usize = 220;

fn text<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item[key].as_str().filter(|text| !text.is_empty())
}

/// What an item's row says on its right: Retry or Failed, Needs you, its step while it updates,
/// or the new version.
fn item_trailing(item: &Value) -> String {
    match item["state"].as_str() {
        Some("failed") if item["canUpdate"] == true => "Retry".to_string(),
        Some("failed") => "Failed".to_string(),
        Some("needsYou") => "Needs you".to_string(),
        Some("updating") => text(item, "progress").unwrap_or("Updating…").to_string(),
        _ => text(item, "latest")
            .or(text(item, "summary"))
            .unwrap_or("Update")
            .to_string(),
    }
}

/// The row's name; an item of another computer says which one ("Claude Code · Linux box"), while
/// a computer's own Ghostex row is already named after it.
pub(crate) fn item_label(item: &Value) -> String {
    let name = text(item, "name").unwrap_or_default();
    let other_computer = text(item, "computer").is_some_and(|computer| computer != "local");
    match text(item, "computerName") {
        Some(computer) if other_computer && item["kind"] != "machine" => {
            format!("{name} · {computer}")
        }
        _ => name.to_string(),
    }
}

/// The tooltip of a failed item's icon, in plain words.
fn failure_reason(item: &Value) -> String {
    let reason = text(item, "error").unwrap_or("The update failed.").trim();
    let mut reason: String = reason.chars().take(REASON_MAX_CHARS).collect();
    if reason.chars().count() == REASON_MAX_CHARS {
        reason.push('…');
    }
    format!("Update failed: {reason}")
}

/// `other_computers`: some of the items are another computer's, so the line says every computer.
pub(crate) fn updates_menu_header(
    count: usize,
    failed: usize,
    other_computers: bool,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .min_h(px(UPDATES_MENU_HEADER_HEIGHT))
        .justify_center()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(titlebar_active_text_color())
                .child(if count == 1 {
                    "1 update".to_string()
                } else {
                    format!("{count} updates")
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(if failed > 0 {
                    titlebar_popup_menu_destructive()
                } else {
                    titlebar_inactive_text_color()
                })
                .child(match failed {
                    0 if other_computers => {
                        "Ghostex, the agents you use and what failed, on every computer."
                            .to_string()
                    }
                    0 => "Ghostex, the agents you use and what failed.".to_string(),
                    1 => "1 update failed. Point at its icon to see why.".to_string(),
                    failed => format!("{failed} updates failed. Point at an icon to see why."),
                }),
        )
}

/// A row's whole face, edge to edge over the menu row's padding. Its click runs `action` itself
/// instead of the menu's activation, which would close the list before the spinner shows.
fn keep_open_row(
    id: impl Into<ElementId>,
    action: impl Action + Clone,
    child: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_1()
        .min_w_0()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .mx(px(-TITLEBAR_POPUP_MENU_ITEM_PADDING_X))
        .px(px(TITLEBAR_POPUP_MENU_ITEM_PADDING_X))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(action.boxed_clone(), cx);
        })
        .child(child)
}

fn row_spinner() -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(15.0))
        .flex()
        .items_center()
        .justify_center()
        .child(modal_spinner(titlebar_popup_menu_foreground().to_rgb()))
        .into_any_element()
}

pub(crate) fn updates_menu_item_row(item: &Value) -> AnyElement {
    let id = text(item, "id").unwrap_or_default().to_string();
    let state = item["state"].as_str().unwrap_or_default();
    let failed = state == "failed";
    let updating = state == "updating";
    let reason: gpui::SharedString = failure_reason(item).into();
    let error = text(item, "error").unwrap_or_default().to_string();
    let row = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(13.0))
                .text_color(titlebar_popup_menu_foreground())
                .child(item_label(item)),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0))
                .when(updating, |this| this.child(row_spinner()))
                .when(failed, |this| {
                    this.child(
                        div()
                            .id(ElementId::Name(format!("updates-failed-{id}").into()))
                            .flex()
                            .size(px(18.0))
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .size(px(14.0))
                                    .path(ICON_FAILED)
                                    .text_color(titlebar_popup_menu_destructive()),
                            )
                            .managed_tooltip_with_placement(
                                ManagedTooltipPlacement::Auto,
                                move |window, cx| titlebar_tooltip(reason.clone(), window, cx),
                            ),
                    )
                })
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(if failed {
                            titlebar_popup_menu_destructive()
                        } else {
                            titlebar_inactive_text_color()
                        })
                        .child(item_trailing(item)),
                )
                .when(failed, |this| {
                    let dismiss = DismissGpuiTitlebarUpdateFailure {
                        id: id.clone(),
                        error: error.clone(),
                    };
                    this.child(
                        div()
                            .id(ElementId::Name(format!("updates-dismiss-{id}").into()))
                            .flex()
                            .size(px(20.0))
                            .ml(px(-2.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .hover(|this| this.bg(titlebar_button_hover_color()))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                window.dispatch_action(dismiss.boxed_clone(), cx);
                            })
                            .child(
                                svg()
                                    .size(px(12.0))
                                    .path(ICON_DISMISS)
                                    .text_color(titlebar_inactive_text_color()),
                            )
                            .managed_tooltip_with_placement(
                                ManagedTooltipPlacement::Auto,
                                |window, cx| titlebar_tooltip("Dismiss", window, cx),
                            ),
                    )
                }),
        );
    keep_open_row(
        ElementId::Name(format!("updates-row-{id}").into()),
        RunGpuiTitlebarUpdateItem { id },
        row,
    )
    .into_any_element()
}

/// Update all; it keeps the list open so each row shows its spinner.
pub(crate) fn updates_menu_update_all_row() -> AnyElement {
    keep_open_row(
        "updates-update-all",
        RunGpuiTitlebarUpdates { update_all: true },
        titlebar_popup_standard_menu_row(
            TITLEBAR_ICON_DOWNLOAD,
            TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
            "Update all".to_string(),
            false,
        ),
    )
    .into_any_element()
}

/// The top bar button's face while Ghostex itself isn't downloading: a spinner while anything
/// updates, else the download icon, then the count; red while a failure stands.
pub(crate) fn update_button_face(busy: bool, failed: bool, badge: usize) -> impl IntoElement {
    let color = if failed {
        titlebar_popup_menu_destructive()
    } else {
        titlebar_update_available_color()
    };
    h_flex()
        .items_center()
        .gap(px(3.0))
        .map(|this| {
            if busy {
                this.child(
                    div()
                        .flex()
                        .size(px(TITLEBAR_UPDATE_ICON_SIZE + 1.0))
                        .ml(px(1.0))
                        .items_center()
                        .justify_center()
                        .child(modal_spinner(color.to_rgb())),
                )
            } else {
                this.child(
                    svg()
                        .size(px(TITLEBAR_UPDATE_ICON_SIZE))
                        .ml(px(1.0))
                        .mt(px(0.5))
                        .path(if failed {
                            ICON_FAILED
                        } else {
                            TITLEBAR_ICON_DOWNLOAD
                        })
                        .text_color(color),
                )
            }
        })
        .when(badge > 0, |this| {
            this.child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color)
                    .child(badge.to_string()),
            )
        })
}

/// The faint red wash behind the button while a failure stands (a tint, never a dark fill).
pub(crate) fn update_button_failed_background() -> gpui::Hsla {
    titlebar_popup_menu_destructive().opacity(0.16)
}

#[cfg(test)]
mod tests {
    use super::item_label;
    use serde_json::json;

    #[test]
    fn a_row_of_another_computer_names_it() {
        let local = json!({ "name": "Claude Code", "computer": "local" });
        assert_eq!(item_label(&local), "Claude Code");
        let remote = json!({ "name": "Claude Code", "kind": "agent", "computer": "remote-box", "computerName": "Linux box" });
        assert_eq!(item_label(&remote), "Claude Code · Linux box");
        let machine = json!({ "name": "Linux box", "kind": "machine", "computer": "remote-box", "computerName": "Linux box" });
        assert_eq!(item_label(&machine), "Linux box");
    }
}
