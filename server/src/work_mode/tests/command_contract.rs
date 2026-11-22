//! What gxserver makes of a command source's JSON: which `info` answers it accepts, how a
//! ticket's fields are checked and cut, and which rows, comments and fields reach clients.

use serde_json::{json, Value};

use super::*;

// The parser with no secret set; the guarded paths are tested in `command_masking.rs`.
fn parse_info(answer: &Value) -> Result<CommandInfo, String> {
    super::parse_info(answer, &AnswerGuard::NONE)
}

fn parse_list(
    answer: &Value,
    format: IdFormat,
    projects: &[CommandProject],
) -> Result<CommandList, String> {
    super::parse_list(answer, format, projects, &AnswerGuard::NONE)
}

fn parse_detail(
    answer: &Value,
    format: IdFormat,
    projects: &[CommandProject],
) -> Result<CommandDetail, String> {
    super::parse_detail(answer, format, projects, &AnswerGuard::NONE)
}

fn parse_created(answer: &Value, format: IdFormat) -> Result<CommandCreated, String> {
    super::parse_created(answer, format, &AnswerGuard::NONE)
}

fn info(changes: Value) -> Value {
    let mut info = json!({
        "id": "lific",
        "name": "Lific",
        "ids": { "format": "key-number", "keys": ["acm"] },
        "credentials": [],
    });
    for (key, value) in changes.as_object().unwrap() {
        info[key] = value.clone();
    }
    info
}

