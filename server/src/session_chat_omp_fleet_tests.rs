//! Synthetic OMP 18.13.3 transcripts: the record shapes OMP writes, with made-up content.

use super::*;
use serde_json::json;
use std::io::Write;

fn at(second: u32) -> String {
    format!("2026-10-30T10:00:{second:02}.000Z")
}

fn ms(second: u32) -> i64 {
    parse_time(&at(second)).unwrap()
}

fn append(path: &Path, rows: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    for row in rows {
        writeln!(file, "{row}").unwrap();
    }
}

fn title() -> Value {
    json!({"type":"title","v":1,"title":"","updatedAt":at(0),"pad":"        "})
}

fn header(id: &str, second: u32) -> Value {
    json!({"type":"session","version":3,"id":id,"timestamp":at(second),"cwd":"/work",
        "parentSession":"/sessions/2026-10-30T10-00-00-000Z_parent.jsonl"})
}

fn model_change(model: &str, second: u32) -> Value {
    json!({"type":"model_change","id":format!("m{second}"),"parentId":null,"timestamp":at(second),"model":model})
}

fn thinking(level: &str, second: u32) -> Value {
    json!({"type":"thinking_level_change","id":format!("t{second}"),"parentId":null,"timestamp":at(second),"thinkingLevel":level})
}

fn user(text: &str, second: u32) -> Value {
    json!({"type":"message","id":format!("u{second}"),"parentId":null,"timestamp":at(second),
        "message":{"role":"user","content":[{"type":"text","text":text}],"timestamp":ms(second)}})
}

fn assistant(stop: &str, tool: Option<&str>, second: u32) -> Value {
    let content = match tool {
        Some(name) => {
            json!([{"type":"toolCall","id":format!("c{second}"),"name":name,"arguments":{}}])
        }
        None => json!([{"type":"text","text":"Done."}]),
    };
    json!({"type":"message","id":format!("a{second}"),"parentId":null,"timestamp":at(second),
        "message":{"role":"assistant","content":content,"api":"anthropic-messages","provider":"anthropic",
            "model":"claude-sonnet-4-5","usage":{"input":1,"output":1,"totalTokens":2},"stopReason":stop,"timestamp":ms(second)}})
}

fn tool_result(tool: &str, details: Value, second: u32) -> Value {
    json!({"type":"message","id":format!("r{second}"),"parentId":null,"timestamp":at(second),
        "message":{"role":"toolResult","toolCallId":format!("c{}", second - 1),"toolName":tool,
            "content":[{"type":"text","text":"ok"}],"details":details,"isError":false,"timestamp":ms(second)}})
}

fn async_result(second: u32) -> Value {
    json!({"type":"custom_message","id":format!("x{second}"),"parentId":null,"timestamp":at(second),
        "customType":"async-result","content":"Background job finished.","display":true})
}

fn session_exit(second: u32) -> Value {
    json!({"type":"custom","id":format!("e{second}"),"parentId":null,"timestamp":at(second),"customType":"session_exit",
        "data":{"reason":"dispose","kind":"normal","recordedAt":at(second)}})
}

/// A spawned child up to its first prompt: title slot, header, model, thinking level, `session_init`, prompt.
fn opening(id: &str, agent: &str, task: &str, second: u32) -> Vec<Value> {
    vec![
        title(),
        header(id, second),
        model_change("anthropic/claude-sonnet-4-5", second),
        thinking("medium", second),
        json!({"type":"session_init","id":"s1","parentId":null,"timestamp":at(second),
            "systemPrompt":["You are a subagent."],"task":task,"tools":["read","yield"],"agent":agent}),
        user(task, second),
    ]
}

fn tombstone(path: &Path) {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(".tombstone");
    fs::write(sidecar, "").unwrap();
}

