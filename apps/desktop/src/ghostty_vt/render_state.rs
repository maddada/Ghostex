use super::*;
use std::{ffi::c_void, marker::PhantomData};

/// Resolve an SGR style color (e.g. `GhosttyStyle::underline_color`) against
/// the active palette. `None` means the style has no explicit color; use the
/// relevant default. Lives here so the union field reads stay in the FFI
/// choke point.
pub fn style_color_rgb(
    color: &ffi::GhosttyStyleColor,
    palette: &[ffi::GhosttyColorRgb; 256],
) -> Option<ffi::GhosttyColorRgb> {
    match color.tag() {
        ffi::GHOSTTY_STYLE_COLOR_PALETTE => Some(palette[unsafe { color.value.palette } as usize]),
        ffi::GHOSTTY_STYLE_COLOR_RGB => Some(unsafe { color.value.rgb }),
        _ => None,
    }
}

/// The palette index of a style colour, when it names one (`bold-color` maps
/// bold 0-7 to 8-15 by index, not by the resolved RGB).
pub fn style_color_palette_index(color: &ffi::GhosttyStyleColor) -> Option<u8> {
    (color.tag() == ffi::GHOSTTY_STYLE_COLOR_PALETTE).then_some(unsafe { color.value.palette })
}

/// A cell's SGR style with the attribute bits of style.h's packed
/// `GhosttyStyle::flags` decoded into fields. The background is left out:
/// renderers read the resolved one from [`VtCellRef::bg_color`].
#[derive(Clone, Copy)]
pub struct VtStyle {
    pub fg_color: ffi::GhosttyStyleColor,
    pub underline_color: ffi::GhosttyStyleColor,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    /// A `GHOSTTY_SGR_UNDERLINE_*` value.
    pub underline: ffi::GhosttySgrUnderline,
}

impl From<ffi::GhosttyStyle> for VtStyle {
    fn from(style: ffi::GhosttyStyle) -> Self {
        let flag = |bit: ffi::GhosttyStyleFlags| style.flags & bit != 0;
        Self {
            fg_color: style.fg_color,
            underline_color: style.underline_color,
            bold: flag(ffi::GHOSTTY_STYLE_FLAG_BOLD),
            italic: flag(ffi::GHOSTTY_STYLE_FLAG_ITALIC),
            faint: flag(ffi::GHOSTTY_STYLE_FLAG_FAINT),
            blink: flag(ffi::GHOSTTY_STYLE_FLAG_BLINK),
            inverse: flag(ffi::GHOSTTY_STYLE_FLAG_INVERSE),
            invisible: flag(ffi::GHOSTTY_STYLE_FLAG_INVISIBLE),
            strikethrough: flag(ffi::GHOSTTY_STYLE_FLAG_STRIKETHROUGH),
            overline: flag(ffi::GHOSTTY_STYLE_FLAG_OVERLINE),
            underline: ffi::GhosttySgrUnderline::from(
                (style.flags & ffi::GHOSTTY_STYLE_UNDERLINE_MASK)
                    >> ffi::GHOSTTY_STYLE_UNDERLINE_SHIFT,
            ),
        }
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
                ffi::GHOSTTY_RENDER_STATE_OPT_DIRTY,
                (&raw const clean).cast::<c_void>(),
            )
        })
    }

    /// Default background/foreground, explicit cursor color, and the active
    /// 256-color palette.
    pub fn colors(&self) -> Result<ffi::GhosttyRenderStateColors, VtError> {
        let mut colors = ffi::GhosttyRenderStateColors::init_sized();
        check(unsafe {
            ffi::ghostty_render_state_get(
                self.raw,
                ffi::GHOSTTY_RENDER_STATE_DATA_COLORS,
                (&mut colors as *mut ffi::GhosttyRenderStateColors).cast(),
            )
        })?;
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
                ffi::GHOSTTY_RENDER_STATE_ROW_OPT_DIRTY,
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
    #[allow(dead_code)] // used by the ghostty-vt-smoke / terminal-model-smoke binaries
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
        let mut buffer = ffi::GhosttyCodepointBuffer {
            ptr: out[start..].as_mut_ptr(),
            cap: len,
            len: 0,
        };
        let result = self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
            (&raw mut buffer).cast::<c_void>(),
        );
        // The buffer was sized from GRAPHEMES_LEN, so it always fits; keep
        // only what the library reports it wrote.
        out.truncate(start + buffer.len.min(len));
        result
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
    pub fn style(&self) -> Result<VtStyle, VtError> {
        // Not a sized struct any more: the library writes all 16 bytes.
        let mut style: ffi::GhosttyStyle = unsafe { std::mem::zeroed() };
        self.get(
            ffi::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
            (&raw mut style).cast::<c_void>(),
        )?;
        Ok(style.into())
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
