/*
CDXC:Terminal 2026-07-03:
Phase 1 GPUI-composited terminals are driven by libghostty-vt (vendored under
.dependencies/ghostty/, MIT), whose C API is functionally stable but explicitly NOT
API-stable. This module is the single choke point over that C API: every
libghostty-vt symbol, struct layout, and enum value used by Rust lives here so
a vendored API bump touches one file. Do not declare ghostty_vt symbols in
other modules, and do not expose raw handles outside this module.

CDXC:Terminal 2026-07-03 (dirty-tracking contract):
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

// 1:1 bindings to libghostty-vt's C headers. The enum-value constants and
// `extern "C"` entry points are kept complete on purpose, so a few of them have
// no Rust caller yet; that is a property of the binding layer, not dead code.
#[allow(dead_code)]
pub mod ffi;

mod host_callbacks;
mod images;
mod input_encoders;
mod render_state;
mod terminal;

pub use host_callbacks::*;
pub use images::VtImagePlacement;
pub use input_encoders::*;
pub use render_state::*;
pub use terminal::*;
