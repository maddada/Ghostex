//! A GBot's settings window preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (the
//! Profile tab), `instructions` (the Instructions tab with the SOUL.md notice), `memory` (the
//! Memory tab), `memory-full` (the Memory tab nearly full, with an undoable change), `memory-edit`
//! (the memory editor whose save is refused because the GBot wrote meanwhile), `department` (the
//! Department tab, Researcher leading Research), `computer` (the Computer tab: Researcher on the
//! connected computer Studio, with a Cloud Box listed), `bot-api` (the Bot API tab, off; its switch
//! turns it on with a demo key), `bot-api-key` (just turned on: the key shown once in the command),
//! `bot-api-on` (on, used, the key no longer shown), `failed` (Save answers with gxserver's refusal
//! after a second), `voice` (Profile tall, with its Voice row: Researcher in the xAI voice Rex),
//! `voice-menu` (the same with the voice list open).
use super::gbot_settings_modal::*;
use gpui::{App, AppContext as _};
use serde_json::json;
use std::rc::Rc;
use std::time::Duration;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let bot = json!({
        "botId": "bot-demo",
        "name": "Researcher",
        "title": "Finds and compares options",
        "description": "Looks things up across the web and my notes, then sums up the best two or three choices with prices and links.",
        "soul": "Keep a running shortlist and say why each option made it.\nAsk before buying anything.\nPrefer sources from the last year.",
        "icon": "microscope",
        "color": "sky",
        "agentId": "claude",
        "model": null,
        "approval": "ask",
        "departmentId": (demo.state == "department").then_some("d_research"),
        "departmentName": (demo.state == "department").then_some("Research"),
        "isLead": demo.state == "department",
        "voice": "rex",
        "voiceChoices": {
            "provider": "xai",
            "providerLabel": "xAI",
            "defaultVoice": "eve",
            "voiceOptions": {
                "xai": ["eve", "ara", "rex", "sal", "leo", "altair", "atlas", "aurora"],
                "openai": ["marin", "cedar"],
            },
        },
        "soulDriftMessage": (demo.state == "instructions").then_some(
            "SOUL.md was edited outside Ghostex, so Researcher still follows the instructions saved in Ghostex. Save new instructions with `ghostex gbots edit Researcher --soul-file <file>`."
        ),
    });
    let status = json!({ "agents": [
        { "agentId": "claude", "name": "Claude Code" },
        { "agentId": "codex", "name": "Codex" },
    ]});
    let record = GbotSettingsRecord::from_show(&bot).expect("the demo bot");
    let fail = demo.state == "failed";
    let state = demo.state.clone();
    let api_state = demo.state.clone();
    let host: Rc<dyn Fn(GbotSettingsModalCommand, &mut App)> = Rc::new(
        move |command, cx: &mut App| {
            match command {
            GbotSettingsModalCommand::Save { changes, reply, .. } => {
                eprintln!("save {}", serde_json::Value::Object(changes));
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    cx.update(|cx| {
                        let result = if fail {
                            Err("There is already a GBot called Writer. Pick another name."
                                .to_string())
                        } else {
                            Ok(())
                        };
                        let ok = result.is_ok();
                        reply.finish(result, cx);
                        if ok {
                            cx.quit();
                        }
                    });
                })
                .detach();
            }
            GbotSettingsModalCommand::MemoryLoad { reply, .. } => {
                reply.finish(Ok(memory_fixture(&state)), cx)
            }
            GbotSettingsModalCommand::MemorySave { reply, .. } => reply.finish(
                Err("MEMORY.md changed while you were editing it (Researcher wrote to it), so your text was not saved. Reload to see the change, then edit again.".to_string()),
                cx,
            ),
            GbotSettingsModalCommand::MemoryUndo { reply, .. } => {
                reply.finish(Ok(memory_fixture("memory")), cx)
            }
            GbotSettingsModalCommand::Routines { op, reply, .. } => {
                if let Some(op) = op {
                    eprintln!("routine {op}");
                }
                reply.finish(Ok(routines_fixture()), cx)
            }
            GbotSettingsModalCommand::Departments { ops, reply, .. } => {
                for op in &ops {
                    eprintln!("department {op}");
                }
                reply.finish(Ok(departments_fixture()), cx)
            }
            GbotSettingsModalCommand::Computer { op, reply, .. } => {
                if let Some(op) = op {
                    eprintln!("computer {op}");
                }
                // Answered on the next tick, as gxserver's answer arrives, not inside the window's
                // own update.
                cx.spawn(async move |cx| {
                    cx.update(|cx| reply.finish(Ok(computer_fixture()), cx));
                })
                .detach();
            }
            GbotSettingsModalCommand::BotApi { op, reply, .. } => {
                let op = op.and_then(|op| op["op"].as_str().map(str::to_string));
                if let Some(op) = &op {
                    eprintln!("bot api {op}");
                }
                let answer = match op.as_deref() {
                    Some("apiOn" | "apiNewKey") => api_fixture(true, false, true),
                    Some("apiOff") => api_fixture(false, false, false),
                    _ => match api_state.as_str() {
                        "bot-api-key" => api_fixture(true, false, true),
                        "bot-api-on" => api_fixture(true, true, false),
                        _ => api_fixture(false, false, false),
                    },
                };
                cx.spawn(async move |cx| {
                    cx.update(|cx| reply.finish(Ok(answer), cx));
                })
                .detach();
            }
            GbotSettingsModalCommand::Reveal { path } => eprintln!("reveal {path}"),
            GbotSettingsModalCommand::Cancel => {
                eprintln!("cancel");
                cx.quit();
            }
        }
        },
    );
    let config = GbotSettingsModalConfig {
        departments: (demo.state == "department").then(departments_fixture),
        memory: demo
            .state
            .starts_with("memory")
            .then(|| memory_fixture(&demo.state)),
        agents: gbot_agent_choices(Some(&status), &record.agent_id),
        record,
        palette: demo.palette,
        tab: match demo.state.as_str() {
            "instructions" => GbotSettingsTab::Instructions,
            "memory" | "memory-full" | "memory-edit" => GbotSettingsTab::Memory,
            "routines" => GbotSettingsTab::Routines,
            "department" => GbotSettingsTab::Department,
            "computer" => GbotSettingsTab::Computer,
            "bot-api" | "bot-api-key" | "bot-api-on" => GbotSettingsTab::BotApi,
            _ => GbotSettingsTab::Profile,
        },
    };
    // The memory states open tall, so one snapshot shows the whole tab.
    let height = if demo.state.starts_with("memory") || demo.state == "department" {
        1480.0
    } else if demo.state.starts_with("voice") {
        // Profile tall enough that Runs on and its Voice row show, with the voice list open.
        1320.0
    } else {
        GBOT_SETTINGS_MODAL_INITIAL_HEIGHT
    };
    let open_voices = demo.state == "voice-menu";
    let (window, view) = super::open_modal_window(
        GBOT_SETTINGS_MODAL_WIDTH,
        height,
        move |window, cx| cx.new(|cx| GpuiGbotSettingsModalWindow::new(config, host, window, cx)),
        cx,
    );
    // `voice-menu`: the Voice row's list open (after the first frame placed its trigger).
    if open_voices {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            let _ = window.update(cx, |_, _, cx| {
                view.update(cx, |view, cx| view.toggle_voice_select(cx));
            });
        })
        .detach();
    }
}

