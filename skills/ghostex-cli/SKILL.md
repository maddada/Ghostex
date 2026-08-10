---
name: ghostex-cli
description: >-
  Use this skill when operating Ghostex through the `ghostex` or `gx` CLI for
  workspace, project, session, quick-action, automation, server, UI, or
  diagnostic tasks. It teaches help-first command discovery, stable target
  selection, JSON inspection, safe execution, verification, and routing to
  Ghostex's more specialized skills.
---

# ghostex-cli

Use Ghostex's CLI help as the source of truth instead of relying on remembered
command names or flags.

## Core Workflow

1. Read the current command catalog:

   ```bash
   ghostex --help
   ```

2. Read focused help when the catalog points to it:

   ```bash
   ghostex automations --help
   ghostex quick-actions --help
   ghostex browser --help
   ghostex server --help
   ghostex manage-beads --help
   ghostex agent-orchestration --help
   ```

3. Inspect current state before choosing a target. Prefer JSON where offered:

   ```bash
   ghostex sessions --json
   ghostex state
   ```

4. Act on an exact project, session, group, automation, or run id. Prefer ids
   from JSON output over titles, aliases, or whichever project is focused.
5. Re-read the relevant state or use the command's assertion/wait operation to
   verify the requested result.

## Routing

- Use `$ghostex-manage-automations` for scheduled project automations.
- Use `$ghostex-agent-orchestration` for coordinating panes or agent sessions.
- Use `$ghostex-manage-beads` for Project Board bead workflows.
- Use `$ghostex-embedded-browser-use` for browser panes inside Ghostex.
- Use `$ghostex-browser-use` for supported external browser page content.
- Use `$ghostex-computer-use` for native desktop application control.

Keep using this skill's inspect, act, and verify loop alongside the specialized
workflow.

## Safety

- Do not guess a command, selector, flag, or JSON shape when current help can
  provide it.
- Inspect exact targets before delete, kill, archive, replacement, or bulk
  operations.
- Use `--token-stdin` for temporary remote tokens; do not put secrets in argv.
- Do not use raw zmx or tmux commands when Ghostex exposes the operation.
- Do not treat a successful process exit as verification when state can be
  checked directly.
