//! Where the page window opens: where the reader last left it and at the size they last gave it,
//! held against the main window (it moves with the main window as its child window), else centred
//! on the main window at the chat's preview size.
//!
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User: "also want to be able to resize and move them please". The page window stays a real window of its own, a child window of the main window: the page is a native browser view that nothing drawn inside the main window could sit above, and a window of its own is what can be dragged and resized by the system. Its header moves it (`window_drag_region`, double-click zooms), its edges and corners resize it (`ghostex_capture::platform::make_resizable`, as the Capture editor's window is, since GPUI gives a window without a title bar no resize edges on macOS), and the next page opens where the last one was left. The frame is kept as an offset from the main window's corner and a size, in `gpui-visual-page-window.json`, so it survives restarts and follows the main window to another place or screen.

use gpui::{Bounds, Pixels, Size, point, px, size};
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

/// Where the next page window opens over a main window at `main`: where the last one was left,
/// else centred at `default_size`.
pub(crate) fn opening_frame(main: Bounds<Pixels>, default_size: Size<Pixels>) -> Bounds<Pixels> {
    place(main, default_size, kept())
}

fn place(main: Bounds<Pixels>, default_size: Size<Pixels>, kept: Option<Kept>) -> Bounds<Pixels> {
    let centred = Bounds::centered_at(main.center(), default_size);
    let Some(kept) = kept else {
        return centred;
    };
    let frame = Bounds::new(
        point(main.origin.x + px(kept.x), main.origin.y + px(kept.y)),
        size(
            px(kept.width).max(MIN_SIZE.width),
            px(kept.height).max(MIN_SIZE.height),
        ),
    );
    // A frame left mostly off the main window (which has since been made smaller) opens centred
    // at its kept size instead, so it is never lost off screen.
    let shown = frame.intersect(&main);
    if f32::from(shown.size.width) < 120.0 || f32::from(shown.size.height) < 80.0 {
        return Bounds::centered_at(main.center(), frame.size);
    }
    frame
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

    #[test]
    fn a_first_page_window_opens_centred_on_the_main_window() {
        let frame = place(main_window(), size(px(800.0), px(600.0)), None);
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
        let frame = place(main_window(), size(px(800.0), px(600.0)), Some(kept));
        assert_eq!(frame.origin, point(px(140.0), px(80.0)));
        assert_eq!(frame.size, size(px(700.0), px(500.0)));
        // The main window moved: the page window keeps its place on it.
        let moved = Bounds::new(point(px(600.0), px(300.0)), main_window().size);
        let frame = place(moved, size(px(800.0), px(600.0)), Some(kept));
        assert_eq!(frame.origin, point(px(640.0), px(330.0)));
    }

    #[test]
    fn a_page_window_left_off_the_main_window_opens_centred_at_its_size() {
        let kept = Kept {
            x: 2000.0,
            y: 0.0,
            width: 700.0,
            height: 500.0,
        };
        let frame = place(main_window(), size(px(800.0), px(600.0)), Some(kept));
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
        let frame = place(main_window(), size(px(800.0), px(600.0)), Some(kept));
        assert_eq!(frame.size, MIN_SIZE);
    }
}
