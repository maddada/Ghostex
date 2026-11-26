//! Input injection with SendInput. Keys arrive as X keysyms: characters are looked up
//! in the current keyboard layout (so the right key, with Shift as the character
//! needs given the remote's Caps Lock, is pressed whatever the layout), named keys
//! come from a table, and characters the layout can't type are sent as Unicode. Every
//! key carries its scan code too, for apps that read those. Scrolling is high
//! resolution.

use std::collections::HashMap;
use std::sync::mpsc;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WHEEL_DELTA,
};

pub enum InputEvent {
    Move {
        x: i32,
        y: i32,
    },
    Button {
        button: u8,
        down: bool,
    },
    /// Wheel ticks: positive dy scrolls down, positive dx scrolls right.
    Scroll {
        dx: i32,
        dy: i32,
    },
    /// Precise scroll in desktop pixels (trackpads, smooth wheels).
    ScrollPx {
        dx: f64,
        dy: f64,
    },
    Key {
        keysym: u32,
        down: bool,
    },
    /// Remote clipboard update, ordered with the key events around it.
    Clipboard(super::desktop::ClipContent),
    ReleaseAll,
    /// Release everything before exit.
    Shutdown,
}

/// Modifier keysyms -> virtual key.
fn modifier(keysym: u32) -> Option<VIRTUAL_KEY> {
    Some(match keysym {
        0xffe1 => VK_LSHIFT,
        0xffe2 => VK_RSHIFT,
        0xffe3 => VK_LCONTROL,
        0xffe4 => VK_RCONTROL,
        0xffe9 => VK_LMENU,
        // Alt_R and AltGr (ISO_Level3_Shift) are the right Alt.
        0xffea | 0xfe03 => VK_RMENU,
        // Super and Meta are the Windows key.
        0xffeb | 0xffe7 => VK_LWIN,
        0xffec | 0xffe8 => VK_RWIN,
        _ => return None,
    })
}

/// Keys that don't type a character.
fn named(keysym: u32) -> Option<VIRTUAL_KEY> {
    Some(match keysym {
        0xff08 => VK_BACK,
        0xff09 | 0xfe20 => VK_TAB, // Tab, ISO_Left_Tab
        0xff0d | 0xff8d => VK_RETURN,
        0xff1b => VK_ESCAPE,
        0xffff => VK_DELETE,
        0xff50 => VK_HOME,
        0xff51 => VK_LEFT,
        0xff52 => VK_UP,
        0xff53 => VK_RIGHT,
        0xff54 => VK_DOWN,
        0xff55 => VK_PRIOR,
        0xff56 => VK_NEXT,
        0xff57 => VK_END,
        0xff63 => VK_INSERT,
        0xff67 => VK_APPS,
        0xff7f => VK_NUMLOCK,
        0xffe5 => VK_CAPITAL,
        0xff61 => VK_SNAPSHOT,
        0xff14 => VK_SCROLL,
        0xff13 => VK_PAUSE,
        0x20 => VK_SPACE,
        // Keypad.
        0xffb0..=0xffb9 => VIRTUAL_KEY(VK_NUMPAD0.0 + (keysym - 0xffb0) as u16),
        0xffaa => VK_MULTIPLY,
        0xffab => VK_ADD,
        0xffad => VK_SUBTRACT,
        0xffae => VK_DECIMAL,
        0xffaf => VK_DIVIDE,
        // Keypad navigation (Num Lock off).
        0xff95 => VK_HOME,
        0xff96 => VK_LEFT,
        0xff97 => VK_UP,
        0xff98 => VK_RIGHT,
        0xff99 => VK_DOWN,
        0xff9a => VK_PRIOR,
        0xff9b => VK_NEXT,
        0xff9c => VK_END,
        0xff9d => VK_CLEAR,
        0xff9e => VK_INSERT,
        0xff9f => VK_DELETE,
        // F1..F24.
        0xffbe..=0xffd5 => VIRTUAL_KEY(VK_F1.0 + (keysym - 0xffbe) as u16),
        // Media keys (XF86).
        0x1008ff11 => VK_VOLUME_DOWN,
        0x1008ff12 => VK_VOLUME_MUTE,
        0x1008ff13 => VK_VOLUME_UP,
        0x1008ff14 => VK_MEDIA_PLAY_PAUSE,
        0x1008ff15 => VK_MEDIA_STOP,
        0x1008ff16 => VK_MEDIA_PREV_TRACK,
        0x1008ff17 => VK_MEDIA_NEXT_TRACK,
        _ => return None,
    })
}

