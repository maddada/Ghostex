//! New bot preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (a name and description
//! typed, starting from a copy of Harry), `refused` (Hermes refused a taken name), `blank` (no bot
//! to copy, so only Blank), `bad-name` (a name Hermes doesn't take).
use super::new_bot_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let host: NewBotModalHost = Rc::new(|command, cx: &mut App| {
        if let NewBotModalCommand::Create {
            profile,
            clone_from,
            description,
            reply,
        } = command
        {
            eprintln!("create {profile} from {clone_from:?} ({description:?})");
            // Answered on the next turn, as gxserver's answer arrives: the dialog is still being
            // updated while it sends.
            cx.defer(move |cx| {
                reply.finish(
                    Err(format!("A profile named '{profile}' already exists.")),
                    cx,
                );
            });
            return;
        }
        cx.quit();
    });
    let source = (demo.state != "blank").then(|| NewBotSource {
        profile: "harry".to_string(),
        title: "Harry".to_string(),
    });
    let config = NewBotModalConfig {
        palette: demo.palette,
        source,
        computer: None,
    };
    let state = demo.state.clone();
    let _ = super::open_modal_window(
        NEW_BOT_MODAL_WIDTH,
        NEW_BOT_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut dialog = GpuiNewBotModalWindow::new(config, host, window, cx);
                match state.as_str() {
                    "refused" => dialog.preview_fill(
                        "harry",
                        "",
                        Some("A profile named 'harry' already exists. Switch to it with `hermes profile use harry`, see all profiles with `hermes profile list`, or choose a different name."),
                        window,
                        cx,
                    ),
                    "bad-name" => dialog.preview_fill("Fare Scout", "", None, window, cx),
                    _ => dialog.preview_fill(
                        "researcher",
                        "Finds and compares flights",
                        None,
                        window,
                        cx,
                    ),
                }
                dialog
            })
        },
        cx,
    );
}
