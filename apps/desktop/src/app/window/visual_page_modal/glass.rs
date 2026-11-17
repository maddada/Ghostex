//! The picture behind the app's glass and the work area's tint over it, handed to the floating
//! window's page so the page's backdrop looks like the window it floats over: the reader's own
//! wallpaper or custom picture under the same tint (server/src/visual_pages/theme.rs paints it
//! under the page's glass cards). While the window is open it follows the app: a new wallpaper,
//! another glass setting, a new tint or the app's theme reach the page within a second, and a
//! move or resize of the window at once. The window paints the same picture, placed the same way,
//! behind its own header (`Backdrop`), so the header and the page read as one surface.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User picked "A" on Q987: hand the page the picture behind its glass, so every page shows the reader's wallpaper (or their custom picture) the way the app's own glass does, instead of the painted dusk. Later the same day: "The bg of this isn't changing per different bg that I apply on my desktop" and "make it darker bg on dark mode": the live blur ("Desktop and windows") now hands over the desktop picture too, the page dims it with the work area's own tint (`windowGlassWorkAreaTintDark` / `Light` over the workspace colour) instead of a fixed purple, and the page follows changes while it is open. Video and live-style glass, glass off, the web build, the phone and pages opened elsewhere keep the painted dusk or dawn.
//! CDXC:SessionChat 2026-11-02 WHY:
//! The page can't be see-through (a windowed CEF browser always paints opaque), so it gets the main window's own blurred picture instead: GPUI's `background_wallpaper_snapshot`, a JPEG of a few dozen kilobytes, put into the page by a page script as a data URL. A gxserver address would be refused by the page's content policy (`img-src data: blob: https:`), and the picture belongs to this computer, not to the gxserver that serves the page. The picture is sent again only when its bytes change (GPUI keeps the last JPEG, so asking every second costs no encoding); the tint, placement and theme go in a small script of their own.
//! CDXC:SessionChat 2026-11-04 WHY:
//! User: "why are the bgs of inline htmls still not showing my real wallpaper but blurred like we agreed?". The scripts above never took: a script added for new documents (`Page.addScriptToEvaluateOnNewDocument`) runs before the parser has made `<html>`, so `document.documentElement` was null and both scripts threw (`TypeError: Cannot read properties of null (reading 'setAttribute')`, reproduced in Chromium with the same DevTools call), and the page kept the painted dusk. They were only ever sent again when the picture or the look changed, so the page window never showed the picture unless the wallpaper changed while it was open. Each script now waits for `<html>` (`on_root`), which still comes before the page's first script and first paint.
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User: "can we make the titlebar part of the inline html instead of being a different color?" and "want to be able to resize and move them". The window paints the page's backdrop behind its header too: this picture under this tint at the same place on screen, or the page's own top colour without one (`PAGE_TOP_DARK` / `PAGE_TOP_LIGHT`). The picture is placed for the whole window and the page is told its part of it (`picture_rect`), so it runs on unbroken from the header into the page. While the window moves or is resized the new placement goes straight into the current page (`execute_app_owned_script`); the scripts kept for later documents are only added again on the once-a-second check, so a long drag does not pile them up.
//! CDXC:SessionChat 2026-11-05 DECISION:
//! User: "please make the bg more blurred (can't be changed by user basically), let's just set a heavier blur once here that we decide on". The main window's picture comes with the reader's own Blur setting (theirs is 3 points, so the picture was nearly sharp under the page's text), so the page window softens it once more itself: made small and blurred by a fixed `PICTURE_BLUR_POINTS` points of the screen (`soften`), the same for every reader. The window's band paints the same softened picture, so the two still match exactly.
//! SEE-ALSO: .dependencies/zed/crates/gpui_macos/src/window_wallpaper.rs (`jpeg`), server/src/visual_pages/theme.rs (`data-gx-glass`, `--gx-glass-tint-dark` / `-light`, `--gx-backdrop-top`).

use super::{GpuiVisualPageModalWindow, TOP_BAND_HEIGHT};
use base64::Engine as _;
use gpui::{App, Bounds, Context, Pixels, Point, Rgba, Size, Window, point, px, size};
use std::{sync::Arc, time::Duration};

