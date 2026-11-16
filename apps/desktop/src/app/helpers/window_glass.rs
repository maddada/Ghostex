//! Window glass: whether the main window shows the blurred desktop through its surfaces, and the
//! fills each layer paints so the tint is applied once instead of stacking into an opaque slab.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use gpui::{Context, Hsla, Window, WindowBackgroundAppearance};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;

static WINDOW_GLASS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// CDXC:Theming 2026-09-23 DECISION:
/// User: "what do you think about making the glass take only from the actual desktop bg and ignore the windows behind the ghostex window??", then "yes let's do it please implement like you said. with the setting too.", then, because a picture held against the screen trailed the window while it was dragged: "can we make it so when we add a photo or when we pick wallpaper only it's either static or it moves with the image? ... doesn't look as nice as just windows + desktop <-lets keep this one as default for now / and keep static as default too for now". The Glass shows setting (`windowGlassSource`) defaults to Desktop and windows, the live blur of everything behind the window; Wallpaper only shows the blurred desktop picture, so other apps' windows never show through. For Wallpaper only and Custom image, `windowGlassImagePlacement` picks Static (the default: the picture covers the window and moves with it, so nothing updates during a drag) or Follows the desktop (the picture stays still against the screen, re-placed on every move).
///
/// CDXC:Theming 2026-09-23 WHY:
/// The picture is read from the wallpaper file rather than captured from the screen, because capturing the desktop needs the Screen Recording permission. Menus, pickers and other popups keep the live blur in both modes: they float over the app itself, which is what they must blur.
///
/// CDXC:Theming 2026-09-23 DECISION:
/// User: "20b sounds good lets try it". macOS's built-in wallpapers (Sequoia, Sonoma, the aerials and the like) are drawn by wallpaper extensions and report only a placeholder picture, so the glass finds the real choice in the wallpaper agent's store and blurs that wallpaper's bundled thumbnail instead (`window_wallpaper_system.rs` in the GPUI macOS crate).
///
/// CDXC:Theming 2026-09-23 WHY:
/// That store and the extensions' thumbnails are private and undocumented and may change with any macOS update, so every step gives up on anything it does not recognise and the window keeps the live blur; solid colour wallpapers keep it too.
static WINDOW_GLASS_WALLPAPER: AtomicBool = AtomicBool::new(false);

/// `windowGlassImagePlacement` is "desktop": the picture stays still against the screen.
static WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN: AtomicBool = AtomicBool::new(false);

/// CDXC:Theming 2026-09-30 DECISION:
/// User: "i'm not able to set the transparency blur level in ghostex on macos please add sliders for this (hope they can work on other oses too)". Settings has a Blur slider (`windowGlassBlurRadius`, 0 to 100 points, default 60: the main window's glass as it always was) and a Menu blur slider (`windowGlassMenuBlurRadius`, default 20: `FROSTED_MENU_BLUR_RADIUS`). Blur reaches every GPUI backend through `set_background_blur_style`: on macOS it sets the live blur, the wallpaper and picture blur and the video blur; on Windows and Linux the system draws the Desktop and windows blur and has no radius to set, so there it sets the wallpaper, picture and video blur. Menu blur is macOS only for the same reason. 0 shows what is behind the glass sharp. On Windows with Desktop and windows the slider gives way to the Windows blur choice (`WINDOW_GLASS_WINDOWS_BLUR`), and Menu blur is hidden off macOS (`availability.rs` in the settings catalog).
static WINDOW_GLASS_BLUR_RADIUS: AtomicU8 = AtomicU8::new(WINDOW_GLASS_BLUR_RADIUS_DEFAULT);

/// CDXC:Theming 2026-10-16 DECISION:
/// User: "please let me adjust the app's blur and strength of transparency in windows when transparency is enabled". Windows draws the Desktop and windows blur itself and has no radius to set, so on Windows Blur is a choice there (`windowGlassWindowsBlur`): Acrylic (the default, the system acrylic with its own frost, kept blurred while Ghostex is in the background), Plain blur (the same blur without that frost, so the tints reach fully clear, at a higher drawing cost) and Off (sharp, the cheapest). Supersedes the 2026-10-04 hiding of Blur on Windows for Desktop and windows.
/// SEE-ALSO: packages/settings-catalog/src/availability.rs, gpui_windows window.rs `set_whole_window_blur`.
static WINDOW_GLASS_WINDOWS_BLUR: AtomicU8 = AtomicU8::new(WINDOWS_BLUR_ACRYLIC);

/// `windowGlassWindowsBlur` values, as stored in `WINDOW_GLASS_WINDOWS_BLUR`.
const WINDOWS_BLUR_ACRYLIC: u8 = 0;
const WINDOWS_BLUR_PLAIN: u8 = 1;
const WINDOWS_BLUR_OFF: u8 = 2;

/// The blur DWM draws behind the main window for Desktop and windows (`windowGlassWindowsBlur`).
fn window_glass_behind_window_blur() -> gpui::BehindWindowBlur {
    match WINDOW_GLASS_WINDOWS_BLUR.load(Ordering::Relaxed) {
        WINDOWS_BLUR_PLAIN => gpui::BehindWindowBlur::Plain,
        WINDOWS_BLUR_OFF => gpui::BehindWindowBlur::Off,
        _ => gpui::BehindWindowBlur::System,
    }
}

/// The main window glass's blur radius in points until Settings says otherwise.
const WINDOW_GLASS_BLUR_RADIUS_DEFAULT: u8 = 60;

/// `windowGlassMenuBlurRadius`: the blur radius of frosted menus and tooltips.
static WINDOW_GLASS_MENU_BLUR_RADIUS: AtomicU8 = AtomicU8::new(FROSTED_MENU_BLUR_RADIUS as u8);

/// The main window glass's blur radius (`windowGlassBlurRadius`), in points.
fn window_glass_blur_radius() -> gpui::Pixels {
    gpui::px(f32::from(WINDOW_GLASS_BLUR_RADIUS.load(Ordering::Relaxed)))
}

/// Reads a blur radius setting: whole points from 0 to 100 (`MAX_WINDOW_GLASS_BLUR_RADIUS` in
/// packages/shared/ghostex-settings/option-tables.ts (deleted 2026-10-01)).
fn read_blur_radius(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: u8,
) -> u8 {
    read_glass_percent(object, key).map_or(default, |value| value.clamp(0.0, 100.0).round() as u8)
}

/// CDXC:Theming 2026-09-23 DECISION:
/// User: "ok i also want the option that the user picks any image to use as their wallpaper (different one for dark and light modes)". Glass shows has a third choice, Custom image, with one picture for dark mode (`windowGlassImageDark`) and one for light mode (`windowGlassImageLight`); the main window's glass is that picture, blurred and placed like the desktop picture (`windowGlassImagePlacement`).
///
/// CDXC:Theming 2026-09-23 WHY:
/// A mode with no picture chosen, or one whose file cannot be read, shows the live blur rather than an empty or black backdrop, so the window is never left without glass while a picture is being chosen or after its file moved.
static WINDOW_GLASS_IMAGES: std::sync::Mutex<WindowGlassImages> =
    std::sync::Mutex::new(WindowGlassImages {
        custom: false,
        dark: String::new(),
        light: String::new(),
    });

struct WindowGlassImages {
    custom: bool,
    dark: String,
    light: String,
}

/// CDXC:Theming 2026-09-23 WHY:
/// A background video must cost nothing while nobody is looking, so it plays only while Ghostex is the front app and its window is on screen, and pauses while the displays sleep, in Low Power Mode, and on battery unless `windowGlassVideoOnlyOnPower` is off (on by default, to spare the battery); Reduce Motion shows its first frame instead. A mode whose Live slot is its own video but has no playable file shows the live blur. Which mode plays a video is the Live slot's choice (`window_glass_live.rs`).
static WINDOW_GLASS_VIDEOS: std::sync::Mutex<WindowGlassVideos> =
    std::sync::Mutex::new(WindowGlassVideos {
        video: false,
        dark: None,
        light: None,
        only_on_power: true,
    });

