---
name: ghostex-cli
description: >-
  Use this skill for anything involving Ghostex or the `ghostex`/`gx` CLI:
  projects and workspaces, terminal panes and sessions, agent orchestration
  (creating agent sessions, messaging agents, reading their output, queueing
  prompts, sleeping/waking/killing sessions), scheduled automations, the
  Kanban/Beads project board, quick actions, session chat and prompt queues,
  prompt history and previous-session search/resume/fork, server management,
  UI controls, logs, screenshots, and diagnostics. It teaches help-first
  command discovery, stable target selection, JSON inspection, safe execution,
  verification, and routing to Ghostex's more specialized skills.
disable-model-invocation: true
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

Everyday Ghostex work — sessions, orchestration, automations, quick actions,
chat queues, prompt history, server, diagnostics — is covered by the CLI help
above. Route to a specialized skill only when its domain applies:

- Use `$ghostex-embedded-browser-use` for browser panes inside Ghostex.
- Use `$ghostex-browser-use` for supported external browser page content.
- Use `$ghostex-computer-use` for native desktop application control.
- Use `$ghostex-manage-beads` for Project Board bead workflows through the
  machine-installed `bd` CLI.
- Use `$ghostex-fable-56-orchestration` for the Fable-planned,
  Codex-implemented, Fable-verified multi-pane pipeline.
- Use `$ghostex-auto-rename-session` when asked to generate a session title.
- Use `$ghostex-move-codex-session` to fork a Codex session into another
  folder.

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
