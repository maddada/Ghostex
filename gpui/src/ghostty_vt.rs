#![allow(dead_code)]

/*
CDXC:GPUILibghosttyVt 2026-07-03:
Phase 1 GPUI-composited terminals are driven by libghostty-vt (vendored under
ghostty/, MIT), whose C API is functionally stable but explicitly NOT
API-stable. This module is the single choke point over that C API: every
libghostty-vt symbol, struct layout, and enum value used by Rust lives here so
a vendored API bump touches one file. Do not declare ghostty_vt symbols in
other modules, and do not expose raw handles outside this module.

CDXC:GPUILibghosttyVt 2026-07-03 (dirty-tracking contract):
render.h keeps two INDEPENDENT dirty layers: a global render-state dirty value
(false/partial/full) and a per-row dirty flag. ghostty_render_state_update()
only ever raises dirty state; it never clears either layer, and clearing one
layer does not clear the other. The renderer (caller) must clear BOTH after
consuming a frame: per-row via VtRow::clear_dirty() while iterating, global
via VtRenderState::clear_dirty() after the frame. Skipping either leaves the
next frame reporting stale dirtiness.

Threading: a terminal plus its render state have no thread affinity but no
internal synchronization either. ghostty_render_state_update() needs exclusive
access to the terminal only for the duration of the call ("short lock");
reading rows/cells afterwards touches only the render-state snapshot. Rust
expresses this as &mut borrows here; cross-thread callers (P1b's PTY reader
vs. render path) must wrap the VtTerminal in a lock held across feed/resize
and update, while row readback can happen outside that lock. Row and cell
data borrowed from the render state is invalidated by the next update, which
the lifetimes below enforce at compile time.
*/

use std::{ffi::c_void, fmt, marker::PhantomData};

pub mod ffi {
    #![allow(non_camel_case_types)]

    use std::ffi::{c_int, c_void};

    pub type GhosttyResult = c_int;
    pub const GHOSTTY_SUCCESS: GhosttyResult = 0;
    pub const GHOSTTY_OUT_OF_MEMORY: GhosttyResult = -1;
    pub const GHOSTTY_INVALID_VALUE: GhosttyResult = -2;
    pub const GHOSTTY_OUT_OF_SPACE: GhosttyResult = -3;
    pub const GHOSTTY_NO_VALUE: GhosttyResult = -4;

    pub type GhosttyTerminal = *mut c_void;
    pub type GhosttyRenderState = *mut c_void;
    pub type GhosttyRenderStateRowIterator = *mut c_void;
    pub type GhosttyRenderStateRowCells = *mut c_void;

    /// Opaque cell value (`GhosttyCell` in screen.h).
    pub type GhosttyCell = u64;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct GhosttyColorRgb {
        pub r: u8,
        pub g: u8,
        pub b: u8,
    }

    pub type GhosttyRenderStateDirty = c_int;
    pub const GHOSTTY_RENDER_STATE_DIRTY_FALSE: GhosttyRenderStateDirty = 0;
    pub const GHOSTTY_RENDER_STATE_DIRTY_PARTIAL: GhosttyRenderStateDirty = 1;
    pub const GHOSTTY_RENDER_STATE_DIRTY_FULL: GhosttyRenderStateDirty = 2;

    pub type GhosttyRenderStateData = c_int;
    pub const GHOSTTY_RENDER_STATE_DATA_COLS: GhosttyRenderStateData = 1;
    pub const GHOSTTY_RENDER_STATE_DATA_ROWS: GhosttyRenderStateData = 2;
    pub const GHOSTTY_RENDER_STATE_DATA_DIRTY: GhosttyRenderStateData = 3;
    pub const GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR: GhosttyRenderStateData = 4;
    pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE: GhosttyRenderStateData = 11;
    pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE: GhosttyRenderStateData = 14;
    pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X: GhosttyRenderStateData = 15;
    pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y: GhosttyRenderStateData = 16;

    pub type GhosttyRenderStateOption = c_int;
    pub const GHOSTTY_RENDER_STATE_OPTION_DIRTY: GhosttyRenderStateOption = 0;

    pub type GhosttyRenderStateRowData = c_int;
    pub const GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY: GhosttyRenderStateRowData = 1;
    pub const GHOSTTY_RENDER_STATE_ROW_DATA_RAW: GhosttyRenderStateRowData = 2;
    pub const GHOSTTY_RENDER_STATE_ROW_DATA_CELLS: GhosttyRenderStateRowData = 3;

    pub type GhosttyRenderStateRowOption = c_int;
    pub const GHOSTTY_RENDER_STATE_ROW_OPTION_DIRTY: GhosttyRenderStateRowOption = 0;

    pub type GhosttyRenderStateRowCellsData = c_int;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW: GhosttyRenderStateRowCellsData = 1;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE: GhosttyRenderStateRowCellsData = 2;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN: GhosttyRenderStateRowCellsData = 3;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF: GhosttyRenderStateRowCellsData = 4;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR: GhosttyRenderStateRowCellsData = 5;
    pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR: GhosttyRenderStateRowCellsData = 6;

    pub type GhosttyCellData = c_int;
    pub const GHOSTTY_CELL_DATA_WIDE: GhosttyCellData = 3;
    pub const GHOSTTY_CELL_DATA_HAS_HYPERLINK: GhosttyCellData = 7;
    pub const GHOSTTY_CELL_DATA_SEMANTIC_CONTENT: GhosttyCellData = 9;

    /// screen.h `GhosttyCellSemanticContent` (OSC 133 cell classification).
    pub type GhosttyCellSemanticContent = c_int;
    pub const GHOSTTY_CELL_SEMANTIC_OUTPUT: GhosttyCellSemanticContent = 0;
    pub const GHOSTTY_CELL_SEMANTIC_INPUT: GhosttyCellSemanticContent = 1;
    pub const GHOSTTY_CELL_SEMANTIC_PROMPT: GhosttyCellSemanticContent = 2;

    /// Opaque row value (`GhosttyRow` in screen.h).
    pub type GhosttyRow = u64;

    pub type GhosttyRowData = c_int;
    pub const GHOSTTY_ROW_DATA_WRAP: GhosttyRowData = 1;
    pub const GHOSTTY_ROW_DATA_WRAP_CONTINUATION: GhosttyRowData = 2;
    pub const GHOSTTY_ROW_DATA_SEMANTIC_PROMPT: GhosttyRowData = 6;

    /// screen.h `GhosttyRowSemanticPrompt` (OSC 133 row classification).
    pub type GhosttyRowSemanticPrompt = c_int;
    pub const GHOSTTY_ROW_SEMANTIC_NONE: GhosttyRowSemanticPrompt = 0;

