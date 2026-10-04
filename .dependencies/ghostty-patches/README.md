# Ghostex patches on the vendored Ghostty tree

`.dependencies/ghostty/` is a vendored copy of upstream [ghostty-org/ghostty]
at the commit recorded in [`UPSTREAM`](UPSTREAM), plus the patch series in this
directory.
The tree in-repo is kept **already patched** (builds need no patch step); this
directory exists so the delta stays explicit, reviewable, and mechanically
re-appliable on the next upstream sync.

Everything not covered by a patch here is pristine upstream. If you change a
file under `.dependencies/ghostty/`, regenerate the affected patch (see below)
in the same commit, or the next sync will silently drop your change.

## Sync procedure

```sh
tooling/sync-ghostty.sh <upstream-ref>   # e.g. tooling/sync-ghostty.sh origin/main
```

The script replaces `.dependencies/ghostty/` with the pristine upstream tree at
the ref, re-applies this series with `patch -p1`, and updates `UPSTREAM`.
Conflicting patches are reported and must be rebased by hand (edit the file,
then regenerate that patch). After a sync always:

1. `cd .dependencies/ghostty && zig build test-lib-vt` (full suite must pass)
2. `cd apps/desktop && cargo check` (build.rs rebuilds libghostty-vt.a)
3. Re-audit `apps/desktop/src/ghostty_vt.rs` and
   `apps/desktop/src/ghostty_kit.rs` against `.dependencies/ghostty/include/` —
   **the implicit C enums (especially
   `ghostty_action_tag_e`) renumber when upstream inserts entries, and Rust
   FFI decls are trusted blindly by the compiler.** The 2026-08-14 sync
   caught a silent +2 shift in every action tag this way.
4. Rebuild the local GhosttyKit xcframework (macOS):
   `cd .dependencies/ghostty && zig build -Demit-xcframework -Dxcframework-target=universal -Demit-macos-app=false -Doptimize=ReleaseSafe`
   (with `DEVELOPER_DIR`, `SDKROOT` and `GHOSTTY_METAL_DEVELOPER_DIR` set the
   way `apps/desktop/scripts/build-macos-app.sh` prints them). The sync keeps
   the old xcframework in place, so every desktop build keeps linking while
   this runs; building it in a staging copy first and swapping the files in
   right after the sync keeps the mismatch window to seconds.
5. Refresh the web build's cached wasm archive
   (`apps/gpui-web/target/libghostty-vt-wasm/`): `apps/gpui-web/build-wasm.mjs`
   only builds it when it is missing, so after a sync delete it (or build a new
   one with the flags in that script and swap it in) or the browser build keeps
   the old Ghostty.
6. Check `copy-on-select` and any other Ghostty config key whose values
   `apps/desktop/src/terminal_ghostty_surface.rs` parses from the finalized
   config string: upstream renames values there (Ghostty 1.4 did for
   `copy-on-select`), and the parse matches on literal strings.

## The series

- **0001-build-lib-vt-shared-option-and-themes-install** — adds
  `-Demit-lib-vt-shared` (skip the dylib for static-only consumers; Zig
  could not link macOS dylibs against arm64e-only Xcode SDKs), installs the
  bundled themes in lib-vt builds (the desktop app embeds them), and skips
  the GTK pkg-config probe on Windows (hard-fails when PATH has an
  unavailable drive).
- **0002-build-xcframework-lazy-universal** — only build the universal
  (x86_64+arm64) GhosttyKit library when `-Dxcframework-target=universal`
  is requested.
- **0003-build-metallib-developer-dir-override** — honor
  `GHOSTTY_METAL_DEVELOPER_DIR` for the metal/metallib xcrun steps
  (macOS-27 machines whose default toolchain lacks the Metal compiler).
- **0005-embed-config-string-apis** — `ghostty_config_load_string` +
  `ghostty_config_to_string` C APIs used by the desktop app to round-trip
  Ghostty config through embedded hosts. Its `include/ghostty.h` diff also
  carries the declaration for 0008 (regen diffs each file whole, so a header
  can belong to only one patch).
- **0006-mouse-cmd-click-encode-and-mod-dedupe** — encode macOS Cmd as the
  Ctrl bit in terminal mouse protocols (Cmd-click opens paths/links in
  TUIs) and include binding modifiers in same-cell motion dedupe so
  pressing Cmd over a cell still reaches the TUI (`last_mods` plumbing;
  the `Surface.zig` `event_mods` hunks in patch 0007 belong to this
  feature). Ships two regression tests.
