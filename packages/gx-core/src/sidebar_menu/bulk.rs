//! The menu a multi-selection of rows carries.
//!
//! Ported from the TypeScript sidebar page's bulk menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/bulk-menu.ts`).
//!
//! SEE-ALSO: the bulk half of packages/core-ui/session-card-capabilities.ts, which records the decision that a bulk menu
//! shows only the actions that can run over the selected rows without guessing.

use serde_json::Value;

use crate::sidebar_view::tags::{enabled_visible_tag_sections, tag_presentation, TagCatalog};
use crate::sidebar_view::view::SessionRow;
use crate::sidebar_view::SidebarSettings;

use super::capabilities::{can_sleep, can_wake, supports_full_reload};
use super::commands::{message, MenuCommand};
use super::item::MenuItem;

/// The selected rows, in selection order, and the settings the menu reads.
pub struct BulkMenuInput<'a> {
    pub selected: &'a [&'a SessionRow],
    pub settings: &'a SidebarSettings,
    pub catalog: &'a TagCatalog,
}

/// `createNativeBulkMenu`: `None` below two rows, which is where the row's own menu takes over.
pub fn bulk_menu(input: &BulkMenuInput<'_>) -> Option<Vec<MenuItem>> {
    if input.selected.len() < 2 {
        return None;
    }
    let parking: Vec<&SessionRow> = if input.settings.enable_session_parking {
        input
            .selected
            .iter()
            .filter(|row| !row.is_browser)
            .copied()
            .collect()
    } else {
        Vec::new()
    };
    let taggable: Vec<&SessionRow> = input
        .selected
        .iter()
        .filter(|row| !row.is_browser)
        .copied()
        .collect();
    let shared_tag = shared_tag(&taggable);
    let tags = tag_rows(input, &taggable, shared_tag.as_deref());

    let mut menu: Vec<MenuItem> = Vec::new();
    menu.extend(rows(
        "Sleep Selected",
        "moon",
        &keep(input.selected, |row| can_sleep(row)),
        |session_id| message::set_session_sleeping(session_id, true),
    ));
    menu.extend(rows(
        "Wake Selected",
        "player-play",
        &keep(input.selected, |row| can_wake(row)),
        |session_id| message::set_session_sleeping(session_id, false),
    ));
    if !tags.is_empty() {
        menu.push(MenuItem::submenu("Tag Selected As", "tag", tags.clone()));
    }
    menu.extend(rows(
        "Pin Selected",
        "pinned",
        &keep(input.selected, |row| !row.is_pinned),
        |session_id| message::set_session_pinned(session_id, true),
    ));
    menu.extend(rows(
        "Unpin Selected",
        "pinned-off",
        &keep(input.selected, |row| row.is_pinned),
        |session_id| message::set_session_pinned(session_id, false),
    ));
    let park: Vec<Value> = parking
        .iter()
        .filter(|row| !row.is_parked)
        .map(|row| message::set_session_parked(&row.sidebar_session_id, true))
        .collect();
    if !park.is_empty() {
        menu.push(
            if input.settings.show_tag_menu_when_parking && !taggable.is_empty() {
                let mut children = vec![
                    MenuItem::row(
                        "No Tag Change",
                        "tag-off",
                        MenuCommand::batch(park.clone(), true),
                    ),
                    MenuItem::separator(),
                ];
                children.extend(
                    tags.iter()
                        .map(|item| park_with_tag(item, &park, shared_tag.as_deref())),
                );
                MenuItem::submenu("Park Selected", "archive", children)
            } else {
                MenuItem::row(
                    "Park Selected",
                    "archive",
                    MenuCommand::batch(park.clone(), true),
                )
            },
        );
    }
    menu.extend(rows(
        "Unpark Selected",
        "archive",
        &parking
            .iter()
            .filter(|row| row.is_parked)
            .copied()
            .collect::<Vec<_>>(),
        |session_id| message::set_session_parked(session_id, false),
    ));
    menu.extend(rows(
        "Full Reload Selected",
        "refresh",
        &keep(input.selected, |row| {
            !row.is_browser && supports_full_reload(row)
        }),
        message::full_reload_session,
    ));
    menu.push(MenuItem::separator());
    menu.extend(
        rows(
            "Close Selected",
            "x",
            input.selected,
            message::close_session,
        )
        .into_iter()
        .map(MenuItem::with_danger),
    );
    Some(menu)
}

/// The effective tag every taggable row shares, if they all share one.
fn shared_tag(taggable: &[&SessionRow]) -> Option<String> {
    let first = taggable.first()?.effective_tag.clone();
    taggable
        .iter()
        .all(|row| row.effective_tag == first)
        .then_some(first)
        .flatten()
}

fn tag_rows(
    input: &BulkMenuInput<'_>,
    taggable: &[&SessionRow],
    shared_tag: Option<&str>,
) -> Vec<MenuItem> {
    let include: Vec<&str> = shared_tag.into_iter().collect();
    let sections =
        enabled_visible_tag_sections(&input.settings.tag_list_items, input.catalog, &include);
    let mut items: Vec<MenuItem> = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            items.push(MenuItem::separator());
        }
        for (value, label) in &section.options {
            let checked = shared_tag == Some(value.as_str());
            items.extend(
                rows(label, "tag", taggable, |session_id| {
                    message::set_session_tag(session_id, if checked { None } else { Some(value) })
                })
                .into_iter()
                .map(|item| {
                    item.with_tag_presentation(
                        tag_presentation(Some(value), input.catalog).as_ref(),
                    )
                    .with_checked(checked)
                }),
            );
        }
    }
    items
}

/// A Tag Selected As row turned into "set this tag on every row, then park them".
fn park_with_tag(item: &MenuItem, park: &[Value], shared_tag: Option<&str>) -> MenuItem {
    let Some(command) = item.command.as_ref() else {
        return item.clone();
    };
    let command = command.as_json();
    if command.get("type").and_then(Value::as_str) != Some("batch") {
        return item.clone();
    }
    let mut messages: Vec<Value> = command
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|message| {
            // The row for the tag the selection already has clears it; parking through it keeps
            // the tag instead.
            let clears = message.get("type").and_then(Value::as_str) == Some("setSessionTag")
                && message.get("sessionTag") == Some(&Value::Null);
            if !clears {
                return message;
            }
            let session_id = message
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            message::set_session_tag(session_id, shared_tag)
        })
        .collect();
    messages.extend(park.iter().cloned());
    MenuItem {
        command: Some(MenuCommand::batch(messages, true)),
        ..item.clone()
    }
}

fn keep<'a>(rows: &'a [&'a SessionRow], keep: impl Fn(&SessionRow) -> bool) -> Vec<&'a SessionRow> {
    rows.iter().filter(|row| keep(row)).copied().collect()
}

/// One row that runs the same message over every selected id and clears the selection.
fn rows(
    label: &str,
    icon: &str,
    candidates: &[&SessionRow],
    build: impl Fn(&str) -> Value,
) -> Vec<MenuItem> {
    if candidates.is_empty() {
        return Vec::new();
    }
    vec![MenuItem::row(
        label,
        icon,
        MenuCommand::batch(
            candidates
                .iter()
                .map(|row| build(&row.sidebar_session_id))
                .collect(),
            true,
        ),
    )]
}
