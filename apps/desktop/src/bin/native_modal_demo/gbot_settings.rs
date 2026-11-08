//! A GBot's settings window preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (the
//! Profile tab), `instructions` (the Instructions tab with the SOUL.md notice), `memory` (the
//! Memory tab), `failed` (Save answers with gxserver's refusal after a second).
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
    let host: Rc<dyn Fn(GbotSettingsModalCommand, &mut App)> =
        Rc::new(move |command, cx: &mut App| match command {
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
            GbotSettingsModalCommand::Cancel => {
                eprintln!("cancel");
                cx.quit();
            }
        });
    let config = GbotSettingsModalConfig {
        agents: gbot_agent_choices(Some(&status), &record.agent_id),
        record,
        palette: demo.palette,
        tab: match demo.state.as_str() {
            "instructions" => GbotSettingsTab::Instructions,
            "memory" => GbotSettingsTab::Memory,
            _ => GbotSettingsTab::Profile,
        },
    };
    let _ = super::open_modal_window(
        GBOT_SETTINGS_MODAL_WIDTH,
        GBOT_SETTINGS_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiGbotSettingsModalWindow::new(config, host, window, cx)),
        cx,
    );
}
