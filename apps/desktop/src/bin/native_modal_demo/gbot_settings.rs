//! A GBot's settings window preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (the
//! Profile tab), `instructions` (the Instructions tab with the SOUL.md notice), `memory` (the
//! Memory tab), `memory-full` (the Memory tab nearly full, with an undoable change), `memory-edit`
//! (the memory editor whose save is refused because the GBot wrote meanwhile), `failed` (Save
//! answers with gxserver's refusal after a second).
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
        "departmentId": null,
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
            GbotSettingsModalCommand::Reveal { path } => eprintln!("reveal {path}"),
            GbotSettingsModalCommand::Cancel => {
                eprintln!("cancel");
                cx.quit();
            }
        }
        },
    );
    let config = GbotSettingsModalConfig {
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
            _ => GbotSettingsTab::Profile,
        },
    };
    // The memory states open tall, so one snapshot shows the whole tab.
    let height = if demo.state.starts_with("memory") {
        1480.0
    } else {
        GBOT_SETTINGS_MODAL_INITIAL_HEIGHT
    };
    let _ = super::open_modal_window(
        GBOT_SETTINGS_MODAL_WIDTH,
        height,
        move |window, cx| cx.new(|cx| GpuiGbotSettingsModalWindow::new(config, host, window, cx)),
        cx,
    );
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
