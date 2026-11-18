use std::{
    env,
    ffi::{CString, c_char, c_int, c_void},
    ptr::NonNull,
    sync::OnceLock,
};

#[cfg(target_os = "macos")]
use std::{
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

use crate::{
    ghostty_kit::ffi,
    ghostty_vt::VtOptionAsAlt,
    shared_settings::SharedTerminalConfirmCloseSurface,
    terminal_element::{
        TerminalBoldColor, TerminalConfiguredColor, TerminalCursorShape, TerminalMetricAdjustment,
        TerminalMouseShiftCapture, TerminalViewSettings,
    },
    terminal_gpui_engine::{
        GpuiTerminalColorDefaults, GpuiTerminalEngineConfig,
        gpui_engine_terminal_font_config_from_parts,
    },
    terminal_model::Rgb,
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GhosttySurfaceRuntimeError {
    InitFailed(c_int),
    ConfigCreateReturnedNull,
    ConfigPathContainsInteriorNul,
    ConfigOptionInvalid,
}

pub(crate) struct GhosttyConfigOwner {
    pub(crate) config: NonNull<c_void>,
    pub(crate) functions: GhosttyKitFunctionTable,
}

impl GhosttyConfigOwner {
    /// Ghostex's own terminal config, never Ghostty's default files: those are the Ghostty app's
    /// config (`~/.config/ghostty`, `com.mitchellh.ghostty`), which Ghostex no longer reads.
    pub(crate) fn load_ghostex_terminal_config_finalized_with_functions(
        functions: GhosttyKitFunctionTable,
    ) -> Result<Self, GhosttySurfaceRuntimeError> {
        // GitHub #653 crashed inside this load; a crash report names it from here on.
        crate::crash_report::note_event("terminal.config.load");
        let path = crate::shared_settings::ghostex_terminal_config_path();
        let path = CString::new(path.to_string_lossy().into_owned())
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigPathContainsInteriorNul)?;
        let config = unsafe { (functions.config_new)() };
        let config =
            NonNull::new(config).ok_or(GhosttySurfaceRuntimeError::ConfigCreateReturnedNull)?;
        let owner = Self { config, functions };
        unsafe {
            (functions.config_load_file)(owner.as_raw(), path.as_ptr());
            (functions.config_load_recursive_files)(owner.as_raw());
            (functions.config_finalize)(owner.as_raw());
        }
        Ok(owner)
    }

    pub(crate) fn as_raw(&self) -> ffi::ghostty_config_t {
        self.config.as_ptr()
    }
}

impl Drop for GhosttyConfigOwner {
    fn drop(&mut self) {
        unsafe {
            (self.functions.config_free)(self.as_raw());
        }
    }
}

static PRODUCTION_GHOSTTY_INIT_RESULT: OnceLock<Result<(), GhosttySurfaceRuntimeError>> =
    OnceLock::new();

pub(crate) fn initialize_production_ghostty_once(
    functions: GhosttyKitFunctionTable,
) -> Result<(), GhosttySurfaceRuntimeError> {
    *PRODUCTION_GHOSTTY_INIT_RESULT.get_or_init(|| initialize_ghostty_runtime(functions))
}

fn initialize_ghostty_runtime(
    functions: GhosttyKitFunctionTable,
) -> Result<(), GhosttySurfaceRuntimeError> {
    let (argc, argv) = leaked_ghostty_process_argv();
    let result = unsafe { (functions.init)(argc, argv) };
    if result == ffi::GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(GhosttySurfaceRuntimeError::InitFailed(result))
    }
}

fn leaked_ghostty_process_argv() -> (usize, *mut *mut c_char) {
    let mut argv_storage = env::args()
        .map(|arg| CString::new(arg).expect("process argv strings cannot contain interior NUL"))
        .collect::<Vec<_>>();
    if argv_storage.is_empty() {
        argv_storage.push(CString::new("ghostex-gpui").expect("static argv is NUL-free"));
    }

    let argv_ptrs = argv_storage
        .iter()
        .map(|arg| arg.as_ptr() as *mut c_char)
        .collect::<Vec<_>>();
    let argc = argv_ptrs.len();
    let argv = Box::leak(argv_ptrs.into_boxed_slice()).as_mut_ptr();
    let _argv_storage = Box::leak(argv_storage.into_boxed_slice());
    (argc, argv)
}