/// A session with children of several roles at two depths and everything OMP's Hub skips. Returns
/// the session's transcript path; its artifacts folder is that path without `.jsonl`.
fn session_folder(base: &Path) -> PathBuf {
    let dir = base.join("2026-10-30T10-00-00-000Z_parent");
    let scout = dir.join("Scout.jsonl");
    append(
        &scout,
        &opening("Scout", "scout", "Map the fleet readers.", 1),
    );
    append(
        &scout,
        &[
            assistant("toolUse", Some("read"), 20),
            tool_result("read", json!({}), 21),
            model_change("openai/gpt-5.5", 22),
            thinking("high", 22),
        ],
    );
    fs::write(dir.join("Scout.md"), "notes").unwrap();
    let builder_task =
        "Complete the assignment below, thoroughly:\n\n# Target\nserver/src only.\n\n# Change\nAdd it.";
    let builder = dir.join("Builder.jsonl");
    append(
        &builder,
        &opening("Builder", "bounded-builder", builder_task, 2),
    );
    append(
        &builder,
        &[
            assistant("toolUse", Some("yield"), 30),
            tool_result("yield", json!({"status":"success","data":{}}), 31),
        ],
    );
    let second_builder = dir.join("Builder-2.jsonl");
    append(
        &second_builder,
        &opening("Builder-2", "bounded-builder", "Second build.", 3),
    );
    append(&second_builder, &[assistant("stop", None, 32)]);
    let review = dir.join("Review.jsonl");
    append(&review, &opening("Review", "reviewer", "Review it.", 4));
    append(&review, &[assistant("toolUse", Some("read"), 33)]);
    tombstone(&review);
    let killed = dir.join("Killed.jsonl");
    append(&killed, &[title(), header("Killed", 5)]);
    tombstone(&killed);
    append(&dir.join("Stub.jsonl"), &[title(), header("Stub", 6)]);
    fs::write(dir.join("Empty.jsonl"), "").unwrap();
    append(
        &dir.join("__advisor.jsonl"),
        &opening("advisor", "advisor", "Advise.", 7),
    );
    append(
        &dir.join("__advisor.security.jsonl"),
        &opening("advisor", "advisor", "Advise.", 7),
    );
    append(
        &dir.join("Old.bak.jsonl"),
        &opening("Old", "task", "Old run.", 8),
    );
    let helper = dir.join("Builder/Helper.jsonl");
    append(&helper, &opening("Helper", "task", "Help.", 9));
    append(&helper, &[assistant("toolUse", Some("read"), 34)]);
    append(
        &dir.join("Builder/Helper/Deep.jsonl"),
        &opening("Deep", "explore", "Dig.", 10),
    );
    let gone = dir.join("Builder/Gone.jsonl");
    append(&gone, &opening("Gone", "task", "Gone.", 11));
    tombstone(&gone);
    dir.with_extension("jsonl")
}

#[test]
fn roster_lists_every_hub_child_with_its_role_and_own_turn() {
    let temp = tempfile::tempdir().unwrap();
    let root = session_folder(temp.path());
    let fleet = omp_fleet(&root, || Ok(0)).unwrap().expect("roster");
    let ids: Vec<_> = fleet
        .agents
        .iter()
        .map(|a| a.id.as_deref().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "Scout",
            "Builder",
            "Builder-2",
            "Review",
            "Killed",
            "Helper",
            "Deep",
            "Gone"
        ]
    );
    let names: Vec<_> = fleet.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "scout",
            "bounded-builder",
            "bounded-builder",
            "reviewer",
            "Killed",
            "task",
            "explore",
            "task"
        ]
    );
    let working: Vec<_> = fleet.agents.iter().map(|a| a.working).collect();
    assert_eq!(
        working,
        [true, false, false, false, false, true, true, false]
    );
    assert!(fleet.agents.iter().all(|agent| agent.nested.is_none()));
    let [scout, builder, second_builder, _, killed, ..] = &fleet.agents[..] else {
        unreachable!()
    };
    assert_eq!(scout.started_at, ms(1));
    assert_eq!(scout.model.as_deref(), Some("openai/gpt-5.5"));
    assert_eq!(scout.effort.as_deref(), Some("high"));
    assert_eq!(scout.task.as_deref(), Some("Map the fleet readers."));
    assert_eq!(
        builder.model.as_deref(),
        Some("anthropic/claude-sonnet-4-5")
    );
    assert_eq!(builder.effort.as_deref(), Some("medium"));
    assert_eq!(builder.task.as_deref(), Some("server/src only."));
    assert_eq!(builder.elapsed_seconds, Some(29));
    assert_eq!(second_builder.task.as_deref(), Some("Second build."));
    assert_eq!(killed.task, None);
}

