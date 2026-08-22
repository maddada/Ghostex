# Ghostex GPUI

This folder contains the GPUI prototype for Ghostex.

The current app focuses on macOS while keeping the browser host behind a platform module:

- left side: the existing Ghostex React sidebar rendered inside CEF
- main area: a CEF browser child view with a GPUI/gpui-component address bar
- runtime: a local macOS `.app` bundle because CEF needs its framework and helper apps under `Contents/Frameworks`

Build the macOS app:

```bash
gpui/scripts/build-macos-app.sh
```

Run it after building:

```bash
open -n gpui/build/macos/GhostexGPUI.app
```

Or build and launch in one command:

```bash
gpui/scripts/build-macos-app.sh --run
```
