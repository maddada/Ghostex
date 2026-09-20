//! Plans every read-only sidebar action for a recorded presentation and writes the CALLS it would
//! make, so the shipped TypeScript that still answers them can be diffed against it.
//!
//! Usage: `cargo run --release --example sidebar_action_parity -- <scenario-dir>`
//!
//! The directory holds the `scenario-<n>.json` files `tooling/gx-core/menu-parity.ts scenarios`
//! writes. This writes `rust-actions-<n>.json` beside each; `tooling/gx-core/action-parity.ts
//! compare` then runs the same payload list through the real `GpuiSidebarRuntime` arms and diffs
//! the two sides call by call.
//!
//! **Why the calls and not the list.** A sidebar action is invisible in any comparison of the
//! drawn sidebar: sending the wrong native action, or the right one with the wrong project id,
//! changes nothing a list comparison can see. So the gate enumerates the calls themselves, and it
//! enumerates rather than samples: every project of the recording is probed with every message
//! type, together with the ids that have no project (a remote group, a user-made group, the Chats
//! group, a group that does not exist, the empty string) and the text edges of the two copy
//! actions. A payload the menus can build and this list misses is a hole in the gate, so the menu
//! dump `sidebar_menu_parity` writes is read when it is there and every read-only command in it is
//! added to the list.
//!
//! Recordings contain private data. Keep both the scenarios and the dumps outside the repository.

use std::process::ExitCode;
use std::time::Instant;

use ghostex_gx_core::protocol::ServerEvent;
use ghostex_gx_core::{
    apply_close_answer, apply_flags_answer, apply_fork_answer, apply_lifecycle_answer,
    apply_snooze_answer, close_optimistic_follow_ups, encode_uri_component, iso_string_from_ms,
    plan_close_request, plan_flags_request, plan_fork_request, plan_lifecycle_request,
    plan_batch, plan_bulk_request, plan_modal_action, plan_read_only_action, plan_snooze_action,
    plan_snooze_request,
    rename_seed_title, session_is_snoozed, snooze_wake_ms, ActiveGroup, CloseAnswer, CloseFollowUp,
    Core, Event, FlagsFollowUp, ForkFollowUp, Intent, LifecycleAnswer, LifecycleFollowUp,
    MachineId, ProjectKey, SectionCollapse, SessionKey, SidebarInputs, SidebarView,
    SidebarViewModel, SnoozeClock, SnoozeFollowUp, QUICK_AUTOMATIONS_PROJECT_ID,
    READ_ONLY_MESSAGE_TYPES, SESSION_SNOOZE_PRESETS,
};
use serde_json::{json, Map, Value};

/// The clock facts file the harness writes beside the scenarios.
const SNOOZE_CLOCK_FILE: &str = "snooze-clock.json";

/// Every id shape that is not a live local project group, so each branch of the resolution is
/// probed even when the recording has no row of that kind.
const SYNTHETIC_GROUP_IDS: [&str; 7] = [
    "remote:machine-1:group:P0erj",
    "remote:machine-1:group:",
    "remote::group:P0erj",
    "combined-chats",
    "gpui-wsg:P0erj:group-1",
    "combined-project:does-not-exist",
    "",
];

/// The same for a session id: two remote shapes, a local one, and the empty string.
const SYNTHETIC_SESSION_IDS: [&str; 5] = [
    "remote:machine-1:session:P0erj:S1",
    "remote:machine-1:session:P0erj:",
    "remote::session:P0erj:S1",
    "combined-session:P0erj:S1",
    "",
];

/// The text edges `normalizeNonEmptyString` decides on: it tests the TRIMMED value and returns the
/// ORIGINAL, so a padded string must reach the clipboard with its padding.
const SYNTHETIC_TEXTS: [&str; 5] = ["Copy me", "  padded  ", "   ", "", "\n"];

