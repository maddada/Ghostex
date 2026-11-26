# Ghostex changes to voxl-server 0.4.0

This is the published voxl-server 0.4.0 crate (MIT, https://github.com/tembo/voxl), with one change
for Ghostex's remote screen helper. It is meant to go upstream; drop this copy once a release has it.

## Windows: `--max-capture-pixels`

macOS already streams a smaller copy of a display above a pixel budget (`--max-capture-pixels`).
Windows always streamed the display's own size, so a phone viewing a 4K PC got a 3840x2160 H.264
stream (level 5.2), which many phones decode slowly or refuse. The same option now works on Windows
(default 0: the display's own size, so nothing changes unless it is given):

- `src/win/mod.rs`: reads the option; `stream_size()` picks the streamed size (same shape, even);
  `Geometry` carries the streamed size beside the display's.
- `src/win/capture.rs`: capture stays at the display's size; each changed band is scaled into a
  smaller BGRA copy with libyuv `ARGBScaleClip` (box filter), which conversion, snapshots and the
  latency marker then read. Changed areas are mapped to streamed pixels (grown by one for the filter).
- `src/win/input.rs`: pointer positions arrive in streamed pixels and are scaled back to the display.
- `src/help.rs`: the option in the Windows help.

`.cargo-ok` was removed (a registry marker); nothing else differs from the crate.
