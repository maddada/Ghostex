//! New GBot preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Start from offers Blank,
//! the built-in presets and one you kept), `none` (no presets read: the dialog as before).
use super::new_gbot_modal::*;
use gpui::{App, AppContext as _};
use serde_json::json;
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let preset = |id: &str, name: &str, title: &str, icon: &str, hex: &str, description: &str| {
        json!({ "presetId": id, "name": name, "title": title, "icon": icon, "colorHex": hex,
                "description": description, "instructions": format!("You are {name}. {description}") })
    };
    let presets = if demo.state == "none" {
        Vec::new()
    } else {
        vec![
            preset(
                "researcher",
                "Researcher",
                "Finds and compares options",
                "microscope",
                "#59d9ff",
                "Looks things up, compares the options and brings back a short list with sources.",
            ),
            preset(
                "writer",
                "Writer",
                "Drafts and edits text",
                "pencil",
                "#d2a7ff",
                "Drafts emails, posts and documents in the user's voice.",
            ),
            preset(
                "planner",
                "Planner",
                "Turns goals into next steps",
                "checklist",
                "#4ee6b8",
                "Breaks a goal into steps and says what to do next.",
            ),
            preset(
                "reviewer",
                "Code reviewer",
                "Reviews changes for bugs",
                "code",
                "#ff8b6b",
                "Reads a change and finds real bugs.",
            ),
            preset(
                "notes",
                "Note keeper",
                "Keeps notes and answers from them",
                "book",
                "#f3cc5f",
                "Keeps what you tell it in topic notes.",
            ),
            preset(
                "p_demo",
                "Fare scout",
                "Watches flight prices",
                "rocket",
                "#59d9ff",
                "Kept from a preset file.",
            ),
        ]
    };
    let host: NewGbotModalHost = Rc::new(|command, cx: &mut App| {
        if let NewGbotModalCommand::Create { name, preset, .. } = command {
            eprintln!("create {name} from {preset:?}");
        }
        cx.quit();
    });
    let config = NewGbotModalConfig {
        palette: demo.palette,
        presets,
    };
    let _ = super::open_modal_window(
        NEW_GBOT_MODAL_WIDTH,
        NEW_GBOT_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiNewGbotModalWindow::new(config, host, window, cx)),
        cx,
    );
}
