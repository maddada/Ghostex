//! The terminal model of the browser build: the desktop's model (`apps/desktop/src/terminal_model.rs`) with its PTY half replaced by gxserver's `/api/terminal` WebSocket, which attaches to the session's zmx daemon on the computer that runs it. Everything that reads or encodes terminal STATE (snapshots, scrollback, key and mouse encoding, modes, titles, hyperlinks) is the desktop's code, lifted out of that file byte for byte by `build.rs` (`extracted-items.txt`), over the same libghostty-vt, here a static wasm32 archive. Only what owns a process or a thread there is written again here.
use std::{
    cell::RefCell,
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{BinaryType, MessageEvent, WebSocket};

use crate::ghostty_vt::{
    self, VtCellWide, VtClearScreen, VtDirty, VtError, VtHostCallbacks, VtKeyEncoder, VtKeyInput,
    VtMouseEncoder, VtMouseInput, VtOptionAsAlt, VtRenderState, VtScrollViewport, VtScrollbar,
    VtTerminal, ffi,
};

struct PtyWriteRequest {
    bytes: Vec<u8>,
}

/// A value that only ever lives on the page's one thread. libghostty's host callbacks are typed `Send` because the desktop calls them from its PTY reader thread; here they run inside `feed`, on the thread that owns the socket.
struct PageLocal<T>(T);
unsafe impl<T> Send for PageLocal<T> {}
unsafe impl<T> Sync for PageLocal<T> {}

/// The socket and what was typed before it opened.
struct Link {
    socket: WebSocket,
    open: bool,
    waiting: Vec<Vec<u8>>,
}

impl Link {
    fn send(&mut self, bytes: &[u8]) {
        if self.open {
            let _ = self.socket.send_with_u8_array(bytes);
        } else {
            self.waiting.push(bytes.to_vec());
        }
    }

    fn send_control(&self, message: serde_json::Value) {
        if self.open {
            let _ = self.socket.send_with_str(&message.to_string());
        }
    }
}

/// Which session to attach to, and the grid to start at.
pub struct TerminalAttachConfig {
    pub base_url: String,
    pub auth_token: String,
    pub project_id: String,
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
    pub max_scrollback: usize,
}

pub struct TerminalModel {
    terminal: Arc<Mutex<Option<VtTerminal>>>,
    render_state: Option<VtRenderState>,
    link: Arc<PageLocal<Rc<RefCell<Link>>>>,
    /// Always `None`: the process belongs to the zmx daemon on the session's computer. The lifted paste path reads it for its diagnostics.
    child_pid: Option<u32>,
    size: (u16, u16),
    cell_size_px: (u32, u32),
    key_encoder: VtKeyEncoder,
    mouse_encoder: VtMouseEncoder,
    option_as_alt: VtOptionAsAlt,
    clipboard_writes: Arc<Mutex<VecDeque<String>>>,
    exit: Arc<Mutex<Option<TerminalExit>>>,
    _listeners: Vec<Closure<dyn FnMut(MessageEvent)>>,
    _on_open: Closure<dyn FnMut()>,
    _on_close: Closure<dyn FnMut()>,
}

include!(concat!(env!("OUT_DIR"), "/terminal_model.rs"));

impl TerminalModel {
    /// The desktop's entry point, which starts a local shell. A page cannot start a process; `attach` is this build's way in.
    pub fn spawn(_config: TerminalSpawnConfig, _events: TerminalEventSink) -> anyhow::Result<Self> {
        anyhow::bail!("a browser cannot start a local process; attach to a session instead")
    }

    pub fn attach(config: TerminalAttachConfig, events: TerminalEventSink) -> anyhow::Result<Self> {
        let url = format!(
            "{}/api/terminal?authToken={}&protocolVersion=1&projectId={}&sessionId={}&cols={}&rows={}",
            config.base_url.replacen("http", "ws", 1),
            js_sys::encode_uri_component(&config.auth_token),
            js_sys::encode_uri_component(&config.project_id),
            js_sys::encode_uri_component(&config.session_id),
            config.cols,
            config.rows,
        );
        let socket = WebSocket::new(&url)
            .map_err(|error| anyhow::anyhow!("could not open the terminal socket: {error:?}"))?;
        socket.set_binary_type(BinaryType::Arraybuffer);
        let link = Arc::new(PageLocal(Rc::new(RefCell::new(Link {
            socket: socket.clone(),
            open: false,
            waiting: Vec::new(),
        }))));

        let mut vt = VtTerminal::new(config.cols, config.rows, config.max_scrollback)?;
        let clipboard_writes: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        {
            let reply_link = Arc::clone(&link);
            let bell_events = Arc::clone(&events);
            let title_events = Arc::clone(&events);
            let clipboard_events = Arc::clone(&events);
            let clipboard_write_queue = Arc::clone(&clipboard_writes);
            vt.set_host_callbacks(VtHostCallbacks {
                // The terminal answering a query (cursor position, device attributes) from inside `feed`.
                write_pty: Some(Box::new(move |bytes| reply_link.0.borrow_mut().send(bytes))),
                bell: Some(Box::new(move || bell_events(TerminalEvent::Bell))),
                title_changed: Some(Box::new(move || title_events(TerminalEvent::TitleChanged))),
                clipboard_write: Some(Box::new(move |text| {
                    if let Ok(mut queue) = clipboard_write_queue.lock() {
                        if queue.len() >= CLIPBOARD_WRITE_QUEUE_LIMIT {
                            queue.pop_front();
                        }
                        queue.push_back(text);
                    }
                    clipboard_events(TerminalEvent::ClipboardWriteRequested);
                })),
            })?;
        }
        let terminal = Arc::new(Mutex::new(Some(vt)));
        let exit: Arc<Mutex<Option<TerminalExit>>> = Arc::new(Mutex::new(None));

        let open_link = Arc::clone(&link);
        let (cols, rows) = (config.cols, config.rows);
        let on_open = Closure::<dyn FnMut()>::new(move || {
            let mut link = open_link.0.borrow_mut();
            link.open = true;
            // The same visibility contract every other client keeps with zmx: a visible client names its real grid (server/src/terminal_ws.rs translates it into ZMX_VISIBLE).
            link.send_control(
                serde_json::json!({"type": "visibility", "state": "visible", "cols": cols, "rows": rows}),
            );
            for bytes in std::mem::take(&mut link.waiting) {
                let _ = link.socket.send_with_u8_array(&bytes);
            }
        });
        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let feed_terminal = Arc::clone(&terminal);
        let feed_events = Arc::clone(&events);
        let feed_exit = Arc::clone(&exit);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            if let Ok(buffer) = data.clone().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                if let Some(terminal) = feed_terminal
                    .lock()
                    .expect("terminal lock poisoned")
                    .as_mut()
                {
                    terminal.feed(&bytes);
                }
                feed_events(TerminalEvent::Wakeup);
            } else if let Some(text) = data.as_string()
                && let Ok(control) = serde_json::from_str::<serde_json::Value>(&text)
            {
                match control["type"].as_str() {
                    Some("exit") => {
                        let code = control["code"].as_i64().map(|code| code as u32);
                        let status = TerminalExit {
                            code,
                            success: code == Some(0),
                        };
                        *feed_exit.lock().expect("terminal exit lock poisoned") = Some(status);
                        feed_events(TerminalEvent::Exited(status));
                    }
                    Some("error") => log::warn!("terminal socket: {}", control["message"]),
                    _ => {}
                }
            }
        });
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let close_link = Arc::clone(&link);
        let on_close = Closure::<dyn FnMut()>::new(move || close_link.0.borrow_mut().open = false);
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(Self {
            terminal,
            render_state: Some(VtRenderState::new()?),
            link,
            child_pid: None,
            size: (config.cols, config.rows),
            cell_size_px: (0, 0),
            key_encoder: VtKeyEncoder::new()?,
            mouse_encoder: VtMouseEncoder::new()?,
            option_as_alt: VtOptionAsAlt::default(),
            clipboard_writes,
            exit,
            _listeners: vec![on_message],
            _on_open: on_open,
            _on_close: on_close,
        })
    }

    fn queue_input(&self, request: PtyWriteRequest) -> std::io::Result<()> {
        self.link.0.borrow_mut().send(&request.bytes);
        Ok(())
    }

    pub fn has_pending_input(&self) -> bool {
        !self.link.0.borrow().waiting.is_empty()
    }

    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> anyhow::Result<()> {
        if (cols, rows) == self.size && (cell_width_px, cell_height_px) == self.cell_size_px {
            return Ok(());
        }
        self.terminal
            .lock()
            .expect("terminal lock poisoned")
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("terminal viewer retired"))?
            .resize(cols, rows, cell_width_px, cell_height_px)?;
        if (cols, rows) != self.size {
            self.link
                .0
                .borrow()
                .send_control(serde_json::json!({"type": "resize", "cols": cols, "rows": rows}));
        }
        self.size = (cols, rows);
        self.cell_size_px = (cell_width_px, cell_height_px);
        Ok(())
    }

    /// The desktop retires its emulator when a zmx viewer detaches to free memory; a page's terminal lives as long as its view.
    pub fn release_viewer_emulator(&mut self) {}

    pub fn exit_status(&self) -> Option<TerminalExit> {
        *self.exit.lock().expect("terminal exit lock poisoned")
    }

    /// Closing the view only detaches this client; the session keeps running in its daemon, so there is never anything to confirm.
    pub fn needs_confirm_close(&mut self, _behavior: TerminalConfirmCloseBehavior) -> bool {
        false
    }
}

impl Drop for TerminalModel {
    fn drop(&mut self) {
        let link = self.link.0.borrow();
        link.socket.set_onopen(None);
        link.socket.set_onmessage(None);
        link.socket.set_onclose(None);
        let _ = link.socket.close();
    }
}