/// Keys whose scan code has the E0 prefix: they need KEYEVENTF_EXTENDEDKEY.
fn extended(keysym: u32, vk: VIRTUAL_KEY) -> bool {
    // KP_Enter is Return with the extended flag; the keypad navigation keys are not.
    keysym == 0xff8d
        || (!(0xff95..=0xff9f).contains(&keysym)
            && matches!(
                vk,
                VK_RCONTROL
                    | VK_RMENU
                    | VK_LWIN
                    | VK_RWIN
                    | VK_APPS
                    | VK_INSERT
                    | VK_DELETE
                    | VK_HOME
                    | VK_END
                    | VK_PRIOR
                    | VK_NEXT
                    | VK_LEFT
                    | VK_RIGHT
                    | VK_UP
                    | VK_DOWN
                    | VK_DIVIDE
                    | VK_NUMLOCK
                    | VK_SNAPSHOT
            ))
}

fn key_input(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_input(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    let n = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if (n as usize) < inputs.len() {
        // Blocked by UIPI (an elevated window has focus) or on another desktop.
        super::follow_input_desktop();
        unsafe { SendInput(&inputs[n as usize..], std::mem::size_of::<INPUT>() as i32) };
    }
}

/// The keyboard layout of the window that has focus (layouts are per thread).
fn layout() -> HKL {
    unsafe {
        let tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
        GetKeyboardLayout(tid)
    }
}

fn caps_lock_on() -> bool {
    // The low bit is the toggle.
    let state = unsafe { GetKeyState(VK_CAPITAL.0 as i32) };
    state & 1 != 0
}

/// What `vk` types on `hkl` with Caps Lock on and Shift as given, leaving any pending
/// dead key alone.
fn typed_with_caps(vk: VIRTUAL_KEY, shift: bool, hkl: HKL) -> Option<u16> {
    let mut state = [0u8; 256];
    state[VK_CAPITAL.0 as usize] = 1;
    if shift {
        state[VK_SHIFT.0 as usize] = 0x80;
    }
    let scan = unsafe { MapVirtualKeyExW(vk.0 as u32, MAPVK_VK_TO_VSC, Some(hkl)) };
    let mut buf = [0u16; 4];
    // Flag 4: don't change the keyboard state (Windows 10 1607 and later).
    let n = unsafe { ToUnicodeEx(vk.0 as u32, scan, &state, &mut buf, 4, Some(hkl)) };
    (n == 1).then_some(buf[0])
}

struct Injector {
    /// keysym -> (virtual key, extended, Shift pressed or lifted around it).
    pressed: HashMap<u32, (VIRTUAL_KEY, bool, ShiftFix)>,
    /// Held modifier keysyms.
    mods: HashMap<u32, VIRTUAL_KEY>,
    buttons: [bool; 16],
    scroll_acc: (f64, f64),
    px_per_notch: f64,
    clipboard: Option<super::desktop::ClipboardHandle>,
}

#[derive(Clone, Copy, PartialEq)]
enum ShiftFix {
    None,
    /// The character needs Shift but none is held: pressed around the key.
    Add,
    /// The character needs no Shift but it is held: lifted around the key.
    Lift,
}

impl Injector {
    fn key_event(&self, vk: VIRTUAL_KEY, ext: bool, down: bool) -> INPUT {
        let hkl = layout();
        let scan = unsafe { MapVirtualKeyExW(vk.0 as u32, MAPVK_VK_TO_VSC_EX, Some(hkl)) };
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if ext || scan & 0xe000 == 0xe000 {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        key_input(vk, (scan & 0xff) as u16, flags)
    }

    fn shift_held(&self) -> Vec<VIRTUAL_KEY> {
        self.mods
            .values()
            .copied()
            .filter(|&v| v == VK_LSHIFT || v == VK_RSHIFT)
            .collect()
    }

    /// Ctrl, Alt or the Windows key held: the key is a shortcut, not text.
    fn shortcut_held(&self) -> bool {
        self.mods
            .iter()
            .any(|(&ks, &v)| ks != 0xfe03 && !matches!(v, VK_LSHIFT | VK_RSHIFT))
    }

    /// Pixels on the virtual desktop -> SendInput's absolute 0..65535 range.
    fn absolute(&self, x: i32, y: i32) -> (i32, i32) {
        let g = super::GEOMETRY.lock().unwrap_or(super::Geometry {
            left: 0,
            top: 0,
            width: 1,
            height: 1,
            stream_width: 1,
            stream_height: 1,
        });
        // Positions come in streamed pixels; the display may stream smaller than it is.
        let scale = |v: i32, stream: u32, real: u32| -> i32 {
            if stream == 0 || stream == real {
                v
            } else {
                ((v as i64 * real as i64 + stream as i64 / 2) / stream as i64) as i32
            }
        };
        let (x, y) = (
            scale(x, g.stream_width, g.width),
            scale(y, g.stream_height, g.height),
        );
        let (vx, vy, vw, vh) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            )
        };
        let px = (g.left + x.clamp(0, g.width as i32 - 1) - vx) as i64;
        let py = (g.top + y.clamp(0, g.height as i32 - 1) - vy) as i64;
        // Windows maps n back to n * size / 65536 pixels; aim at the pixel's centre.
        (
            ((px * 65536 + 32768) / vw as i64) as i32,
            ((py * 65536 + 32768) / vh as i64) as i32,
        )
    }

    fn handle(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Move { x, y } => {
                let (ax, ay) = self.absolute(x, y);
                send(&[mouse_input(
                    ax,
                    ay,
                    0,
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                )]);
            }
            InputEvent::Button { button, down } => {
                let b = button as usize;
                if b >= self.buttons.len() || self.buttons[b] == down {
                    return;
                }
                self.buttons[b] = down;
                // X buttons: 1 left, 2 middle, 3 right, 8 back, 9 forward.
                let (flags, data) = match (button, down) {
                    (1, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                    (1, false) => (MOUSEEVENTF_LEFTUP, 0),
                    (2, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                    (2, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                    (3, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                    (3, false) => (MOUSEEVENTF_RIGHTUP, 0),
                    (8, true) => (MOUSEEVENTF_XDOWN, 1),
                    (8, false) => (MOUSEEVENTF_XUP, 1),
                    (9, true) => (MOUSEEVENTF_XDOWN, 2),
                    (9, false) => (MOUSEEVENTF_XUP, 2),
                    _ => return,
                };
                send(&[mouse_input(0, 0, data, flags)]);
            }
            InputEvent::Scroll { dx, dy } => {
                let d = WHEEL_DELTA as i32;
                let mut v = Vec::new();
                if dy != 0 {
                    v.push(mouse_input(0, 0, -dy.clamp(-20, 20) * d, MOUSEEVENTF_WHEEL));
                }
                if dx != 0 {
                    v.push(mouse_input(0, 0, dx.clamp(-20, 20) * d, MOUSEEVENTF_HWHEEL));
                }
                send(&v);
            }
            InputEvent::ScrollPx { dx, dy } => {
                // Desktop pixels -> wheel units (120 per notch); apps that support
                // high-resolution wheels scroll by fractions of a notch.
                let k = WHEEL_DELTA as f64 / self.px_per_notch.max(1.0);
                self.scroll_acc.0 += dx * k;
                self.scroll_acc.1 -= dy * k;
                let (x, y) = (self.scroll_acc.0.trunc(), self.scroll_acc.1.trunc());
                self.scroll_acc.0 -= x;
                self.scroll_acc.1 -= y;
                let mut v = Vec::new();
                if y != 0.0 {
                    v.push(mouse_input(0, 0, y as i32, MOUSEEVENTF_WHEEL));
                }
                if x != 0.0 {
                    v.push(mouse_input(0, 0, x as i32, MOUSEEVENTF_HWHEEL));
                }
                send(&v);
            }
            InputEvent::Key { keysym, down } => self.key(keysym, down),
            InputEvent::Clipboard(content) => {
                if let Some(c) = &self.clipboard {
                    c.set_blocking(content);
                }
            }
            InputEvent::ReleaseAll | InputEvent::Shutdown => self.release_all(),
        }
    }

    fn key(&mut self, keysym: u32, down: bool) {
        if let Some(vk) = modifier(keysym) {
            if down == self.mods.contains_key(&keysym) {
                return;
            }
            if down {
                self.mods.insert(keysym, vk);
            } else {
                self.mods.remove(&keysym);
            }
            send(&[self.key_event(vk, extended(keysym, vk), down)]);
            return;
        }
        if !down {
            // Shift was already put back right after the key went down.
            if let Some((vk, ext, _)) = self.pressed.remove(&keysym) {
                send(&[self.key_event(vk, ext, false)]);
            }
            return;
        }
        if let Some(&(vk, ext, fix)) = self.pressed.get(&keysym) {
            // Autorepeat.
            self.press(vk, ext, fix);
            return;
        }
        if let Some(vk) = named(keysym) {
            let ext = extended(keysym, vk);
            self.pressed.insert(keysym, (vk, ext, ShiftFix::None));
            send(&[self.key_event(vk, ext, true)]);
            return;
        }
        // Printable: Latin-1 keysyms are the code point, others 0x01000000 + code point.
        let cp = if keysym >= 0x0100_0000 {
            keysym - 0x0100_0000
        } else {
            keysym
        };
        let Some(c) = char::from_u32(cp) else {
            return;
        };
        let mut utf16 = [0u16; 2];
        let units = c.encode_utf16(&mut utf16);
        // Where the character is on the layout: low byte the key, high byte the
        // modifiers it needs (1 Shift, 2 Ctrl, 4 Alt).
        let hkl = layout();
        let scan = if units.len() == 1 {
            unsafe { VkKeyScanExW(units[0], hkl) }
        } else {
            -1
        };
        let (vk, need) = (
            VIRTUAL_KEY((scan as u16) & 0xff),
            ((scan as u16) >> 8) & 0xff,
        );
        let shortcut = self.shortcut_held();
        let shift = if scan == -1 || (need & !1 != 0 && !shortcut) {
            None
        } else if shortcut || !caps_lock_on() {
            Some(need & 1 != 0)
        } else {
            // Caps Lock on the remote flips Shift on the keys it affects, but the
            // viewer sent the character already cased: pick the Shift that types it.
            [need & 1 != 0, need & 1 == 0]
                .into_iter()
                .find(|&s| typed_with_caps(vk, s, hkl) == Some(units[0]))
        };
        if let Some(shift) = shift {
            let held = !self.shift_held().is_empty();
            // The character decides Shift (the viewer's layout already applied it);
            // Ctrl, Alt and the Windows key stay as held, for shortcuts.
            let fix = match (shift, held) {
                (true, false) if !shortcut => ShiftFix::Add,
                (false, true) if !shortcut => ShiftFix::Lift,
                _ => ShiftFix::None,
            };
            self.pressed.insert(keysym, (vk, false, fix));
            self.press(vk, false, fix);
            return;
        }
        // Not on the layout (only with AltGr/Ctrl+Alt, or not as cased with Caps Lock
        // on): type it as Unicode.
        let mut v = Vec::new();
        for &u in units.iter() {
            v.push(key_input(VIRTUAL_KEY(0), u, KEYEVENTF_UNICODE));
            v.push(key_input(
                VIRTUAL_KEY(0),
                u,
                KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
            ));
        }
        send(&v);
    }

    /// Presses `vk` with Shift adjusted for the character, then puts Shift back.
    fn press(&self, vk: VIRTUAL_KEY, ext: bool, fix: ShiftFix) {
        let mut v = Vec::new();
        let held = self.shift_held();
        match fix {
            ShiftFix::Add => v.push(self.key_event(VK_LSHIFT, false, true)),
            ShiftFix::Lift => v.extend(
                held.iter()
                    .map(|&s| self.key_event(s, s == VK_RSHIFT, false)),
            ),
            ShiftFix::None => {}
        }
        v.push(self.key_event(vk, ext, true));
        match fix {
            ShiftFix::Add => v.push(self.key_event(VK_LSHIFT, false, false)),
            ShiftFix::Lift => v.extend(
                held.iter()
                    .map(|&s| self.key_event(s, s == VK_RSHIFT, true)),
            ),
            ShiftFix::None => {}
        }
        send(&v);
    }

    fn release_all(&mut self) {
        for (_, (vk, ext, _)) in std::mem::take(&mut self.pressed) {
            send(&[self.key_event(vk, ext, false)]);
        }
        for ks in self.mods.keys().copied().collect::<Vec<_>>() {
            self.key(ks, false);
        }
        for b in 1..self.buttons.len() {
            if self.buttons[b] {
                self.handle(InputEvent::Button {
                    button: b as u8,
                    down: false,
                });
            }
        }
    }
}

pub fn spawn(
    _display: Option<String>,
    clipboard: Option<super::desktop::ClipboardHandle>,
    px_per_notch: f64,
) -> Result<mpsc::Sender<InputEvent>, String> {
    let mut inj = Injector {
        pressed: HashMap::new(),
        mods: HashMap::new(),
        buttons: [false; 16],
        scroll_acc: (0.0, 0.0),
        px_per_notch,
        clipboard,
    };
    let (tx, rx) = mpsc::channel::<InputEvent>();
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            super::follow_input_desktop();
            while let Ok(first) = rx.recv() {
                let mut pending_move: Option<(i32, i32)> = None;
                let handle =
                    |inj: &mut Injector, ev: InputEvent, pm: &mut Option<(i32, i32)>| match ev {
                        InputEvent::Move { x, y } => *pm = Some((x, y)),
                        other => {
                            if let Some((x, y)) = pm.take() {
                                inj.handle(InputEvent::Move { x, y });
                            }
                            inj.handle(other);
                        }
                    };
                handle(&mut inj, first, &mut pending_move);
                // Coalesce whatever queued up meanwhile; moves collapse to the latest.
                while let Ok(ev) = rx.try_recv() {
                    handle(&mut inj, ev, &mut pending_move);
                }
                if let Some((x, y)) = pending_move {
                    inj.handle(InputEvent::Move { x, y });
                }
            }
        })
        .unwrap();
    Ok(tx)
}
