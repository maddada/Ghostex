//! The picture behind the app's glass and the work area's tint over it, handed to the floating
//! window's page so the page's backdrop looks like the window it floats over: the reader's own
//! wallpaper or custom picture under the same tint (server/src/visual_pages/theme.rs paints it
//! under the page's glass cards). While the window is open it follows the app: a new wallpaper,
//! another glass setting, a new tint or the app's theme reach the page within a second, a resize
//! of the window at once and a move held by the mouse on release. The window paints the same picture, placed the same way,
//! behind its own header (`Backdrop`), so the header and the page read as one surface.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User picked "A" on Q987: hand the page the picture behind its glass, so every page shows the reader's wallpaper (or their custom picture) the way the app's own glass does, instead of the painted dusk. Later the same day: "The bg of this isn't changing per different bg that I apply on my desktop" and "make it darker bg on dark mode": the live blur ("Desktop and windows") now hands over the desktop picture too, the page dims it with the work area's own tint (`windowGlassWorkAreaTintDark` / `Light` over the workspace colour) instead of a fixed purple, and the page follows changes while it is open. Video and live-style glass, glass off, the web build, the phone and pages opened elsewhere keep the painted dusk or dawn.
//! CDXC:SessionChat 2026-11-02 WHY:
//! The page can't be see-through (a windowed CEF browser always paints opaque), so it gets the main window's own blurred picture instead: GPUI's `background_wallpaper_snapshot`, a JPEG of a few dozen kilobytes, put into the page by a page script as a data URL. A gxserver address would be refused by the page's content policy (`img-src data: blob: https:`), and the picture belongs to this computer, not to the gxserver that serves the page. The picture is sent again only when its bytes change (GPUI keeps the last JPEG, so asking every second costs no encoding); the tint, placement and theme go in a small script of their own.
//! CDXC:SessionChat 2026-11-04 WHY:
//! User: "why are the bgs of inline htmls still not showing my real wallpaper but blurred like we agreed?". The scripts above never took: a script added for new documents (`Page.addScriptToEvaluateOnNewDocument`) runs before the parser has made `<html>`, so `document.documentElement` was null and both scripts threw (`TypeError: Cannot read properties of null (reading 'setAttribute')`, reproduced in Chromium with the same DevTools call), and the page kept the painted dusk. They were only ever sent again when the picture or the look changed, so the page window never showed the picture unless the wallpaper changed while it was open. Each script now waits for `<html>` (`on_root`), which still comes before the page's first script and first paint.
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User: "can we make the titlebar part of the inline html instead of being a different color?" and "want to be able to resize and move them". The window paints the page's backdrop behind its header too: this picture under this tint at the same place on screen, or the page's own top colour without one (`PAGE_TOP_DARK` / `PAGE_TOP_LIGHT`). The picture is placed for the whole window and the page is told its part of it (`picture_rect`), so it runs on unbroken from the header into the page. A resize places the picture at once, straight into the current page (`execute_app_owned_script`); a move held by the mouse places it on release (`follow_window_bounds`); the scripts kept for later documents are only added again on the once-a-second check, so a long drag does not pile them up.
//! CDXC:SessionChat 2026-11-05 DECISION:
//! User: "please make the bg more blurred (can't be changed by user basically), let's just set a heavier blur once here that we decide on". The main window's picture comes with the reader's own Blur setting (theirs is 3 points, so the picture was nearly sharp under the page's text), so the page window softens it once more itself: made small and blurred by a fixed `PICTURE_BLUR_POINTS` points of the screen (`soften`), the same for every reader. The window's band paints the same softened picture, so the two still match exactly.
//! CDXC:SessionChat 2026-11-22 DECISION:
//! User: "If we have Art selected, the background of these Live pages should be that art, but more blurred (like we do to the wallpaper)." Art themes (built-in artworks and Your image, `window_art.rs`) paint in an opaque GPUI window without a platform wallpaper snapshot, and Animated (`window_glass_live.rs`) previously returned `None`, leaving every Live page on Art or Animated with the flat dusk/dawn fill. `page_glass` now checks Art first (softening `window_art_snapshot` by `PICTURE_BLUR_POINTS` on top of `windowArtBlur` and combining `windowArtDim` with the work area tint in `art_tint`), then Animated (softening a frame of the active style in the current theme's colours from `live_backdrop.rs`), before falling through to Window transparency's Wallpaper / Custom image / Desktop snapshot.
//! SEE-ALSO: .dependencies/zed/crates/gpui_macos/src/window_wallpaper.rs (`jpeg`), app/helpers/window_art.rs (`window_art_snapshot`), server/src/visual_pages/theme.rs (`data-gx-glass`, `--gx-glass-tint-dark` / `-light`, `--gx-backdrop-top`).

