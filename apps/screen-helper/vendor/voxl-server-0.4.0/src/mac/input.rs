//! Input injection with CGEvent. Keys arrive as X keysyms: characters are looked up in
//! the Mac's current keyboard layout (so the right key and Shift/Option are pressed,
//! whatever the layout), named keys come from a table, and characters the layout
//! can't type are sent as Unicode text. Scrolling is pixel-precise.

use super::ffi::*;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
    /// Finish queued input on its original geometry before selecting another display.
    DisplayBarrier(mpsc::Sender<()>),
    /// Release everything before exit.
    Shutdown,
}

/// Modifier keysyms: virtual key, flag, and the device bit for its side.
fn modifier(keysym: u32) -> Option<(u16, u64)> {
    Some(match keysym {
        0xffe1 => (56, kCGEventFlagMaskShift | NX_DEVICELSHIFTKEYMASK),
        0xffe2 => (60, kCGEventFlagMaskShift | NX_DEVICERSHIFTKEYMASK),
        0xffe3 => (59, kCGEventFlagMaskControl | NX_DEVICELCTLKEYMASK),
        0xffe4 => (62, kCGEventFlagMaskControl | NX_DEVICERCTLKEYMASK),
        // Alt is Option; AltGr (ISO_Level3_Shift) is the right Option.
        0xffe9 => (58, kCGEventFlagMaskAlternate | NX_DEVICELALTKEYMASK),
        0xffea | 0xfe03 => (61, kCGEventFlagMaskAlternate | NX_DEVICERALTKEYMASK),
        // Super and Meta are Command (the web client sends Command as Super).
        0xffeb | 0xffe7 => (55, kCGEventFlagMaskCommand | NX_DEVICELCMDKEYMASK),
        0xffec | 0xffe8 => (54, kCGEventFlagMaskCommand | NX_DEVICERCMDKEYMASK),
        _ => return None,
    })
}

/// Keys that don't type a character.
fn named(keysym: u32) -> Option<u16> {
    Some(match keysym {
        0xff08 => 51,          // BackSpace -> Delete
        0xff09 | 0xfe20 => 48, // Tab, ISO_Left_Tab
        0xff0d => 36,          // Return
        0xff8d => 76,          // KP_Enter
        0xff1b => 53,          // Escape
        0xffff => 117,         // Delete -> Forward Delete
        0xff50 => 115,         // Home
        0xff51 => 123,         // Left
        0xff52 => 126,         // Up
        0xff53 => 124,         // Right
        0xff54 => 125,         // Down
        0xff55 => 116,         // Page Up
        0xff56 => 121,         // Page Down
        0xff57 => 119,         // End
        0xff63 => 114,         // Insert -> Help
        0xff67 => 110,         // Menu -> Contextual Menu
        0xff7f => 71,          // Num_Lock -> Clear
        0xff61 => 105,         // Print -> F13
        0xff14 => 107,         // Scroll_Lock -> F14
        0xff13 => 113,         // Pause -> F15
        0x20 => 49,            // space
        0xffbe..=0xffd5 => {
            // F1..F20
            const F: [u16; 20] = [
                122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79,
                80, 90,
            ];
            *F.get((keysym - 0xffbe) as usize)?
        }
        _ => return None,
    })
}

/// Shift and Option as UCKeyTranslate modifier states (Carbon modifiers >> 8).
const UC_SHIFT: u32 = 0x02;
const UC_OPTION: u32 = 0x08;

