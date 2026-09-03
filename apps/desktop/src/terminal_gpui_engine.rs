/*
CDXC:Terminal 2026-07-04:
P1e integration glue for the GPUI-composited terminal engine (libghostty-vt +
TerminalElement). This is the single terminal pipeline on every OS; the macOS
GhosttyKit implementation remains compiled but is not selected at runtime.
This module owns
only the engine-side helpers (font registration/mapping, spawn-config
construction mirroring ghostty's macOS login-shell exec semantics, and the
per-session runtime record). Pane wiring lives in main.rs next to the native
paths it mirrors.
*/

use std::path::{Path, PathBuf};

use gpui::{App, Entity, FontWeight, SharedString, px};

use crate::AgentsTerminalRuntimeSessionId;
use crate::ghostty_vt::VtOptionAsAlt;
use crate::shared_settings::{SharedGpuiTerminalEngineSettings, SharedTerminalConfirmCloseSurface};
use crate::terminal_element::{
    TerminalBackgroundImage, TerminalBackgroundImageFit, TerminalConfiguredColor,
    TerminalCursorShape, TerminalFontConfig, TerminalMetricAdjustment, TerminalView,
    TerminalViewSettings,
};
use crate::terminal_model::{Rgb, TerminalConfirmCloseBehavior, TerminalSpawnConfig};

include!(concat!(env!("OUT_DIR"), "/embedded_ghostty_themes.rs"));

/// Initial grid guess before the first prepaint measures the real body; the
/// element resizes the terminal synchronously on first layout.
const INITIAL_GRID_COLS: u16 = 80;
const INITIAL_GRID_ROWS: u16 = 24;
const INITIAL_CELL_WIDTH_PX: u32 = 8;
const INITIAL_CELL_HEIGHT_PX: u32 = 17;

#[derive(Clone, Debug)]
pub(crate) struct GpuiTerminalColorDefaults {
    pub(crate) foreground: Rgb,
    pub(crate) background: Rgb,
    pub(crate) cursor: Option<Rgb>,
    pub(crate) palette: [Rgb; 256],
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiTerminalEngineConfig {
    pub(crate) font: TerminalFontConfig,
    pub(crate) view: TerminalViewSettings,
    pub(crate) colors: Option<GpuiTerminalColorDefaults>,
    pub(crate) scrollback_limit_bytes: u64,
    pub(crate) option_as_alt: VtOptionAsAlt,
    pub(crate) confirm_close_surface: SharedTerminalConfirmCloseSurface,
}

impl GpuiTerminalEngineConfig {
    #[allow(dead_code)] // no caller: superseded by the shared-settings theme path used by the engine today
    pub(crate) fn from_shared(settings: &SharedGpuiTerminalEngineSettings) -> Self {
        let cursor_shape = match settings.cursor_style.as_str() {
            "block" => TerminalCursorShape::Block,
            "underline" => TerminalCursorShape::Underline,
            _ => TerminalCursorShape::Bar,
        };
        let mut font = gpui_engine_terminal_font_config_from_parts(
            settings.font_family.as_str(),
            settings.font_size,
            settings.font_weight,
        );
        font.cell_width_adjustment = TerminalMetricAdjustment::Absolute(settings.letter_spacing);
        font.cell_height_adjustment = TerminalMetricAdjustment::Percent(settings.line_height - 1.0);
        let mut config = Self {
            font,
            view: TerminalViewSettings {
                cursor_shape,
                background_image: terminal_background_image_from_settings(settings),
                cursor_blink: settings.cursor_style_blink,
                copy_on_select: settings.copy_on_select,
                selection_clipboard_enabled: settings.selection_clipboard_enabled,
                clipboard_trim_trailing_spaces: settings.clipboard_trim_trailing_spaces,
                mouse_hide_while_typing: settings.mouse_hide_while_typing,
                mouse_scroll_precision: settings.mouse_scroll_multiplier_precision,
                mouse_scroll_discrete: settings.mouse_scroll_multiplier_discrete,
                scrollbar_visible: settings.scrollbar_visible,
                scroll_to_bottom_when_typing: settings.scroll_to_bottom_when_typing,
                ..TerminalViewSettings::default()
            },
            colors: None,
            scrollback_limit_bytes: settings.scrollback_limit_bytes,
            option_as_alt: VtOptionAsAlt::default(),
            confirm_close_surface: settings.confirm_close_surface,
        };
        config.apply_ghostty_theme(&settings.ghostty_theme);
        if let Some(background) = settings.terminal_background_rgb {
            config.apply_terminal_background(background);
        }
        config
    }

