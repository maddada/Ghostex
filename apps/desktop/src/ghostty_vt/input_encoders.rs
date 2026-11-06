use super::*;
use std::ffi::c_void;

/// The cleared key-event text (`(GhosttyString){0}` in key/event.h).
const EMPTY_UTF8: ffi::GhosttyString = ffi::GhosttyString {
    ptr: std::ptr::null(),
    len: 0,
};

/// Physical key identity for key encoding (key/event.h `GhosttyKey`).
pub type VtKey = ffi::GhosttyKey;

/// Modifier bitmask for key/mouse encoding (`GHOSTTY_MODS_*`).
pub type VtMods = ffi::GhosttyMods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtKeyAction {
    Press,
    Release,
    Repeat,
}

/// macOS option-key behavior for the key encoder (`macos-option-as-alt`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VtOptionAsAlt {
    #[default]
    False,
    True,
    Left,
    Right,
}

/// One key event to encode. `utf8` is the layout-produced text BEFORE any
/// ctrl/meta transformation (never C0 controls or macOS PUA function-key
/// codes; pass `None` and let the logical `key` drive encoding instead).
#[derive(Clone, Copy, Debug)]
pub struct VtKeyInput<'a> {
    pub action: VtKeyAction,
    pub key: VtKey,
    pub mods: VtMods,
    /// Mods already consumed by the platform to produce `utf8` (e.g. shift
    /// in "A", option in "ß"); the encoder won't re-apply them.
    pub consumed_mods: VtMods,
    pub utf8: Option<&'a str>,
    /// Codepoint the key produces without any modifiers (0 when unknown).
    pub unshifted_codepoint: u32,
    /// IME composition (marked text) is active. The encoder suppresses
    /// everything except plain modifiers so composition-owned keys never
    /// reach the PTY.
    pub composing: bool,
}

/// Key encoder plus its reusable event handle. Encodes key events into
/// legacy or Kitty escape sequences based on options synced from the live
/// terminal ([`sync_from_terminal`](Self::sync_from_terminal)), so DECCKM,
/// modifyOtherKeys, and Kitty flags always match what the running program
/// asked for.
pub struct VtKeyEncoder {
    encoder: ffi::GhosttyKeyEncoder,
    event: ffi::GhosttyKeyEvent,
}

// SAFETY: encoder/event state is self-contained with no thread affinity;
// &mut methods enforce exclusive access like the other handles here.
unsafe impl Send for VtKeyEncoder {}

impl VtKeyEncoder {
    pub fn new() -> Result<Self, VtError> {
        let mut encoder: ffi::GhosttyKeyEncoder = std::ptr::null_mut();
        check(unsafe { ffi::ghostty_key_encoder_new(std::ptr::null(), &mut encoder) })?;
        let mut event: ffi::GhosttyKeyEvent = std::ptr::null_mut();
        if let Err(error) =
            check(unsafe { ffi::ghostty_key_event_new(std::ptr::null(), &mut event) })
        {
            unsafe { ffi::ghostty_key_encoder_free(encoder) };
            return Err(error);
        }
        Ok(Self { encoder, event })
    }

    /// Sync encoder options (cursor-key application, keypad mode, Kitty
    /// flags, ...) from the terminal's current state, then re-apply the
    /// host-owned option-as-alt setting the sync resets.
    pub fn sync_from_terminal(&mut self, terminal: &mut VtTerminal, option_as_alt: VtOptionAsAlt) {
        let _ = unsafe { ffi::ghostty_key_encoder_set_from_terminal(self.encoder, terminal.raw) };
        let value: ffi::GhosttyOptionAsAlt = match option_as_alt {
            VtOptionAsAlt::False => ffi::GHOSTTY_OPTION_AS_ALT_FALSE,
            VtOptionAsAlt::True => ffi::GHOSTTY_OPTION_AS_ALT_TRUE,
            VtOptionAsAlt::Left => ffi::GHOSTTY_OPTION_AS_ALT_LEFT,
            VtOptionAsAlt::Right => ffi::GHOSTTY_OPTION_AS_ALT_RIGHT,
        };
        let _ = unsafe {
            ffi::ghostty_key_encoder_set(
                self.encoder,
                ffi::GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT,
                (&raw const value).cast::<c_void>(),
            )
        };
    }

