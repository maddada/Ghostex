//! Where the page window opens: where the reader last left it and at the size they last gave it,
//! held against the main window (it moves with the main window as its child window), else centred
//! on the main window at the chat's preview size. Either way it is kept reachable on a display that
//! exists now (`reachable`).
//!
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User: "also want to be able to resize and move them please". The page window stays a real window of its own, a child window of the main window: the page is a native browser view that nothing drawn inside the main window could sit above, and a window of its own is what can be dragged and resized by the system. Its header moves it (`window_drag_region`, double-click zooms), its edges and corners resize it (`ghostex_capture::platform::make_resizable`, as the Capture editor's window is, since GPUI gives a window without a title bar no resize edges on macOS), and the next page opens where the last one was left. The frame is kept as an offset from the main window's corner and a size, in `gpui-visual-page-window.json`, so it survives restarts and follows the main window to another place or screen.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! GitHub issue #560: a page window dragged from a MacBook's screen to an external display "snaps back to the built-in display, and it lands almost entirely above the top edge of the screen. Only about 50px of its bottom edge is visible", so it could only be closed by clicking away. With "Displays have separate Spaces" on (the macOS default) a child window cannot be shown on a display other than its parent's: macOS keeps it in the main window's Space at the frame it was dropped at, which on that screen is mostly off its top. The kept offset was also measured wrongly across displays, because GPUI's macOS bounds count from each window's own display. Frames are now measured in the space all displays share, and a frame is used only while its top band (the drag area and the round Close, Open in browser and Refresh buttons) lies whole on a display the window can be shown on (with separate Spaces, only the main window's) at a size that display holds; otherwise the window goes centred on the main window, at its size clamped to the main window's display. The same rule puts the window back once a drag ends somewhere it cannot stay, and keeps a remembered size larger than the display it opens on from running off it.

use gpui::{App, Bounds, Pixels, Size, point, px, size};
use std::sync::Mutex;

/// The smallest the page window can be made.
pub(crate) const MIN_SIZE: Size<Pixels> = Size {
    width: px(420.0),
    height: px(300.0),
};

/// The page window's frame as last left: its top-left corner from the main window's, and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kept {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// `None` until read from disk; then the kept frame, if any.
static KEPT: Mutex<Option<Option<Kept>>> = Mutex::new(None);

fn kept_path() -> std::path::PathBuf {
    crate::app::helpers::ghostex_state_root().join("gpui-visual-page-window.json")
}

fn read_kept() -> Option<Kept> {
    let text = std::fs::read_to_string(kept_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let number = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| value as f32)
    };
    Some(Kept {
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    })
}

fn kept() -> Option<Kept> {
    let mut slot = KEPT.lock().ok()?;
    *slot.get_or_insert_with(read_kept)
}

/// The displays the page window can be shown on, by their visible frames (no menu bar or Dock) in
/// the space all displays share.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Screens {
    /// The main window's display, where a window that cannot stay where it is goes.
    home: Bounds<Pixels>,
    /// Every display the window can sit on: only `home` while displays have separate Spaces.
    shown_on: Vec<Bounds<Pixels>>,
}

impl Screens {
    /// The displays now, for a main window at `main` (in the shared space).
    pub(crate) fn now(main: Bounds<Pixels>, cx: &App) -> Self {
        let frames: Vec<Bounds<Pixels>> = cx
            .displays()
            .into_iter()
            .map(|display| crate::app::window::popup_frame::global_display_frames(&*display).1)
            .collect();
        let home = frames
            .iter()
            .find(|frame| frame.contains(&main.center()))
            .or_else(|| {
                frames.iter().max_by(|a, b| {
                    let area = |frame: &Bounds<Pixels>| {
                        let shown = frame.intersect(&main).size;
                        f32::from(shown.width) * f32::from(shown.height)
                    };
                    area(a).total_cmp(&area(b))
                })
            })
            .copied()
            .unwrap_or(main);
        let shown_on = if crate::app::ghostex_capture::platform::screens_have_separate_spaces() {
            vec![home]
        } else {
            frames
        };
        Self { home, shown_on }
    }
}

/// `window`'s frame in the space all displays share. GPUI's macOS bounds count from the window's
/// own display, so they are read from AppKit there; elsewhere GPUI's bounds are already shared.
pub(crate) fn shared_frame(window: &gpui::Window) -> Bounds<Pixels> {
    crate::app::helpers::cef_parent_native_view(window)
        .ok()
        .and_then(|native| crate::app::ghostex_capture::platform::window_frame(native as usize))
        .unwrap_or_else(|| window.bounds())
}

/// Where the next page window opens over a main window at `main` (in the shared space): where the
/// last one was left, else centred at `default_size`, kept reachable on `screens`.
pub(crate) fn opening_frame(
    main: Bounds<Pixels>,
    default_size: Size<Pixels>,
    screens: &Screens,
) -> Bounds<Pixels> {
    place(main, default_size, kept(), screens)
}

/// Where a page window now at `frame` has to go to be reachable, when it cannot stay there (a drag
/// that ended on another display or off the top of the screen).
pub(crate) fn settle(
    main: Bounds<Pixels>,
    frame: Bounds<Pixels>,
    screens: &Screens,
) -> Option<Bounds<Pixels>> {
    let reachable = reachable(main, frame, screens);
    (reachable != frame).then_some(reachable)
}