    /// types.h `GhosttyString`: a borrowed byte string. Lifetime is bounded
    /// by the producing API (terminal title/pwd stay valid until the next
    /// `ghostty_terminal_vt_write`/`ghostty_terminal_reset`), so wrappers
    /// must copy the bytes out before releasing terminal access.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyString {
        pub ptr: *const u8,
        pub len: usize,
    }

    /// terminal.h `GhosttyTerminalScrollbar`: viewport position within the
    /// scrollable area, in rows.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub struct GhosttyTerminalScrollbar {
        pub total: u64,
        pub offset: u64,
        pub len: u64,
    }

    /// point.h coordinate/tagged-point types for grid references.
    pub type GhosttyPointTag = c_int;
    pub const GHOSTTY_POINT_TAG_ACTIVE: GhosttyPointTag = 0;
    pub const GHOSTTY_POINT_TAG_VIEWPORT: GhosttyPointTag = 1;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct GhosttyPointCoordinate {
        pub x: u16,
        pub y: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union GhosttyPointValue {
        pub coordinate: GhosttyPointCoordinate,
        pub _padding: [u64; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyPoint {
        pub tag: GhosttyPointTag,
        pub value: GhosttyPointValue,
    }

    /// Sized struct (grid_ref.h). A resolved cell reference, valid only
    /// until the next mutating terminal call.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyGridRef {
        pub size: usize,
        pub node: *mut c_void,
        pub x: u16,
        pub y: u16,
    }

    impl GhosttyGridRef {
        pub fn init_sized() -> Self {
            let mut grid_ref: Self = unsafe { std::mem::zeroed() };
            grid_ref.size = std::mem::size_of::<Self>();
            grid_ref
        }
    }

    pub type GhosttyCellWide = c_int;
    pub const GHOSTTY_CELL_WIDE_NARROW: GhosttyCellWide = 0;
    pub const GHOSTTY_CELL_WIDE_WIDE: GhosttyCellWide = 1;
    pub const GHOSTTY_CELL_WIDE_SPACER_TAIL: GhosttyCellWide = 2;
    pub const GHOSTTY_CELL_WIDE_SPACER_HEAD: GhosttyCellWide = 3;

    /// sgr.h `GhosttySgrUnderline`: value of [`GhosttyStyle::underline`].
    pub type GhosttySgrUnderline = c_int;
    pub const GHOSTTY_SGR_UNDERLINE_NONE: GhosttySgrUnderline = 0;
    pub const GHOSTTY_SGR_UNDERLINE_SINGLE: GhosttySgrUnderline = 1;
    pub const GHOSTTY_SGR_UNDERLINE_DOUBLE: GhosttySgrUnderline = 2;
    pub const GHOSTTY_SGR_UNDERLINE_CURLY: GhosttySgrUnderline = 3;
    pub const GHOSTTY_SGR_UNDERLINE_DOTTED: GhosttySgrUnderline = 4;
    pub const GHOSTTY_SGR_UNDERLINE_DASHED: GhosttySgrUnderline = 5;

    pub type GhosttyTerminalOption = c_int;
    pub const GHOSTTY_TERMINAL_OPT_USERDATA: GhosttyTerminalOption = 0;
    pub const GHOSTTY_TERMINAL_OPT_WRITE_PTY: GhosttyTerminalOption = 1;
    pub const GHOSTTY_TERMINAL_OPT_BELL: GhosttyTerminalOption = 2;
    pub const GHOSTTY_TERMINAL_OPT_TITLE_CHANGED: GhosttyTerminalOption = 5;
    pub const GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND: GhosttyTerminalOption = 11;
    pub const GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND: GhosttyTerminalOption = 12;
    pub const GHOSTTY_TERMINAL_OPT_COLOR_CURSOR: GhosttyTerminalOption = 13;
    pub const GHOSTTY_TERMINAL_OPT_COLOR_PALETTE: GhosttyTerminalOption = 14;
    pub const GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE: GhosttyTerminalOption = 26;
    pub const GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES: GhosttyTerminalOption = 27;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct GhosttyTerminalModeConfig {
        pub mode: GhosttyMode,
        pub value: bool,
    }

    pub type GhosttyTerminalData = c_int;
    pub const GHOSTTY_TERMINAL_DATA_CURSOR_X: GhosttyTerminalData = 3;
    pub const GHOSTTY_TERMINAL_DATA_CURSOR_Y: GhosttyTerminalData = 4;
    pub const GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN: GhosttyTerminalData = 6;
    pub const GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS: GhosttyTerminalData = 8;
    pub const GHOSTTY_TERMINAL_DATA_SCROLLBAR: GhosttyTerminalData = 9;
    pub const GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING: GhosttyTerminalData = 11;
    pub const GHOSTTY_TERMINAL_DATA_TITLE: GhosttyTerminalData = 12;
    pub const GHOSTTY_TERMINAL_DATA_PWD: GhosttyTerminalData = 13;
    pub const GHOSTTY_TERMINAL_DATA_MODE: GhosttyTerminalData = 37;

    pub type GhosttyTerminalScreen = c_int;
    pub const GHOSTTY_TERMINAL_SCREEN_PRIMARY: GhosttyTerminalScreen = 0;
    pub const GHOSTTY_TERMINAL_SCREEN_ALTERNATE: GhosttyTerminalScreen = 1;

    /// modes.h `GhosttyMode`: packed 16-bit mode id, bits 0-14 the mode
    /// value, bit 15 set for ANSI modes (clear for DEC private modes).
    pub type GhosttyMode = u16;
    pub const GHOSTTY_MODE_ALT_SCROLL: GhosttyMode = 1007;
    pub const GHOSTTY_MODE_FOCUS_EVENT: GhosttyMode = 1004;
    pub const GHOSTTY_MODE_BRACKETED_PASTE: GhosttyMode = 2004;

    pub type GhosttyTerminalScrollViewportTag = c_int;
    pub const GHOSTTY_SCROLL_VIEWPORT_TOP: GhosttyTerminalScrollViewportTag = 0;
    pub const GHOSTTY_SCROLL_VIEWPORT_BOTTOM: GhosttyTerminalScrollViewportTag = 1;
    pub const GHOSTTY_SCROLL_VIEWPORT_DELTA: GhosttyTerminalScrollViewportTag = 2;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union GhosttyTerminalScrollViewportValue {
        pub delta: isize,
        pub _padding: [u64; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyTerminalScrollViewport {
        pub tag: GhosttyTerminalScrollViewportTag,
        pub value: GhosttyTerminalScrollViewportValue,
    }

    pub type GhosttyKeyEvent = *mut c_void;
    pub type GhosttyKeyEncoder = *mut c_void;
    pub type GhosttyMouseEvent = *mut c_void;
    pub type GhosttyMouseEncoder = *mut c_void;

    /// key/event.h `GhosttyMods` bitmask.
    pub type GhosttyMods = u16;
    pub const GHOSTTY_MODS_SHIFT: GhosttyMods = 1 << 0;
    pub const GHOSTTY_MODS_CTRL: GhosttyMods = 1 << 1;
    pub const GHOSTTY_MODS_ALT: GhosttyMods = 1 << 2;
    pub const GHOSTTY_MODS_SUPER: GhosttyMods = 1 << 3;
    pub const GHOSTTY_MODS_CAPS_LOCK: GhosttyMods = 1 << 4;
    pub const GHOSTTY_MODS_NUM_LOCK: GhosttyMods = 1 << 5;

    pub type GhosttyKeyAction = c_int;
    pub const GHOSTTY_KEY_ACTION_RELEASE: GhosttyKeyAction = 0;
    pub const GHOSTTY_KEY_ACTION_PRESS: GhosttyKeyAction = 1;
    pub const GHOSTTY_KEY_ACTION_REPEAT: GhosttyKeyAction = 2;

    /// key/event.h `GhosttyKey`: W3C physical key codes. Declared as a
    /// repr(C-int) enum so the discriminants track the C enum ORDER exactly
    /// (both sides assign sequentially from 0); do not reorder or skip
    /// entries when syncing with a vendored header bump.
    #[repr(i32)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum GhosttyKey {
        Unidentified = 0,
        // Writing System Keys (W3C § 3.1.1)
        Backquote,
        Backslash,
        BracketLeft,
        BracketRight,
        Comma,
        Digit0,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Equal,
        IntlBackslash,
        IntlRo,
        IntlYen,
        A,
        B,
        C,
        D,
        E,
        F,
        G,
        H,
        I,
        J,
        K,
        L,
        M,
        N,
        O,
        P,
        Q,
        R,
        S,
        T,
        U,
        V,
        W,
        X,
        Y,
        Z,
        Minus,
        Period,
        Quote,
        Semicolon,
        Slash,
        // Functional Keys (W3C § 3.1.2)
        AltLeft,
        AltRight,
        Backspace,
        CapsLock,
        ContextMenu,
        ControlLeft,
        ControlRight,
        Enter,
        MetaLeft,
        MetaRight,
        ShiftLeft,
        ShiftRight,
        Space,
        Tab,
        Convert,
        KanaMode,
        NonConvert,
        // Control Pad Section (W3C § 3.2)
        Delete,
        End,
        Help,
        Home,
        Insert,
        PageDown,
        PageUp,
        // Arrow Pad Section (W3C § 3.3)
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        // Numpad Section (W3C § 3.4)
        NumLock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadBackspace,
        NumpadClear,
        NumpadClearEntry,
        NumpadComma,
        NumpadDecimal,
        NumpadDivide,
        NumpadEnter,
        NumpadEqual,
        NumpadMemoryAdd,
        NumpadMemoryClear,
        NumpadMemoryRecall,
        NumpadMemoryStore,
        NumpadMemorySubtract,
        NumpadMultiply,
        NumpadParenLeft,
        NumpadParenRight,
        NumpadSubtract,
        NumpadSeparator,
        NumpadUp,
        NumpadDown,
        NumpadRight,
        NumpadLeft,
        NumpadBegin,
        NumpadHome,
        NumpadEnd,
        NumpadInsert,
        NumpadDelete,
        NumpadPageUp,
        NumpadPageDown,
        // Function Section (W3C § 3.5)
        Escape,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        F25,
        Fn,
        FnLock,
        PrintScreen,
        ScrollLock,
        Pause,
        // Media Keys (W3C § 3.6)
        BrowserBack,
        BrowserFavorites,
        BrowserForward,
        BrowserHome,
        BrowserRefresh,
        BrowserSearch,
        BrowserStop,
        Eject,
        LaunchApp1,
        LaunchApp2,
        LaunchMail,
        MediaPlayPause,
        MediaSelect,
        MediaStop,
        MediaTrackNext,
        MediaTrackPrevious,
        Power,
        Sleep,
        AudioVolumeDown,
        AudioVolumeMute,
        AudioVolumeUp,
        WakeUp,
        // Legacy, Non-standard, and Special Keys (W3C § 3.7)
        Copy,
        Cut,
        Paste,
    }

    pub type GhosttyOptionAsAlt = c_int;
    pub const GHOSTTY_OPTION_AS_ALT_FALSE: GhosttyOptionAsAlt = 0;
    pub const GHOSTTY_OPTION_AS_ALT_TRUE: GhosttyOptionAsAlt = 1;
    pub const GHOSTTY_OPTION_AS_ALT_LEFT: GhosttyOptionAsAlt = 2;
    pub const GHOSTTY_OPTION_AS_ALT_RIGHT: GhosttyOptionAsAlt = 3;

    pub type GhosttyKeyEncoderOption = c_int;
    pub const GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT: GhosttyKeyEncoderOption = 6;

    pub type GhosttyMouseAction = c_int;
    pub const GHOSTTY_MOUSE_ACTION_PRESS: GhosttyMouseAction = 0;
    pub const GHOSTTY_MOUSE_ACTION_RELEASE: GhosttyMouseAction = 1;
    pub const GHOSTTY_MOUSE_ACTION_MOTION: GhosttyMouseAction = 2;

    pub type GhosttyMouseButton = c_int;
    pub const GHOSTTY_MOUSE_BUTTON_UNKNOWN: GhosttyMouseButton = 0;
    pub const GHOSTTY_MOUSE_BUTTON_LEFT: GhosttyMouseButton = 1;
    pub const GHOSTTY_MOUSE_BUTTON_RIGHT: GhosttyMouseButton = 2;
    pub const GHOSTTY_MOUSE_BUTTON_MIDDLE: GhosttyMouseButton = 3;
    /// Wheel up in xterm-style encodings (button 64).
    pub const GHOSTTY_MOUSE_BUTTON_FOUR: GhosttyMouseButton = 4;
    /// Wheel down in xterm-style encodings (button 65).
    pub const GHOSTTY_MOUSE_BUTTON_FIVE: GhosttyMouseButton = 5;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct GhosttyMousePosition {
        pub x: f32,
        pub y: f32,
    }

    /// Sized struct (mouse/encoder.h). Construct via
    /// [`GhosttyMouseEncoderSize::init_sized`].
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct GhosttyMouseEncoderSize {
        pub size: usize,
        pub screen_width: u32,
        pub screen_height: u32,
        pub cell_width: u32,
        pub cell_height: u32,
        pub padding_top: u32,
        pub padding_bottom: u32,
        pub padding_right: u32,
        pub padding_left: u32,
    }

    impl GhosttyMouseEncoderSize {
        pub fn init_sized() -> Self {
            let mut size: Self = unsafe { std::mem::zeroed() };
            size.size = std::mem::size_of::<Self>();
            size
        }
    }

    pub type GhosttyMouseEncoderOption = c_int;
    pub const GHOSTTY_MOUSE_ENCODER_OPT_SIZE: GhosttyMouseEncoderOption = 2;
    pub const GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED: GhosttyMouseEncoderOption = 3;
    pub const GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL: GhosttyMouseEncoderOption = 4;

    pub type GhosttyFocusEvent = c_int;
    pub const GHOSTTY_FOCUS_GAINED: GhosttyFocusEvent = 0;
    pub const GHOSTTY_FOCUS_LOST: GhosttyFocusEvent = 1;

    /// terminal.h `GhosttyTerminalWritePtyFn`: query auto-replies (DA1, DSR,
    /// DECRQM, ...) that must be written back to the PTY. `data` is only
    /// valid for the duration of the call.
    pub type GhosttyTerminalWritePtyFn = unsafe extern "C" fn(
        terminal: GhosttyTerminal,
        userdata: *mut c_void,
        data: *const u8,
        len: usize,
    );
    /// terminal.h `GhosttyTerminalBellFn`.
    pub type GhosttyTerminalBellFn =
        unsafe extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void);
    /// terminal.h `GhosttyTerminalTitleChangedFn`. The new title is queried
    /// from the terminal after the callback returns.
    pub type GhosttyTerminalTitleChangedFn =
        unsafe extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void);

    /// terminal.h `GhosttyClipboardLocation`: normalized clipboard
    /// destination of a program-initiated clipboard write.
    pub type GhosttyClipboardLocation = c_int;
    pub const GHOSTTY_CLIPBOARD_LOCATION_STANDARD: GhosttyClipboardLocation = 0;

    /// terminal.h `GhosttyClipboardWriteResult`. Protocols without write
    /// acknowledgements (OSC 52, OSC 1337 Copy) ignore the result.
    pub type GhosttyClipboardWriteResult = c_int;
    pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS: GhosttyClipboardWriteResult = 0;
    pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED: GhosttyClipboardWriteResult = 2;

    /// terminal.h `GhosttyClipboardContent`: one MIME representation in a
    /// clipboard write. Both strings are borrowed and only valid for the
    /// duration of the callback; `data` is already protocol-decoded.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyClipboardContent {
        pub mime: GhosttyString,
        pub data: GhosttyString,
    }

    /// Sized struct (terminal.h `GhosttyClipboardWrite`): a semantic, atomic
    /// clipboard write. `contents_len == 0` requests clearing the
    /// destination. Borrowed for the duration of the callback.
    #[repr(C)]
    pub struct GhosttyClipboardWrite {
        pub size: usize,
        pub location: GhosttyClipboardLocation,
        pub contents: *const GhosttyClipboardContent,
        pub contents_len: usize,
    }

    /// terminal.h `GhosttyTerminalClipboardWriteFn`: invoked synchronously
    /// from feed() when the running program performs a clipboard write
    /// (OSC 52, OSC 1337 Copy). Read requests are never forwarded.
    pub type GhosttyTerminalClipboardWriteFn = unsafe extern "C" fn(
        terminal: GhosttyTerminal,
        userdata: *mut c_void,
        write: *const GhosttyClipboardWrite,
    ) -> GhosttyClipboardWriteResult;

    pub type GhosttyStyleColorTag = c_int;
    pub const GHOSTTY_STYLE_COLOR_NONE: GhosttyStyleColorTag = 0;
    pub const GHOSTTY_STYLE_COLOR_PALETTE: GhosttyStyleColorTag = 1;
    pub const GHOSTTY_STYLE_COLOR_RGB: GhosttyStyleColorTag = 2;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union GhosttyStyleColorValue {
        pub palette: u8,
        pub rgb: GhosttyColorRgb,
        pub _padding: u64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyStyleColor {
        pub tag: GhosttyStyleColorTag,
        pub value: GhosttyStyleColorValue,
    }

    /// Sized struct (style.h). Construct via [`GhosttyStyle::init_sized`] so
    /// the library can detect which struct version the caller compiled with.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyStyle {
        pub size: usize,
        pub fg_color: GhosttyStyleColor,
        pub bg_color: GhosttyStyleColor,
        pub underline_color: GhosttyStyleColor,
        pub bold: bool,
        pub italic: bool,
        pub faint: bool,
        pub blink: bool,
        pub inverse: bool,
        pub invisible: bool,
        pub strikethrough: bool,
        pub overline: bool,
        pub underline: c_int,
    }

    impl GhosttyStyle {
        pub fn init_sized() -> Self {
            // GHOSTTY_INIT_SIZED equivalent: zeroed with the size field set.
            let mut style: Self = unsafe { std::mem::zeroed() };
            style.size = std::mem::size_of::<Self>();
            style
        }
    }

    /// Sized struct (render.h). Construct via
    /// [`GhosttyRenderStateColors::init_sized`].
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyRenderStateColors {
        pub size: usize,
        pub background: GhosttyColorRgb,
        pub foreground: GhosttyColorRgb,
        pub cursor: GhosttyColorRgb,
        pub cursor_has_value: bool,
        pub palette: [GhosttyColorRgb; 256],
    }

    impl GhosttyRenderStateColors {
        pub fn init_sized() -> Self {
            let mut colors: Self = unsafe { std::mem::zeroed() };
            colors.size = std::mem::size_of::<Self>();
            colors
        }
    }

    unsafe extern "C" {
        pub fn ghostty_color_palette_default(out: *mut GhosttyColorRgb);
        pub fn ghostty_terminal_new(
            allocator: *const c_void,
            terminal: *mut GhosttyTerminal,
            cols: u16,
            rows: u16,
        ) -> GhosttyResult;
        pub fn ghostty_terminal_free(terminal: GhosttyTerminal);
        pub fn ghostty_terminal_reset(terminal: GhosttyTerminal);
        pub fn ghostty_terminal_resize(
            terminal: GhosttyTerminal,
            cols: u16,
            rows: u16,
            cell_width_px: u32,
            cell_height_px: u32,
        ) -> GhosttyResult;
        pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);
        /// For pointer-typed options (userdata, callbacks) `value` IS the
        /// pointer/function pointer itself, not a pointer to it. NULL clears.
        pub fn ghostty_terminal_set(
            terminal: GhosttyTerminal,
            option: GhosttyTerminalOption,
            value: *const c_void,
        ) -> GhosttyResult;

        pub fn ghostty_render_state_new(
            allocator: *const c_void,
            state: *mut GhosttyRenderState,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_free(state: GhosttyRenderState);
        pub fn ghostty_render_state_update(
            state: GhosttyRenderState,
            terminal: GhosttyTerminal,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_get(
            state: GhosttyRenderState,
            data: GhosttyRenderStateData,
            out: *mut c_void,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_set(
            state: GhosttyRenderState,
            option: GhosttyRenderStateOption,
            value: *const c_void,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_colors_get(
            state: GhosttyRenderState,
            out_colors: *mut GhosttyRenderStateColors,
        ) -> GhosttyResult;

        pub fn ghostty_render_state_row_iterator_new(
            allocator: *const c_void,
            out_iterator: *mut GhosttyRenderStateRowIterator,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_row_iterator_free(iterator: GhosttyRenderStateRowIterator);
        pub fn ghostty_render_state_row_iterator_next(
            iterator: GhosttyRenderStateRowIterator,
        ) -> bool;
        pub fn ghostty_render_state_row_get(
            iterator: GhosttyRenderStateRowIterator,
            data: GhosttyRenderStateRowData,
            out: *mut c_void,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_row_set(
            iterator: GhosttyRenderStateRowIterator,
            option: GhosttyRenderStateRowOption,
            value: *const c_void,
        ) -> GhosttyResult;

        pub fn ghostty_render_state_row_cells_new(
            allocator: *const c_void,
            out_cells: *mut GhosttyRenderStateRowCells,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_row_cells_free(cells: GhosttyRenderStateRowCells);
        pub fn ghostty_render_state_row_cells_next(cells: GhosttyRenderStateRowCells) -> bool;
        pub fn ghostty_render_state_row_cells_select(
            cells: GhosttyRenderStateRowCells,
            x: u16,
        ) -> GhosttyResult;
        pub fn ghostty_render_state_row_cells_get(
            cells: GhosttyRenderStateRowCells,
            data: GhosttyRenderStateRowCellsData,
            out: *mut c_void,
        ) -> GhosttyResult;

        pub fn ghostty_cell_get(
            cell: GhosttyCell,
            data: GhosttyCellData,
            out: *mut c_void,
        ) -> GhosttyResult;

        pub fn ghostty_terminal_get(
            terminal: GhosttyTerminal,
            data: GhosttyTerminalData,
            out: *mut c_void,
        ) -> GhosttyResult;
        pub fn ghostty_terminal_scroll_viewport(
            terminal: GhosttyTerminal,
            behavior: GhosttyTerminalScrollViewport,
        );
        pub fn ghostty_terminal_grid_ref(
            terminal: GhosttyTerminal,
            point: GhosttyPoint,
            out_ref: *mut GhosttyGridRef,
        ) -> GhosttyResult;

        pub fn ghostty_grid_ref_cell(
            grid_ref: *const GhosttyGridRef,
            out_cell: *mut GhosttyCell,
        ) -> GhosttyResult;
        pub fn ghostty_grid_ref_row(
            grid_ref: *const GhosttyGridRef,
            out_row: *mut GhosttyRow,
        ) -> GhosttyResult;
        pub fn ghostty_grid_ref_hyperlink_uri(
            grid_ref: *const GhosttyGridRef,
            buf: *mut u8,
            buf_len: usize,
            out_len: *mut usize,
        ) -> GhosttyResult;

        pub fn ghostty_row_get(
            row: GhosttyRow,
            data: GhosttyRowData,
            out: *mut c_void,
        ) -> GhosttyResult;

        pub fn ghostty_key_event_new(
            allocator: *const c_void,
            event: *mut GhosttyKeyEvent,
        ) -> GhosttyResult;
        pub fn ghostty_key_event_free(event: GhosttyKeyEvent);
        pub fn ghostty_key_event_set_action(event: GhosttyKeyEvent, action: GhosttyKeyAction);
        pub fn ghostty_key_event_set_key(event: GhosttyKeyEvent, key: GhosttyKey);
        pub fn ghostty_key_event_set_mods(event: GhosttyKeyEvent, mods: GhosttyMods);
        pub fn ghostty_key_event_set_consumed_mods(event: GhosttyKeyEvent, mods: GhosttyMods);
        pub fn ghostty_key_event_set_composing(event: GhosttyKeyEvent, composing: bool);
        /// The event does NOT take ownership of `utf8`; the pointer must stay
        /// valid until the event is encoded or the utf8 is replaced.
        pub fn ghostty_key_event_set_utf8(event: GhosttyKeyEvent, utf8: *const u8, len: usize);
        pub fn ghostty_key_event_set_unshifted_codepoint(event: GhosttyKeyEvent, codepoint: u32);

        pub fn ghostty_key_encoder_new(
            allocator: *const c_void,
            encoder: *mut GhosttyKeyEncoder,
        ) -> GhosttyResult;
        pub fn ghostty_key_encoder_free(encoder: GhosttyKeyEncoder);
        pub fn ghostty_key_encoder_setopt(
            encoder: GhosttyKeyEncoder,
            option: GhosttyKeyEncoderOption,
            value: *const c_void,
        );
        pub fn ghostty_key_encoder_setopt_from_terminal(
            encoder: GhosttyKeyEncoder,
            terminal: GhosttyTerminal,
        );
        pub fn ghostty_key_encoder_encode(
            encoder: GhosttyKeyEncoder,
            event: GhosttyKeyEvent,
            out_buf: *mut u8,
            out_buf_size: usize,
            out_len: *mut usize,
        ) -> GhosttyResult;

        pub fn ghostty_mouse_event_new(
            allocator: *const c_void,
            event: *mut GhosttyMouseEvent,
        ) -> GhosttyResult;
        pub fn ghostty_mouse_event_free(event: GhosttyMouseEvent);
        pub fn ghostty_mouse_event_set_action(event: GhosttyMouseEvent, action: GhosttyMouseAction);
        pub fn ghostty_mouse_event_set_button(event: GhosttyMouseEvent, button: GhosttyMouseButton);
        pub fn ghostty_mouse_event_clear_button(event: GhosttyMouseEvent);
        pub fn ghostty_mouse_event_set_mods(event: GhosttyMouseEvent, mods: GhosttyMods);
        pub fn ghostty_mouse_event_set_position(
            event: GhosttyMouseEvent,
            position: GhosttyMousePosition,
        );

        pub fn ghostty_mouse_encoder_new(
            allocator: *const c_void,
            encoder: *mut GhosttyMouseEncoder,
        ) -> GhosttyResult;
        pub fn ghostty_mouse_encoder_free(encoder: GhosttyMouseEncoder);
        pub fn ghostty_mouse_encoder_setopt(
            encoder: GhosttyMouseEncoder,
            option: GhosttyMouseEncoderOption,
            value: *const c_void,
        );
        pub fn ghostty_mouse_encoder_setopt_from_terminal(
            encoder: GhosttyMouseEncoder,
            terminal: GhosttyTerminal,
        );
        pub fn ghostty_mouse_encoder_reset(encoder: GhosttyMouseEncoder);
        pub fn ghostty_mouse_encoder_encode(
            encoder: GhosttyMouseEncoder,
            event: GhosttyMouseEvent,
            out_buf: *mut u8,
            out_buf_size: usize,
            out_len: *mut usize,
        ) -> GhosttyResult;

        /// `data` is modified in place (unsafe byte stripping) during
        /// encoding; both calls of the query-then-encode pattern are
        /// idempotent over the same buffer.
        pub fn ghostty_paste_encode(
            data: *mut u8,
            data_len: usize,
            bracketed: bool,
            buf: *mut u8,
            buf_len: usize,
            out_written: *mut usize,
        ) -> GhosttyResult;

        pub fn ghostty_focus_encode(
            event: GhosttyFocusEvent,
            buf: *mut u8,
            buf_len: usize,
            out_written: *mut usize,
        ) -> GhosttyResult;
    }
}

/// Error from a libghostty-vt call, carrying the raw `GhosttyResult` code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VtError {
    pub code: i32,
}

impl fmt::Display for VtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            ffi::GHOSTTY_OUT_OF_MEMORY => "out of memory",
            ffi::GHOSTTY_INVALID_VALUE => "invalid value",
            ffi::GHOSTTY_OUT_OF_SPACE => "out of space",
            ffi::GHOSTTY_NO_VALUE => "no value",
            _ => "unknown libghostty-vt error",
        };
        write!(f, "libghostty-vt: {name} (code {})", self.code)
    }
}

impl std::error::Error for VtError {}

fn check(result: ffi::GhosttyResult) -> Result<(), VtError> {
    if result == ffi::GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(VtError { code: result })
    }
}