- **0007-teardown-deadlock-hardening** — every cross-thread mailbox push
  reachable during `ghostty_surface_free` teardown becomes bounded (1s)
  instead of `.forever`, the renderer completion callback releases its
  frame semaphore before any potentially-blocking push, `killCommand`'s
  process-group kill loop escalates SIGHUP→SIGKILL and gives up after ~5s,
  and the fork/setsid pgid retry is bounded. Fixes the 2026-07-10/11
  process-wide freeze family (io-thread ↔ app-thread join cycles). Also
  contains a bounds guard on the renderer's shaper-cell advance scan.
- **0008-embed-custom-shader-msl-api** — Darwin-only
  `ghostty_custom_shader_load_msl`, which runs a Shadertoy custom shader file
  through Ghostty's own `renderer/shadertoy.zig` pipeline (uniform prefix,
  glslang, SPIRV-Cross) and returns the MSL for Ghostex terminal shaders. The
  result is freed with `ghostty_string_free`.
- **0009-lib-vt-virtual-image-placements** — exposes resolved viewport
  Unicode-placeholder runs through `ghostty_kitty_graphics_virtual_placements`.
  Reuses Ghostty's placeholder iterator and aspect-fit calculation, preserving
  fractional source coverage for enlarged images. The native integer helper
  retains its original result. The C result contains owned geometry and image
  IDs, with no borrowed pins. The
  native GPUI terminal renderer uses this API. Ghostty currently disables
  Kitty graphics on the freestanding browser target, so the shared web
  renderer builds with an empty image state. Ordinary relative descendants
  of virtual placements resolve from the root’s minimum visible placeholder
  coordinates, using Ghostty’s parent-chain and current-cell geometry rules.
- **0010-lib-vt-image-resource-limits** — adds per-terminal maximum image
  dimension and decoded pixel-count options. Oversized declared raw dimensions
  are rejected before buffering; PNG output is checked before retention. The
  core's inherited transmission/decompression ceiling remains 400 MiB, while
  the host decoder rejects encoded PNG input above 40 MiB after buffering. Limits survive RIS
  and screen changes; standalone Ghostty keeps its existing defaults. The
  native GPUI bridge sets 4096 per side and 4 Mi pixels (16 MiB RGBA8). Its
  PNG decoder separately allows 32 MiB of decode working allocation for
  16-bit source pixels before conversion to RGBA8. These are
  application resource limits, not a query of the GPU's texture capacity.

## Rebased in the 2026-09-27 sync

- `0001` — upstream turned the libghostty-vt shared library into an optional
  (null for native freestanding targets), so the `-Demit-lib-vt-shared=false`
  skip is now an early `break :shared null`, and the "cannot execute the ABI
  manifest" schema-step error is raised only for native freestanding targets
  rather than whenever the shared library is skipped. The other patches only
  moved line offsets; none was dropped (upstream has not changed the patched
  code in `GhosttyXCFramework.zig`, `MetallibStep.zig`, `CApi.zig`,
  `mouse_encode.zig` or `termio/mailbox.zig`, and still uses unbounded pushes
  on the teardown paths 0007 bounds).

## Dropped in the 2026-08-26 sync

- `0004-app-debug-skip-slow-integrity-checks` — every Ghostex Ghostty build
  path uses `-Doptimize=ReleaseSafe`, so its Debug-only integrity-check skip
  has no consumer. Its full former content is preserved in
  [`DROPPED-PATCHES.md`](../../docs/2026-08-26/ghostty-sync/DROPPED-PATCHES.md).

## Dropped in the 2026-08-14 sync (were in the tree before)

- `write_pty_cb` surface option / `ghostty_surface_write_data` /
  termio `Thread.zig` write routing — served the retired native iOS app;
  no active consumer. The matching field was removed from
  `apps/desktop/src/ghostty_kit.rs`'s `ghostty_surface_config_s` mirror.
- `ghostty_surface_padding` — served the deprecated Swift macOS app (zmux);
  no consumer anywhere.
- `config/Wasm.zig` std.Io modernization — upstream made the same change.
- Backward-shift-deletion hash_map, `eraseRow` capacity handling, etc. —
  landed upstream (they came from Ghostex or were fixed independently).

## Regenerating a patch after editing `.dependencies/ghostty/<file>`

```sh
tooling/sync-ghostty.sh --regen   # rebuilds every patch from the current tree
```

[ghostty-org/ghostty]: https://github.com/ghostty-org/ghostty
