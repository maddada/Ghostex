//! Exercises the four edits M4d part 2 moved out of the sidebar page: a Project Group's Rename,
//! colour and Ungroup, the New/Edit Space dialog's result, and the row a Space switch restores.
//!
//! Usage: `cargo run --example project_doc_edits`
//!
//! It needs no recording: every case below is a document in, a document out. The expected answers
//! were derived by reading the shipped TypeScript, not by running it, because the page that runs it
//! is being deleted in this same milestone and a gate driving it would outlive what it proves:
//!
//! - `runNativeCollectionAction` (the deleted sidebar page's `collections.ts`) with
//!   `updateSidebarProjectCollection` and `removeSidebarProjectCollection`
//!   (packages/core-ui/project-collections.ts (deleted 2026-10-01));
//! - `applySidebarSpaceEditorResult`, `createSidebarSpace`, `updateSidebarSpace`,
//!   `deleteSidebarSpace` and `sanitizeSidebarSpacesState` (packages/core-ui/spaces.ts (deleted 2026-10-01)), with the
//!   palette of packages/core-ui/space-colors.ts (deleted 2026-10-01);
//! - `switchNativeSidebarSpace` (tooling/gx-core/sidebar-page-frozen/space-navigation.ts).
//!
//! This is tooling, not a test suite; it prints what it found and fails the process on a
//! difference.

use std::cell::Cell;
use std::process::ExitCode;
use std::sync::Arc;

use ghostex_gx_core::{
    plan_collection_menu_edit, plan_space_editor_result, plan_space_switch_restore,
    CollectionsDocument, GroupCore, GroupSummary, SessionRow, SessionView, SidebarView,
    SpaceEditorResult, SpaceSwitchFocus, SpacesDocument,
};
use serde_json::{json, Value};

/// Two folders with a project each, and a counter that no edit below may move.
fn collections() -> CollectionsDocument {
    CollectionsDocument::from_storage_json(&json!({
        "collections": [
            { "collectionId": "c1", "color": "#7c6df2", "projectIds": ["alpha"], "title": "Group 1" },
            { "collectionId": "c2", "color": "#3aa675", "projectIds": ["beta"], "title": "Group 2" },
        ],
        "nextCollectionNumber": 7,
    }))
}

/// Two Spaces, the second holding the project the create cases move around.
fn spaces() -> SpacesDocument {
    SpacesDocument::from_echo_json(&json!({
        "order": ["s1", "s2"],
        "spaces": {
            "s1": {
                "color": "#7c6df2", "icon": "stack", "memberCollectionIds": ["c1"],
                "memberProjectIds": [], "name": "Work", "spaceId": "s1",
            },
            "s2": {
                "color": "#3aa675", "icon": "flask", "memberCollectionIds": [],
                "memberProjectIds": ["beta"], "name": "Side", "spaceId": "s2",
            },
        },
    }))
    .expect("the fixture is a document")
}

fn collection_command(action: &str, collection_id: &str, value: Option<&str>) -> Value {
    let mut command = json!({
        "type": "collectionAction",
        "action": action,
        "collectionId": collection_id,
    });
    if let Some(value) = value {
        command["value"] = Value::from(value);
    }
    command
}

fn editor_result(fields: Value) -> SpaceEditorResult {
    SpaceEditorResult::from_json(&fields).expect("the fixture names a mode")
}

/// The titles, colours and order of a collections document, which is the whole of what an edit here
/// may move besides the counter.
fn collection_rows(document: &CollectionsDocument) -> Vec<(String, String, String, Vec<String>)> {
    document
        .state
        .collections
        .iter()
        .map(|collection| {
            (
                collection.collection_id.clone(),
                collection.title.clone(),
                collection.color.clone(),
                collection.project_ids.clone(),
            )
        })
        .collect()
}

/// The Spaces in order, with the fields the dialog can write and the members it can move.
fn space_rows(document: &SpacesDocument) -> Vec<(String, String, String, String, Vec<String>)> {
    document
        .state
        .order
        .iter()
        .filter_map(|space_id| document.state.spaces.get(space_id))
        .map(|space| {
            (
                space.space_id.clone(),
                space.name.clone(),
                space.icon.clone(),
                space.color.clone(),
                space.member_project_ids.clone(),
            )
        })
        .collect()
}

