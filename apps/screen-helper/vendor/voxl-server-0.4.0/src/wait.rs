//! `GET /wait`: returns when the screen settles or changes, for agents and scripts that
//! would otherwise sleep after every action and hope it was long enough.
//!
//! - `GET /wait?idle=300`: once nothing has changed for 300 ms (the default). The quiet
//!   time counts from the request at the earliest, so what an action just did has time
//!   to show up.
//! - `GET /wait?change`: at the first change.
//! - `region=x,y,w,h` only counts changes in that rectangle (desktop pixels, to the
//!   16-pixel block), `timeout=ms` gives up (default 10 s), and `since=n` counts changes
//!   after an earlier `seq` (from `/wait` or a screenshot's `X-Voxl-Seq` header) instead of
//!   after the request, so a change that came before the request isn't missed.
//!
//! The answer is JSON: `changed` (anything changed since `since`), `timed_out`, `seq`
//! (pass it as `since` later) and `waited_ms`.
//!
//! The capture thread hears about every change anyway (XDamage, ScreenCaptureKit's and
//! DXGI's dirty rectangles), whether or not anyone is watching, so it notes when each
//! 16x16 block last changed and a wait needs no screenshots and no polling.

use crate::hub::Hub;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

const DEFAULT_IDLE: Duration = Duration::from_millis(300);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_WAIT: Duration = Duration::from_secs(600);

/// A change: its sequence number (counting up from 1) and when it happened.
type Mark = (u64, Instant);

/// When each part of the screen last changed.
pub struct Activity {
    map: Mutex<Map>,
    /// The latest change's sequence number; wakes waiting requests.
    seq: watch::Sender<u64>,
}

struct Map {
    width: u32,
    height: u32,
    cols: usize,
    /// Last change of each 16x16 block, row-major.
    blocks: Vec<Mark>,
    /// Latest change anywhere.
    last: Mark,
    /// The latest new capture (startup or a resize), which changed everything.
    reset: Mark,
}

