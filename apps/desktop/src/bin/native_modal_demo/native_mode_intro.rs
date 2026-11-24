//! Native mode announcement modal preview (Option F "Bold minimal").
//! Run with `GHOSTEX_NATIVE_MODAL_DEMO=native-mode-intro` and `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`.
use super::native_mode_intro_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let host: NativeModeIntroHost = Rc::new(|command, cx: &mut App| match command {
        NativeModeIntroCommand::TurnOn => {
            eprintln!("native-mode-intro: turn on");
            cx.quit();
        }
        NativeModeIntroCommand::Dismiss => {
            eprintln!("native-mode-intro: dismiss");
            cx.quit();
        }
    });
    let config = NativeModeIntroModalConfig {
        palette: demo.palette,
    };
    let _ = super::open_modal_window(
        NATIVE_MODE_INTRO_MODAL_WIDTH,
        NATIVE_MODE_INTRO_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiNativeModeIntroModalWindow::new(config, host, window, cx))
        },
        cx,
    );
}