/// The picture's largest width in pixels: under the glass's blur no finer detail survives.
const PICTURE_MAX_WIDTH: u32 = 640;

/// How soft the page's picture is: a blur of this many points on the screen, whatever the main
/// window's own Blur setting (CDXC:SessionChat 2026-11-05 DECISION above). Halved from 60 to 30 on
/// the user's "please reduce the blur on the inline html bg it's too much now (do it by half)".
const PICTURE_BLUR_POINTS: f32 = 30.0;

/// The softened picture's width in pixels. Under that blur nothing finer than a few points
/// survives, so a small picture scaled up looks the same, and blurring and encoding it is cheap.
const SOFT_PICTURE_WIDTH: u32 = 320;

/// How often the open window checks whether the app's glass or theme changed.
const GLASS_CHECK_EVERY: Duration = Duration::from_secs(1);

/// The top of the page's painted dusk (dark) and dawn (light), which the header paints when there
/// is no picture: `--gx-backdrop-top` in server/src/visual_pages/theme.rs, where the floating
/// window's page fades its backdrop into this colour at its top edge.
pub(crate) const PAGE_TOP_DARK: u32 = 0x131313;
pub(crate) const PAGE_TOP_LIGHT: u32 = 0xf0f0f0;

/// What the page was last handed.
#[derive(Default)]
pub(super) struct PageGlass {
    /// The picture's JPEG, empty for none; `None` before the first hand-over.
    picture: Option<Vec<u8>>,
    /// The look script last added for the documents the page loads later.
    kept_look: Option<String>,
    /// The look the current document was last given.
    shown_look: Option<String>,
    /// What the window paints behind its header, while the page has a picture.
    pub(super) backdrop: Option<Backdrop>,
    /// The app's theme as of the last check, which picks the header's colour without a picture.
    pub(super) light: bool,
}

/// The picture behind the header: the page's own, at the same place, under the same tint.
pub(super) struct Backdrop {
    pub(super) image: Arc<gpui::Image>,
    /// Where the picture lies, in the window's coordinates.
    pub(super) rect: Bounds<Pixels>,
    pub(super) tint: Rgba,
}

/// The glass the page gets: its picture and where it lies in the window, and the work area's tint.
struct Glass {
    jpeg: Vec<u8>,
    rect: Bounds<Pixels>,
    tint: Rgba,
}

impl PageGlass {
    /// Nothing handed yet, in the app's theme as the window opens (`light`).
    pub(super) fn new(light: bool) -> Self {
        Self {
            light,
            ..Self::default()
        }
    }
}

impl GpuiVisualPageModalWindow {
    /// Hands the new page its glass and keeps it in step while the window is open.
    pub(super) fn follow_page_glass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_page_glass(true, window, cx);
        self._glass_watch = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(GLASS_CHECK_EVERY).await;
                if this
                    .update_in(cx, |this, window, cx| {
                        this.sync_page_glass(true, window, cx)
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
        self._glass_bounds = Some(cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_page_glass(false, window, cx);
        }));
    }

    /// `keep`: the look also goes into the documents the page loads later (a reload, Refresh);
    /// otherwise only into the current one.
    fn sync_page_glass(&mut self, keep: bool, window: &Window, cx: &mut Context<Self>) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        let light = crate::app::helpers::chrome_uses_light_appearance();
        let glass = page_glass(window, cx);
        let jpeg = glass
            .as_ref()
            .map(|glass| glass.jpeg.clone())
            .unwrap_or_default();
        // Page scripts run in the current document and in every one it loads later, in the order
        // they were added, so a reload ends on the latest of each.
        if self.glass.picture.as_ref() != Some(&jpeg) {
            surface.read(cx).add_page_script(&picture_script(&jpeg));
            self.glass.picture = Some(jpeg);
        }
        let look = look_script(
            if light { "light" } else { "dark" },
            glass.as_ref().map(|glass| (glass.rect, glass.tint)),
        );
        if keep && self.glass.kept_look.as_ref() != Some(&look) {
            surface.read(cx).add_page_script(&look);
            self.glass.kept_look = Some(look.clone());
            self.glass.shown_look = Some(look);
        } else if self.glass.shown_look.as_ref() != Some(&look) {
            surface.update(cx, |surface, _| surface.execute_app_owned_script(&look));
            self.glass.shown_look = Some(look);
        }
        let backdrop = glass.map(|glass| Backdrop {
            image: match &self.glass.backdrop {
                Some(backdrop) if backdrop.image.bytes() == glass.jpeg.as_slice() => {
                    backdrop.image.clone()
                }
                _ => Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Jpeg, glass.jpeg)),
            },
            rect: glass.rect,
            tint: glass.tint,
        });
        let changed = match (&self.glass.backdrop, &backdrop) {
            (Some(old), Some(new)) => {
                old.rect != new.rect || old.tint != new.tint || !Arc::ptr_eq(&old.image, &new.image)
            }
            (None, None) => false,
            _ => true,
        } || self.glass.light != light;
        self.glass.backdrop = backdrop;
        self.glass.light = light;
        if changed {
            cx.notify();
        }
    }
}