impl Map {
    /// Blocks covering [x0, x1) x [y0, y1), clipped to the screen: (col0, col1, row0, row1).
    fn span(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> (usize, usize, usize, usize) {
        let (x1, y1) = (x1.min(self.width), y1.min(self.height));
        if x1 <= x0 || y1 <= y0 {
            return (0, 0, 0, 0);
        }
        (
            x0 as usize / 16,
            (x1 as usize).div_ceil(16),
            y0 as usize / 16,
            (y1 as usize).div_ceil(16),
        )
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Default for Activity {
    fn default() -> Self {
        let start = (0, Instant::now());
        Activity {
            map: Mutex::new(Map {
                width: 0,
                height: 0,
                cols: 0,
                blocks: Vec::new(),
                last: start,
                reset: start,
            }),
            seq: watch::channel(0).0,
        }
    }
}

impl Activity {
    /// A new capture (startup or a resize): everything on screen is new.
    pub fn reset(&self, width: u32, height: u32) {
        let mut m = self.map.lock();
        let now = (m.last.0 + 1, Instant::now());
        let cols = (width as usize).div_ceil(16);
        *m = Map {
            width,
            height,
            cols,
            blocks: vec![now; cols * (height as usize).div_ceil(16)],
            last: now,
            reset: now,
        };
        drop(m);
        self.seq.send_replace(now.0);
    }

    /// Areas that changed, as (x0, y0, x1, y1) in desktop pixels.
    pub fn note(&self, rects: &[(u32, u32, u32, u32)]) {
        let mut m = self.map.lock();
        let m = &mut *m;
        let now = (m.last.0 + 1, Instant::now());
        let mut any = false;
        for &(x0, y0, x1, y1) in rects {
            let (c0, c1, r0, r1) = m.span(x0, y0, x1, y1);
            for row in r0..r1 {
                m.blocks[row * m.cols + c0..row * m.cols + c1].fill(now);
                any = true;
            }
        }
        if any {
            m.last = now;
            self.seq.send_replace(now.0);
        }
    }

    /// Sequence number of the latest change.
    pub fn seq(&self) -> u64 {
        self.map.lock().last.0
    }

    /// The latest change in `region` (or anywhere) and the latest sequence number,
    /// read together: a `seq` handed out never covers a change the answer didn't see.
    fn latest(&self, region: Option<Rect>) -> (Mark, u64) {
        let m = self.map.lock();
        let Some(r) = region else {
            return (m.last, m.last.0);
        };
        let (c0, c1, r0, r1) = m.span(r.x, r.y, r.x.saturating_add(r.w), r.y.saturating_add(r.h));
        let mark = (r0..r1)
            .flat_map(|row| &m.blocks[row * m.cols + c0..row * m.cols + c1])
            .fold(m.reset, |a, &b| if b.0 > a.0 { b } else { a });
        (mark, m.last.0)
    }

    fn on_screen(&self, r: Rect) -> bool {
        let m = self.map.lock();
        m.span(r.x, r.y, r.x.saturating_add(r.w), r.y.saturating_add(r.h)) != (0, 0, 0, 0)
    }
}

fn json(code: StatusCode, body: serde_json::Value) -> Response {
    (
        code,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}

fn bad(msg: &str) -> Response {
    json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg }))
}

fn millis(q: &HashMap<String, String>, key: &str, default: Duration) -> Option<Duration> {
    match q.get(key) {
        None => Some(default),
        Some(v) => v
            .parse()
            .ok()
            .map(|ms| Duration::from_millis(ms).min(MAX_WAIT)),
    }
}

fn parse_region(v: &str) -> Option<Rect> {
    let n: Vec<u32> = v
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<_>>()?;
    match n[..] {
        [x, y, w, h] if w > 0 && h > 0 => Some(Rect { x, y, w, h }),
        _ => None,
    }
}

pub async fn wait(
    State(hub): State<Arc<Hub>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let start = Instant::now();
    let a = &hub.activity;
    let Some(idle) = millis(&q, "idle", DEFAULT_IDLE) else {
        return bad("idle must be a whole number of milliseconds");
    };
    let Some(timeout) = millis(&q, "timeout", DEFAULT_TIMEOUT) else {
        return bad("timeout must be a whole number of milliseconds");
    };
    let change = q
        .get("change")
        .is_some_and(|v| !matches!(v.as_str(), "0" | "false"));
    let region = match q.get("region") {
        None => None,
        Some(v) => match parse_region(v) {
            Some(r) if a.on_screen(r) => Some(r),
            Some(_) => return bad("region is off screen"),
            None => return bad("region must be x,y,w,h"),
        },
    };
    let since = match q.get("since") {
        None => a.seq(),
        Some(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => return bad("since must be a seq from an earlier /wait or screenshot"),
        },
    };
    let deadline = start + timeout;
    let mut rx = a.seq.subscribe();
    let timed_out = loop {
        // Mark the current change seen before looking, so a later one wakes us.
        rx.borrow_and_update();
        let ((seq, at), _) = a.latest(region);
        let now = Instant::now();
        let done_at = if change {
            (seq > since).then_some(now)
        } else {
            Some(at.max(start) + idle)
        };
        if done_at.is_some_and(|t| t <= now) {
            break false;
        }
        if now >= deadline {
            break true;
        }
        let wake = done_at.map_or(deadline, |t| t.min(deadline));
        tokio::select! {
            _ = rx.changed() => {}
            _ = tokio::time::sleep_until(wake.into()) => {}
        }
    };
    let ((latest, _), seq) = a.latest(region);
    json(
        StatusCode::OK,
        serde_json::json!({
            "changed": latest > since,
            "timed_out": timed_out,
            "seq": seq,
            "waited_ms": start.elapsed().as_millis() as u64,
        }),
    )
}
