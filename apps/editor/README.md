# Ghostex Editor

Standalone Monaco editor assets for `GhostexEditor.app`.

Phase 1 builds a self-contained web page with no React or Ghostex app imports.
The page boots with an empty Monaco model, posts `ready`, and is configured by
the host later through a `ghostex-editor-host-message` CustomEvent with
`type: "configure"`. The page talks back through the `ghostexEditorHost`
WKWebView message handler, or through `window.ipc.postMessage` for wry hosts.

Build the web bundle from the repo root:

```bash
bun editor/scripts/build-editor-web.mjs
```

The output is written to `editor/dist/web/`, with Monaco's AMD runtime staged at
`editor/dist/web/monaco/vs`.

## Linux and Windows desktop host

`editor/desktop` builds the Rust `ghostex-editor` daemon host. It uses `wry`
to render the same web bundle through the OS webview backend: WebKitGTK on
Linux, WebView2 on Windows, and WKWebView on macOS for development checks.

Build and stage the current host platform from the repo root:

```bash
bash editor/scripts/build-editor-desktop.sh
```

The script rebuilds `editor/dist/web`, compiles
`editor/desktop/Cargo.toml --release`, and stages the binary plus
`editor/dist/desktop/web/`. Run the daemon as:

```bash
editor/dist/desktop/ghostex-editor --daemon
```

The daemon also accepts `--socket <path>` and honors
`GHOSTEX_EDITOR_SOCKET`; `GHOSTEX_EDITOR_WEB_ROOT` can point at an alternate
web bundle during development.