#[test]
fn nested_child_is_its_own_row_working_and_openable() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("2026-10-30T10-00-00-000Z_root");
    let root = dir.with_extension("jsonl");
    let parent = dir.join("Parent.jsonl");
    append(&parent, &opening("Parent", "planner", "Plan.", 1));
    append(
        &parent,
        &[
            assistant("toolUse", Some("yield"), 2),
            tool_result("yield", json!({}), 3),
        ],
    );
    let nested = dir.join("Parent/Nested.jsonl");
    append(&nested, &opening("Nested", "custom-worker", "Work.", 4));
    let fleet = omp_fleet(&root, || Ok(0)).unwrap().expect("roster");
    let rows: Vec<_> = fleet
        .agents
        .iter()
        .map(|a| (a.id.as_deref().unwrap(), a.working))
        .collect();
    assert_eq!(rows, [("Parent", false), ("Nested", true)]);
    assert!(fleet.is_working());
    let (path, agent) = omp_child_transcript(&root, "Nested").unwrap();
    assert_eq!(path, nested);
    assert_eq!(agent.as_deref(), Some("custom-worker"));
}

#[test]
fn a_repeated_id_counts_once_first_in_name_order_depth_first() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("2026-10-30T10-00-00-000Z_root");
    let root = dir.with_extension("jsonl");
    append(
        &dir.join("Builder.jsonl"),
        &opening("Builder", "task", "Build.", 1),
    );
    let nested = dir.join("Builder/Helper.jsonl");
    append(&nested, &opening("Helper", "explore", "Look.", 2));
    append(
        &dir.join("Helper.jsonl"),
        &opening("Helper", "task", "Help.", 3),
    );
    let fleet = omp_fleet(&root, || Ok(0)).unwrap().expect("roster");
    let rows: Vec<_> = fleet
        .agents
        .iter()
        .map(|a| (a.id.as_deref().unwrap(), a.name.as_str()))
        .collect();
    assert_eq!(rows, [("Builder", "task"), ("Helper", "explore")]);
    assert_eq!(omp_child_transcript(&root, "Helper").unwrap().0, nested);
}

#[cfg(unix)]
#[test]
fn a_later_file_with_a_claimed_id_is_not_read() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("2026-10-30T10-00-00-000Z_root");
    let root = dir.with_extension("jsonl");
    append(
        &dir.join("AParent.jsonl"),
        &opening("AParent", "task", "A.", 1),
    );
    let first = dir.join("AParent/Twin.jsonl");
    append(&first, &opening("Twin", "explore", "First twin.", 2));
    append(
        &dir.join("BParent.jsonl"),
        &opening("BParent", "task", "B.", 3),
    );
    let later = dir.join("BParent/Twin.jsonl");
    append(&later, &opening("Twin", "task", "Later twin.", 4));
    fs::set_permissions(&later, fs::Permissions::from_mode(0o000)).unwrap();
    let fleet = omp_fleet(&root, || Ok(0));
    let opened = omp_child_transcript(&root, "Twin");
    fs::set_permissions(&later, fs::Permissions::from_mode(0o600)).unwrap();
    let rows: Vec<_> = fleet
        .unwrap()
        .expect("roster")
        .agents
        .into_iter()
        .map(|agent| (agent.id.unwrap(), agent.name))
        .collect();
    assert_eq!(
        rows,
        [
            ("AParent".to_string(), "task".to_string()),
            ("Twin".to_string(), "explore".to_string()),
            ("BParent".to_string(), "task".to_string())
        ]
    );
    assert_eq!(opened.unwrap().0, first);
}

fn vibe_spawn(id: &str, child_file: &str, second: u32) -> Value {
    json!({"type":"custom","id":format!("v{second}"),"parentId":null,"timestamp":at(second),
        "customType":"vibe-session-lifecycle","data":{"version":1,"id":id,"ownerId":"main",
            "parentSessionId":"parent","action":"spawn","cli":"fast","agent":"task",
            "childSessionFile":child_file,"createdAt":ms(second)}})
}

