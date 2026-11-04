//! `QrCode` (packages/components/ui/qr-code.tsx (deleted 2026-10-01)): the `qrcode` library's render of a
//! payload, black modules on a white tile with a 4-module quiet zone, at a square pixel size.
//! Modules are painted as runs of squares snapped to the pixel edges the canvas renderer uses
//! (`floor(pixel / scale)` picks the module), as the Remote Setup dialog's QR does.
//!
//! CDXC:RemotePairing 2026-10-27 WHY:
//! An Android tester's phone saw nothing when pointed at the pairing QR: a version 19 code at error
//! correction M, `#111113` on `#f4f4f5` with a 2-module quiet zone, set straight against the dark
//! dialog. Phone scanners find a code by its finder patterns against light space, so the tile now
//! has the 4-module quiet zone the QR standard asks for, pure black on pure white, and error
//! correction L: a code on a screen is never torn or smudged, and L takes the pairing link down
//! one or two versions, which makes every module larger at the same size.
use super::super::super::native_modal_kit::*;
use gpui::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, px, rgb};

const QR_MARGIN_MODULES: usize = 4;
const QR_DARK: u32 = 0x000000;
const QR_LIGHT: u32 = 0xffffff;

/// The QR modules, row-major, `true` for a dark module.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QrGrid {
    width: usize,
    dark: Vec<bool>,
}

impl QrGrid {
    pub(crate) fn encode(text: &str) -> Option<Self> {
        let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::L)
            .ok()?;
        let width = code.width();
        let dark = code
            .to_colors()
            .into_iter()
            .map(|color| color == qrcode::Color::Dark)
            .collect();
        Some(Self { width, dark })
    }
}

/// The QR tile `size` pixels square (the light quiet zone with the dark modules on it). `None`
/// draws the tile empty, the placeholder the React component shows while its image renders.
pub(crate) fn qr_code(grid: Option<&QrGrid>, size: f32) -> AnyElement {
    let mut frame = div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .overflow_hidden()
        .bg(hsla(rgb(QR_LIGHT)));
    if let Some(grid) = grid {
        let total = grid.width + QR_MARGIN_MODULES * 2;
        let scale = size / total as f32;
        let edge = |index: usize| (index as f32 * scale).ceil();
        for row in 0..grid.width {
            let top = edge(row + QR_MARGIN_MODULES);
            let bottom = edge(row + QR_MARGIN_MODULES + 1);
            let mut column = 0;
            while column < grid.width {
                if !grid.dark[row * grid.width + column] {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < grid.width && grid.dark[row * grid.width + column] {
                    column += 1;
                }
                let left = edge(start + QR_MARGIN_MODULES);
                let right = edge(column + QR_MARGIN_MODULES);
                frame = frame.child(
                    div()
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(right - left))
                        .h(px(bottom - top))
                        .bg(hsla(rgb(QR_DARK))),
                );
            }
        }
    }
    frame.into_any_element()
}
