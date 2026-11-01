//! What of a command's answer reaches a client while a secret credential is set: display text is
//! masked before it is cut, so no prefix of a secret survives a cap; a structural field holding a
//! secret refuses the row or the answer; a link, a branch name or a time holding one is dropped.

use serde_json::{json, Value};

use super::*;

const SECRET: &str = "s3cr3t-tok3n";

fn env_holding(secret: &str) -> CommandEnv {
    CommandEnv {
        workspace_id: "ws-mask".to_string(),
        credentials: vec![
            CommandCredential {
                name: "token".to_string(),
                value: secret.to_string(),
                secret: true,
            },
            CommandCredential {
                name: "url".to_string(),
                value: "https://tracker.example".to_string(),
                secret: false,
            },
        ],
        ..CommandEnv::default()
    }
}

fn ticket(changes: Value) -> Value {
    let mut ticket = json!({
        "id": "ACM-12",
        "title": "Spec the contract",
        "state": { "group": "todo", "name": "Todo" },
    });
    for (key, value) in changes.as_object().unwrap() {
        ticket[key] = value.clone();
    }
    ticket
}

fn list_of(row: Value, env: &CommandEnv) -> CommandList {
    parse_list(
        &json!({ "tickets": [row] }),
        IdFormat::KeyNumber,
        &[],
        &AnswerGuard::new(env),
    )
    .expect("a list")
}

fn number_list_of(row: Value, env: &CommandEnv) -> CommandList {
    parse_list(
        &json!({ "tickets": [row] }),
        IdFormat::Number,
        &[],
        &AnswerGuard::new(env),
    )
    .expect("a list")
}

#[test]
fn an_echoed_secret_is_masked_in_display_fields() {
    let env = env_holding(SECRET);
    let list = list_of(
        ticket(json!({
            "title": format!("Rotate {SECRET} today"),
            "state": { "group": "todo", "name": format!("To {SECRET}"), "type": SECRET },
            "labels": [format!("l-{SECRET}")],
            "cycle": format!("c {SECRET}"),
            "assignee": { "name": format!("Ana {SECRET}") },
        })),
        &env,
    );
    let masked = &list.tickets[0];
    assert_eq!(masked.title, "Rotate •••• today");
    assert_eq!(masked.status.name, "To ••••");
    assert_eq!(masked.state, "••••");
    assert_eq!(masked.labels, ["l-••••"]);
    assert_eq!(masked.cycle.as_deref(), Some("c ••••"));
    assert_eq!(masked.assignee.as_ref().unwrap().name, "Ana ••••");
    // A value declared not secret is shown in Settings anyway, so it is kept as sent.
    let open = list_of(ticket_titled("Open https://tracker.example"), &env);
    assert_eq!(open.tickets[0].title, "Open https://tracker.example");
}

fn ticket_titled(title: &str) -> Value {
    ticket(json!({ "title": title }))
}

#[test]
fn a_structural_field_holding_a_secret_refuses_its_row() {
    let env = env_holding("acm");
    assert_eq!(
        list_of(ticket(json!({ "projectId": "p-acm" })), &env).skipped,
        1
    );
    assert_eq!(
        list_of(ticket(json!({ "repo": "acme/app" })), &env).skipped,
        1
    );
    // The id, padded or not, and a number source's team are structural.
    let env = env_holding("CM-1");
    assert_eq!(list_of(ticket(json!({})), &env).skipped, 1);
    assert_eq!(
        list_of(ticket(json!({ "id": "  ACM-12  " })), &env).skipped,
        1
    );
    let team = number_list_of(
        ticket(json!({ "id": "12", "team": format!("t-{SECRET}") })),
        &env_holding(SECRET),
    );
    assert_eq!(team.skipped, 1);
    let not_found = parse_list(
        &json!({ "tickets": [], "notFound": ["ACM-7", format!("ACM-{SECRET}")] }),
        IdFormat::KeyNumber,
        &[],
        &AnswerGuard::new(&env_holding(SECRET)),
    )
    .unwrap();
    assert_eq!(not_found.not_found, ["ACM-7"]);
    let unknown = env_holding("zzz-not-there");
    assert_eq!(list_of(ticket(json!({})), &unknown).tickets.len(), 1);
}