/// The picture behind the main window's glass, where it lies in the page window `window`, and the
/// work area's tint; `None` while the glass shows no picture (glass off, a video or a live style).
fn page_glass(window: &Window, cx: &mut App) -> Option<Glass> {
    let (snapshot, main_bounds) = crate::app::helpers::window_glass_active()
        .then(|| crate::app::helpers::main_window_handle(cx))
        .flatten()
        .and_then(|main| {
            main.update(cx, |_, window, _| {
                (
                    window.background_wallpaper_snapshot(PICTURE_MAX_WIDTH),
                    window.bounds(),
                )
            })
            .ok()
        })?;
    let bounds = window.bounds();
    let tint = Rgba::from(
        crate::app::helpers::workspace_background_color()
            .opacity(crate::app::helpers::work_area_glass_alpha()),
    );
    let Some(snapshot) = snapshot else {
        if crate::app::helpers::window_glass_live::window_glass_live_mode() {
            return None;
        }
        return stand_in_glass(window, cx, tint).or_else(|| remembered_glass(bounds.size, tint));
    };
    // Held against the screen (and always under the live blur, which is the screen behind the
    // window), the page window shows the part of the screen it sits over, like the main window
    // does; attached to the window, the picture covers the page window. The page window and the
    // main window share one screen and one coordinate space, so the page window sits on the
    // screen where the main window does, moved by their difference.
    let on_screen = (crate::app::helpers::window_glass_shows_live_blur()
        || crate::app::helpers::window_glass_picture_follows_screen())
    .then(|| {
        let offset: Point<Pixels> = bounds.origin - main_bounds.origin;
        snapshot.window.origin + offset
    });
    let jpeg = softened(&snapshot.jpeg, f32::from(snapshot.screen.width));
    remember_picture(&jpeg);
    Some(Glass {
        jpeg,
        rect: picture_rect(bounds.size, snapshot.screen, on_screen),
        tint,
    })
}

/// CDXC:SessionChat 2026-11-16 DECISION:
/// User: "why dont i see the bg show here in the ghostex page pls? we implemented this before but i dont see it now anymore". The page window's picture comes from the main window's snapshot of the desktop picture, and that snapshot is `None` whenever macOS names a desktop picture file that cannot be opened. That is what happened on 2026-11-16: a wallpaper changer left the desktop on `$TMPDIR/StaticWallpaper/<n>_static.jpg`, which was gone by the afternoon (the system log showed Ghostex's once-a-second check failing on it, "can't open ... fileExists == false"). The main window's live blur kept working, since the system blurs what is on screen, but the page window fell back to the painted dusk. So the last picture the page window was handed is kept (in memory, and in the cache folder for the next launch) and used while the desktop picture can't be read, placed covering the window; the real picture takes over again as soon as one can be read. A video or a live style is not this: there the glass shows no picture on purpose, so the page keeps the painted dusk or dawn.
fn remembered_glass(window_size: Size<Pixels>, tint: Rgba) -> Option<Glass> {
    let (jpeg, screen) = remembered_picture()?;
    Some(Glass {
        jpeg,
        rect: picture_rect(window_size, screen, None),
        tint,
    })
}

