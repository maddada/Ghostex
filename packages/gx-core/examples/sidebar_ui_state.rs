//! Exercises the sidebar's own state: the intents that move it and the client-storage shapes it
//! is written in.
//!
//! Usage: `cargo run --example sidebar_ui_state`
//!
//! It needs no recording, because none of this reads the daemon. Three things are checked:
//!
//! 1. A payload written by the TypeScript build that this port replaces reads back with the same
//!    collapsed groups, collections, lists, hover rows, sections and Space selection.
//! 2. What this writes is byte-for-byte what `writeSidebarUiCollapseState` wrote for the same
//!    state, including `isReferenceChatsCollapsed`, the one field the sidebar state does not own,
//!    so a build from before the port reads its own shape back.
//! 3. A long intent sequence survives a write and a read: state, storage, state again.
//!
//! This is tooling, not a test suite; it prints what it found and fails the process on a
//! difference so it can be run from a build script by hand.

use std::process::ExitCode;

use ghostex_gx_core::{
    collapse_into_storage, collapse_state_from_storage, hidden_items_from_storage,
    hidden_items_into_storage, machine_tab_from_storage, sidebar_window_storage_key, SectionId,
    SidebarCollapseDiff, SidebarHiddenItems, SidebarUiIntent, SidebarUiStore,
    ToggleAllProjectsInput, COLLAPSE_STORAGE_KEY, MACHINE_TAB_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID,
};

/// A version 3 envelope as the TypeScript build wrote it, with `isReferenceChatsCollapsed` (the
/// one field this state does not own) and the per-Space session memory (which it does, since M5
/// piece 7c) carrying values, a section record holding both persisted headings, and a `false` entry
/// that only `true` counts as. The memory's duplicated id is there because the stored payload can
/// hold one and both readers drop it.
const STORED_COLLAPSE: &str = r#"{"state":{"collapsedGroupsById":{"project:alpha":true,"project:beta":false},"collapsedProjectCollectionsByKey":{"local:c1":true},"expandedProjectSessionListsById":{"alpha":true},"expandedSessionCardHoverActionsById":{"beta":true},"collapsedProjectSessionSectionsById":{"alpha":{"pinned":true,"sessions":false}},"isReferenceChatsCollapsed":true,"recentSessionIdsBySpace":{"local":{"s1":["combined-session:alpha:one","combined-session:alpha:one","combined-session:alpha:two"]}},"selectedSpaceIdBySectionKey":{"local":"s1","":"ignored"}},"version":3}"#;

/// The state as a restart would see it: the two headings storage keeps, and everything else.
fn persisted_only(
    mut state: ghostex_gx_core::SidebarCollapseState,
) -> ghostex_gx_core::SidebarCollapseState {
    for sections in state.section_collapse.values_mut() {
        *sections = ghostex_gx_core::SectionCollapse {
            pinned: sections.pinned,
            sessions: sections.sessions,
            ..ghostex_gx_core::SectionCollapse::default()
        };
    }
    state
}

