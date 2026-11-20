//! Meet the rail preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (on Rail, as it opens
//! for someone 11.12.0 just switched), `classic` (after Switch to Classic). Switch flips the pop-up;
//! Done quits.
use super::rail_intro_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let host: RailIntroHost = Rc::new(|command, cx: &mut App| match command {
        RailIntroCommand::Switch { rail } => {
            eprintln!("switch to {}", if rail { "rail" } else { "classic" })
        }
        RailIntroCommand::Done { rail } => {
            eprintln!("done on {}", if rail { "rail" } else { "classic" });
            cx.quit();
        }
    });
    let config = RailIntroModalConfig {
        palette: demo.palette,
        rail: demo.state != "classic",
    };
    let _ = super::open_modal_window(
        RAIL_INTRO_MODAL_WIDTH,
        RAIL_INTRO_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiRailIntroModalWindow::new(config, host, window, cx)),
        cx,
    );
}
