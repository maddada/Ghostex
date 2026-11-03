//! The record's copy in memory and the ids it interns: what a change writes is what a reader
//! without I/O sees, a deletion shows only once it commits, the startup load interns every id the
//! record holds, and nothing else interns one. Each test uses ids of its own: the copy and the
//! interned ids are process-wide.

use super::*;

/// The record of `store` with `workspace` pinning `id` in `format`, taken from a real pin so its
/// `info` is one the contract accepted.
pub(super) fn record_pinning(
    store: &Store,
    workspace: &str,
    id: &str,
    format: IdFormat,
) -> WorkSources {
    store.answer("good");
    let mut command = store.set(workspace, false).expect("pinned");
    command.source_id = id.to_string();
    command.info.id = id.to_string();
    command.info.id_format = format;
    command.info.id_keys = match format {
        IdFormat::KeyNumber => vec!["ACM".to_string()],
        IdFormat::Number => Vec::new(),
    };
    let mut sources = store.sources();
    sources.workspaces.insert(store.ws(workspace), command);
    sources
}

fn scope_in(workspace_id: &str) -> TicketScope<'_> {
    TicketScope {
        effective_workspace_id: Some(workspace_id),
        ..TicketScope::default()
    }
}

pub(super) fn command_source(id: &str) -> WorkSourceId {
    WorkSourceId::parse(id).unwrap_or_else(|| panic!("{id} isn't known"))
}

#[test]
fn a_saved_record_is_published_with_its_ids_interned() {
    let store = Store::new();
    let sources = record_pinning(&store, "ws-a", "pub-keys", IdFormat::KeyNumber);
    assert!(CommandSourceId::known("pub-keys").is_none());
    assert_eq!(WorkSourceId::parse("pub-keys"), None);

    save_work_sources(&store.db, &sources).expect("saved");
    let workspace_id = store.ws("ws-a");
    assert_eq!(
        workspace_command(&workspace_id).as_deref(),
        sources.workspaces.get(&workspace_id)
    );
    let source = command_source("pub-keys");
    assert_eq!(source.as_wire(), "pub-keys");
    let rules = work_source(source);
    assert_eq!(rules.name(&scope_in(&workspace_id)), "Fixture");
    assert_eq!(rules.link_kind(), "pub-keys");
    assert_eq!(rules.key("ACM-12", "acme/app"), "pub-keys:ACM-12");
    assert_eq!(rules.parse_ref(" acm-12 ").as_deref(), Some("ACM-12"));
    assert_eq!(rules.parse_ref("218"), None);
    assert_eq!(rules.team_of("ACM-12").as_deref(), Some("ACM"));
    assert_eq!(
        rules.tickets_in_branch(&scope_in(&workspace_id), "dev/acm-12-spec"),
        ["ACM-12"]
    );
    assert!(rules.branch_names("acm-12-spec", "ACM-12"));
    // Another workspace doesn't pin it: none of its keys, and the id as its name.
    let elsewhere = store.ws("ws-b");
    assert!(rules
        .tickets_in_branch(&scope_in(&elsewhere), "acm-12-spec")
        .is_empty());
    assert_eq!(rules.name(&scope_in(&elsewhere)), "pub-keys");
    // Not read yet, and refused where it can't be used.
    assert!(matches!(
        rules.cached_ticket(&scope_in(&workspace_id), "ACM-12"),
        TicketRead::Unread
    ));
    assert_eq!(
        rules
            .read_detail(&scope_in(&workspace_id), "ACM-12", false)
            .unwrap_err(),
        "Ghostex can't open Fixture tickets yet."
    );
    let start =
        crate::work_mode::WorkTicket::of(WorkItemRef::Ticket(TicketRef::new(source, "ACM-12")));
    assert_eq!(
        start.err().map(|error| error.message),
        Some("Ghostex can't start work on pub-keys tickets yet.".to_string())
    );
}