struct WindowGlassVideos {
    video: bool,
    dark: Option<std::path::PathBuf>,
    light: Option<std::path::PathBuf>,
    only_on_power: bool,
}

/// The user's own video the main window's glass plays in the current appearance, when Glass
/// shows is Live, that mode's slot is its own video and the file is playable, and whether it
/// pauses on battery.
fn window_glass_video() -> (Option<std::path::PathBuf>, bool) {
    let Ok(videos) = WINDOW_GLASS_VIDEOS.lock() else {
        return (None, true);
    };
    let light = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    if !videos.video || !window_glass_live::window_glass_live_plays_video(light) {
        return (None, videos.only_on_power);
    }
    let video = if light {
        videos.light.clone()
    } else {
        videos.dark.clone()
    };
    (video, videos.only_on_power)
}

/// CDXC:Theming 2026-11-08 DECISION:
/// User: "can we please on macos detect if the app is put into native macos fullscreen and the app is transparent mode then we show the bg of the desktop behind the app instead of showing black bg like we do now??". On macOS a workspace window in native fullscreen whose glass would show the live blur (Behind the glass set to Desktop and windows, or Picture or Live with nothing chosen for this appearance) shows the screen's desktop picture instead, held against the screen as macOS places it and blurred and tinted exactly as the glass is (Blur, Strength). Leaving fullscreen returns to the live blur. Wallpaper only, a chosen picture and Live already show their own backdrop and are unchanged.
///
/// CDXC:Theming 2026-11-08 WHY:
/// A fullscreen window sits alone in its own Space, so the live behind-window blur has nothing behind it but black. The picture is the Wallpaper only backdrop the macOS backend already draws (gpui_macos window_wallpaper.rs): read through `desktopImageURLForScreen:` and its options, built-in wallpapers through their still, decoded and blurred once per picture, screen size and blur and cached, and re-read when the window changes screens or Spaces or Ghostex becomes active. A picture it cannot read (a solid colour wallpaper from the colour picker, a wallpaper provider it does not know) leaves the live blur, which is today's black, never a broken image. Nothing is read while the window is not fullscreen or the glass is off.
fn window_glass_fullscreen_shows_desktop(window: &Window, blurred: bool) -> bool {
    cfg!(target_os = "macos") && blurred && window.is_fullscreen()
}

/// Whether the glass shows a picture, animation or video rather than the live blur: Wallpaper
/// only always does, Custom image and Live only once one is chosen for the current appearance.
fn window_glass_uses_backdrop(
    image: &Option<std::path::PathBuf>,
    video: &Option<std::path::PathBuf>,
    live: &Option<gpui::LiveBackground>,
) -> bool {
    if !WINDOW_GLASS_WALLPAPER.load(Ordering::Relaxed) {
        return false;
    }
    if window_glass_live::window_glass_live_mode() {
        return live.is_some() || video.is_some();
    }
    let custom = WINDOW_GLASS_IMAGES.lock().is_ok_and(|images| images.custom);
    if custom { image.is_some() } else { true }
}

/// The picture the main window's glass shows in the current appearance, when Glass shows is
/// Custom image and one is chosen for that appearance.
fn window_glass_custom_image() -> Option<std::path::PathBuf> {
    let images = WINDOW_GLASS_IMAGES.lock().ok()?;
    if !images.custom {
        return None;
    }
    let path = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        &images.light
    } else {
        &images.dark
    };
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

/// What each workspace window's glass was last switched to, by window id. GPUI has no getter for
/// a window's background appearance, and re-applying it every frame rebuilds the window's AppKit
/// backing state. Kept per window, because File > New Window opens several and one window's
/// record would otherwise stop the next from ever getting its glass (app/workspace_windows/).
static APPLIED_WINDOW_GLASS: std::sync::Mutex<
    Option<std::collections::HashMap<u64, AppliedWindowGlass>>,
> = std::sync::Mutex::new(None);

#[derive(Clone, PartialEq)]
struct AppliedWindowGlass {
    /// 1 opaque, 2 live blur, 3 wallpaper blur.
    code: u8,
    /// Native fullscreen showing the desktop picture in place of the live blur.
    fullscreen_desktop: bool,
    blur: u8,
    behind_blur: gpui::BehindWindowBlur,
    image: Option<std::path::PathBuf>,
    video: (Option<std::path::PathBuf>, bool),
    live: Option<gpui::LiveBackground>,
}

/// Only the main window is blurred. Views that also render in their own windows (the chat) ask
/// with `window_glass_active_in` so they stay opaque there.
static MAIN_WINDOW_ID: AtomicU64 = AtomicU64::new(u64::MAX);

/// Default coverage of the sidebar's and the work area's tints over the blurred desktop: point 10
/// of the Transparency strength slider (CDXC:Theming 2026-10-04 DECISION on the tint defaults in
/// packages/settings-catalog/src/data/defaults.rs; `transparencyStrengthPatch` in
/// packages/core-ui/settings-modal/theme-simple-controls.tsx (deleted 2026-10-01)), the same tint for the sidebar
/// and the work area. SEE-ALSO: the CDXC:Theming 2026-09-25 DECISION on
/// `DEFAULT_WINDOW_GLASS_SIDEBAR_OPACITY_DARK_PERCENT` in packages/shared/ghostex-settings/types.ts (deleted 2026-10-01).
const SIDEBAR_GLASS_ALPHA_DARK: f32 = 0.94;
const SIDEBAR_GLASS_ALPHA_LIGHT: f32 = 0.97;
const WORK_AREA_GLASS_ALPHA_DARK: f32 = 0.94;
const WORK_AREA_GLASS_ALPHA_LIGHT: f32 = 0.97;

/// CDXC:Theming 2026-09-23 DECISION:
/// User: "implement sliders for the glass for sidebar vs main area (2 different sliders for dark mode, and 2 for light mode)", then "can we make the sidebar darker than main area somehow? currently this isn't possible / i feel would be nicer if they are separate and each can be modified freely? not doubling up the transparency for workarea when i do for sidebar??". Under glass the sidebar and the work area each paint their own tint straight over the blurred desktop, and nothing tints the window underneath both, so either area can be the darker one. Settings holds four percentages (`windowGlassSidebarOpacityDark`, `windowGlassWorkAreaTintDark`, `windowGlassSidebarOpacityLight`, `windowGlassWorkAreaTintLight`) whose defaults are the constants above. This supersedes the same day's shell tint under the whole window with the work area's value as an extra layer over it; a saved extra layer (`windowGlassMainOpacity*`) is carried over as the coverage the two layers added up to.
static SIDEBAR_GLASS_PERCENT_DARK: AtomicU8 =
    AtomicU8::new(glass_percent(SIDEBAR_GLASS_ALPHA_DARK));
static SIDEBAR_GLASS_PERCENT_LIGHT: AtomicU8 =
    AtomicU8::new(glass_percent(SIDEBAR_GLASS_ALPHA_LIGHT));
static WORK_AREA_GLASS_PERCENT_DARK: AtomicU8 =
    AtomicU8::new(glass_percent(WORK_AREA_GLASS_ALPHA_DARK));
static WORK_AREA_GLASS_PERCENT_LIGHT: AtomicU8 =
    AtomicU8::new(glass_percent(WORK_AREA_GLASS_ALPHA_LIGHT));

const fn glass_percent(alpha: f32) -> u8 {
    (alpha * 100.0 + 0.5) as u8
}