    /// Encode one key event, appending the bytes to `out`. Empty output
    /// (e.g. bare modifier presses) is success. The library borrows
    /// `input.utf8` without copying, so the pointer is cleared again before
    /// returning — the reusable event must never outlive the borrow.
    pub fn encode(&mut self, input: &VtKeyInput<'_>, out: &mut Vec<u8>) -> Result<(), VtError> {
        let action = match input.action {
            VtKeyAction::Press => ffi::GHOSTTY_KEY_ACTION_PRESS,
            VtKeyAction::Release => ffi::GHOSTTY_KEY_ACTION_RELEASE,
            VtKeyAction::Repeat => ffi::GHOSTTY_KEY_ACTION_REPEAT,
        };
        // The setters only fail for a NULL event or an out-of-range enum,
        // neither of which this wrapper can produce.
        unsafe {
            let _ = ffi::ghostty_key_event_set_action(self.event, action);
            let _ = ffi::ghostty_key_event_set_key(self.event, input.key);
            let _ = ffi::ghostty_key_event_set_mods(self.event, input.mods);
            let _ = ffi::ghostty_key_event_set_consumed_mods(self.event, input.consumed_mods);
            let _ = ffi::ghostty_key_event_set_composing(self.event, input.composing);
            let _ = ffi::ghostty_key_event_set_unshifted_codepoint(
                self.event,
                input.unshifted_codepoint,
            );
            let _ = ffi::ghostty_key_event_set_utf8(
                self.event,
                match input.utf8 {
                    Some(text) => ffi::GhosttyString {
                        ptr: text.as_ptr(),
                        len: text.len(),
                    },
                    None => EMPTY_UTF8,
                },
            );
        }
        let result = encode_with_retry(out, |buf, len, written| unsafe {
            ffi::ghostty_key_encoder_encode(self.encoder, self.event, buf, len, written)
        });
        let _ = unsafe { ffi::ghostty_key_event_set_utf8(self.event, EMPTY_UTF8) };
        result
    }
}