/// A drawn group with the rows it holds, which is all `plan_space_switch_restore` reads.
fn group(group_id: &str, sidebar_session_ids: &[&str]) -> ghostex_gx_core::GroupView {
    ghostex_gx_core::GroupView {
        core: Arc::new(GroupCore {
            group_id: group_id.to_string(),
            storage_id: group_id.to_string(),
            title: group_id.to_string(),
            title_tooltip: None,
            is_active: false,
            project_context: None,
            summary: GroupSummary::default(),
            collapsed: false,
            hover_actions_expanded: false,
            sections: Vec::new(),
            sessions: sidebar_session_ids
                .iter()
                .map(|session_id| SessionView {
                    row: Arc::new(SessionRow {
                        sidebar_session_id: (*session_id).to_string(),
                        ..SessionRow::default()
                    }),
                    is_focused: false,
                    is_visible: true,
                    is_multi_selected: false,
                    nesting: Default::default(),
                })
                .collect(),
            remote_machine: None,
            is_stale: false,
            machine_names: Vec::new(),
            action_target: None,
            folded_worktrees: Vec::new(),
        }),
        collection_color: None,
        collection_id: None,
        space_project: None,
        holds_active_session_group: false,
    }
}

fn view(groups: Vec<ghostex_gx_core::GroupView>) -> SidebarView {
    SidebarView {
        groups,
        ..SidebarView::default()
    }
}