use super::{CONTROL_INSET, GpuiVisualPageModalWindow, TOP_BAND_HEIGHT, controls_side_attr};
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

/// How often a held move checks whether the mouse button is up, to place the picture once it is.
const MOVE_END_CHECK_EVERY: Duration = Duration::from_millis(30);

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
    /// Whether the window's controls sat on the left at the last check (`close_buttons_on_left`).
    controls_on_left: bool,
    /// The window's size when the page was last handed its glass, which tells a move from a resize.
    size: Option<Size<Pixels>>,
    /// A held move the picture waits out (`follow_window_bounds`), placed once the button is up.
    move_end: Option<gpui::Task<()>>,
}

/// The picture behind the header: the page's own, at the same place, under the same tint.
/// Kept as a decoded `RenderImage` so the top band paints it on the very first frame with no
/// async-decode flash against the page below.
pub(super) struct Backdrop {
    pub(super) image: Arc<gpui::RenderImage>,
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
            controls_on_left: crate::app::window::round_close_button::close_buttons_on_left(),
            ..Self::default()
        }
    }
}

impl GpuiVisualPageModalWindow {
    /// Hands the new page its glass and keeps it in step while the window is open.
    pub(super) fn follow_page_glass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        PAGE_WINDOWS_OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sync_page_glass(true, window, cx);
        self._glass_watch = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(GLASS_CHECK_EVERY).await;
                if this
                    .update_in(cx, |this, window, cx| {
                        // A hidden page is only ordered out: nothing to check until it is shown again.
                        if !this.hidden && this.glass.move_end.is_none() {
                            this.sync_page_glass(true, window, cx)
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
        self._glass_bounds = Some(cx.observe_window_bounds(window, |this, window, cx| {
            this.follow_window_bounds(window, cx);
        }));
    }

    /// CDXC:SessionChat 2026-11-25 WHY:
    /// User: "resizing doesnt lag only moving does". While the window is dragged by its band, macOS moves it itself and tells the app only about ten times a second; each of those re-placed the picture, held against the screen, through three calls to the system's wallpaper service, a script into the page and a redraw, so the picture jumped behind a window that glides. A move held by the mouse now leaves the picture where it is on the window and places it once the button is up. A resize (the size changed) still follows every step, which the user finds smooth.
    fn follow_window_bounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let held_move = self.glass.size == Some(window.bounds().size)
            && crate::app::ghostex_capture::platform::primary_mouse_button_down();
        if !held_move {
            self.sync_page_glass(false, window, cx);
            return;
        }
        if self.glass.move_end.is_some() {
            return;
        }
        self.glass.move_end = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(MOVE_END_CHECK_EVERY).await;
                if !crate::app::ghostex_capture::platform::primary_mouse_button_down() {
                    break;
                }
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.glass.move_end = None;
                this.sync_page_glass(false, window, cx);
            });
        }));
    }

    /// `keep`: the look also goes into the documents the page loads later (a reload, Refresh);
    /// otherwise only into the current one.
    fn sync_page_glass(&mut self, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        self.glass.size = Some(window.bounds().size);
        let light = crate::app::helpers::chrome_uses_light_appearance();
        let controls_on_left = crate::app::window::round_close_button::close_buttons_on_left();
        let glass = page_glass(window, cx);
        let jpeg = glass
            .as_ref()
            .map(|glass| glass.jpeg.clone())
            .unwrap_or_default();
        let picture_same = self.glass.picture.as_ref() == Some(&jpeg);
        // Page scripts run in the current document and in every one it loads later, in the order
        // they were added, so a reload ends on the latest of each.
        if !picture_same {
            surface.read(cx).add_page_script(&picture_script(&jpeg));
            self.glass.picture = Some(jpeg);
        }
        let look = look_script(
            if light { "light" } else { "dark" },
            controls_on_left,
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
        let backdrop = glass.and_then(|glass| {
            let image = match &self.glass.backdrop {
                Some(backdrop) if picture_same => backdrop.image.clone(),
                _ => decode_backdrop_image(&glass.jpeg)?,
            };
            Some(Backdrop {
                image,
                rect: glass.rect,
                tint: glass.tint,
            })
        });
        let changed = match (&self.glass.backdrop, &backdrop) {
            (Some(old), Some(new)) => {
                old.rect != new.rect || old.tint != new.tint || !Arc::ptr_eq(&old.image, &new.image)
            }
            (None, None) => false,
            _ => true,
        } || self.glass.light != light
            || self.glass.controls_on_left != controls_on_left;
        let replaced_image = match (&self.glass.backdrop, &backdrop) {
            (Some(old), Some(new)) if !Arc::ptr_eq(&old.image, &new.image) => {
                Some(old.image.clone())
            }
            (Some(old), None) => Some(old.image.clone()),
            _ => None,
        };
        self.glass.backdrop = backdrop;
        self.glass.light = light;
        self.glass.controls_on_left = controls_on_left;
        if let Some(old_image) = replaced_image {
            let _ = window.drop_image(old_image);
        }
        if changed {
            cx.notify();
        }
    }
}

