use serde_json::{json, Value};

use super::{create_sidebar_hud_settings_mutation, read_sidebar_hud};

fn command_ids(hud: &Value) -> Vec<&str> {
    hud.get("commands")
        .and_then(|value| value.as_array())
        .unwrap()
        .iter()
        .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
}

#[test]
fn hides_hidden_default_agents_until_stored() {
    let hud = read_sidebar_hud(&[], None);
    let agents = hud
        .get("agents")
        .and_then(|value| value.as_array())
        .unwrap();
    assert!(agents
        .iter()
        .all(|agent| agent.get("agentId").and_then(|value| value.as_str()) != Some("rovodev")));

    let hud = read_sidebar_hud(
        &[json!({
            "customAgents": [
                {
                    "agentId": "rovodev",
                    "command": "acli rovodev run",
                    "isDefault": true,
                    "name": "Rovo Dev"
                }
            ],
            "customAgentOrder": ["rovodev", "codex"],
            "projectId": "P1a"
        })],
        None,
    );
    let agents = hud
        .get("agents")
        .and_then(|value| value.as_array())
        .unwrap();
    assert_eq!(
        agents
            .first()
            .and_then(|agent| agent.get("agentId"))
            .and_then(|value| value.as_str()),
        Some("rovodev")
    );
}

#[test]
fn validates_custom_command_runnable_fields_and_deleted_defaults() {
    let hud = read_sidebar_hud(
        &[json!({
            "customCommandOrder": ["browser-good", "terminal-good", "build"],
            "customCommands": [
                {
                    "actionType": "browser",
                    "commandId": "browser-bad",
                    "name": "Bad Browser"
                },
                {
                    "actionType": "browser",
                    "commandId": "browser-good",
                    "name": "Browser",
                    "url": "https://example.test"
                },
                {
                    "actionType": "terminal",
                    "commandId": "terminal-bad",
                    "name": "Bad Terminal"
                },
                {
                    "actionType": "terminal",
                    "command": "bun run dev",
                    "commandId": "terminal-good",
                    "name": "Dev"
                }
            ],
            "deletedDefaultCommandIds": ["build"],
            "projectId": "P1a"
        })],
        None,
    );
    let commands = hud
        .get("commands")
        .and_then(|value| value.as_array())
        .unwrap();
    let command_ids = commands
        .iter()
        .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(command_ids[0], "browser-good");
    assert_eq!(command_ids[1], "terminal-good");
    assert!(!command_ids.contains(&"browser-bad"));
    assert!(!command_ids.contains(&"terminal-bad"));
    assert!(!command_ids.contains(&"build"));
}

#[test]
fn scopes_commands_to_active_project_owner() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo test",
                    "commandId": "parent-test",
                    "name": "Parent Test"
                }
            ],
            "customCommandOrder": ["parent-test"],
            "projectId": "Pparent"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "bun test",
                    "commandId": "worktree-test",
                    "name": "Worktree Test"
                }
            ],
            "projectId": "Pworktree",
            "worktree": { "parentProjectId": "Pparent" }
        }),
    ];
    let hud = read_sidebar_hud(&projects, Some("Pworktree"));
    let command_ids = hud
        .get("commands")
        .and_then(|value| value.as_array())
        .unwrap()
        .iter()
        .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(command_ids.first().copied(), Some("parent-test"));

    let hud = read_sidebar_hud(&projects, Some("Pmissing"));
    let command_ids = hud
        .get("commands")
        .and_then(|value| value.as_array())
        .unwrap()
        .iter()
        .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(command_ids, vec!["dev", "build", "test", "setup"]);
}

#[test]
fn skips_explicit_recent_project_for_no_active_command_source() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo check",
                    "commandId": "parked-check",
                    "name": "Parked Check"
                }
            ],
            "customCommandOrder": ["parked-check"],
            "isRecentProject": true,
            "projectId": "Precent"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo test",
                    "commandId": "normal-test",
                    "name": "Normal Test"
                }
            ],
            "customCommandOrder": ["normal-test"],
            "projectId": "Pnormal"
        }),
    ];

    let hud = read_sidebar_hud(&projects, None);
    let command_ids = command_ids(&hud);
    assert_eq!(command_ids.first().copied(), Some("normal-test"));
    assert!(!command_ids.contains(&"parked-check"));
}

#[test]
fn explicit_active_recent_project_does_not_expose_custom_commands() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo check",
                    "commandId": "parked-check",
                    "name": "Parked Check"
                }
            ],
            "customCommandOrder": ["parked-check"],
            "isRecentProject": true,
            "projectId": "Precent"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo test",
                    "commandId": "normal-test",
                    "name": "Normal Test"
                }
            ],
            "customCommandOrder": ["normal-test"],
            "projectId": "Pnormal"
        }),
    ];

    let hud = read_sidebar_hud(&projects, Some("Precent"));
    assert_eq!(command_ids(&hud), vec!["dev", "build", "test", "setup"]);
}

