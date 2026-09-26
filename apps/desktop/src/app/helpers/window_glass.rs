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

/// CDXC:Theming 2026-09-23 DECISION:
/// User, asked whether the glass could show a video: "ok set those in the dropdown to pick from (if any are downloaded we can let the user pick those) / also allow the user to pick a specific mp4/mov file". Glass shows has a Video choice with one video for dark mode (`windowGlassVideoDark`) and one for light mode (`windowGlassVideoLight`): an aerial wallpaper macOS has downloaded (saved as `aerial:<id>`) or a picked .mov/.mp4/.m4v file. It plays muted and looping, blurred and placed like the picture (`windowGlassImagePlacement`).
///
/// CDXC:Theming 2026-09-23 WHY:
/// A background video must cost nothing while nobody is looking, so it plays only while Ghostex is the front app and its window is on screen, and pauses while the displays sleep, in Low Power Mode, and on battery unless `windowGlassVideoOnlyOnPower` is off (on by default, to spare the battery); Reduce Motion shows its first frame instead. A mode with no video chosen, or one whose file or aerial is gone, shows the live blur.
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

/// The video the main window's glass plays in the current appearance, when Glass shows is Video
/// and a playable one is chosen for that appearance, and whether it pauses on battery.
fn window_glass_video() -> (Option<std::path::PathBuf>, bool) {
    let Ok(videos) = WINDOW_GLASS_VIDEOS.lock() else {
        return (None, true);
    };
    if !videos.video {
        return (None, videos.only_on_power);
    }
    let video = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        videos.light.clone()
    } else {
        videos.dark.clone()
    };
    (video, videos.only_on_power)
}

/// Whether the glass shows a picture, video or live style rather than the live blur: Wallpaper
/// only always does, Custom image, Video and Live only once one is chosen for the current
/// appearance.
fn window_glass_uses_backdrop(
    image: &Option<std::path::PathBuf>,
    video: &Option<std::path::PathBuf>,
    live: &Option<gpui::LiveBackground>,
) -> bool {
    if !WINDOW_GLASS_WALLPAPER.load(Ordering::Relaxed) {
        return false;
    }
    if window_glass_live::window_glass_live_mode() {
        return live.is_some();
    }
    let custom = WINDOW_GLASS_IMAGES.lock().is_ok_and(|images| images.custom);
    let video_mode = WINDOW_GLASS_VIDEOS.lock().is_ok_and(|videos| videos.video);
    if custom {
        image.is_some()
    } else if video_mode {
        video.is_some()
    } else {
        true
    }
}

/// What the main window's glass drew last; see `APPLIED_MAIN_WINDOW_GLASS`.
static APPLIED_MAIN_WINDOW_GLASS_LIVE: std::sync::Mutex<Option<gpui::LiveBackground>> =
    std::sync::Mutex::new(None);

/// What the main window's glass played last; see `APPLIED_MAIN_WINDOW_GLASS`.
static APPLIED_MAIN_WINDOW_GLASS_VIDEO: std::sync::Mutex<(Option<std::path::PathBuf>, bool)> =
    std::sync::Mutex::new((None, true));

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

/// What the main window's glass showed last; see `APPLIED_MAIN_WINDOW_GLASS`.
static APPLIED_MAIN_WINDOW_GLASS_IMAGE: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

/// What the main window was last switched to: 0 nothing yet, 1 opaque, 2 live blur, 3 wallpaper
/// blur. GPUI has no
/// getter for the window's background appearance, and re-applying it every frame rebuilds the
/// window's AppKit backing state.
static APPLIED_MAIN_WINDOW_GLASS: AtomicU8 = AtomicU8::new(0);

/// Only the main window is blurred. Views that also render in their own windows (the chat) ask
/// with `window_glass_active_in` so they stay opaque there.
static MAIN_WINDOW_ID: AtomicU64 = AtomicU64::new(u64::MAX);

/// Default coverage of the sidebar's and the work area's tints over the blurred desktop: point 20
/// of the Transparency strength slider (`transparencyStrengthPatch` in
/// packages/core-ui/settings-modal/theme-simple-controls.tsx), which keeps the sidebar 7 points
/// more solid than the work area. SEE-ALSO: the CDXC:Theming 2026-09-25 DECISION on
/// `DEFAULT_WINDOW_GLASS_SIDEBAR_OPACITY_DARK_PERCENT` in packages/shared/ghostex-settings/types.ts.
const SIDEBAR_GLASS_ALPHA_DARK: f32 = 0.88;
const SIDEBAR_GLASS_ALPHA_LIGHT: f32 = 0.93;
const WORK_AREA_GLASS_ALPHA_DARK: f32 = 0.81;
const WORK_AREA_GLASS_ALPHA_LIGHT: f32 = 0.86;

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
        .map(|value| value.clamp(40.0, 100.0).round() as u8)
        .unwrap_or_else(|| glass_percent(default_alpha));
    target.store(percent, Ordering::Relaxed);
    percent
}

