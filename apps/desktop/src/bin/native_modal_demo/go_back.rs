//! Preview of the "Ghostex <new> isn't working on this computer" message (go_back_modal.rs).
//! States: the default (Go back swaps the kept version in), `command` (a deb install's command),
//! `download` (no kept copy), `going` (after Go back), `failed` (going back could not start).
use super::go_back_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let way = match demo.state.as_str() {
        "command" => GoBackModalWay::Command(
            "curl -fLo /tmp/ghostex_11.7.1_amd64.deb https://github.com/maddada/Ghostex/releases/download/v11.7.1/ghostex_11.7.1_amd64.deb && sudo apt install --allow-downgrades -y /tmp/ghostex_11.7.1_amd64.deb".to_string(),
        ),
        "download" => GoBackModalWay::Download,
        _ => GoBackModalWay::Swap,
    };
    let config = GoBackModalConfig {
        current: "11.8.2".to_string(),
        previous: "11.7.1".to_string(),
        way,
        can_send_report: true,
    };
    let host: GoBackModalHost = Rc::new(|command, _cx: &mut App| eprintln!("{command:?}"));
    let state = demo.state.clone();
    let palette = demo.palette;
    super::open_modal_window(
        GO_BACK_MODAL_WIDTH,
        GO_BACK_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut view = GoBackModalWindow::new(config, palette, host, window, cx);
                match state.as_str() {
                    "going" => view.set_going(cx),
                    "failed" => view.set_failed(
                        "The previous version is no longer on this computer.".to_string(),
                        cx,
                    ),
                    _ => {}
                }
                view
            })
        },
        cx,
    );
}