/// Ghostty's canonical base16 + xterm extended 256-color palette. Theme
/// loaders replace the entries they explicitly define while preserving the
/// same extended colors Ghostty uses when `palette-generate` is disabled.
pub fn default_color_palette() -> [ffi::GhosttyColorRgb; 256] {
    let mut palette = [ffi::GhosttyColorRgb::default(); 256];
    unsafe { ffi::ghostty_color_palette_default(palette.as_mut_ptr()) };
    palette
}

/// Global render-state dirtiness after an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtDirty {
    /// Nothing changed; rendering can be skipped entirely.
    Clean,
    /// Some rows changed; consult per-row dirty flags.
    Partial,
    /// Global state changed; redraw everything.
    Full,
}

/// Width behavior of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtCellWide {
    Narrow,
    Wide,
    /// Spacer after a wide character. Do not render.
    SpacerTail,
    /// Spacer at the end of a soft-wrapped line before a wide character.
    SpacerHead,
}

/// A libghostty-vt terminal instance: VT parser plus full terminal state
/// (screen, scrollback, alt screen, modes, styles).
///
/// Not `Sync`: exclusive access is required for every call, expressed as
/// `&mut self`. Cross-thread sharing (PTY reader thread vs. render path)
/// must go through a lock owned by the caller.
pub struct VtTerminal {
    raw: ffi::GhosttyTerminal,
    /// Heap cell registered as the terminal's userdata; trampolines below
    /// dispatch through it. Null until [`set_host_callbacks`] installs hooks.
    ///
    /// [`set_host_callbacks`]: Self::set_host_callbacks
    host_callbacks: *mut VtHostCallbacks,
}

