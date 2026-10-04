// CDXC:Zmx 2026-10-04 SEE-ALSO: zmx src/loop.zig::clientLoop emits these
// nonce-scoped APC records with four fixed-width hex fields and an ESC\ terminator.
/// The attach process translates ordered GridSync IPC into fixed-length,
/// nonce-scoped records. Strip them before the VT parser: a daemon resize
/// can happen between bytes of UTF-8, CSI or a Kitty transmission.
pub(super) struct ViewerGridParser {
    prefix: Vec<u8>,
    pending: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ViewerGrid {
    pub(super) cols: u16,
    pub(super) rows: u16,
    pub(super) cell_width: u32,
    pub(super) cell_height: u32,
}

#[derive(Default)]
pub(super) struct ViewerGridState {
    pub(super) grid: Option<ViewerGrid>,
    pub(super) desired: Option<ViewerGrid>,
}

pub(super) enum ViewerGridChunk<'a> {
    Bytes(&'a [u8]),
    Grid(Option<ViewerGrid>),
}

impl ViewerGridParser {
    pub(super) fn new(nonce: &str) -> Self {
        Self {
            prefix: format!("\x1b_ZMX_GRID;{nonce};").into_bytes(),
            pending: Vec::with_capacity(64),
        }
    }

    pub(super) fn feed<E>(
        &mut self,
        bytes: &[u8],
        mut accept: impl FnMut(ViewerGridChunk<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.pending.is_empty() {
            return self.parse(bytes, &mut accept);
        }
        // Allocate only when a record straddles reads. Ordinary ANSI output
        // stays in one borrowed feed, instead of one FFI call per escape.
        let mut joined = std::mem::take(&mut self.pending);
        joined.extend_from_slice(bytes);
        self.parse(&joined, &mut accept)
    }

    fn parse<E>(
        &mut self,
        mut bytes: &[u8],
        accept: &mut impl FnMut(ViewerGridChunk<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        while !bytes.is_empty() {
            // Old daemons emit no records. Skip ordinary output with one
            // byte search, comparing the nonce-scoped prefix only at ESC.
            let mut search = bytes;
            let mut start = None;
            let mut tail = 0;
            while let Some(offset) = search.iter().position(|byte| *byte == 0x1b) {
                search = &search[offset..];
                if search.starts_with(&self.prefix) {
                    start = Some(bytes.len() - search.len());
                    break;
                }
                if self.prefix.starts_with(search) {
                    tail = search.len();
                    break;
                }
                search = &search[1..];
            }
            if let Some(start) = start {
                if start != 0 {
                    accept(ViewerGridChunk::Bytes(&bytes[..start]))?;
                }
                bytes = &bytes[start..];
                let length = self.prefix.len() + 18;
                if bytes.len() < length {
                    self.pending.extend_from_slice(bytes);
                    break;
                }
                if let Some(grid) = Self::decode(&bytes[self.prefix.len()..length]) {
                    accept(ViewerGridChunk::Grid(grid))?;
                    bytes = &bytes[length..];
                } else {
                    // Invalid records remain ordinary program output.
                    accept(ViewerGridChunk::Bytes(&bytes[..1]))?;
                    bytes = &bytes[1..];
                }
                continue;
            }
            // Retain only a possible prefix suffix. All other bytes, including
            // unknown APC and C1 bytes, reach Ghostty without reinterpretation.
            accept(ViewerGridChunk::Bytes(&bytes[..bytes.len() - tail]))?;
            self.pending.extend_from_slice(&bytes[bytes.len() - tail..]);
            break;
        }
        Ok(())
    }

    fn decode(body: &[u8]) -> Option<Option<ViewerGrid>> {
        if &body[16..] != b"\x1b\\" {
            return None;
        }
        let mut values = [0u16; 4];
        for (slot, digits) in values.iter_mut().zip(body[..16].chunks_exact(4)) {
            *slot = u16::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
        }
        let [rows, cols, width, height] = values;
        if values == [0; 4] {
            return Some(None);
        }
        if rows == 0 || cols == 0 {
            return None;
        }
        Some(Some(ViewerGrid {
            rows,
            cols,
            cell_width: u32::from(width / cols),
            cell_height: u32::from(height / rows),
        }))
    }

    pub(super) fn finish<E>(
        &mut self,
        mut accept: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        accept(&self.pending)?;
        self.pending.clear();
        Ok(())
    }
}