/// A `computer` answer for the demo: Researcher on the connected computer Studio, with computer
/// use on, one more computer whose Agent access is off, and a Cloud Box.
fn computer_fixture() -> serde_json::Value {
    json!({
        "computer": {
            "kind": "computer", "computer": "computer:S42", "name": "Studio",
            "folder": "~/GBots/Researcher", "computerUse": true, "available": true,
        },
        "thisComputer": "MacBook",
        "choices": {
            "computers": [
                { "serverId": "S42", "name": "Studio", "agentAccess": true, "computer": "computer:S42" },
                { "serverId": "S43", "name": "Build box", "agentAccess": false, "computer": "computer:S43" },
            ],
            "cloudBoxes": [
                { "boxId": "gx_c31", "name": "box-c31", "state": "stopped", "provider": "Instabox",
                  "projectName": "Ghostex", "monthUsd": 1.42, "computer": "cloudbox:gx_c31" },
            ],
        },
        "cloudSpend": { "totalUsd": 1.42, "capUsd": 20.0 },
        "remoteChats": [ { "globalRef": "S42:P1:G1", "title": "Compare flights" } ],
    })
}

/// A `routines` answer for the demo: a watcher with a check, a monthly report sent to Chief of
/// Staff, and a paused one.
fn routines_fixture() -> serde_json::Value {
    json!({ "routines": [
        {
            "id": "automation-1", "name": "Inbox watch", "paused": false,
            "scheduleSentence": "Every 15 minutes", "nextLabel": "Mon 9 Nov, 09:15",
            "precheckSentence": "the files in ~/inbox", "deliver": "chat",
            "check": { "lastCheckOutcome": "quiet", "lastCheckNote": "Nothing changed." },
            "lastRun": { "status": "findings", "at": "2026-11-09T08:30:00Z", "summary": "invoice-march.pdf arrived; it is the March invoice from Acme." },
        },
        {
            "id": "automation-2", "name": "Monthly report", "paused": false,
            "scheduleSentence": "Every month on the 1st at 09:00 (Asia/Dubai time)", "nextLabel": "Tue 1 Dec, 09:00",
            "deliver": "chief",
        },
        {
            "id": "automation-3", "name": "Weekday brief", "paused": true,
            "scheduleSentence": "Every weekday at 08:30", "precheckSentence": "new mail in Gmail",
            "deliver": "chat",
            "check": { "lastCheckOutcome": "failed", "lastCheckNote": "Connectors are off. Turn them on in Settings > Extensions." },
        },
    ]})
}

