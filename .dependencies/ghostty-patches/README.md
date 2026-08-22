# Ghostex patches on the vendored Ghostty tree

`ghostty/` is a vendored copy of upstream [ghostty-org/ghostty] at the commit
recorded in [`UPSTREAM`](UPSTREAM), plus the patch series in this directory.
The tree in-repo is kept **already patched** (builds need no patch step); this
directory exists so the delta stays explicit, reviewable, and mechanically
re-appliable on the next upstream sync.

Everything not covered by a patch here is pristine upstream. If you change a
file under `ghostty/`, regenerate the affected patch (see below) in the same
commit, or the next sync will silently drop your change.

## Sync procedure

```sh
scripts/sync-ghostty.sh <upstream-ref>   # e.g. scripts/sync-ghostty.sh origin/main
```

The script replaces `ghostty/` with the pristine upstream tree at the ref,
re-applies this series with `patch -p1`, and updates `UPSTREAM`. Conflicting
patches are reported and must be rebased by hand (edit the file, then
regenerate that patch). After a sync always:

1. `cd ghostty && zig build test-lib-vt` (full suite must pass)
2. `cd gpui && cargo check` (build.rs rebuilds libghostty-vt.a)
3. Re-audit `gpui/src/ghostty_vt.rs` and `gpui/src/ghostty_kit.rs` against
   `ghostty/include/` — **the implicit C enums (especially
   `ghostty_action_tag_e`) renumber when upstream inserts entries, and Rust
   FFI decls are trusted blindly by the compiler.** The 2026-08-14 sync
   caught a silent +2 shift in every action tag this way.
4. Rebuild the local GhosttyKit xcframework (macOS):
   `cd ghostty && zig build -Demit-xcframework -Dxcframework-target=universal -Demit-macos-app=false -Doptimize=ReleaseSafe`

## The series

- **0001-build-lib-vt-shared-option-and-themes-install** — adds
  `-Demit-lib-vt-shared` (skip the dylib for static-only consumers; Zig
  could not link macOS dylibs against arm64e-only Xcode SDKs), installs the
  bundled themes in lib-vt builds (gpui embeds them), and skips the GTK
  pkg-config probe on Windows (hard-fails when PATH has an unavailable
  drive).
- **0002-build-xcframework-lazy-universal** — only build the universal
  (x86_64+arm64) GhosttyKit library when `-Dxcframework-target=universal`
  is requested.
- **0003-build-metallib-developer-dir-override** — honor
  `GHOSTTY_METAL_DEVELOPER_DIR` for the metal/metallib xcrun steps
  (macOS-27 machines whose default toolchain lacks the Metal compiler).
- **0004-app-debug-skip-slow-integrity-checks** — full-app Debug builds skip
  the page/PageList integrity scans except under `zig test`
  (`builtin.is_test`). Note the *terminal module options* path
  (`src/build/Config.zig terminalOptions`) intentionally stays on upstream
  semantics: Zig tests share those modules and comptime-assert the checks
  are on. Use `-Doptimize=ReleaseSafe` for a fast dev GhosttyKit.
- **0005-embed-config-string-apis** — `ghostty_config_load_string` +
  `ghostty_config_to_string` C APIs used by gpui to round-trip Ghostty
  config through embedded hosts.
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

## Dropped in the 2026-08-14 sync (were in the tree before)

- `write_pty_cb` surface option / `ghostty_surface_write_data` /
  termio `Thread.zig` write routing — served the retired native iOS app;
  no active consumer. The matching field was removed from
  `gpui/src/ghostty_kit.rs`'s `ghostty_surface_config_s` mirror.
- `ghostty_surface_padding` — served the deprecated Swift macOS app (zmux);
  no consumer anywhere.
- `config/Wasm.zig` std.Io modernization — upstream made the same change.
- Backward-shift-deletion hash_map, `eraseRow` capacity handling, etc. —
  landed upstream (they came from Ghostex or were fixed independently).

## Regenerating a patch after editing `ghostty/<file>`

```sh
scripts/sync-ghostty.sh --regen   # rebuilds every patch from the current tree
```

[ghostty-org/ghostty]: https://github.com/ghostty-org/ghostty