fn read_glass_percent(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<f64> {
    object
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
}

/// Stores the sidebar's percentage and returns it.
fn store_sidebar_glass_percent(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default_alpha: f32,
    target: &AtomicU8,
) -> u8 {
    let percent = read_glass_percent(object, key)
        // Full 0-100% (`MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT` in packages/shared/ghostex-settings/types.ts (deleted 2026-10-01)).
        .map(|value| value.clamp(0.0, 100.0).round() as u8)
        .unwrap_or_else(|| glass_percent(default_alpha));
    target.store(percent, Ordering::Relaxed);
    percent
}

/// Stores the work area's percentage. Settings that still hold the retired extra layer
/// (`legacy_extra_key`) get the coverage it and the sidebar tint added up to, the same migration
/// `normalizeWindowGlassWorkAreaTint` in packages/shared/ghostex-settings/normalize-fields.ts (deleted 2026-10-01) applies.
fn store_work_area_glass_percent(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    legacy_extra_key: &str,
    sidebar_percent: u8,
    default_alpha: f32,
    target: &AtomicU8,
) {
    let percent = read_glass_percent(object, key)
        .or_else(|| {
            read_glass_percent(object, legacy_extra_key).map(|extra| {
                let sidebar = f64::from(sidebar_percent) / 100.0;
                let extra = extra.clamp(0.0, 100.0) / 100.0;
                (1.0 - (1.0 - sidebar) * (1.0 - extra)) * 100.0
            })
        })
        .map(|value| value.clamp(0.0, 100.0).round() as u8)
        .unwrap_or_else(|| glass_percent(default_alpha));
    target.store(percent, Ordering::Relaxed);
}

fn load_glass_alpha(source: &AtomicU8) -> f32 {
    f32::from(source.load(Ordering::Relaxed)) / 100.0
}

/// CDXC:Theming 2026-09-23 DECISION:
/// User: the whole desktop window can be frosted glass that shows the blurred desktop behind it, as its own setting rather than part of a theme. Automatic (the default) is frosted in dark mode and opaque in light mode, where glass has the weakest text contrast; Frosted and Opaque force one look.
///
/// CDXC:Theming 2026-09-27 DECISION:
/// User: "I want transparency to work on linux just like it does on macOS for the gpui app", including every transparency setting on Wayland and X11. This supersedes the Linux exclusion from the 2026-09-25 Windows glass implementation. Shared tint and source rules apply on all three desktop platforms; Linux's GPUI backend owns compositor blur, pictures and Live playback. System transparency preferences still win; Linux compositors choose their own behind-window blur algorithm.
///
/// Returns whether the resolved state changed.
pub(crate) fn refresh_window_glass(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let light = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    // A setting never saved takes the platform's default (Never on Windows, Dark only elsewhere).
    let wanted = match object
        .get("windowGlass")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            ghostex_settings_catalog::availability::default_value_on(
                ghostex_settings_catalog::Platform::current(),
                "windowGlass",
            )
            .and_then(ghostex_settings_catalog::J::as_str)
        }) {
        Some("frosted") => true,
        Some("opaque") => false,
        _ => !light,
    };
    let sidebar_dark = store_sidebar_glass_percent(
        object,
        "windowGlassSidebarOpacityDark",
        SIDEBAR_GLASS_ALPHA_DARK,
        &SIDEBAR_GLASS_PERCENT_DARK,
    );
    // Same for light and dark (cleanup Q19): light mode reads the dark-mode tints, picture and video.
    let light_key =
        |light: &'static str| crate::shared_settings::window_glass_light_key(object, light);
    let sidebar_light = store_sidebar_glass_percent(
        object,
        light_key("windowGlassSidebarOpacityLight"),
        SIDEBAR_GLASS_ALPHA_LIGHT,
        &SIDEBAR_GLASS_PERCENT_LIGHT,
    );
    store_work_area_glass_percent(
        object,
        "windowGlassWorkAreaTintDark",
        "windowGlassMainOpacityDark",
        sidebar_dark,
        WORK_AREA_GLASS_ALPHA_DARK,
        &WORK_AREA_GLASS_PERCENT_DARK,
    );
    store_work_area_glass_percent(
        object,
        light_key("windowGlassWorkAreaTintLight"),
        if light_key("windowGlassWorkAreaTintLight") == "windowGlassWorkAreaTintLight" {
            "windowGlassMainOpacityLight"
        } else {
            "windowGlassMainOpacityDark"
        },
        sidebar_light,
        WORK_AREA_GLASS_ALPHA_LIGHT,
        &WORK_AREA_GLASS_PERCENT_LIGHT,
    );
    let source = object
        .get("windowGlassSource")
        .and_then(serde_json::Value::as_str);
    // The wallpaper, picture and Live backdrops are drawn by the macOS, Windows and Linux window
    // backends (gpui_macos window_wallpaper.rs / window_live.rs, gpui_windows directx_backdrop.rs,
    // gpui_linux); Settings offers them only there.
    WINDOW_GLASS_WALLPAPER.store(
        cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )) && matches!(source, Some("wallpaper" | "customImage" | "video" | "live")),
        Ordering::Relaxed,
    );
    WINDOW_GLASS_BLUR_RADIUS.store(
        read_blur_radius(
            object,
            "windowGlassBlurRadius",
            WINDOW_GLASS_BLUR_RADIUS_DEFAULT,
        ),
        Ordering::Relaxed,
    );
    WINDOW_GLASS_WINDOWS_BLUR.store(
        match object
            .get("windowGlassWindowsBlur")
            .and_then(serde_json::Value::as_str)
        {
            Some("plain") => WINDOWS_BLUR_PLAIN,
            Some("off") => WINDOWS_BLUR_OFF,
            _ => WINDOWS_BLUR_ACRYLIC,
        },
        Ordering::Relaxed,
    );
    WINDOW_GLASS_MENU_BLUR_RADIUS.store(
        read_blur_radius(
            object,
            "windowGlassMenuBlurRadius",
            FROSTED_MENU_BLUR_RADIUS as u8,
        ),
        Ordering::Relaxed,
    );
    WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.store(
        object
            .get("windowGlassImagePlacement")
            .and_then(serde_json::Value::as_str)
            == Some("desktop"),
        Ordering::Relaxed,
    );
    if let Ok(mut images) = WINDOW_GLASS_IMAGES.lock() {
        let image = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        };
        images.custom = source == Some("customImage");
        images.dark = image("windowGlassImageDark");
        images.light = image(crate::shared_settings::window_glass_light_key(
            object,
            "windowGlassImageLight",
        ));
    }
    if let Ok(mut videos) = WINDOW_GLASS_VIDEOS.lock() {
        let video = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(window_glass_video::resolve_glass_video)
        };
        // A settings file saved before Live took over the videos may still say Video. The user's own
        // video plays in the macOS and Linux backends; Windows does not offer it.
        videos.video = cfg!(any(target_os = "macos", target_os = "linux"))
            && matches!(source, Some("live" | "video"));
        videos.dark = video("windowGlassVideoDark");
        videos.light = video(crate::shared_settings::window_glass_light_key(
            object,
            "windowGlassVideoLight",
        ));
        videos.only_on_power = object
            .get("windowGlassVideoOnlyOnPower")
            .and_then(serde_json::Value::as_bool)
            != Some(false);
    }
    window_glass_live::refresh_window_glass_live(object);
    terminal_glass::refresh_terminal_glass(object);
    // A picture, video or Live backdrop is drawn by Ghostex itself; only the desktop behind the
    // window needs the compositor's blur.
    let shows_desktop = !matches!(source, Some("wallpaper" | "customImage" | "video" | "live"));
    let blur_withheld = shows_desktop && desktop_withholds_blur();
    let active = cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )) && wanted
        && !system_reduces_transparency()
        && !blur_withheld;
    WINDOW_GLASS_SYSTEM_BLOCKED.store(
        (cfg!(any(target_os = "macos", target_os = "windows")) && system_reduces_transparency())
            || blur_withheld,
        Ordering::Relaxed,
    );
    WINDOW_GLASS_ACTIVE.swap(active, Ordering::Relaxed) != active
}

/// The system switch that turns transparency off, as of the last `refresh_window_glass`.
static WINDOW_GLASS_SYSTEM_BLOCKED: AtomicBool = AtomicBool::new(false);