#[test]
fn children_the_vibe_mode_spawned_are_not_subagents() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("2026-10-30T10-00-00-000Z_root");
    let root = dir.with_extension("jsonl");
    append(
        &root,
        &[
            title(),
            header("root", 0),
            vibe_spawn("Vibe", "Vibe.jsonl", 1),
            // Not a valid claim: the child file is not `<id>.jsonl`.
            vibe_spawn("Other", "Elsewhere.jsonl", 1),
        ],
    );
    append(
        &dir.join("Vibe.jsonl"),
        &opening("Vibe", "task", "Vibe.", 1),
    );
    append(
        &dir.join("Vibe/Inner.jsonl"),
        &opening("Inner", "task", "Inner.", 2),
    );
    append(
        &dir.join("Other.jsonl"),
        &opening("Other", "task", "Other.", 3),
    );
    append(
        &dir.join("Plain.jsonl"),
        &opening("Plain", "task", "Plain.", 4),
    );
    let ids = |root: &Path| -> Vec<String> {
        omp_fleet(root, || Ok(0))
            .unwrap()
            .expect("roster")
            .agents
            .into_iter()
            .map(|agent| agent.id.unwrap())
            .collect()
    };
    assert_eq!(ids(&root), ["Other", "Plain"]);
    assert!(omp_child_transcript(&root, "Vibe").is_err());
    assert!(omp_child_transcript(&root, "Inner").is_err());
    assert!(omp_child_transcript(&root, "Plain").is_ok());
    // A claim appended later is read from where the last read stopped.
    append(&root, &[vibe_spawn("Plain", "Plain.jsonl", 5)]);
    assert_eq!(ids(&root), ["Other"]);
    assert!(omp_child_transcript(&root, "Plain").is_err());
}

#[test]
fn the_title_slot_does_not_count_against_the_leading_records() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("2026-10-30T10-00-00-000Z_root");
    let root = dir.with_extension("jsonl");
    // OMP reads 64 records after the title slot: here `session_init` is the 64th, and the 65th.
    for (id, filler) in [("Inside", 62), ("Outside", 63)] {
        let path = dir.join(format!("{id}.jsonl"));
        append(&path, &[title(), header(id, 1)]);
        for _ in 0..filler {
            append(
                &path,
                &[json!({"type":"custom","customType":"prefix","timestamp":at(1)})],
            );
        }
        append(
            &path,
            &[
                json!({"type":"session_init","timestamp":at(1),"agent":"task","task":"Boundary.",
                "systemPrompt":["Role."],"tools":["read"]}),
            ],
        );
    }
    let fleet = omp_fleet(&root, || Ok(0)).unwrap().expect("roster");
    let ids: Vec<_> = fleet.agents.iter().map(|a| a.id.as_deref()).collect();
    assert_eq!(ids, [Some("Inside")]);
}

#[test]
fn roster_reports_no_work_from_turns_older_than_the_omp_process() {
    let temp = tempfile::tempdir().unwrap();
    let root = session_folder(temp.path());
    let restarted = parse_time("2026-10-30T11:00:00.000Z").unwrap();
    let fleet = omp_fleet(&root, || Ok(restarted)).unwrap().expect("roster");
    assert!(fleet.agents.iter().all(|agent| !agent.working));
    // An idle roster never asks for the process.
    let idle = temp.path().join("idle");
    let child = idle.join("Done.jsonl");
    append(&child, &opening("Done", "task", "Done.", 1));
    append(&child, &[assistant("stop", None, 2)]);
    let fleet = omp_fleet(&idle.with_extension("jsonl"), || {
        panic!("process probed for an idle roster")
    })
    .unwrap()
    .expect("roster");
    assert_eq!(fleet.agents.len(), 1);
}

#[test]
fn omp_child_turn_stays_working_until_the_run_ends() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Worker.jsonl");
    let turn = || {
        latest_child_turn(&path, SessionChatTranscriptAgent::Pi)
            .unwrap()
            .unwrap()
    };
    append(&path, &opening("Worker", "task", "Work.", 1));
    assert!(turn().working);
    append(
        &path,
        &[
            assistant("toolUse", Some("read"), 2),
            tool_result("read", json!({}), 3),
        ],
    );
    let open = turn();
    assert!(open.working);
    assert_eq!(open.started_at, ms(1));
    // An incremental section is not the end of the run.
    append(
        &path,
        &[
            assistant("toolUse", Some("yield"), 4),
            tool_result("yield", json!({"status":"success","type":["section"]}), 5),
        ],
    );
    assert!(turn().working);
    append(
        &path,
        &[
            assistant("toolUse", Some("yield"), 6),
            tool_result("yield", json!({"status":"success","data":{"ok":true}}), 7),
        ],
    );
    assert!(!turn().working);
    // A background result wakes the finished child; its reply shows the run resumed.
    append(
        &path,
        &[async_result(8), assistant("toolUse", Some("read"), 9)],
    );
    assert!(turn().working);
    append(&path, &[assistant("stop", None, 10)]);
    assert!(!turn().working);
    append(&path, &[user("Follow up.", 11)]);
    assert!(turn().working);
    append(&path, &[session_exit(12)]);
    let closed = turn();
    assert!(!closed.working);
    assert_eq!(closed.timestamp, ms(12));
}

