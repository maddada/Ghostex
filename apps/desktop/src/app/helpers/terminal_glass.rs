//! Terminal background opacity under window glass (`terminalBackgroundOpacity`): while it is below
//! 100%, terminal panes are cut out of the work area's glass tint and paint their own, thinner
//! share of it, so more of what is behind the window shows through them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Bounds, Hsla, IntoElement, Pixels, Styled as _, Window, canvas, fill, point, px};

use crate::app::helpers::*;

/// CDXC:Terminal 2026-10-14 DECISION:
/// User: "why is the terminal transparent on macos and linux i believe but not on windows? (i mean ghostty terminal) also can we add settings about this please in settings?", with Ghostty's `background-opacity = 0.75` and a background blur as the example. Terminals were see-through only through window glass (Enable transparency, which defaults to Never on Windows), never through Ghostty's `background-opacity`. The user then chose: Settings > Terminal > Background opacity applies while Enable transparency is on, on all three platforms; terminals are cut out of the work area's glass tint and painted at the work area's strength times this percent, so 100% looks like before; while transparency is off the row is disabled with a note and a Turn on transparency button. Background blur is the window's Blur (`windowGlassBlurRadius`) shown again under Settings > Terminal, because a window has one blur.
/// CDXC:Settings 2026-11-14 DECISION (AGENTS.md; Q3110, user: "fix all these too but make sure ux makes sense after fix"): supersedes the disabled row above. While transparency is off, Settings > Terminal shows a Transparency switch (on picks Dark only) and hides Background opacity, which shows as its ↳ row once transparency is on (settings_modal/tabs/general/terminal_transparency.rs).
static TERMINAL_GLASS_PERCENT: AtomicU8 = AtomicU8::new(100);

/// Reads `terminalBackgroundOpacity`: whole percent from 0 to 100, 100 when unset.
pub(crate) fn refresh_terminal_glass(object: &serde_json::Map<String, serde_json::Value>) {
    let percent = object
        .get("terminalBackgroundOpacity")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .map_or(100, |value| value.clamp(0.0, 100.0).round() as u8);
    TERMINAL_GLASS_PERCENT.store(percent, Ordering::Relaxed);
}

/// Whether terminal panes are cut out of the work area's glass tint this frame. At 100% they are
/// not: the column's single tint already is the terminals' background.
pub(crate) fn terminal_glass_cutout_active() -> bool {
    window_glass_active() && TERMINAL_GLASS_PERCENT.load(Ordering::Relaxed) < 100
}

/// The tint a terminal pane paints over the blurred desktop while it is cut out.
fn terminal_glass_fill() -> Hsla {
    let percent = f32::from(TERMINAL_GLASS_PERCENT.load(Ordering::Relaxed)) / 100.0;
    workspace_background_color().opacity(work_area_glass_alpha() * percent)
}

thread_local! {
    /// The terminal panes laid out this frame, by window id. The column's tint creates its window's
    /// list in prepaint and takes it in paint; GPUI prepaints the whole tree before it paints any of
    /// it, so the panes that follow the tint have recorded themselves by then.
    static TERMINAL_GLASS_HOLES: RefCell<HashMap<u64, Vec<Bounds<Pixels>>>> =
        RefCell::new(HashMap::new());
}

fn window_key(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

/// `bounds` with its edges on device pixels, so a pane's fill and the column tint around it meet
/// without an antialiased seam.
fn snap_to_device_pixels(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    let snap = |value: Pixels| px((value.as_f32() * scale).round() / scale);
    Bounds::from_corners(
        point(snap(bounds.left()), snap(bounds.top())),
        point(snap(bounds.right()), snap(bounds.bottom())),
    )
}

/// The parts of `outer` no hole covers, as horizontal bands split around each hole.
fn bounds_outside_holes(outer: Bounds<Pixels>, holes: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let (left, top, right, bottom) = (
        outer.left().as_f32(),
        outer.top().as_f32(),
        outer.right().as_f32(),
        outer.bottom().as_f32(),
    );
    let holes: Vec<[f32; 4]> = holes
        .iter()
        .map(|hole| {
            [
                hole.left().as_f32().max(left),
                hole.top().as_f32().max(top),
                hole.right().as_f32().min(right),
                hole.bottom().as_f32().min(bottom),
            ]
        })
        .filter(|[l, t, r, b]| r > l && b > t)
        .collect();
    let rect = |l: f32, t: f32, r: f32, b: f32| {
        Bounds::from_corners(point(px(l), px(t)), point(px(r), px(b)))
    };
    if holes.is_empty() {
        return vec![outer];
    }
    let mut ys: Vec<f32> = vec![top, bottom];
    ys.extend(holes.iter().flat_map(|[_, t, _, b]| [*t, *b]));
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut parts = Vec::new();
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        if y1 <= y0 {
            continue;
        }
        let mut spans: Vec<(f32, f32)> = holes
            .iter()
            .filter(|[_, t, _, b]| *t <= y0 && *b >= y1)
            .map(|[l, _, r, _]| (*l, *r))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = left;
        for (l, r) in spans {
            if l > x {
                parts.push(rect(x, y0, l, y1));
            }
            x = x.max(r);
        }
        if x < right {
            parts.push(rect(x, y0, right, y1));
        }
    }
    parts
}

/// The work area column's glass tint while terminals are cut out of it: painted everywhere in the
/// column except the terminal panes. Must be the column's first child.
pub(crate) fn workspace_column_glass_tint() -> impl IntoElement {
    canvas(
        |bounds, window, _cx| {
            TERMINAL_GLASS_HOLES.with(|holes| {
                holes.borrow_mut().insert(window_key(window), Vec::new());
            });
            snap_to_device_pixels(bounds, window.scale_factor())
        },
        |_, bounds, window, _cx| {
            let holes = TERMINAL_GLASS_HOLES
                .with(|holes| holes.borrow_mut().remove(&window_key(window)))
                .unwrap_or_default();
            let tint = workspace_column_background();
            for part in bounds_outside_holes(bounds, &holes) {
                window.paint_quad(fill(part, tint));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A terminal pane's own share of the glass tint while terminals are cut out: it records the
/// pane's rectangle for the column's tint to leave out and paints `terminal_glass_fill` there.
/// Must be the first child of the pane (Agents) or body (command pane), so everything in it paints
/// over it.
pub(crate) fn terminal_glass_pane_fill() -> impl IntoElement {
    canvas(
        |bounds, window, _cx| {
            let bounds = snap_to_device_pixels(bounds, window.scale_factor());
            // A window whose column did not cut terminals out this frame keeps its whole tint, so
            // the pane adds nothing over it.
            TERMINAL_GLASS_HOLES.with(|holes| {
                holes.borrow_mut().get_mut(&window_key(window)).map(|list| {
                    list.push(bounds);
                    bounds
                })
            })
        },
        |_, bounds, window, _cx| {
            if let Some(bounds) = bounds {
                window.paint_quad(fill(bounds, terminal_glass_fill()));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
