//! Rounded window corners without a compositor.
//!
//! A compositor like picom has to repaint the whole screen in software on Xvfb to
//! blend a few pixels per corner. The SHAPE extension does the same job natively:
//! each top-level window gets a rounded bounding shape, and the X server shows
//! whatever is behind it through the corners. Nothing is repainted per frame, so
//! damage stays precise.

use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, EventMask, MapState, Rectangle,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

struct State {
    conn: RustConnection,
    root: u32,
    screen_w: u16,
    radius: u16,
    /// Windows rounded by title regardless of type (e.g. a dock drawn as a pill).
    named: Vec<(String, u16)>,
    skip_types: Vec<u32>,
    net_wm_window_type: u32,
    /// Size and radius each window was last shaped for (None: left rectangular).
    shaped: HashMap<u32, Option<(u16, u16, u16)>>,
    /// Last size seen per window; moves (same size) need no work.
    seen: HashMap<u32, (u16, u16)>,
}

impl State {
    fn atom(conn: &RustConnection, name: &str) -> u32 {
        conn.intern_atom(false, name.as_bytes())
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
            .unwrap_or(0)
    }

    /// Docks, desktops and similar chrome stay square, as with the picom config.
    fn skip(&self, win: u32) -> bool {
        let mut wins = vec![win];
        if let Ok(Ok(tree)) = self.conn.query_tree(win).map(|c| c.reply()) {
            wins.extend(tree.children);
        }
        wins.iter().any(|&w| {
            self.conn
                .get_property(false, w, self.net_wm_window_type, AtomEnum::ATOM, 0, 8)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(|v| v.collect::<Vec<_>>()))
                .is_some_and(|types| types.iter().any(|t| self.skip_types.contains(t)))
        })
    }

    /// Radius requested for this window (or its client child) by title.
    fn named_radius(&self, win: u32) -> Option<u16> {
        if self.named.is_empty() {
            return None;
        }
        let mut wins = vec![win];
        if let Ok(Ok(tree)) = self.conn.query_tree(win).map(|c| c.reply()) {
            wins.extend(tree.children);
        }
        wins.iter().find_map(|&w| {
            let title = self
                .conn
                .get_property(false, w, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 64)
                .ok()?
                .reply()
                .ok()?
                .value;
            let title = String::from_utf8_lossy(&title);
            self.named
                .iter()
                .find(|(n, _)| *n == title)
                .map(|&(_, r)| r)
        })
    }

    fn update(&mut self, win: u32, width: u16, height: u16) {
        if self.seen.insert(win, (width, height)) == Some((width, height)) {
            return;
        }
        let Ok(Ok(attrs)) = self.conn.get_window_attributes(win).map(|c| c.reply()) else {
            return;
        };
        let (square, radius) = match self.named_radius(win) {
            Some(r) => (attrs.map_state != MapState::VIEWABLE, r),
            // Menus and tooltips (override-redirect) and maximized windows stay square.
            None => (
                attrs.override_redirect
                    || attrs.map_state != MapState::VIEWABLE
                    || width >= self.screen_w
                    || self.skip(win),
                self.radius,
            ),
        };
        let want = (!square && radius > 0).then_some((width, height, radius));
        if self.shaped.get(&win) == Some(&want) {
            return;
        }
        if std::env::var("VOXL_DEBUG_CORNERS").is_ok() {
            log::info!(
                "corners: window {win:#x} {width}x{height} -> {want:?} (was {:?})",
                self.shaped.get(&win)
            );
        }
        if let Some((w, h, r)) = want {
            let rects = rounded_rect(w, h, r);
            let _ = self.conn.shape_rectangles(
                shape::SO::SET,
                shape::SK::BOUNDING,
                x11rb::protocol::xproto::ClipOrdering::YX_BANDED,
                win,
                0,
                0,
                &rects,
            );
        } else if self.shaped.get(&win).is_some_and(|s| s.is_some()) {
            let _ =
                self.conn
                    .shape_mask(shape::SO::SET, shape::SK::BOUNDING, win, 0, 0, x11rb::NONE);
        }
        self.shaped.insert(win, want);
        let _ = self.conn.flush();
    }
}

/// Row bands approximating a rounded rectangle (a quarter circle per corner).
fn rounded_rect(w: u16, h: u16, r: u16) -> Vec<Rectangle> {
    let r = r.min(w / 2).min(h / 2);
    let mut v = Vec::with_capacity(2 * r as usize + 1);
    for i in 0..r {
        let dy = r as f32 - i as f32 - 0.5;
        let inset = (r as f32 - (r as f32 * r as f32 - dy * dy).max(0.0).sqrt()).round() as u16;
        v.push(Rectangle {
            x: inset as i16,
            y: i as i16,
            width: w - 2 * inset,
            height: 1,
        });
    }
    v.push(Rectangle {
        x: 0,
        y: r as i16,
        width: w,
        height: h - 2 * r,
    });
    for i in (0..r).rev() {
        let row = v[i as usize];
        v.push(Rectangle {
            y: (h - 1 - i) as i16,
            ..row
        });
    }
    v
}

pub fn spawn(
    display: Option<String>,
    radius: u16,
    named: Vec<(String, u16)>,
) -> Result<(), String> {
    let (conn, n) = RustConnection::connect(display.as_deref()).map_err(|e| e.to_string())?;
    conn.shape_query_version()
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("no SHAPE: {e}"))?;
    let screen = &conn.setup().roots[n];
    let (root, screen_w) = (screen.root, screen.width_in_pixels);
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
    )
    .map_err(|e| e.to_string())?;
    let skip_types = ["_NET_WM_WINDOW_TYPE_DOCK", "_NET_WM_WINDOW_TYPE_DESKTOP"]
        .iter()
        .map(|n| State::atom(&conn, n))
        .collect();
    let net_wm_window_type = State::atom(&conn, "_NET_WM_WINDOW_TYPE");
    let mut st = State {
        conn,
        root,
        screen_w,
        radius,
        named,
        skip_types,
        net_wm_window_type,
        shaped: HashMap::new(),
        seen: HashMap::new(),
    };
    std::thread::Builder::new()
        .name("corners".into())
        .spawn(move || {
            if let Ok(Ok(tree)) = st.conn.query_tree(st.root).map(|c| c.reply()) {
                for w in tree.children {
                    if let Ok(Ok(g)) = st.conn.get_geometry(w).map(|c| c.reply()) {
                        st.update(w, g.width, g.height);
                    }
                }
            }
            loop {
                match st.conn.wait_for_event() {
                    Ok(Event::MapNotify(e)) if e.event == st.root => {
                        st.seen.remove(&e.window);
                        if let Ok(Ok(g)) = st.conn.get_geometry(e.window).map(|c| c.reply()) {
                            st.update(e.window, g.width, g.height);
                        }
                    }
                    Ok(Event::ConfigureNotify(e)) if e.event == st.root => {
                        if std::env::var("VOXL_DEBUG_CORNERS").is_ok() {
                            log::info!(
                                "configure {:#x} {}x{} at {},{}",
                                e.window,
                                e.width,
                                e.height,
                                e.x,
                                e.y
                            );
                        }
                        st.update(e.window, e.width, e.height)
                    }
                    Ok(Event::DestroyNotify(e)) => {
                        st.shaped.remove(&e.window);
                        st.seen.remove(&e.window);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("corners X connection: {e}");
                        return;
                    }
                }
            }
        })
        .unwrap();
    Ok(())
}