/// The other writer, as the TypeScript sidebar actually writes: it serializes its WHOLE in-memory
/// object, so the envelope is replaced rather than edited. Its copy of the owned fields mirrors the
/// same commands and is therefore the state as of the last write, which is exactly what the
/// difference this state carries is measured against.
fn foreign_whole_object_write(base: &ghostex_gx_core::SidebarCollapseState) -> String {
    let mirrored = collapse_into_storage(base, None);
    let mut envelope: serde_json::Value = serde_json::from_str(&mirrored).unwrap_or_default();
    if let Some(state) = envelope
        .pointer_mut("/state")
        .and_then(|state| state.as_object_mut())
    {
        // The one field this state does not own. `recentSessionIdsBySpace` used to be injected
        // here too, and since M5 piece 7c it is this state's, so a foreign writer that replaced it
        // would be testing a downgrade rather than another owner.
        state.insert(
            "isReferenceChatsCollapsed".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    envelope.to_string()
}

fn main() -> ExitCode {
    let failures = std::cell::Cell::new(0usize);
    let check = |label: &str, ok: bool, detail: String| {
        println!("{:<52} {}", label, if ok { "ok" } else { "DIFFERS" });
        if !ok {
            failures.set(failures.get() + 1);
            println!("  {detail}");
        }
    };

    check(
        "collapse key",
        sidebar_window_storage_key(COLLAPSE_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID)
            == "ghostex-sidebar-ui-collapse-state:window:main",
        sidebar_window_storage_key(COLLAPSE_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID),
    );
    check(
        "machine tab key",
        sidebar_window_storage_key(MACHINE_TAB_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID)
            == "ghostex-sidebar-selected-machine-tab:window:main",
        sidebar_window_storage_key(MACHINE_TAB_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID),
    );
    check(
        "machine tab default",
        machine_tab_from_storage(None) == "local" && machine_tab_from_storage(Some("")) == "local",
        machine_tab_from_storage(None),
    );

    let read = collapse_state_from_storage(Some(STORED_COLLAPSE), None, None);
    check(
        "stored collapsed groups",
        read.collapsed_groups
            .iter()
            .map(String::as_str)
            .eq(["project:alpha"]),
        format!("{:?}", read.collapsed_groups),
    );
    check(
        "stored collapsed collections",
        read.collapsed_collections
            .iter()
            .map(String::as_str)
            .eq(["local:c1"]),
        format!("{:?}", read.collapsed_collections),
    );
    check(
        "stored hover rows",
        read.expanded_hover_actions
            .iter()
            .map(String::as_str)
            .eq(["beta"]),
        format!("{:?}", read.expanded_hover_actions),
    );
    let sections = read
        .section_collapse
        .get("alpha")
        .copied()
        .unwrap_or_default();
    check(
        "stored section collapse takes defaults for the rest",
        sections.pinned
            && !sections.sessions
            && sections.drafts
            && sections.parked
            && sections.snoozed
            && !sections.browser,
        format!("{sections:?}"),
    );
    check(
        "stored Space selection drops the empty section key",
        read.selected_space_by_section.len() == 1
            && read
                .selected_space_by_section
                .get("local")
                .map(String::as_str)
                == Some("s1"),
        format!("{:?}", read.selected_space_by_section),
    );

    // A damaged or unsupported envelope reads as the defaults rather than as a legacy payload.
    for (label, raw) in [
        (
            "unsupported version",
            r#"{"state":{"collapsedGroupsById":{"a":true}},"version":9}"#,
        ),
        ("not an object", "[]"),
        ("not JSON", "{"),
    ] {
        let state = collapse_state_from_storage(Some(raw), None, None);
        check(
            &format!("damaged envelope reads as defaults ({label})"),
            state == Default::default(),
            format!("{state:?}"),
        );
    }

    // The legacy unscoped payload, whose collapsed collections live in the collections document.
    let legacy = collapse_state_from_storage(
        None,
        Some(r#"{"collapsedGroupsById":{"project:alpha":true}}"#),
        Some(r#"{"collections":[{"collectionId":"c9","collapsed":true},{"collectionId":"c8"}]}"#),
    );
    check(
        "legacy payload keeps its groups and collections",
        legacy
            .collapsed_groups
            .iter()
            .map(String::as_str)
            .eq(["project:alpha"])
            && legacy
                .collapsed_collections
                .iter()
                .map(String::as_str)
                .eq(["local:c9"]),
        format!("{legacy:?}"),
    );

    // What a write produces for the state that was just read: every owned field re-derived, and
    // the one unowned field (`isReferenceChatsCollapsed`) carried through byte for byte, because
    // editing another writer's value is not this writer's business. The per-Space session memory
    // IS owned since M5 piece 7c, so the duplicate the stored payload carries is normalized away
    // exactly as `normalizeStoredRecentSessionIdsBySpace` drops it on the way in.
    let written = collapse_into_storage(&read, Some(STORED_COLLAPSE));
    const EXPECTED: &str = r#"{"state":{"collapsedCoordinatorsById":{},"collapsedGroupsById":{"project:alpha":true},"collapsedProjectCollectionsByKey":{"local:c1":true},"collapsedProjectSessionSectionsById":{"alpha":{"pinned":true,"sessions":false}},"expandedCoordinatorOlderThreadsById":{},"expandedProjectSessionListsById":{"alpha":true},"expandedSessionCardHoverActionsById":{"beta":true},"isReferenceChatsCollapsed":true,"recentSessionIdsBySpace":{"local":{"s1":["combined-session:alpha:one","combined-session:alpha:two"]}},"selectedSpaceIdBySectionKey":{"local":"s1"}},"version":3}"#;
    check(
        "write keeps the unowned field, the memory and the version",
        written == EXPECTED,
        written.clone(),
    );
    check(
        "the per-Space session memory is read and re-derived, not carried",
        read.recent_sessions_by_space
            .get("local")
            .and_then(|by_space| by_space.get("s1"))
            .is_some_and(|ids| {
                ids == &[
                    "combined-session:alpha:one".to_string(),
                    "combined-session:alpha:two".to_string(),
                ]
            }),
        format!("{:?}", read.recent_sessions_by_space),
    );
    check(
        "a first write spells out the unowned defaults",
        collapse_into_storage(&Default::default(), None)
            == r#"{"state":{"collapsedCoordinatorsById":{},"collapsedGroupsById":{},"collapsedProjectCollectionsByKey":{},"collapsedProjectSessionSectionsById":{},"expandedCoordinatorOlderThreadsById":{},"expandedSessionCardHoverActionsById":{},"isReferenceChatsCollapsed":false,"recentSessionIdsBySpace":{},"selectedSpaceIdBySectionKey":{}},"version":3}"#,
        collapse_into_storage(&Default::default(), None),
    );

    // A version 2 envelope is readable and undamaged; a write must upgrade it and keep the two
    // fields this state does not own, exactly as `writeSidebarUiCollapseState` does.
    const STORED_V2: &str = r#"{"state":{"collapsedGroupsById":{"project:alpha":true},"isReferenceChatsCollapsed":true,"recentSessionIdsBySpace":{"local":{"s1":["combined-session:alpha:one"]}}},"version":2}"#;
    let from_v2 = collapse_state_from_storage(Some(STORED_V2), None, None);
    let mut moved_v2 = from_v2.clone();
    moved_v2.collapsed_groups.insert("project:beta".to_string());
    let upgraded =
        SidebarCollapseDiff::between(&from_v2, &moved_v2).apply(Some(STORED_V2), &moved_v2);
    let upgraded_value: serde_json::Value = serde_json::from_str(&upgraded).unwrap_or_default();
    check(
        "a version 2 envelope is upgraded, not replaced",
        upgraded_value.get("version") == Some(&serde_json::json!(3))
            && upgraded_value.pointer("/state/isReferenceChatsCollapsed")
                == Some(&serde_json::Value::Bool(true))
            && upgraded_value.pointer("/state/recentSessionIdsBySpace/local/s1")
                == STORED_V2
                    .parse::<serde_json::Value>()
                    .ok()
                    .as_ref()
                    .and_then(|stored| stored.pointer("/state/recentSessionIdsBySpace/local/s1"))
            && collapse_state_from_storage(Some(&upgraded), None, None)
                .collapsed_groups
                .len()
                == 2,
        upgraded.clone(),
    );
    // An envelope from a build this one cannot read keeps its own object rather than losing it.
    const STORED_FUTURE: &str =
        r#"{"state":{"collapsedGroupsById":{},"somethingNewer":{"kept":1}},"version":9}"#;
    let future = collapse_state_from_storage(Some(STORED_FUTURE), None, None);
    let mut moved_future = future.clone();
    moved_future
        .collapsed_groups
        .insert("project:beta".to_string());
    let carried = SidebarCollapseDiff::between(&future, &moved_future)
        .apply(Some(STORED_FUTURE), &moved_future);
    let carried_value: serde_json::Value = serde_json::from_str(&carried).unwrap_or_default();
    check(
        "an unknown version keeps the fields and the number it carried",
        carried_value.pointer("/state/somethingNewer/kept") == Some(&serde_json::json!(1))
            && carried_value.get("version") == Some(&serde_json::json!(9)),
        carried,
    );

    let hidden = hidden_items_from_storage(Some(
        r#"{"groupIds":["a","a","",1,"b"],"collectionKeys":["local:c1"]}"#,
    ));
    check(
        "hidden items drop duplicates, blanks and non-strings",
        hidden
            == SidebarHiddenItems {
                group_ids: vec!["a".to_string(), "b".to_string()],
                collection_keys: vec!["local:c1".to_string()],
                auto_hidden_group_ids: Vec::new(),
            },
        format!("{hidden:?}"),
    );
    check(
        "hidden items write in the shape the sidebar wrote",
        hidden_items_into_storage(&hidden)
            == r#"{"collectionKeys":["local:c1"],"groupIds":["a","b"]}"#,
        hidden_items_into_storage(&hidden),
    );

    // A long intent sequence, written and read back after every step.
    let mut store = SidebarUiStore::new();
    let groups: Vec<String> = (0..7).map(|index| format!("project:p{index}")).collect();
    let mut raw = collapse_into_storage(&store.state().collapse.clone(), None);
    let mut base = store.state().collapse.clone();
    let mut round_trips = 0usize;
    let mut writes = 0usize;
    let mut foreign_writes = 0usize;
    for step in 0..400usize {
        let group_id = groups[step % groups.len()].clone();
        let storage_id = format!("p{}", step % groups.len());
        let intent = match step % 11 {
            0 => SidebarUiIntent::ToggleGroupCollapsed { group_id },
            1 => SidebarUiIntent::ToggleCoordinatorCollapsed {
                sidebar_session_id: format!("combined-session:{storage_id}:one"),
            },
            2 => SidebarUiIntent::ToggleHoverActions { storage_id },
            3 => SidebarUiIntent::ToggleSection {
                storage_id,
                section: SectionId::ORDER[step % SectionId::ORDER.len()],
            },
            4 => SidebarUiIntent::ToggleCollectionCollapsed {
                storage_id: format!("local:c{}", step % 3),
            },
            5 => SidebarUiIntent::SelectSpace {
                space_id: format!("s{}", step % 4),
            },
            6 => SidebarUiIntent::SelectMachine {
                machine_id: if step % 22 == 6 {
                    "remote-one".to_string()
                } else {
                    "local".to_string()
                },
            },
            7 => SidebarUiIntent::ToggleTagFilter {
                tag: format!("tag{}", step % 5),
            },
            8 => SidebarUiIntent::RememberSpaceSession {
                section_key: store.state().section_key(),
                space_id: format!("s{}", step % 4),
                sidebar_session_id: format!("combined-session:p{}:one", step % 7),
            },
            9 => {
                if step % 22 == 9 {
                    SidebarUiIntent::HideGroup { group_id }
                } else {
                    SidebarUiIntent::UnhideGroup { group_id }
                }
            }
            _ => SidebarUiIntent::ToggleAllProjects(ToggleAllProjectsInput {
                machine_id: store.selected_machine_id().to_string(),
                group_ids: groups.clone(),
            }),
        };
        let outcome = store.apply(intent);
        let pending = store.take_pending();
        if pending.collapse {
            // The write the host performs: the difference since the last write, applied to what is
            // stored at that moment.
            let diff = SidebarCollapseDiff::between(&base, &store.state().collapse);
            raw = diff.apply(Some(&raw), &store.state().collapse);
            base = store.state().collapse.clone();
            writes += 1;
        }
        // Every so often the other writer replaces the whole envelope with its own object, the way
        // the TypeScript sidebar still does while the port runs.
        if step % 17 == 16 {
            raw = foreign_whole_object_write(&base);
            foreign_writes += 1;
        }
        if outcome.changed && !pending.is_empty() {
            let read_back = collapse_state_from_storage(Some(&raw), None, None);
            round_trips += 1;
            // Only Pinned and Sessions are persisted, so the comparison reduces both sides to
            // those: Drafts, Parked, Snoozed and Browser go back to their defaults on a restart
            // by design, exactly as `persistedProjectSessionSectionCollapseState` decided.
            if pending.collapse
                && persisted_only(read_back.clone())
                    != persisted_only(store.state().collapse.clone())
            {
                println!("step {step}: the collapse state did not survive a write and a read");
                println!("  in  {:?}", persisted_only(store.state().collapse.clone()));
                println!("  out {:?}", persisted_only(read_back));
                failures.set(failures.get() + 1);
                break;
            }
        }
    }
    let final_state = collapse_state_from_storage(Some(&raw), None, None);
    let _ = final_state;
    let stored_tail: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    check(
        "a write keeps another writer's field and stores this writer's memory",
        stored_tail
            .pointer("/state/recentSessionIdsBySpace/local")
            .and_then(|value| value.as_object())
            .is_some_and(|by_space| !by_space.is_empty())
            && stored_tail.pointer("/state/isReferenceChatsCollapsed")
                == Some(&serde_json::Value::Bool(true)),
        raw.clone(),
    );
    println!(
        "\n{writes} collapse writes, {foreign_writes} writes by another owner, {round_trips} round trips, {} failures so far",
        failures.get()
    );

    // Show Hidden, the tag filters and the multi-selection are deliberately not persisted: they
    // start over on the next launch, exactly as the TypeScript state did.
    let mut fresh = SidebarUiStore::new();
    fresh.apply(SidebarUiIntent::ToggleShowHidden);
    fresh.apply(SidebarUiIntent::ToggleTagFilter {
        tag: "favorite".to_string(),
    });
    fresh.apply(SidebarUiIntent::SetSelectedSessions {
        session_ids: vec!["combined-session:a:b".to_string()],
    });
    check(
        "unpersisted state owes no write",
        fresh.pending().is_empty(),
        format!("{:?}", fresh.pending()),
    );

    if failures.get() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