fn main() -> ExitCode {
    let failures = Cell::new(0u32);
    let checks = Cell::new(0u32);
    let check = |label: &str, passed: bool, found: String| {
        checks.set(checks.get() + 1);
        if passed {
            println!("ok    {label}");
        } else {
            failures.set(failures.get() + 1);
            println!("FAIL  {label}: {found}");
        }
    };

    // ---- A Project Group's Rename, colour and Ungroup -------------------------------------
    let base = collections();

    let renamed = plan_collection_menu_edit(
        &base,
        &collection_command("rename", "c1", Some("  Infra  ")),
    )
    .expect("a known collection writes");
    check(
        "rename trims with JavaScript's rules and touches nothing else",
        collection_rows(&renamed)
            == vec![
                (
                    "c1".into(),
                    "Infra".into(),
                    "#7c6df2".into(),
                    vec!["alpha".into()],
                ),
                (
                    "c2".into(),
                    "Group 2".into(),
                    "#3aa675".into(),
                    vec!["beta".into()],
                ),
            ],
        format!("{:?}", collection_rows(&renamed)),
    );
    check(
        "rename leaves nextCollectionNumber alone",
        renamed.next_collection_number == 7,
        renamed.next_collection_number.to_string(),
    );

    let blank =
        plan_collection_menu_edit(&base, &collection_command("rename", "c1", Some("   \t ")))
            .expect("a blank rename still writes");
    check(
        "a name the user cleared keeps the old one (`|| item.title`)",
        blank.state.collections[0].title == "Group 1",
        blank.state.collections[0].title.clone(),
    );

    let long = "x".repeat(85);
    let sliced = plan_collection_menu_edit(&base, &collection_command("rename", "c1", Some(&long)))
        .expect("a long rename writes");
    check(
        "rename slices to 80 units (`.slice(0, 80)`)",
        sliced.state.collections[0].title.chars().count() == 80,
        sliced.state.collections[0]
            .title
            .chars()
            .count()
            .to_string(),
    );

    // Not a palette value on purpose: neither `updateSidebarProjectCollection` nor
    // `saveNativeCollections` sanitizes, and the read-side sanitizer is what decides what draws.
    let recoloured =
        plan_collection_menu_edit(&base, &collection_command("color", "c2", Some("nonsense")))
            .expect("a colour writes");
    check(
        "a colour is stored verbatim, on that collection only",
        collection_rows(&recoloured)
            == vec![
                (
                    "c1".into(),
                    "Group 1".into(),
                    "#7c6df2".into(),
                    vec!["alpha".into()],
                ),
                (
                    "c2".into(),
                    "Group 2".into(),
                    "nonsense".into(),
                    vec!["beta".into()],
                ),
            ],
        format!("{:?}", collection_rows(&recoloured)),
    );

    let kept = plan_collection_menu_edit(&base, &collection_command("color", "c2", None))
        .expect("a colour with no value still writes");
    check(
        "a colour with no value keeps the current one (`?? item.color`)",
        kept.state.collections[1].color == "#3aa675",
        kept.state.collections[1].color.clone(),
    );

    let ungrouped = plan_collection_menu_edit(&base, &collection_command("ungroup", "c1", None))
        .expect("a known collection ungroups");
    check(
        "ungroup drops the folder and keeps the counter, so no name is reused",
        collection_rows(&ungrouped)
            == vec![(
                "c2".into(),
                "Group 2".into(),
                "#3aa675".into(),
                vec!["beta".into()],
            )]
            && ungrouped.next_collection_number == 7,
        format!(
            "{:?} n={}",
            collection_rows(&ungrouped),
            ungrouped.next_collection_number
        ),
    );

    check(
        "an id the document does not hold writes nothing (`if (!collection) return`)",
        plan_collection_menu_edit(&base, &collection_command("rename", "gone", Some("x")))
            .is_none(),
        "wrote".into(),
    );
    check(
        "the four sidebar-state actions are not this file's",
        plan_collection_menu_edit(&base, &collection_command("hide", "c1", None)).is_none()
            && plan_collection_menu_edit(&base, &collection_command("toggle", "c1", None))
                .is_none(),
        "wrote".into(),
    );

    // ---- The New/Edit Space dialog's result ------------------------------------------------
    let held = spaces();
    // 1764700000000 is 0x19ac6e77a00, whose base-36 form is what `Date.now().toString(36)` gives.
    let now_ms = 1_764_700_000_000i64;

    let created = plan_space_editor_result(
        &held,
        &editor_result(json!({ "mode": "create", "name": "Ops" })),
        now_ms,
    )
    .expect("a named create writes");
    check(
        "a new Space goes last, with the default icon and the palette colour for its position",
        space_rows(&created)
            == vec![
                (
                    "s1".into(),
                    "Work".into(),
                    "stack".into(),
                    "#7c6df2".into(),
                    vec![],
                ),
                (
                    "s2".into(),
                    "Side".into(),
                    "flask".into(),
                    "#3aa675".into(),
                    vec!["beta".into()],
                ),
                (
                    format!("space-3-{}", radix36(now_ms)),
                    "Ops".into(),
                    "stack".into(),
                    // `SIDEBAR_SPACE_COLORS[state.order.length % length]` with two Spaces held:
                    // the third entry of the palette, the dark-theme gray taken out.
                    "#3aa675".into(),
                    vec![],
                ),
            ],
        format!("{:?}", space_rows(&created)),
    );

    let created_with_member = plan_space_editor_result(
        &held,
        &editor_result(json!({ "mode": "create", "name": "Ops", "memberProjectId": "beta" })),
        now_ms,
    )
    .expect("a create with a member writes");
    check(
        "a member moves into the new Space and out of every other (one Space per project)",
        space_rows(&created_with_member)
            .iter()
            .map(|row| (row.0.clone(), row.4.clone()))
            .collect::<Vec<_>>()
            == vec![
                ("s1".to_string(), vec![]),
                ("s2".to_string(), vec![]),
                (
                    format!("space-3-{}", radix36(now_ms)),
                    vec!["beta".to_string()],
                ),
            ],
        format!("{:?}", space_rows(&created_with_member)),
    );

    check(
        "a create with nothing but whitespace for a name writes nothing",
        plan_space_editor_result(
            &held,
            &editor_result(json!({ "mode": "create", "name": "   " })),
            now_ms,
        )
        .is_none(),
        "wrote".into(),
    );

    let edited = plan_space_editor_result(
        &held,
        &editor_result(json!({ "mode": "edit", "spaceId": "s2", "name": "Personal" })),
        now_ms,
    )
    .expect("a known Space edits");
    check(
        "an edit changes only the fields the dialog reported, members untouched",
        space_rows(&edited)
            == vec![
                (
                    "s1".into(),
                    "Work".into(),
                    "stack".into(),
                    "#7c6df2".into(),
                    vec![],
                ),
                (
                    "s2".into(),
                    "Personal".into(),
                    "flask".into(),
                    "#3aa675".into(),
                    vec!["beta".into()],
                ),
            ],
        format!("{:?}", space_rows(&edited)),
    );

    let bad_colour = plan_space_editor_result(
        &held,
        &editor_result(json!({ "mode": "edit", "spaceId": "s1", "color": "nonsense" })),
        now_ms,
    )
    .expect("a known Space edits");
    check(
        "an edit is sanitized, so a colour that is not #rrggbb falls back by position",
        space_rows(&bad_colour)[0].3 == "#4f5663",
        space_rows(&bad_colour)[0].3.clone(),
    );

    let deleted = plan_space_editor_result(
        &held,
        &editor_result(json!({ "mode": "delete", "spaceId": "s1" })),
        now_ms,
    )
    .expect("a known Space deletes");
    check(
        "a delete drops the Space and leaves every other member id exactly as it was",
        space_rows(&deleted)
            == vec![(
                "s2".into(),
                "Side".into(),
                "flask".into(),
                "#3aa675".into(),
                vec!["beta".into()],
            )],
        format!("{:?}", space_rows(&deleted)),
    );

    check(
        "a delete or an edit naming a Space that is gone writes nothing",
        plan_space_editor_result(
            &held,
            &editor_result(json!({ "mode": "delete", "spaceId": "gone" })),
            now_ms,
        )
        .is_none()
            && plan_space_editor_result(
                &held,
                &editor_result(json!({ "mode": "edit", "spaceId": "gone", "name": "x" })),
                now_ms,
            )
            .is_none(),
        "wrote".into(),
    );

    // ---- The row a Space switch restores ---------------------------------------------------
    let drawn = view(vec![
        group(
            "project:alpha",
            &["combined-session:alpha:one", "combined-session:alpha:two"],
        ),
        group("project:beta", &["combined-session:beta:one"]),
    ]);
    check(
        "the newest remembered row the Space still draws wins",
        plan_space_switch_restore(
            &drawn,
            &[
                "combined-session:gone:one".to_string(),
                "combined-session:beta:one".to_string(),
                "combined-session:alpha:one".to_string(),
            ],
        ) == Some(SpaceSwitchFocus::Session {
            sidebar_session_id: "combined-session:beta:one".to_string(),
        }),
        format!("{:?}", plan_space_switch_restore(&drawn, &[])),
    );
    check(
        "with nothing remembered the Space opens on its first row",
        plan_space_switch_restore(&drawn, &[])
            == Some(SpaceSwitchFocus::Session {
                sidebar_session_id: "combined-session:alpha:one".to_string(),
            }),
        format!("{:?}", plan_space_switch_restore(&drawn, &[])),
    );
    let empty_projects = view(vec![group("project:alpha", &[])]);
    check(
        "a Space whose projects hold no session focuses the first project instead",
        plan_space_switch_restore(&empty_projects, &["combined-session:gone:one".to_string()])
            == Some(SpaceSwitchFocus::Group {
                group_id: "project:alpha".to_string(),
            }),
        format!("{:?}", plan_space_switch_restore(&empty_projects, &[])),
    );
    check(
        "a Space that draws nothing at all restores nothing",
        plan_space_switch_restore(&view(Vec::new()), &[]).is_none(),
        "restored".into(),
    );

    println!("\n{} checks, {} failures", checks.get(), failures.get());
    if failures.get() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `Number.prototype.toString(36)`, written out here so the expected Space id is derived rather
/// than pasted.
fn radix36(value: i64) -> String {
    let mut digits = Vec::new();
    let mut remaining = value as u64;
    while remaining > 0 {
        digits.push(char::from_digit((remaining % 36) as u32, 36).unwrap_or('0'));
        remaining /= 36;
    }
    digits.iter().rev().collect()
}
