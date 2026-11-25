//! The Close Project successor rule, asserted against a table derived by READING
//! the deleted React sidebar's `close-project-successor.ts` and the `post` that calls it in
//! the deleted sidebar page's `controller.ts`.
//!
//! **Why a table and not a parity harness.** The TypeScript half was the deleted sidebar page's `controller.ts`,
//! which M4d part 2 deletes, so a gate driving it would be a gate that cannot run a week from now.
//! The cases below are the ones the TypeScript answers, written down with the answer it gives.
//!
//! The four claims:
//! 1. candidates are the drawn groups AFTER the closing project, then the ones BEFORE it nearest
//!    first, and a collection contributes its groups whether or not it is collapsed;
//! 2. the closing project not being in the drawn order names nothing (`closingIndex === -1`);
//! 3. only an awake, non-browser row counts, and it is the FIRST such row of the group;
//! 4. the closing group is judged active against the PROJECT the active group belongs to, because
//!    the old runtime marks a project's own group active for a row in one of its user-made groups.
//!
//!   cargo run --release --example close_project_successor          # from packages/gx-core
//!   CLOSE_SUCCESSOR_MUTATION=before-first cargo run --release --example close_project_successor
//!   CLOSE_SUCCESSOR_MUTATION=skip-collapsed-collections cargo run --release --example close_project_successor
//!   CLOSE_SUCCESSOR_MUTATION=sleeping-counts cargo run --release --example close_project_successor
//!   CLOSE_SUCCESSOR_MUTATION=subgroup-is-not-its-project cargo run --release --example close_project_successor

use std::process::ExitCode;
use std::sync::Arc;

use ghostex_gx_core::{
    close_project_group_is_active, close_project_successor_candidates,
    first_awake_successor_session_id, ActiveGroup, CollectionView, OrderItem, OrderKind,
    ProjectKey, SessionRow, SessionView, SidebarView,
};

