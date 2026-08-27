---
name: ghostex-computer-use
description: >-
  Use this skill when the user asks for Ghostex Computer Use, Desktop Control,
  or native macOS app automation. It wraps the CLI-first Trycua workflow so
  agents can drive the computer and desktop apps without the user needing to
  remember `$cua-driver`.
---

# ghostex-computer-use

Use this skill when a task needs native macOS app automation through Ghostex
Desktop Control. This skill is intentionally a wrapper around `$cua-driver`: if
the `$cua-driver` skill is available, load it for current schemas and safety
rules, but issue operations through the `cua-driver` CLI.

Use the CLI, not MCP. Do not configure, register, or invoke a Trycua MCP
server for this workflow.

Route browser work by surface:

- Use `$ghostex-browser-use` for web content in external Chrome, Chromium,
  Edge, or supported Electron apps through Trycua's typed browser tools.
- Use `$ghostex-embedded-browser-use` for browser panes built into Ghostex.
- Continue with this skill for native apps, browser chrome, native browser
  dialogs, or browser engines that do not expose an exact typed page route.

## Requirements

- Trycua must be installed. Ghostex installs it from first-launch setup or
  Settings > Integrations, where the Trycua card runs the official installer in
  a command pane.
- Trycua must be available as `cua-driver` (see Windows and WSL below).
- macOS Accessibility and Screen Recording permissions must be granted for
  Trycua.

Check the machine state before acting:

```bash
which cua-driver
cua-driver status
cua-driver check_permissions '{"prompt":false}'
```

<!--
CDXC:CuaPermissions 2026-05-29-12:21:
State checks must be read-only. The default Trycua permission check can ask macOS to show missing-grant prompts, so the Ghostex wrapper uses prompt:false and only sends users to System Settings when grants are actually missing.
-->

If the daemon is not running, start it with:

```bash
open -n -g -a CuaDriver --args serve
```

## Windows and WSL

On Windows, Trycua installs on the Windows side. A Ghostex agent session runs
inside the WSL distribution, where there is no Linux `cua-driver` binary, so
`which cua-driver` failing there does not mean Trycua is missing. Call it across
the interop boundary through `powershell.exe` instead:

```bash
powershell.exe -NoProfile -Command "cua-driver status"
powershell.exe -NoProfile -Command "cua-driver check_permissions '{\"prompt\":false}'"
powershell.exe -NoProfile -Command "cua-driver list_windows '{\"pid\":1234}'"
```

The general form is `powershell.exe -NoProfile -Command "cua-driver <tool>
'<JSON>'"`. Inside a bash double-quoted string, escape the JSON's own double
quotes as `\"`; PowerShell then hands the single-quoted JSON to Trycua as one
argument.

Trycua is a Windows process, so every path it reads or writes is a Windows path.
Translate at the boundary:

```bash
powershell.exe -NoProfile -Command "cua-driver start_recording '{\"path\":\"$(wslpath -w /tmp/run.mp4)\"}'"
wslpath -u 'C:\Users\me\Downloads\report.pdf'
```

If Trycua is not installed on the Windows side, install it from Windows
PowerShell, or from WSL through the same interop boundary:

```bash
powershell.exe -NoProfile -Command "irm https://cua.ai/driver/install.ps1 | iex"
```

## Operating Rules

- Use CLI calls: `cua-driver <tool> '<JSON>'`.
- Do not register or rely on a persistent `cua-driver mcp` server for ordinary
  Ghostex Computer Use. The CLI proxies through the signed CuaDriver.app daemon,
  while global MCP registration can leave many stdio helper processes around.
- Do not foreground the user's apps unless the user explicitly asks for that.
- Do not use `open -a`, AppleScript activation, HID event injection, `cliclick`,
  or menu-bar automation for background control.
- Do not rely on coordinates when the accessibility tree gives stable
  `element_index` values.

## Canonical Loop

1. Launch or find the app with Trycua, for example:

   ```bash
   cua-driver launch_app '{"bundle_id":"com.apple.TextEdit"}'
   ```

2. Get the window state:

   ```bash
   cua-driver get_window_state '{"pid":1234,"window_id":5678}'
   ```

3. Choose the target by `element_index` from the snapshot.
4. Perform one action such as click, type, scroll, drag, key press, or set value.
5. Re-run `get_window_state` and verify the expected UI state before continuing.

## Notes

- Keep the user's current foreground app alone. Trycua can launch and drive
  apps in the background when you identify the right app, pid, window id, and
  element indexes.
- Prefer one small verified action at a time. Re-snapshot after every stateful
  action because element indexes can change as the UI updates.
- For full command details, defer to `$cua-driver`; this wrapper exists so users
  can ask for `$ghostex-computer-use`.
- When a native-app workflow reaches supported browser page content, switch to
  `$ghostex-browser-use`; switch back here for browser chrome or native dialogs.
