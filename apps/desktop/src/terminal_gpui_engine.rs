/*
CDXC:Terminal 2026-07-04:
P1e integration glue for the GPUI-composited terminal engine (libghostty-vt +
TerminalElement), the single terminal pipeline on every OS. This module owns
only the engine-side helpers (font registration/mapping, spawn-config
construction mirroring ghostty's macOS login-shell exec semantics, and the
per-session runtime record).
*/

use std::path::{Path, PathBuf};

use gpui::{App, Entity, FontWeight, SharedString, px};

use crate::AgentsTerminalRuntimeSessionId;
use crate::ghostty_vt::VtOptionAsAlt;
use crate::shared_settings::{SharedGpuiTerminalEngineSettings, SharedTerminalConfirmCloseSurface};
use crate::terminal_element::{
    TerminalBackgroundImage, TerminalBackgroundImageFit, TerminalConfiguredColor,
    TerminalFontConfig, TerminalView, TerminalViewSettings,
};
#[cfg(not(target_os = "macos"))]
use crate::terminal_element::{TerminalCursorShape, TerminalMetricAdjustment};
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
    /// Bold weight an explicit `font-style-bold` / `font-variation-bold` in
    /// the Ghostty config asks for; `None` leaves it to the Settings switch.
    pub(crate) ghostty_bold_weight: Option<FontWeight>,
}