    #[allow(dead_code)] // no caller: superseded by the shared-settings theme path used by the engine today
    pub(crate) fn apply_ghostty_theme(&mut self, name: &str) {
        let Some(theme) = gpui_terminal_theme(name) else {
            return;
        };
        self.colors = Some(GpuiTerminalColorDefaults {
            foreground: theme.foreground,
            background: theme.background,
            cursor: theme.cursor,
            palette: theme.palette,
        });
        self.view.cursor_text = theme.cursor_text.map(TerminalConfiguredColor::Rgb);
        self.view.selection_background =
            theme.selection_background.map(TerminalConfiguredColor::Rgb);
    }

    pub(crate) fn apply_terminal_background(&mut self, [r, g, b]: [u8; 3]) {
        let background = Rgb { r, g, b };
        if let Some(colors) = &mut self.colors {
            colors.background = background;
            return;
        }
        self.colors = Some(GpuiTerminalColorDefaults {
            foreground: Rgb {
                r: 0xff,
                g: 0xff,
                b: 0xff,
            },
            background,
            cursor: None,
            palette: crate::ghostty_vt::default_color_palette(),
        });
    }
}

/// An empty path leaves the pane on its solid background, which is the default.
pub(crate) fn terminal_background_image_from_settings(
    settings: &SharedGpuiTerminalEngineSettings,
) -> Option<TerminalBackgroundImage> {
    let path = settings.background_image_path.trim();
    if path.is_empty() {
        return None;
    }
    Some(TerminalBackgroundImage {
        path: PathBuf::from(path),
        opacity: settings.background_image_opacity.clamp(0.0, 1.0),
        fit: match settings.background_image_fit.as_str() {
            "contain" => TerminalBackgroundImageFit::Contain,
            "stretch" => TerminalBackgroundImageFit::Stretch,
            "natural" => TerminalBackgroundImageFit::Natural,
            _ => TerminalBackgroundImageFit::Cover,
        },
    })
}

#[allow(dead_code)] // no constructor: superseded by the shared-settings theme path used by the engine today
struct GpuiTerminalTheme {
    foreground: Rgb,
    background: Rgb,
    cursor: Option<Rgb>,
    cursor_text: Option<Rgb>,
    selection_background: Option<Rgb>,
    palette: [Rgb; 256],
}

pub(crate) fn ghostty_theme_source(name: &str) -> Option<&'static str> {
    embedded_ghostty_theme_source(name)
}

#[allow(dead_code)] // no caller: superseded by the shared-settings theme path used by the engine today
fn gpui_terminal_theme(name: &str) -> Option<GpuiTerminalTheme> {
    let source = ghostty_theme_source(name)?;
    let mut foreground = None;
    let mut background = None;
    let mut cursor = None;
    let mut cursor_text = None;
    let mut selection_background = None;
    let mut palette = crate::ghostty_vt::default_color_palette();

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "foreground" => foreground = parse_theme_rgb(value),
            "background" => background = parse_theme_rgb(value),
            "cursor-color" => cursor = parse_theme_rgb(value),
            "cursor-text" => cursor_text = parse_theme_rgb(value),
            "selection-background" => selection_background = parse_theme_rgb(value),
            "palette" => {
                let (index, color) = value.split_once('=')?;
                let index = index.trim().parse::<usize>().ok()?;
                *palette.get_mut(index)? = parse_theme_rgb(color.trim())?;
            }
            _ => {}
        }
    }

    Some(GpuiTerminalTheme {
        foreground: foreground?,
        background: background?,
        cursor,
        cursor_text,
        selection_background,
        palette,
    })
}

fn parse_theme_rgb(value: &str) -> Option<Rgb> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    Some(Rgb {
        r: u8::from_str_radix(&hex[0..2], 16).ok()?,
        g: u8::from_str_radix(&hex[2..4], 16).ok()?,
        b: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}

/// One live GPUI-engine terminal owned by the app shell. Dropping the record
/// drops the view entity (killing the child via the model) and the event
/// subscription together.
pub(crate) struct GpuiEngineTerminalRecord {
    pub(crate) view: Entity<TerminalView>,
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    /// Ghostty `wait-after-command` semantics: exited contents stay on
    /// screen instead of auto-closing the tab.
    pub(crate) wait_after_command: bool,
    pub(crate) confirm_close_behavior: TerminalConfirmCloseBehavior,
    pub(crate) _subscription: gpui::Subscription,
}

/// Register the vendored JetBrains Mono Nerd Font faces the ghostty tree
/// already carries so the engine's default terminal font resolves without a
/// system install. Idempotent per process; call once at startup.
pub(crate) fn register_gpui_terminal_engine_fonts(cx: &App) {
    let fonts: Vec<std::borrow::Cow<'static, [u8]>> = vec![
        include_bytes!(
            "../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Regular.ttf"
        )
        .as_slice()
        .into(),
        include_bytes!(
            "../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Bold.ttf"
        )
        .as_slice()
        .into(),
        include_bytes!(
            "../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-Italic.ttf"
        )
        .as_slice()
        .into(),
        include_bytes!(
            "../../../.dependencies/ghostty/src/font/res/JetBrainsMonoNerdFont-BoldItalic.ttf"
        )
        .as_slice()
        .into(),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("gpui terminal engine font registration failed: {error}");
    }
}