/// A `memoryShow` answer for the demo: a few notes, two topics, a day's log and the journal;
/// `memory-full` is nearly at what loads.
fn memory_fixture(state: &str) -> serde_json::Value {
    let full = state == "memory-full";
    let mut text = String::from(
        "# Researcher's memory\n\nWhat Researcher keeps between chats: one line per fact, newest last.\n\n- 2026-11-02 · Prefers aisle seats on flights over two hours\n- 2026-11-05 · Budget for the Lisbon trip is 1,800 EUR, flexible by 10%\n- 2026-11-08 · Hotel shortlist lives in files/lisbon-hotels.md · until 2026-12-01\n- 2026-11-09 · Asked for prices with taxes included, always\n",
    );
    if full {
        for day in 1..=40 {
            text.push_str(&format!(
                "- 2026-10-{:02} · Compared fares for the March trip, round {day}\n",
                day % 28 + 1
            ));
        }
    }
    let lines = text.lines().count();
    let bytes = text.len();
    let share = (lines as f64 / 200.0).max(bytes as f64 / 24576.0);
    let share = if full { 0.86 } else { share };
    json!({
        "memory": {
            "text": text, "hash": "demo", "path": "/Users/you/GBots/researcher/MEMORY.md",
            "lines": if full { 172 } else { lines }, "bytes": if full { 18400 } else { bytes },
            "maxLines": 200, "maxBytes": 24576, "share": share,
            "level": if full { "amber" } else { "ok" },
        },
        "archive": { "notes": if full { 23 } else { 0 } },
        "loads": { "instructionsLines": 3, "preferencesLines": 4, "topicsIndexed": 2, "team": 3 },
        "topics": [
            { "name": "airlines", "title": "Airlines", "description": "Which ones to book and which to avoid", "bytes": 1840, "path": "/Users/you/GBots/researcher/memory/airlines.md" },
            { "name": "lisbon", "title": "Lisbon trip", "description": "Dates, neighbourhoods, what is booked", "bytes": 3210, "path": "/Users/you/GBots/researcher/memory/lisbon.md" },
        ],
        "log": [
            { "date": "2026-11-09", "lines": [
                "09:12 · Flight options for March · Found three non-stop flights under 300 EUR.",
                "11:40 · Hotel shortlist · Narrowed the shortlist to two hotels in Príncipe Real.",
                "14:05 · Laptop comparison · The M4 Air is the better buy at your budget.",
            ]},
            { "date": "2026-11-08", "lines": ["17:30 · Flight options for March · Started a fare table in files/fares.md."] },
        ],
        "today": "2026-11-09",
        "journal": [
            { "id": "m6", "at": "2026-11-09T13:05:00Z", "byLabel": "Researcher", "what": "Added a note", "undoable": true },
            { "id": "m5", "at": "2026-11-09T09:40:00Z", "byLabel": "You", "what": "Edited MEMORY.md" },
            { "id": "m4", "at": "2026-11-08T16:31:00Z", "byLabel": "Chief of Staff", "what": "Added a note to lisbon" },
            { "id": "m3", "at": "2026-11-08T10:02:00Z", "byLabel": "Researcher", "what": "Forgot a note", "undone": true },
        ],
    })
}