#[test]
fn a_retired_id_keeps_its_format_and_name_until_it_is_pruned() {
    let store = Store::new();
    let mut sources = record_pinning(&store, "ws-a", "pub-retired", IdFormat::Number);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-a");
    sources.workspaces.remove(&workspace_id);
    sources.retired.push(RetiredSource {
        id: "pub-retired".to_string(),
        format: IdFormat::Number,
        name: "Old source".to_string(),
        icon: None,
        retired_at: now_iso(),
    });
    save_work_sources(&store.db, &sources).expect("retired");

    let rules = work_source(command_source("pub-retired"));
    assert!(workspace_command(&workspace_id).is_none());
    assert_eq!(rules.parse_ref("#218").as_deref(), Some("218"));
    assert_eq!(rules.team_of("218"), None);
    assert_eq!(rules.name(&scope_in(&workspace_id)), "Old source");

    sources
        .retired
        .retain(|retired| retired.id != "pub-retired");
    save_work_sources(&store.db, &sources).expect("pruned");
    // Still interned for this process, but with nothing that knows its format.
    assert_eq!(rules.parse_ref("218"), None);
    assert_eq!(rules.name(&scope_in(&workspace_id)), "pub-retired");
}

#[test]
fn a_deletion_is_published_only_when_it_commits() {
    let store = Store::new();
    for workspace in ["ws-commit", "ws-undo"] {
        let sources = record_pinning(&store, workspace, "pub-deleted", IdFormat::KeyNumber);
        save_work_sources(&store.db, &sources).expect("pinned");
    }
    let committed = store.ws("ws-commit");
    let deletion =
        delete_workspace_work_source(&store.db, &store.paths, &committed, &no_references)
            .expect("deleting");
    assert!(workspace_command(&committed).is_some());
    deletion.commit();
    assert!(workspace_command(&committed).is_none());

    let undone = store.ws("ws-undo");
    let deletion = delete_workspace_work_source(&store.db, &store.paths, &undone, &no_references)
        .expect("deleting");
    deletion.undo(&store.paths).expect("undone");
    // The caller rolls its transaction back, so the record keeps the command, and so does the copy.
    assert!(workspace_command(&undone).is_some());
}

#[test]
fn loading_the_store_interns_every_id_it_holds() {
    let store = Store::new();
    let mut sources = record_pinning(&store, "ws-a", "load-active", IdFormat::KeyNumber);
    sources.retired.push(RetiredSource {
        id: "load-retired".to_string(),
        format: IdFormat::Number,
        name: "Old".to_string(),
        icon: None,
        retired_at: now_iso(),
    });
    // Written without publishing, as a record a previous gxserver left.
    write_work_sources(&store.db, &sources).expect("seed");
    assert!(CommandSourceId::known("load-active").is_none());
    assert!(CommandSourceId::known("load-retired").is_none());

    load_work_source_store(&store.db).expect("loaded");
    assert!(CommandSourceId::known("load-active").is_some());
    assert_eq!(
        work_source(command_source("load-retired"))
            .parse_ref("7")
            .as_deref(),
        Some("7")
    );
    assert_eq!(
        workspace_command(&store.ws("ws-a")).map(|command| command.source_id.clone()),
        Some("load-active".to_string())
    );
}

#[test]
fn an_unreadable_store_loads_nothing() {
    let store = Store::new();
    let sources = record_pinning(&store, "ws-a", "load-damaged", IdFormat::KeyNumber);
    write_work_sources(&store.db, &sources).expect("seed");
    let mut damaged: Value = serde_json::from_str(&stored_row(&store)).expect("document");
    damaged["retired"] = json!([{ "id": "load-damaged-retired" }]);
    store
        .db
        .execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'workTicketSources'",
            [&damaged.to_string()],
        )
        .expect("damage row");

    assert!(load_work_source_store(&store.db).is_err());
    assert!(CommandSourceId::known("load-damaged").is_none());
    assert_ne!(
        workspace_command(&store.ws("ws-a")).map(|command| command.source_id.clone()),
        Some("load-damaged".to_string())
    );
}