#[test]
fn non_true_recent_flags_remain_normal_command_sources() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo check",
                    "commandId": "false-flag-check",
                    "name": "False Flag Check"
                }
            ],
            "customCommandOrder": ["false-flag-check"],
            "isRecentProject": false,
            "projectId": "Pfalse"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo test",
                    "commandId": "missing-flag-test",
                    "name": "Missing Flag Test"
                }
            ],
            "customCommandOrder": ["missing-flag-test"],
            "projectId": "Pmissing"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo build",
                    "commandId": "non-boolean-flag-build",
                    "name": "Non Boolean Flag Build"
                }
            ],
            "customCommandOrder": ["non-boolean-flag-build"],
            "isRecentProject": "true",
            "projectId": "PnonBoolean"
        }),
    ];

    let hud = read_sidebar_hud(&projects, None);
    assert_eq!(command_ids(&hud).first().copied(), Some("false-flag-check"));

    let hud = read_sidebar_hud(&projects, Some("Pmissing"));
    assert_eq!(
        command_ids(&hud).first().copied(),
        Some("missing-flag-test")
    );

    let hud = read_sidebar_hud(&projects, Some("PnonBoolean"));
    assert_eq!(
        command_ids(&hud).first().copied(),
        Some("non-boolean-flag-build")
    );
}

#[test]
fn worktree_owner_resolution_skips_explicit_recent_project_rows() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo check",
                    "commandId": "parked-parent-check",
                    "name": "Parked Parent Check"
                }
            ],
            "customCommandOrder": ["parked-parent-check"],
            "isRecentProject": true,
            "projectId": "Pparent"
        }),
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo test",
                    "commandId": "worktree-test",
                    "name": "Worktree Test"
                }
            ],
            "customCommandOrder": ["worktree-test"],
            "projectId": "Pworktree",
            "worktree": { "parentProjectId": "Pparent" }
        }),
    ];

    let hud = read_sidebar_hud(&projects, Some("Pworktree"));
    let command_ids = command_ids(&hud);
    assert_eq!(command_ids.first().copied(), Some("worktree-test"));
    assert!(!command_ids.contains(&"parked-parent-check"));
}

#[test]
fn command_mutation_scope_skips_explicit_recent_project_rows() {
    let projects = vec![
        json!({
            "customCommands": [
                {
                    "actionType": "terminal",
                    "command": "cargo check",
                    "commandId": "parked-parent-check",
                    "name": "Parked Parent Check"
                }
            ],
            "isRecentProject": true,
            "projectId": "Pparent"
        }),
        json!({
            "projectId": "Pworktree",
            "worktree": { "parentProjectId": "Pparent" }
        }),
    ];
    let params = json!({
        "activeProjectId": "Pworktree",
        "actionType": "terminal",
        "command": "cargo test",
        "name": "Workspace Verify",
        "operation": "save",
        "target": "command"
    })
    .as_object()
    .unwrap()
    .clone();

    let mutation = create_sidebar_hud_settings_mutation(&projects, &params).unwrap();
    assert_eq!(mutation.updates.len(), 1);
    assert_eq!(mutation.updates[0].project_id, "Pworktree");

    let params = json!({
        "activeProjectId": "Pparent",
        "actionType": "terminal",
        "command": "cargo test",
        "name": "Workspace Verify",
        "operation": "save",
        "target": "command"
    })
    .as_object()
    .unwrap()
    .clone();
    assert!(create_sidebar_hud_settings_mutation(&projects, &params).is_err());
}

/*
CDXC:AgentLauncher 2026-08-01-19:00:
A Global Action may not claim a reserved built-in id. The read projection
recomputes isDefault from the id, so a stored global "dev" would come back
marked as a default however it was written, and a run-by-id selector could
not tell it apart from the project action of the same name.
*/
#[test]
fn global_command_save_rejects_reserved_default_ids() {
    let projects = Vec::new();
    for command_id in ["dev", "build", "test", "setup"] {
        let params = json!({
            "actionType": "terminal",
            "command": "echo hi",
            "commandId": command_id,
            "name": "Reserved",
            "operation": "save",
            "target": "globalCommand"
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(
            create_sidebar_hud_settings_mutation(&projects, &params).is_err(),
            "expected reserved global command id {command_id} to be rejected"
        );
    }
}

#[test]
fn global_command_save_needs_no_project() {
    let projects = Vec::new();
    let params = json!({
        "actionType": "terminal",
        "command": "gh pr list",
        "name": "PRs",
        "operation": "save",
        "target": "globalCommand"
    })
    .as_object()
    .unwrap()
    .clone();
    let mutation = create_sidebar_hud_settings_mutation(&projects, &params)
        .expect("global saves must not require an active project");
    assert!(mutation.updates.is_empty());
    assert!(mutation.global_command_update.is_some());
}