/// Whether the system's own switch (Reduce Transparency on macOS, Transparency effects off on
/// Windows) is keeping the window opaque, so Settings can say why the glass shows nothing.
pub(crate) fn window_glass_blocked_by_system() -> bool {
    WINDOW_GLASS_SYSTEM_BLOCKED.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
fn system_reduces_transparency() -> bool {
    unsafe { GhostexGpuiAccessibilityDisplayShouldReduceTransparency() == 1 }
}

/// Windows' Settings > Personalization > Colors > Transparency effects, off.
#[cfg(target_os = "windows")]
fn system_reduces_transparency() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "EnableTransparency\0".encode_utf16().collect();
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    status == 0 && data == 0
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_reduces_transparency() -> bool {
    false
}

/// Whether the desktop is known not to blur what is behind a see-through window: Hyprland with
/// `decoration:blur:enabled` off (Omarchy's default; its Glassy Windows turns it on), and GNOME,
/// whose Mutter has no blur for apps. KDE blurs on request, and a desktop Ghostex cannot ask keeps
/// the glass.
///
/// CDXC:Theming 2026-11-14 DECISION:
/// User (Omarchy audit Q8): "paint the window opaque when the desktop won't blur it (and tell people how to turn on Omarchy's Glassy look)". Without blur, glass showed the windows behind it sharp through the work area and sidebar. The desktop look of glass is turned off while the desktop won't blur, and Settings > Interface > Transparency says why and how (`status_note`); a picture, video or Live backdrop is Ghostex's own and stays. Turning Glassy Windows on or off applies at once (`watch_linux_desktop_blur`).
#[cfg(all(target_os = "linux", not(ghostex_web)))]
fn desktop_withholds_blur() -> bool {
    if let Some(blurs) = crate::linux_hyprland::blurs_windows() {
        return !blurs;
    }
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("GNOME"))
    })
}

#[cfg(not(all(target_os = "linux", not(ghostex_web))))]
fn desktop_withholds_blur() -> bool {
    false
}

