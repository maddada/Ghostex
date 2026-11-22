use super::cursor::cursor_encoded_path_segment;
use super::*;
use crate::agent::Agent;

fn scanner() -> Scanner {
    Scanner::new("/nonexistent-home", "/nonexistent-cache")
}

#[test]
fn parses_claude_history_rows() {
    let mut s = scanner();
    s.parse_claude_history(
        br#"{"display":"fix the parser","project":"/tmp/x","sessionId":"abc","timestamp":1750000000000}
not json
{"display":"","project":"/tmp/x"}
{"display":"second","sessionId":"def"}"#,
    );
    assert_eq!(s.records.len(), 2);
    assert_eq!(s.records[0].text, "fix the parser");
    assert_eq!(s.records[0].project, "/tmp/x");
    assert_eq!(s.records[0].session, "abc");
    assert_eq!(s.records[0].ts, 1_750_000_000);
    assert_eq!(s.records[1].text, "second");
    assert_eq!(s.records[1].ts, 0);
}

#[test]
fn parses_codex_session_user_messages_and_metadata() {
    let mut s = scanner();
    s.parse_codex_session(
        br#"{"type":"session_meta","payload":{"id":"sess-1","cwd":"/work/app","model_provider":"openai","title":"Rework login"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add a test"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"text","text":"sure"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"text","text":"<environment_context>skip me"}]}}"#,
    );
    assert_eq!(s.records.len(), 1);
    let rec = &s.records[0];
    assert_eq!(rec.agent, Agent::Codex);
    assert_eq!(rec.text, "add a test");
    assert_eq!(rec.project, "/work/app");
    assert_eq!(rec.session, "sess-1");
    assert_eq!(rec.title, "Rework login");
    assert_eq!(rec.meta.provider, "openai");
}

#[test]
fn codex_token_counts_land_on_session_records() {
    let mut s = scanner();
    s.parse_codex_session(
        br#"{"type":"session_meta","payload":{"id":"s","cwd":"/w"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":"hello"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":200000,"total_token_usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}},"rate_limits":{"primary":{"used_percent":12.5},"plan_type":"pro"}}}"#,
    );
    assert_eq!(s.records.len(), 1);
    assert_eq!(s.records[0].meta.usage.input, 10);
    assert_eq!(s.records[0].meta.usage.context_window, 200_000);
    assert_eq!(s.records[0].meta.usage.rate_percent, 12.5);
    assert_eq!(s.records[0].meta.plan, "pro");
}

#[test]
fn parses_pi_sessions_with_titles_and_usage() {
    let mut s = scanner();
    s.parse_pi_session(
        br#"{"type":"session","id":"pi-1","cwd":"/p","title":"Naming things","timestamp":1750000000}
{"type":"message","message":{"role":"user","content":"rename the module"}}
{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"opus","usage":{"input":3,"output":4,"cost":{"total":0.5}}}}
{"type":"message","message":{"role":"user","content":[{"type":"text","text":"again"}]}}"#,
    );
    assert_eq!(s.records.len(), 2);
    assert_eq!(s.records[0].title, "Naming things");
    assert_eq!(s.records[0].session, "pi-1");
    assert_eq!(s.records[0].ts, 1_750_000_000);
    assert_eq!(s.records[1].text, "again");
    assert_eq!(s.records[1].meta.model, "opus");
    assert_eq!(s.records[0].meta.usage.cost, 0.5);
}