/// The Department tab's read: two departments, Researcher leading Research.
fn departments_fixture() -> serde_json::Value {
    json!({
        "departments": { "departments": [
            {
                "departmentId": "d_research", "name": "Research", "icon": "",
                "instructions": "Cite a source for every fact.\nSay how sure you are.",
                "lead": { "botId": "bot-demo", "name": "Researcher" },
                "members": [
                    { "botId": "bot-demo", "name": "Researcher", "isLead": true },
                    { "botId": "b2", "name": "Scout", "isLead": false },
                ],
            },
            {
                "departmentId": "d_writing", "name": "Writing", "icon": "",
                "instructions": "Plain words, short sentences.",
                "lead": { "botId": "b3", "name": "Writer" },
                "members": [
                    { "botId": "b3", "name": "Writer", "isLead": true },
                    { "botId": "b4", "name": "Editor", "isLead": false },
                ],
            },
        ]},
        "bot": { "botId": "bot-demo", "departmentId": "d_research", "isLead": true },
    })
}

/// An `api` answer for the demo: off, or on (`used` with a last use), with the key just made when
/// `key` (the answer of Turn on and New key).
fn api_fixture(on: bool, used: bool, key: bool) -> serde_json::Value {
    let url = "http://127.0.0.1:58744/hooks/gbots/bots/bot-demo/messages";
    let shown = "gxb_q3Vd8sLmT0aYpWnR4eXcJ7uK2hB9fGzN1oPiQ6wEr5A";
    let command = format!(
        "curl -s {url} \\\n  -H 'Authorization: Bearer {}' \\\n  -H 'Content-Type: application/json' \\\n  -d '{{\"message\": \"Hello\"}}'",
        if key { shown } else { "<your GBot API key>" }
    );
    let mut answer = json!({
        "bot": { "botId": "bot-demo", "name": "Researcher" },
        "api": {
            "on": on,
            "key": on.then(|| json!({
                "keyId": "k_demo",
                "label": "Home script",
                "createdAt": "2026-11-09T08:12:40.000Z",
                "lastUsedAt": used.then_some("2026-11-09T09:41:05.000Z"),
                "uses": if used { 14 } else { 0 },
            })),
            "url": url,
            "command": command,
            "reach": "Apps and scripts on this computer, and devices that already reach it over SSH (a paired phone, another computer of yours). Ghostex opens no port for it.",
        },
    });
    if key {
        answer["key"] = json!(shown);
    }
    answer
}