// SAFETY: libghostty-vt terminal state has no thread affinity (no TLS, no
// run-loop coupling); it only requires exclusive access, which &mut methods
// and the !Sync auto impl (raw pointer field) already enforce.
unsafe impl Send for VtTerminal {}

impl VtTerminal {
    pub fn new(cols: u16, rows: u16, max_scrollback: usize) -> Result<Self, VtError> {
        let mut raw: ffi::GhosttyTerminal = std::ptr::null_mut();
        check(unsafe { ffi::ghostty_terminal_new(std::ptr::null(), &mut raw, cols, rows) })?;
        // Upstream replaced the creation-time max_scrollback option with a
        // runtime setter. Semantics are unchanged: the limit is in bytes and
        // zero disables scrollback entirely (matches the old
        // `no_scrollback = max_scrollback == 0` core behavior).
        let scrollback_result = check(unsafe {
            ffi::ghostty_terminal_set(
                raw,
                ffi::GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,
                std::ptr::from_ref(&max_scrollback).cast(),
            )
        });
        if let Err(err) = scrollback_result {
            unsafe { ffi::ghostty_terminal_free(raw) };
            return Err(err);
        }
        Ok(Self {
            raw,
            host_callbacks: std::ptr::null_mut(),
        })
    }

    /// Install terminal → host hooks. Hooks fire synchronously inside
    /// [`feed`](Self::feed) on whichever thread is feeding, so they must be
    /// `Send`, must never call back into this terminal (no reentrancy per
    /// terminal.h), and must not block. `None` hooks are cleared in the
    /// library so the corresponding sequences are ignored. Replaces any
    /// previously installed set.
    pub fn set_host_callbacks(&mut self, callbacks: VtHostCallbacks) -> Result<(), VtError> {
        let write_pty_fn: *const c_void = if callbacks.write_pty.is_some() {
            let f: ffi::GhosttyTerminalWritePtyFn = write_pty_trampoline;
            f as *const c_void
        } else {
            std::ptr::null()
        };
        let bell_fn: *const c_void = if callbacks.bell.is_some() {
            let f: ffi::GhosttyTerminalBellFn = bell_trampoline;
            f as *const c_void
        } else {
            std::ptr::null()
        };
        let title_fn: *const c_void = if callbacks.title_changed.is_some() {
            let f: ffi::GhosttyTerminalTitleChangedFn = title_changed_trampoline;
            f as *const c_void
        } else {
            std::ptr::null()
        };
        let clipboard_write_fn: *const c_void = if callbacks.clipboard_write.is_some() {
            let f: ffi::GhosttyTerminalClipboardWriteFn = clipboard_write_trampoline;
            f as *const c_void
        } else {
            std::ptr::null()
        };

        let boxed = Box::into_raw(Box::new(callbacks));
        let result = unsafe {
            check(ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_USERDATA,
                boxed.cast::<c_void>(),
            ))
            .and_then(|()| {
                check(ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_WRITE_PTY,
                    write_pty_fn,
                ))
            })
            .and_then(|()| {
                check(ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_BELL,
                    bell_fn,
                ))
            })
            .and_then(|()| {
                check(ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
                    title_fn,
                ))
            })
            .and_then(|()| {
                check(ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE,
                    clipboard_write_fn,
                ))
            })
        };
        if let Err(error) = result {
            // Leave the terminal with no hooks rather than half a set wired
            // to a userdata pointer we are about to free.
            unsafe {
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_WRITE_PTY,
                    std::ptr::null(),
                );
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_BELL,
                    std::ptr::null(),
                );
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
                    std::ptr::null(),
                );
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE,
                    std::ptr::null(),
                );
                ffi::ghostty_terminal_set(
                    self.raw,
                    ffi::GHOSTTY_TERMINAL_OPT_USERDATA,
                    std::ptr::null(),
                );
                drop(Box::from_raw(boxed));
            }
            return Err(error);
        }
        let previous = std::mem::replace(&mut self.host_callbacks, boxed);
        if !previous.is_null() {
            // Safe to free only now: the library already points at `boxed`.
            drop(unsafe { Box::from_raw(previous) });
        }
        Ok(())
    }

    pub fn set_default_colors(
        &mut self,
        foreground: ffi::GhosttyColorRgb,
        background: ffi::GhosttyColorRgb,
        cursor: Option<ffi::GhosttyColorRgb>,
        palette: &[ffi::GhosttyColorRgb; 256],
    ) -> Result<(), VtError> {
        unsafe {
            check(ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND,
                (&raw const foreground).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND,
                (&raw const background).cast::<c_void>(),
            ))?;
            check(ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_COLOR_CURSOR,
                cursor.as_ref().map_or(std::ptr::null(), |value| {
                    (value as *const ffi::GhosttyColorRgb).cast::<c_void>()
                }),
            ))?;
            check(ffi::ghostty_terminal_set(
                self.raw,
                ffi::GHOSTTY_TERMINAL_OPT_COLOR_PALETTE,
                palette.as_ptr().cast::<c_void>(),
            ))
        }
    }

    /// Feed raw VT-encoded bytes (typically PTY output) through the parser.
    /// Never fails; malformed input is absorbed by the library.
    pub fn feed(&mut self, bytes: &[u8]) {
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, bytes.as_ptr(), bytes.len()) }
    }

    /// Resize the grid. The primary screen reflows; the alternate screen does
    /// not. Cell pixel sizes feed image protocols and size reports.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), VtError> {
        check(unsafe {
            ffi::ghostty_terminal_resize(self.raw, cols, rows, cell_width_px, cell_height_px)
        })
    }

    /// Full terminal reset (RIS). Dimensions are preserved.
    pub fn reset(&mut self) {
        unsafe { ffi::ghostty_terminal_reset(self.raw) }
    }

    /// Current value of a terminal mode (packed per modes.h; the exported
    /// `GHOSTTY_MODE_*` constants are DEC private modes and already packed).
    pub fn mode(&mut self, mode: ffi::GhosttyMode) -> Result<bool, VtError> {
        let mut config = ffi::GhosttyTerminalModeConfig { mode, value: false };
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_MODE,
                std::ptr::from_mut(&mut config).cast(),
            )
        })?;
        Ok(config.value)
    }

    /// Whether any mouse tracking mode (X10/normal/button/any-event) is on.
    pub fn mouse_tracking(&mut self) -> Result<bool, VtError> {
        let mut tracking = false;
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING,
                (&raw mut tracking).cast::<c_void>(),
            )
        })?;
        Ok(tracking)
    }

    /// Active Kitty keyboard-protocol flags. Zero means legacy encoding.
    pub fn kitty_keyboard_flags(&mut self) -> Result<u8, VtError> {
        let mut flags = 0_u8;
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
                (&raw mut flags).cast::<c_void>(),
            )
        })?;
        Ok(flags)
    }

    /// Whether the alternate screen is the active screen.
    pub fn alternate_screen_active(&mut self) -> Result<bool, VtError> {
        let mut screen: ffi::GhosttyTerminalScreen = ffi::GHOSTTY_TERMINAL_SCREEN_PRIMARY;
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                (&raw mut screen).cast::<c_void>(),
            )
        })?;
        Ok(screen == ffi::GHOSTTY_TERMINAL_SCREEN_ALTERNATE)
    }

    /// Scroll the viewport within the scrollback. `Delta` rows are negative
    /// for up (toward history). No-op on screens without scrollback.
    pub fn scroll_viewport(&mut self, behavior: VtScrollViewport) {
        let behavior = match behavior {
            VtScrollViewport::Top => ffi::GhosttyTerminalScrollViewport {
                tag: ffi::GHOSTTY_SCROLL_VIEWPORT_TOP,
                value: ffi::GhosttyTerminalScrollViewportValue { _padding: [0; 2] },
            },
            VtScrollViewport::Bottom => ffi::GhosttyTerminalScrollViewport {
                tag: ffi::GHOSTTY_SCROLL_VIEWPORT_BOTTOM,
                value: ffi::GhosttyTerminalScrollViewportValue { _padding: [0; 2] },
            },
            VtScrollViewport::Delta(delta) => ffi::GhosttyTerminalScrollViewport {
                tag: ffi::GHOSTTY_SCROLL_VIEWPORT_DELTA,
                value: ffi::GhosttyTerminalScrollViewportValue { delta },
            },
        };
        unsafe { ffi::ghostty_terminal_scroll_viewport(self.raw, behavior) }
    }

    /// Copy out a borrowed terminal string datum (title/pwd). The borrowed
    /// pointer is only valid until the next feed/reset, so the copy happens
    /// here under the exclusive borrow. Empty means "not set" per terminal.h.
    fn owned_string(&mut self, data: ffi::GhosttyTerminalData) -> Result<Option<String>, VtError> {
        let mut string = ffi::GhosttyString {
            ptr: std::ptr::null(),
            len: 0,
        };
        check(unsafe {
            ffi::ghostty_terminal_get(self.raw, data, (&raw mut string).cast::<c_void>())
        })?;
        if string.ptr.is_null() || string.len == 0 {
            return Ok(None);
        }
        let bytes = unsafe { std::slice::from_raw_parts(string.ptr, string.len) };
        Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
    }

    /// The terminal title as set by OSC 0/2, if any.
    pub fn title(&mut self) -> Result<Option<String>, VtError> {
        self.owned_string(ffi::GHOSTTY_TERMINAL_DATA_TITLE)
    }

    /// The terminal working directory as reported by OSC 7, if any.
    pub fn pwd(&mut self) -> Result<Option<String>, VtError> {
        self.owned_string(ffi::GHOSTTY_TERMINAL_DATA_PWD)
    }

    /// Scrollbar state (total/offset/len in rows) for the active viewport.
    /// terminal.h warns this can be expensive for arbitrary viewport pins;
    /// call on demand, not per frame.
    pub fn scrollbar(&mut self) -> Result<VtScrollbar, VtError> {
        let mut scrollbar = ffi::GhosttyTerminalScrollbar::default();
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_SCROLLBAR,
                (&raw mut scrollbar).cast::<c_void>(),
            )
        })?;
        Ok(VtScrollbar {
            total: scrollbar.total,
            offset: scrollbar.offset,
            len: scrollbar.len,
        })
    }

    /// OSC 8 hyperlink URI of the cell at viewport coordinates, if any.
    /// Grid refs invalidate on any terminal mutation, so resolve and copy
    /// within this exclusive borrow.
    pub fn hyperlink_uri_at_viewport(&mut self, x: u16, y: u16) -> Result<Option<String>, VtError> {
        let mut grid_ref = ffi::GhosttyGridRef::init_sized();
        let point = ffi::GhosttyPoint {
            tag: ffi::GHOSTTY_POINT_TAG_VIEWPORT,
            value: ffi::GhosttyPointValue {
                coordinate: ffi::GhosttyPointCoordinate { x, y: u32::from(y) },
            },
        };
        if unsafe { ffi::ghostty_terminal_grid_ref(self.raw, point, &mut grid_ref) }
            != ffi::GHOSTTY_SUCCESS
        {
            // Out-of-bounds points have no link rather than being an error.
            return Ok(None);
        }
        let mut out: Vec<u8> = Vec::new();
        encode_with_retry(&mut out, |buf, len, written| unsafe {
            ffi::ghostty_grid_ref_hyperlink_uri(&grid_ref, buf, len, written)
        })?;
        if out.is_empty() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&out).into_owned()))
    }

    /// Whether the cursor sits at a shell-integration (OSC 133) prompt.
    /// Mirrors ghostty `Terminal.cursorIsAtPrompt`: alternate screen is
    /// never a prompt; otherwise the cursor row's semantic prompt state or
    /// the cursor cell's semantic content decides. Without shell
    /// integration this stays false (everything reads as output).
    pub fn cursor_at_prompt(&mut self) -> Result<bool, VtError> {
        if self.alternate_screen_active()? {
            return Ok(false);
        }
        let (cursor_x, cursor_y) = self.cursor_position()?;
        let mut grid_ref = ffi::GhosttyGridRef::init_sized();
        let point = ffi::GhosttyPoint {
            tag: ffi::GHOSTTY_POINT_TAG_ACTIVE,
            value: ffi::GhosttyPointValue {
                coordinate: ffi::GhosttyPointCoordinate {
                    x: cursor_x,
                    y: u32::from(cursor_y),
                },
            },
        };
        check(unsafe { ffi::ghostty_terminal_grid_ref(self.raw, point, &mut grid_ref) })?;

        let mut row: ffi::GhosttyRow = 0;
        check(unsafe { ffi::ghostty_grid_ref_row(&grid_ref, &mut row) })?;
        let mut row_prompt: ffi::GhosttyRowSemanticPrompt = ffi::GHOSTTY_ROW_SEMANTIC_NONE;
        check(unsafe {
            ffi::ghostty_row_get(
                row,
                ffi::GHOSTTY_ROW_DATA_SEMANTIC_PROMPT,
                (&raw mut row_prompt).cast::<c_void>(),
            )
        })?;
        if row_prompt != ffi::GHOSTTY_ROW_SEMANTIC_NONE {
            return Ok(true);
        }

        let mut cell: ffi::GhosttyCell = 0;
        check(unsafe { ffi::ghostty_grid_ref_cell(&grid_ref, &mut cell) })?;
        let mut semantic: ffi::GhosttyCellSemanticContent = ffi::GHOSTTY_CELL_SEMANTIC_OUTPUT;
        check(unsafe {
            ffi::ghostty_cell_get(
                cell,
                ffi::GHOSTTY_CELL_DATA_SEMANTIC_CONTENT,
                (&raw mut semantic).cast::<c_void>(),
            )
        })?;
        Ok(matches!(
            semantic,
            ffi::GHOSTTY_CELL_SEMANTIC_INPUT | ffi::GHOSTTY_CELL_SEMANTIC_PROMPT
        ))
    }

    /// Cursor position on the active screen, in zero-based cells.
    fn cursor_position(&mut self) -> Result<(u16, u16), VtError> {
        let mut cursor_x: u16 = 0;
        let mut cursor_y: u16 = 0;
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_CURSOR_X,
                (&raw mut cursor_x).cast::<c_void>(),
            )
        })?;
        check(unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::GHOSTTY_TERMINAL_DATA_CURSOR_Y,
                (&raw mut cursor_y).cast::<c_void>(),
            )
        })?;
        Ok((cursor_x, cursor_y))
    }

    /// Ghostty's `clear_screen` binding action (Cmd-K on macOS), mirroring
    /// `termio.clearScreen` with history:
    ///
    /// - The alternate screen is never cleared. An emulator-level clear
    ///   desynchronizes the running program's idea of where the cursor is,
    ///   so ghostty leaves that binding unconsumed and so do we.
    /// - The scrollback goes first.
    /// - Away from a prompt the rows above the cursor are dropped, lifting
    ///   the cursor row to the top with its content (the half-typed command
    ///   line) intact.
    /// - At an OSC 133 prompt the whole screen is erased and the shell owes
    ///   a repaint, which the caller triggers with a form feed.
    ///
    /// libghostty-vt exposes no erase entry point, so these go through the
    /// parser the same way the PTY's own bytes do.
    pub fn clear_screen(&mut self, history: bool) -> Result<VtClearScreen, VtError> {
        if self.alternate_screen_active()? {
            return Ok(VtClearScreen::NotCleared);
        }
        if history {
            self.feed(b"\x1b[3J");
        }
        if self.cursor_at_prompt()? {
            self.feed(b"\x1b[2J");
            return Ok(VtClearScreen::ClearedAtPrompt);
        }
        let (cursor_x, cursor_y) = self.cursor_position()?;
        if cursor_y > 0 {
            // DL is the only way to drop rows off the top of the active
            // area from out here. DECSC/DECRC carry the pen (SGR, charset,
            // origin mode) across it, and the pen is reset in between so the
            // rows DL opens at the bottom are blank in the default
            // background instead of whatever the program was painting with.
            // DL parks the cursor at the start of the row it began on, which
            // is now row 0, so the trailing CUP only restores the column.
            // A program that set a scrolling region while on the primary
            // screen keeps its rows: DL declines outside the region.
            let mut bytes = Vec::from(b"\x1b7\x1b[m\x1b[H".as_slice());
            bytes.extend_from_slice(format!("\x1b[{cursor_y}M").as_bytes());
            bytes.extend_from_slice(b"\x1b8");
            bytes.extend_from_slice(format!("\x1b[1;{}H", u32::from(cursor_x) + 1).as_bytes());
            self.feed(&bytes);
        }
        Ok(VtClearScreen::Cleared)
    }
}