/// Stores the work area's percentage. Settings that still hold the retired extra layer
/// (`legacy_extra_key`) get the coverage it and the sidebar tint added up to, the same migration
/// `normalizeWindowGlassWorkAreaTint` in packages/shared/ghostex-settings/normalize.ts applies.
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
/// CDXC:Theming 2026-09-25 DECISION:
/// User: "let's enable transparency on windows please also if possible. like it works on mac exactly." Glass runs on macOS and Windows; the system setting that turns transparency off always wins (Reduce Transparency on macOS, Transparency effects on Windows). Linux stays opaque: its compositors cannot promise a blur, so glass there would expose the raw desktop.
///
/// Returns whether the resolved state changed.
pub(crate) fn refresh_window_glass(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let light = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    let wanted = match object
        .get("windowGlass")
        .and_then(serde_json::Value::as_str)
    {
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
    // The wallpaper and custom-image backdrops exist only in the macOS window backend; Settings
    // offers them only there.
    WINDOW_GLASS_WALLPAPER.store(
        cfg!(target_os = "macos")
            && matches!(source, Some("wallpaper" | "customImage" | "video" | "live")),
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
        videos.video = source == Some("video");
        videos.dark = video("windowGlassVideoDark");
        videos.light = video("windowGlassVideoLight");
        videos.only_on_power = object
            .get("windowGlassVideoOnlyOnPower")
            .and_then(serde_json::Value::as_bool)
            != Some(false);
    }
    window_glass_live::refresh_window_glass_live(object);
    let active = cfg!(any(target_os = "macos", target_os = "windows"))
        && wanted
        && !system_reduces_transparency();
    WINDOW_GLASS_ACTIVE.swap(active, Ordering::Relaxed) != active
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
                || id == FLOATING_REVEAL_WINDOW_ID.load(Ordering::Relaxed)
                || id == DOCS_DRAWER_WINDOW_ID.load(Ordering::Relaxed)
        })
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
        if let Ok(mut size) = MAIN_WINDOW_SIZE.lock() {
            *size = Some(window.viewport_size());
        }
        let wanted = window_glass_background_appearance();
        window.set_background_wallpaper_follows_screen(
            WINDOW_GLASS_PICTURE_FOLLOWS_SCREEN.load(Ordering::Relaxed),
        );
        let image = window_glass_custom_image();
        let video = window_glass_video();
        let live = window_glass_live::window_glass_live();
        // Custom image, Video or Live with nothing chosen for this appearance is the live blur.
        let wallpaper = window_glass_uses_backdrop(&image, &video.0, &live);
        let code = match (wanted == WindowBackgroundAppearance::Blurred, wallpaper) {
            (false, _) => 1,
            (true, false) => 2,
            (true, true) => 3,
        };
        let image_changed = APPLIED_MAIN_WINDOW_GLASS_IMAGE
            .lock()
            .map(|mut applied| {
                let changed = *applied != image;
                *applied = image.clone();
                changed
            })
            .unwrap_or(true);
        let video_changed = APPLIED_MAIN_WINDOW_GLASS_VIDEO
            .lock()
            .map(|mut applied| {
                let changed = *applied != video;
                *applied = video.clone();
                changed
            })
            .unwrap_or(true);
        let live_changed = APPLIED_MAIN_WINDOW_GLASS_LIVE
            .lock()
            .map(|mut applied| {
                let changed = *applied != live;
                *applied = live.clone();
                changed
            })
            .unwrap_or(true);
        let previous = APPLIED_MAIN_WINDOW_GLASS.swap(code, Ordering::Relaxed);
        if previous == code && !image_changed && !video_changed && !live_changed {
            return;
        }
        window.set_background_live(live);
        window.set_background_video(video.0, video.1);
        window.set_background_wallpaper_image(image);
        window.set_background_wallpaper(wallpaper);
        window.set_background_appearance(wanted);
        // Switching between the two blurs leaves the glass on, so terminals keep their config.
        if previous != 0 && (previous == 1) != (code == 1) {
            cx.defer_in(window, |this, _window, cx| {
                this.reload_live_gpui_engine_terminal_config(cx);
            });
        }
    }
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
pub(crate) fn frosted_menu_fill(color: Hsla) -> Hsla {
    let lifted = if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        color
    } else {
        color.blend(gpui::white().opacity(WINDOW_GLASS_MENU_LIFT_DARK))
    };
    lifted.opacity(frosted_menu_alpha())
}

/// How much of a frosted menu or tooltip its fill covers: little in dark mode, where the menu's
/// light text reads over anything behind it, more in light mode, where dark text needs a lighter
/// backing.
pub(crate) fn frosted_menu_alpha() -> f32 {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        WINDOW_GLASS_MENU_ALPHA_LIGHT
    } else {
        WINDOW_GLASS_MENU_ALPHA
    }
}

/// Blur radius of a frosted menu or tooltip window. Narrower than the main window's glass so the
/// shapes and colours behind a menu still read through it.
pub(crate) const FROSTED_MENU_BLUR_RADIUS: f32 = 20.0;

/// Whether a frosted menu's backdrop keeps the colour saturation the main window's glass strips.
pub(crate) const FROSTED_MENU_KEEP_SATURATION: bool = true;

/// Gives a menu or tooltip window the frosted menus' blur (`FROSTED_MENU_BLUR_RADIUS`,
/// `FROSTED_MENU_KEEP_SATURATION`). Call it where the window sets its corner radius.
pub(crate) fn apply_frosted_menu_blur(window: &gpui::Window) {
    window.set_background_blur_style(
        gpui::px(FROSTED_MENU_BLUR_RADIUS),
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