impl Drop for VtKeyEncoder {
    fn drop(&mut self) {
        unsafe {
            ffi::ghostty_key_event_free(self.event);
            ffi::ghostty_key_encoder_free(self.encoder);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtMouseAction {
    Press,
    Release,
    Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtMouseButton {
    Left,
    Right,
    Middle,
    /// Wheel up.
    WheelUp,
    /// Wheel down.
    WheelDown,
}

/// One mouse event to encode. Position is in pixels relative to the grid
/// origin, in the same pixel space as [`VtMouseEncoder::set_size`].
#[derive(Clone, Copy, Debug)]
pub struct VtMouseInput {
    pub action: VtMouseAction,
    /// `None` for buttonless motion (hover reporting in any-event mode).
    pub button: Option<VtMouseButton>,
    pub mods: VtMods,
    pub x: f32,
    pub y: f32,
}

/// Mouse encoder plus its reusable event handle. Tracking mode and output
/// format sync from the live terminal; the encoder itself filters events the
/// active tracking mode does not report (returning empty output).
pub struct VtMouseEncoder {
    encoder: ffi::GhosttyMouseEncoder,
    event: ffi::GhosttyMouseEvent,
}

// SAFETY: same reasoning as VtKeyEncoder.
unsafe impl Send for VtMouseEncoder {}

impl VtMouseEncoder {
    pub fn new() -> Result<Self, VtError> {
        let mut encoder: ffi::GhosttyMouseEncoder = std::ptr::null_mut();
        check(unsafe { ffi::ghostty_mouse_encoder_new(std::ptr::null(), &mut encoder) })?;
        let mut event: ffi::GhosttyMouseEvent = std::ptr::null_mut();
        if let Err(error) =
            check(unsafe { ffi::ghostty_mouse_event_new(std::ptr::null(), &mut event) })
        {
            unsafe { ffi::ghostty_mouse_encoder_free(encoder) };
            return Err(error);
        }
        // Dedup motion events by cell so drag reporting doesn't flood the
        // PTY with one event per pixel.
        let track: bool = true;
        let _ = unsafe {
            ffi::ghostty_mouse_encoder_set(
                encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL,
                (&raw const track).cast::<c_void>(),
            )
        };
        Ok(Self { encoder, event })
    }

    /// Sync tracking mode and output format from the terminal's state.
    pub fn sync_from_terminal(&mut self, terminal: &mut VtTerminal) {
        let _ = unsafe { ffi::ghostty_mouse_encoder_set_from_terminal(self.encoder, terminal.raw) };
    }

    /// Set the rendered geometry used to convert pixel positions to cells.
    /// All values share one pixel space (logical or device, consistently).
    pub fn set_size(
        &mut self,
        screen_width: u32,
        screen_height: u32,
        cell_width: u32,
        cell_height: u32,
    ) {
        let mut size = ffi::GhosttyMouseEncoderSize::init_sized();
        size.screen_width = screen_width;
        size.screen_height = screen_height;
        size.cell_width = cell_width.max(1);
        size.cell_height = cell_height.max(1);
        let _ = unsafe {
            ffi::ghostty_mouse_encoder_set(
                self.encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                (&raw const size).cast::<c_void>(),
            )
        };
    }

    /// Tell the encoder whether any button is held (button-event tracking
    /// only reports motion while a button is pressed).
    pub fn set_any_button_pressed(&mut self, pressed: bool) {
        let _ = unsafe {
            ffi::ghostty_mouse_encoder_set(
                self.encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
                (&raw const pressed).cast::<c_void>(),
            )
        };
    }

    /// Encode one mouse event, appending the bytes to `out`. Empty output
    /// means the active tracking mode does not report this event.
    pub fn encode(&mut self, input: &VtMouseInput, out: &mut Vec<u8>) -> Result<(), VtError> {
        let action = match input.action {
            VtMouseAction::Press => ffi::GHOSTTY_MOUSE_ACTION_PRESS,
            VtMouseAction::Release => ffi::GHOSTTY_MOUSE_ACTION_RELEASE,
            VtMouseAction::Motion => ffi::GHOSTTY_MOUSE_ACTION_MOTION,
        };
        // As with key events, the setters only fail for a NULL event or an
        // out-of-range enum.
        unsafe {
            let _ = ffi::ghostty_mouse_event_set_action(self.event, action);
            let _ = match input.button {
                Some(button) => ffi::ghostty_mouse_event_set_button(
                    self.event,
                    match button {
                        VtMouseButton::Left => ffi::GHOSTTY_MOUSE_BUTTON_LEFT,
                        VtMouseButton::Right => ffi::GHOSTTY_MOUSE_BUTTON_RIGHT,
                        VtMouseButton::Middle => ffi::GHOSTTY_MOUSE_BUTTON_MIDDLE,
                        VtMouseButton::WheelUp => ffi::GHOSTTY_MOUSE_BUTTON_FOUR,
                        VtMouseButton::WheelDown => ffi::GHOSTTY_MOUSE_BUTTON_FIVE,
                    },
                ),
                None => ffi::ghostty_mouse_event_clear_button(self.event),
            };
            let _ = ffi::ghostty_mouse_event_set_mods(self.event, input.mods);
            let _ = ffi::ghostty_mouse_event_set_position(
                self.event,
                ffi::GhosttyMousePosition {
                    x: input.x,
                    y: input.y,
                },
            );
        }
        encode_with_retry(out, |buf, len, written| unsafe {
            ffi::ghostty_mouse_encoder_encode(self.encoder, self.event, buf, len, written)
        })
    }
}

impl Drop for VtMouseEncoder {
    fn drop(&mut self) {
        unsafe {
            ffi::ghostty_mouse_event_free(self.event);
            ffi::ghostty_mouse_encoder_free(self.encoder);
        }
    }
}

/// Encode paste data for the PTY: strips unsafe control bytes and applies
/// bracketed-paste wrapping when `bracketed` is true (newlines become CRs
/// when it is false).
pub fn encode_paste(text: &str, bracketed: bool) -> Result<Vec<u8>, VtError> {
    // The library sanitizes the input buffer in place, so encode from an
    // owned scratch copy (the stripping is idempotent across the size-query
    // retry).
    let mut data = text.as_bytes().to_vec();
    let mut out: Vec<u8> = Vec::new();
    encode_with_retry(&mut out, |buf, len, written| unsafe {
        ffi::ghostty_paste_encode(data.as_mut_ptr(), data.len(), bracketed, buf, len, written)
    })?;
    Ok(out)
}

/// Encode a focus gained/lost report (CSI I / CSI O) for mode 1004.
pub fn encode_focus(gained: bool) -> Result<Vec<u8>, VtError> {
    let event = if gained {
        ffi::GHOSTTY_FOCUS_GAINED
    } else {
        ffi::GHOSTTY_FOCUS_LOST
    };
    let mut out: Vec<u8> = Vec::new();
    encode_with_retry(&mut out, |buf, len, written| unsafe {
        ffi::ghostty_focus_encode(event, buf, len, written)
    })?;
    Ok(out)
}

/// Shared buffer-sizing pattern for the vt `*_encode` calls: try a stack
/// buffer, retry once with the exact size on `GHOSTTY_OUT_OF_SPACE`, and
/// append the encoded bytes to `out`.
pub(crate) fn encode_with_retry(
    out: &mut Vec<u8>,
    mut encode: impl FnMut(*mut u8, usize, *mut usize) -> ffi::GhosttyResult,
) -> Result<(), VtError> {
    let mut buffer = [0u8; 256];
    let mut written: usize = 0;
    match encode(buffer.as_mut_ptr(), buffer.len(), &mut written) {
        ffi::GHOSTTY_SUCCESS => {
            out.extend_from_slice(&buffer[..written]);
            Ok(())
        }
        ffi::GHOSTTY_OUT_OF_SPACE => {
            let mut grown = vec![0u8; written];
            check(encode(grown.as_mut_ptr(), grown.len(), &mut written))?;
            grown.truncate(written);
            out.extend_from_slice(&grown);
            Ok(())
        }
        code => Err(VtError { code }),
    }
}