/// Character -> (virtual key, needs Shift, needs Option) for the current layout.
fn layout_map() -> HashMap<char, (u16, bool, bool)> {
    let mut map = HashMap::new();
    // Keyboard layout APIs must be called on the main thread.
    let ok = dispatch2::run_on_main(|_| unsafe {
        let src = TISCopyCurrentKeyboardLayoutInputSource();
        if src.is_null() {
            return false;
        }
        let data = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            CFRelease(src);
            return false;
        }
        let layout = CFDataGetBytePtr(data);
        let kbd = LMGetKbdType() as u32;
        // Plain keys first, then Shift, Option, Shift+Option; the main block before
        // the keypad, so digits come from the number row.
        let keys: Vec<u16> = (0..65).chain(93..128).chain(65..93).collect();
        for mods in [0, UC_SHIFT, UC_OPTION, UC_SHIFT | UC_OPTION] {
            for &kc in &keys {
                let mut dead = 0u32;
                let mut buf = [0u16; 4];
                let mut len = 0usize;
                let st = UCKeyTranslate(
                    layout,
                    kc,
                    kUCKeyActionDown,
                    mods,
                    kbd,
                    0,
                    &mut dead,
                    buf.len(),
                    &mut len,
                    buf.as_mut_ptr(),
                );
                // Dead keys (accents that wait for the next key) are typed as text instead.
                if st != 0 || dead != 0 || len == 0 {
                    continue;
                }
                let mut it = char::decode_utf16(buf[..len].iter().copied());
                if let (Some(Ok(c)), None) = (it.next(), it.next()) {
                    if !c.is_control() {
                        map.entry(c)
                            .or_insert((kc, mods & UC_SHIFT != 0, mods & UC_OPTION != 0));
                    }
                }
            }
        }
        CFRelease(src);
        true
    });
    if !ok {
        log::warn!("keyboard layout unavailable; typing characters as text");
    }
    map
}

struct Injector {
    src: CGEventSourceRef,
    chars: HashMap<char, (u16, bool, bool)>,
    chars_at: Instant,
    /// keysym -> (virtual key, extra flags it was pressed with).
    pressed: HashMap<u32, (u16, u64)>,
    /// Held modifier keysyms and their flags.
    mods: HashMap<u32, u64>,
    buttons: [bool; 16],
    pos: CGPoint,
    last_click: (Instant, CGPoint, u8),
    clicks: i64,
    scroll_acc: (f64, f64),
    clipboard: Option<super::desktop::ClipboardHandle>,
}

unsafe impl Send for Injector {}

impl Injector {
    fn flags(&self) -> u64 {
        self.mods.values().fold(0, |a, &f| a | f)
    }

    fn post(&self, e: CGEventRef) {
        if !e.is_null() {
            unsafe {
                CGEventPost(kCGHIDEventTap, e);
                CFRelease(e as CFTypeRef);
            }
        }
    }

    fn to_point(&self, x: i32, y: i32) -> CGPoint {
        let Some(g) = *super::GEOMETRY.lock() else {
            return CGPoint::default();
        };
        let k = 1.0 / g.scale();
        CGPoint {
            x: g.bounds.origin.x + (x as f64 + 0.5) * k,
            y: g.bounds.origin.y + (y as f64 + 0.5) * k,
        }
    }

    fn mouse(&self, ty: u32, button: u32) {
        let e = unsafe { CGEventCreateMouseEvent(self.src, ty, self.pos, button) };
        if e.is_null() {
            return;
        }
        unsafe {
            CGEventSetFlags(e, self.flags() | NX_NONCOALSESCEDMASK);
            if ty != kCGEventMouseMoved {
                CGEventSetIntegerValueField(e, kCGMouseEventClickState, self.clicks);
            }
            CGEventSetIntegerValueField(e, kCGMouseEventButtonNumber, button as i64);
        }
        self.post(e);
    }

    fn key_event(&self, kc: u16, down: bool, flags: u64, repeat: bool) {
        let e = unsafe { CGEventCreateKeyboardEvent(self.src, kc, down) };
        if e.is_null() {
            return;
        }
        unsafe {
            CGEventSetFlags(e, flags | NX_NONCOALSESCEDMASK);
            if repeat {
                CGEventSetIntegerValueField(e, kCGKeyboardEventAutorepeat, 1);
            }
        }
        self.post(e);
    }

