---
name: ghostex-embedded-browser-use
description: >-
  Use this skill when an agent needs to open, inspect, automate, or debug a
  browser pane built into Ghostex through its CLI-launched CEF bridge. It
  covers pane creation and reuse, ephemeral bridge startup, CEF page selection,
  console logs, DOM snapshots, clicks, fills, key presses, evaluation, and
  screenshots. Use ghostex-browser-use instead for external browsers through
  Cua Driver's CLI.
disable-model-invocation: true
---

# ghostex-embedded-browser-use

Use Ghostex's CEF DevTools bridge for pages rendered inside Ghostex. This route
does not require a second browser automation runtime.

Enter through the `ghostex` CLI, not a configured MCP integration. Do not add a
persistent `mcp_servers.ghostex-browser` entry. The temporary stdio bridge
speaks MCP internally because that is the embedded page-control protocol, but
launch it only through `ghostex browser mcp` for the current task.

## Requirements

- Ghostex must be running before the CLI-launched bridge can attach to CEF.
- A pane can be created or reused with `ghostex browser open <url>`.
- The Ghostex CLI is bundled with the desktop app and linked on app startup.
- Install this skill with `ghostex browser install-skill` when needed.

## CLI transport

Launch the bridge directly from the CLI for the current task:

```bash
ghostex browser mcp
```

If Ghostex uses a non-default CEF remote-debugging port, pass it explicitly:

```bash
ghostex browser mcp --port 9333
```

The same value can be provided as `GHOSTEX_CEF_REMOTE_DEBUGGING_PORT`.

## Canonical loop

1. Open or reuse a suitable pane with `ghostex browser open <url>`.
2. Call `ghostex_list_pages` and select the intended pane with
   `ghostex_select_page` when multiple pages exist.
3. Read `ghostex_console_logs` before and after interactions when debugging.
4. Call `ghostex_snapshot` and choose stable `@e` element refs.
5. Perform one action with `ghostex_click`, `ghostex_fill`,
   `ghostex_press_key`, or `ghostex_navigate`.
6. Re-run `ghostex_snapshot` after navigation or a major DOM change. Verify the
   expected page state before continuing.
7. Use `ghostex_evaluate` for focused inspection and `ghostex_screenshot` when
   visual evidence matters.

## Opening panes

- `ghostex browser open <url>` defaults to the agent process cwd as the project
  path and reuses a same-origin pane in that project.
- Pass `--project-path "$PWD"` or `--project-id <id>` when the task belongs to
  a specific Ghostex project or worktree.
- Keep the returned browser session id and the MCP page id from
  `ghostex_list_pages`; reuse them instead of opening duplicate panes.
- Use `--reuse exact` for exact-URL reuse, or `--new` only when a separate pane
  is intentional.

## Boundaries

- Element refs are valid only for the current page state. Snapshot again after
  navigation or significant DOM changes.
- Console collection starts when the CLI-launched bridge attaches, so attach
  before reproducing an error when possible.
- Use `$ghostex-browser-use` for external Chrome, Chromium, Edge, or supported
  Electron page content through Cua Driver.
- Use `$ghostex-computer-use` for native macOS UI outside the embedded page.
