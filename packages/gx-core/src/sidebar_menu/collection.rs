//! A project collection's context menu.
//!
//! Ported from the TypeScript sidebar page's collection menu (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/collection-menu.ts`; see git history).

use serde_json::Value;

use crate::sidebar_view::spaces::SpacesState;
use crate::sidebar_view::tags::{
    normalize_tag_list_items, tag_label, tag_presentation, TagCatalog, TagListItemKind,
};
use crate::sidebar_view::view::SessionView;

use super::capabilities::{can_sleep, can_wake};
use super::commands::{message, MenuCommand};
use super::item::MenuItem;
use super::membership::{space_membership_menu, SpaceMember};

/// The thirteen collection colours and their names.
const COLLECTION_COLORS: [(&str, &str); 13] = [
    ("#4f5663", "Dark Gray"),
    ("#808080", "Gray"),
    ("#7c6df2", "Violet"),
    ("#3aa675", "Green"),
    ("#d6873f", "Orange"),
    ("#d75b72", "Pink"),
    ("#3f8fc7", "Blue"),
    ("#b36ad4", "Purple"),
    ("#8c9b45", "Lime"),
    ("#c95353", "Red"),
    ("#c4a23d", "Gold"),
    ("#2f9b95", "Teal"),
    ("#596fd1", "Indigo"),
];

/// Everything the collection menu reads.
pub struct CollectionMenuInput<'a> {
    pub collection_id: &'a str,
    pub color: &'a str,
    /// Every row the collection's groups draw, in list order.
    pub sessions: &'a [&'a SessionView],
    pub hidden: bool,
    /// The user's tag filter list, raw as it sits in settings.
    pub tag_list_items: &'a Value,
    pub catalog: &'a TagCatalog,
    pub spaces: Option<&'a SpacesState>,
}

/// `createNativeCollectionMenu`.
pub fn collection_menu(input: &CollectionMenuInput<'_>) -> Vec<MenuItem> {
    let id = input.collection_id;
    let intent = |label: &str, icon: &str, action: &str| {
        MenuItem::row(
            label,
            icon,
            MenuCommand::collection_action(id, action, None),
        )
    };
    let agents: Vec<&SessionView> = input
        .sessions
        .iter()
        .filter(|session| !session.row.is_browser)
        .copied()
        .collect();
    // The collection menu reads the tag list as SETTINGS normalize it, without the daemon's
    // catalog, so a custom tag the daemon no longer knows still has a row here (labelled
    // "Custom tag") where the Tag As menu drops it. That is the TypeScript's behaviour, kept.
    let tags: Vec<String> = normalize_tag_list_items(input.tag_list_items, None)
        .into_iter()
        .filter(|item| item.kind == TagListItemKind::Tag && item.enabled && item.visible)
        .map(|item| item.id)
        .collect();

    let mut menu =
        vec![intent("Select All Sessions", "check", "select")
            .with_disabled(input.sessions.is_empty())];
    menu.extend(batch(
        "Sleep Sessions",
        "moon",
        &filtered(input.sessions, |session| can_sleep(&session.row)),
        |session_id| message::set_session_sleeping(session_id, true),
    ));
    menu.extend(batch(
        "Wake Sessions",
        "player-play",
        &filtered(input.sessions, |session| can_wake(&session.row)),
        |session_id| message::set_session_sleeping(session_id, false),
    ));
    if !agents.is_empty() && !tags.is_empty() {
        let mut children = batch("No Tag", "tag-off", &agents, |session_id| {
            message::set_session_tag(session_id, None)
        });
        children.push(MenuItem::separator());
        for tag in &tags {
            let label = tag_label(Some(tag), input.catalog).unwrap_or_else(|| tag.clone());
            children.extend(
                batch(&label, "tag", &agents, |session_id| {
                    message::set_session_tag(session_id, Some(tag))
                })
                .into_iter()
                .map(|item| {
                    item.with_tag_presentation(tag_presentation(Some(tag), input.catalog).as_ref())
                }),
            );
        }
        menu.push(MenuItem::submenu("Tag Sessions", "tag", children).with_page());
    }
    menu.extend(batch(
        "Pin Sessions",
        "pinned",
        &filtered(input.sessions, |session| !session.row.is_pinned),
        |session_id| message::set_session_pinned(session_id, true),
    ));
    menu.extend(batch(
        "Unpin Sessions",
        "pinned-off",
        &filtered(input.sessions, |session| session.row.is_pinned),
        |session_id| message::set_session_pinned(session_id, false),
    ));
    menu.extend(batch(
        "Full Reload Sessions",
        "refresh",
        &agents,
        |session_id| message::full_reload_session(session_id),
    ));
    menu.push(MenuItem::separator());
    menu.push(MenuItem {
        label: Some("Rename Group".to_string()),
        icon: Some("pencil".to_string()),
        command: Some(MenuCommand::rename_collection(id)),
        ..MenuItem::default()
    });
    menu.push(
        MenuItem::submenu(
            "Group Color",
            "palette",
            COLLECTION_COLORS
                .iter()
                .map(|(color, label)| MenuItem {
                    label: Some((*label).to_string()),
                    color: Some((*color).to_string()),
                    checked: input.color == *color,
                    command: Some(MenuCommand::collection_action(id, "color", Some(color))),
                    ..MenuItem::default()
                })
                .collect(),
        )
        .with_page(),
    );
    if let Some(spaces) = space_membership_menu(input.spaces, SpaceMember::Collection(id)) {
        menu.push(spaces);
    }
    menu.push(intent(
        if input.hidden {
            "Unhide Group"
        } else {
            "Hide Group"
        },
        "eye-off",
        "hide",
    ));
    menu.push(intent("Delete Group", "trash", "ungroup").with_danger());
    menu.push(MenuItem {
        label: Some("Close All Sessions".to_string()),
        icon: Some("x".to_string()),
        danger: true,
        disabled: input.sessions.is_empty(),
        command: Some(MenuCommand::command(message::close_sessions(
            &input
                .sessions
                .iter()
                .map(|session| session.row.sidebar_session_id.clone())
                .collect::<Vec<_>>(),
        ))),
        ..MenuItem::default()
    });
    menu
}

fn filtered<'a>(
    sessions: &'a [&'a SessionView],
    keep: impl Fn(&SessionView) -> bool,
) -> Vec<&'a SessionView> {
    sessions
        .iter()
        .filter(|session| keep(session))
        .copied()
        .collect()
}

/// One row that runs the same message over every candidate, or nothing when there are none.
fn batch(
    label: &str,
    icon: &str,
    candidates: &[&SessionView],
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
                .map(|session| build(&session.row.sidebar_session_id))
                .collect(),
            false,
        ),
    )]
}