/// Decodes a softened backdrop JPEG into a BGRA `RenderImage` so GPUI paints the header band
/// synchronously on the first frame instead of flashing a solid top colour while decoding.
fn decode_backdrop_image(jpeg: &[u8]) -> Option<Arc<gpui::RenderImage>> {
    let mut rgba = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
        rgba,
    )])))
}

/// The picture behind the main window's backdrop (Art, Your image, Animated, or Window
/// transparency's wallpaper/picture), where it lies in the page window `window`, and the tint
/// over it; `None` while the window is solid or uses a custom video.
fn page_glass(window: &Window, cx: &mut App) -> Option<Glass> {
    if !crate::app::helpers::window_glass_active() {
        return None;
    }
    let bounds = window.bounds();
    let display_width = window
        .display(cx)
        .map(|display| f32::from(display.bounds().size.width))
        .unwrap_or_else(|| f32::from(bounds.size.width).max(1280.0));
    let workspace = Rgba::from(crate::app::helpers::workspace_background_color());
    let work_alpha = crate::app::helpers::work_area_glass_alpha();

    // Art (built-in artwork or Your image): painted by GPUI inside an opaque window rather than
    // through the platform wallpaper layer. Soften it by `PICTURE_BLUR_POINTS` on top of the main
    // window's art blur, and combine `windowArtDim` with the work area's tint so dark and light
    // text stay as readable as in the chat behind it. Never fall through to the desktop wallpaper
    // while Art is active.
    if crate::app::helpers::window_art_active() {
        let art = crate::app::helpers::window_art_snapshot(cx)?;
        let jpeg = softened(&art.jpeg, display_width);
        let tint = art_tint(workspace, work_alpha, art.dim, art.light);
        return Some(Glass {
            jpeg,
            rect: picture_rect(bounds.size, art.size, None),
            tint,
        });
    }

    let tint = Rgba {
        a: work_alpha,
        ..workspace
    };

    // Animated (Live): render a snapshot of the active animated style in the current theme's
    // colours and brightness and soften it like Wallpaper mode. Custom video keeps dusk/dawn.
    if crate::app::helpers::window_glass_live::window_glass_live_mode() {
        let live = crate::app::helpers::window_glass_live::window_glass_live()?;
        let (raw_jpeg, art_size) = super::live_backdrop::render_snapshot(&live)?;
        let jpeg = softened(&raw_jpeg, display_width);
        return Some(Glass {
            jpeg,
            rect: picture_rect(bounds.size, art_size, None),
            tint,
        });
    }

    let (snapshot, main_bounds) = crate::app::helpers::main_window_handle(cx).and_then(|main| {
        main.update(cx, |_, window, _| {
            (
                window.background_wallpaper_snapshot(PICTURE_MAX_WIDTH),
                window.bounds(),
            )
        })
        .ok()
    })?;
    let desktop = desktop_picture_path(bounds.center());
    let source = picture_source(desktop.as_deref());
    let Some(snapshot) = snapshot else {
        return stand_in_glass(window, cx, tint, desktop.as_deref(), &source)
            .or_else(|| remembered_glass(bounds.size, tint, &source));
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
    remember_picture(&jpeg, &source);
    Some(Glass {
        jpeg,
        rect: picture_rect(bounds.size, snapshot.screen, on_screen),
        tint,
    })
}

/// Combines the artwork dimming wash (`dim` of black in dark mode, white in light mode) with the
/// work area's tint (`workspace` at `work_alpha`) into one equivalent Source-Over tint layer.
/// Keeping the dimming in the tint layer rather than baking it into the JPEG also lets the reader
/// flip the page's Dark / Light switch to the other appearance cleanly.
fn art_tint(workspace: Rgba, work_alpha: f32, dim: f32, light: bool) -> Rgba {
    let w = work_alpha.clamp(0.0, 1.0);
    let d = dim.clamp(0.0, 1.0);
    let alpha = 1.0 - (1.0 - w) * (1.0 - d);
    if alpha <= f32::EPSILON {
        return Rgba {
            r: workspace.r,
            g: workspace.g,
            b: workspace.b,
            a: 0.0,
        };
    }
    let wash = if light { 1.0 } else { 0.0 };
    let dim_weight = (1.0 - w) * d;
    Rgba {
        r: (workspace.r * w + wash * dim_weight) / alpha,
        g: (workspace.g * w + wash * dim_weight) / alpha,
        b: (workspace.b * w + wash * dim_weight) / alpha,
        a: alpha,
    }
}

/// What the page window's picture is a picture of: the custom picture the glass shows, else the
/// desktop picture macOS names for the page window's display (`desktop`), else nothing. The kept
/// picture is used only for the same one (`remembered_glass`).
fn picture_source(desktop: Option<&std::path::Path>) -> String {
    let (kind, path) = match crate::app::helpers::window_glass_custom_image() {
        Some(custom) => ("custom", custom),
        None => match desktop {
            Some(desktop) => ("desktop", desktop.to_path_buf()),
            None => return String::new(),
        },
    };
    format!("{kind}|{}", path.display())
}

/// CDXC:SessionChat 2026-11-16 DECISION:
/// User: "why dont i see the bg show here in the ghostex page pls? we implemented this before but i dont see it now anymore". The page window's picture comes from the main window's snapshot of the desktop picture, and that snapshot is `None` whenever macOS names a desktop picture file that cannot be opened. That is what happened on 2026-11-16: a wallpaper changer left the desktop on `$TMPDIR/StaticWallpaper/<n>_static.jpg`, which was gone by the afternoon (the system log showed Ghostex's once-a-second check failing on it, "can't open ... fileExists == false"). So the last picture the page window was handed is kept (in memory, and in the cache folder for the next launch) and used while the desktop picture can't be read, placed covering the window; the real picture takes over again as soon as one can be read. A video or a live style is not this: there the glass shows no picture on purpose, so the page keeps the painted dusk or dawn.
/// CDXC:SessionChat 2026-11-20 WHY:
/// User: "why is this inline html using my old wallpaper? make it use my current one, at least on restart; I hate stale wallpaper, it's not matching". The kept picture was used for whatever desktop picture could not be read, including a different one set since, so a new wallpaper that Ghostex may not open showed the previous one, across restarts too. It is now kept with what it is a picture of (`picture_source`: the desktop picture's path, or the custom picture's) and used only while that is still what the glass shows.
fn remembered_glass(window_size: Size<Pixels>, tint: Rgba, source: &str) -> Option<Glass> {
    let (jpeg, screen) = remembered_picture(source)?;
    Some(Glass {
        jpeg,
        rect: picture_rect(window_size, screen, None),
        tint,
    })
}

/// CDXC:SessionChat 2026-11-17 WHY:
/// User: "why is this not showing the blurred bg like it used to??? it's inline html in the macos app". The desktop is a Wallspace video wallpaper. Wallspace plays the video itself and names a still of it, `$TMPDIR/StaticWallpaper/<key>_static.jpg`, as the macOS desktop picture, and that file was gone again (the system log showed the main window's once-a-second read failing on it, "fileExists == false"). The fallback of 2026-11-16 above only helps once a picture has been seen, and none had been since the app started, so the page window painted the flat dusk top (`PAGE_TOP_DARK`). While the desktop picture can't be read, the page now uses Wallspace's own poster of the wallpaper it is playing (`wallspace_poster`), which is the same scene, placed as the main window's picture would be. The kept picture still comes after it, for any other desktop picture that disappears.
/// CDXC:SessionChat 2026-11-20 WHY:
/// User: "why is this inline html using my old wallpaper? make it use my current one". The user had moved from Wallspace to iWallpaper, whose desktop picture lies in its own sandbox, which macOS will not let Ghostex open (the system log: "could not open ... com.macosgame.iwallpaper/Data/Documents/DesktopImage/<uuid>.png", "Operation not permitted"). The poster was picked from Wallspace's preferences, which still named Wallspace's last wallpaper, so the page showed that old scene. The poster is now taken only when the desktop picture is a Wallspace still, and by that still's own key; any other desktop picture Ghostex can't open is read off the screen instead (`captured_desktop`), when Ghostex already may read the screen.
fn stand_in_glass(
    window: &Window,
    cx: &App,
    tint: Rgba,
    desktop: Option<&std::path::Path>,
    source: &str,
) -> Option<Glass> {
    let bounds = window.bounds();
    let display = window.display(cx)?.bounds();
    let jpeg = desktop
        .and_then(wallspace_poster)
        .and_then(|poster| stand_in_picture(&poster, display.size))
        .or_else(|| captured_desktop(display, source))?;
    remember_picture(&jpeg, source);
    let on_screen = (crate::app::helpers::window_glass_shows_live_blur()
        || crate::app::helpers::window_glass_picture_follows_screen())
    .then(|| bounds.origin - display.origin);
    Some(Glass {
        jpeg,
        rect: picture_rect(bounds.size, display.size, on_screen),
        tint,
    })
}

/// The path of the desktop picture macOS names for the display that holds `point` (GPUI's global
/// space), whether or not Ghostex may open it.
#[cfg(target_os = "macos")]
fn desktop_picture_path(point: Point<Pixels>) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStrExt as _;
    unsafe extern "C" {
        fn GhostexGpuiDesktopPicturePath(
            x: f64,
            y: f64,
            out: *mut std::ffi::c_char,
            capacity: usize,
        ) -> usize;
    }
    let mut out = vec![0u8; 4096];
    let length = unsafe {
        GhostexGpuiDesktopPicturePath(
            f32::from(point.x) as f64,
            f32::from(point.y) as f64,
            out.as_mut_ptr().cast(),
            out.len(),
        )
    };
    (length > 0).then(|| std::ffi::OsStr::from_bytes(&out[..length.min(out.len())]).into())
}

