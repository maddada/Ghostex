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
/// User: "i'm not able to set the transparency blur level in ghostex on macos please add sliders for this (hope they can work on other oses too)". Settings has a Blur slider (`windowGlassBlurRadius`, 0 to 100 points, default 60: the main window's glass as it always was) and a Menu blur slider (`windowGlassMenuBlurRadius`, default 20: `FROSTED_MENU_BLUR_RADIUS`). Blur reaches every GPUI backend through `set_background_blur_style`: on macOS it sets the live blur, the wallpaper and picture blur and the video blur; on Windows and Linux the system draws the Desktop and windows blur and has no radius to set, so there it sets the wallpaper, picture and video blur. Menu blur is macOS only for the same reason. 0 shows what is behind the glass sharp. On 2026-10-04 the user chose "hide the Blur slider on Windows only while 'What shows behind the glass' is 'Desktop and windows'" (it does nothing there), and Menu blur is hidden off macOS (`availability.rs` in the settings catalog).
static WINDOW_GLASS_BLUR_RADIUS: AtomicU8 = AtomicU8::new(WINDOW_GLASS_BLUR_RADIUS_DEFAULT);

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
    blur: u8,
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
/// packages/core-ui/settings-modal/theme-simple-controls.tsx (deleted 2026-10-01)), which keeps the sidebar a little
/// more solid than the work area. SEE-ALSO: the CDXC:Theming 2026-09-25 DECISION on
/// `DEFAULT_WINDOW_GLASS_SIDEBAR_OPACITY_DARK_PERCENT` in packages/shared/ghostex-settings/types.ts (deleted 2026-10-01).
const SIDEBAR_GLASS_ALPHA_DARK: f32 = 0.94;
const SIDEBAR_GLASS_ALPHA_LIGHT: f32 = 0.97;
const WORK_AREA_GLASS_ALPHA_DARK: f32 = 0.91;
const WORK_AREA_GLASS_ALPHA_LIGHT: f32 = 0.93;

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
    let sidebar_light = store_sidebar_glass_percent(
        object,
        "windowGlassSidebarOpacityLight",
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
        "windowGlassWorkAreaTintLight",
        "windowGlassMainOpacityLight",
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
        images.light = image("windowGlassImageLight");
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
        videos.light = video("windowGlassVideoLight");
        videos.only_on_power = object
            .get("windowGlassVideoOnlyOnPower")
            .and_then(serde_json::Value::as_bool)
            != Some(false);
    }
    window_glass_live::refresh_window_glass_live(object);
    let active = cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )) && wanted
        && !system_reduces_transparency();
    WINDOW_GLASS_SYSTEM_BLOCKED.store(
        cfg!(any(target_os = "macos", target_os = "windows")) && system_reduces_transparency(),
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

/// Whether the main window was created on a backend that can show glass. On Windows a window's
/// per-pixel alpha is fixed when it is created (it either presents through DirectComposition or
/// through an opaque redirection surface), so a main window opened opaque stays opaque until the
/// app restarts, and glass turned off later keeps working because an opaque fill needs no alpha.
static MAIN_WINDOW_CAN_SHOW_GLASS: AtomicBool = AtomicBool::new(true);

/// Records the background the main window is being created with. Call right before it opens.
///
/// CDXC:Theming 2026-09-25 WHY:
/// The Windows main window hosts windowed CEF pages as child HWNDs, which is why it normally opens on the opaque redirection surface; only a main window opened with glass uses DirectComposition (`ensure_child_composited_by_dwm` in src/cef/windows.rs keeps its CEF children visible there). Turning glass on while the app runs on Windows would draw the translucent tints over an opaque surface, so it waits for the next launch instead.
pub(crate) fn note_main_window_background(appearance: WindowBackgroundAppearance) {
    MAIN_WINDOW_CAN_SHOW_GLASS.store(
        !cfg!(target_os = "windows") || appearance != WindowBackgroundAppearance::Opaque,
        Ordering::Relaxed,
    );
}

pub(crate) fn window_glass_active() -> bool {
    WINDOW_GLASS_ACTIVE.load(Ordering::Relaxed)
        && MAIN_WINDOW_CAN_SHOW_GLASS.load(Ordering::Relaxed)
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

pub(crate) fn set_detached_board_glass_window(window: gpui::WindowId, glass: bool) {
    let id = window.as_u64();
    if let Ok(mut ids) = DETACHED_BOARD_WINDOW_IDS.lock() {
        ids.retain(|known| *known != id);
        if glass {
            ids.push(id);
        }
    }
}

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
    window.set_background_wallpaper_follows_screen(
        WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.load(Ordering::Relaxed),
    );
    let image = window_glass_custom_image();
    let video = window_glass_video();
    let live = window_glass_live::window_glass_live();
    // Custom image or Live with nothing chosen for this appearance is the live blur.
    let wallpaper = window_glass_uses_backdrop(&image, &video.0, &live);
    let code = match (wanted == WindowBackgroundAppearance::Blurred, wallpaper) {
        (false, _) => 1,
        (true, false) => 2,
        (true, true) => 3,
    };
    let wanted_glass = AppliedWindowGlass {
        code,
        blur: WINDOW_GLASS_BLUR_RADIUS.load(Ordering::Relaxed),
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
    window.set_background_live(live);
    window.set_background_video(video.0, video.1);
    window.set_background_wallpaper_image(image);
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

/// Fill coverage of a menu or popover window under glass; its own window blurs what is behind it.
pub(crate) const WINDOW_GLASS_MENU_ALPHA: f32 = 0.32;

/// `WINDOW_GLASS_MENU_ALPHA` in light mode.
pub(crate) const WINDOW_GLASS_MENU_ALPHA_LIGHT: f32 = 0.6;

/// How much a dark menu colour is lifted toward white under glass, so the frost reads as a light
/// pane over the blur instead of a dark hole.
const WINDOW_GLASS_MENU_LIFT_DARK: f32 = 0.08;

/// CDXC:Theming 2026-09-25 DECISION:
/// User: a single frosted menu was "not looking glassy at all", then "for the context menus and menus, we need them to be more transparent by default. Right now, the settings you have, they don't look transparent still." Every frosted menu and tooltip window uses one recipe: a 20px blur that keeps the backdrop's colour saturation (`FROSTED_MENU_BLUR_RADIUS`, `FROSTED_MENU_KEEP_SATURATION`, applied by `apply_frosted_menu_blur`; the main window's glass keeps its 60px, desaturated blur), and a fill of the theme's menu colour lifted a little toward white in dark mode covering 32% in dark mode and 60% in light mode (`frosted_menu_alpha`), so shapes and colours behind a menu read through it. Supersedes the same day's 50% fill over the main window's blur.
///
/// CDXC:Theming 2026-10-10 DECISION:
/// User, on Windows, of the sidebar's ☰ menu next to the Notifications panel: "this menu is gray right now and this one is bluish please make them all consistent with the bluish one so all menus are same". On Windows every frosted menu, dropdown, context menu and tooltip takes the Notifications panel's surface: the theme colour, not lifted toward white, covering `frosted_modal_alpha` of the blur (`frosted_lift`, `frosted_menu_alpha`). Supersedes the thinner menu fill above on Windows; macOS keeps it.
///
/// CDXC:Theming 2026-10-10 WHY:
/// A blurred window on Windows 11 is backed by the system acrylic (`DWMSBT_TRANSIENTWINDOW`, chosen for slide performance), which adds its own grey luminosity layer; a 32% fill lifted toward white let that grey win, while the panels' 86% fill kept the theme's tint.
pub(crate) fn frosted_menu_fill(color: Hsla) -> Hsla {
    frosted_lift(color).opacity(frosted_menu_alpha())
}

/// The frosted menu colour before its coverage: lifted a little toward white in dark mode.
///
/// CDXC:Theming 2026-10-01 DECISION:
/// User, on Linux (Hyprland): "the ask ghostex dropdown is way too transparent", "the dropdown that has Ask Ghostex and Tips & Tricks needs to be same bg look as the one that's top of the sidebar" and "all dropdowns etc need to have better bgs please in the linux app ... on macos and windows they're perfect". On Linux every menu, dropdown and tooltip in a window of its own takes the sidebar menus' fill exactly (`titlebar_popup_menu_background`, opaque and not lifted toward white), because Linux draws the sidebar's menus inside the main window with that fill (`frosted_hosting_active`). macOS and Windows keep the lifted frost.
fn frosted_lift(color: Hsla) -> Hsla {
    if cfg!(any(target_os = "linux", target_os = "windows"))
        || CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed)
    {
        color
    } else {
        color.blend(gpui::white().opacity(WINDOW_GLASS_MENU_LIFT_DARK))
    }
}

/// CDXC:Tooltips 2026-09-30 DECISION:
/// User: "make tooltips always have darker color in dark mode pls", "even if transparency enabled they should have dimmed bg". In dark mode a tooltip takes the menu colour darkened toward black (`TOOLTIP_DARKEN_DARK`), and under glass that colour covers most of the tooltip's blur (`FROSTED_TOOLTIP_ALPHA_DARK`) without the menus' lift toward white, so a tinted wallpaper no longer shows through as a coloured bubble. Light mode keeps the menus' colour and fill. Supersedes the 2026-09-25 frosted menu fill for dark-mode tooltips.
pub(crate) fn tooltip_background(menu: Hsla) -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        menu
    } else {
        menu.blend(gpui::black().opacity(TOOLTIP_DARKEN_DARK))
    }
}