fn main() -> ExitCode {
    let Some(directory) = std::env::args().nth(1) else {
        eprintln!("usage: sidebar_action_parity <scenario-dir>");
        return ExitCode::from(2);
    };
    let mut scenarios: Vec<std::path::PathBuf> = match std::fs::read_dir(&directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("scenario-") && name.ends_with(".json"))
            })
            .collect(),
        Err(error) => {
            eprintln!("cannot read {directory}: {error}");
            return ExitCode::from(2);
        }
    };
    scenarios.sort();
    if scenarios.is_empty() {
        eprintln!("no scenario-*.json in {directory}");
        return ExitCode::from(2);
    }
    let mut total_payloads = 0usize;
    let mut total_calls = 0usize;
    let mut from_menus = 0usize;
    let mut total_lifecycle = 0usize;
    let mut total_close = 0usize;
    let mut total_fork = 0usize;
    let mut total_flags = 0usize;
    let mut total_modals = 0usize;
    let mut total_snooze = 0usize;
    let mut total_bulk = 0usize;
    let mut resolve_micros: Vec<u128> = Vec::new();
    // The clock facts the snooze rule is answered against. The harness writes them under a pinned
    // time zone because this crate reads neither a clock nor a zone; without the file the snooze
    // probes are empty and `action-parity.ts compare` fails rather than reporting a clean run it
    // never measured.
    let clock_file: Option<Value> =
        std::fs::read_to_string(std::path::Path::new(&directory).join(SNOOZE_CLOCK_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
    if clock_file.is_none() {
        eprintln!(
            "no {SNOOZE_CLOCK_FILE} in {directory}: run `bun tooling/gx-core/action-parity.ts snooze-clock {directory}` first, or the snooze half measures nothing"
        );
    }
    for path in &scenarios {
        let scenario: Value = match std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
        {
            Some(value) => value,
            None => {
                eprintln!("cannot read {}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let menu_dump = menu_dump_beside(path);
        let Some(dump) = build(
            &scenario,
            menu_dump.as_ref(),
            clock_file.as_ref(),
            &mut resolve_micros,
        ) else {
            eprintln!("cannot build {}", path.display());
            return ExitCode::FAILURE;
        };
        total_payloads += dump.payloads;
        total_calls += dump.calls;
        from_menus += dump.from_menus;
        total_lifecycle += dump.lifecycle;
        total_close += dump.close;
        total_fork += dump.fork;
        total_flags += dump.flags;
        total_modals += dump.modals;
        total_snooze += dump.snooze;
        total_bulk += dump.bulk;
        let out = path.with_file_name(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .replace("scenario-", "rust-actions-"),
        );
        // A payload carries a project path, a resume command line and a session title, and a plan
        // carries them back; both are written as privately as the scenarios are.
        if let Err(error) = write_private(&out, &serde_json::to_string(&dump.value).unwrap()) {
            eprintln!("cannot write {}: {error}", out.display());
            return ExitCode::FAILURE;
        }
        println!(
            "{}: {} payloads, {} calls ({} from menus), {} lifecycle transitions, {} closes",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
            dump.payloads,
            dump.calls,
            dump.from_menus,
            dump.lifecycle,
            dump.close
        );
    }
    resolve_micros.sort_unstable();
    // The one cost this milestone adds to a click is the resolution, and a timer nobody looks at
    // is a timer that was never assigned: the group-resolving actions build the project facts, so
    // the median over them must not read as free.
    let median = resolve_micros
        .get(resolve_micros.len() / 2)
        .copied()
        .unwrap_or(0);
    println!(
        "scenarios {} payloads {total_payloads} calls {total_calls} fromMenus {from_menus} lifecycle {total_lifecycle} close {total_close} fork {total_fork} flags {total_flags} modals {total_modals} snooze {total_snooze} bulk {total_bulk} resolveUs median {median} max {}",
        scenarios.len(),
        resolve_micros.last().copied().unwrap_or(0)
    );
    if median == 0 {
        eprintln!(
            "the project-path resolution measured 0 us over {} probes, which is a timer that is not measuring what it says",
            resolve_micros.len()
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct Dump {
    value: Value,
    payloads: usize,
    calls: usize,
    from_menus: usize,
    lifecycle: usize,
    close: usize,
    fork: usize,
    flags: usize,
    modals: usize,
    snooze: usize,
    bulk: usize,
}

/// The menu dump `sidebar_menu_parity` writes for the same scenario, when it has been run.
fn menu_dump_beside(scenario: &std::path::Path) -> Option<Value> {
    let path = scenario.with_file_name(
        scenario
            .file_name()
            .and_then(|name| name.to_str())?
            .replace("scenario-", "rust-"),
    );
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_private(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(body.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn build(
    scenario: &Value,
    menu_dump: Option<&Value>,
    clock_file: Option<&Value>,
    resolve_micros: &mut Vec<u128>,
) -> Option<Dump> {
    let now_ms = scenario.get("nowMs").and_then(Value::as_u64).unwrap_or(0);
    let mut core = Core::new();
    let frame = json!({
        "type": "presentationSnapshot",
        "protocolVersion": 1,
        "serverId": "parity",
        "clientId": "parity",
        "revision": scenario.pointer("/snapshot/revision").cloned().unwrap_or(json!(1)),
        "snapshot": scenario.get("snapshot")?.clone(),
    });
    let frame = ServerEvent::parse(&frame.to_string()).ok()?;
    let changes = core
        .handle(
            Event::Frame {
                machine: MachineId::Local,
                frame: Box::new(frame),
            },
            now_ms,
        )
        .changes;
    // One drawn list, under default inputs, so the dialog probes below ask about rows the sidebar
    // really draws. Nothing else in this example needs it.
    let mut model = SidebarViewModel::new();
    let view_inputs = SidebarInputs::default();
    model.update(&core, &view_inputs, &changes, now_ms);

    let project_ids: Vec<String> = scenario
        .pointer("/snapshot/projects")?
        .as_array()?
        .iter()
        .filter_map(|project| project.get("projectId")?.as_str())
        .map(str::to_string)
        .collect();

    // Two host input sets, because `parked_project_ids` is the one host value the resolution
    // reads: one where nothing is parked, and one where the FIRST project is, which is exactly
    // what takes a project's group away without touching the presentation.
    let parked = project_ids.first().cloned();
    let mut variants: Vec<(&str, SidebarInputs)> = vec![("none", SidebarInputs::default())];
    if let Some(parked) = parked.clone() {
        let mut inputs = SidebarInputs::default();
        inputs.host.recent_project_ids.insert(parked);
        variants.push(("firstParked", inputs));
    }

    let mut payloads: Vec<Value> = Vec::new();
    let mut from_menus = 0usize;
    for project_id in &project_ids {
        let group_id = format!("combined-project:{}", encode_uri_component(project_id));
        for kind in group_message_types() {
            payloads.push(json!({ "type": kind, "groupId": group_id }));
        }
    }
    for group_id in SYNTHETIC_GROUP_IDS {
        for kind in group_message_types() {
            payloads.push(json!({ "type": kind, "groupId": group_id }));
        }
    }
    // NOT probed: the three group actions with no `groupId` at all. `resolveProjectIdForGroup`
    // hands `undefined` to `parseGxserverPresentationProjectGroupId`, which throws, so the
    // TypeScript answers that payload with an unhandled rejection rather than with a call. The
    // renderer cannot build one (`NativeSidebarCommand` requires the field and every menu sets
    // it), so the two sides are not compared on a shape neither can be asked for. The two session
    // actions and the two copy actions ARE probed without their field, because those answer it.
    for session_id in session_ids(scenario).iter().chain(
        SYNTHETIC_SESSION_IDS
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<String>>()
            .iter(),
    ) {
        payloads.push(json!({ "type": "copyResumeCommand", "sessionId": session_id }));
        payloads.push(json!({ "type": "copyAttachCommand", "sessionId": session_id }));
    }
    payloads.push(json!({ "type": "copyResumeCommand" }));
    payloads.push(json!({ "type": "copyAttachCommand" }));
    for text in SYNTHETIC_TEXTS {
        payloads.push(json!({ "type": "copySessionDetails", "detailsText": text }));
        payloads.push(json!({ "type": "copyWorkspaceProjectRemoteUrl", "remoteUrl": text }));
    }
    payloads.push(json!({ "type": "copySessionDetails" }));
    payloads.push(json!({ "type": "copyWorkspaceProjectRemoteUrl" }));
    if let Some(dump) = menu_dump {
        let mut found: Vec<Value> = Vec::new();
        collect_menu_commands(dump, &mut found);
        from_menus = found.len();
        payloads.extend(found);
    }

    let mut entries: Vec<Value> = Vec::new();
    let mut calls = 0usize;
    for (variant, inputs) in &variants {
        for payload in &payloads {
            let started = Instant::now();
            let plan = plan_read_only_action(&core, inputs, payload)?;
            if payload
                .get("groupId")
                .and_then(Value::as_str)
                .is_some_and(|group_id| group_id.starts_with("combined-project:"))
            {
                resolve_micros.push(started.elapsed().as_micros());
            }
            calls += plan.effects.len();
            entries.push(json!({
                "variant": variant,
                "payload": payload,
                "calls": plan.to_json(),
            }));
        }
    }
    let lifecycle = lifecycle_entries(&core, scenario, now_ms);
    let lifecycle_count = lifecycle.len();
    let closes = close_entries(&core, scenario, now_ms);
    let close_count = closes.len();
    let forks = fork_entries(&core, scenario, now_ms);
    let fork_count = forks.len();
    let flags = flags_entries(&core, scenario, menu_dump, now_ms);
    let flags_count = flags.len();
    let modals = modal_entries(model.view());
    let modal_count = modals.len();
    let bulk = bulk_entries(&core, &view_inputs, scenario);
    let batch = batch_entries();
    let snooze_clock = snooze_clock_entries(clock_file);
    let snooze_boundary = snooze_boundary_entries(&core, scenario, model.view());
    let snooze_actions = snooze_action_entries(model.view(), clock_file);
    let snooze_calls = snooze_call_entries(scenario);
    let snooze_count =
        snooze_clock.len() + snooze_boundary.len() + snooze_actions.len() + snooze_calls.len();
    let bulk_count = bulk.len() + batch.len();
    Some(Dump {
        payloads: entries.len(),
        calls,
        from_menus,
        lifecycle: lifecycle_count,
        close: close_count,
        fork: fork_count,
        flags: flags_count,
        modals: modal_count,
        snooze: snooze_count,
        bulk: bulk_count,
        value: json!({
            "parkedProjectId": parked,
            "entries": Value::Array(entries),
            "lifecycle": Value::Array(lifecycle),
            "close": Value::Array(closes),
            "fork": Value::Array(forks),
            "flags": Value::Array(flags),
            "modals": Value::Array(modals),
            "titleRule": Value::Array(title_rule_entries()),
            "snoozeClock": Value::Array(snooze_clock),
            "snoozeBoundary": Value::Array(snooze_boundary),
            "snoozeActions": Value::Array(snooze_actions),
            "snoozeCalls": Value::Array(snooze_calls),
            "bulk": Value::Array(bulk),
            "batch": Value::Array(batch),
        }),
    })
}

/// The transition half of the gate.
///
/// A lifecycle action is not one answer but four, and only the first is visible in a call diff:
/// what the call was, what the client shows the moment the daemon accepts it, what it shows when
/// the daemon then echoes AGREEMENT, and what it shows when the daemon echoes something ELSE. The
/// last one is the case that reaches users (the row said asleep, the daemon says running) and the
/// one nothing has ever compared, so every entry here carries all four.
///
/// Every session of the recording is driven, under both calls and under three starting focus
/// states, because the focus is what decides the follow-ups: which row the sleep hands the focus
/// to, and whether a wake takes it back.
fn lifecycle_entries(core: &Core, scenario: &Value, now_ms: u64) -> Vec<Value> {
    let rows: Vec<Value> = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut targets: Vec<(String, String, Option<Value>)> = rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("projectId")?.as_str()?.to_string(),
                row.get("sessionId")?.as_str()?.to_string(),
                Some(row.clone()),
            ))
        })
        .collect();
    // A session the store has never heard of, which the TypeScript still calls the daemon for,
    // because `parseGxserverPresentationProjectSessionId` is a string parse and asks the store
    // nothing.
    targets.push((
        "unknown-project".to_string(),
        "unknown-session".to_string(),
        None,
    ));
    // A session of the Quick Automations project, whose whole answer is "nothing happens".
    targets.push((
        QUICK_AUTOMATIONS_PROJECT_ID.to_string(),
        "quick-1".to_string(),
        None,
    ));
    // The focus sitting on a real row of another project, so "somewhere else" is a row the store
    // holds rather than an id it would drop.
    let first_project = targets.first().map(|(project_id, _, _)| project_id.clone());
    let elsewhere = targets
        .iter()
        .rev()
        .find(|(project_id, _, row)| row.is_some() && Some(project_id) != first_project.as_ref())
        .map(|(project_id, session_id, _)| SessionKey::local(project_id, session_id));
    let mut entries = Vec::new();
    for (project_id, session_id, row) in &targets {
        let session = SessionKey::local(project_id, session_id);
        for sleeping in [true, false] {
            // `movesDuringCall` is the state the other three cannot reach: the user picks
            // another session WHILE the daemon is answering. It is the only way to exercise
            // `focusMovedElsewhereDuringWake`, and without it a wake that steals the focus back
            // is invisible to this gate.
            for focus in ["self", "other", "none", "movesDuringCall"] {
                let focused = match focus {
                    "self" | "movesDuringCall" => Some(session.clone()),
                    "other" => elsewhere.clone(),
                    _ => None,
                };
                entries.push(lifecycle_entry(
                    core,
                    &session,
                    sleeping,
                    focus,
                    focused,
                    match focus {
                        "movesDuringCall" => elsewhere.clone(),
                        _ => None,
                    },
                    row.as_ref(),
                    now_ms,
                ));
            }
        }
    }
    entries
}

/// One action, driven from one starting state through all three answers and, for the accepted
/// one, all three echoes.
fn lifecycle_entry(
    core: &Core,
    session: &SessionKey,
    sleeping: bool,
    focus: &str,
    focused: Option<SessionKey>,
    // Where the focus moved to while the daemon was answering, when this case is about that.
    moved_to: Option<SessionKey>,
    row: Option<&Value>,
    now_ms: u64,
) -> Value {
    let mut base = core.clone();
    if let Some(focused) = &focused {
        base.handle(
            Event::Intent(Intent::FocusSession {
                session: focused.clone(),
                visible: None,
            }),
            now_ms,
        );
    }
    let payload = json!({
        "type": "setSessionSleeping",
        "sessionId": session.to_sidebar_session_id(),
        "sleeping": sleeping,
    });
    let Some(request) = plan_lifecycle_request(&base, &payload) else {
        return json!({
            "sessionId": session.to_sidebar_session_id(),
            "sleeping": sleeping,
            "focus": focus,
            "owned": false,
        });
    };
    // The focus as the answer finds it, which is not always the focus the call left with.
    let focused_now = moved_to.or_else(|| base.focus().focused_session.clone());
    // Each answer is compared by what it LEAVES, not by the follow-up list: the two sides express
    // the optimistic value differently (an overlay here, a written row there), so the comparable
    // thing is the state the sidebar would draw afterwards plus which row took the focus.
    let mut answers = Map::new();
    let mut accepted_store = base.clone();
    for answer in [
        LifecycleAnswer::Accepted,
        LifecycleAnswer::Declined,
        LifecycleAnswer::Failed,
    ] {
        let follow_ups = apply_lifecycle_answer(&request, answer, focused_now.as_ref(), now_ms);
        let mut store = base.clone();
        for follow_up in &follow_ups {
            if let LifecycleFollowUp::Patch { session, patch } = follow_up {
                store.handle(
                    Event::Intent(Intent::PatchSession {
                        session: session.clone(),
                        patch: patch.clone(),
                    }),
                    now_ms,
                );
            }
        }
        answers.insert(
            answer.as_str().to_string(),
            json!({
                "state": lifecycle_json(effective_lifecycle(&store, session).as_deref()),
                "focus": Value::Array(
                    follow_ups
                        .iter()
                        .filter(|follow_up| matches!(follow_up, LifecycleFollowUp::Focus { .. }))
                        .map(LifecycleFollowUp::to_json)
                        .collect(),
                ),
            }),
        );
        if answer == LifecycleAnswer::Accepted {
            accepted_store = store;
        }
    }
    // The echo half: the daemon is made to say three different things about the same row, from the
    // state an accepted answer left behind.
    let after = accepted_store;
    let mut echo = Map::new();
    if let Some(row) = row {
        let original = row
            .get("lifecycleState")
            .and_then(Value::as_str)
            .unwrap_or("running")
            .to_string();
        let agreed = match sleeping {
            true => "sleeping".to_string(),
            false => "running".to_string(),
        };
        // The third echo has to be a value NEITHER side predicted, so it is chosen against both
        // the row\'s own state and the one the action asked for. Picking a fixed word instead made
        // this probe agree with `stillOld` for every already-stopped row, and the three echoes
        // then said the same thing and proved nothing.
        let moved_on = ["stopped", "running", "sleeping", "unknown"]
            .into_iter()
            .find(|state| *state != original && *state != agreed)
            .unwrap_or("unknown")
            .to_string();
        let mut states = Map::new();
        states.insert("agrees".to_string(), Value::String(agreed.clone()));
        states.insert("stillOld".to_string(), Value::String(original.clone()));
        states.insert("movedOn".to_string(), Value::String(moved_on.clone()));
        echo.insert("states".to_string(), Value::Object(states));
        for (name, state) in [
            ("agrees", agreed.clone()),
            ("stillOld", original.clone()),
            ("movedOn", moved_on.clone()),
        ] {
            let mut echoed = after.clone();
            let mut echo_row = row.clone();
            echo_row["lifecycleState"] = Value::String(state.clone());
            let frame = json!({
                "type": "presentationDelta",
                "protocolVersion": 1,
                "serverId": "parity",
                "clientId": "parity",
                "revision": now_revision(&echoed, session),
                "delta": { "type": "sessionUpdated", "session": echo_row },
            });
            // A gate whose echo silently fails to apply cannot fail: every one of the three
            // answers would then read back the optimistic value, and two of the three are
            // ALLOWED to. So a frame that does not parse, or that the store ignores, stops the
            // run instead of quietly agreeing with itself.
            let parsed = ServerEvent::parse(&frame.to_string())
                .unwrap_or_else(|error| panic!("the echo frame does not parse: {error:?}"));
            let output = echoed.handle(
                Event::Frame {
                    machine: MachineId::Local,
                    frame: Box::new(parsed),
                },
                now_ms,
            );
            assert!(
                output.changes.ignored.is_none(),
                "the echo frame was ignored: {:?}",
                output.changes.ignored
            );
            echo.insert(
                name.to_string(),
                lifecycle_json(effective_lifecycle(&echoed, session).as_deref()),
            );
        }
    }
    json!({
        "sessionId": session.to_sidebar_session_id(),
        "sleeping": sleeping,
        "focus": focus,
        "owned": true,
        "request": request.to_json(),
        "answers": Value::Object(answers),
        "echo": Value::Object(echo),
    })
}

/// One past the revision the store holds, so the delta is never dropped as stale.
fn now_revision(core: &Core, session: &SessionKey) -> i64 {
    core.presentation()
        .loaded(&session.machine)
        .map(|loaded| loaded.revision + 1)
        .unwrap_or(1)
}

/// The lifecycle state the sidebar would draw for a row: the daemon value with the overlay on top,
/// and nothing at all when the row is not there.
fn effective_lifecycle(core: &Core, session: &SessionKey) -> Option<String> {
    core.presentation()
        .machine(&session.machine)?
        .effective_session(&session.project_id, &session.session_id)
        .map(|row| row.lifecycle_state.as_str().to_string())
}

fn lifecycle_json(state: Option<&str>) -> Value {
    state.map_or(Value::Null, |state| Value::String(state.to_string()))
}

fn group_message_types() -> Vec<&'static str> {
    READ_ONLY_MESSAGE_TYPES
        .into_iter()
        .filter(|kind| kind.ends_with("ForGroup"))
        .collect()
}

/// Every session the recording holds, in its sidebar form.
fn session_ids(scenario: &Value) -> Vec<String> {
    scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .map(|sessions| {
            sessions
                .iter()
                .filter_map(|session| {
                    Some(format!(
                        "combined-session:{}:{}",
                        encode_uri_component(session.get("projectId")?.as_str()?),
                        encode_uri_component(session.get("sessionId")?.as_str()?)
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every `{ type: 'command', message }` payload anywhere in a menu dump whose message is one of
/// the types this milestone ported, deduplicated and in a stable order.
fn collect_menu_commands(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Object(entries) => {
            if entries.get("type") == Some(&Value::String("command".to_string())) {
                if let Some(Value::Object(message)) = entries.get("message") {
                    if message
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(|kind| READ_ONLY_MESSAGE_TYPES.contains(&kind))
                    {
                        let message = Value::Object(message.clone());
                        if !found.contains(&message) {
                            found.push(message);
                        }
                    }
                }
            }
            for item in entries.values() {
                collect_menu_commands(item, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_menu_commands(item, found);
            }
        }
        _ => {}
    }
}

/// The close half of the gate.
///
/// Close is the only action whose optimistic update takes a row AWAY, so it gets a fourth answer
/// the other actions do not have: the call that never comes home. That is the case where an
/// optimistic removal and a never-arriving echo leave the client's story permanently ahead of the
/// daemon's, and it is the one the original cannot even reach, because its `rpc` is a bare `fetch`
/// with no timeout.
///
/// Every entry carries what the user sees at three moments: the instant they click, once the
/// answer is in (for each of the four answers), and after the daemon says one of three things
/// about the row afterwards.
fn close_entries(core: &Core, scenario: &Value, now_ms: u64) -> Vec<Value> {
    let rows: Vec<Value> = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut targets: Vec<(String, String, Option<Value>)> = rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("projectId")?.as_str()?.to_string(),
                row.get("sessionId")?.as_str()?.to_string(),
                Some(row.clone()),
            ))
        })
        .collect();
    targets.push((
        "unknown-project".to_string(),
        "unknown-session".to_string(),
        None,
    ));
    let first_project = targets.first().map(|(project_id, _, _)| project_id.clone());
    let elsewhere = targets
        .iter()
        .rev()
        .find(|(project_id, _, row)| row.is_some() && Some(project_id) != first_project.as_ref())
        .map(|(project_id, session_id, _)| SessionKey::local(project_id, session_id));
    let mut entries = Vec::new();
    for (project_id, session_id, row) in &targets {
        let session = SessionKey::local(project_id, session_id);
        for focus in ["self", "other", "none"] {
            let focused = match focus {
                "self" => Some(session.clone()),
                "other" => elsewhere.clone(),
                _ => None,
            };
            entries.push(close_entry(
                core,
                &session,
                focus,
                focused,
                row.as_ref(),
                now_ms,
            ));
        }
    }
    entries
}

fn close_entry(
    core: &Core,
    session: &SessionKey,
    focus: &str,
    focused: Option<SessionKey>,
    row: Option<&Value>,
    now_ms: u64,
) -> Value {
    let mut base = core.clone();
    if let Some(focused) = &focused {
        base.handle(
            Event::Intent(Intent::FocusSession {
                session: focused.clone(),
                visible: None,
            }),
            now_ms,
        );
    }
    let payload = json!({
        "type": "closeSession",
        "sessionId": session.to_sidebar_session_id(),
    });
    let Some(request) = plan_close_request(&base, &payload) else {
        return json!({
            "sessionId": session.to_sidebar_session_id(),
            "focus": focus,
            "owned": false,
        });
    };
    // The click itself.
    let optimistic = close_optimistic_follow_ups(&request);
    let mut after_click = base.clone();
    run_close_follow_ups(&mut after_click, &optimistic, now_ms);
    let mut answers = Map::new();
    let mut accepted_store = after_click.clone();
    for answer in [
        CloseAnswer::Accepted,
        CloseAnswer::Failed,
        CloseAnswer::NeverAnswered,
    ] {
        let follow_ups = apply_close_answer(&request, answer);
        let mut store = after_click.clone();
        run_close_follow_ups(&mut store, &follow_ups, now_ms);
        answers.insert(
            answer.as_str().to_string(),
            json!({ "drawn": row_is_drawn(&store, session) }),
        );
        if answer == CloseAnswer::Accepted {
            accepted_store = store;
        }
    }
    let mut echo = Map::new();
    if let Some(row) = row {
        // What the daemon can say next about a row the client has already taken away.
        for (name, frame) in [
            (
                "removed",
                json!({
                    "type": "sessionRemoved",
                    "projectId": session.project_id,
                    "sessionId": session.session_id,
                }),
            ),
            (
                "stillRunning",
                json!({
                    "type": "sessionUpdated",
                    "session": with_lifecycle(row, "running"),
                }),
            ),
            (
                "stopped",
                json!({
                    "type": "sessionUpdated",
                    "session": with_lifecycle(row, "stopped"),
                }),
            ),
        ] {
            let mut echoed = accepted_store.clone();
            let envelope = json!({
                "type": "presentationDelta",
                "protocolVersion": 1,
                "serverId": "parity",
                "clientId": "parity",
                "revision": now_revision(&echoed, session),
                "delta": frame,
            });
            let parsed = ServerEvent::parse(&envelope.to_string())
                .unwrap_or_else(|error| panic!("the close echo frame does not parse: {error:?}"));
            let output = echoed.handle(
                Event::Frame {
                    machine: MachineId::Local,
                    frame: Box::new(parsed),
                },
                now_ms,
            );
            assert!(
                output.changes.ignored.is_none(),
                "the close echo frame was ignored: {:?}",
                output.changes.ignored
            );
            echo.insert(
                name.to_string(),
                json!({ "drawn": row_is_drawn(&echoed, session) }),
            );
        }
    }
    json!({
        "sessionId": session.to_sidebar_session_id(),
        "focus": focus,
        "owned": true,
        "request": request.to_json(),
        "optimistic": {
            "drawn": row_is_drawn(&after_click, session),
            "focus": Value::Array(
                optimistic
                    .iter()
                    .filter(|follow_up| matches!(follow_up, CloseFollowUp::Focus { .. }))
                    .map(CloseFollowUp::to_json)
                    .collect(),
            ),
        },
        "answers": Value::Object(answers),
        "echo": Value::Object(echo),
    })
}

fn with_lifecycle(row: &Value, state: &str) -> Value {
    let mut row = row.clone();
    row["lifecycleState"] = Value::String(state.to_string());
    row
}

fn run_close_follow_ups(core: &mut Core, follow_ups: &[CloseFollowUp], now_ms: u64) {
    for follow_up in follow_ups {
        let intent = match follow_up {
            CloseFollowUp::Hide { session } => Intent::HideSession {
                session: session.clone(),
            },
            CloseFollowUp::Unhide { session } => Intent::UnhideSession {
                session: session.clone(),
            },
            CloseFollowUp::Focus { .. } => continue,
        };
        core.handle(Event::Intent(intent), now_ms);
    }
}

/// Whether the sidebar would draw the row at all: the daemon holds it and no local overlay hides
/// it. This is the only thing a close can be judged by, because a close does not change a value,
/// it changes whether there is a row.
fn row_is_drawn(core: &Core, session: &SessionKey) -> bool {
    core.presentation()
        .machine(&session.machine)
        .and_then(|entry| entry.effective_session(&session.project_id, &session.session_id))
        .is_some()
}

/// The fork half of the gate.
///
/// Fork has no optimistic half to compare, which is itself the thing worth proving: the pane move
/// happens only after the daemon has answered with a session id, so the four answers are a call
/// that succeeds, one that succeeds with nothing usable in it, one that fails, and one that never
/// comes home. Only the first places a pane; the other three must all land in the same toast and
/// move nothing.
fn fork_entries(core: &Core, scenario: &Value, now_ms: u64) -> Vec<Value> {
    let rows: Vec<Value> = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut targets: Vec<(String, String)> = rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("projectId")?.as_str()?.to_string(),
                row.get("sessionId")?.as_str()?.to_string(),
            ))
        })
        .collect();
    // A row the store does not hold: the fork has no source and neither side calls anything.
    targets.push(("unknown-project".to_string(), "unknown-session".to_string()));
    let mut entries = Vec::new();
    for (project_id, session_id) in &targets {
        let session = SessionKey::local(project_id, session_id);
        for active in ["elsewhere", "already"] {
            let mut base = core.clone();
            if active == "already" {
                base.handle(
                    Event::Intent(Intent::FocusSession {
                        session: session.clone(),
                        visible: None,
                    }),
                    now_ms,
                );
            }
            // Recorded rather than assumed. A loaded snapshot already re-homes the focus to SOME
            // project, so "do nothing" does not mean "no project is active", and for a session of
            // that project it silently means the opposite of what the case is called. The
            // TypeScript half starts from this exact pair instead of from a label.
            let focus = base.focus();
            let active_before = json!({
                "project": focus.active_project.as_ref().map(ProjectKey::to_workspace_project_id),
                "group": focus.active_group.as_ref().map(ActiveGroup::to_sidebar_group_id),
            });
            let Some(request) = plan_fork_request(
                &base,
                &json!({
                    "type": "forkSession",
                    "sessionId": session.to_sidebar_session_id(),
                }),
            ) else {
                entries.push(json!({
                    "sessionId": session.to_sidebar_session_id(),
                    "active": active,
                    "activeBefore": active_before,
                    "owned": false,
                }));
                continue;
            };
            let mut answers = Map::new();
            for (name, result) in fork_answers(session_id) {
                answers.insert(
                    name.to_string(),
                    Value::Array(
                        apply_fork_answer(&request, result.as_ref().map_err(String::as_str))
                            .iter()
                            .map(ForkFollowUp::to_json)
                            .collect(),
                    ),
                );
            }
            entries.push(json!({
                "sessionId": session.to_sidebar_session_id(),
                "active": active,
                "activeBefore": active_before,
                "owned": true,
                "request": request.to_json(),
                "answers": Value::Object(answers),
            }));
        }
    }
    entries
}

/// The four things `/api/forkSession` can do, in the shape the host hands them on.
fn fork_answers(session_id: &str) -> Vec<(&'static str, Result<Value, String>)> {
    vec![
        (
            "accepted",
            Ok(json!({ "fork": { "session": { "sessionId": format!("{session_id}-fork") } } })),
        ),
        // A success envelope with nothing usable in it. The TypeScript throws its own error for
        // this and lands in the same toast as a transport failure, which is the leg a port is
        // most likely to answer with a pane that has no session behind it.
        ("emptyFork", Ok(json!({ "fork": { "session": {} } }))),
        ("failed", Err("transport".to_string())),
        ("neverAnswered", Err("timeout".to_string())),
    ]
}

/// The flags half of the gate.
///
/// Four commands, one call, and two details that are easy to lose: a tag also writes the star, and
/// a cleared tag reaches the daemon as an explicit `null` but reaches the row as an ABSENT field.
/// Both are probed directly rather than left to whichever tags a recording happens to contain, and
/// every tag the real menus can emit is added on top of them from the menu dump.
fn flags_entries(
    core: &Core,
    scenario: &Value,
    menu_dump: Option<&Value>,
    now_ms: u64,
) -> Vec<Value> {
    let rows: Vec<Value> = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tags: Vec<Value> = vec![
        Value::String("favorite".to_string()),
        Value::String("later".to_string()),
        // The clear, which is the leg a port collapses into "say nothing".
        Value::Null,
    ];
    if let Some(dump) = menu_dump {
        let mut found: Vec<Value> = Vec::new();
        collect_tag_values(dump, &mut found);
        for tag in found {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    let mut entries = Vec::new();
    for row in rows.iter().take(FLAGS_ROWS_PER_SCENARIO) {
        let (Some(project_id), Some(session_id)) = (
            row.get("projectId").and_then(Value::as_str),
            row.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        let session = SessionKey::local(project_id, session_id);
        let sidebar_session_id = session.to_sidebar_session_id();
        let mut payloads: Vec<Value> = vec![
            json!({ "type": "setSessionPinned", "sessionId": sidebar_session_id, "pinned": true }),
            json!({ "type": "setSessionPinned", "sessionId": sidebar_session_id, "pinned": false }),
            json!({ "type": "setSessionParked", "sessionId": sidebar_session_id, "parked": true }),
            json!({ "type": "setSessionParked", "sessionId": sidebar_session_id, "parked": false }),
            json!({ "type": "setSessionFavorite", "sessionId": sidebar_session_id, "favorite": true }),
            json!({ "type": "setSessionFavorite", "sessionId": sidebar_session_id, "favorite": false }),
        ];
        for tag in &tags {
            payloads.push(json!({
                "type": "setSessionTag",
                "sessionId": sidebar_session_id,
                "sessionTag": tag,
            }));
        }
        for payload in payloads {
            // Parking is the one leg a setting changes, so both settings are driven.
            for sleep_when_parking in [false, true] {
                let Some(request) = plan_flags_request(&payload, sleep_when_parking) else {
                    entries.push(json!({ "payload": payload, "sleepWhenParking": sleep_when_parking, "owned": false }));
                    continue;
                };
                let mut answers = Map::new();
                for accepted in [true, false] {
                    let follow_ups = apply_flags_answer(&request, accepted, now_ms);
                    let mut store = core.clone();
                    for follow_up in &follow_ups {
                        if let FlagsFollowUp::Patch { session, patch } = follow_up {
                            store.handle(
                                Event::Intent(Intent::PatchSession {
                                    session: session.clone(),
                                    patch: patch.clone(),
                                }),
                                now_ms,
                            );
                        }
                    }
                    answers.insert(
                        match accepted {
                            true => "accepted".to_string(),
                            false => "failed".to_string(),
                        },
                        json!({
                            "follow": Value::Array(
                                follow_ups.iter().map(FlagsFollowUp::to_json).collect(),
                            ),
                            "row": flags_row(&store, &session),
                        }),
                    );
                }
                entries.push(json!({
                    "payload": payload,
                    "sleepWhenParking": sleep_when_parking,
                    "owned": true,
                    "request": request.to_json(),
                    "answers": Value::Object(answers),
                }));
            }
        }
    }
    entries
}

/// Enough rows to cover the shapes without multiplying the whole recording by twenty payloads:
/// the flags path reads nothing about the row, so a second hundred rows would add no branch.
const FLAGS_ROWS_PER_SCENARIO: usize = 12;

/// The four fields a flag call can move, as the sidebar would draw them.
fn flags_row(core: &Core, session: &SessionKey) -> Value {
    match core
        .presentation()
        .machine(&session.machine)
        .and_then(|entry| entry.effective_session(&session.project_id, &session.session_id))
    {
        Some(row) => json!({
            "isPinned": row.is_pinned,
            "isParked": row.is_parked,
            "isFavorite": row.is_favorite,
            "sessionTag": row.session_tag,
        }),
        None => Value::Null,
    }
}

/// Every `sessionTag` value anywhere in a menu dump, so the gate asks about the tags the user's
/// own catalog really offers and not only the two written above.
fn collect_tag_values(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Object(entries) => {
            if entries.get("type") == Some(&Value::String("setSessionTag".to_string())) {
                if let Some(tag) = entries.get("sessionTag") {
                    if !found.contains(tag) {
                        found.push(tag.clone());
                    }
                }
            }
            for item in entries.values() {
                collect_tag_values(item, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_tag_values(item, found);
            }
        }
        _ => {}
    }
}

/// The dialog half of the gate.
///
/// Rename and Note call nothing, so what is compared is the pair of host calls they make: the
/// dismissal the open dialog is closed with, and the payload the new one is seeded with. The seed
/// is the row's own title and note, which is where the title rule lives, so the probe drives every
/// drawn row rather than a sample: a row whose primary title is blank, whose terminal title is
/// blank, or which has neither takes a different branch of the same `||` chain.
fn modal_entries(view: &SidebarView) -> Vec<Value> {
    let mut entries = Vec::new();
    for group in &view.groups {
        for session in &group.core.sessions {
            let sidebar_session_id = session.row.sidebar_session_id.clone();
            for action in ["rename", "note", "firstMessage", "delayedSend"] {
                let payload = json!({
                    "type": "sessionAction",
                    "sessionId": sidebar_session_id,
                    "action": action,
                });
                // The row the seed comes from travels with the entry. Which rows a group holds
                // is M4a's gate, not this one; what this one asks is what the two sides make of
                // the SAME row, so re-deriving the row here would add a second place to differ.
                let row = json!({
                    "primaryTitle": session.row.menu_facts.primary_title,
                    "terminalTitle": session.row.menu_facts.terminal_title,
                    "alias": session.row.alias,
                    "agentIcon": session.row.agent_icon,
                    "sessionNote": session.row.session_note,
                });
                match plan_modal_action(view, &payload) {
                    Some(plan) => entries.push(json!({
                        "payload": payload,
                        "row": row,
                        "owned": true,
                        "action": plan.to_json(),
                    })),
                    None => entries.push(json!({ "payload": payload, "row": row, "owned": false })),
                }
            }
        }
    }
    entries
}

/// The bulk half of the gate.
///
/// A plural payload is a SET and an ORDER over actions that are already gated one at a time, so
/// that is what this compares: which rows, in which order, through which per-session action, and
/// whether the fan-out is paced. None of it is visible in a list comparison and the order is not
/// cosmetic, because a paced sleep sends the requests in exactly this order 350 ms apart.
///
/// Every project of the recording is probed with all four project payloads, and the two explicit
/// ones with id lists taken from the recording plus the shapes no recording contains (an empty
/// list, an id the store does not hold, a browser id, a remote id).
fn bulk_entries(core: &Core, inputs: &SidebarInputs, scenario: &Value) -> Vec<Value> {
    let project_ids: Vec<String> = scenario
        .pointer("/snapshot/projects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| project.get("projectId")?.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut payloads: Vec<Value> = Vec::new();
    for project_id in project_ids.iter().take(BULK_PROJECTS_PER_SCENARIO) {
        let group_id = format!("combined-project:{}", encode_uri_component(project_id));
        payloads.push(json!({ "type": "setGroupSleeping", "groupId": group_id, "sleeping": true }));
        payloads
            .push(json!({ "type": "setGroupSleeping", "groupId": group_id, "sleeping": false }));
        payloads.push(json!({ "type": "wakeProjectSleepingSessions", "groupId": group_id }));
        payloads.push(json!({ "type": "sleepInactiveProjectSessions", "groupId": group_id }));
        payloads.push(json!({ "type": "closeInactiveProjectSessions", "groupId": group_id }));
    }
    // The group shapes that resolve to no local project, each a refusal with its own reason.
    for group_id in SYNTHETIC_GROUP_IDS {
        payloads.push(json!({ "type": "setGroupSleeping", "groupId": group_id, "sleeping": true }));
        payloads.push(json!({ "type": "wakeProjectSleepingSessions", "groupId": group_id }));
    }
    // The explicit lists a multi-selection sends.
    let selected: Vec<String> = session_ids(scenario)
        .into_iter()
        .take(BULK_SELECTED_PER_SCENARIO)
        .collect();
    for ids in [
        selected.clone(),
        Vec::new(),
        vec!["combined-session:nope:nope".to_string()],
        vec!["gpui-browser:P0erj:tab-1".to_string()],
        vec!["remote:machine-1:session:P0erj:S1".to_string()],
    ] {
        payloads.push(
            json!({ "type": "setSessionsSleeping", "sessionIds": ids, "sleeping": true, "source": "sleepBelow" }),
        );
        payloads
            .push(json!({ "type": "setSessionsSleeping", "sessionIds": ids, "sleeping": false }));
        payloads.push(json!({ "type": "closeSessions", "sessionIds": ids }));
    }
    payloads
        .into_iter()
        .map(|payload| match plan_bulk_request(core, inputs, &payload) {
            Some(request) => json!({
                "payload": payload,
                "owned": true,
                "request": request.to_json(),
            }),
            None => json!({ "payload": payload, "owned": false }),
        })
        .collect()
}

/// Enough projects to cover the shapes without multiplying the recording by five payloads. The
/// resolution reads only the project's own rows, so a sixth project adds no branch.
const BULK_PROJECTS_PER_SCENARIO: usize = 5;
const BULK_SELECTED_PER_SCENARIO: usize = 4;

/// The renderer's batch envelope, which is a pass-through and is compared as one.
fn batch_entries() -> Vec<Value> {
    let message = |kind: &str| json!({ "type": kind, "sessionId": "combined-session:P0erj:S1" });
    let commands = vec![
        json!({ "type": "batch", "clearSelection": true, "messages": [message("closeSession")] }),
        json!({ "type": "batch", "messages": [message("closeSession"), message("forkSession")] }),
        json!({ "type": "batch", "messages": [] }),
        json!({ "type": "batch", "clearSelection": false, "messages": [message("closeSession")] }),
        // Not a batch at all, so the planner must not answer it.
        json!({ "type": "command", "message": message("closeSession") }),
    ];
    commands
        .into_iter()
        .map(|command| match plan_batch(&command) {
            Some(plan) => json!({ "command": command, "owned": true, "plan": plan.to_json() }),
            None => json!({ "command": command, "owned": false }),
        })
        .collect()
}

/// The wake-time half of the gate, driven directly from clock facts the harness wrote.
///
/// The rule reads the clock twice (the instant, and the local calendar behind "Tomorrow" and
/// "Next week"), so neither side may read a real clock while the gate runs: the harness pins a
/// time zone, writes a fixed instant per case together with the local offset at that instant and
/// the offset at 09:00 local on each of the next seven days, and both sides answer for exactly
/// those. A case list chosen here instead would be a list of instants that miss the two days a
/// year the answer is hard.
fn snooze_clock_entries(clock_file: Option<&Value>) -> Vec<Value> {
    let cases = clock_file
        .and_then(|file| file.get("cases"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    cases
        .iter()
        .filter_map(|case| {
            let clock = snooze_clock_from_case(case)?;
            let mut wake = Map::new();
            for preset in SESSION_SNOOZE_PRESETS {
                wake.insert(
                    preset.to_string(),
                    match snooze_wake_ms(preset, &clock) {
                        Some(ms) => Value::String(iso_string_from_ms(ms)),
                        None => Value::Null,
                    },
                );
            }
            // An unknown preset is refused rather than answered, and the harness asserts the
            // TypeScript throws on it rather than posting a call.
            wake.insert(
                "notAPreset".to_string(),
                match snooze_wake_ms("notAPreset", &clock) {
                    Some(ms) => Value::String(iso_string_from_ms(ms)),
                    None => Value::Null,
                },
            );
            Some(json!({
                "case": case.get("case").cloned().unwrap_or(Value::Null),
                "nowMs": clock.now_ms,
                "wake": Value::Object(wake),
            }))
        })
        .collect()
}

fn snooze_clock_from_case(case: &Value) -> Option<SnoozeClock> {
    let now_ms = case.get("nowMs")?.as_i64()?;
    let offset_ms = case.get("offsetMs")?.as_i64()?;
    let mut clock = SnoozeClock::fixed(now_ms, offset_ms);
    let offsets = case.get("morningOffsetsMs")?.as_array()?;
    for (index, slot) in clock.morning_offset_ms.iter_mut().enumerate() {
        if let Some(offset) = offsets.get(index).and_then(Value::as_i64) {
            *slot = offset;
        }
    }
    Some(clock)
}

/// The boundary half: the exact moment a snooze ends.
///
/// The action surface is not the only thing that has to agree about it. The section a row is drawn
/// in and the menu item that offers Snooze or Unsnooze read the same instant, and a port where
/// those drift by a tick draws a row under the Snoozed heading whose own menu already offers the
/// presets. So the probe reports what the shared predicate says AND which section the row really
/// lands in, rebuilt through the whole view model, and the harness holds both against the one
/// TypeScript predicate.
fn snooze_boundary_entries(core: &Core, scenario: &Value, view: &SidebarView) -> Vec<Value> {
    // A row the list really draws, so the section below is a section and not a `None`.
    let Some((session, row_id, storage_id)) = view
        .groups
        .iter()
        .find_map(|group| {
            let row = group
                .core
                .sessions
                .iter()
                .find(|session| !session.row.is_browser)?;
            Some((row, group.core.storage_id.clone()))
        })
        .and_then(|(session, storage_id)| {
            Some((
                SessionKey::parse_sidebar_session_id(&session.row.sidebar_session_id)?,
                session.row.sidebar_session_id.clone(),
                storage_id,
            ))
        })
    else {
        return Vec::new();
    };
    // Every heading open and the whole list shown, because a `SectionView` carries the rows it
    // DRAWS: Snoozed starts collapsed, so a probe under the defaults read `None` for exactly the
    // cases it exists to check and would have agreed with any answer at all.
    let mut inputs = SidebarInputs::default();
    inputs.ui.collapse.section_collapse.insert(
        storage_id.clone(),
        SectionCollapse {
            browser: false,
            pinned: false,
            drafts: false,
            sessions: false,
            parked: false,
            snoozed: false,
        },
    );
    inputs.ui.collapse.expanded_session_lists.insert(storage_id);
    let Some(server_row) = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|row| {
                row.get("projectId").and_then(Value::as_str) == Some(session.project_id.as_str())
                    && row.get("sessionId").and_then(Value::as_str)
                        == Some(session.session_id.as_str())
            })
        })
        .cloned()
    else {
        return Vec::new();
    };
    // A round local instant, and the four stamps around it that decide the boundary.
    let now_ms: i64 = 1_800_000_000_000;
    let cases: Vec<(&str, Option<String>)> = vec![
        ("aTickBefore", Some(iso_string_from_ms(now_ms - 1))),
        ("exactly", Some(iso_string_from_ms(now_ms))),
        ("aTickAfter", Some(iso_string_from_ms(now_ms + 1))),
        ("aMinuteAfter", Some(iso_string_from_ms(now_ms + 60_000))),
        ("longPast", Some(iso_string_from_ms(now_ms - 86_400_000))),
        ("absent", None),
        ("empty", Some(String::new())),
        ("notADate", Some("not-a-date".to_string())),
    ];
    cases
        .into_iter()
        .map(|(name, snoozed_until)| {
            let mut echoed = core.clone();
            let mut echo_row = server_row.clone();
            match &snoozed_until {
                Some(value) => {
                    echo_row["snoozedUntil"] = Value::String(value.clone());
                }
                None => {
                    if let Some(object) = echo_row.as_object_mut() {
                        object.remove("snoozedUntil");
                    }
                }
            }
            let frame = json!({
                "type": "presentationDelta",
                "protocolVersion": 1,
                "serverId": "parity",
                "clientId": "parity",
                "revision": now_revision(&echoed, &session),
                "delta": { "type": "sessionUpdated", "session": echo_row },
            });
            let parsed = ServerEvent::parse(&frame.to_string())
                .unwrap_or_else(|error| panic!("the snooze echo frame does not parse: {error:?}"));
            let output = echoed.handle(
                Event::Frame {
                    machine: MachineId::Local,
                    frame: Box::new(parsed),
                },
                now_ms as u64,
            );
            assert!(
                output.changes.ignored.is_none(),
                "the snooze echo frame was ignored: {:?}",
                output.changes.ignored
            );
            let mut model = SidebarViewModel::new();
            model.update(&echoed, &inputs, &output.changes, now_ms as u64);
            let drawn = model
                .view()
                .groups
                .iter()
                .flat_map(|group| group.core.sessions.iter())
                .find(|session| session.row.sidebar_session_id == row_id);
            let snoozed_until_ms = drawn.and_then(|session| session.row.timing.snoozed_until_ms);
            let section = model
                .view()
                .groups
                .iter()
                .flat_map(|group| group.core.sections.iter())
                .find(|section| section.session_ids.iter().any(|id| *id == row_id))
                .map(|section| section.id.as_str().to_string());
            json!({
                "case": name,
                "snoozedUntil": snoozed_until,
                "nowMs": now_ms,
                "parsedMs": snoozed_until_ms,
                "isSnoozed": session_is_snoozed(snoozed_until_ms, now_ms as u64),
                "section": section,
            })
        })
        .collect()
}

/// The menu row: what `sessionAction: snooze` posts, for every preset, both tag shapes and the
/// absent one, under every clock case the harness wrote.
fn snooze_action_entries(view: &SidebarView, clock_file: Option<&Value>) -> Vec<Value> {
    let cases = clock_file
        .and_then(|file| file.get("cases"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows: Vec<String> = view
        .groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .filter(|session| !session.row.is_browser)
        .take(SNOOZE_ACTION_ROWS_PER_SCENARIO)
        .map(|session| session.row.sidebar_session_id.clone())
        .collect();
    let mut entries = Vec::new();
    for case in &cases {
        let Some(clock) = snooze_clock_from_case(case) else {
            continue;
        };
        for sidebar_session_id in &rows {
            for payload in snooze_action_payloads(sidebar_session_id) {
                entries.push(snooze_action_entry(view, &payload, &clock));
            }
        }
    }
    // A row the list does not draw stops the whole action before the switch, which is the one
    // refusal the menu itself cannot produce and the harness asserts posts nothing.
    if let Some(clock) = cases.first().and_then(snooze_clock_from_case) {
        for payload in snooze_action_payloads("combined-session:nope:nope") {
            entries.push(snooze_action_entry(view, &payload, &clock));
        }
    }
    entries
}

fn snooze_action_entry(view: &SidebarView, payload: &Value, clock: &SnoozeClock) -> Value {
    let planned = plan_snooze_action(view, payload, clock);
    // Whether the list draws the row travels with the entry rather than being decided again on
    // the other side: which rows a group holds is M4a's gate, and what this one asks is what the
    // two sides make of the SAME row. The TypeScript half seeds its store from this.
    let drawn = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .is_some_and(|id| {
            view.groups
                .iter()
                .flat_map(|group| group.core.sessions.iter())
                .any(|session| session.row.sidebar_session_id == id)
        });
    json!({
        "payload": payload,
        "nowMs": clock.now_ms,
        "drawn": drawn,
        "owned": planned.is_some(),
        "messages": planned.map(|action| action.to_json()).unwrap_or(Value::Null),
    })
}

/// Enough rows to cover the shapes: the action reads nothing about a row beyond whether it is
/// drawn, so a second hundred rows would add no branch and would multiply by every clock case.
const SNOOZE_ACTION_ROWS_PER_SCENARIO: usize = 2;

/// Every shape `snooze_with_tag` and the plain preset row can build, plus the two the renderer can
/// be handed with no preset at all.
fn snooze_action_payloads(sidebar_session_id: &str) -> Vec<Value> {
    let mut payloads = Vec::new();
    for preset in SESSION_SNOOZE_PRESETS {
        for tag in [None, Some(Value::Null), Some(Value::String("later".into()))] {
            let mut payload = json!({
                "type": "sessionAction",
                "sessionId": sidebar_session_id,
                "action": "snooze",
                "preset": preset,
            });
            if let Some(tag) = tag {
                payload["sessionTag"] = tag;
            }
            payloads.push(payload);
        }
    }
    payloads.push(json!({
        "type": "sessionAction",
        "sessionId": sidebar_session_id,
        "action": "snooze",
        "sessionTag": "favorite",
    }));
    payloads.push(json!({
        "type": "sessionAction",
        "sessionId": sidebar_session_id,
        "action": "snooze",
    }));
    // `if (command.preset)` is a truthiness test, so the empty string posts nothing.
    payloads.push(json!({
        "type": "sessionAction",
        "sessionId": sidebar_session_id,
        "action": "snooze",
        "preset": "",
    }));
    payloads
}

/// The two calls: what they send, and what an accepted and a refused answer do.
fn snooze_call_entries(scenario: &Value) -> Vec<Value> {
    let rows: Vec<Value> = scenario
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sidebar_session_ids: Vec<String> = rows
        .iter()
        .take(SNOOZE_CALL_ROWS_PER_SCENARIO)
        .filter_map(|row| {
            Some(
                SessionKey::local(
                    row.get("projectId")?.as_str()?,
                    row.get("sessionId")?.as_str()?,
                )
                .to_sidebar_session_id(),
            )
        })
        .collect();
    sidebar_session_ids
        .push(SessionKey::local(QUICK_AUTOMATIONS_PROJECT_ID, "quick-1").to_sidebar_session_id());
    sidebar_session_ids.push("combined-session:unknown-project:unknown-session".to_string());
    sidebar_session_ids.push("gpui-browser:P0erj:tab-1".to_string());
    sidebar_session_ids.push("remote:machine-1:session:P0erj:S1".to_string());
    sidebar_session_ids.push(String::new());
    let mut entries = Vec::new();
    for sidebar_session_id in &sidebar_session_ids {
        let payloads = vec![
            json!({
                "type": "snoozeSession",
                "sessionId": sidebar_session_id,
                "snoozedUntil": "2026-09-21T09:00:00.000Z",
            }),
            // No `snoozedUntil` at all: the TypeScript spreads an `undefined` and `JSON.stringify`
            // drops the key, so the call must carry no key either.
            json!({ "type": "snoozeSession", "sessionId": sidebar_session_id }),
            json!({ "type": "unsnoozeSession", "sessionId": sidebar_session_id }),
        ];
        for payload in payloads {
            let Some(request) = plan_snooze_request(&payload) else {
                entries.push(json!({ "payload": payload, "owned": false }));
                continue;
            };
            let mut answers = Map::new();
            for accepted in [true, false] {
                answers.insert(
                    match accepted {
                        true => "accepted".to_string(),
                        false => "failed".to_string(),
                    },
                    Value::Array(
                        apply_snooze_answer(&request, accepted)
                            .iter()
                            .map(SnoozeFollowUp::to_json)
                            .collect(),
                    ),
                );
            }
            entries.push(json!({
                "payload": payload,
                "owned": true,
                "request": request.to_json(),
                "answers": Value::Object(answers),
            }));
        }
    }
    entries
}

const SNOOZE_CALL_ROWS_PER_SCENARIO: usize = 6;

/// The seed-title rule, driven directly.
///
/// A recording is not guaranteed to contain a padded or blank title, and this one does not: the
/// gate's own untrimmed-title mutation produced ZERO differences when the rule was only ever
/// reached through drawn rows, which is a gate that cannot fail. The triples below reach every
/// branch of the `||` chain whatever the recording holds.
fn title_rule_entries() -> Vec<Value> {
    let cases: [(Option<&str>, Option<&str>, &str); 12] = [
        (Some("Primary"), Some("Terminal"), "alias"),
        // Padded: the trim decides the VALUE, not only the test.
        (Some("  Primary  "), Some("Terminal"), "alias"),
        (Some("\tPrimary\n"), None, "alias"),
        // Blank primary falls through to the terminal title.
        (Some("   "), Some("Terminal"), "alias"),
        (Some(""), Some("  Terminal  "), "alias"),
        (None, Some("Terminal"), "alias"),
        // Both blank falls through to the alias, which is NOT trimmed.
        (Some("   "), Some("   "), "  alias  "),
        (None, None, "  alias  "),
        (Some(""), Some(""), ""),
        (None, None, ""),
        // The JavaScript trim also removes these two, which Rust's `str::trim` does not.
        (Some("\u{0085}Primary\u{0085}"), None, "alias"),
        (Some("\u{feff}   \u{feff}"), Some("Terminal"), "alias"),
    ];
    cases
        .into_iter()
        .map(|(primary, terminal, alias)| {
            json!({
                "primaryTitle": primary,
                "terminalTitle": terminal,
                "alias": alias,
                "title": rename_seed_title(primary, terminal, alias),
            })
        })
        .collect()
}