impl GpuiTerminalEngineConfig {
    /// Linux and Windows build the engine config from Settings; macOS reads it through GhosttyKit instead.
    #[cfg(not(target_os = "macos"))]
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
            ghostty_bold_weight: None,
        };
        config.apply_ghostty_theme(&settings.ghostty_theme);
        if let Some(background) = settings.terminal_background.override_rgb(false) {
            config.apply_terminal_background(background);
        }
        config
    }

    /// The app-owned Settings switch, applied after every config load.
    pub(crate) fn apply_bold_text_setting(&mut self, settings: &SharedGpuiTerminalEngineSettings) {
        self.font
            .resolve_bold_weight(self.ghostty_bold_weight, settings.bold_uses_heavier_font);
    }

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

    /// CDXC:Theming 2026-09-13 DECISION:
    /// User: terminals can follow the system with a separate light theme. Only light mode replaces the finalized color palette; returning to dark restores the existing Ghostty config palette.
    ///
    /// CDXC:Theming 2026-09-28 DECISION:
    /// User: "allow us to set terminal background color to full black/white and make it black by default not the background color from the theme". The palette still comes from the Ghostty config or the light theme; the background is the Terminal background choice: Black / white (the default) paints pure black behind dark terminals and pure white behind light ones, Follow theme paints the theme's content colour (`theme_background`), and Custom replaces it (in dark mode only until 2026-11-01, in both appearances since: CDXC:Theming 2026-11-01 on `SharedTerminalBackground::Custom` in shared_settings/types.rs). It never colours the work area (SEE-ALSO `refresh_gpui_visual_settings` in app/helpers/project/colors.rs, `normalizeTerminalBackgroundMode` in packages/shared/ghostex-settings/normalize-fields.ts (deleted 2026-10-01)). Supersedes the 2026-09-23 decision that the background follows the theme by default.
    ///
    /// CDXC:Theming 2026-09-30 WHY:
    /// Shader input pixels and iBackgroundColor must agree, or background-sensitive effects change brightness. The opt-in chain therefore retains the finalized Ghostty background (or selected light palette); disabling it restores the ordinary Terminal background choice. This proposed exception leaves the work area theme unchanged.
    pub(crate) fn apply_color_scheme(
        &mut self,
        settings: &SharedGpuiTerminalEngineSettings,
        system_is_light: bool,
        theme_background: [u8; 3],
    ) {
        let light = settings.uses_light_theme(system_is_light);
        self.view.light_theme = light;
        if light {
            self.apply_ghostty_theme(&settings.light_theme);
            // CDXC:Theming 2026-09-13 WHY:
            // GPUI retains ANSI foreground colors in selected text; Ghostty's opaque selection background assumes a separate selection foreground.
            // Use the renderer's adaptive translucent selection tint so light palettes remain readable.
            self.view.selection_background = None;
        }
        #[cfg(target_os = "macos")]
        let shader_background = self
            .view
            .shaders
            .as_ref()
            .is_some_and(|shaders| shaders.enabled);
        #[cfg(not(target_os = "macos"))]
        let shader_background = false;
        if !shader_background {
            self.apply_terminal_background(settings.grid_background_rgb(light, theme_background));
        }
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

fn gpui_terminal_theme(name: &str) -> Option<GpuiTerminalTheme> {
    let custom_source = custom_ghostty_theme_source(name);
    let source = custom_source
        .as_deref()
        .or_else(|| ghostty_theme_source(name))?;
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

/// CDXC:Theming 2026-09-14 SEE-ALSO:
/// shared_settings/ghostty_themes.rs imports configured theme names, including user-defined files that are absent from the embedded catalog.
/// CDXC:Theming 2026-10-14 WHY:
/// The `themes/` folder beside Ghostex's own terminal config comes first. Ghostty's themes folders stay as read-only lookups because libghostty resolves `theme = <name>` there too, and a custom theme name carried over from the Ghostty app's config must keep finding its file.
fn custom_ghostty_theme_source(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return std::fs::read_to_string(path).ok();
    }
    let home = PathBuf::from(crate::shared_settings::user_home_dir()?);
    if let Some(relative) = name.strip_prefix("~/") {
        return std::fs::read_to_string(home.join(relative)).ok();
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let mut paths = Vec::new();
    if let Some(parent) = crate::shared_settings::ghostex_terminal_config_path().parent() {
        paths.push(parent.join("themes").join(name));
    }
    paths.push(xdg.join("ghostty/themes").join(name));
    if let Ok(config) = crate::shared_settings::selected_ghostty_config_path()
        && let Some(parent) = config.parent()
    {
        paths.push(parent.join("themes").join(name));
    }
    paths
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
}

/// A theme colour: six hex digits, with or without a leading `#`, as Ghostty itself reads them.
///
/// CDXC:Theming 2026-11-01 WHY:
/// Theme projects' own Ghostty files (the Catppuccin and Kanagawa palettes the theme presets
/// bundle) write `background = 1e1e2e` without the `#`, which Ghostty accepts; requiring it would
/// drop those palettes wherever this parser reads them (light mode, Windows and Linux). Anything
/// but exactly six ASCII hex digits is still refused, so a stray sign or a multi-byte character
/// never reaches the byte slicing below.
fn parse_theme_rgb(value: &str) -> Option<Rgb> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
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
    pub(crate) viewer_leases: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

pub(crate) struct GpuiTerminalViewerLease(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for GpuiTerminalViewerLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

impl GpuiEngineTerminalRecord {
    pub(crate) fn pin_viewer(&self) -> GpuiTerminalViewerLease {
        self.viewer_leases
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        GpuiTerminalViewerLease(self.viewer_leases.clone())
    }

    pub(crate) fn viewer_is_pinned(&self) -> bool {
        self.viewer_leases
            .load(std::sync::atomic::Ordering::Acquire)
            > 0
    }
}

/// The JetBrains Mono Nerd Font faces the ghostty tree carries (Regular, Bold,
/// Italic, BoldItalic) packed into one zstd frame by build.rs.
const TERMINAL_FONT_FACES_FRAME: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/terminal_font_faces.zst"));
/// Byte length of each face in the unpacked frame, in registration order.
const TERMINAL_FONT_FACE_LENGTHS: [usize; 4] =
    include!(concat!(env!("OUT_DIR"), "/terminal_font_face_lengths.rs"));

/// Register the vendored JetBrains Mono Nerd Font faces the ghostty tree
/// already carries so the engine's default terminal font resolves without a
/// system install. Idempotent per process: the GPUI chat view registers the same
/// faces for its monospace text, and a host that opens both must not add them twice.
///
/// CDXC:Terminal 2026-10-22 DECISION:
/// User chose "Ship the four faces as one compressed file, unpack at startup" over rebuilding the faces from a Nerd Font symbols font, because a fallback font would change how icons look on Windows and in powerline prompts.
///
/// CDXC:Terminal 2026-10-22 WHY:
/// The four faces (9.15 MB) share about 10,000 byte-identical icon glyphs, but each face's copy sits 2.3 MB after the previous one: the DMG compresses 4 MiB chunks on their own and the Windows zip's deflate looks back only 32 KB, so neither could share them. One zstd frame of all four is 1.31 MB. Measured on a 10.30.0 release build (macOS arm64): the app binary shrank by 7.6 MB, a 4 MiB-chunk xz model of the DMG by 2.0 MB and gzip -9 (the zip) by 3.7 MB; the Linux tar.zst and xz packages already saw across the faces and stay about the same size. ruzstd unpacks the frame in about 9 ms (M5 Pro), once per process.
/// On macOS and Linux the unpacked faces are registered as borrowed `'static` slices, exactly as the embedded files were: on macOS an owned buffer takes another CoreText path (a font-kit memory handle instead of a CGFont over the bytes), so borrowing keeps the rendering identical. On Windows both paths copy the data into DirectWrite, so the faces go in owned and the unpacked buffer is freed, which avoids keeping 9 MB twice.
pub(crate) fn register_gpui_terminal_engine_fonts(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let faces = match unpack_terminal_font_faces() {
            Ok(faces) => faces,
            Err(error) => {
                eprintln!("gpui terminal engine font unpacking failed: {error}");
                return;
            }
        };
        // DirectWrite copies in-memory font data it is given without an owner, so on Windows the
        // unpacked buffer is handed over owned and freed after registration instead of kept twice.
        #[cfg(target_os = "windows")]
        let fonts: Vec<std::borrow::Cow<'static, [u8]>> = split_terminal_font_faces(&faces)
            .into_iter()
            .map(|face| std::borrow::Cow::Owned(face.to_vec()))
            .collect();
        #[cfg(not(target_os = "windows"))]
        let fonts: Vec<std::borrow::Cow<'static, [u8]>> = split_terminal_font_faces(faces.leak())
            .into_iter()
            .map(std::borrow::Cow::Borrowed)
            .collect();
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("gpui terminal engine font registration failed: {error}");
        }
    });
}