#[cfg(not(target_os = "macos"))]
fn desktop_picture_path(_point: Point<Pixels>) -> Option<std::path::PathBuf> {
    None
}

/// Counts the page windows opened, so each one reads the screen afresh (`captured_desktop`).
static PAGE_WINDOWS_OPENED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long a picture read off the screen is used before it is read again while a page window
/// stays open: a wallpaper app may change its picture without a new name.
const CAPTURE_KEPT_FOR: Duration = Duration::from_secs(30);

/// The desktop of the display at `display` as it is on screen, cut to the display's shape and
/// softened like the main window's picture (`softened`); `None` when Ghostex may not read the
/// screen (it never asks) or on another system. Read again for each page window, when the desktop
/// picture (`source`) or the display changes, and every `CAPTURE_KEPT_FOR`.
fn captured_desktop(display: Bounds<Pixels>, source: &str) -> Option<Vec<u8>> {
    type Key = (String, [u32; 4], u64);
    static LAST: std::sync::Mutex<Option<(Key, std::time::Instant, Vec<u8>)>> =
        std::sync::Mutex::new(None);
    let key: Key = (
        source.to_string(),
        [
            f32::from(display.origin.x).to_bits(),
            f32::from(display.origin.y).to_bits(),
            f32::from(display.size.width).to_bits(),
            f32::from(display.size.height).to_bits(),
        ],
        PAGE_WINDOWS_OPENED.load(std::sync::atomic::Ordering::Relaxed),
    );
    if let Ok(last) = LAST.lock()
        && let Some((last_key, taken, soft)) = last.as_ref()
        && *last_key == key
        && taken.elapsed() < CAPTURE_KEPT_FOR
    {
        return Some(soft.clone());
    }
    let center = display.center();
    let capture = screen_capture_jpeg(f32::from(center.x) as f64, f32::from(center.y) as f64)?;
    let jpeg = screen_shaped_jpeg(&capture, display.size)?;
    let soft = softened(&jpeg, f32::from(display.size.width));
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, std::time::Instant::now(), soft.clone()));
    }
    Some(soft)
}