fn project(id: &str, repo: &str) -> CommandProject {
    CommandProject {
        id: id.to_string(),
        name: id.to_string(),
        path: None,
        repo: Some(repo.to_string()),
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

fn one_ticket(row: Value, projects: &[CommandProject]) -> Result<CommandTicket, String> {
    let list = parse_list(&json!({ "tickets": [row] }), IdFormat::KeyNumber, projects)?;
    list.tickets
        .into_iter()
        .next()
        .ok_or_else(|| "skipped".to_string())
}

#[test]
fn info_needs_a_free_well_formed_id_a_name_an_id_format_and_its_credentials() {
    assert!(parse_info(&info(json!({}))).is_ok());
    for (changes, why) in [
        (json!({ "id": "linear" }), "id \"linear\" is reserved"),
        (json!({ "id": "pr" }), "id \"pr\" is reserved"),
        (
            json!({ "id": "Lific" }),
            "id \"Lific\" isn't 2 to 32 lowercase letters, digits or dashes starting with a letter",
        ),
        (json!({ "name": null }), "name is missing"),
        (
            json!({ "ids": { "format": "regex" } }),
            "ids.format isn't key-number or number",
        ),
        (json!({ "credentials": null }), "credentials is missing"),
        (
            json!({ "ids": { "format": "key-number", "keys": ["GH-ST"] } }),
            "ids.keys has an invalid key \"GH-ST\"",
        ),
        (
            json!({ "credentials": [{ "name": "API token", "label": "Token" }] }),
            "credential name \"API token\" isn't lowercase letters, digits or underscores",
        ),
        (
            json!({ "credentials": [
                { "name": "token", "label": "Token", "secret": true },
                { "name": "url", "label": "Server" },
                { "name": "token", "label": "Same name", "secret": false },
            ] }),
            "credential name \"token\" is used twice",
        ),
    ] {
        assert_eq!(
            parse_info(&info(changes.clone())),
            Err(why.to_string()),
            "{changes}"
        );
    }
    let fields: Vec<Value> = (0..9)
        .map(|n| json!({ "name": format!("f{n}"), "label": "F" }))
        .collect();
    assert_eq!(
        parse_info(&info(json!({ "credentials": fields }))),
        Err("credentials has more than 8 fields".to_string())
    );
}

#[test]
fn info_keys_are_uppercase_secrets_are_the_default_and_create_is_off_unless_asked() {
    let parsed = parse_info(&info(json!({
        "credentials": [{ "name": "token", "label": "API token" }],
    })))
    .unwrap();
    assert_eq!(parsed.id_keys, vec!["ACM"]);
    assert_eq!(
        parsed.credentials,
        vec![CredentialField {
            name: "token".to_string(),
            label: "API token".to_string(),
            secret: true,
            required: false,
            placeholder: None,
        }]
    );
    assert!(!parsed.can_create);
    assert_eq!(parsed.icon, None);
    let numbers = parse_info(&info(
        json!({ "ids": { "format": "number", "keys": ["X"] } }),
    ))
    .unwrap();
    assert_eq!(numbers.id_format, IdFormat::Number);
    assert!(numbers.id_keys.is_empty());
}

#[test]
fn a_ticket_is_cut_to_its_caps_and_keeps_only_safe_links() {
    let long_title = "t".repeat(600);
    let parsed = one_ticket(
        ticket(json!({
            "title": long_title,
            "url": "javascript:alert(1)",
            "updatedAt": "yesterday",
            "assignee": { "name": "Ana", "avatarUrl": "http://insecure/a.png" },
            "labels": (0..25).map(|n| format!("label-{n}")).collect::<Vec<_>>(),
            "pullRequestUrls": ["https://github.com/acme/app/pull/7", "https://gitlab.com/acme/app/-/merge_requests/3"],
            "state": { "group": "review", "name": "In review with a much longer name than forty chars" },
        })),
        &[],
    )
    .unwrap();
    assert_eq!(parsed.title.chars().count(), 501, "500 and an ellipsis");
    assert_eq!(parsed.url, None);
    assert_eq!(parsed.updated_at, None);
    let assignee = parsed.assignee.unwrap();
    assert_eq!(
        (assignee.name.as_str(), assignee.avatar_url, assignee.is_me),
        ("Ana", None, false)
    );
    assert_eq!(parsed.labels.len(), 20);
    assert_eq!(
        parsed.pull_request_urls,
        vec!["https://github.com/acme/app/pull/7"]
    );
    assert_eq!(parsed.status.group, "review");
    assert_eq!(parsed.status.name.chars().count(), 41);
    assert_eq!(parsed.state, parsed.status.name, "no type: the name");
}

#[test]
fn a_pull_request_url_is_exactly_a_github_pull_request_of_at_most_the_url_cap() {
    let kept = |url: &str| {
        one_ticket(ticket(json!({ "pullRequestUrls": [url] })), &[])
            .unwrap()
            .pull_request_urls
    };
    for url in [
        "https://github.com/acme/app/pull/7",
        "https://github.com/acme-co/my.app_1/pull/1234",
    ] {
        assert_eq!(kept(url), vec![url]);
    }
    // The URL cap counts the whole link: 2048 bytes are kept, 2049 are not.
    let at_cap = |len: usize| {
        let short = "https://github.com//b/pull/1".len();
        format!("https://github.com/{}/b/pull/1", "a".repeat(len - short))
    };
    assert_eq!(kept(&at_cap(2048)), vec![at_cap(2048)]);
    assert_eq!(kept(&at_cap(2049)), Vec::<String>::new());
    for url in [
        // A ticket's first URL is the selector of `gh pr view`, so this would read as its flag.
        "--repo=a/github.com/x/pull/1",
        "-R/github.com/x/pull/1",
        "javascript:alert(1)//github.com/a/pull/1",
        "https://evil.example/github.com/acme/app/pull/7",
        "https://github.com.evil.example/acme/app/pull/7",
        "https://user@github.com/acme/app/pull/7",
        "http://github.com/acme/app/pull/7",
        "HTTPS://github.com/acme/app/pull/7",
        " https://github.com/acme/app/pull/7",
        "https://github.com/acme/app/pull/7/files",
        "https://github.com/acme/app/pull/7/",
        "https://github.com/acme/app/pull/7?x=1",
        "https://github.com/acme/app/pull/7#top",
        "https://github.com/acme/app/pull/",
        "https://github.com/acme/app/pull/x",
        "https://github.com/acme/app/pull/+7",
        "https://github.com/acme/app/issues/7",
        "https://github.com/acme/pull/7",
        "https://github.com/acme/app/more/pull/7",
        "https://github.com//app/pull/7",
        "https://github.com/acme/../pull/7",
        "https://github.com/acme/a b/pull/7",
        "https://github.com/ac%6de/app/pull/7",
    ] {
        assert_eq!(kept(url), Vec::<String>::new(), "{url}");
    }
    // A number too large for any PR can't be read back as one.
    assert_eq!(
        kept(&format!(
            "https://github.com/acme/app/pull/{}",
            "9".repeat(30)
        )),
        Vec::<String>::new()
    );
}

#[test]
fn a_ticket_row_is_skipped_for_an_id_not_in_canonical_form_or_an_unknown_group() {
    for row in [
        ticket(json!({ "id": "acm-12" })),
        ticket(json!({ "id": "-x" })),
        ticket(json!({ "state": { "group": "closed", "name": "Closed" } })),
        ticket(json!({ "state": { "group": "merged", "name": "Merged" } })),
        ticket(json!({ "title": "" })),
    ] {
        assert!(one_ticket(row.clone(), &[]).is_err(), "{row}");
    }
    let numbers = parse_list(
        &json!({ "tickets": [ticket(json!({ "id": "218" })), ticket(json!({ "id": "#219" }))] }),
        IdFormat::Number,
        &[],
    )
    .unwrap();
    assert_eq!(numbers.tickets.len(), 1);
    assert_eq!(numbers.skipped, 1);
}

#[test]
fn a_ticket_belongs_to_the_project_it_names_or_whose_repo_it_names() {
    let projects = [project("PCmdA", "acme/a"), project("PCmdB", "acme/b")];
    let project_of = |changes| one_ticket(ticket(changes), &projects).unwrap().project_id;
    assert_eq!(
        project_of(json!({ "projectId": "PCmdB", "repo": "acme/a" })).as_deref(),
        Some("PCmdB")
    );
    assert_eq!(
        project_of(json!({ "projectId": "PCmdElsewhere", "repo": "Acme/A" })).as_deref(),
        Some("PCmdA")
    );
    assert_eq!(project_of(json!({ "repo": "acme/c" })), None);
}

#[test]
fn a_long_list_keeps_its_first_200_rows_and_says_so() {
    let rows: Vec<Value> = (1..=205)
        .map(|n| ticket(json!({ "id": format!("ACM-{n}") })))
        .collect();
    let list = parse_list(
        &json!({ "tickets": rows, "notFound": ["acm-300", "nonsense"] }),
        IdFormat::KeyNumber,
        &[],
    )
    .unwrap();
    assert_eq!(list.tickets.len(), 200);
    assert_eq!(list.tickets.last().unwrap().id, "ACM-200");
    assert_eq!(list.not_found, vec!["ACM-300"]);
    assert_eq!(
        list.errors("Lific"),
        vec!["Lific: showed the first 200 tickets"]
    );
    assert_eq!(
        parse_list(&json!({ "notFound": [] }), IdFormat::KeyNumber, &[]).err(),
        Some("tickets is missing".to_string())
    );
}

#[test]
fn a_detail_keeps_the_newest_comments_and_only_the_allowlisted_fields() {
    let comments: Vec<Value> = (1..=35)
        .map(|n| json!({ "author": "Ana", "body": format!("comment {n}"), "createdAt": "2026-10-28T08:00:00Z" }))
        .chain([json!({ "body": "no author" })])
        .collect();
    let detail = parse_detail(
        &ticket(json!({
            "description": "d".repeat(20_005),
            "priority": "High",
            "createdAt": "2026-10-27T16:47:26Z",
            "comments": comments,
            "internalNotes": "never shown",
        })),
        IdFormat::KeyNumber,
        &[],
    )
    .unwrap();
    assert_eq!(detail.comments.len(), 30);
    assert_eq!(detail.comments[0]["body"], json!("comment 6"));
    assert_eq!(detail.comments[29]["body"], json!("comment 35"));
    assert_eq!(detail.comment_count, 35, "the comments with an author");
    assert_eq!(detail.description.unwrap().chars().count(), 20_001);
    let mut fields: Vec<&str> = detail.fields.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        vec![
            "assignee",
            "author",
            "branchName",
            "createdAt",
            "cycle",
            "labels",
            "priority",
            "pullRequestUrls",
            "updatedAt"
        ]
    );
    assert_eq!(detail.fields["priority"], json!("High"));
    let counted = parse_detail(
        &ticket(json!({ "commentCount": 31, "comments": [] })),
        IdFormat::KeyNumber,
        &[],
    )
    .unwrap();
    assert_eq!(counted.comment_count, 31);
}

#[test]
fn a_created_ticket_needs_a_canonical_id() {
    assert_eq!(
        parse_created(
            &json!({ "id": "ACM-41", "url": "ftp://x" }),
            IdFormat::KeyNumber
        ),
        Ok(CommandCreated {
            id: "ACM-41".to_string(),
            url: None,
            branch_name: None,
        })
    );
    assert!(parse_created(&json!({ "id": "41" }), IdFormat::KeyNumber).is_err());
    assert!(parse_created(&json!({}), IdFormat::Number).is_err());
}

#[test]
fn an_error_object_names_its_code_and_its_whole_message() {
    let long = "m".repeat(400);
    for (code, expected) in [
        ("auth", ErrorCode::Auth),
        ("notFound", ErrorCode::NotFound),
        ("rateLimited", ErrorCode::RateLimited),
        ("invalid", ErrorCode::Invalid),
        ("teapot", ErrorCode::Other),
    ] {
        let (parsed, message) =
            reported_error(&json!({ "error": { "code": code, "message": long } })).unwrap();
        assert_eq!(parsed, expected);
        // Cut only after the runner masks credentials in it.
        assert_eq!(message, long);
    }
    assert_eq!(reported_error(&json!({ "error": "plain" })), None);
    assert_eq!(
        reported_error(&json!({ "error": { "code": "auth" } })),
        None
    );
}

#[test]
fn the_inputs_carry_the_projects_the_ids_and_the_draft() {
    let projects = [project("PCmdA", "acme/a")];
    assert_eq!(
        list_input(&projects, Some(&["ACM-1".to_string()])),
        json!({
            "projects": [{ "id": "PCmdA", "name": "PCmdA", "path": null, "repo": "acme/a" }],
            "ids": ["ACM-1"],
        })
    );
    assert_eq!(list_input(&[], None), json!({ "projects": [] }));
    let extra = serde_json::Map::from_iter([("teamId".to_string(), json!("t1"))]);
    let title = format!("  {}  ", "x".repeat(300));
    let input = create_input(
        &TicketDraft {
            title: &title,
            description: Some("Body"),
            assign_to_me: false,
            extra: &extra,
        },
        None,
    );
    assert_eq!(input["title"].as_str().unwrap().chars().count(), 257);
    assert_eq!(input["project"], Value::Null);
    assert!(input.get("teamId").is_none() && input.get("extra").is_none());
}

#[test]
fn a_state_word_is_cut_and_a_branch_name_or_id_past_its_cap_is_dropped() {
    let huge = "w".repeat(2 * 1024 * 1024);
    let parsed = one_ticket(
        ticket(json!({
            "state": { "group": "todo", "name": "Todo", "type": huge },
            "branchName": "b".repeat(256),
        })),
        &[],
    )
    .unwrap();
    assert_eq!(parsed.state.chars().count(), 41, "40 and an ellipsis");
    assert_eq!(parsed.branch_name, None);
    let kept = one_ticket(ticket(json!({ "branchName": "b".repeat(200) })), &[]).unwrap();
    assert_eq!(kept.branch_name.map(|branch| branch.len()), Some(200));
    let too_long = one_ticket(ticket(json!({ "branchName": "b".repeat(201) })), &[]).unwrap();
    assert_eq!(too_long.branch_name, None);

    let created = parse_created(
        &json!({ "id": "ACM-41", "branchName": "b".repeat(64 * 1024) }),
        IdFormat::KeyNumber,
    )
    .unwrap();
    assert_eq!(created.branch_name, None);

    let long_id = format!("ACM-{}", "1".repeat(61));
    assert!(one_ticket(ticket(json!({ "id": long_id })), &[]).is_err());
    assert_eq!(IdFormat::KeyNumber.parse_ref(&long_id), None);
    let longest_id = format!("ACM-{}", "1".repeat(60));
    assert!(one_ticket(ticket(json!({ "id": longest_id })), &[]).is_ok());
}

#[test]
fn a_branch_name_start_work_would_refuse_is_dropped() {
    for refused in [
        "--upload-pack=x",
        "-b",
        "café",
        "a..b",
        "a//b",
        "a/",
        "a b",
        ".hidden",
    ] {
        let parsed = one_ticket(ticket(json!({ "branchName": refused })), &[]).unwrap();
        assert_eq!(parsed.branch_name, None, "{refused}");
        assert!(
            !crate::server::is_allowed_worktree_branch(refused),
            "{refused}"
        );
    }
    let kept = one_ticket(ticket(json!({ "branchName": "ana/acm-12-spec" })), &[]).unwrap();
    assert_eq!(kept.branch_name.as_deref(), Some("ana/acm-12-spec"));
    let created = parse_created(
        &json!({ "id": "ACM-41", "branchName": "--upload-pack=x" }),
        IdFormat::KeyNumber,
    )
    .unwrap();
    assert_eq!(created.branch_name, None);
}

#[test]
fn a_time_longer_than_any_real_one_is_dropped_before_it_is_parsed() {
    let longest = "2026-10-28T09:12:00.123456789+05:30";
    let kept = one_ticket(ticket(json!({ "updatedAt": longest })), &[]).unwrap();
    assert_eq!(kept.updated_at.as_deref(), Some(longest));
    // Valid RFC 3339, with a fraction of a second a megabyte long.
    let huge = format!("2026-10-28T09:12:00.{}Z", "1".repeat(1024 * 1024));
    assert!(chrono::DateTime::parse_from_rfc3339(&huge).is_ok());
    let dropped = one_ticket(ticket(json!({ "updatedAt": huge })), &[]).unwrap();
    assert_eq!(dropped.updated_at, None);
    let detail = parse_detail(
        &ticket(json!({ "createdAt": format!("2026-10-28T09:12:00.{}Z", "1".repeat(64)) })),
        IdFormat::KeyNumber,
        &[],
    )
    .unwrap();
    assert_eq!(detail.fields["createdAt"], Value::Null);
}

#[test]
fn a_name_label_or_placeholder_with_a_control_character_is_refused() {
    for name in ["Two\nlines", "Tab\there", "Bell\u{7}"] {
        assert_eq!(
            parse_info(&info(json!({ "name": name }))),
            Err("name has a line break or another control character".to_string()),
            "{name:?}"
        );
    }
    let field = |label: &str, placeholder: &str| {
        info(json!({ "credentials": [
            { "name": "token", "label": label, "placeholder": placeholder }
        ] }))
    };
    assert_eq!(
        parse_info(&field("API\ntoken", "x")),
        Err("a credential label has a line break or another control character".to_string())
    );
    assert_eq!(
        parse_info(&field("API token", "paste\rit")),
        Err("a credential placeholder has a line break or another control character".to_string())
    );
    assert!(parse_info(&field("API token", "paste it here")).is_ok());
    // Spaces around a value are trimmed, not refused.
    assert!(parse_info(&info(json!({ "name": "  Fixture\n" }))).is_ok());
}