/// Re-applies the glass each time Hyprland's blur is turned on or off, so Omarchy's Glassy Windows
/// takes effect without a restart (`desktop_withholds_blur`).
#[cfg(all(target_os = "linux", not(ghostex_web)))]
pub(crate) fn watch_linux_desktop_blur(cx: &mut Context<GhostexGpuiApp>) {
    use futures::StreamExt as _;
    let mut changes = crate::linux_hyprland::watch_blur();
    cx.spawn(async move |this, cx| {
        while changes.next().await.is_some() {
            let refreshed = this.update(cx, |app, cx| {
                let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                if refresh_window_glass(settings.object()) {
                    app.refresh_workarea_page_themes(&settings, cx);
                    cx.notify();
                    app.update_other_workspace_windows(cx, |app, cx| {
                        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                        app.refresh_workarea_page_themes(&settings, cx);
                        cx.notify();
                    });
                }
            });
            if refreshed.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// The background a window that follows the glass live (every workspace window, the Project
/// board's own window) opens with. Its first render gives it the background the settings ask for
/// (`apply_workspace_window_glass`), and every later change switches it in place.
///
/// CDXC:Theming 2026-11-02 DECISION:
/// User: "when i apply transparency for ghostex on windows it doesn't apply until i restart the app. please fix." On Windows these windows always open on DirectComposition, so turning transparency on, off or between its choices applies to every open window at once, as on macOS and Linux. Supersedes the 2026-09-25 rule that a Windows main window opened opaque stayed opaque until the next launch.
///
/// CDXC:Theming 2026-11-02 WHY:
/// A Windows window's compositing path is fixed when it is created: GPUI opens an opaque window on the redirection surface (`GPUI_DISABLE_DIRECT_COMPOSITION` in main.rs, for windowed CEF children), whose swap chain ignores alpha and which `WS_EX_NOREDIRECTIONBITMAP` can never be added to later, so glass drawn there was solid. Asking for Transparent at creation keeps the window on DirectComposition, where the DWM backdrop, the blur and the per-pixel alpha all change live; an opaque background there is just an opaque fill. Windowed CEF pages stay visible on it because `ensure_child_composited_by_dwm` in src/cef/windows.rs makes them layered, the path glass windows have used since 2026-09-25.
pub(crate) fn live_glass_window_creation_background() -> WindowBackgroundAppearance {
    creation_background_for(
        window_glass_background_appearance(),
        cfg!(target_os = "windows"),
    )
}

/// `live_glass_window_creation_background` for a given wanted background and platform.
fn creation_background_for(
    wanted: WindowBackgroundAppearance,
    windows: bool,
) -> WindowBackgroundAppearance {
    if windows && wanted == WindowBackgroundAppearance::Opaque {
        WindowBackgroundAppearance::Transparent
    } else {
        wanted
    }
}

pub(crate) fn window_glass_active() -> bool {
    WINDOW_GLASS_ACTIVE.load(Ordering::Relaxed)
}

pub(crate) fn window_glass_active_in(window: &Window) -> bool {
    window_glass_active_for(Some(window.window_handle()))
}

/// `window_glass_active_in` for code that has only the handle a view recorded at its last render.
pub(crate) fn window_glass_active_for(window: Option<gpui::AnyWindowHandle>) -> bool {
    window_glass_active()
        && window.is_some_and(|window| {
            let id = window.window_id().as_u64();
            id == MAIN_WINDOW_ID.load(Ordering::Relaxed)
                // Every workspace window takes glass, not only the one that drew last
                // (app/workspace_windows/).
                || crate::app::workspace_windows::is_workspace_window(window.window_id())
                || id == FLOATING_REVEAL_WINDOW_ID.load(Ordering::Relaxed)
                || id == DOCS_DRAWER_WINDOW_ID.load(Ordering::Relaxed)
                || detached_board_glass_window(id)
        })
}

/// The Project board's own windows (app/native_kanban/window.rs), one per workspace window at
/// most, which draw the board on glass as the panel does.
static DETACHED_BOARD_WINDOW_IDS: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

/// Adds `window` to the board windows that take glass, or removes it once the window closes.
pub(crate) fn set_detached_board_glass_window(window: gpui::WindowId, glass: bool) {
    let id = window.as_u64();
    if let Ok(mut ids) = DETACHED_BOARD_WINDOW_IDS.lock() {
        ids.retain(|known| *known != id);
        if glass {
            ids.push(id);
        }
    }
    if !glass {
        // The closed window's last glass record would otherwise stay in the map for good.
        if let Ok(mut applied) = APPLIED_WINDOW_GLASS.lock()
            && let Some(applied) = applied.as_mut()
        {
            applied.remove(&id);
        }
    }
}

/// Whether the window with `id` is one of the board's own windows.
fn detached_board_glass_window(id: u64) -> bool {
    DETACHED_BOARD_WINDOW_IDS
        .lock()
        .is_ok_and(|ids| ids.contains(&id))
}

/// The Docs view's floating files list, which draws in a window of its own the same way.
static DOCS_DRAWER_WINDOW_ID: AtomicU64 = AtomicU64::new(u64::MAX);

pub(crate) fn set_docs_drawer_glass_window(window: Option<gpui::AnyWindowHandle>) {
    DOCS_DRAWER_WINDOW_ID.store(
        window.map_or(u64::MAX, |window| window.window_id().as_u64()),
        Ordering::Relaxed,
    );
}

/// The floating reveal panel draws the main window's sidebar and sessions column in a blurred
/// window of its own, so its surfaces take glass exactly as they do docked.
static FLOATING_REVEAL_WINDOW_ID: AtomicU64 = AtomicU64::new(u64::MAX);

pub(crate) fn set_floating_reveal_glass_window(window: Option<gpui::AnyWindowHandle>) {
    FLOATING_REVEAL_WINDOW_ID.store(
        window.map_or(u64::MAX, |window| window.window_id().as_u64()),
        Ordering::Relaxed,
    );
}

/// Puts the backdrop of a window laid over part of the main window (the floating reveal panel) in
/// line with the main window's glass. `main_origin` is where the main window's content sits in this
/// window's coordinates.
///
/// CDXC:Sidebar 2026-09-23 WHY:
/// A blurred window blurs everything behind it on screen, and behind the floating panel is the main window itself: its tint (or an opaque web page when the view panel is maximised) was blurred and then tinted a second time, so the floating sidebar and sessions column came out as a near-solid slab instead of the glass they have docked. The panel therefore never uses the live blur. It shows the blurred picture the main window's glass is made of, placed exactly where the main window has it: the custom image or the desktop picture held against the screen, or the same picture covering the main window's rectangle when that picture moves with the window. With Glass shows set to Desktop and windows (or Custom image with no picture for this appearance) it shows the desktop picture held against the screen, which is what the main window's glass shows wherever no other window is behind it. A desktop picture that cannot be read leaves the live blur.
pub(crate) fn sync_overlay_window_glass(window: &Window, main_origin: gpui::Point<gpui::Pixels>) {
    if !window_glass_active() {
        window.set_background_wallpaper(false);
        return;
    }
    let image = window_glass_custom_image();
    let (video, only_on_power) = window_glass_video();
    let live = window_glass_live::window_glass_live();
    let main_uses_picture = window_glass_uses_backdrop(&image, &video, &live);
    // A live style is laid out over the main window, so the panel draws the same part of it.
    let main_uses_live = main_uses_picture && live.is_some();
    let follows_screen = !main_uses_picture
        || (!main_uses_live && WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.load(Ordering::Relaxed));
    // The panel plays the main window's own player and live clock, so both show the same frame.
    window.set_background_blur_style(window_glass_blur_radius(), false);
    window.set_background_live(if main_uses_picture { live } else { None });
    window.set_background_video(if main_uses_picture { video } else { None }, only_on_power);
    window.set_background_wallpaper_image(if main_uses_picture { image } else { None });
    window.set_background_wallpaper_follows_screen(follows_screen);
    window.set_background_wallpaper_cover(
        (!follows_screen)
            .then(|| MAIN_WINDOW_SIZE.lock().ok().and_then(|size| *size))
            .flatten()
            .map(|size| gpui::Bounds::new(main_origin, size)),
    );
    window.set_background_wallpaper(true);
}

/// The main window's size as of its last root render, for windows laid over it that show the same
/// part of its picture.
static MAIN_WINDOW_SIZE: std::sync::Mutex<Option<gpui::Size<gpui::Pixels>>> =
    std::sync::Mutex::new(None);

/// Whether the main window's glass picture stays still against the screen
/// (`windowGlassImagePlacement` "desktop") rather than covering the window.
pub(crate) fn window_glass_picture_follows_screen() -> bool {
    WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.load(Ordering::Relaxed)
}

/// Whether Glass shows is Desktop and windows: the live blur of what is behind the window, which
/// stays still against the screen while the window moves, whatever the picture placement says.
pub(crate) fn window_glass_shows_live_blur() -> bool {
    !WINDOW_GLASS_WALLPAPER.load(Ordering::Relaxed)
}

/// The main window, whose glass the app's other surfaces match.
pub(crate) fn main_window_handle(cx: &gpui::App) -> Option<gpui::AnyWindowHandle> {
    let id = MAIN_WINDOW_ID.load(Ordering::Relaxed);
    cx.windows()
        .into_iter()
        .find(|window| window.window_id().as_u64() == id)
}

pub(crate) fn window_glass_background_appearance() -> WindowBackgroundAppearance {
    if window_glass_active() {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

impl GhostexGpuiApp {
    /// Puts the main window's compositing in line with the resolved glass state. Called from the
    /// root render, which is where settings saves, system appearance changes and Reduce
    /// Transparency all land. Terminals carry their background alpha in their own config, so a
    /// change reloads it after this frame.
    pub(crate) fn sync_main_window_glass(&mut self, window: &Window, cx: &mut Context<Self>) {
        MAIN_WINDOW_ID.store(
            window.window_handle().window_id().as_u64(),
            Ordering::Relaxed,
        );
        // Windows laid over a workspace window are laid over the active one, so with several
        // windows open its size is the one they take (app/workspace_windows/).
        if let Ok(mut size) = MAIN_WINDOW_SIZE.lock()
            && (window.is_window_active() || size.is_none())
        {
            *size = Some(window.viewport_size());
        }
        let Some((previous, code)) = apply_workspace_window_glass(window) else {
            return;
        };
        // Switching between the two blurs leaves the glass on, so terminals keep their config.
        if previous != 0 && (previous == 1) != (code == 1) {
            cx.defer_in(window, |this, _window, cx| {
                this.reload_live_gpui_engine_terminal_config(cx);
            });
        }
    }
}

/// Gives `window` the backdrop a workspace window has (opaque, the live blur or the picture, as
/// the settings say) when it differs from what the window last took. The previous and the new
/// blur code (1 opaque, 2 live blur, 3 picture; 0 for a window that had none), `None` when nothing
/// changed.
fn apply_workspace_window_glass(window: &Window) -> Option<(u8, u8)> {
    let wanted = window_glass_background_appearance();
    let image = window_glass_custom_image();
    let video = window_glass_video();
    let live = window_glass_live::window_glass_live();
    // Custom image or Live with nothing chosen for this appearance is the live blur.
    let backdrop = window_glass_uses_backdrop(&image, &video.0, &live);
    // In native fullscreen the live blur has only black behind it: show the desktop picture.
    let fullscreen_desktop = !backdrop
        && window_glass_fullscreen_shows_desktop(
            window,
            wanted == WindowBackgroundAppearance::Blurred,
        );
    let wallpaper = backdrop || fullscreen_desktop;
    // The desktop picture stands where macOS draws it on the screen, which a fullscreen window
    // below a camera housing does not fill.
    window.set_background_wallpaper_follows_screen(
        fullscreen_desktop || WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.load(Ordering::Relaxed),
    );
    let code = match (wanted == WindowBackgroundAppearance::Blurred, wallpaper) {
        (false, _) => 1,
        (true, false) => 2,
        (true, true) => 3,
    };
    let wanted_glass = AppliedWindowGlass {
        code,
        fullscreen_desktop,
        blur: WINDOW_GLASS_BLUR_RADIUS.load(Ordering::Relaxed),
        behind_blur: window_glass_behind_window_blur(),
        image: image.clone(),
        video: video.clone(),
        live: live.clone(),
    };
    let previous_glass = APPLIED_WINDOW_GLASS.lock().ok().and_then(|mut applied| {
        applied.get_or_insert_with(Default::default).insert(
            window.window_handle().window_id().as_u64(),
            wanted_glass.clone(),
        )
    });
    if previous_glass.as_ref() == Some(&wanted_glass) {
        return None;
    }
    let previous = previous_glass.map_or(0, |glass| glass.code);
    window.set_background_blur_style(window_glass_blur_radius(), false);
    window.set_background_behind_window_blur(wanted_glass.behind_blur);
    window.set_background_live(if fullscreen_desktop { None } else { live });
    window.set_background_video(if fullscreen_desktop { None } else { video.0 }, video.1);
    window.set_background_wallpaper_image(if fullscreen_desktop { None } else { image });
    window.set_background_wallpaper(wallpaper && wanted == WindowBackgroundAppearance::Blurred);
    window.set_background_appearance(wanted);
    Some((previous, code))
}

/// The Project board's own window (app/native_kanban/window.rs) takes the workspace windows'
/// backdrop, so the board looks there as it does in the panel. Called from its every render;
/// nothing is applied while the settings are unchanged.
pub(crate) fn sync_detached_board_window_glass(window: &Window) {
    let _ = apply_workspace_window_glass(window);
}

/// CDXC:Theming 2026-09-25 DECISION:
/// User: a single frosted menu was "not looking glassy at all", then "for the context menus and menus, we need them to be more transparent by default. Right now, the settings you have, they don't look transparent still." Every frosted menu and tooltip window uses one recipe: a 20px blur that keeps the backdrop's colour saturation (`FROSTED_MENU_BLUR_RADIUS`, `FROSTED_MENU_KEEP_SATURATION`, applied by `apply_frosted_menu_blur`; the main window's glass keeps its 60px, desaturated blur), and a fill of the theme's menu colour lifted a little toward white in dark mode covering 32% in dark mode and 60% in light mode (`frosted_menu_alpha`), so shapes and colours behind a menu read through it. Supersedes the same day's 50% fill over the main window's blur. The dark-mode coverage is 45% since 2026-10-29 (CDXC:AppModal on `frosted_dialog_fill`). Superseded on 2026-10-29 on macOS by the dropdowns' fill: no lift toward white and 60% dark / 75% light, see the Theming 2026-10-29 decision below.
///
/// CDXC:Theming 2026-10-10 DECISION:
/// User, on Windows, of the sidebar's ☰ menu next to the Notifications panel: "this menu is gray right now and this one is bluish please make them all consistent with the bluish one so all menus are same". On Windows every frosted menu, dropdown, context menu and tooltip takes the Notifications panel's surface: the theme colour, not lifted toward white, covering `frosted_modal_alpha` of the blur (`frosted_lift`, `frosted_menu_alpha`). Supersedes the thinner menu fill above on Windows; macOS keeps it (until the 2026-10-29 decision below).
///
/// CDXC:Theming 2026-10-10 WHY:
/// A blurred window on Windows 11 is backed by the system acrylic (`DWMSBT_TRANSIENTWINDOW`, chosen for slide performance), which adds its own grey luminosity layer; a 32% fill lifted toward white let that grey win, while the panels' 86% fill kept the theme's tint.
///
/// CDXC:Theming 2026-10-01 DECISION:
/// User, on Linux (Hyprland): "the ask ghostex dropdown is way too transparent", "the dropdown that has Ask Ghostex and Tips & Tricks needs to be same bg look as the one that's top of the sidebar" and "all dropdowns etc need to have better bgs please in the linux app ... on macos and windows they're perfect". On Linux every menu, dropdown and tooltip in a window of its own takes the sidebar menus' fill exactly (`titlebar_popup_menu_background`, opaque and not lifted toward white), because Linux draws the sidebar's menus inside the main window with that fill (`frosted_hosting_active`). macOS and Windows keep the lifted frost. Superseded on 2026-10-29 on macOS: the lift is gone (see below).
///
/// CDXC:Theming 2026-10-29 DECISION:
/// User, of the titlebar's dropdown panels (Notifications, Tips & Tricks, Resources, Dev servers) looking darker than the menus next to them (the sidebar's ☰ menu, the ⋯ menus, context menus), asked which way they should match, picked "Menus take the dropdowns' fill: every menu gets the darker 60% frost of Notifications and the large panels" (Q751). On macOS every menu is the large panels' fill: the theme colour, not lifted toward white, covering 60% of the blur in dark mode and 75% in light mode (`FROSTED_PANEL_ALPHA_DARK` / `_LIGHT`). Everything that shared the menus' fill follows: tooltips (`frosted_tooltip_fill`), the popovers and the form dialogs (`frosted_dialog_fill`) and the header's dropdown menus (`popup_window_surface`), so every frosted surface on macOS is one material. Supersedes the lifted 45% dark / 60% light menu fill (32% before 2026-10-29). Windows (86%, unlifted) and Linux (opaque) are unchanged, and so is the blur.
pub(crate) fn frosted_menu_fill(color: Hsla) -> Hsla {
    frosted_panel_fill(color)
}

/// CDXC:Tooltips 2026-09-30 DECISION:
/// User: "make tooltips always have darker color in dark mode pls". Without window glass a dark-mode tooltip takes the menu colour darkened toward black (`TOOLTIP_DARKEN_DARK`), the solid dark surface; light mode keeps the menus' colour.
///
/// CDXC:Tooltips 2026-10-15 DECISION:
/// User picked "mini menu + frosted glass (when transparency enabled)": under window glass a tooltip is the frosted menus' material (`frosted_tooltip_fill`, the same fill and blur as the frosted menus), replacing the 2026-09-30 "even if transparency enabled they should have dimmed bg" (a darkened fill covering 90% of the blur in dark mode).
pub(crate) fn tooltip_background(menu: Hsla) -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        menu
    } else {
        menu.blend(gpui::black().opacity(TOOLTIP_DARKEN_DARK))
    }
}

/// How far a dark-mode tooltip's colour moves from the menu colour toward black.
const TOOLTIP_DARKEN_DARK: f32 = 0.55;

/// How much of a frosted tooltip its fill covers: the frosted menus' coverage.
pub(crate) fn frosted_tooltip_alpha() -> f32 {
    frosted_menu_alpha()
}

/// The fill of a frosted tooltip under glass: the frosted menus' fill (CDXC:Tooltips 2026-10-15).
pub(crate) fn frosted_tooltip_fill(menu: Hsla) -> Hsla {
    frosted_menu_fill(menu)
}

/// How much of a frosted menu or tooltip its fill covers: the large panels' coverage
/// (`frosted_panel_alpha`), so every frosted surface is one material (CDXC:Theming 2026-10-29 on
/// `frosted_menu_fill`).
pub(crate) fn frosted_menu_alpha() -> f32 {
    frosted_panel_alpha()
}

/// Default blur radius of a frosted menu or tooltip window (`windowGlassMenuBlurRadius`); every
/// frosted surface but a tooltip blurs `FROSTED_SURFACE_BLUR_SCALE` times it.
/// Narrower than the main window's glass so the shapes and colours behind a menu still read
/// through it.
pub(crate) const FROSTED_MENU_BLUR_RADIUS: f32 = 20.0;

/// Whether a frosted menu's backdrop keeps the colour saturation the main window's glass strips.
pub(crate) const FROSTED_MENU_KEEP_SATURATION: bool = true;

/// Gives a menu or tooltip window the frosted menus' blur (`windowGlassMenuBlurRadius`,
/// `FROSTED_MENU_KEEP_SATURATION`). Call it where the window sets its corner radius.
///
/// CDXC:Theming 2026-10-15 WHY:
/// The user's "moving the blur slider does nothing": a window that stays open (Settings and every app modal, the reused frosted menu and tooltip hosts) took the Menu blur only when it opened, so those call this from their render on macOS, where an unchanged radius costs nothing. Windows and Linux have no menu blur radius, and Windows refuses a backdrop change during a draw.
///
/// Every frosted surface but a tooltip (the dialogs, the large panels, the popovers and the
/// dropdowns and menus) blurs `FROSTED_SURFACE_BLUR_SCALE` times the Menu blur value
/// (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`); tooltips take `apply_frosted_tooltip_blur`.
pub(crate) fn apply_frosted_menu_blur(window: &gpui::Window) {
    window.set_background_blur_style(
        gpui::px(frosted_menu_blur_value() * FROSTED_SURFACE_BLUR_SCALE),
        FROSTED_MENU_KEEP_SATURATION,
    );
}

/// `apply_frosted_menu_blur` for a tooltip bubble's window: the Menu blur value itself.
pub(crate) fn apply_frosted_tooltip_blur(window: &gpui::Window) {
    window.set_background_blur_style(
        gpui::px(frosted_menu_blur_value()),
        FROSTED_MENU_KEEP_SATURATION,
    );
}

/// The Menu blur slider's value (`windowGlassMenuBlurRadius`), in points.
fn frosted_menu_blur_value() -> f32 {
    f32::from(WINDOW_GLASS_MENU_BLUR_RADIUS.load(Ordering::Relaxed))
}

/// How much more than the Menu blur value a frosted surface other than a tooltip blurs: 25% more,
/// so the default 20 points become 25 (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`).
pub(crate) const FROSTED_SURFACE_BLUR_SCALE: f32 = 1.25;

/// The fill of a menu or panel that has a window of its own (the header's dropdowns): the frosted
/// menu fill under glass, where that window blurs whatever is behind it.
pub(crate) fn popup_window_surface(color: Hsla) -> Hsla {
    if window_glass_active() {
        frosted_menu_fill(color)
    } else {
        color
    }
}

/// CDXC:AppModal 2026-09-30 DECISION:
/// User, on Linux: "the open a project modal is too transparent by default. please fix. same for the cmd + n modal and probably others. please fix all those to be less transparent (more like the settings modal). same for the notification modal, it's way too light by default on linux right now", then "the settings and the find by prompt and add a project are all very transparent now please fix" and "the quick access modal is very transparent on hyper land by default". Every dialog that is frosted under window glass (the native app modals such as Add Project and Settings, the New Thread picker, Quick Access and Browser History, Search by Prompt, and the titlebar's Notifications, Tips, Resources and Dev servers panels) takes this fill, which is fully opaque on Linux; macOS and Windows keep the menus' coverage (`frosted_menu_alpha`). Then, of the chat's model picker: "on linux also the default transparency needs to be lower on the modals that are shown (like the model picker modal)", so on Linux menus and tooltips are opaque too (`FROSTED_ALPHA_LINUX`); macOS and Windows keep the thinner menu fill. Supersedes the same day's "menus and tooltips keep the thinner menu fill everywhere".
///
/// CDXC:AppModal 2026-09-30 WHY:
/// A Linux window only asks the compositor for blur (`_KDE_NET_WM_BLUR_BEHIND_REGION` on X11, the blur protocol on Wayland); many compositors ignore the request or ship with blur off (Hyprland on Omarchy does both), so any see-through dialog showed the desktop and the windows behind it unblurred and its text could not be read.
///
/// CDXC:AppModal 2026-09-30 DECISION:
/// User: "Please make the settings modal and find with search and other modals that appear on the app in the middle less transparent (darker bg), they're too transparent now." On macOS and Windows the dialogs no longer share the menus' coverage: they cover most of what is behind them (`FROSTED_MODAL_ALPHA_DARK` / `_LIGHT`) and are not lifted toward white in dark mode (`frosted_modal_fill`). Supersedes "macOS and Windows keep the menus' coverage" above; menus and tooltips are unchanged. Superseded on 2026-10-29 for the form dialogs and for Settings, Search by Prompt, the Agents Hub, Quick Access and the New Thread picker (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`); only the Mermaid diagram, Markdown table and Visual page viewers keep this fill.
pub(crate) fn frosted_modal_alpha() -> f32 {
    if cfg!(target_os = "linux") {
        FROSTED_ALPHA_LINUX
    } else if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        FROSTED_MODAL_ALPHA_LIGHT
    } else {
        FROSTED_MODAL_ALPHA_DARK
    }
}

/// Fill coverage of a frosted dialog on macOS and Windows; see `frosted_modal_alpha`.
const FROSTED_MODAL_ALPHA_DARK: f32 = 0.86;
const FROSTED_MODAL_ALPHA_LIGHT: f32 = 0.9;

/// Fill coverage of every frosted dialog, menu and tooltip on Linux; see `frosted_modal_alpha`.
const FROSTED_ALPHA_LINUX: f32 = 1.0;

/// The fill of a frosted dialog under glass: the frosted menu colour at `frosted_modal_alpha`.
pub(crate) fn frosted_modal_fill(color: Hsla) -> Hsla {
    color.opacity(frosted_modal_alpha())
}

/// `popup_window_surface` for a dropdown panel rather than a menu (the titlebar's Notifications,
/// Tips, Resources and Dev servers panels): the large panels' darker frost (`frosted_panel_fill`).
pub(crate) fn popup_window_panel_surface(color: Hsla) -> Hsla {
    if window_glass_active() {
        frosted_panel_fill(color)
    } else {
        color
    }
}

/// CDXC:AppModal 2026-10-29 DECISION:
/// User, of the Send Bug / Feedback dialog drawn as a near-black box over the glass window, next to the Accounts & limits popover: "please make these kinds of pop ups not so black when transparency is enabled it looks weird, make them transparent but with frosted bg like this one". Under window glass the form dialogs (Send Bug / Feedback, Add Project, Rename Session and Rename Worktree, Add and Delete Worktree, New and Make Coordinator, Agent Hooks, Session note, Delayed send and the other small dialogs opened through `gpui_native_dialog_palette`) take the popover's fill exactly: `frosted_menu_fill` over the frosted menus' blur in their own window, so the content behind them reads blurred through a thin tint (32% dark at first, 45% now, see below; 60% light on macOS; the menus' coverage on Windows; opaque on Linux, where a compositor may not blur). Their fields and section panels get a stronger ink wash (`ModalPalette::frosted_dialog`, `frosted_dialog_washes`) so they stay clear on the thinner glass. Asked whether the large panels should follow, the user answered "Frost them too": Settings and its pages, Search by Prompt, the Agents Hub, Quick Access (and Browser History, which shares its palette) and the New Thread picker take the same fill and the same stronger washes (`frosted_dialog_washes`) for their lists, fields and rows. Supersedes the 2026-09-30 "less transparent (darker bg)" fill (`frosted_modal_fill`) for all of these; the Mermaid diagram, Markdown table and Visual page viewers keep it. Then, of those large panels: "please make the glass in the settings and other similar windows that we made transparent+frosted show darker bg pls". The large panels stay frosted but take a darker tint (`frosted_panel_fill`): the theme colour, not lifted toward white, covering `FROSTED_PANEL_ALPHA_DARK` (60%) of the blur in dark mode and `FROSTED_PANEL_ALPHA_LIGHT` (75%) in light mode on macOS; Windows keeps the menus' 86%, already darker, and Linux stays opaque. Their washes are unchanged. The form dialogs keep the popover's fill. Then, of the bell's Notifications dropdown: "please for dropdowns like this one we need to do same as what we did for settings bg ... this dropdown is too black now": the titlebar's dropdown panels (Notifications, Tips, Resources, Dev servers) take the large panels' fill (`popup_window_panel_surface`) and the dialogs' washes on their cards and hairlines, instead of the 2026-09-30 near-solid fill. And: "we need to make all of those frosted backgrounds we did in settings and other backgrounds have 25% more blur (same for the dropdowns too)": every frosted surface but a tooltip (the dialogs, the large panels, the popovers, the dropdowns and menus) blurs `FROSTED_SURFACE_BLUR_SCALE` (1.25) times the Menu blur value, 25 points at the default 20, so the user's own Menu blur choice still scales it; tooltips keep the Menu blur value. Asked whether tooltips should blur more too, the user answered "No, keep tooltips as they are", and Settings and Help say that menus, panels and dialogs blur a quarter more than the Menu blur number while tooltips use it as is. Asked whether the terminal bar's account and skills flyouts should become frosted windows too, the user answered "Yes, as a separate task": they draw in a frosted host of their own with the popover's fill and this blur, and, approved next, so does the ⋯ menu itself, so the menu and its flyouts match (CDXC:AppModal 2026-10-29 in render/terminal_agent_action_bar/flyout_host.rs). Then, shown the browser panel's ⋯ menu, Browser History and Settings washing out over a white page and offered darker tints, macOS vibrancy or solid fills, the user answered "let's try 45% tint": in dark mode on macOS the popover fill the menus, dropdowns, popovers and form dialogs share covers 45% of the blur (`WINDOW_GLASS_MENU_ALPHA`) instead of 32%; the large panels stay at 60% for now, and light mode (60%), Windows (86%) and Linux (opaque) are unchanged. Glass off, nothing changes. Superseded on 2026-10-29 for the fill's numbers: the popover fill is now the large panels' (no lift, 60% dark / 75% light on macOS), so the 45% above and the "large panels stay at 60% for now" are no longer a difference (CDXC:Theming 2026-10-29 on `frosted_menu_fill`).
pub(crate) fn frosted_dialog_fill(color: Hsla) -> Hsla {
    frosted_menu_fill(color)
}

/// How much of the blur a frosted surface's tint covers on macOS in dark mode: 60%, darker than the
/// popover's 45% (32% when this was set) so the backdrop shows through less, still well short of the
/// 2026-09-30 near-solid 86% (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`). Since the same day
/// the menus, popovers, form dialogs and tooltips take it too (CDXC:Theming 2026-10-29 on
/// `frosted_menu_fill`; their 45% is superseded).
pub(crate) const FROSTED_PANEL_ALPHA_DARK: f32 = 0.6;

/// `FROSTED_PANEL_ALPHA_DARK` in light mode (the popover's 60% before 2026-10-29).
pub(crate) const FROSTED_PANEL_ALPHA_LIGHT: f32 = 0.75;

/// How much of the blur a frosted surface's tint covers, the one source for the large panels, the
/// dropdowns, the menus, the popovers, the form dialogs and the tooltips: `FROSTED_PANEL_ALPHA_DARK`
/// / `_LIGHT` on macOS, `frosted_modal_alpha` (86% dark) on Windows, opaque on Linux.
pub(crate) fn frosted_panel_alpha() -> f32 {
    if cfg!(target_os = "linux") {
        FROSTED_ALPHA_LINUX
    } else if cfg!(target_os = "windows") {
        frosted_modal_alpha()
    } else if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        FROSTED_PANEL_ALPHA_LIGHT
    } else {
        FROSTED_PANEL_ALPHA_DARK
    }
}

/// The fill of a large frosted panel under glass: the theme colour, not lifted toward white, at
/// `frosted_panel_alpha` (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`).
pub(crate) fn frosted_panel_fill(color: Hsla) -> Hsla {
    color.opacity(frosted_panel_alpha())
}

/// The window's outermost fill. Under glass it is left clear: the sidebar and the work area each
/// tint the blurred desktop themselves (`sidebar_glass_tint`, `workspace_column_background`).
pub(crate) fn window_shell_background() -> Hsla {
    if window_glass_active() {
        gpui::transparent_black()
    } else {
        workspace_background_color()
    }
}

/// The sidebar's own tint under glass: its chrome colour over the blurred desktop, painted by the
/// sidebar and by the chrome that belongs to its column (its resize divider, the floating panel's
/// rail).
pub(crate) fn sidebar_glass_tint() -> Hsla {
    let alpha = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        load_glass_alpha(&SIDEBAR_GLASS_PERCENT_LIGHT)
    } else {
        load_glass_alpha(&SIDEBAR_GLASS_PERCENT_DARK)
    };
    titlebar_background().opacity(alpha)
}

/// The workspace column's fill: under glass its own tint straight over the blurred desktop.
pub(crate) fn workspace_column_background() -> Hsla {
    let workspace = workspace_background_color();
    if window_glass_active() {
        workspace.opacity(work_area_glass_alpha())
    } else {
        workspace
    }
}

/// How much of the blurred desktop the work area's tint covers in the current appearance
/// (`windowGlassWorkAreaTintDark` / `Light`).
pub(crate) fn work_area_glass_alpha() -> f32 {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        load_glass_alpha(&WORK_AREA_GLASS_PERCENT_LIGHT)
    } else {
        load_glass_alpha(&WORK_AREA_GLASS_PERCENT_DARK)
    }
}

