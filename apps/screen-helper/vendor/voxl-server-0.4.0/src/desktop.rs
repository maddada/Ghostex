//! Cursor shape tracking and two-way CLIPBOARD sync on a dedicated X connection.

use crate::hub::{Hub, Out};
use bytes::Bytes;
use nix::fcntl::OFlag;
use nix::poll::{poll, PollFd, PollFlags};
use nix::unistd::{pipe2, read, write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::mpsc;
use std::sync::Arc;
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, EventMask, PropMode, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Clipboard contents exchanged with clients.
pub enum ClipContent {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
}

/// Largest image moved through the clipboard (X requests are capped near 16 MiB).
const MAX_IMAGE: usize = 15 << 20;

/// Sets the remote CLIPBOARD and returns once the X server has registered us as owner,
/// so a paste shortcut injected right afterwards sees the new contents.
#[derive(Clone)]
pub struct ClipboardHandle {
    tx: mpsc::Sender<(ClipContent, mpsc::SyncSender<()>)>,
    wake_w: Arc<OwnedFd>,
}

impl ClipboardHandle {
    pub fn set_blocking(&self, content: ClipContent) {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        if self.tx.send((content, done_tx)).is_ok() {
            let _ = write(&*self.wake_w, &[1u8]);
            let _ = done_rx.recv_timeout(std::time::Duration::from_millis(300));
        }
    }
}

pub struct ClipboardMode {
    pub to_server: bool,
    pub to_client: bool,
}

struct Atoms {
    clipboard: u32,
    targets: u32,
    utf8: u32,
    text: u32,
    text_plain: u32,
    incr: u32,
    prop: u32,
    png: u32,
}

fn atom(c: &RustConnection, name: &str) -> Result<u32, String> {
    Ok(c.intern_atom(false, name.as_bytes())
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?
        .atom)
}

pub fn spawn(
    display: Option<String>,
    hub: Arc<Hub>,
    mode: ClipboardMode,
) -> Result<ClipboardHandle, String> {
    let (conn, n) = RustConnection::connect(display.as_deref()).map_err(|e| e.to_string())?;
    let root = conn.setup().roots[n].root;
    conn.xfixes_query_version(5, 0)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    let win = conn.generate_id().map_err(|e| e.to_string())?;
    conn.create_window(
        0,
        win,
        root,
        -10,
        -10,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &xproto::CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(|e| e.to_string())?;
    let atoms = Atoms {
        clipboard: atom(&conn, "CLIPBOARD")?,
        targets: atom(&conn, "TARGETS")?,
        utf8: atom(&conn, "UTF8_STRING")?,
        text: atom(&conn, "TEXT")?,
        text_plain: atom(&conn, "text/plain;charset=utf-8")?,
        incr: atom(&conn, "INCR")?,
        prop: atom(&conn, "VOXL_CLIP")?,
        png: atom(&conn, "image/png")?,
    };
    conn.xfixes_select_cursor_input(root, xfixes::CursorNotifyMask::DISPLAY_CURSOR)
        .map_err(|e| e.to_string())?;
    // Clients draw the cursor locally. Xvfb's software cursor is painted into the
    // framebuffer we read directly, which would show a second, lagging cursor and
    // turn every pointer move into an encode, so hide it while we are connected.
    conn.xfixes_hide_cursor(root).map_err(|e| e.to_string())?;
    if mode.to_client {
        conn.xfixes_select_selection_input(
            root,
            atoms.clipboard,
            xfixes::SelectionEventMask::SET_SELECTION_OWNER,
        )
        .map_err(|e| e.to_string())?;
    }
    conn.flush().map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel::<(ClipContent, mpsc::SyncSender<()>)>();
    let (wake_r, wake_w) =
        pipe2(OFlag::O_NONBLOCK | OFlag::O_CLOEXEC).map_err(|e| e.to_string())?;
    let wake_w = Arc::new(wake_w);
    std::thread::Builder::new()
        .name("desktop".into())
        .spawn(move || {
            let mut owned: Option<ClipContent> = None;
            // INCR transfer in progress: (target being fetched, bytes so far).
            let mut incr: Option<(u32, Vec<u8>)> = None;
            let mut last_image: Option<u64> = None;
            send_cursor(&conn, &hub);
            loop {
                let mut drain = [0u8; 64];
                while matches!(read(&wake_r, &mut drain), Ok(n) if n > 0) {}
                while let Ok((content, done)) = rx.try_recv() {
                    if mode.to_server {
                        match &content {
                            ClipContent::Text(t) => *hub.last_clipboard.lock() = Some(t.clone()),
                            ClipContent::Image(png) => last_image = Some(hash(png)),
                        }
                        owned = Some(content);
                        let _ = conn.set_selection_owner(win, atoms.clipboard, x11rb::CURRENT_TIME);
                        // Round trip: ownership is in effect once this returns.
                        let _ = conn.get_selection_owner(atoms.clipboard).map(|c| c.reply());
                    }
                    let _ = done.send(());
                }
                let ev = match conn.poll_for_event() {
                    Ok(Some(ev)) => ev,
                    Ok(None) => {
                        let mut p = [
                            PollFd::new(conn.stream().as_fd(), PollFlags::POLLIN),
                            PollFd::new(wake_r.as_fd(), PollFlags::POLLIN),
                        ];
                        let _ = poll(&mut p, 1000u16);
                        continue;
                    }
                    Err(e) => {
                        log::error!("desktop X connection: {e}");
                        return;
                    }
                };
                match ev {
                    Event::XfixesCursorNotify(_) => send_cursor(&conn, &hub),
                    Event::XfixesSelectionNotify(e) => {
                        if e.owner != win && e.owner != x11rb::NONE {
                            // Ask what the new owner offers, then fetch text or an image.
                            let _ = conn.convert_selection(
                                win,
                                atoms.clipboard,
                                atoms.targets,
                                atoms.prop,
                                x11rb::CURRENT_TIME,
                            );
                            let _ = conn.flush();
                        }
                    }
                    Event::SelectionNotify(e) => {
                        if e.property == x11rb::NONE {
                            continue;
                        }
                        if let Ok(Ok(r)) = conn
                            .get_property(true, win, atoms.prop, AtomEnum::ANY, 0, u32::MAX / 4)
                            .map(|c| c.reply())
                        {
                            if e.target == atoms.targets {
                                let offered: Vec<u32> =
                                    r.value32().map(|v| v.collect()).unwrap_or_default();
                                // Text wins when both are offered (rich text copies carry both).
                                let want = if offered.contains(&atoms.utf8) {
                                    Some(atoms.utf8)
                                } else if offered.contains(&atoms.png) {
                                    Some(atoms.png)
                                } else if offered.contains(&u32::from(AtomEnum::STRING)) {
                                    Some(u32::from(AtomEnum::STRING))
                                } else {
                                    None
                                };
                                if let Some(t) = want {
                                    let _ = conn.convert_selection(
                                        win,
                                        atoms.clipboard,
                                        t,
                                        atoms.prop,
                                        x11rb::CURRENT_TIME,
                                    );
                                }
                            } else if r.type_ == atoms.incr {
                                incr = Some((e.target, Vec::new()));
                            } else {
                                publish(&hub, &atoms, e.target, r.value, &mut last_image);
                            }
                            let _ = conn.flush();
                        }
                    }
                    Event::PropertyNotify(e)
                        if e.atom == atoms.prop
                            && e.state == xproto::Property::NEW_VALUE
                            && incr.is_some() =>
                    {
                        if let Ok(Ok(r)) = conn
                            .get_property(true, win, atoms.prop, AtomEnum::ANY, 0, u32::MAX / 4)
                            .map(|c| c.reply())
                        {
                            if r.value.is_empty() {
                                let (target, data) = incr.take().unwrap();
                                publish(&hub, &atoms, target, data, &mut last_image);
                            } else {
                                incr.as_mut().unwrap().1.extend_from_slice(&r.value);
                            }
                            let _ = conn.flush();
                        }
                    }
                    Event::SelectionRequest(e) => {
                        let mut prop = e.property;
                        if prop == x11rb::NONE {
                            prop = e.target;
                        }
                        let text_targets = [
                            atoms.utf8,
                            atoms.text,
                            atoms.text_plain,
                            u32::from(AtomEnum::STRING),
                        ];
                        let ok = match &owned {
                            Some(ClipContent::Text(text)) if e.selection == atoms.clipboard => {
                                if e.target == atoms.targets {
                                    let list = [
                                        atoms.targets,
                                        atoms.utf8,
                                        u32::from(AtomEnum::STRING),
                                        atoms.text,
                                        atoms.text_plain,
                                    ];
                                    conn.change_property32(
                                        PropMode::REPLACE,
                                        e.requestor,
                                        prop,
                                        AtomEnum::ATOM,
                                        &list,
                                    )
                                    .is_ok()
                                } else if text_targets.contains(&e.target) {
                                    conn.change_property8(
                                        PropMode::REPLACE,
                                        e.requestor,
                                        prop,
                                        e.target,
                                        text.as_bytes(),
                                    )
                                    .is_ok()
                                } else {
                                    false
                                }
                            }
                            Some(ClipContent::Image(png)) if e.selection == atoms.clipboard => {
                                if e.target == atoms.targets {
                                    conn.change_property32(
                                        PropMode::REPLACE,
                                        e.requestor,
                                        prop,
                                        AtomEnum::ATOM,
                                        &[atoms.targets, atoms.png],
                                    )
                                    .is_ok()
                                } else if e.target == atoms.png {
                                    conn.change_property8(
                                        PropMode::REPLACE,
                                        e.requestor,
                                        prop,
                                        atoms.png,
                                        png,
                                    )
                                    .is_ok()
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };
                        let notify = xproto::SelectionNotifyEvent {
                            response_type: xproto::SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property: if ok { prop } else { x11rb::NONE },
                        };
                        let _ = conn.send_event(false, e.requestor, EventMask::NO_EVENT, notify);
                        let _ = conn.flush();
                    }
                    Event::SelectionClear(_) => owned = None,
                    _ => {}
                }
            }
        })
        .unwrap();
    Ok(ClipboardHandle { tx, wake_w })
}

fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

fn publish(hub: &Hub, atoms: &Atoms, target: u32, data: Vec<u8>, last_image: &mut Option<u64>) {
    if target == atoms.png {
        if data.is_empty() || data.len() > MAX_IMAGE {
            return;
        }
        let h = hash(&data);
        if *last_image == Some(h) {
            return;
        }
        *last_image = Some(h);
        let mut msg = Vec::with_capacity(data.len() + 1);
        msg.push(0x04);
        msg.extend_from_slice(&data);
        let b = Bytes::from(msg);
        hub.broadcast(|| Out::Bin(b.clone()));
        return;
    }
    let Ok(text) = String::from_utf8(data) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    {
        let mut last = hub.last_clipboard.lock();
        if last.as_deref() == Some(text.as_str()) {
            return;
        }
        *last = Some(text.clone());
    }
    let msg = serde_json::json!({"t": "clip", "d": text}).to_string();
    hub.broadcast(|| Out::Text(msg.clone()));
}

fn send_cursor(conn: &RustConnection, hub: &Hub) {
    let Ok(Ok(img)) = conn.xfixes_get_cursor_image().map(|c| c.reply()) else {
        return;
    };
    let (w, h) = (img.width as usize, img.height as usize);
    let mut msg = Vec::with_capacity(14 + w * h * 4);
    msg.push(0x03);
    msg.push(0);
    msg.extend_from_slice(&(w as u16).to_be_bytes());
    msg.extend_from_slice(&(h as u16).to_be_bytes());
    msg.extend_from_slice(&img.xhot.to_be_bytes());
    msg.extend_from_slice(&img.yhot.to_be_bytes());
    msg.extend_from_slice(&img.cursor_serial.to_be_bytes());
    for &p in img.cursor_image.iter().take(w * h) {
        let a = (p >> 24) & 0xff;
        let un = |c: u32| {
            if a == 0 {
                0
            } else {
                ((c * 255 + a / 2) / a).min(255) as u8
            }
        };
        msg.extend_from_slice(&[
            un((p >> 16) & 0xff),
            un((p >> 8) & 0xff),
            un(p & 0xff),
            a as u8,
        ]);
    }
    let b = Bytes::from(msg);
    *hub.cursor.lock() = Some(b.clone());
    hub.broadcast(|| Out::Bin(b.clone()));
}