fn place(
    main: Bounds<Pixels>,
    default_size: Size<Pixels>,
    kept: Option<Kept>,
    screens: &Screens,
) -> Bounds<Pixels> {
    let Some(kept) = kept else {
        return centred(main, default_size, screens.home);
    };
    let frame = Bounds::new(
        point(main.origin.x + px(kept.x), main.origin.y + px(kept.y)),
        size(
            px(kept.width).max(MIN_SIZE.width),
            px(kept.height).max(MIN_SIZE.height),
        ),
    );
    reachable(main, frame, screens)
}

/// `frame` when its top band lies whole on a display it can be shown on, made to fit that display
/// when it is larger; else centred on the main window on its display.
fn reachable(main: Bounds<Pixels>, frame: Bounds<Pixels>, screens: &Screens) -> Bounds<Pixels> {
    let band = Bounds::new(
        frame.origin,
        size(frame.size.width, px(super::TOP_BAND_HEIGHT)),
    );
    match screens
        .shown_on
        .iter()
        .find(|display| display.intersect(&band) == band)
    {
        Some(display) => fit(frame, *display),
        None => centred(main, frame.size, screens.home),
    }
}

/// `size` (clamped to `display`) centred on the main window, moved onto `display` where it would
/// run off it.
fn centred(main: Bounds<Pixels>, size: Size<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    let size = size.min(&display.size);
    inside(Bounds::centered_at(main.center(), size), display)
}

/// `frame` as it is when `display` holds its size, else shrunk to `display` and moved onto it.
fn fit(frame: Bounds<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    if frame.size.width <= display.size.width && frame.size.height <= display.size.height {
        return frame;
    }
    inside(
        Bounds::new(frame.origin, frame.size.min(&display.size)),
        display,
    )
}

/// `frame` moved the least that puts it whole on `display`; it is no larger than `display`.
fn inside(frame: Bounds<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    let x = frame
        .origin
        .x
        .min(display.origin.x + display.size.width - frame.size.width)
        .max(display.origin.x);
    let y = frame
        .origin
        .y
        .min(display.origin.y + display.size.height - frame.size.height)
        .max(display.origin.y);
    Bounds::new(point(x, y), frame.size)
}

/// Keeps `frame`, the page window's frame now, against the main window's `main`. Returns whether
/// it changed, for the caller to save it.
pub(crate) fn keep(main: Bounds<Pixels>, frame: Bounds<Pixels>) -> bool {
    let next = Kept {
        x: f32::from(frame.origin.x - main.origin.x).round(),
        y: f32::from(frame.origin.y - main.origin.y).round(),
        width: f32::from(frame.size.width).round(),
        height: f32::from(frame.size.height).round(),
    };
    let Ok(mut slot) = KEPT.lock() else {
        return false;
    };
    if *slot == Some(Some(next)) {
        return false;
    }
    *slot = Some(Some(next));
    true
}

/// Writes the kept frame to disk.
pub(crate) fn save() {
    let Some(kept) = KEPT.lock().ok().and_then(|slot| slot.flatten()) else {
        return;
    };
    let _ = std::fs::write(
        kept_path(),
        serde_json::json!({
            "x": kept.x,
            "y": kept.y,
            "width": kept.width,
            "height": kept.height,
        })
        .to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main_window() -> Bounds<Pixels> {
        Bounds::new(point(px(100.0), px(50.0)), size(px(1400.0), px(900.0)))
    }

    /// A 1600x1000 display holding the main window, and one beside it.
    fn screens() -> Screens {
        let home = Bounds::new(point(px(0.0), px(0.0)), size(px(1600.0), px(1000.0)));
        let beside = Bounds::new(point(px(1600.0), px(0.0)), size(px(1600.0), px(1000.0)));
        Screens {
            home,
            shown_on: vec![home, beside],
        }
    }

    #[test]
    fn a_first_page_window_opens_centred_on_the_main_window() {
        let frame = place(main_window(), size(px(800.0), px(600.0)), None, &screens());
        assert_eq!(frame.center(), main_window().center());
        assert_eq!(frame.size, size(px(800.0), px(600.0)));
    }

    #[test]
    fn the_next_page_window_opens_where_the_last_was_left_against_the_main_window() {
        let kept = Kept {
            x: 40.0,
            y: 30.0,
            width: 700.0,
            height: 500.0,
        };
        let frame = place(
            main_window(),
            size(px(800.0), px(600.0)),
            Some(kept),
            &screens(),
        );
        assert_eq!(frame.origin, point(px(140.0), px(80.0)));
        assert_eq!(frame.size, size(px(700.0), px(500.0)));
        // The main window moved: the page window keeps its place on it.
        let moved = Bounds::new(point(px(600.0), px(300.0)), main_window().size);
        let frame = place(moved, size(px(800.0), px(600.0)), Some(kept), &screens());
        assert_eq!(frame.origin, point(px(640.0), px(330.0)));
    }

    #[test]
    fn a_page_window_left_off_the_main_window_opens_centred_at_its_size() {
        let kept = Kept {
            x: 4000.0,
            y: 0.0,
            width: 700.0,
            height: 500.0,
        };
        let frame = place(
            main_window(),
            size(px(800.0), px(600.0)),
            Some(kept),
            &screens(),
        );
        assert_eq!(frame.center(), main_window().center());
        assert_eq!(frame.size, size(px(700.0), px(500.0)));
    }

    #[test]
    fn a_kept_frame_is_never_smaller_than_the_window_can_be_made() {
        let kept = Kept {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 40.0,
        };
        let frame = place(
            main_window(),
            size(px(800.0), px(600.0)),
            Some(kept),
            &screens(),
        );
        assert_eq!(frame.size, MIN_SIZE);
    }
}