/// Outcome of [`VtTerminal::clear_screen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtClearScreen {
    /// Nothing was cleared, so the key belongs to the running program.
    NotCleared,
    /// The screen was cleared.
    Cleared,
    /// The screen was cleared at a shell prompt: the shell has to repaint
    /// it, so the caller writes a form feed (0x0C) to the PTY.
    ClearedAtPrompt,
}

/// Scrollbar state for the terminal viewport, in rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VtScrollbar {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

/// Viewport scroll behavior for [`VtTerminal::scroll_viewport`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtScrollViewport {
    Top,
    Bottom,
    /// Scroll by rows; up (toward history) is negative.
    Delta(isize),
}

impl Drop for VtTerminal {
    fn drop(&mut self) {
        // Free the terminal before the callback cell: callbacks only fire
        // from feed(), but this order keeps the userdata pointer valid for
        // the terminal's entire lifetime.
        unsafe { ffi::ghostty_terminal_free(self.raw) }
        if !self.host_callbacks.is_null() {
            drop(unsafe { Box::from_raw(self.host_callbacks) });
        }
    }
}

/// Terminal → host hooks dispatched from [`VtTerminal::feed`]. `write_pty`
/// receives query auto-replies (DA1, DSR, DECRQM, ...) that must reach the
/// PTY for applications to keep working; `bell` and `title_changed` are
/// notification hooks (the new title is queried from the terminal later).
/// `clipboard_write` receives program-initiated standard-clipboard text
/// (OSC 52 / OSC 1337 Copy), already decoded; selection/primary
/// destinations, clears, and non-`text/plain` representations are reported
/// unsupported, matching the embedded-Ghostty surface path.
#[derive(Default)]
pub struct VtHostCallbacks {
    pub write_pty: Option<Box<dyn FnMut(&[u8]) + Send>>,
    pub bell: Option<Box<dyn FnMut() + Send>>,
    pub title_changed: Option<Box<dyn FnMut() + Send>>,
    pub clipboard_write: Option<Box<dyn FnMut(String) + Send>>,
}

