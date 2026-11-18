//! Preview of the native Ghostex Update dialog. States: `ready`, `portable`,
//! `nonotes`, `notify` (a Linux tarball: the download page only), `notify-command` (a Linux
//! package: its update command with Copy command, and Release page); the default shows an available update with real release notes
//! read from the repository CHANGELOG.
use super::update_available_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

/// A slice of CHANGELOG.md under the H1 the release feed carries, plus the
/// bold, quote and link constructs real notes contain.
fn demo_release_notes() -> String {
    let changelog =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../CHANGELOG.md"))
            .unwrap_or_default();
    let body: Vec<&str> = changelog.lines().skip(2).take(40).collect();
    format!(
        "# Ghostex 9.6.0\n\n{}\n\n> Sessions started from a custom profile pick up confirmed renames instead of staying pending.\n\nRun `brew upgrade ghostex` or use **Settings > Updates**; the full list is on [the release page](https://github.com/maddada/ghostex/releases).\n\n## 9.5.2 - 2026-09-08\n\n- Stabilization\n  - Codex questions asked mid-turn stay answerable after the turn ends.\n  - One dead remote tunnel can no longer stall reconnects to every other computer.\n",
        body.join("\n")
    )
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = match demo.state.as_str() {
        "ready" | "portable" => UpdateAvailableState::Ready,
        "notify" | "notify-command" => UpdateAvailableState::Notify,
        _ => UpdateAvailableState::Available,
    };
    let host: UpdateAvailableModalHost = Rc::new(|command, cx: &mut App| {
        match command {
            UpdateAvailableModalCommand::Cancel => eprintln!("cancel"),
            UpdateAvailableModalCommand::Download => eprintln!("download update"),
            UpdateAvailableModalCommand::Restart => eprintln!("restart and update"),
            UpdateAvailableModalCommand::OpenDownloadPage => eprintln!("open download page"),
        }
        cx.quit();
    });
    let config = UpdateAvailableModalConfig {
        version: "9.6.0".to_string(),
        state,
        notes_markdown: if demo.state == "nonotes" {
            String::new()
        } else {
            demo_release_notes()
        },
        portable: demo.state == "portable",
        linux_command: (demo.state == "notify-command").then(|| "yay -S ghostex-bin".to_string()),
        palette: demo.palette,
    };
    super::open_modal_window(
        UPDATE_AVAILABLE_MODAL_WIDTH,
        UPDATE_AVAILABLE_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiUpdateAvailableModalWindow::new(config, host, window, cx))
        },
        cx,
    );
}
