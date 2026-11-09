//! The GBots Team window preview (Import Team…, Export Team…). States
//! (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (a team file's preview: three GBots, a department,
//! two paused routines, a renamed GBot, what was left out and an ignored field), `imported` (the
//! import done, with Undo), `undone`, `preset` (a preset file's preview), `problems` (a file that
//! can't import), `paste` (the browser's paste box), `export` (Export Team, whole team),
//! `export-bot` (One GBot picked), `exported` (saved, with what was left out). Import answers after
//! a second with the `imported` answer; Undo with the `undone` one.
use super::gbot_team_modal::*;
use gpui::{App, AppContext as _};
use serde_json::{Value, json};
use std::rc::Rc;
use std::time::Duration;

fn preview(state: &str) -> Value {
    if state == "preset" {
        return json!({
            "kind": "preset", "name": "Fare scout", "summary": "",
            "bots": [{ "key": "fare-scout", "name": "Fare scout", "title": "Watches flight prices", "icon": "rocket", "colorHex": "#59d9ff",
                       "starterNotes": { "memoryLines": 3, "topics": ["airlines"] }, "routines": 0 }],
            "departments": [], "routines": [], "leftOut": [], "ignored": [], "problems": [], "notes": [], "warnings": [],
            "approvalSentence": "", "canImport": false,
        });
    }
    let problems = if state == "problems" {
        json!([
            "bots[1]: colour teal is not one of periwinkle, sky, mint, gold, coral, lavender, pink, rose.",
            "The routine Fare watch belongs to scout, which the file does not hold."
        ])
    } else {
        json!([])
    };
    json!({
        "kind": "team",
        "name": "Travel desk",
        "summary": "Plans trips, watches prices and keeps the itinerary.",
        "bots": [
            { "key": "planner", "name": "Trip planner", "title": "Plans the trip end to end", "icon": "checklist", "colorHex": "#4ee6b8",
              "department": "Travel", "isLead": true, "starterNotes": { "memoryLines": 4, "topics": [] }, "routines": 1 },
            { "key": "researcher", "name": "Researcher 2", "renamedFrom": "Researcher", "title": "Finds and compares options", "icon": "microscope", "colorHex": "#59d9ff",
              "department": "Travel", "isLead": false, "starterNotes": { "memoryLines": 2, "topics": ["airlines", "hotels"] }, "routines": 1 },
            { "key": "writer", "name": "Itinerary writer", "title": "Writes the day-by-day plan", "icon": "pencil", "colorHex": "#d2a7ff",
              "starterNotes": { "memoryLines": 0, "topics": [] }, "routines": 0 },
        ],
        "departments": [{ "name": "Travel", "lead": "Trip planner", "members": ["Trip planner", "Researcher 2"] }],
        "routines": [
            { "bot": "Researcher 2", "name": "Fare watch", "scheduleSentence": "Every day at 9:00", "precheckSentence": "the page https://example.com/fares" },
            { "bot": "Trip planner", "name": "Weekly check-in", "scheduleSentence": "Mondays at 8:30" },
        ],
        "leftOut": ["1 secret redacted in Researcher's instructions"],
        "ignored": ["bots[0].voice"],
        "notes": ["There is already a GBot called Researcher, so this one is added as Researcher 2."],
        "warnings": [],
        "problems": problems,
        "approvalSentence": "They ask before they act (approval Ask), as every new GBot does; change that in GBot Settings.",
        "canImport": state != "problems",
    })
}

fn imported() -> Value {
    json!({
        "changeId": "c_demo",
        "name": "Travel desk",
        "bots": [
            { "name": "Trip planner", "title": "Plans the trip end to end", "icon": "checklist", "colorHex": "#4ee6b8" },
            { "name": "Researcher 2", "title": "Finds and compares options", "icon": "microscope", "colorHex": "#59d9ff" },
            { "name": "Itinerary writer", "title": "Writes the day-by-day plan", "icon": "pencil", "colorHex": "#d2a7ff" },
        ],
        "routines": [{ "name": "Fare watch" }, { "name": "Weekly check-in" }],
        "skipped": [],
    })
}

fn undone() -> Value {
    json!({ "done": {
        "removedBots": ["Trip planner", "Researcher 2", "Itinerary writer"],
        "removedRoutines": 2,
        "removedDepartments": ["Travel"],
        "keptDepartments": [],
    }})
}

fn list() -> Value {
    json!({
        "bots": [
            { "botId": "b_planner", "name": "Trip planner", "title": "Plans the trip end to end", "icon": "checklist", "colorHex": "#4ee6b8" },
            { "botId": "b_researcher", "name": "Researcher", "title": "Finds and compares options", "icon": "microscope", "colorHex": "#59d9ff" },
            { "botId": "b_writer", "name": "Itinerary writer", "title": "Writes the day-by-day plan", "icon": "pencil", "colorHex": "#d2a7ff" },
        ],
        "departments": [{ "departmentId": "d_travel", "name": "Travel", "members": [{}, {}] }],
    })
}

fn exported() -> Value {
    json!({
        "kind": "team", "bots": 3, "departments": 1, "routines": 2,
        "fileName": "travel-desk.gbots-team.json",
        "savedTo": "~/Downloads/travel-desk.gbots-team.json",
        "leftOut": ["1 secret redacted in Researcher's instructions"],
        "routinesArrivePaused": true,
    })
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let host: GbotTeamModalHost = Rc::new(move |command, cx: &mut App| {
        let later = |reply: GbotTeamReply, answer: Value, cx: &mut App| {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                cx.update(|cx| reply.finish(Ok(answer), cx));
            })
            .detach();
        };
        match command {
            GbotTeamModalCommand::Preview { reply, .. } => {
                reply.finish(Ok(json!({ "preview": preview("") })), cx)
            }
            GbotTeamModalCommand::Import { reply, .. } => later(reply, imported(), cx),
            GbotTeamModalCommand::Undo { reply, .. } => later(reply, undone(), cx),
            GbotTeamModalCommand::KeepPreset { reply, .. }
            | GbotTeamModalCommand::KeepBotPreset { reply, .. } => later(
                reply,
                json!({ "preset": preview("preset")["bots"][0].clone() }),
                cx,
            ),
            GbotTeamModalCommand::Export { reply, .. } => later(reply, exported(), cx),
            GbotTeamModalCommand::Close => cx.quit(),
        }
    });
    let start = match state.as_str() {
        "paste" => GbotTeamStart::Paste { error: None },
        "export" | "export-bot" | "exported" => GbotTeamStart::Export {
            list: list(),
            can_save: true,
        },
        other => GbotTeamStart::Preview {
            text: "{}".to_string(),
            preview: preview(other),
        },
    };
    let config = GbotTeamModalConfig {
        palette: demo.palette,
        start,
    };
    let (window, view) = super::open_modal_window(
        GBOT_TEAM_MODAL_WIDTH,
        GBOT_TEAM_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiGbotTeamModalWindow::new(config, host, window, cx)),
        cx,
    );
    // The after-states are reached the way a click reaches them.
    let _ = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| modal.demo_jump(&state, window, cx));
    });
}