unsafe extern "C" fn write_pty_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(write_pty) = callbacks.write_pty.as_mut() {
        let bytes: &[u8] = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, len) }
        };
        write_pty(bytes);
    }
}

unsafe extern "C" fn bell_trampoline(_terminal: ffi::GhosttyTerminal, userdata: *mut c_void) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(bell) = callbacks.bell.as_mut() {
        bell();
    }
}

unsafe extern "C" fn title_changed_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
) {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    if let Some(title_changed) = callbacks.title_changed.as_mut() {
        title_changed();
    }
}

unsafe extern "C" fn clipboard_write_trampoline(
    _terminal: ffi::GhosttyTerminal,
    userdata: *mut c_void,
    write: *const ffi::GhosttyClipboardWrite,
) -> ffi::GhosttyClipboardWriteResult {
    let callbacks = unsafe { &mut *userdata.cast::<VtHostCallbacks>() };
    let Some(clipboard_write) = callbacks.clipboard_write.as_mut() else {
        return ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED;
    };
    let Some(text) = (unsafe { clipboard_write_standard_text_plain(write) }) else {
        return ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED;
    };
    clipboard_write(text);
    ffi::GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS
}

/// Copy the non-empty `text/plain` representation out of a standard-clipboard
/// write. Selection/primary destinations, clears, and non-text
/// representations yield `None` (reported unsupported to the library).
unsafe fn clipboard_write_standard_text_plain(
    write: *const ffi::GhosttyClipboardWrite,
) -> Option<String> {
    let write = unsafe { write.as_ref() }?;
    // Sized struct: only trust fields the producing library actually filled.
    if write.size < std::mem::size_of::<ffi::GhosttyClipboardWrite>()
        || write.location != ffi::GHOSTTY_CLIPBOARD_LOCATION_STANDARD
        || write.contents.is_null()
        || write.contents_len == 0
    {
        return None;
    }
    let contents = unsafe { std::slice::from_raw_parts(write.contents, write.contents_len) };
    for content in contents {
        if content.mime.ptr.is_null() || content.data.ptr.is_null() || content.data.len == 0 {
            continue;
        }
        let mime = unsafe { std::slice::from_raw_parts(content.mime.ptr, content.mime.len) };
        if mime != b"text/plain" {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(content.data.ptr, content.data.len) };
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        return Some(text.to_string());
    }
    None
}

