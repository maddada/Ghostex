//! Rename Session preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`):
//! default (a short name: plain rename), `empty` (no title: Generate from the
//! session), `long` (text past the 70-character rule: Generate from the text),
//! `nohistory` (an agent that cannot be named from its transcript).
//! Generate answers with a fixed name after a second.
use super::rename_session_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;
use std::time::Duration;

const LONG_TITLE: &str = "Fix the stale sign-in card that stays up after switching accounts, and make queued messages send again once the new account works";

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.as_str();
    let initial_title = match state {
        "empty" | "nohistory" => String::new(),
        "long" => LONG_TITLE.to_string(),
        _ => "Mr Big Chungus".to_string(),
    };
    let host: RenameSessionModalHost = Rc::new(|command, cx: &mut App| {
        match command {
            RenameSessionModalCommand::Rename { title } => eprintln!("rename: {title:?}"),
            RenameSessionModalCommand::OpenGeneratorSettings => eprintln!("open settings"),
            RenameSessionModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let generate: RenameSessionGenerator = Rc::new(|text: String, cx: &mut App| {
        eprintln!("generate name from {text:?}");
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            Ok("Fix stale sign-in card".to_string())
        })
    });
    let config = RenameSessionModalConfig {
        initial_title,
        can_generate_from_history: state != "nohistory",
        generator_name: "Claude".to_string(),
        generate,
        palette: demo.palette,
    };
    let _ = super::open_modal_window(
        RENAME_SESSION_MODAL_WIDTH,
        RENAME_SESSION_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiRenameSessionModalWindow::new(config, host, window, cx)),
        cx,
    );
}