/// What the desktop of the display holding (`x`, `y`) shows, as a JPEG (the native shim's
/// `GhostexGpuiDesktopCaptureJpeg`).
#[cfg(target_os = "macos")]
fn screen_capture_jpeg(x: f64, y: f64) -> Option<Vec<u8>> {
    unsafe extern "C" {
        fn GhostexGpuiDesktopCaptureJpeg(x: f64, y: f64, length: *mut usize) -> *mut u8;
        fn GhostexGpuiFreeBytes(bytes: *mut u8);
    }
    let mut length = 0usize;
    let bytes = unsafe { GhostexGpuiDesktopCaptureJpeg(x, y, &mut length) };
    if bytes.is_null() {
        return None;
    }
    let jpeg = unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec();
    unsafe { GhostexGpuiFreeBytes(bytes) };
    Some(jpeg)
}

#[cfg(not(target_os = "macos"))]
fn screen_capture_jpeg(_x: f64, _y: f64) -> Option<Vec<u8>> {
    None
}

/// Wallspace's poster of the wallpaper whose still is the desktop picture `desktop`
/// (`<key>_static.jpg` in its `StaticWallpaper` folder), when there is one.
#[cfg(target_os = "macos")]
fn wallspace_poster(desktop: &std::path::Path) -> Option<std::path::PathBuf> {
    let key = wallspace_key(desktop)?;
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let poster = home
        .join("Library/Caches/Wallspace/images/posters")
        .join(format!("{key}.webp"));
    poster.is_file().then_some(poster)
}

