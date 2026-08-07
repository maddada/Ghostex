---
name: ghostex-manage-beads
description: >-
  Use this skill when managing Ghostex project board beads with Ghostex's
  bundled `gx bd` Beads CLI wrapper:
  creating, updating, commenting on, reviewing, closing, or associating beads
  with the current Ghostex or Codex session. It covers the project swimlane
  workflow, session-link comments, external refs, and safe examples for making
  review beads like the current session association workflow.
---

# ghostex-manage-beads

Use this skill when a user asks to manage project board beads, create or update
review tasks, move work through bead statuses, add bead comments, or associate a
bead with the current Ghostex or Codex session.

## Requirements

- Run bead commands from the repository root so Beads finds the project database
  and `.beads` JSONL export.
- Use `gx bd`, not raw `bd`, so commands run through Ghostex's bundled Beads
  binary instead of a shell-installed version.
- Prefer `gx bd --help` and `gx bd <command> --help` as the source of truth for
  the bundled Beads version.
- Inspect nearby beads before creating a new one so title, labels, status, and
  external-ref style match the project.

## Core Workflow

1. Inspect current work:

   ```bash
   gx bd list --json
   gx bd show <id> --json
   gx bd comments <id> --json
   ```

2. Move the bead through project swimlanes:

   ```bash
   gx bd update <id> --status in_progress
   gx bd update <id> --status test
   gx bd update <id> --status review
   gx bd close <id>
   ```

3. After each meaningful turn, add a short human-readable comment:

   ```bash
   gx bd comment <id> "<summary>"
   ```

Keep comments focused on user-facing requirements delivered and high-level
technical approach. Do not require humans to read the agent transcript to know
what changed.

## Dispatch A Bead To A Worker Session (Orchestrators)

This section is for orchestrators and dispatchers deciding who works a bead,
not for an agent that is already doing the bead's work.

To dispatch a bead through Ghostex, run:

```bash
ghostex board start-work <bead-id> [--agent <agentId>] [--project-id <id>] [--json]
```

- **The command is the dispatch.** It creates and starts the visible worker
  session in the bead's board project, sends the canonical bead work prompt,
  and links the conversation to the card. Call it *instead of* launching a
  worker session yourself — it is not a preparation step before separately
  starting another worker. Calling it and then starting your own worker puts
  two workers on the same card.
- **Already working the bead? Do not call it.** An agent that is itself doing
  the bead's work must not run the command; that would create an additional
  worker for work that is already underway.
- **Repeated calls are safe.** If the bead already has a usable linked
  conversation — live, sleeping, or restorable — the command returns it with
  `{ "sessionId": ..., "created": false }` instead of creating another
  worker, so automation can call it idempotently.
- Without `--agent`, the bead's assignee is matched case-insensitively against
  the configured agents' ids and names; an assignee that matches no configured
  agent falls back to the default prompt agent.

## Create A Review Bead

Use a review bead when the implementation is ready for another pass:

```bash
gx bd create "Review <specific change>" \
  --type task \
  --priority P2 \
  --labels review,<area> \
  --external-ref "codex-thread:$CODEX_THREAD_ID" \
  --description "<review focus, files or areas, verification, known blockers>" \
  --json
gx bd update <id> --status review
```

If `CODEX_THREAD_ID` is missing, omit the external ref rather than inventing
one.

## Associate A Bead With The Current Session

Prefer a bead comment for full session association because `external-ref` holds
one stable reference and comments can include both Ghostex and Codex ids:

```bash
gx bd comment <id> "Associated session: Ghostex ${GHOSTEX_GLOBAL_SESSION_REF:-unknown} / ${GHOSTEX_NATIVE_SESSION_ID:-unknown}, Codex thread ${CODEX_THREAD_ID:-unknown}. <brief work summary and verification status>."
```

Useful environment variables when present:

- `GHOSTEX_GLOBAL_SESSION_REF`: full Ghostex session reference, such as
  `S90:P3lv0:G5jjo`.
- `GHOSTEX_NATIVE_SESSION_ID`: native project/session id, such as
  `P3lv0:G5jjo`.
- `GHOSTEX_SESSION_ID`: provider session id, such as `G5jjo`.
- `CODEX_THREAD_ID`: current Codex thread id.

When creating a new bead for the current agent session, set
`--external-ref "codex-thread:$CODEX_THREAD_ID"` and add the Ghostex session ids
in a comment.

## Example: Session-Associated Review Bead

```bash
gx bd create "Review companion CEF flicker layout-key fix" \
  --type task \
  --priority P2 \
  --labels cef,native-sidebar,review \
  --external-ref "codex-thread:$CODEX_THREAD_ID" \
  --description "Review the geometry-only native layout-key extraction for companion terminal focus changes. Verify focused tests, typecheck, and any known unrelated blockers." \
  --json
gx bd update <new-id> --status review
gx bd comment <new-id> "Associated session: Ghostex ${GHOSTEX_GLOBAL_SESSION_REF:-unknown} / ${GHOSTEX_NATIVE_SESSION_ID:-unknown}, Codex thread ${CODEX_THREAD_ID:-unknown}. Implemented geometry-only native layout-key extraction so companion session clicks no longer classify active-tab focus changes as AppKit layout changes; focused tests and typecheck passed."
```

## Safety

- Do not delete or close beads unless the user explicitly asks or the work is
  genuinely done.
- Do not overwrite unrelated bead descriptions or labels when a comment is
  enough.
- Keep bead comments free of secrets, command output, private file contents, and
  unnecessary paths.