#[cfg(target_os = "macos")]
pub(crate) fn load_default_ghostty_background_color() -> Option<ffi::ghostty_config_color_s> {
    let functions = GhosttyKitFunctionTable::production();
    initialize_production_ghostty_once(functions).ok()?;
    let config =
        GhosttyConfigOwner::load_ghostex_terminal_config_finalized_with_functions(functions)
            .ok()?;

    let key = b"background";
    let mut color = ffi::ghostty_config_color_s { r: 0, g: 0, b: 0 };
    let has_value = unsafe {
        ffi::ghostty_config_get(
            config.as_raw(),
            (&mut color as *mut ffi::ghostty_config_color_s).cast::<c_void>(),
            key.as_ptr().cast::<c_char>(),
            key.len(),
        )
    };

    has_value.then_some(color)
}

/// Load the finalized terminal-relevant configuration through Ghostty itself
/// from Ghostex's own terminal config (`shared_settings::ghostex_terminal_config_path`),
/// never the Ghostty app's. Themes, defaults, and recursive `config-file`
/// entries are resolved before the canonical snapshot crosses into Rust.
#[cfg(target_os = "macos")]
pub(crate) fn load_ghostty_terminal_engine_config_from_path(
    path: &Path,
    selected_theme_source: Option<&str>,
) -> Result<GpuiTerminalEngineConfig, GhosttySurfaceRuntimeError> {
    let functions = GhosttyKitFunctionTable::production();
    initialize_production_ghostty_once(functions)?;

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigPathContainsInteriorNul)?;
    let config = unsafe { (functions.config_new)() };
    let config =
        NonNull::new(config).ok_or(GhosttySurfaceRuntimeError::ConfigCreateReturnedNull)?;
    let owner = GhosttyConfigOwner { config, functions };
    unsafe {
        // Seed the selected embedded theme before the user's config. Ghostty
        // defines theme colors as the base layer and explicitly configured
        // foreground/background/palette values as overrides. Loading this
        // source after the config reverses that precedence and, for example,
        // replaces a user's white foreground with GitHub Dark's gray one.
        // Keeping the embedded source first also makes the theme available
        // when Ghostex is running without Ghostty.app's resource directory.
        if let Some(source) = selected_theme_source {
            (functions.config_load_string)(
                owner.as_raw(),
                source.as_ptr().cast::<c_char>(),
                source.len(),
            );
        }
        (functions.config_load_file)(owner.as_raw(), path.as_ptr());
        (functions.config_load_recursive_files)(owner.as_raw());
        (functions.config_finalize)(owner.as_raw());
    }

    let formatted = unsafe { ffi::ghostty_config_to_string(owner.as_raw()) };
    let bytes = if formatted.ptr.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(formatted.ptr.cast::<u8>(), formatted.len) }.to_vec()
    };
    unsafe { (functions.string_free)(formatted) };
    let formatted =
        String::from_utf8(bytes).map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
    parse_ghostty_terminal_engine_config(&formatted)
}