/// CDXC:SessionChat 2026-11-17 WHY:
/// User: "why is this not showing the blurred bg like it used to??? it's inline html in the macos app". The desktop is a Wallspace video wallpaper. Wallspace plays the video itself and names a still of it, `$TMPDIR/StaticWallpaper/<key>_static.jpg`, as the macOS desktop picture, and that file was gone again (the system log showed the main window's once-a-second read failing on it, "fileExists == false"). The fallback of 2026-11-16 above only helps once a picture has been seen, and none had been since the app started, so the page window painted the flat dusk top (`PAGE_TOP_DARK`). While the desktop picture can't be read, the page now uses Wallspace's own poster of the wallpaper it is playing (`wallspace_poster`), which is the same scene, placed as the main window's picture would be. The kept picture still comes after it, for any other desktop picture that disappears.
fn stand_in_glass(window: &Window, cx: &App, tint: Rgba) -> Option<Glass> {
    let poster = wallspace_poster()?;
    let bounds = window.bounds();
    let display = window.display(cx)?.bounds();
    let jpeg = stand_in_picture(&poster, display.size)?;
    remember_picture(&jpeg);
    let on_screen = (crate::app::helpers::window_glass_shows_live_blur()
        || crate::app::helpers::window_glass_picture_follows_screen())
    .then(|| bounds.origin - display.origin);
    Some(Glass {
        jpeg,
        rect: picture_rect(bounds.size, display.size, on_screen),
        tint,
    })
}

/// The poster Wallspace keeps for the wallpaper it is playing, when there is one.
#[cfg(target_os = "macos")]
fn wallspace_poster() -> Option<std::path::PathBuf> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let prefs = home.join("Library/Preferences/wallspace.app.plist");
    // The preferences are read again only when the file changes.
    static LAST: std::sync::Mutex<Option<(std::time::SystemTime, Option<String>)>> =
        std::sync::Mutex::new(None);
    let modified = std::fs::metadata(&prefs).and_then(|m| m.modified()).ok()?;
    let mut last = LAST.lock().ok()?;
    let key = match last.as_ref() {
        Some((seen, key)) if *seen == modified => key.clone(),
        _ => {
            let key = plist::Value::from_file(&prefs)
                .ok()
                .and_then(|prefs| wallspace_key(prefs.as_dictionary()?));
            *last = Some((modified, key.clone()));
            key
        }
    }?;
    drop(last);
    let poster = home
        .join("Library/Caches/Wallspace/images/posters")
        .join(format!("{key}.webp"));
    poster.is_file().then_some(poster)
}

#[cfg(not(target_os = "macos"))]
fn wallspace_poster() -> Option<std::path::PathBuf> {
    None
}

/// The playing wallpaper's key in Wallspace's preferences: `savedWallpaperKey`, else the name of
/// `savedWallpaperMasterUrl`'s video. Only digits, letters, `-` and `_`, since it names a file.
#[cfg(target_os = "macos")]
fn wallspace_key(prefs: &plist::Dictionary) -> Option<String> {
    let key = match prefs.get("savedWallpaperKey") {
        Some(plist::Value::Integer(key)) => Some(key.to_string()),
        Some(plist::Value::String(key)) => Some(key.clone()),
        _ => None,
    }
    .or_else(|| {
        let url = prefs.get("savedWallpaperMasterUrl")?.as_string()?;
        let name = url.rsplit('/').next()?;
        Some(name.split('.').next()?.to_string())
    })?;
    (!key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
    .then_some(key)
}

/// The picture at `path`, cut to the shape of a screen `screen` points in size (the middle of it,
/// as a desktop fills the screen) and softened like the main window's (`softened`). Made again only
/// when the file or the screen changes.
fn stand_in_picture(path: &std::path::Path, screen: Size<Pixels>) -> Option<Vec<u8>> {
    type Key = (std::path::PathBuf, std::time::SystemTime, u32, u32);
    static LAST: std::sync::Mutex<Option<(Key, Vec<u8>)>> = std::sync::Mutex::new(None);
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let key: Key = (
        path.to_path_buf(),
        modified,
        f32::from(screen.width).to_bits(),
        f32::from(screen.height).to_bits(),
    );
    if let Ok(last) = LAST.lock()
        && let Some((last_key, jpeg)) = last.as_ref()
        && *last_key == key
    {
        return Some(jpeg.clone());
    }
    let jpeg = screen_shaped_jpeg(&std::fs::read(path).ok()?, screen)?;
    let soft = softened(&jpeg, f32::from(screen.width));
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, soft.clone()));
    }
    Some(soft)
}