#[test]
fn a_typed_reference_never_interns_an_id() {
    for typed in ["never-pinned", " never-pinned ", "NEVER-PINNED"] {
        assert_eq!(WorkSourceId::parse(typed), None);
        assert_eq!(WorkSourceId::from_wire(Some(&json!(typed))), None);
    }
    assert!(CommandSourceId::known("never-pinned").is_none());
    let request = json!({ "source": "never-pinned", "ticketId": "ACM-12" });
    assert_eq!(
        TicketRef::from_params(request.as_object().unwrap())
            .expect("names a ticket")
            .unwrap_err(),
        "source must be linear or github."
    );
}

#[test]
fn a_team_or_tracker_field_never_names_a_command() {
    let store = Store::new();
    let sources = record_pinning(&store, "ws-a", "pub-tracker", IdFormat::KeyNumber);
    save_work_sources(&store.db, &sources).expect("pinned");
    assert!(WorkSourceId::from_wire(Some(&json!("pub-tracker"))).is_some());
    assert_eq!(
        WorkSourceId::from_wire_built_in(Some(&json!("pub-tracker"))),
        None
    );
    let workspaces = json!({ "workspaces": { "ws-a": { "tracker": "pub-tracker" } } });
    assert_eq!(stored_workspace_work_tracker(&workspaces, "ws-a"), None);
    assert_eq!(
        WorkSourceId::from_wire_built_in(Some(&json!(" github "))),
        Some(WorkSourceId::Github)
    );
}

#[test]
fn interning_is_bounded_and_all_or_none() {
    let ids = SourceIds::new(2);
    ids.intern_all(["a1", "b2", "a1"]).expect("two fit");
    assert_eq!(ids.len(), 2);
    ids.intern_all(["b2"]).expect("already held");
    assert_eq!(ids.intern_all(["b2", "c3"]), Err(TOO_MANY_SOURCE_IDS));
    assert_eq!(ids.len(), 2);
    assert!(ids.known("c3").is_none());
    assert_eq!(ids.known("a1").map(CommandSourceId::as_str), Some("a1"));
}

#[test]
fn it_can_read_only_with_its_program_and_required_credentials() {
    let store = Store::new();
    let sources = record_pinning(&store, "ws-a", "pub-readable", IdFormat::KeyNumber);
    save_work_sources(&store.db, &sources).expect("pinned");
    let workspace_id = store.ws("ws-a");
    let rules = work_source(command_source("pub-readable"));
    let readable = || {
        let credentials = WorkCredentials::new(&store.paths);
        rules.can_read(&TicketScope {
            effective_workspace_id: Some(&workspace_id),
            credentials: Some(&credentials),
            ..TicketScope::default()
        })
    };
    // The fixture's `token` is required.
    assert!(!readable());
    mutate_credentials(&store.paths, |credentials| {
        let mut workspaces = Map::new();
        workspaces.insert(
            workspace_id.clone(),
            json!({ "pub-readable": { "token": "tok-readable-1" } }),
        );
        credentials.insert(
            "workspaceSourceCredentials".to_string(),
            Value::Object(workspaces),
        );
    })
    .expect("stored");
    assert!(readable());
    std::fs::remove_file(&store.program).expect("remove program");
    assert!(!readable());
    // No credentials to read from: the projection never says it can.
    assert!(!rules.can_read(&scope_in(&workspace_id)));
}

#[test]
fn a_project_scope_reads_commands_in_its_parent_workspace() {
    let mut project = work_project(
        json!({
            "projectId": "PWorktree",
            "name": "app",
            "path": "/gx/app",
            "launchSettings": { "workspaceId": "ws-own" },
        }),
        Vec::new(),
        None,
        "acme/app",
        WorkSourceId::Github,
    );
    project.workspace_id = "ws-parent".to_string();
    let scope = project.ticket_scope();
    assert_eq!(scope.workspace_id, Some("ws-own"));
    assert_eq!(scope.effective_workspace_id, Some("ws-parent"));
}