fn main() -> ExitCode {
    let mutation = std::env::var("CLOSE_SUCCESSOR_MUTATION").unwrap_or_default();
    if !matches!(
        mutation.as_str(),
        "" | "before-first"
            | "skip-collapsed-collections"
            | "sleeping-counts"
            | "subgroup-is-not-its-project"
    ) {
        eprintln!("unknown CLOSE_SUCCESSOR_MUTATION: {mutation}");
        return ExitCode::from(2);
    }
    let mut failures = Vec::new();
    let mut checks = 0usize;

    // The drawn list: project A, then a COLLAPSED collection holding B and C, then project D.
    let view = view();

    let candidates = |closing: &str| -> Vec<String> {
        let mut answer = close_project_successor_candidates(&view, closing);
        if mutation == "before-first" {
            let ordered = ghostex_gx_core::close_project_successor_group_order(&view);
            if let Some(index) = ordered.iter().position(|id| id == closing) {
                answer = ordered[..index]
                    .iter()
                    .rev()
                    .cloned()
                    .chain(ordered[index + 1..].iter().cloned())
                    .collect();
            }
        }
        if mutation == "skip-collapsed-collections" {
            answer.retain(|id| id != "group-b" && id != "group-c");
        }
        answer
    };

    let mut expect_candidates = |closing: &str, expected: &[&str]| {
        checks += 1;
        let answer = candidates(closing);
        if answer != expected {
            failures.push(format!(
                "candidates for {closing}: {answer:?}, expected {expected:?}"
            ));
        }
    };
    // Claim 1: after, then before nearest first, with the collapsed collection's groups in place.
    expect_candidates("group-a", &["group-b", "group-c", "group-d"]);
    expect_candidates("group-c", &["group-d", "group-b", "group-a"]);
    expect_candidates("group-d", &["group-c", "group-b", "group-a"]);
    // Claim 2: a project that is not drawn (another Space, hidden, filtered out) names nothing.
    expect_candidates("group-elsewhere", &[]);

    let mut expect_first_awake = |label: &str, rows: &[SessionView], expected: Option<&str>| {
        checks += 1;
        let answer = match mutation == "sleeping-counts" {
            true => rows
                .iter()
                .find(|session| !session.row.is_browser)
                .map(|session| session.row.sidebar_session_id.as_str()),
            false => first_awake_successor_session_id(rows),
        };
        if answer != expected {
            failures.push(format!(
                "first awake of {label}: {answer:?}, expected {expected:?}"
            ));
        }
    };
    // Claim 3: a sleeping row and a browser tab are passed over; the first awake row wins.
    expect_first_awake(
        "sleeping then awake",
        &[row("s1", "sleeping", false), row("s2", "running", false)],
        Some("s2"),
    );
    expect_first_awake(
        "browser then awake",
        &[row("b1", "running", true), row("s3", "running", false)],
        Some("s3"),
    );
    expect_first_awake(
        "all sleeping",
        &[row("s4", "sleeping", false), row("s5", "sleeping", false)],
        None,
    );
    expect_first_awake("no rows", &[], None);
    // A row the daemon reports in error or done is not sleeping, so focusing it wakes nothing.
    expect_first_awake(
        "error row first",
        &[row("s6", "error", false), row("s7", "running", false)],
        Some("s6"),
    );

    let project = ProjectKey::local("proj-a");
    let project_group_id = project.to_sidebar_group_id();
    let subgroup = ActiveGroup::Subgroup {
        project: project.clone(),
        group_id: "user-made".to_string(),
    };
    let mut expect_active = |label: &str, active: Option<&ActiveGroup>, group: &str, yes: bool| {
        checks += 1;
        let answer = match (mutation == "subgroup-is-not-its-project", active) {
            (true, Some(active)) => active.to_sidebar_group_id() == group,
            (true, None) => false,
            (false, _) => close_project_group_is_active(active, group),
        };
        if answer != yes {
            failures.push(format!("active for {label}: {answer}, expected {yes}"));
        }
    };
    // Claim 4.
    expect_active(
        "the project itself",
        Some(&ActiveGroup::Project(project.clone())),
        &project_group_id,
        true,
    );
    expect_active(
        "a row in one of its user-made groups",
        Some(&subgroup),
        &project_group_id,
        true,
    );
    expect_active(
        "another project",
        Some(&ActiveGroup::Project(ProjectKey::local("proj-b"))),
        &project_group_id,
        false,
    );
    expect_active("no focus at all", None, &project_group_id, false);

    println!(
        "close project successor: {checks} checks, {} failures",
        failures.len()
    );
    if failures.is_empty() {
        return match mutation.is_empty() {
            true => ExitCode::SUCCESS,
            false => {
                eprintln!("MUTATION {mutation} was not caught");
                ExitCode::FAILURE
            }
        };
    }
    for failure in &failures {
        println!("  {failure}");
    }
    match mutation.is_empty() {
        true => ExitCode::FAILURE,
        false => {
            println!("mutation {mutation} caught");
            ExitCode::SUCCESS
        }
    }
}

/// Project A, a collapsed collection of B and C, then project D.
fn view() -> SidebarView {
    SidebarView {
        order: vec![
            OrderItem {
                kind: OrderKind::Project,
                id: "group-a".to_string(),
            },
            OrderItem {
                kind: OrderKind::Collection,
                id: "collection-1".to_string(),
            },
            OrderItem {
                kind: OrderKind::Project,
                id: "group-d".to_string(),
            },
        ],
        collections: vec![CollectionView {
            collection_id: "collection-1".to_string(),
            storage_id: "local:collection-1".to_string(),
            group_ids: vec!["group-b".to_string(), "group-c".to_string()],
            collapsed: true,
            ..CollectionView::default()
        }],
        ..SidebarView::default()
    }
}

fn row(id: &str, lifecycle_state: &str, is_browser: bool) -> SessionView {
    SessionView {
        row: Arc::new(SessionRow {
            sidebar_session_id: id.to_string(),
            is_browser,
            session_kind: Some(match is_browser {
                true => "browser".to_string(),
                false => "terminal".to_string(),
            }),
            lifecycle_state: lifecycle_state.to_string(),
            ..SessionRow::default()
        }),
        is_focused: false,
        is_visible: false,
        is_multi_selected: false,
        nesting: Default::default(),
        machine_fold: None,
        thread_of: None,
    }
}