#[cfg(target_os = "macos")]
fn parse_ghostty_terminal_engine_config(
    formatted: &str,
) -> Result<GpuiTerminalEngineConfig, GhosttySurfaceRuntimeError> {
    let value = |key: &str| canonical_config_values(formatted, key).into_iter().last();
    /*
    Ghostty's canonical formatter represents an unset optional value as an
    empty right-hand side (for example `cursor-text = `). That is valid typed
    configuration, not a malformed color or enum. Keep ordinary string
    settings capable of carrying an intentional empty value, but decode the
    nullable terminal settings through this explicit boundary so a valid
    finalized config cannot abort creation of a newly selected session.
    */
    let optional_value = |key: &str| value(key).filter(|value| !value.is_empty());
    let font_family = canonical_config_values(formatted, "font-family")
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or("JetBrains Mono");
    let font_size = parse_config_f32(value("font-size"))?;
    let font_weight = canonical_config_values(formatted, "font-variation")
        .into_iter()
        .filter_map(|value| value.strip_prefix("wght="))
        .filter_map(|value| value.parse::<f32>().ok())
        .next_back()
        .unwrap_or(400.0);
    let mut font = gpui_engine_terminal_font_config_from_parts(font_family, font_size, font_weight);
    font.bold_color = optional_value("bold-color")
        .map(|value| match value {
            "bright" => Ok(TerminalBoldColor::Bright),
            value => parse_rgb_value(value).map(TerminalBoldColor::Rgb),
        })
        .transpose()?;
    let ghostty_bold_weight = parse_ghostty_bold_weight(
        value("font-style-bold"),
        canonical_config_values(formatted, "font-variation-bold"),
        font_weight,
    );
    font.cell_width_adjustment = parse_metric_adjustment(optional_value("adjust-cell-width"))?;
    font.cell_height_adjustment = parse_metric_adjustment(optional_value("adjust-cell-height"))?;

    let foreground = parse_rgb(value("foreground"))?;
    let background = parse_rgb(value("background"))?;
    let cursor = optional_value("cursor-color")
        .map(parse_rgb_value)
        .transpose()?;
    let mut palette = [Rgb::default(); 256];
    let mut palette_count = 0usize;
    for entry in canonical_config_values(formatted, "palette") {
        let Some((index, color)) = entry.split_once('=') else {
            return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
        };
        let index = index
            .trim()
            .parse::<usize>()
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        let slot = palette
            .get_mut(index)
            .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        *slot = parse_rgb_value(color.trim())?;
        palette_count += 1;
    }
    if palette_count != 256 {
        return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }

    let cursor_shape = match value("cursor-style").unwrap_or("block") {
        "bar" => TerminalCursorShape::Bar,
        "underline" => TerminalCursorShape::Underline,
        "block" | "block_hollow" => TerminalCursorShape::Block,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let option_as_alt = match optional_value("macos-option-as-alt").unwrap_or("false") {
        "false" => VtOptionAsAlt::False,
        "true" => VtOptionAsAlt::True,
        "left" => VtOptionAsAlt::Left,
        "right" => VtOptionAsAlt::Right,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let confirm_close_surface = match value("confirm-close-surface").unwrap_or("false") {
        "false" => SharedTerminalConfirmCloseSurface::False,
        "true" => SharedTerminalConfirmCloseSurface::True,
        "always" => SharedTerminalConfirmCloseSurface::Always,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let mouse_shift_capture = match value("mouse-shift-capture").unwrap_or("false") {
        "false" => TerminalMouseShiftCapture::False,
        "true" => TerminalMouseShiftCapture::True,
        "always" => TerminalMouseShiftCapture::Always,
        "never" => TerminalMouseShiftCapture::Never,
        _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    };
    let (mouse_scroll_precision, mouse_scroll_discrete) =
        parse_mouse_scroll_multiplier(value("mouse-scroll-multiplier"))?;

    // Ghostty's numeric blur retains the configured background; its macOS
    // glass styles supply that background themselves. Match the byte alpha
    // used by Ghostty's renderer before any custom shader samples it.
    let shader_background_alpha = if !crate::terminal_shaders::enabled() {
        1.0
    } else {
        match value("background-blur") {
            Some("macos-glass-regular" | "macos-glass-clear") => 0.0,
            _ => {
                (parse_config_f32(value("background-opacity"))?.clamp(0.0, 1.0) * 255.0).round()
                    / 255.0
            }
        }
    };

    Ok(GpuiTerminalEngineConfig {
        font,
        view: TerminalViewSettings {
            light_theme: false,
            cursor_shape,
            shaders: crate::terminal_shaders::load(
                canonical_config_values(formatted, "custom-shader")
                    .into_iter()
                    .filter(|path| !path.is_empty())
                    .filter_map(|path| {
                        if let Some(optional) = path.strip_prefix('?') {
                            let path = PathBuf::from(optional);
                            path.exists().then_some(path)
                        } else {
                            Some(PathBuf::from(path))
                        }
                    })
                    .collect(),
                value("custom-shader-animation").unwrap_or("true"),
                shader_background_alpha,
            ),
            // The GhosttyKit surface path is not selected at runtime; the
            // composited engine owns background images.
            background_image: None,
            background_alpha: 1.0,
            cursor_blink: parse_config_bool(
                optional_value("cursor-style-blink").unwrap_or("false"),
            )?,
            cursor_opacity: parse_config_f32(value("cursor-opacity"))?,
            cursor_text: optional_value("cursor-text")
                .map(parse_terminal_configured_color)
                .transpose()?,
            selection_background: optional_value("selection-background")
                .map(parse_terminal_configured_color)
                .transpose()?,
            selection_clear_on_copy: parse_config_bool(
                value("selection-clear-on-copy").unwrap_or("false"),
            )?,
            selection_clear_on_typing: parse_config_bool(
                value("selection-clear-on-typing").unwrap_or("true"),
            )?,
            selection_word_chars: value("selection-word-chars")
                .unwrap_or(" \t'\"│`|:;,()[]{}<>$")
                .to_string(),
            // CDXC:Clipboard 2026-09-27 WHY: Ghostty 1.4 renamed the copy-on-select values to none | primary | clipboard | both and finalizes the legacy `true` as `clipboard` on macOS, so the formatted config can no longer tell Ghostex's `true` (selection clipboard) from `clipboard` (system and selection). Both now copy to the system clipboard and keep middle-click paste; `primary` is selection-only; `none`/`false` is off.
            copy_on_select: matches!(value("copy-on-select"), Some("clipboard" | "both")),
            selection_clipboard_enabled: matches!(
                value("copy-on-select"),
                Some("true" | "primary" | "clipboard" | "both")
            ),
            clipboard_trim_trailing_spaces: parse_config_bool(
                value("clipboard-trim-trailing-spaces").unwrap_or("true"),
            )?,
            mouse_hide_while_typing: parse_config_bool(
                value("mouse-hide-while-typing").unwrap_or("false"),
            )?,
            mouse_scroll_precision,
            mouse_scroll_discrete,
            mouse_shift_capture,
            scrollbar_visible: value("scrollbar").unwrap_or("system") != "never",
            // This is app-owned rather than a Ghostty config key. Callers
            // overwrite it from shared Settings after loading the finalized
            // Ghostty config.
            scroll_to_bottom_when_typing: true,
        },
        colors: Some(GpuiTerminalColorDefaults {
            foreground,
            background,
            cursor,
            palette,
        }),
        // Ghostty 1.4 renamed `scrollback-limit` to `scrollback-limit-bytes`;
        // the canonical formatter emits only the new key, either as a byte
        // count or as the `unlimited` sentinel (integer max upstream).
        scrollback_limit_bytes: match value("scrollback-limit-bytes")
            .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?
        {
            "unlimited" => u64::MAX,
            value => value
                .parse::<u64>()
                .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?,
        },
        option_as_alt,
        confirm_close_surface,
        ghostty_bold_weight,
    })
}

/// The bold weight Ghostty's own bold keys ask for, `None` when neither is
/// set. `font-style-bold = false` draws bold with the regular face (and so the
/// regular `font-variation`), a `wght` in `font-variation-bold` is the bold
/// face's weight, and a named style maps to its standard weight. Ghostty uses a
/// named style only with `font-family-bold`; Ghostex draws one family, so the
/// name applies to it.
#[cfg(target_os = "macos")]
fn parse_ghostty_bold_weight(
    font_style_bold: Option<&str>,
    font_variation_bold: Vec<&str>,
    regular_weight: f32,
) -> Option<gpui::FontWeight> {
    if font_style_bold == Some("false") {
        return Some(gpui::FontWeight(regular_weight));
    }
    let variation = font_variation_bold
        .into_iter()
        .filter_map(|value| value.strip_prefix("wght="))
        .filter_map(|value| value.parse::<f32>().ok())
        .next_back();
    if let Some(weight) = variation {
        return Some(gpui::FontWeight(weight));
    }
    let name = font_style_bold?
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let weight = match name.as_str() {
        "thin" | "hairline" => 100.0,
        "extralight" | "ultralight" => 200.0,
        "light" => 300.0,
        "regular" | "normal" | "book" | "roman" => 400.0,
        "medium" => 500.0,
        "semibold" | "demibold" => 600.0,
        "bold" => 700.0,
        "extrabold" | "ultrabold" => 800.0,
        "black" | "heavy" => 900.0,
        _ => return None,
    };
    Some(gpui::FontWeight(weight))
}

#[cfg(target_os = "macos")]
fn canonical_config_values<'a>(formatted: &'a str, key: &str) -> Vec<&'a str> {
    formatted
        .lines()
        .filter_map(|line| {
            let (candidate, value) = line.split_once(" = ")?;
            (candidate == key).then_some(value)
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn parse_rgb(value: Option<&str>) -> Result<Rgb, GhosttySurfaceRuntimeError> {
    parse_rgb_value(value.ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?)
}

#[cfg(target_os = "macos")]
fn parse_rgb_value(value: &str) -> Result<Rgb, GhosttySurfaceRuntimeError> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 {
        return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }
    let component = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&value[range], 16)
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
    };
    Ok(Rgb {
        r: component(0..2)?,
        g: component(2..4)?,
        b: component(4..6)?,
    })
}

#[cfg(target_os = "macos")]
fn parse_terminal_configured_color(
    value: &str,
) -> Result<TerminalConfiguredColor, GhosttySurfaceRuntimeError> {
    match value {
        "cell-foreground" => Ok(TerminalConfiguredColor::CellForeground),
        "cell-background" => Ok(TerminalConfiguredColor::CellBackground),
        value => parse_rgb_value(value).map(TerminalConfiguredColor::Rgb),
    }
}

#[cfg(target_os = "macos")]
fn parse_config_bool(value: &str) -> Result<bool, GhosttySurfaceRuntimeError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
    }
}

