//! What a session's own window keeps between runs, in `gpui-session-popups.json`: per session its
//! small and expanded frames, its Pin and whether it was expanded; per workspace window the
//! pop-ups it had open, which open again at launch.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use gpui::{Bounds, Pixels, point, px, size};
use serde::{Deserialize, Serialize};

use crate::app::helpers::ghostex_state_root;

/// How many sessions keep their frames; the ones used longest ago go first.
const MAX_SAVED_SESSIONS: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Frame {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// The display the frame is on; macOS reads a window's frame against its own display.
    #[serde(default)]
    display: Option<u64>,
}

impl Frame {
    fn from_saved(frame: SavedFrame) -> Self {
        Self {
            x: f32::from(frame.bounds.origin.x),
            y: f32::from(frame.bounds.origin.y),
            width: f32::from(frame.bounds.size.width),
            height: f32::from(frame.bounds.size.height),
            display: frame.display,
        }
    }

    fn saved(self) -> Option<SavedFrame> {
        let usable =
            self.width >= 1.0 && self.height >= 1.0 && self.x.is_finite() && self.y.is_finite();
        usable.then(|| SavedFrame {
            bounds: Bounds::new(
                point(px(self.x), px(self.y)),
                size(px(self.width), px(self.height)),
            ),
            display: self.display,
        })
    }
}

/// A frame as GPUI reports it (`Window::bounds`) and the display it is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SavedFrame {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) display: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SavedSessionRecord {
    small: Option<Frame>,
    full: Option<Frame>,
    expanded: bool,
    pinned: bool,
    /// When it was last changed (ms since the epoch), for the trim.
    at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SavedState {
    /// By the session's sidebar row id.
    sessions: BTreeMap<String, SavedSessionRecord>,
    /// By workspace window slot: the row ids of the pop-ups it had open.
    open: BTreeMap<String, Vec<String>>,
}

/// A session's saved pop-up, as the window code reads it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SavedSession {
    pub(super) small: Option<SavedFrame>,
    pub(super) full: Option<SavedFrame>,
    pub(super) expanded: bool,
    pub(super) pinned: bool,
}

fn path() -> PathBuf {
    ghostex_state_root().join("gpui-session-popups.json")
}

fn read() -> SavedState {
    fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(state: &SavedState) {
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(state) {
        let _ = fs::write(path, text);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

pub(super) fn session(row_id: &str) -> Option<SavedSession> {
    let state = read();
    let record = state.sessions.get(row_id)?;
    Some(SavedSession {
        small: record.small.and_then(Frame::saved),
        full: record.full.and_then(Frame::saved),
        expanded: record.expanded,
        pinned: record.pinned,
    })
}

/// Changes `row_id`'s record (made when missing) and writes the file when it changed.
pub(super) fn update_session(row_id: &str, change: impl FnOnce(&mut SavedSession)) {
    let mut state = read();
    let record = state.sessions.entry(row_id.to_string()).or_default();
    let mut session = SavedSession {
        small: record.small.and_then(Frame::saved),
        full: record.full.and_then(Frame::saved),
        expanded: record.expanded,
        pinned: record.pinned,
    };
    change(&mut session);
    let next = SavedSessionRecord {
        small: session.small.map(Frame::from_saved),
        full: session.full.map(Frame::from_saved),
        expanded: session.expanded,
        pinned: session.pinned,
        at: record.at,
    };
    if next.small == record.small
        && next.full == record.full
        && next.expanded == record.expanded
        && next.pinned == record.pinned
        && record.at != 0
    {
        return;
    }
    *record = SavedSessionRecord {
        at: now_ms(),
        ..next
    };
    trim(&mut state);
    write(&state);
}

/// The pop-ups window `slot` had open.
pub(super) fn open(slot: u32) -> Vec<String> {
    read().open.remove(&slot.to_string()).unwrap_or_default()
}

/// The pop-ups window `slot` has open now.
pub(super) fn set_open(slot: u32, rows: Vec<String>) {
    let mut state = read();
    let key = slot.to_string();
    if state.open.get(&key).map(Vec::as_slice).unwrap_or_default() == rows.as_slice() {
        return;
    }
    if rows.is_empty() {
        state.open.remove(&key);
    } else {
        state.open.insert(key, rows);
    }
    write(&state);
}

fn trim(state: &mut SavedState) {
    if state.sessions.len() <= MAX_SAVED_SESSIONS {
        return;
    }
    let open = state.open.values().flatten().cloned().collect::<Vec<_>>();
    let mut by_age = state
        .sessions
        .iter()
        .filter(|(row, _)| !open.contains(row))
        .map(|(row, record)| (record.at, row.clone()))
        .collect::<Vec<_>>();
    by_age.sort();
    let excess = state.sessions.len() - MAX_SAVED_SESSIONS;
    for (_, row) in by_age.into_iter().take(excess) {
        state.sessions.remove(&row);
    }
}