#[test]
fn a_secret_in_an_info_id_key_or_credential_name_refuses_the_info() {
    let info = |changes: Value| {
        let mut info = json!({
            "id": "fixture", "name": "Fixture",
            "ids": { "format": "key-number", "keys": ["ACM"] },
            "credentials": [ { "name": "token", "label": "API token" } ],
        });
        for (key, value) in changes.as_object().unwrap() {
            info[key] = value.clone();
        }
        info
    };
    let refused = |answer: Value, secret: &str| {
        parse_info(&answer, &AnswerGuard::new(&env_holding(secret))).unwrap_err()
    };
    assert_eq!(refused(info(json!({})), "fixt"), "id contains a credential");
    assert_eq!(
        refused(info(json!({})), "AC"),
        "ids.keys contains a credential"
    );
    assert_eq!(
        refused(info(json!({})), "oken"),
        "a credential name contains a credential"
    );
    let named = parse_info(
        &info(json!({ "name": format!("Fixture {SECRET}") })),
        &AnswerGuard::new(&env_holding(SECRET)),
    )
    .expect("a name is display text");
    assert_eq!(named.name, "Fixture ••••");
    let icon = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M2 2h20v20H2z\" fill=\"{SECRET}\"/></svg>"
    );
    let with_icon = parse_info(
        &info(json!({ "icon": icon })),
        &AnswerGuard::new(&env_holding(SECRET)),
    )
    .expect("an icon is dropped, not the info");
    assert_eq!(with_icon.icon, Some(Err(ICON_REFUSED.to_string())));
}

#[test]
fn a_link_branch_or_time_holding_a_secret_is_dropped() {
    let env = env_holding("tok3n");
    let list = list_of(
        ticket(json!({
            "url": "https://tracker.example/ACM-12?k=tok3n",
            "branchName": "ana/tok3n-acm-12",
            "pullRequestUrls": ["https://github.com/acme/tok3n/pull/7", "https://github.com/acme/app/pull/8"],
            "updatedAt": "2026-10-28T09:12:00Z",
            "assignee": { "name": "Ana", "avatarUrl": "https://tracker.example/tok3n.png" },
        })),
        &env,
    );
    let kept = &list.tickets[0];
    assert_eq!(kept.url, None);
    assert_eq!(kept.branch_name, None);
    assert_eq!(
        kept.pull_request_urls,
        ["https://github.com/acme/app/pull/8"]
    );
    assert_eq!(kept.assignee.as_ref().unwrap().avatar_url, None);
    assert_eq!(kept.updated_at.as_deref(), Some("2026-10-28T09:12:00Z"));
    let dated = list_of(
        ticket(json!({ "updatedAt": "2026-10-28T09:12:00Z" })),
        &env_holding("10-28"),
    );
    assert_eq!(dated.tickets[0].updated_at, None);
    let created = parse_created(
        &json!({ "id": "ACM-41", "url": "https://tracker.example/tok3n", "branchName": "tok3n" }),
        IdFormat::KeyNumber,
        &AnswerGuard::new(&env),
    )
    .unwrap();
    assert_eq!((created.url, created.branch_name), (None, None));
}

#[test]
fn a_secret_equal_to_a_key_or_a_scheme_leaves_a_valid_answer_valid() {
    for secret in ["id", "http", "title", "state"] {
        let list = list_of(
            ticket(json!({ "url": "https://tracker.example/ACM-12" })),
            &env_holding(secret),
        );
        assert_eq!(list.tickets.len(), 1, "{secret}");
        let kept = &list.tickets[0];
        assert_eq!(kept.id, "ACM-12");
        assert_eq!(kept.status.group, "todo");
        if secret == "http" {
            assert_eq!(kept.url, None, "a link holding a secret is dropped");
        } else {
            assert_eq!(kept.url.as_deref(), Some("https://tracker.example/ACM-12"));
        }
    }
}