#[cfg(target_os = "macos")]
fn parse_config_f32(value: Option<&str>) -> Result<f32, GhosttySurfaceRuntimeError> {
    value
        .ok_or(GhosttySurfaceRuntimeError::ConfigOptionInvalid)?
        .parse::<f32>()
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
}

#[cfg(target_os = "macos")]
fn parse_metric_adjustment(
    value: Option<&str>,
) -> Result<TerminalMetricAdjustment, GhosttySurfaceRuntimeError> {
    let Some(value) = value else {
        return Ok(TerminalMetricAdjustment::None);
    };
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .parse::<f32>()
            .map(|value| TerminalMetricAdjustment::Percent(value / 100.0))
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid);
    }
    value
        .parse::<f32>()
        .map(TerminalMetricAdjustment::Absolute)
        .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)
}

#[cfg(target_os = "macos")]
fn parse_mouse_scroll_multiplier(
    value: Option<&str>,
) -> Result<(f32, f32), GhosttySurfaceRuntimeError> {
    let mut precision = 1.0;
    let mut discrete = 3.0;
    for part in value.unwrap_or("precision:1,discrete:3").split(',') {
        let Some((key, value)) = part.split_once(':') else {
            let value = part
                .parse::<f32>()
                .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
            return Ok((value, value));
        };
        let value = value
            .parse::<f32>()
            .map_err(|_| GhosttySurfaceRuntimeError::ConfigOptionInvalid)?;
        match key {
            "precision" => precision = value,
            "discrete" => discrete = value,
            _ => return Err(GhosttySurfaceRuntimeError::ConfigOptionInvalid),
        }
    }
    Ok((precision, discrete))
}