/// Element font config from the shared terminal settings. The app default
/// family "JetBrains Mono" is not system-installed; the registered vendored
/// faces resolve as "JetBrainsMono Nerd Font" (the same embedded face the
/// native ghostty renderer uses), so the default maps to that family.
pub(crate) fn gpui_engine_terminal_font_config(
    settings: &GpuiTerminalEngineConfig,
) -> TerminalFontConfig {
    settings.font.clone()
}

pub(crate) fn gpui_engine_terminal_font_config_from_parts(
    font_family: &str,
    font_size: f32,
    font_weight: f32,
) -> TerminalFontConfig {
    let family: SharedString = if font_family == "JetBrains Mono" {
        "JetBrainsMono Nerd Font".into()
    } else {
        font_family.to_string().into()
    };
    TerminalFontConfig {
        family,
        size: px(font_size),
        weight: FontWeight(font_weight),
        ..TerminalFontConfig::default()
    }
}

/// Close-confirm policy for the engine from the shared Ghostty-backed
/// `terminalConfirmCloseSurface` setting.
pub(crate) fn gpui_engine_confirm_close_behavior(
    settings: &GpuiTerminalEngineConfig,
) -> TerminalConfirmCloseBehavior {
    match settings.confirm_close_surface {
        SharedTerminalConfirmCloseSurface::True => TerminalConfirmCloseBehavior::UnlessPrompt,
        SharedTerminalConfirmCloseSurface::False => TerminalConfirmCloseBehavior::Never,
        SharedTerminalConfirmCloseSurface::Always => TerminalConfirmCloseBehavior::Always,
    }
}

/// Build the spawn config for a GPUI-engine terminal from the same launch
/// payload fields the native Ghostty path consumes.
///
/// Process shape mirrors ghostty's macOS exec semantics (termio/Exec.zig):
/// `/usr/bin/login -flp <user>` for real login-shell behavior, then bash
/// `exec -l` into the payload command (or the user's shell when the payload
/// has none). An inaccessible cwd is ignored exactly like ghostty ignores
/// it. Env mirrors the native surfaces in this app: no ghostty terminfo is
/// bundled, so TERM stays xterm-256color with truecolor + TERM_PROGRAM set.
pub(crate) fn gpui_engine_terminal_spawn_config(
    working_directory: Option<String>,
    command: Option<String>,
    env_vars: Vec<(String, String)>,
    scrollback_limit_bytes: u64,
) -> TerminalSpawnConfig {
    let cwd = working_directory
        .map(PathBuf::from)
        .filter(|path| path.is_dir());

    let mut env = crate::terminal_environment::color_capable_terminal_env_vars(env_vars);
    env.retain(|(key, _)| !matches!(key.as_str(), "TERM" | "COLORTERM" | "TERM_PROGRAM"));
    env.extend([
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        ("TERM_PROGRAM".into(), "ghostty".into()),
    ]);
    #[cfg(target_os = "macos")]
    if command.is_none() {
        configure_default_zsh_shell_integration(&mut env);
    }
    // The Linux app removed WAYLAND_DISPLAY from its own environment to run
    // as an X11 client (see CDXC:PlatformSupport in main.rs); terminal
    // children are not part of that constraint, so they get the inherited
    // value back and user-launched GUI apps keep running native Wayland.
    #[cfg(target_os = "linux")]
    if let Some(wayland_display) = crate::linux_inherited_wayland_display() {
        env.push(("WAYLAND_DISPLAY".into(), wayland_display.to_string()));
    }

    let (program, args) = spawn_invocation(command, cwd.as_deref());

    TerminalSpawnConfig {
        program,
        args,
        env,
        cwd,
        cols: INITIAL_GRID_COLS,
        rows: INITIAL_GRID_ROWS,
        cell_width_px: INITIAL_CELL_WIDTH_PX,
        cell_height_px: INITIAL_CELL_HEIGHT_PX,
        max_scrollback: scrollback_limit_bytes as usize,
    }
}

