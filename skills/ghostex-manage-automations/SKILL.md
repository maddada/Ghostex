---
name: ghostex-manage-automations
description: >-
  Use this skill when managing Ghostex scheduled project automations through
  the `ghostex` or `gx` CLI: inspecting definitions and run history, creating
  or updating schedules, running an automation immediately, pausing or
  resuming it, marking runs read, or archiving and deleting automation data.
---

# ghostex-manage-automations

Manage gxserver-owned project automations through the Ghostex CLI. Do not
confuse these scheduled tasks with per-session `delayed-send` or
`close-after-done` controls.

## Requirements

- Run `ghostex automations --help` before choosing commands or definition JSON.
- Identify the target project by exact path or id from `ghostex sessions --json`
  or `ghostex state`.
- Read current automation state before every update or destructive operation.

## Core Workflow

1. Inspect definitions and run history:

   ```bash
   ghostex automation-state --path <project-path>
   ```

2. Create a disabled automation first unless the user explicitly wants it
   scheduled immediately:

   ```bash
   ghostex automation-save --path <project-path> --definition-json '{"name":"Daily review","agentId":"codex","prompt":"Review the project and report actionable findings.","enabled":false,"schedule":{"kind":"daily","time":"09:00","timezone":"local"},"executionMode":{"kind":"local"}}'
   ```

3. Verify the saved definition with `automation-state`, then enable or run it:

   ```bash
   ghostex automation-set-enabled <automation-id> true --path <project-path>
   ghostex automation-run-now <automation-id> --path <project-path>
   ```

4. Re-read `automation-state`. For run-now, identify the newest run whose
   `automationId` matches and monitor it until it leaves `queued` or `running`.

## Updates

Start from the definition returned by `automation-state`. Preserve its `id`,
`createdAt`, project selection, and fields the user did not ask to change, then
pass the complete edited object to `automation-save`. Do not reconstruct an
existing definition from memory.

Use the schedule and execution-mode shapes shown by
`ghostex automations --help`. Thread mode requires an exact existing session
id; worktree mode requires a project that can create Git worktrees.

## Run Maintenance

```bash
ghostex automation-mark-run-read --run-id <run-id> --path <project-path>
ghostex automation-archive-run --run-id <run-id> --path <project-path>
ghostex automation-archive-run --run-id <run-id> --path <project-path> --remove-worktree true
ghostex automation-delete <automation-id> --path <project-path>
```

Use `--remove-worktree true` only when the user wants the automation worktree
removed and the returned run identifies that exact worktree.

## Safety

- Inspect the exact automation or run id before changing it.
- Do not enable a newly created schedule unless requested.
- Do not delete an automation or remove a run worktree without clear user
  intent.
- Verify every mutation by reading automation state again.
- Keep prompts and definition JSON free of secrets because command arguments
  may be visible to other local processes.