/// `bytes`, a picture in any format the app reads, cut to the shape of `screen` around its middle
/// and made at most `PICTURE_MAX_WIDTH` pixels wide, as a JPEG.
fn screen_shaped_jpeg(bytes: &[u8], screen: Size<Pixels>) -> Option<Vec<u8>> {
    let picture = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (width, height) = (picture.width(), picture.height());
    if width == 0 || height == 0 {
        return None;
    }
    let shape = f32::from(screen.width) / f32::from(screen.height).max(1.0);
    let (cut_width, cut_height) = if width as f32 / height as f32 > shape {
        (
            ((height as f32 * shape).round() as u32).clamp(1, width),
            height,
        )
    } else {
        (
            width,
            ((width as f32 / shape).round() as u32).clamp(1, height),
        )
    };
    let cut = image::imageops::crop_imm(
        &picture,
        (width - cut_width) / 2,
        (height - cut_height) / 2,
        cut_width,
        cut_height,
    )
    .to_image();
    let out_width = cut_width.min(PICTURE_MAX_WIDTH);
    let out_height =
        ((cut_height as f32 * out_width as f32 / cut_width as f32).round() as u32).max(1);
    let small = image::imageops::resize(
        &cut,
        out_width,
        out_height,
        image::imageops::FilterType::Triangle,
    );
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&small)
        .ok()?;
    Some(out)
}

/// The last picture the page window was handed, and its size (only its shape matters, to cover
/// the window); read from the cache folder once per launch.
struct Remembered {
    jpeg: Vec<u8>,
    screen: Size<Pixels>,
}

static REMEMBERED: std::sync::Mutex<Option<Option<Remembered>>> = std::sync::Mutex::new(None);

fn remembered_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .cache_dir
        .join("page-window-glass.jpg")
}

/// Keeps `jpeg` for the times the desktop picture can't be read; written only when it changed.
fn remember_picture(jpeg: &[u8]) {
    let Ok(mut remembered) = REMEMBERED.lock() else {
        return;
    };
    if matches!(&*remembered, Some(Some(known)) if known.jpeg == jpeg) {
        return;
    }
    let Some(screen) = picture_size(jpeg) else {
        return;
    };
    *remembered = Some(Some(Remembered {
        jpeg: jpeg.to_vec(),
        screen,
    }));
    drop(remembered);
    let path = remembered_path();
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let partial = path.with_extension("jpg.partial");
    if std::fs::write(&partial, jpeg).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

/// The kept picture, from memory or else from the cache folder.
fn remembered_picture() -> Option<(Vec<u8>, Size<Pixels>)> {
    let mut remembered = REMEMBERED.lock().ok()?;
    let known = remembered.get_or_insert_with(|| {
        let jpeg = std::fs::read(remembered_path()).ok()?;
        let screen = picture_size(&jpeg)?;
        Some(Remembered { jpeg, screen })
    });
    known
        .as_ref()
        .map(|known| (known.jpeg.clone(), known.screen))
}

fn picture_size(jpeg: &[u8]) -> Option<Size<Pixels>> {
    let picture = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    Some(size(
        px(picture.width() as f32),
        px(picture.height() as f32),
    ))
}

/// `jpeg`, the main window's picture of a screen `screen_width` points wide, softened for the page
/// (`soften`). The last one is kept, so the check every second costs nothing while the picture
/// stays the same; a picture that can't be read is handed over as it is.
fn softened(jpeg: &[u8], screen_width: f32) -> Vec<u8> {
    use std::hash::{Hash as _, Hasher as _};
    static LAST: std::sync::Mutex<Option<(u64, Vec<u8>)>> = std::sync::Mutex::new(None);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    jpeg.hash(&mut hasher);
    screen_width.to_bits().hash(&mut hasher);
    let key = hasher.finish();
    if let Ok(last) = LAST.lock()
        && let Some((last_key, soft)) = last.as_ref()
        && *last_key == key
    {
        return soft.clone();
    }
    let soft = soften(jpeg, screen_width, PICTURE_BLUR_POINTS).unwrap_or_else(|| jpeg.to_vec());
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, soft.clone()));
    }
    soft
}