    fn type_text(&self, c: char) {
        let mut buf = [0u16; 2];
        let s = c.encode_utf16(&mut buf);
        for down in [true, false] {
            let e = unsafe { CGEventCreateKeyboardEvent(self.src, 0, down) };
            if e.is_null() {
                return;
            }
            unsafe {
                CGEventSetFlags(e, NX_NONCOALSESCEDMASK);
                CGEventKeyboardSetUnicodeString(e, s.len(), s.as_ptr());
            }
            self.post(e);
        }
    }

    fn handle(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Move { x, y } => {
                self.pos = self.to_point(x, y);
                let (ty, b) = if self.buttons[1] {
                    (kCGEventLeftMouseDragged, 0)
                } else if self.buttons[3] {
                    (kCGEventRightMouseDragged, 1)
                } else if self.buttons[2] {
                    (kCGEventOtherMouseDragged, 2)
                } else {
                    (kCGEventMouseMoved, 0)
                };
                self.mouse(ty, b);
            }
            InputEvent::Button { button, down } => {
                let b = button as usize;
                if b >= self.buttons.len() || self.buttons[b] == down {
                    return;
                }
                self.buttons[b] = down;
                if down {
                    // macOS wants the click count on the events for double/triple clicks.
                    let (t, p, last) = self.last_click;
                    let near = (p.x - self.pos.x).abs() < 4.0 && (p.y - self.pos.y).abs() < 4.0;
                    self.clicks =
                        if last == button && near && t.elapsed() < Duration::from_millis(500) {
                            self.clicks % 3 + 1
                        } else {
                            1
                        };
                    self.last_click = (Instant::now(), self.pos, button);
                }
                // X buttons: 1 left, 2 middle, 3 right, 8 back, 9 forward.
                let (dn, up, mb) = match button {
                    1 => (kCGEventLeftMouseDown, kCGEventLeftMouseUp, 0),
                    3 => (kCGEventRightMouseDown, kCGEventRightMouseUp, 1),
                    2 => (kCGEventOtherMouseDown, kCGEventOtherMouseUp, 2),
                    8 => (kCGEventOtherMouseDown, kCGEventOtherMouseUp, 3),
                    9 => (kCGEventOtherMouseDown, kCGEventOtherMouseUp, 4),
                    _ => return,
                };
                self.mouse(if down { dn } else { up }, mb);
            }
            InputEvent::Scroll { dx, dy } => {
                let e = unsafe {
                    CGEventCreateScrollWheelEvent2(
                        self.src,
                        kCGScrollEventUnitLine,
                        2,
                        -dy.clamp(-20, 20),
                        -dx.clamp(-20, 20),
                        0,
                    )
                };
                self.post(e);
            }
            InputEvent::ScrollPx { dx, dy } => {
                // Desktop pixels -> points; macOS scrolls content down for positive values.
                let k = super::GEOMETRY
                    .lock()
                    .map(|g| 1.0 / g.scale())
                    .unwrap_or(1.0);
                self.scroll_acc.0 -= dx * k;
                self.scroll_acc.1 -= dy * k;
                let (x, y) = (self.scroll_acc.0.trunc(), self.scroll_acc.1.trunc());
                if x == 0.0 && y == 0.0 {
                    return;
                }
                self.scroll_acc.0 -= x;
                self.scroll_acc.1 -= y;
                let e = unsafe {
                    CGEventCreateScrollWheelEvent2(
                        self.src,
                        kCGScrollEventUnitPixel,
                        2,
                        y as i32,
                        x as i32,
                        0,
                    )
                };
                if !e.is_null() {
                    unsafe { CGEventSetIntegerValueField(e, kCGScrollWheelEventIsContinuous, 1) };
                }
                self.post(e);
            }
            InputEvent::Key { keysym, down } => self.key(keysym, down),
            InputEvent::Clipboard(content) => {
                if let Some(c) = &self.clipboard {
                    c.set_blocking(content);
                }
            }
            InputEvent::ReleaseAll | InputEvent::Shutdown => self.release_all(),
            InputEvent::DisplayBarrier(done) => {
                self.release_all();
                let _ = done.send(());
            }
        }
    }

    fn key(&mut self, keysym: u32, down: bool) {
        if let Some((kc, flag)) = modifier(keysym) {
            if down == self.mods.contains_key(&keysym) {
                return;
            }
            if down {
                self.mods.insert(keysym, flag);
            } else {
                self.mods.remove(&keysym);
            }
            // Modifier keys are flag changes, carrying the state after the change.
            let e = unsafe { CGEventCreateKeyboardEvent(self.src, kc, down) };
            if !e.is_null() {
                unsafe {
                    CGEventSetType(e, kCGEventFlagsChanged);
                    CGEventSetFlags(e, self.flags() | NX_NONCOALSESCEDMASK);
                }
                self.post(e);
            }
            return;
        }
        if !down {
            if let Some((kc, extra)) = self.pressed.remove(&keysym) {
                self.key_event(kc, false, self.flags() | extra, false);
            }
            return;
        }
        if let Some(&(kc, extra)) = self.pressed.get(&keysym) {
            // Autorepeat.
            self.key_event(kc, true, self.flags() | extra, true);
            return;
        }
        if let Some(kc) = named(keysym) {
            self.pressed.insert(keysym, (kc, 0));
            self.key_event(kc, true, self.flags(), false);
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
        if self.chars_at.elapsed() > Duration::from_secs(3) {
            // Pick up layout switches.
            self.chars = layout_map();
            self.chars_at = Instant::now();
        }
        match self.chars.get(&c).copied() {
            Some((kc, shift, option)) => {
                // The character decides Shift and Option (the viewer's layout already
                // applied them); Command and Control stay as held, for shortcuts. Alt
                // held with a plain letter stays too (Option-letter shortcuts).
                let held = self.flags();
                let keep_option = !option && c.is_ascii_alphanumeric();
                let mut flags = held
                    & !(kCGEventFlagMaskShift | NX_DEVICELSHIFTKEYMASK | NX_DEVICERSHIFTKEYMASK);
                if !keep_option {
                    flags &=
                        !(kCGEventFlagMaskAlternate | NX_DEVICELALTKEYMASK | NX_DEVICERALTKEYMASK);
                }
                let mut extra = 0;
                if shift {
                    extra |= kCGEventFlagMaskShift;
                }
                if option {
                    extra |= kCGEventFlagMaskAlternate;
                }
                // Release uses the same flags, so remember them relative to `held`.
                let flags = flags | extra;
                self.pressed.insert(keysym, (kc, flags & !held));
                self.key_event(kc, true, flags, false);
            }
            None => self.type_text(c),
        }
    }

    fn release_all(&mut self) {
        for (_, (kc, extra)) in std::mem::take(&mut self.pressed) {
            self.key_event(kc, false, self.flags() | extra, false);
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
    _px_per_notch: f64,
) -> Result<mpsc::Sender<InputEvent>, String> {
    let src = unsafe { CGEventSourceCreate(kCGEventSourceStateHIDSystemState) };
    if src.is_null() {
        return Err("CGEventSourceCreate failed".into());
    }
    let chars = layout_map();
    log::info!("keyboard layout: {} characters", chars.len());
    let mut inj = Injector {
        src,
        chars,
        chars_at: Instant::now(),
        pressed: HashMap::new(),
        mods: HashMap::new(),
        buttons: [false; 16],
        pos: CGPoint::default(),
        last_click: (Instant::now(), CGPoint::default(), 0),
        clicks: 1,
        scroll_acc: (0.0, 0.0),
        clipboard,
    };
    let (tx, rx) = mpsc::channel::<InputEvent>();
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
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