/// Resolve an SGR style color (e.g. `GhosttyStyle::underline_color`) against
/// the active palette. `None` means the style has no explicit color; use the
/// relevant default. Lives here so the union field reads stay in the FFI
/// choke point.
pub fn style_color_rgb(
    color: &ffi::GhosttyStyleColor,
    palette: &[ffi::GhosttyColorRgb; 256],
) -> Option<ffi::GhosttyColorRgb> {
    match color.tag {
        ffi::GHOSTTY_STYLE_COLOR_PALETTE => Some(palette[unsafe { color.value.palette } as usize]),
        ffi::GHOSTTY_STYLE_COLOR_RGB => Some(unsafe { color.value.rgb }),
        _ => None,
    }
}

/// Snapshot of a terminal viewport for rendering, with two-level dirty
/// tracking (global + per-row).
///
/// Usage per frame: [`update`](Self::update) under exclusive terminal access,
/// read rows/cells via [`rows`](Self::rows), then clear BOTH dirty layers —
/// per-row with [`VtRow::clear_dirty`], global with
/// [`clear_dirty`](Self::clear_dirty). Updates never clear dirty state, and
/// clearing one layer never clears the other (see module CDXC).
pub struct VtRenderState {
    raw: ffi::GhosttyRenderState,
    row_iter: ffi::GhosttyRenderStateRowIterator,
    cells: ffi::GhosttyRenderStateRowCells,
}

// SAFETY: the render state is a self-contained snapshot after update; like
// VtTerminal it has no thread affinity and &mut methods enforce exclusivity.
unsafe impl Send for VtRenderState {}

impl VtRenderState {
    pub fn new() -> Result<Self, VtError> {
        let mut raw: ffi::GhosttyRenderState = std::ptr::null_mut();
        check(unsafe { ffi::ghostty_render_state_new(std::ptr::null(), &mut raw) })?;

        let mut row_iter: ffi::GhosttyRenderStateRowIterator = std::ptr::null_mut();
        if let Err(error) = check(unsafe {
            ffi::ghostty_render_state_row_iterator_new(std::ptr::null(), &mut row_iter)
        }) {
            unsafe { ffi::ghostty_render_state_free(raw) };
            return Err(error);
        }

        let mut cells: ffi::GhosttyRenderStateRowCells = std::ptr::null_mut();
        if let Err(error) =
            check(unsafe { ffi::ghostty_render_state_row_cells_new(std::ptr::null(), &mut cells) })
        {
            unsafe {
                ffi::ghostty_render_state_row_iterator_free(row_iter);
                ffi::ghostty_render_state_free(raw);
            }
            return Err(error);
        }

        Ok(Self {
            raw,
            row_iter,
            cells,
        })
    }

    /// Sync this snapshot from the terminal. Requires exclusive terminal
    /// access only for the duration of this call (the "short lock").
    /// Invalidates all row/cell data read from previous updates, which the
    /// `&mut self` borrow enforces against the borrowing readers below.
    pub fn update(&mut self, terminal: &mut VtTerminal) -> Result<(), VtError> {
        check(unsafe { ffi::ghostty_render_state_update(self.raw, terminal.raw) })
    }

    fn get(&self, data: ffi::GhosttyRenderStateData, out: *mut c_void) -> Result<(), VtError> {
        check(unsafe { ffi::ghostty_render_state_get(self.raw, data, out) })
    }

    /// Viewport size in cells as `(cols, rows)`.
    pub fn size(&self) -> Result<(u16, u16), VtError> {
        let mut cols: u16 = 0;
        let mut rows: u16 = 0;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_COLS,
            (&raw mut cols).cast::<c_void>(),
        )?;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_ROWS,
            (&raw mut rows).cast::<c_void>(),
        )?;
        Ok((cols, rows))
    }

    /// Global dirty state. Raised by [`update`](Self::update); only ever
    /// cleared by the caller via [`clear_dirty`](Self::clear_dirty).
    pub fn dirty(&self) -> Result<VtDirty, VtError> {
        let mut dirty: ffi::GhosttyRenderStateDirty = 0;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_DIRTY,
            (&raw mut dirty).cast::<c_void>(),
        )?;
        Ok(match dirty {
            ffi::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL => VtDirty::Partial,
            ffi::GHOSTTY_RENDER_STATE_DIRTY_FULL => VtDirty::Full,
            _ => VtDirty::Clean,
        })
    }

    /// Clear the GLOBAL dirty layer after consuming a frame. Per-row dirty
    /// flags are independent and must be cleared per row while iterating
    /// ([`VtRow::clear_dirty`]).
    pub fn clear_dirty(&mut self) -> Result<(), VtError> {
        let clean = ffi::GHOSTTY_RENDER_STATE_DIRTY_FALSE;
        check(unsafe {
            ffi::ghostty_render_state_set(
                self.raw,
                ffi::GHOSTTY_RENDER_STATE_OPTION_DIRTY,
                (&raw const clean).cast::<c_void>(),
            )
        })
    }

    /// Default background/foreground, explicit cursor color, and the active
    /// 256-color palette.
    pub fn colors(&self) -> Result<ffi::GhosttyRenderStateColors, VtError> {
        let mut colors = ffi::GhosttyRenderStateColors::init_sized();
        check(unsafe { ffi::ghostty_render_state_colors_get(self.raw, &mut colors) })?;
        Ok(colors)
    }

    /// Whether the cursor is visible per terminal modes (DECTCEM). Distinct
    /// from [`cursor_viewport`](Self::cursor_viewport), which reports whether
    /// the cursor position falls inside the viewport.
    pub fn cursor_visible(&self) -> Result<bool, VtError> {
        let mut visible = false;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE,
            (&raw mut visible).cast::<c_void>(),
        )?;
        Ok(visible)
    }

    /// Cursor position in viewport cells, if the cursor is visible within
    /// the viewport.
    pub fn cursor_viewport(&self) -> Result<Option<(u16, u16)>, VtError> {
        let mut has_value = false;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE,
            (&raw mut has_value).cast::<c_void>(),
        )?;
        if !has_value {
            return Ok(None);
        }
        let mut x: u16 = 0;
        let mut y: u16 = 0;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
            (&raw mut x).cast::<c_void>(),
        )?;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y,
            (&raw mut y).cast::<c_void>(),
        )?;
        Ok(Some((x, y)))
    }

    /// Begin iterating viewport rows top to bottom. Row and cell data stay
    /// valid until the next [`update`](Self::update), enforced by borrows.
    pub fn rows(&mut self) -> Result<VtRows<'_>, VtError> {
        // Re-arms the pre-allocated iterator at the first viewport row.
        let mut row_iter = self.row_iter;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
            (&raw mut row_iter).cast::<c_void>(),
        )?;
        Ok(VtRows { state: self })
    }
}

impl Drop for VtRenderState {
    fn drop(&mut self) {
        unsafe {
            ffi::ghostty_render_state_row_cells_free(self.cells);
            ffi::ghostty_render_state_row_iterator_free(self.row_iter);
            ffi::ghostty_render_state_free(self.raw);
        }
    }
}

/// Streaming row iterator (not `std::iter::Iterator`: each row borrows the
/// iterator so cell data cannot outlive its row).
pub struct VtRows<'a> {
    state: &'a mut VtRenderState,
}

impl VtRows<'_> {
    pub fn next_row(&mut self) -> Option<VtRow<'_>> {
        if unsafe { ffi::ghostty_render_state_row_iterator_next(self.state.row_iter) } {
            Some(VtRow { state: self.state })
        } else {
            None
        }
    }
}

/// One viewport row positioned under the row iterator.
pub struct VtRow<'a> {
    state: &'a mut VtRenderState,
}

