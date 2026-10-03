use super::*;
use std::{ffi::c_void, fmt};

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

pub(crate) fn check(result: ffi::GhosttyResult) -> Result<(), VtError> {
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
    pub(crate) raw: ffi::GhosttyTerminal,
    /// Heap cell registered as the terminal's userdata; trampolines below
    /// dispatch through it. Null until [`set_host_callbacks`] installs hooks.
    ///
    /// [`set_host_callbacks`]: Self::set_host_callbacks
    host_callbacks: *mut VtHostCallbacks,
    images: super::images::VtImageState,
}

// SAFETY: libghostty-vt terminal state has no thread affinity (no TLS, no
// run-loop coupling); it only requires exclusive access, which &mut methods
// and the !Sync auto impl (raw pointer field) already enforce.
unsafe impl Send for VtTerminal {}

impl VtTerminal {
    pub fn new(cols: u16, rows: u16, max_scrollback: usize) -> Result<Self, VtError> {
        let images = super::images::VtImageState::new()?;
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
        if let Err(err) =
            scrollback_result.and_then(|()| super::images::VtImageState::configure(raw))
        {
            unsafe { ffi::ghostty_terminal_free(raw) };
            return Err(err);
        }
        Ok(Self {
            raw,
            host_callbacks: std::ptr::null_mut(),
            images,
        })
    }

    /// Copy active-screen image state while exclusive terminal access is held.
    pub fn image_placements(&mut self) -> Result<Vec<VtImagePlacement>, VtError> {
        self.images.snapshot(self.raw)
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
    #[allow(dead_code)] // public VT wrapper API kept complete over the ghostty_terminal_reset binding
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