/// Fills for surfaces inside the workspace column. Under glass the column has already tinted the
/// area, so another layer of the same colour would only make it opaque again.
pub(crate) fn workspace_nested_background() -> Hsla {
    if window_glass_active() {
        gpui::transparent_black()
    } else {
        workspace_background_color()
    }
}

/// A pane-level fill inside the workspace column (a chat host, a pane's base, the command pane's
/// chrome). Under glass it is left out so the column's tint is the only layer over the desktop.
pub(crate) fn glass_clear(color: Hsla) -> Hsla {
    if window_glass_active() {
        gpui::transparent_black()
    } else {
        color
    }
}

/// A card that sits on a page's glass (a placeholder's card, badge or button), under window glass:
/// the same flat ink wash as the paused view card (`view_card_frame`), so it reads as a lighter
/// pane on the glass instead of a solid slab. `strength` scales the wash for raised controls on the
/// card. The opaque window keeps the given colour.
pub(crate) fn glass_card(color: Hsla, strength: f32) -> Hsla {
    if window_glass_active() {
        let base = if chrome_uses_light_appearance() {
            0.04
        } else {
            0.06
        };
        Hsla::from(chrome_ink()).opacity(base * strength)
    } else {
        color
    }
}

/// CDXC:Theming 2026-09-23 DECISION:
/// User picked "23a": under window glass the divider lines stay, but as faint see-through 1px lines like the chat composer's border (the chrome ink at 8%), instead of solid dark or light grey lines that read as grooves cut into the glass. Pane borders, resize rails, the sidebar edge and the command pane's edges all take it; focus and attention outlines and the resize hover highlight keep their colours. The opaque window keeps the given colour.
/// The Dividers theme role replaces both when it is set (helpers/theme_roles.rs).
pub(crate) fn glass_divider(color: Hsla) -> Hsla {
    let derived = if window_glass_active() {
        Hsla::from(chrome_ink()).opacity(0.08)
    } else {
        color
    };
    theme_role_or("divider", chrome_uses_light_appearance(), derived)
}