#[test]
fn a_secret_straddling_a_cut_never_leaks_its_prefix() {
    let secret = "Q7xK2mP9vL4w";
    let env = env_holding(secret);
    let around = |cap: usize| format!("{}{secret}", "a".repeat(cap - 2));
    let row = ticket(json!({
        "title": around(500),
        "state": { "group": "todo", "name": around(40), "type": around(40) },
        "labels": [around(64)],
        "cycle": around(64),
        "assignee": { "name": around(64) },
    }));
    let list = list_of(row.clone(), &env);
    let detail = parse_detail(
        &ticket(json!({
            "title": around(500),
            "description": around(MAX_TEXT_CHARS),
            "comments": [ { "author": around(64), "body": around(MAX_TEXT_CHARS) } ],
            "priority": around(40),
            "author": around(64),
        })),
        IdFormat::KeyNumber,
        &[],
        &AnswerGuard::new(&env),
    )
    .unwrap();
    let everything = format!("{:?} {:?}", list.tickets, detail);
    for prefix in ["Q7", "Q7x", "Q7xK2m"] {
        assert!(
            !everything.contains(&format!("a{prefix}")),
            "{prefix}: kept a prefix"
        );
    }
    // A credential's label (40) and placeholder (80), through `parse_info`.
    let info = parse_info(
        &json!({
            "id": "fixture", "name": "Fixture",
            "ids": { "format": "number" },
            "credentials": [ { "name": "token", "label": around(40), "placeholder": around(80) } ],
        }),
        &AnswerGuard::new(&env),
    )
    .unwrap();
    let field = format!("{:?}", info.credentials);
    for prefix in ["Q7", "Q7x", "Q7xK2m"] {
        assert!(
            !field.contains(&format!("a{prefix}")),
            "{prefix}: kept a prefix: {field}"
        );
    }
    assert!(info.credentials[0].label.ends_with('…'), "{field}");
    // A padded structural value is checked as sent, before it is trimmed.
    let padded = list_of(ticket(json!({ "id": format!("  ACM-12{secret}  ") })), &env);
    assert_eq!(padded.skipped, 1);
}

#[test]
fn a_display_field_keeps_its_own_cap_while_a_secret_is_set() {
    let env = env_holding(SECRET);
    let title = "t".repeat(500);
    let description = "d".repeat(MAX_TEXT_CHARS);
    let detail = parse_detail(
        &ticket(json!({ "title": title, "description": description })),
        IdFormat::KeyNumber,
        &[],
        &AnswerGuard::new(&env),
    )
    .unwrap();
    assert_eq!(detail.ticket.title, title);
    assert_eq!(detail.description.as_deref(), Some(description.as_str()));
}

#[test]
fn only_a_valid_known_name_marks_its_field_secret() {
    let answer = json!({ "credentials": [
        { "name": "token", "label": "API\ntoken" },
        { "name": "mode", "secret": false },
        { "name": "url", "secret": "yes" },
        { "name": "Bad-Name" },
        { "name": "unknown" },
        "not an object",
    ] });
    assert_eq!(
        credential_secrecy(&answer, &["token", "mode", "url", "Bad-Name"]),
        ["token", "url"]
    );
    assert!(credential_secrecy(&json!({ "credentials": "none" }), &["token"]).is_empty());
}

#[test]
fn widening_never_makes_a_secret_open() {
    let env = env_holding(SECRET).with_secret(&["url".to_string()]);
    assert!(env.credentials.iter().all(|credential| credential.secret));
    let narrowed = env.with_secret(&[]);
    assert!(narrowed
        .credentials
        .iter()
        .all(|credential| credential.secret));
}

#[test]
fn a_command_env_is_built_from_declared_fields_with_values() {
    let declared = [
        CredentialField {
            name: "token".to_string(),
            label: "API token".to_string(),
            secret: true,
            required: true,
            placeholder: None,
        },
        CredentialField {
            name: "url".to_string(),
            label: "URL".to_string(),
            secret: false,
            required: false,
            placeholder: None,
        },
        CredentialField {
            name: "team".to_string(),
            label: "Team".to_string(),
            secret: false,
            required: false,
            placeholder: None,
        },
    ];
    let stored = json!({ "token": "  padded  ", "url": "https://tracker.example", "team": "   ", "gone": "x" });
    let env = CommandEnv::new(
        Some("fixture"),
        "ws-mask",
        None,
        &declared,
        stored.as_object(),
    );
    let names: Vec<(&str, &str, bool)> = env
        .credentials
        .iter()
        .map(|credential| {
            (
                credential.name.as_str(),
                credential.value.as_str(),
                credential.secret,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("token", "padded", true),
            ("url", "https://tracker.example", false)
        ]
    );
    assert!(env.holds_secrets());
    let open_only = CommandEnv::new(None, "ws-mask", None, &declared[1..], stored.as_object());
    assert!(!open_only.holds_secrets());
}
