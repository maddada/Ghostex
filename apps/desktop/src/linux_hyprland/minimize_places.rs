//! Which Ghostex windows Omarchy's minimize works for right now: the Hyprland workspace each one
//! is on and whether Omarchy floats that workspace, followed while windows move and workspaces
//! change layout (`omarchy_minimizes_window`, read by app/helpers/titlebar/window_caption.rs).
//!
//! CDXC:Titlebar 2026-11-15 WHY:
//! Omarchy keeps a workspace's layout in a file of its own (`workspace-layouts/<id>.lua` under
//! `$XDG_STATE_HOME/omarchy`, else the `default.lua` there, else Hyprland's tiled layout), not in
//! Hyprland, and its minimize decides from that file through `omarchy-hyprland-workspace-layout-current`.
//! Ghostex asks the same command, so the button shows exactly where the minimize acts. Hyprland
//! sends no event when a layout is switched (it only re-floats the windows the old layout tiled), and
//! the switch writes the file after applying the layout, so a change of that folder is the signal;
//! moves come from Hyprland's `movewindowv2` event, and nothing is polled. Hyprland names no X11
//! window, so a window's Hyprland address is learned when it becomes the active window, checked
//! against this process; until then its minimize button stays hidden.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

const LAYOUT_COMMAND: &str = "omarchy-hyprland-workspace-layout-current";

struct WindowPlace {
    /// GPUI's window id.
    window: u64,
    /// Hyprland's address, `0x…` as `j/clients` writes it.
    address: String,
    minimizes: bool,
}

static PLACES: Mutex<Vec<WindowPlace>> = Mutex::new(Vec::new());
static WATCHERS: Mutex<Vec<UnboundedSender<()>>> = Mutex::new(Vec::new());
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static REFRESH: OnceLock<Mutex<Sender<()>>> = OnceLock::new();

/// Starts following where Ghostex's windows are, on a Hyprland that has Omarchy's minimize.
pub(super) fn start(directory: &Path, omarchy_bin: PathBuf) {
    if DIRECTORY.set(directory.to_path_buf()).is_err() {
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel::<()>();
    if REFRESH.set(Mutex::new(sender)).is_err() {
        return;
    }
    let layout_command = omarchy_bin.join(LAYOUT_COMMAND);
    let refresher = std::thread::Builder::new()
        .name("hyprland-minimize-places".to_string())
        .spawn(move || {
            while receiver.recv().is_ok() {
                while receiver.try_recv().is_ok() {}
                refresh(&layout_command);
            }
        });
    if let Err(error) = refresher {
        super::log_failure("could not follow Ghostex's workspaces", &error.to_string());
        return;
    }
    if let Some(layouts) = omarchy_layouts_directory() {
        let watcher = std::thread::Builder::new()
            .name("omarchy-layout-files".to_string())
            .spawn(move || watch_layout_files(&layouts));
        if let Err(error) = watcher {
            super::log_failure("could not watch Omarchy's layouts", &error.to_string());
        }
    }
}

/// Whether Omarchy's minimize would move this window to its dock now.
pub(crate) fn omarchy_minimizes_window(window: u64) -> bool {
    PLACES.lock().is_ok_and(|places| {
        places
            .iter()
            .any(|place| place.window == window && place.minimizes)
    })
}

/// A message each time `omarchy_minimizes_window` changes for any window.
pub(crate) fn watch_minimize_places() -> UnboundedReceiver<()> {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    if let Ok(mut watchers) = WATCHERS.lock() {
        watchers.push(sender);
    }
    receiver
}

/// Called while `window` has just become active: Hyprland's active window is it, when it belongs
/// to this process.
pub(crate) fn learn_active_window(window: u64) {
    let Some(directory) = DIRECTORY.get() else {
        return;
    };
    if PLACES
        .lock()
        .is_ok_and(|places| places.iter().any(|place| place.window == window))
    {
        return;
    }
    let Some(address) = super::send_request(directory, "j/activewindow")
        .ok()
        .and_then(|answer| own_window_address(&answer))
    else {
        return;
    };
    if let Ok(mut places) = PLACES.lock() {
        places.retain(|place| place.address != address);
        places.push(WindowPlace {
            window,
            address,
            minimizes: false,
        });
    }
    request_refresh();
}

/// One line of Hyprland's event socket.
pub(super) fn note_event(line: &str) {
    let (event, data) = line.split_once(">>").unwrap_or((line, ""));
    match event.trim() {
        "movewindowv2" => {
            let moved = event_address(data);
            if PLACES
                .lock()
                .is_ok_and(|places| places.iter().any(|place| place.address == moved))
            {
                request_refresh();
            }
        }
        "closewindow" => {
            let closed = event_address(data);
            if let Ok(mut places) = PLACES.lock() {
                places.retain(|place| place.address != closed);
            }
        }
        "configreloaded" => request_refresh(),
        _ => {}
    }
}

/// Events write an address without the `0x` that `j/clients` and `j/activewindow` put first.
fn event_address(data: &str) -> String {
    let address = data.split(',').next().unwrap_or_default().trim();
    format!("0x{}", address.trim_start_matches("0x"))
}

fn own_window_address(answer: &str) -> Option<String> {
    let window: serde_json::Value = serde_json::from_str(answer.trim()).ok()?;
    let own = window.get("pid")?.as_u64()? == u64::from(std::process::id())
        && window.get("class")?.as_str()? == super::WINDOW_CLASS;
    own.then(|| window.get("address")?.as_str().map(str::to_string))
        .flatten()
}

fn request_refresh() {
    if let Some(refresh) = REFRESH.get()
        && let Ok(sender) = refresh.lock()
    {
        sender.send(()).ok();
    }
}

fn refresh(layout_command: &Path) {
    let addresses: Vec<String> = match PLACES.lock() {
        Ok(places) if !places.is_empty() => {
            places.iter().map(|place| place.address.clone()).collect()
        }
        _ => return,
    };
    let Some(directory) = DIRECTORY.get() else {
        return;
    };
    let Some(clients) = super::send_request(directory, "j/clients")
        .ok()
        .and_then(|answer| serde_json::from_str::<serde_json::Value>(answer.trim()).ok())
    else {
        return;
    };
    let mut floating_workspaces: Vec<(i64, bool)> = Vec::new();
    let mut minimizes: Vec<(String, bool)> = Vec::new();
    for address in addresses {
        let workspace = clients.as_array().and_then(|clients| {
            clients
                .iter()
                .find(|client| client.get("address").and_then(|a| a.as_str()) == Some(&address))
                .and_then(|client| client.get("workspace")?.get("id")?.as_i64())
        });
        // Omarchy's minimize leaves special and named workspaces (id below 1) alone.
        let floating = match workspace {
            Some(id) if id >= 1 => {
                match floating_workspaces.iter().find(|(known, _)| *known == id) {
                    Some((_, floating)) => *floating,
                    None => {
                        let floating = workspace_floats(layout_command, id);
                        floating_workspaces.push((id, floating));
                        floating
                    }
                }
            }
            _ => false,
        };
        minimizes.push((address, floating));
    }
    let mut changed = false;
    if let Ok(mut places) = PLACES.lock() {
        for place in places.iter_mut() {
            if let Some((_, now)) = minimizes
                .iter()
                .find(|(address, _)| *address == place.address)
                && place.minimizes != *now
            {
                place.minimizes = *now;
                changed = true;
            }
        }
    }
    if changed && let Ok(mut watchers) = WATCHERS.lock() {
        watchers.retain(|watcher| watcher.unbounded_send(()).is_ok());
    }
}

fn workspace_floats(layout_command: &Path, workspace: i64) -> bool {
    match std::process::Command::new(layout_command)
        .arg(workspace.to_string())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim() == "floating",
        Err(error) => {
            super::log_failure("Omarchy's layout command did not run", &error.to_string());
            false
        }
    }
}