/// The body row behind the sidebar, its divider and the workspace column. Opaque, it is the
/// divider's colour showing between them; under glass each of them tints itself.
pub(crate) fn window_body_row_background() -> Hsla {
    if window_glass_active() {
        gpui::transparent_black()
    } else {
        sidebar_divider_background_color()
    }
}

/// The sidebar's own fill: its chrome gradient, or its glass tint under glass.
pub(crate) fn sidebar_chrome_fill(glass: bool, angle: f32) -> gpui::Background {
    if glass {
        sidebar_glass_tint().into()
    } else {
        sidebar_chrome_gradient_fill(angle)
    }
}

/// How much of a terminal's default background it paints itself. Under glass the column tint shows
/// through instead; cells with an explicit background colour still paint it in full.
pub(crate) fn terminal_default_background_alpha() -> f32 {
    if window_glass_active() { 0.0 } else { 1.0 }
}

#[cfg(test)]
mod frosted_dialog_tests {
    use super::*;

    #[test]
    fn frosted_surfaces_blur_a_quarter_more_than_the_menu_blur() {
        assert_eq!(FROSTED_MENU_BLUR_RADIUS * FROSTED_SURFACE_BLUR_SCALE, 25.0);
    }

    /// Every frosted surface is one material (CDXC:Theming 2026-10-29 on `frosted_menu_fill`): the
    /// menus, tooltips, popovers, form dialogs and dropdowns take the large panels' fill.
    #[test]
    fn every_frosted_surface_takes_the_panel_fill() {
        let chrome: Hsla = gpui::rgb(0x0e0e0e).into();
        let panel = frosted_panel_fill(chrome);
        assert_eq!(frosted_menu_fill(chrome), panel);
        assert_eq!(frosted_dialog_fill(chrome), panel);
        assert_eq!(frosted_tooltip_fill(chrome), panel);
        assert_eq!(frosted_menu_alpha(), frosted_panel_alpha());
        // Not lifted toward white, and never more see-through than the near-solid dialog era.
        assert_eq!(panel.l, chrome.l);
        assert!(panel.a <= frosted_modal_fill(chrome).a);
        if cfg!(target_os = "macos") {
            let expected = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
                FROSTED_PANEL_ALPHA_LIGHT
            } else {
                FROSTED_PANEL_ALPHA_DARK
            };
            assert_eq!(panel.a, expected);
        }
    }
}

#[cfg(test)]
mod live_glass_creation_tests {
    use super::*;

    /// On Windows an opaque window opens on DirectComposition (as Transparent) so glass can be
    /// turned on later without a restart (CDXC:Theming 2026-11-02 on
    /// `live_glass_window_creation_background`); a window opened with glass keeps it.
    #[test]
    fn windows_opens_opaque_windows_on_direct_composition() {
        assert_eq!(
            creation_background_for(WindowBackgroundAppearance::Opaque, true),
            WindowBackgroundAppearance::Transparent
        );
        assert_eq!(
            creation_background_for(WindowBackgroundAppearance::Blurred, true),
            WindowBackgroundAppearance::Blurred
        );
    }

    /// macOS and Linux switch a window's background live whatever it opened with, so they open
    /// with the background the settings ask for.
    #[test]
    fn other_platforms_open_with_the_wanted_background() {
        for wanted in [
            WindowBackgroundAppearance::Opaque,
            WindowBackgroundAppearance::Blurred,
        ] {
            assert_eq!(creation_background_for(wanted, false), wanted);
        }
    }
}