impl VtRow<'_> {
    fn raw_row(&self) -> Result<ffi::GhosttyRow, VtError> {
        let mut row: ffi::GhosttyRow = 0;
        check(unsafe {
            ffi::ghostty_render_state_row_get(
                self.state.row_iter,
                ffi::GHOSTTY_RENDER_STATE_ROW_DATA_RAW,
                (&raw mut row).cast::<c_void>(),
            )
        })?;
        Ok(row)
    }

    fn bool_row_data(&self, data: ffi::GhosttyRowData) -> Result<bool, VtError> {
        let mut value = false;
        check(unsafe {
            ffi::ghostty_row_get(self.raw_row()?, data, (&raw mut value).cast::<c_void>())
        })?;
        Ok(value)
    }

    /// Whether this row soft-wraps into the following row.
    pub fn wraps(&self) -> Result<bool, VtError> {
        self.bool_row_data(ffi::GHOSTTY_ROW_DATA_WRAP)
    }

    /// Whether this row continues a soft-wrapped row above it.
    pub fn wrap_continuation(&self) -> Result<bool, VtError> {
        self.bool_row_data(ffi::GHOSTTY_ROW_DATA_WRAP_CONTINUATION)
    }

    /// Per-row dirty flag. Independent from the global dirty layer.
    pub fn is_dirty(&self) -> Result<bool, VtError> {
        let mut dirty = false;
        check(unsafe {
            ffi::ghostty_render_state_row_get(
                self.state.row_iter,
                ffi::GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                (&raw mut dirty).cast::<c_void>(),
            )
        })?;
        Ok(dirty)
    }

    /// Clear this row's dirty flag after rendering it. Does not touch the
    /// global dirty layer.
    pub fn clear_dirty(&mut self) -> Result<(), VtError> {
        let clean = false;
        check(unsafe {
            ffi::ghostty_render_state_row_set(
                self.state.row_iter,
                ffi::GHOSTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                (&raw const clean).cast::<c_void>(),
            )
        })
    }

    /// Begin iterating this row's cells left to right, reusing the render
    /// state's pre-allocated cells container.
    pub fn cells(&mut self) -> Result<VtCells<'_>, VtError> {
        let mut cells = self.state.cells;
        check(unsafe {
            ffi::ghostty_render_state_row_get(
                self.state.row_iter,
                ffi::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                (&raw mut cells).cast::<c_void>(),
            )
        })?;
        Ok(VtCells {
            raw: self.state.cells,
            _row: PhantomData,
        })
    }

    /// Convenience readback of the row's text: empty cells become spaces,
    /// wide-character spacers are skipped, trailing whitespace is trimmed.
    pub fn text(&mut self) -> Result<String, VtError> {
        let mut text = String::new();
        let mut codepoints: Vec<u32> = Vec::new();
        let mut cells = self.cells()?;
        while let Some(cell) = cells.next_cell() {
            match cell.wide()? {
                VtCellWide::SpacerTail | VtCellWide::SpacerHead => continue,
                VtCellWide::Narrow | VtCellWide::Wide => {}
            }
            codepoints.clear();
            cell.append_codepoints(&mut codepoints)?;
            if codepoints.is_empty() {
                text.push(' ');
                continue;
            }
            for codepoint in &codepoints {
                text.push(char::from_u32(*codepoint).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
        }
        text.truncate(text.trim_end().len());
        Ok(text)
    }
}

/// Streaming cell iterator for one row.
pub struct VtCells<'a> {
    raw: ffi::GhosttyRenderStateRowCells,
    _row: PhantomData<&'a mut VtRenderState>,
}

impl VtCells<'_> {
    pub fn next_cell(&mut self) -> Option<VtCellRef<'_>> {
        if unsafe { ffi::ghostty_render_state_row_cells_next(self.raw) } {
            Some(VtCellRef {
                raw: self.raw,
                _cells: PhantomData,
            })
        } else {
            None
        }
    }
}

/// One cell positioned under the cells iterator.
pub struct VtCellRef<'a> {
    raw: ffi::GhosttyRenderStateRowCells,
    _cells: PhantomData<&'a mut VtRenderState>,
}

impl VtCellRef<'_> {
    fn get(
        &self,
        data: ffi::GhosttyRenderStateRowCellsData,
        out: *mut c_void,
    ) -> Result<(), VtError> {
        check(unsafe { ffi::ghostty_render_state_row_cells_get(self.raw, data, out) })
    }

    /// Number of grapheme codepoints including the base codepoint; 0 means
    /// the cell has no text.
    pub fn grapheme_len(&self) -> Result<u32, VtError> {
        let mut len: u32 = 0;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
            (&raw mut len).cast::<c_void>(),
        )?;
        Ok(len)
    }

    /// Append the cell's grapheme codepoints (base first) to `out`.
    pub fn append_codepoints(&self, out: &mut Vec<u32>) -> Result<(), VtError> {
        let len = self.grapheme_len()? as usize;
        if len == 0 {
            return Ok(());
        }
        let start = out.len();
        out.resize(start + len, 0);
        self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
            out[start..].as_mut_ptr().cast::<c_void>(),
        )?;
        Ok(())
    }

    /// Resolved foreground color, or `None` when the cell has no explicit
    /// foreground (use the render-state default).
    pub fn fg_color(&self) -> Result<Option<ffi::GhosttyColorRgb>, VtError> {
        self.optional_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR)
    }

    /// Resolved background color, or `None` when the cell has no explicit
    /// background (use the render-state default).
    pub fn bg_color(&self) -> Result<Option<ffi::GhosttyColorRgb>, VtError> {
        self.optional_color(ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR)
    }

    fn optional_color(
        &self,
        data: ffi::GhosttyRenderStateRowCellsData,
    ) -> Result<Option<ffi::GhosttyColorRgb>, VtError> {
        let mut color = ffi::GhosttyColorRgb::default();
        match unsafe {
            ffi::ghostty_render_state_row_cells_get(
                self.raw,
                data,
                (&raw mut color).cast::<c_void>(),
            )
        } {
            ffi::GHOSTTY_SUCCESS => Ok(Some(color)),
            ffi::GHOSTTY_INVALID_VALUE => Ok(None),
            code => Err(VtError { code }),
        }
    }

    /// Full SGR style for the cell (default style when unstyled).
    pub fn style(&self) -> Result<ffi::GhosttyStyle, VtError> {
        let mut style = ffi::GhosttyStyle::init_sized();
        self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
            (&raw mut style).cast::<c_void>(),
        )?;
        Ok(style)
    }

    /// Width behavior; spacer cells must not be rendered.
    pub fn wide(&self) -> Result<VtCellWide, VtError> {
        let mut wide: ffi::GhosttyCellWide = 0;
        check(unsafe {
            ffi::ghostty_cell_get(
                self.raw_cell()?,
                ffi::GHOSTTY_CELL_DATA_WIDE,
                (&raw mut wide).cast::<c_void>(),
            )
        })?;
        Ok(match wide {
            ffi::GHOSTTY_CELL_WIDE_WIDE => VtCellWide::Wide,
            ffi::GHOSTTY_CELL_WIDE_SPACER_TAIL => VtCellWide::SpacerTail,
            ffi::GHOSTTY_CELL_WIDE_SPACER_HEAD => VtCellWide::SpacerHead,
            _ => VtCellWide::Narrow,
        })
    }

    /// Whether the cell carries an OSC 8 hyperlink. The URI itself is read
    /// through [`VtTerminal::hyperlink_uri_at_viewport`] on demand.
    pub fn has_hyperlink(&self) -> Result<bool, VtError> {
        let mut has_hyperlink = false;
        check(unsafe {
            ffi::ghostty_cell_get(
                self.raw_cell()?,
                ffi::GHOSTTY_CELL_DATA_HAS_HYPERLINK,
                (&raw mut has_hyperlink).cast::<c_void>(),
            )
        })?;
        Ok(has_hyperlink)
    }

    fn raw_cell(&self) -> Result<ffi::GhosttyCell, VtError> {
        let mut raw_cell: ffi::GhosttyCell = 0;
        self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
            (&raw mut raw_cell).cast::<c_void>(),
        )?;
        Ok(raw_cell)
    }
}

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
        unsafe { ffi::ghostty_key_encoder_setopt_from_terminal(self.encoder, terminal.raw) };
        let value: ffi::GhosttyOptionAsAlt = match option_as_alt {
            VtOptionAsAlt::False => ffi::GHOSTTY_OPTION_AS_ALT_FALSE,
            VtOptionAsAlt::True => ffi::GHOSTTY_OPTION_AS_ALT_TRUE,
            VtOptionAsAlt::Left => ffi::GHOSTTY_OPTION_AS_ALT_LEFT,
            VtOptionAsAlt::Right => ffi::GHOSTTY_OPTION_AS_ALT_RIGHT,
        };
        unsafe {
            ffi::ghostty_key_encoder_setopt(
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
        unsafe {
            ffi::ghostty_key_event_set_action(self.event, action);
            ffi::ghostty_key_event_set_key(self.event, input.key);
            ffi::ghostty_key_event_set_mods(self.event, input.mods);
            ffi::ghostty_key_event_set_consumed_mods(self.event, input.consumed_mods);
            ffi::ghostty_key_event_set_composing(self.event, false);
            ffi::ghostty_key_event_set_unshifted_codepoint(self.event, input.unshifted_codepoint);
            match input.utf8 {
                Some(text) => {
                    ffi::ghostty_key_event_set_utf8(self.event, text.as_ptr(), text.len())
                }
                None => ffi::ghostty_key_event_set_utf8(self.event, std::ptr::null(), 0),
            }
        }
        let result = encode_with_retry(out, |buf, len, written| unsafe {
            ffi::ghostty_key_encoder_encode(self.encoder, self.event, buf, len, written)
        });
        unsafe { ffi::ghostty_key_event_set_utf8(self.event, std::ptr::null(), 0) };
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
        unsafe {
            ffi::ghostty_mouse_encoder_setopt(
                encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL,
                (&raw const track).cast::<c_void>(),
            )
        };
        Ok(Self { encoder, event })
    }

    /// Sync tracking mode and output format from the terminal's state.
    pub fn sync_from_terminal(&mut self, terminal: &mut VtTerminal) {
        unsafe { ffi::ghostty_mouse_encoder_setopt_from_terminal(self.encoder, terminal.raw) };
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
        unsafe {
            ffi::ghostty_mouse_encoder_setopt(
                self.encoder,
                ffi::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                (&raw const size).cast::<c_void>(),
            )
        };
    }

    /// Tell the encoder whether any button is held (button-event tracking
    /// only reports motion while a button is pressed).
    pub fn set_any_button_pressed(&mut self, pressed: bool) {
        unsafe {
            ffi::ghostty_mouse_encoder_setopt(
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
        unsafe {
            ffi::ghostty_mouse_event_set_action(self.event, action);
            match input.button {
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
            }
            ffi::ghostty_mouse_event_set_mods(self.event, input.mods);
            ffi::ghostty_mouse_event_set_position(
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
fn encode_with_retry(
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