#[test]
fn subagent_model_reads_the_latest_omp_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = session_folder(temp.path());
    let model = crate::session_chat_subagent::model::read(
        &root.with_extension("").join("Scout.jsonl"),
        SessionChatTranscriptAgent::Pi,
    )
    .unwrap();
    assert_eq!(model.model.as_deref(), Some("openai/gpt-5.5"));
    assert_eq!(model.effort.as_deref(), Some("high"));
}

fn session(agent: &str, root: &Path) -> Value {
    json!({"agentId":agent,"lifecycleState":"running",
        "runtimeSettings":{"agentSessionPath":root.to_str().unwrap()}})
}

#[test]
fn only_omp_sessions_get_a_roster() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("2026-10-30T10-00-00-000Z_parent.jsonl");
    append(&root, &[title(), header("parent", 0)]);
    let child = temp
        .path()
        .join("2026-10-30T10-00-00-000Z_parent/Done.jsonl");
    append(&child, &opening("Done", "planner", "Plan.", 1));
    append(&child, &[assistant("stop", None, 2)]);
    use crate::session_chat_fleet_status::{has_fleet_reader, read_fleet};
    use crate::session_chat_options::SessionChatOptionAgent;
    assert!(has_fleet_reader(Some(SessionChatOptionAgent::Omp)));
    assert!(!has_fleet_reader(Some(SessionChatOptionAgent::Pi)));
    let fleet = read_fleet(&session("omp", &root), None)
        .unwrap()
        .expect("OMP roster");
    assert_eq!(fleet.agents[0].name, "planner");
    assert_eq!(read_fleet(&session("pi", &root), None).unwrap(), None);
}

#[test]
fn child_transcript_opens_only_roster_ids_of_omp_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let root = session_folder(temp.path());
    let dir = root.with_extension("");
    let (path, agent) = omp_child_transcript(&root, "Builder-2").unwrap();
    assert_eq!(path, dir.join("Builder-2.jsonl"));
    assert_eq!(agent.as_deref(), Some("bounded-builder"));
    assert!(omp_child_transcript(&root, "Killed").is_ok());
    assert_eq!(
        omp_child_transcript(&root, "Helper").unwrap().0,
        dir.join("Builder/Helper.jsonl")
    );
    append(
        &temp.path().join("Sibling.jsonl"),
        &opening("Sibling", "task", "Not a child.", 1),
    );
    for refused in [
        "../Sibling",
        "a/b",
        "",
        ".",
        "..",
        "/etc/passwd",
        "Builder\\x",
        "Stub",
        "Empty",
        "__advisor",
        "__advisor.security",
        "Old.bak",
        "Missing",
        "Builder/Helper",
    ] {
        assert!(
            omp_child_transcript(&root, refused).is_err(),
            "{refused:?} must be refused"
        );
    }
    #[cfg(unix)]
    {
        let outside = temp.path().join("outside/Escape.jsonl");
        append(&outside, &opening("Escape", "task", "Outside.", 1));
        std::os::unix::fs::symlink(&outside, dir.join("Escape.jsonl")).unwrap();
        assert!(omp_child_transcript(&root, "Escape").is_err());
    }
    use crate::session_chat_subagent::subagent_family;
    assert_eq!(
        subagent_family(&session("omp", &root)).unwrap(),
        SessionChatTranscriptAgent::Pi
    );
    assert_eq!(
        subagent_family(&session("claude", &root)).unwrap(),
        SessionChatTranscriptAgent::Claude
    );
    let refusal = subagent_family(&session("pi", &root))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("OMP"), "{refusal}");
}
