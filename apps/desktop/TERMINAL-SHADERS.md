# Ghostty custom shaders in terminal panes (macOS, experimental)

Ghostex can apply the `custom-shader` chain from the user's Ghostty config to
the pixels its GPUI terminal paints. The terminal model, PTY, input handling,
selection, agent controls, Chat, session lifecycle and extension bridges keep
their existing owners and protocols. There is no second terminal, native child
view or protocol change. Extensions cannot do this: they cannot reach the
terminal's final GPU texture.

## Using it

1. List `custom-shader` files in the normal Ghostty config. They run in the
   configured order. `?`-prefixed optional paths are skipped when missing.
2. Turn on **Enable Experimental Features**, then **Settings → General →
   Terminal → Custom shaders (experimental)** (`terminalShadersEnabled`,
   default `false`).
3. Turning it off restores the ordinary renderer in the same sessions. No agent
   restart is needed.

Enable Experimental Features controls whether this setting is visible,
including in search. Hiding the setting preserves its saved choice; turn
Custom shaders off before hiding experimental controls if you want ordinary
rendering.

While Custom shaders is on, **Terminal background** and **Terminal background
color** are disabled with an explanation. Their saved values are preserved.
The chain uses the background in your Ghostty config instead. Background images,
fonts, cursor controls and terminal interactions remain available. Glass affects
the content behind transparent shader output; shaders that write opaque alpha
can cover it.

`custom-shader-animation` follows Ghostty: `true` animates the focused pane
while its window is active, `always` animates every painted pane, and `false`
redraws only when something changes. Hidden terminals do not animate. Config
and shader edits apply on Ghostex's normal terminal config reload.

No GLSL files are bundled.

## How it works

- `apps/desktop/src/terminal_shaders.rs` loads the chain and fills the
  4496-byte uniform block Ghostty's shaders expect (time, resolution, cursor
  history, focus and palette).
- Ghostty's own GLSL → SPIR-V → MSL pipeline translates each file through the
  Darwin-only `ghostty_custom_shader_load_msl` export
  (`.dependencies/ghostty-patches/0008-embed-custom-shader-msl-api.patch`).
  Rebuild GhosttyKit from the vendored source after updating the checkout.
- A GPUI effect API (`Window::paint_effect`, a commit on the `ghostex` branch
  of the pinned `maddada/zed` fork) captures the terminal's paint
  into reusable Metal textures, runs the passes and composites the result
  inside the pane clip.
- Translation is cached and bounded. A translation failure keeps the ordinary
  renderer. A Metal pipeline failure displays the usable unshaded capture,
  which can retain the configured shader background and opacity until the
  option is turned off. Unsupported backends and oversized canvases keep
  ordinary painting.

While effects are on, the grid uses the finalized Ghostty background colour
and byte-quantized `background-opacity`, so shader input and
`iBackgroundColor` agree. Ghostty's macOS glass blur styles clear that
background, as they do upstream. Turning shaders off restores the ordinary
Terminal background choice and Glass.

## Known limits

- macOS with Metal only. Windows, Linux and the web build keep the existing
  renderer. Intel Macs have not been runtime-tested.
- Shaders control their output alpha. A shader that writes alpha `1.0` makes
  the pane opaque even with Glass.
- App-owned pane padding is outside the effect, so a shader that bends or
  replaces the background can show a band at non-zero padding.
- Linear blending, wide-gamut colour, glyph rasterization and configured
  selection or cursor-text colour uniforms are not pixel-identical to native
  Ghostty.
- Chat, app chrome and embedded web pages are not shaded.

## Practical test scope

A reviewer can check the feature with any public Ghostty shader:

- Build: rebuild GhosttyKit, then
  `cargo check --bin ghostex-gpui` from `apps/desktop`. Run
  `bun run typecheck`, `bun run desktop:typecheck` and
  `packages/core-ui/settings-modal-source.test.ts`.
- Default: the setting is off and hidden until experimental features are on.
  Rendering must match an unpatched build.
- On: effects show in existing and newly split panes and follow resizing. Each
  `custom-shader-animation` value behaves as described above.
- Input: keys, paste, scroll, selection and copy send the same bytes with
  effects on and off. Chat and terminal extensions keep working.
- Failure: an invalid shader disables the chain and leaves the shell usable.
  Fixing it and reloading the config restores the effects.
- Off again: ordinary pixels, background choice and Glass return without
  restarting sessions.

Every third-party shader, extension, agent CLI and remote workflow has not been
checked. Representative checks and unchanged ownership support compatibility.
They do not guarantee it.

## Rebuilding the embedded compiler

An older GhosttyKit archive does not export the new shader API. The macOS
packager checks the target slice and prints this rebuild command before
compilation if the archive is missing or stale. With Zig 0.17.0 and the Metal
toolchain installed, run from the repository root:

```sh
(cd .dependencies/ghostty &&
 env DEVELOPER_DIR="$(xcode-select -p)" \
 SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" \
 GHOSTTY_METAL_DEVELOPER_DIR="$(xcode-select -p)" \
 zig build -Demit-xcframework -Dxcframework-target=universal \
 -Demit-macos-app=false -Doptimize=ReleaseSafe)
```

The staged GhosttyKit release job builds the shader export from the tracked
vendored source.