#[cfg(not(target_os = "macos"))]
fn wallspace_poster(_desktop: &std::path::Path) -> Option<std::path::PathBuf> {
    None
}

/// The wallpaper's key in the name of Wallspace's still `desktop`, `StaticWallpaper/<key>_static.*`.
/// Only digits, letters, `-` and `_`, since it names a file.
fn wallspace_key(desktop: &std::path::Path) -> Option<String> {
    let folder = desktop.parent()?.file_name()?.to_str()?;
    if folder != "StaticWallpaper" {
        return None;
    }
    let key = desktop.file_stem()?.to_str()?.strip_suffix("_static")?;
    (!key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
    .then(|| key.to_string())
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

/// The last picture the page window was handed, what it is a picture of (`picture_source`) and
/// its size (only its shape matters, to cover the window); read from the cache folder once per
/// launch.
struct Remembered {
    jpeg: Vec<u8>,
    source: String,
    screen: Size<Pixels>,
}

static REMEMBERED: std::sync::Mutex<Option<Option<Remembered>>> = std::sync::Mutex::new(None);

fn remembered_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .cache_dir
        .join("page-window-glass.jpg")
}

/// Where the kept picture's `source` is written, beside it. A picture kept without one (before
/// 2026-11-20) belongs to no source, so it is never shown.
fn remembered_source_path() -> std::path::PathBuf {
    remembered_path().with_extension("source")
}

/// Keeps `jpeg`, a picture of `source`, for the times that can't be read; written only when it
/// changed.
fn remember_picture(jpeg: &[u8], source: &str) {
    let Ok(mut remembered) = REMEMBERED.lock() else {
        return;
    };
    if matches!(&*remembered, Some(Some(known)) if known.jpeg == jpeg && known.source == source) {
        return;
    }
    let Some(screen) = picture_size(jpeg) else {
        return;
    };
    *remembered = Some(Some(Remembered {
        jpeg: jpeg.to_vec(),
        source: source.to_string(),
        screen,
    }));
    drop(remembered);
    let path = remembered_path();
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    for (path, bytes) in [
        (path.clone(), jpeg),
        (remembered_source_path(), source.as_bytes()),
    ] {
        let partial = path.with_extension("partial");
        if std::fs::write(&partial, bytes).is_ok() {
            let _ = std::fs::rename(&partial, &path);
        }
    }
}

/// The kept picture, from memory or else from the cache folder, while it is a picture of
/// `source`.
fn remembered_picture(source: &str) -> Option<(Vec<u8>, Size<Pixels>)> {
    if source.is_empty() {
        return None;
    }
    let mut remembered = REMEMBERED.lock().ok()?;
    let known = remembered.get_or_insert_with(|| {
        let source = std::fs::read_to_string(remembered_source_path()).ok()?;
        let jpeg = std::fs::read(remembered_path()).ok()?;
        let screen = picture_size(&jpeg)?;
        Some(Remembered {
            jpeg,
            source,
            screen,
        })
    });
    known
        .as_ref()
        .filter(|known| known.source == source)
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
/// which side of the band holds the window controls and their inset (`CONTROL_INSET`), and with a
/// picture where it lies (`rect`, in the window's coordinates; the page starts under the header)
/// and the tint for that theme; the other theme's tint is left to the page's default.
fn look_script(
    theme: &str,
    controls_on_left: bool,
    glass: Option<(Bounds<Pixels>, Rgba)>,
) -> String {
    let other = if theme == "light" { "dark" } else { "light" };
    let controls = controls_side_attr(controls_on_left);
    let controls_inset = css_px(px(CONTROL_INSET));
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
        "var s=d.style;d.setAttribute(\"data-gx-controls\",\"{controls}\");\
s.setProperty(\"--gx-controls-inset\",\"{controls_inset}px\");{glass}\
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
            false,
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

    #[test]
    fn wallspace_names_its_wallpaper_by_the_key_in_its_still() {
        use std::path::Path;
        let still = Path::new("/var/folders/x/T/StaticWallpaper/816347559438864_static.jpg");
        assert_eq!(wallspace_key(still).as_deref(), Some("816347559438864"));
        // Another app's desktop picture is not Wallspace's, whatever Wallspace last played.
        let other = Path::new(
            "/Users/me/Library/Containers/com.macosgame.iwallpaper/Data/Documents/DesktopImage/D270FA36.png",
        );
        assert_eq!(wallspace_key(other), None);
        assert_eq!(
            wallspace_key(Path::new("/Pictures/816347559438864_static.jpg")),
            None
        );
        // A key that would leave the posters folder is refused.
        assert_eq!(
            wallspace_key(Path::new("/T/StaticWallpaper/.._static.jpg")),
            None
        );
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
            look_script("light", true, None),
        ] {
            assert!(script.contains("var d=document.documentElement;if(d){run(d);return;}"));
            assert!(script.contains("new MutationObserver"));
            // Nothing touches `<html>` outside the function that is handed it.
            let outside = script.split_once("function run(d){").map(|(head, _)| head);
            assert_eq!(outside, Some("(function(){"));
        }
    }
}