#[cfg(target_os = "macos")]
fn configure_default_zsh_shell_integration(env: &mut Vec<(String, String)>) {
    /*
    CDXC:Terminal 2026-07-10:
    Plain GPUI-engine zsh terminals must enter the same real Ghostty shell
    integration mode as native Ghostty surfaces. The engine's close policy
    intentionally uses OSC 133 prompt semantics; without the integration,
    every idle shell looks like a running process and tab X/middle-click/
    Cmd-W gets trapped behind close confirmation. Point zsh at the packaged
    upstream integration directory (or the source resource directory for an
    explicit development binary), preserving the user's original ZDOTDIR for
    Ghostty's .zshenv loader to restore before it sources user configuration.
    */
    let shell = default_shell();
    if Path::new(&shell).file_name().and_then(|name| name.to_str()) != Some("zsh") {
        return;
    }
    let Some(resources_dir) = gpui_engine_shell_integration_resources_dir() else {
        return;
    };
    let integration_dir = resources_dir.join("shell-integration/zsh");
    if !integration_dir.join(".zshenv").is_file()
        || !integration_dir.join("ghostty-integration").is_file()
    {
        return;
    }

    let original_zdotdir = env
        .iter()
        .rev()
        .find_map(|(key, value)| (key == "ZDOTDIR").then(|| value.clone()))
        .or_else(|| std::env::var("ZDOTDIR").ok())
        .filter(|value| !value.is_empty());
    env.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "ZDOTDIR" | "GHOSTTY_ZSH_ZDOTDIR" | "GHOSTTY_RESOURCES_DIR"
        )
    });
    if let Some(original_zdotdir) = original_zdotdir {
        env.push(("GHOSTTY_ZSH_ZDOTDIR".into(), original_zdotdir));
    }
    env.push((
        "ZDOTDIR".into(),
        integration_dir.to_string_lossy().into_owned(),
    ));
    env.push((
        "GHOSTTY_RESOURCES_DIR".into(),
        resources_dir.to_string_lossy().into_owned(),
    ));
}

#[cfg(target_os = "macos")]
fn gpui_engine_shell_integration_resources_dir() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    let contents_dir = executable_dir.parent()?;
    if executable_dir.file_name().and_then(|name| name.to_str()) == Some("MacOS")
        && contents_dir.file_name().and_then(|name| name.to_str()) == Some("Contents")
    {
        return Some(contents_dir.join("Resources"));
    }

    Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.dependencies/ghostty/src"))
}

#[cfg(not(target_os = "windows"))]
fn spawn_invocation(
    command: Option<String>,
    _working_directory: Option<&Path>,
) -> (String, Vec<String>) {
    let shell_command = command.unwrap_or_else(default_shell);
    login_shell_invocation(&shell_command)
}

/// Windows ConPTY sessions route through the selected backend. Native
/// PowerShell remains available without persistence; WSL2 runs commands in a
/// Linux login shell so gxserver-provided zmx attach payloads retain their
/// Unix semantics.
#[cfg(target_os = "windows")]
fn spawn_invocation(
    command: Option<String>,
    working_directory: Option<&Path>,
) -> (String, Vec<String>) {
    crate::windows_terminal_backend::terminal_invocation(command, working_directory)
}

#[cfg(not(target_os = "windows"))]
fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

#[cfg(target_os = "windows")]
fn default_shell() -> String {
    "powershell.exe".to_string()
}

#[cfg(target_os = "macos")]
fn login_shell_invocation(shell_command: &str) -> (String, Vec<String>) {
    // Mirrors ghostty's darwin path: login(1) provides getlogin()/SHELL and
    // hushlogin behavior, bash `exec -l` replaces itself with the real
    // command as a login shell. If USER is somehow absent, run the command
    // through the shell directly — the same degradation ghostty applies
    // when its passwd lookup fails.
    let Ok(user) = std::env::var("USER") else {
        return (
            default_shell(),
            vec!["-c".to_string(), shell_command.to_string()],
        );
    };
    let hushlogin = std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".hushlogin").exists())
        .unwrap_or(false);
    let mut args: Vec<String> = Vec::new();
    if hushlogin {
        args.push("-q".to_string());
    }
    args.extend([
        "-flp".to_string(),
        user,
        "/bin/bash".to_string(),
        "--noprofile".to_string(),
        "--norc".to_string(),
        "-c".to_string(),
        format!("exec -l {shell_command}"),
    ]);
    ("/usr/bin/login".to_string(), args)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn login_shell_invocation(shell_command: &str) -> (String, Vec<String>) {
    // Mirrors ghostty's non-darwin exec path (termio/Exec.zig): wrap the
    // shell command in `/bin/sh -c` so argument parsing stays with the shell
    // and NixOS-style /bin/sh environment setup applies. The command itself
    // defaults to $SHELL (default_shell), so a plain launch lands in the
    // user's own shell; login(1) semantics are a macOS-only expectation.
    (
        "/bin/sh".to_string(),
        vec!["-c".to_string(), shell_command.to_string()],
    )
}