/// How far a dark-mode tooltip's colour moves from the menu colour toward black.
const TOOLTIP_DARKEN_DARK: f32 = 0.55;

/// Fill coverage of a dark-mode tooltip under glass on macOS and Windows.
const FROSTED_TOOLTIP_ALPHA_DARK: f32 = 0.9;

/// How much of a frosted tooltip its fill covers: the menus' coverage in light mode, most of the
/// blur in dark mode (see `tooltip_background`).
pub(crate) fn frosted_tooltip_alpha() -> f32 {
    if cfg!(target_os = "linux") || CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        frosted_menu_alpha()
    } else {
        FROSTED_TOOLTIP_ALPHA_DARK
    }
}

/// The fill of a frosted tooltip under glass: `tooltip_background` at `frosted_tooltip_alpha`,
/// lifted toward white only in light mode, like the menus.
pub(crate) fn frosted_tooltip_fill(menu: Hsla) -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        frosted_menu_fill(menu)
    } else {
        tooltip_background(menu).opacity(frosted_tooltip_alpha())
    }
}

/// How much of a frosted menu or tooltip its fill covers: little in dark mode, where the menu's
/// light text reads over anything behind it, more in light mode, where dark text needs a lighter
/// backing. Opaque on Linux, like the dialogs (`frosted_modal_alpha`), and the dialogs' coverage on
/// Windows.
pub(crate) fn frosted_menu_alpha() -> f32 {
    if cfg!(target_os = "linux") {
        FROSTED_ALPHA_LINUX
    } else if cfg!(target_os = "windows") {
        frosted_modal_alpha()
    } else if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        WINDOW_GLASS_MENU_ALPHA_LIGHT
    } else {
        WINDOW_GLASS_MENU_ALPHA
    }
}

