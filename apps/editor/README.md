# Ghostex Editor

Standalone prompt editor for `GhostexEditor.app` on macOS and the Rust/wry
host on Linux and Windows. It mounts the same Lexical input as Session Chat,
including its editing commands, find/replace, reference links, and undo/redo.
The host adds save/cancel, cursor restoration, image paste, and image previews.

## Build

From the repository root:

```sh
bun apps/editor/scripts/build-editor-web.mjs
bash apps/editor/scripts/build-editor-app.sh       # macOS bundle
bash apps/editor/scripts/build-editor-desktop.sh   # Rust/wry host
```

The web build produces `apps/editor/dist/web/index.html`, a self-contained
production bundle with inline JavaScript and styles. There is no Monaco
runtime or worker payload. The macOS bundle is written to
`apps/editor/dist/GhostexEditor.app`; the Rust host and page are staged under
`apps/editor/dist/desktop/`.

## Host contract

The page boots an empty composer and posts `ready`. The host dispatches a
`ghostex-editor-host-message` custom event with a `configure` detail containing
`initialText` and an optional `cursorOffset`. The page then posts `configured`.
Each configuration starts a fresh editing session and undo history.

Messages go through the `ghostexEditorHost` WebKit message handler on macOS,
or through `window.ipc.postMessage` on wry. Save/cancel, draft/cursor updates,
and image request/reply messages keep the existing daemon protocol.

The daemon accepts `--daemon`, `--socket <path>`, and `GHOSTEX_EDITOR_SOCKET`.
`GHOSTEX_EDITOR_WEB_ROOT` can point to another built page during development.

## Memory lifetime

The macOS daemon retains open editors and one warm editor. Closed controllers
are released after native cleanup; they must never be accumulated in a retired
window list. The controller owns the NSWindow through ARC and disables
AppKit's release-on-close to avoid a second release.

Raster previews are decoded serially at a maximum of 1600 pixels. Removed
attachments release their page cache entries, and closing a preview clears
its image source. Originals on disk remain untouched.