#[test]
fn parses_cursor_and_grok_transcripts() {
    let mut s = scanner();
    s.parse_cursor_transcript(
        br#"{"role":"user","message":{"content":[{"type":"text","text":"cursor prompt"}]}}
{"role":"assistant","message":{"content":"nope"}}"#,
        "/proj",
        "Cursor thread",
        "cur-1",
        42,
    );
    assert_eq!(s.records.len(), 1);
    assert_eq!(s.records[0].agent, Agent::Cursor);
    assert_eq!(s.records[0].ts, 42);

    let info = parse_grok_summary(
        br#"{"info":{"id":"grok-1","cwd":"/g"},"title":"Grok thread","current_model_id":"grok-4","updated_at":"2026-08-20T10:11:12Z"}"#,
        "fallback",
    );
    assert_eq!(info.session, "grok-1");
    assert_eq!(info.project, "/g");
    assert_eq!(info.model, "grok-4");
    assert!(info.ts > 0);
    s.parse_grok_chat_history(br#"{"type":"user","content":"grok prompt"}"#, &info);
    assert_eq!(s.records.len(), 2);
    assert_eq!(s.records[1].agent, Agent::Grok);
    assert_eq!(s.records[1].meta.provider, "xai");
}

#[test]
fn parses_opencode_rows() {
    let mut s = scanner();
    s.parse_opencode(
        br#"{"role":"user","type":"text","text":"opencode prompt","project":"/o","session":"oc-1","provider":"anthropic","model":"sonnet","ts":1750000000000}
{"role":"assistant","type":"text","text":"reply"}"#,
    );
    assert_eq!(s.records.len(), 1);
    assert_eq!(s.records[0].ts, 1_750_000_000);
    assert_eq!(s.records[0].meta.model, "sonnet");
}

#[test]
fn dedup_keeps_the_most_recent_occurrence_per_agent_and_text() {
    let mut s = scanner();
    let mk = |agent, text: &str, ts| Record {
        agent,
        kind: RecordKind::User,
        interrupted: false,
        title: String::new(),
        text: text.to_string(),
        project: String::new(),
        session: String::new(),
        ts,
        meta: Meta::default(),
    };
    s.records = vec![
        mk(Agent::Claude, "same", 10),
        mk(Agent::Codex, "same", 20),
        mk(Agent::Claude, "same", 30),
        mk(Agent::Claude, "other", 5),
    ];
    s.dedup();
    assert_eq!(s.records.len(), 3);
    assert_eq!(s.records[0].ts, 30);
    assert_eq!(s.records[1].agent, Agent::Codex);
    assert_eq!(s.records[2].text, "other");
}

#[test]
fn timestamps_normalize_millis_and_iso_strings() {
    assert_eq!(normalize_timestamp(1_750_000_000_000), 1_750_000_000);
    assert_eq!(normalize_timestamp(1_750_000_000), 1_750_000_000);
    assert_eq!(normalize_timestamp(-4), 0);
    assert_eq!(parse_iso8601_seconds("1970-01-01T00:00:00Z"), 0);
    assert_eq!(parse_iso8601_seconds("1970-01-02T00:00:01Z"), 86_401);
    assert_eq!(parse_iso8601_seconds("short"), 0);
}

#[test]
fn civil_date_round_trips() {
    for day in [-1000i64, 0, 19_000, 20_684] {
        let d = civil_from_day_key(day);
        assert_eq!(days_from_civil(d.year, d.month as i64, d.day as i64), day);
    }
}

#[test]
fn titles_reject_control_characters_and_replacement_chars() {
    assert_eq!(clean_title("  Fine title  ").as_deref(), Some("Fine title"));
    assert_eq!(clean_title(""), None);
    assert_eq!(clean_title("bad\u{7}title"), None);
    assert_eq!(clean_title("moji\u{fffd}bake"), None);
}

#[test]
fn cursor_encoded_segments_collapse_non_alphanumerics() {
    assert_eq!(cursor_encoded_path_segment("_active"), "active");
    assert_eq!(
        cursor_encoded_path_segment("my project.v2"),
        "my-project-v2"
    );
    assert_eq!(cursor_encoded_path_segment("--"), "");
}

#[test]
fn project_display_name_uses_the_last_segment() {
    assert_eq!(project_display_name("/a/b/Ghostex"), "Ghostex");
    assert_eq!(project_display_name("/a/b/Ghostex/"), "Ghostex");
    assert_eq!(project_display_name(""), "");
}