/// `jpeg`, a picture of a whole screen `screen_width` points wide, made at most
/// `SOFT_PICTURE_WIDTH` pixels wide and blurred by `blur_points` points of that screen.
fn soften(jpeg: &[u8], screen_width: f32, blur_points: f32) -> Option<Vec<u8>> {
    let picture = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .to_rgb8();
    let width = picture.width().clamp(1, SOFT_PICTURE_WIDTH);
    let height = ((picture.height() as f32 * width as f32 / picture.width().max(1) as f32).round()
        as u32)
        .max(1);
    let small = image::imageops::resize(
        &picture,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    let sigma = blur_points * width as f32 / screen_width.max(1.0);
    let soft = image::imageops::fast_blur(&small, sigma);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
        .encode_image(&soft)
        .ok()?;
    Some(out)
}

/// Where a picture laid out on a whole screen of `screen` lies in a window of `window_size`, in
/// the window's coordinates: held against the screen when the window's top-left corner is
/// `on_screen` there, else covering the window, centred.
fn picture_rect(
    window_size: Size<Pixels>,
    screen: Size<Pixels>,
    on_screen: Option<Point<Pixels>>,
) -> Bounds<Pixels> {
    if let Some(origin) = on_screen {
        return Bounds::new(point(-origin.x, -origin.y), screen);
    }
    let scale = (f32::from(window_size.width) / f32::from(screen.width).max(1.0))
        .max(f32::from(window_size.height) / f32::from(screen.height).max(1.0));
    let picture = size(screen.width * scale, screen.height * scale);
    Bounds::new(
        point(
            (window_size.width - picture.width) / 2.0,
            (window_size.height - picture.height) / 2.0,
        ),
        picture,
    )
}

/// Runs `body` with `d` set to the page's `<html>`, as soon as there is one. A script added for
/// new documents runs before the parser has made it (CDXC:SessionChat 2026-11-04 WHY above); the
/// observer's callback still runs before the page's first script.
fn on_root(body: &str) -> String {
    format!(
        "(function(){{function run(d){{{body}}}var d=document.documentElement;if(d){{run(d);return;}}\
new MutationObserver(function(_,o){{var d=document.documentElement;if(d){{o.disconnect();run(d);}}}})\
.observe(document,{{childList:true}});}})();"
    )
}

/// Shows `jpeg` behind the page, or takes the picture away when it is empty.
fn picture_script(jpeg: &[u8]) -> String {
    if jpeg.is_empty() {
        return on_root(
            "d.removeAttribute(\"data-gx-glass\");d.style.removeProperty(\"--gx-glass-picture\");",
        );
    }
    let picture = base64::engine::general_purpose::STANDARD.encode(jpeg);
    on_root(&format!(
        "d.setAttribute(\"data-gx-glass\",\"\");\
d.style.setProperty(\"--gx-glass-picture\",'url(\"data:image/jpeg;base64,{picture}\")');"
    ))
}

/// The app's theme (unless the reader or the page itself chose one, `data-gx-theme-from`, kit.rs),
/// and with a picture where it lies (`rect`, in the window's coordinates; the page starts under the
/// header) and the tint for that theme; the other theme's tint is left to the page's default.
fn look_script(theme: &str, glass: Option<(Bounds<Pixels>, Rgba)>) -> String {
    let other = if theme == "light" { "dark" } else { "light" };
    let glass = match glass {
        Some((rect, tint)) => format!(
            "s.setProperty(\"--gx-glass-size\",\"{}px {}px\");s.setProperty(\"--gx-glass-position\",\"{}px {}px\");\
s.setProperty(\"--gx-glass-tint-{theme}\",\"{}\");s.removeProperty(\"--gx-glass-tint-{other}\");",
            css_px(rect.size.width),
            css_px(rect.size.height),
            css_px(rect.origin.x),
            css_px(rect.origin.y - px(TOP_BAND_HEIGHT)),
            css_rgba(tint),
        ),
        None => "s.removeProperty(\"--gx-glass-tint-light\");s.removeProperty(\"--gx-glass-tint-dark\");"
            .to_string(),
    };
    on_root(&format!(
        "var s=d.style;{glass}\
if(d.hasAttribute(\"data-gx-theme-from\")||d.getAttribute(\"data-gx-theme\")===\"{theme}\")return;\
d.setAttribute(\"data-gx-theme\",\"{theme}\");\
Array.prototype.forEach.call(document.querySelectorAll(\".gx-theme-switch [data-gx-set-theme]\"),function(b){{\
b.setAttribute(\"aria-pressed\",b.getAttribute(\"data-gx-set-theme\")===\"{theme}\"?\"true\":\"false\");}});"
    ))
}

fn css_px(value: Pixels) -> String {
    format!("{}", (f32::from(value) * 100.0).round() / 100.0)
}

fn css_rgba(tint: Rgba) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        (tint.r * 255.0).round(),
        (tint.g * 255.0).round(),
        (tint.b * 255.0).round(),
        tint.a
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_held_against_the_screen_sits_where_the_window_is_on_it() {
        let rect = picture_rect(
            size(px(900.0), px(700.0)),
            size(px(1728.0), px(1117.0)),
            Some(point(px(400.0), px(150.0))),
        );
        assert_eq!(rect.origin, point(px(-400.0), px(-150.0)));
        assert_eq!(rect.size, size(px(1728.0), px(1117.0)));
    }

    #[test]
    fn a_picture_attached_to_the_window_covers_it_centred() {
        // A tall window over a wide screen: the height decides, the sides are cut evenly.
        let rect = picture_rect(
            size(px(600.0), px(1000.0)),
            size(px(1600.0), px(1000.0)),
            None,
        );
        assert_eq!(rect.size, size(px(1600.0), px(1000.0)));
        assert_eq!(rect.origin, point(px(-500.0), px(0.0)));
        // A wide window: the width decides, top and bottom are cut evenly.
        let rect = picture_rect(
            size(px(2000.0), px(500.0)),
            size(px(1000.0), px(1000.0)),
            None,
        );
        assert_eq!(rect.size, size(px(2000.0), px(2000.0)));
        assert_eq!(rect.origin, point(px(0.0), px(-750.0)));
    }

    #[test]
    fn the_page_is_told_its_part_of_the_window_picture() {
        let look = look_script(
            "dark",
            Some((
                Bounds::new(point(px(-400.0), px(-150.0)), size(px(1728.0), px(1117.0))),
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.77,
                },
            )),
        );
        // The page starts under the header, so its picture starts that much higher.
        let top = -150.0 - TOP_BAND_HEIGHT;
        assert!(
            look.contains(&format!("\"--gx-glass-position\",\"-400px {top}px\"")),
            "{look}"
        );
        assert!(
            look.contains("\"--gx-glass-size\",\"1728px 1117px\""),
            "{look}"
        );
        assert!(
            look.contains("\"--gx-glass-tint-dark\",\"rgba(0,0,0,0.770)\""),
            "{look}"
        );
    }

    /// A picture of hard black and white stripes, `width` x `height`, as a JPEG.
    fn stripes(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbImage::from_fn(width, height, |x, _| {
            if (x / 4) % 2 == 0 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
            .encode_image(&picture)
            .unwrap();
        out
    }

    /// How far a picture's pixels stray from its mean brightness.
    fn spread(jpeg: &[u8]) -> f32 {
        let picture = image::load_from_memory(jpeg).unwrap().to_luma8();
        let values: Vec<f32> = picture
            .pixels()
            .map(|pixel| f32::from(pixel.0[0]))
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        (values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f32>()
            / values.len() as f32)
            .sqrt()
    }

    #[test]
    fn the_page_picture_is_softened_small_whatever_the_window_blur() {
        let sharp = stripes(640, 360);
        let soft = soften(&sharp, 1728.0, PICTURE_BLUR_POINTS).unwrap();
        let picture = image::load_from_memory(&soft).unwrap();
        assert_eq!(
            (picture.width(), picture.height()),
            (SOFT_PICTURE_WIDTH, 180)
        );
        // Stripes 4 pixels wide are gone: what is left is close to an even grey.
        assert!(spread(&sharp) > 100.0);
        assert!(spread(&soft) < 15.0, "{}", spread(&soft));
    }

    #[test]
    fn a_heavier_blur_leaves_less_of_the_picture() {
        let picture = image::RgbImage::from_fn(640, 360, |x, y| {
            image::Rgb([
                (x % 256) as u8,
                (y % 256) as u8,
                ((x / 64 + y / 64) % 2 * 255) as u8,
            ])
        });
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode_image(&picture)
            .unwrap();
        let light = spread(&soften(&jpeg, 1728.0, 10.0).unwrap());
        let heavy = spread(&soften(&jpeg, 1728.0, 90.0).unwrap());
        assert!(heavy < light, "{heavy} < {light}");
    }

    #[test]
    fn a_stand_in_picture_is_cut_to_the_screen_shape_around_its_middle() {
        // A 16:9 poster on a 16:10 screen loses its sides; on a 4:3 screen it loses more of them.
        let poster = stripes(1280, 720);
        let cut = screen_shaped_jpeg(&poster, size(px(1728.0), px(1080.0))).unwrap();
        let cut = image::load_from_memory(&cut).unwrap();
        assert_eq!((cut.width(), cut.height()), (PICTURE_MAX_WIDTH, 400));
        let cut = screen_shaped_jpeg(&poster, size(px(1024.0), px(768.0))).unwrap();
        let cut = image::load_from_memory(&cut).unwrap();
        assert_eq!((cut.width(), cut.height()), (PICTURE_MAX_WIDTH, 480));
        // A tall screen loses the top and bottom of a square picture instead.
        let cut = screen_shaped_jpeg(&stripes(400, 400), size(px(800.0), px(1600.0))).unwrap();
        let cut = image::load_from_memory(&cut).unwrap();
        assert_eq!((cut.width(), cut.height()), (200, 400));
        assert!(screen_shaped_jpeg(b"not a picture", size(px(800.0), px(600.0))).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn wallspace_names_its_wallpaper_by_key_or_by_its_video() {
        let mut prefs = plist::Dictionary::new();
        prefs.insert(
            "savedWallpaperMasterUrl".into(),
            plist::Value::String(
                "file:///Users/me/Library/Caches/Wallspace/Wallpapers/107826141533452.mp4".into(),
            ),
        );
        assert_eq!(wallspace_key(&prefs).as_deref(), Some("107826141533452"));
        prefs.insert(
            "savedWallpaperKey".into(),
            plist::Value::Integer(816347559438864_i64.into()),
        );
        assert_eq!(wallspace_key(&prefs).as_deref(), Some("816347559438864"));
        // A key that would leave the posters folder is refused.
        prefs.insert(
            "savedWallpaperKey".into(),
            plist::Value::String("../../x".into()),
        );
        assert_eq!(wallspace_key(&prefs), None);
        assert_eq!(wallspace_key(&plist::Dictionary::new()), None);
    }

    #[test]
    fn a_picture_that_cannot_be_read_is_handed_over_as_it_is() {
        assert_eq!(softened(b"not a jpeg", 1728.0), b"not a jpeg".to_vec());
    }

    #[test]
    fn every_page_script_waits_for_the_html_element() {
        for script in [
            picture_script(b"jpeg"),
            picture_script(&[]),
            look_script("light", None),
        ] {
            assert!(script.contains("var d=document.documentElement;if(d){run(d);return;}"));
            assert!(script.contains("new MutationObserver"));
            // Nothing touches `<html>` outside the function that is handed it.
            let outside = script.split_once("function run(d){").map(|(head, _)| head);
            assert_eq!(outside, Some("(function(){"));
        }
    }
}