/// `$XDG_STATE_HOME/omarchy/workspace-layouts`, the folder Omarchy's layout commands read.
fn omarchy_layouts_directory() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|state| !state.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("omarchy").join("workspace-layouts"))
}

/// Refreshes after each change of a file in Omarchy's layouts folder. The folder appears with the
/// first layout switch, so its parent is watched until then.
fn watch_layout_files(layouts: &Path) {
    use std::os::unix::ffi::OsStrExt as _;
    // SAFETY: plain inotify calls on a descriptor this thread owns.
    let descriptor = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    if descriptor < 0 {
        return;
    }
    let add_watch = |path: &Path, mask: u32| -> i32 {
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return -1;
        };
        // SAFETY: `path` is a valid C string for the duration of the call.
        unsafe { libc::inotify_add_watch(descriptor, path.as_ptr(), mask) }
    };
    let file_changes =
        libc::IN_CLOSE_WRITE | libc::IN_DELETE | libc::IN_MOVED_TO | libc::IN_MOVED_FROM;
    let mut layouts_watch = add_watch(layouts, file_changes);
    if layouts_watch < 0
        && let Some(parent) = layouts.parent()
    {
        add_watch(parent, libc::IN_CREATE | libc::IN_MOVED_TO);
    }
    let mut buffer = [0u8; 4096];
    loop {
        // SAFETY: `buffer` is writable for its whole length.
        let read = unsafe { libc::read(descriptor, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        let read = read as usize;
        if layouts_watch < 0 {
            layouts_watch = add_watch(layouts, file_changes);
            if layouts_watch >= 0 {
                request_refresh();
            }
            continue;
        }
        // Each event is a `struct inotify_event`: wd, mask, cookie, len, then `len` name bytes.
        let mut offset = 0;
        let mut layouts_changed = false;
        while offset + 16 <= read {
            let field = |at: usize| {
                <[u8; 4]>::try_from(&buffer[offset + at..offset + at + 4]).unwrap_or_default()
            };
            layouts_changed |= i32::from_ne_bytes(field(0)) == layouts_watch;
            offset += 16 + u32::from_ne_bytes(field(12)) as usize;
        }
        if layouts_changed {
            request_refresh();
        }
    }
}