/// Default blur radius of a frosted menu or tooltip window (`windowGlassMenuBlurRadius`).
/// Narrower than the main window's glass so the shapes and colours behind a menu still read
/// through it.
pub(crate) const FROSTED_MENU_BLUR_RADIUS: f32 = 20.0;

/// Whether a frosted menu's backdrop keeps the colour saturation the main window's glass strips.
pub(crate) const FROSTED_MENU_KEEP_SATURATION: bool = true;

/// Gives a menu or tooltip window the frosted menus' blur (`windowGlassMenuBlurRadius`,
/// `FROSTED_MENU_KEEP_SATURATION`). Call it where the window sets its corner radius.
pub(crate) fn apply_frosted_menu_blur(window: &gpui::Window) {
    window.set_background_blur_style(
        gpui::px(f32::from(
            WINDOW_GLASS_MENU_BLUR_RADIUS.load(Ordering::Relaxed),
        )),
        FROSTED_MENU_KEEP_SATURATION,
    );
}

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
/// User: "Please make the settings modal and find with search and other modals that appear on the app in the middle less transparent (darker bg), they're too transparent now." On macOS and Windows the dialogs no longer share the menus' coverage: they cover most of what is behind them (`FROSTED_MODAL_ALPHA_DARK` / `_LIGHT`) and are not lifted toward white in dark mode (`frosted_modal_fill`). Supersedes "macOS and Windows keep the menus' coverage" above; menus and tooltips are unchanged.
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

/// `popup_window_surface` for a dialog or panel rather than a menu (`frosted_modal_fill`).
pub(crate) fn popup_window_modal_surface(color: Hsla) -> Hsla {
    if window_glass_active() {
        frosted_modal_fill(color)
    } else {
        color
    }
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
        let alpha = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
            load_glass_alpha(&WORK_AREA_GLASS_PERCENT_LIGHT)
        } else {
            load_glass_alpha(&WORK_AREA_GLASS_PERCENT_DARK)
        };
        workspace.opacity(alpha)
    } else {
        workspace
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
pub(crate) fn glass_divider(color: Hsla) -> Hsla {
    if window_glass_active() {
        Hsla::from(chrome_ink()).opacity(0.08)
    } else {
        color
    }
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