/// Unpacks the frame into the four original files, back to back. Outside Windows
/// the caller keeps this buffer for the rest of the process, because the text
/// system borrows the faces for as long as the app runs, as it borrowed the
/// embedded files. Runs once, inside `register_gpui_terminal_engine_fonts`.
fn unpack_terminal_font_faces() -> Result<Vec<u8>, String> {
    let total = TERMINAL_FONT_FACE_LENGTHS.iter().sum::<usize>();
    let mut bytes = vec![0u8; total];
    let written = ruzstd::decoding::FrameDecoder::new()
        .decode_all(TERMINAL_FONT_FACES_FRAME, &mut bytes)
        .map_err(|error| error.to_string())?;
    if written != total {
        return Err(format!("unpacked {written} of {total} bytes"));
    }
    Ok(bytes)
}

/// Splits the unpacked buffer back into the four faces, in registration order.
fn split_terminal_font_faces(mut rest: &[u8]) -> [&[u8]; 4] {
    TERMINAL_FONT_FACE_LENGTHS.map(|length| {
        let (face, tail) = rest.split_at(length);
        rest = tail;
        face
    })
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
    let installed = crate::ui_fonts::installed_families();
    let font_family = crate::ui_fonts::terminal_family(
        font_family,
        installed.as_ref().map(|families| families.as_slice()),
    );
    let family: SharedString = if font_family == "JetBrains Mono" {
        "JetBrainsMono Nerd Font".into()
    } else {
        font_family.into()
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

    #[cfg(windows)]
    let (program, args) = {
        // CDXC:RemoteMachines 2026-09-23 WHY:
        // Remote SSH uses the host's OpenSSH and credential helper even when local projects use WSL; routing this payload through WSL would lose its Windows executable paths and askpass environment.
        let native_remote = env
            .iter()
            .any(|(key, value)| key == "GHOSTEX_REMOTE_SSH_CLIENT" && value == "windows");
        env.retain(|(key, _)| key != "GHOSTEX_REMOTE_SSH_CLIENT");
        if native_remote {
            crate::windows_terminal_backend::native_terminal_invocation(command, cwd.as_deref())
        } else {
            spawn_invocation(command, cwd.as_deref())
        }
    };
    #[cfg(not(windows))]
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

#[cfg(test)]
mod theme_preset_palette_tests {
    use super::*;

    /// Background, foreground, cursor, selection background and ANSI 0 to 15 of every theme
    /// preset palette, as the theme projects define them.
    const CANONICAL: [(&str, &str); 20] = [
        (
            "Ghostex Catppuccin Mocha",
            "1e1e2e cdd6f4 f5e0dc 353749 45475a f38ba8 a6e3a1 f9e2af 89b4fa f5c2e7 94e2d5 a6adc8 585b70 f38ba8 a6e3a1 f9e2af 89b4fa f5c2e7 94e2d5 bac2de",
        ),
        (
            "Ghostex Catppuccin Macchiato",
            "24273a cad3f5 f4dbd6 3a3e53 494d64 ed8796 a6da95 eed49f 8aadf4 f5bde6 8bd5ca a5adcb 5b6078 ed8796 a6da95 eed49f 8aadf4 f5bde6 8bd5ca b8c0e0",
        ),
        (
            "Ghostex Catppuccin Frappe",
            "303446 c6d0f5 f2d5cf 44495d 51576d e78284 a6d189 e5c890 8caaee f4b8e4 81c8be a5adce 626880 e78284 a6d189 e5c890 8caaee f4b8e4 81c8be b5bfe2",
        ),
        (
            "Ghostex Catppuccin Latte",
            "eff1f5 4c4f69 dc8a78 d8dae1 5c5f77 d20f39 40a02b df8e1d 1e66f5 ea76cb 179299 acb0be 6c6f85 d20f39 40a02b df8e1d 1e66f5 ea76cb 179299 bcc0cc",
        ),
        (
            "Ghostex Dracula",
            "282a36 f8f8f2 f8f8f2 44475a 21222c ff5555 50fa7b f1fa8c bd93f9 ff79c6 8be9fd f8f8f2 6272a4 ff6e6e 69ff94 ffffa5 d6acff ff92df a4ffff ffffff",
        ),
        (
            "Ghostex Alucard",
            "fffbeb 1f1f1f 1f1f1f cfcfde fffbeb cb3a2a 14710a 846e15 644ac9 a3144d 036a96 1f1f1f 6c664b d74c3d 198d0c 9e841a 7862d0 bf185a 047fb4 2c2b31",
        ),
        (
            "Ghostex Solarized Dark",
            "002b36 839496 839496 073642 073642 dc322f 859900 b58900 268bd2 d33682 2aa198 eee8d5 002b36 cb4b16 586e75 657b83 839496 6c71c4 93a1a1 fdf6e3",
        ),
        (
            "Ghostex Solarized Light",
            "fdf6e3 657b83 657b83 eee8d5 073642 dc322f 859900 b58900 268bd2 d33682 2aa198 eee8d5 002b36 cb4b16 586e75 657b83 839496 6c71c4 93a1a1 fdf6e3",
        ),
        (
            "Ghostex Tokyo Night Night",
            "1a1b26 c0caf5 c0caf5 283457 15161e f7768e 9ece6a e0af68 7aa2f7 bb9af7 7dcfff a9b1d6 414868 ff899d 9fe044 faba4a 8db0ff c7a9ff a4daff c0caf5",
        ),
        (
            "Ghostex Tokyo Night Storm",
            "24283b c0caf5 c0caf5 2e3c64 1d202f f7768e 9ece6a e0af68 7aa2f7 bb9af7 7dcfff a9b1d6 414868 ff899d 9fe044 faba4a 8db0ff c7a9ff a4daff c0caf5",
        ),
        (
            "Ghostex Tokyo Night Moon",
            "222436 c8d3f5 c8d3f5 2d3f76 1b1d2b ff757f c3e88d ffc777 82aaff c099ff 86e1fc 828bb8 444a73 ff8d94 c7fb6d ffd8ab 9ab8ff caabff b2ebff c8d3f5",
        ),
        (
            "Ghostex Tokyo Night Day",
            "e1e2e7 3760bf 3760bf b7c1e3 b4b5b9 f52a65 587539 8c6c3e 2e7de9 9854f1 007197 6172b0 a1a6c5 ff4774 5c8524 a27629 358aff a463ff 007ea8 3760bf",
        ),
        (
            "Ghostex Nord",
            "2e3440 d8dee9 d8dee9 4c566a 3b4252 bf616a a3be8c ebcb8b 81a1c1 b48ead 88c0d0 e5e9f0 4c566a bf616a a3be8c ebcb8b 81a1c1 b48ead 8fbcbb eceff4",
        ),
        (
            "Ghostex Kanagawa Wave",
            "1f1f28 dcd7ba c8c093 2d4f67 16161d c34043 76946a c0a36e 7e9cd8 957fb8 6a9589 c8c093 727169 e82424 98bb6c e6c384 7fb4ca 938aa9 7aa89f dcd7ba",
        ),
        (
            "Ghostex Kanagawa Dragon",
            "181616 c5c9c5 c8c093 2d4f67 0d0c0c c4746e 8a9a7b c4b28a 8ba4b0 a292a3 8ea4a2 c8c093 a6a69c e46876 87a987 e6c384 7fb4ca 938aa9 7aa89f c5c9c5",
        ),
        (
            "Ghostex Kanagawa Lotus",
            "f2ecbc 545464 43436c c9cbd1 1f1f28 c84053 6f894e 77713f 4d699b b35b79 597b75 545464 8a8980 d7474b 6e915f 836f4a 6693bf 624c83 5e857a 43436c",
        ),
        (
            "Ghostex Everforest Dark",
            "2d353b d3c6aa d3c6aa 543a48 475258 e67e80 a7c080 dbbc7f 7fbbb3 d699b6 83c092 d3c6aa 475258 e67e80 a7c080 dbbc7f 7fbbb3 d699b6 83c092 d3c6aa",
        ),
        (
            "Ghostex Everforest Light",
            "fdf6e3 5c6a72 5c6a72 eaedc8 5c6a72 f85552 8da101 dfa000 3a94c5 df69ba 35a77c e6e2cc 5c6a72 f85552 8da101 dfa000 3a94c5 df69ba 35a77c e6e2cc",
        ),
        (
            "Ghostex Gruvbox Material Dark",
            "282828 d4be98 d4be98 45403d 5a524c ea6962 a9b665 d8a657 7daea3 d3869b 89b482 d4be98 5a524c ea6962 a9b665 d8a657 7daea3 d3869b 89b482 d4be98",
        ),
        (
            "Ghostex Gruvbox Material Light",
            "fbf1c7 654735 654735 eee0b7 654735 c14a4a 6c782e b47109 45707a 945e80 4c7a5d ddccab 654735 c14a4a 6c782e b47109 45707a 945e80 4c7a5d ddccab",
        ),
    ];

    fn hex(color: Rgb) -> String {
        format!("{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    }

    #[test]
    fn every_theme_preset_palette_reads_its_projects_colours() {
        for (name, expected) in CANONICAL {
            let theme = gpui_terminal_theme(name).unwrap_or_else(|| panic!("{name} is missing"));
            let mut read = vec![
                hex(theme.background),
                hex(theme.foreground),
                theme.cursor.map(hex).unwrap_or_default(),
                theme.selection_background.map(hex).unwrap_or_default(),
            ];
            read.extend(theme.palette[..16].iter().copied().map(hex));
            assert_eq!(read.join(" "), expected, "{name}");
        }
    }

    #[test]
    fn theme_colours_read_with_or_without_the_hash_and_refuse_anything_else() {
        let rgb = |value: &str| parse_theme_rgb(value).map(hex);
        assert_eq!(rgb("1e1e2e").as_deref(), Some("1e1e2e"));
        assert_eq!(rgb("#1E1E2E").as_deref(), Some("1e1e2e"));
        for malformed in [
            "",
            "#",
            "1e1e2",
            "#1e1e2e2",
            "+1e1e2",
            "1e1e2g",
            "##1e1e2e",
            "a\u{e9}aaa",
        ] {
            assert_eq!(rgb(malformed), None, "{malformed:?}");
        }
    }

    #[test]
    fn every_preset_names_a_bundled_palette_and_shows_its_colours() {
        let presets = crate::app::window::settings_modal::tabs::theme::presets::theme_presets();
        assert_eq!(presets.len(), CANONICAL.len());
        for preset in presets {
            assert!(
                CANONICAL.iter().any(|(name, _)| *name == preset.palette),
                "{}: {}",
                preset.id,
                preset.palette
            );
            assert!(
                ghostty_theme_source(&preset.palette).is_some(),
                "{}",
                preset.id
            );
            let theme = gpui_terminal_theme(&preset.palette).unwrap();
            let shown: Vec<String> = theme.palette[1..5]
                .iter()
                .map(|color| format!("#{}", hex(*color)))
                .collect();
            assert_eq!(shown, preset.swatch, "{}", preset.id);
        }
    }

    /// The phone's copy of the presets (`src/theme/presets/<id>.json` in the ghostex-mobile
    /// checkout) must match the host's: the same presets, colours, palettes and the settings each
    /// one writes, with light mode following and with light mode picked on purpose. Run it with
    /// the phone checkout at hand:
    /// `GHOSTEX_MOBILE_DIR=<ghostex-mobile> cargo test theme_preset_parity -- --ignored`
    /// (without the variable it reads the `apps/mobile/app` submodule).
    #[test]
    #[ignore = "reads the phone checkout; set GHOSTEX_MOBILE_DIR and run with --ignored"]
    fn theme_preset_parity_with_the_phone() {
        use crate::app::window::settings_modal::tabs::theme::presets::{
            theme_preset_choice, theme_presets,
        };
        use serde_json::{Value, json};
        let mobile = std::env::var_os("GHOSTEX_MOBILE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../mobile/app"));
        let dir = mobile.join("src/theme/presets");
        assert!(
            dir.is_dir(),
            "no phone presets at {}: set GHOSTEX_MOBILE_DIR to a ghostex-mobile checkout that has them",
            dir.display()
        );
        let presets = theme_presets();
        let mut phone_ids: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                name.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        phone_ids.sort();
        let mut host_ids: Vec<String> = presets.iter().map(|preset| preset.id.clone()).collect();
        host_ids.sort();
        assert_eq!(
            phone_ids, host_ids,
            "the phone and the host offer different presets"
        );
        for preset in &presets {
            let path = dir.join(format!("{}.json", preset.id));
            let phone: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let theme = gpui_terminal_theme(&preset.palette).unwrap();
            let colour = |color: Rgb| format!("#{}", hex(color));
            let mut palette = json!({
                "background": colour(theme.background),
                "foreground": colour(theme.foreground),
                "cursor": theme.cursor.map(colour),
                "selectionBackground": theme.selection_background.map(colour),
                "ansi": theme.palette[..16].iter().copied().map(colour).collect::<Vec<_>>(),
            });
            if let Some(cursor_text) = theme.cursor_text {
                palette["cursorText"] = json!(colour(cursor_text));
            }
            let entry = settings_catalog_preset(&preset.id);
            let expected = json!({
                "id": preset.id,
                "name": preset.name,
                "mode": if preset.dark { "dark" } else { "light" },
                "partner": preset.partner,
                "background": entry["background"],
                "tintColor": preset.tint,
                "terminalPalette": preset.palette,
                "palette": palette,
                "patches": {
                    "lightFollows": theme_preset_choice(&preset.id, false).unwrap().1,
                    "lightPickedOnPurpose": theme_preset_choice(&preset.id, true).unwrap().1,
                },
            });
            assert_eq!(phone, expected, "{} differs from the host", path.display());
        }
    }

    fn settings_catalog_preset(id: &str) -> serde_json::Value {
        crate::app::window::settings_modal::catalog::settings_catalog()
            .module_value(
                crate::app::window::settings_modal::catalog::module::SETTINGS,
                "THEME_PRESETS",
            )
            .and_then(serde_json::Value::as_array)
            .and_then(|presets| presets.iter().find(|preset| preset["id"] == id))
            .cloned()
            .unwrap()
    }
}
