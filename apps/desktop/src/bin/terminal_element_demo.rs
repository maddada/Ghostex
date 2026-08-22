/*
CDXC:GPUITerminalElement 2026-07-03:
TEMPORARY demo binary for the P1c/P1d GPUI-composited terminal element — the
deliverable proving PTY → vt snapshots → TerminalElement rendering plus the
P1d input path (keyboard through the vt key encoder, mouse
selection/reporting, clipboard, IME) end to end in a real gpui window,
without touching any existing pane/workspace code paths. Opens one window
running /bin/zsh through a style/color showcase and then a fully interactive
prompt. Delete once P1e integrates the element into real panes. Run with:

    cargo run --release --bin terminal-element-demo

(Release build required: debug gpui hits an unrelated pre-existing
"hover style already set" debug_assert on first render.)

Set GHOSTEX_GPUI_VT_DEMO_CMD to run a custom `zsh -c` command instead of the
built-in showcase.
*/

// This smoke/demo binary `#[path]`-includes shared modules (ghostty_vt, terminal_model,
// terminal_element, shared_settings, support_logs, ...) but only exercises a slice of
// them, so most of their items are legitimately unused *here*. The allow is scoped to
// this demo crate root so the real app binary keeps full dead-code coverage.
#![allow(dead_code)]

#[path = "../ghostty_vt.rs"]
mod ghostty_vt;
#[path = "../shared_settings.rs"]
mod shared_settings;
#[path = "../support_logs.rs"]
mod support_logs;
#[path = "../terminal_element.rs"]
mod terminal_element;
#[path = "../terminal_environment.rs"]
mod terminal_environment;
#[path = "../terminal_model.rs"]
mod terminal_model;

use gpui::{
    App, AppContext as _, Bounds, Focusable as _, TitlebarOptions, WindowBounds, WindowOptions, px,
    size,
};
use gpui_platform::application;

use terminal_element::{TerminalFontConfig, TerminalView};
use terminal_model::TerminalSpawnConfig;

/// Rendering showcase: SGR attributes, 16-color palette, truecolor ramp,
/// wide chars, and column-aligned output, then an interactive prompt so the
/// window keeps a live cursor.
const SHOWCASE: &str = r#"
printf '\e[1mbold\e[0m \e[3mitalic\e[0m \e[4munderline\e[0m \e[4:3mcurly\e[0m \e[9mstrike\e[0m \e[7minverse\e[0m \e[2mfaint\e[0m \e[1;3;4mall\e[0m\n'
printf 'palette:  '
for i in {0..7}; do printf '\e[4%dm  \e[0m' $i; done
printf '\n          '
for i in {0..7}; do printf '\e[10%dm  \e[0m' $i; done
printf '\ntruecolor '
for i in {0..31}; do printf '\e[48;2;%d;100;%dm \e[0m' $((i*8)) $((255-i*8)); done
printf '\nwide: \e[33m你好, 世界\e[0m 🚀 \e[36mターミナル\e[0m | tail stays aligned\n'
printf 'colored fg: \e[31mred\e[0m \e[32mgreen\e[0m \e[34mblue\e[0m \e[35;4mmagenta+ul\e[0m \e[58:2::255:80:80m\e[4mcolored-ul\e[0m\n'
ls -lh / | head -6
exec "$SHELL" -i
"#;

/// The user's shell, macOS/Linux default fallback included, so the demo runs
/// on hosts without zsh (Linux bring-up 2026-07-05).
fn demo_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

fn main() {
    application().run(|cx: &mut App| {
        // The app's terminal font is ghostty's embedded JetBrainsMono NF; it
        // is not a system-installed font, so register the vendored copies
        // (regular/bold/italic variants) with the gpui text system the same
        // way the app will in P1e.
        cx.text_system()
            .add_fonts(vec![
                include_bytes!("../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Regular.ttf")
                    .as_slice()
                    .into(),
                include_bytes!("../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Bold.ttf")
                    .as_slice()
                    .into(),
                include_bytes!("../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Italic.ttf")
                    .as_slice()
                    .into(),
                include_bytes!(
                    "../../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-BoldItalic.ttf"
                )
                .as_slice()
                .into(),
            ])
            .expect("register demo terminal fonts");
        let command = std::env::var("GHOSTEX_GPUI_VT_DEMO_CMD")
            .ok()
            .filter(|command| !command.trim().is_empty())
            .unwrap_or_else(|| SHOWCASE.to_string());
        let bounds = Bounds::centered(None, size(px(960.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Ghostex VT Demo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let terminal = cx.new(|cx| {
                    TerminalView::spawn(
                        TerminalSpawnConfig {
                            program: demo_shell(),
                            args: vec!["-c".into(), command],
                            env: vec![("TERM".into(), "xterm-256color".into())],
                            cwd: None,
                            // Initial grid is provisional; the element's
                            // first prepaint resizes to the real bounds.
                            cols: 80,
                            rows: 24,
                            cell_width_px: 8,
                            cell_height_px: 17,
                            max_scrollback: 10_000,
                        },
                        TerminalFontConfig {
                            // CoreText resolves this family by its
                            // typographic name (name table ID 16).
                            family: "JetBrainsMono Nerd Font".into(),
                            ..TerminalFontConfig::default()
                        },
                        cx,
                    )
                    .expect("spawn demo terminal")
                });
                // Focus at open so typing works without a first click.
                window.focus(&terminal.focus_handle(cx), cx);
                terminal
            },
        )
        .expect("open demo window");
        cx.activate(true);
    });
}
