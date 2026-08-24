---
name: ghostex-manage-beads
description: >-
  Use this skill whenever you are asked to work, tackle, pick up, or fix a
  Ghostex project board bead or card by id, and when managing beads with the
  normal machine-installed `bd` CLI: creating, updating, commenting on,
  reviewing, closing, or linking the session you are working in to a card. It
  covers linking this session to the bead, the project swimlane workflow,
  external refs, and safe examples for making review beads.
---

# ghostex-manage-beads

Use this skill when a user asks you to work a project board bead ("tackle bead
12345"), to manage board beads, create or update review tasks, move work through
bead statuses, add bead comments, or link a session to a card.

## Requirements

- Run bead commands from the repository root so Beads finds the project database
  and `.beads` JSONL export.
- Use the normal `bd` CLI installed in the environment running the project—
  macOS, Linux, or the selected WSL distribution.
- Prefer `bd --help` and `bd <command> --help` as the source of truth for the
  installed Beads version.
- Inspect nearby beads before creating a new one so title, labels, status, and
  external-ref style match the project.

## Working A Bead: Link This Session First

The moment you are asked to work a bead, put the session you are running in on
its card:

```bash
gx board associate <bead-id>
```

That is the whole command. It takes no session id: Ghostex reads the session you
are running in from the environment it exports into every pane, and the card
starts showing this conversation as the one working it — the same link the
board's own "Start work" button writes.

- **Do this even though nobody asked you to.** A card whose work is invisible is
  the normal failure: only work dispatched from the card links itself, so an
  agent asked by hand has to say so.
- **It creates nothing.** No session, no worker, no duplicate. Running it twice
  is safe; run it again after a fork or restore so the card follows the session
  that is really working.
- Pass `--session-id <alias|id|title>` to link a different session, and
  `--project-id <id>` only when the same bead id exists on more than one board.
- `gx board start-work <bead-id>` is a different command with a different job:
  it *dispatches* a card to a fresh worker session. Never run it for a bead you
  are working yourself — that puts a second agent on your card.

## Core Workflow

1. Inspect current work:

   ```bash
   bd list --json
   bd show <id> --json
   bd comments <id> --json
   ```

2. Move the bead through project swimlanes:

   ```bash
   bd update <id> --status in_progress
   bd update <id> --status test
   bd update <id> --status review
   bd close <id>
   ```

3. After each meaningful turn, add a short human-readable comment:

   ```bash
   bd comment <id> "<summary>"
   ```

Keep comments focused on user-facing requirements delivered and high-level
technical approach. Do not require humans to read the agent transcript to know
what changed.

## Dispatch A Bead To A Worker Session (Orchestrators)

This section is for orchestrators and dispatchers deciding who works a bead,
not for an agent that is already doing the bead's work.

To dispatch a bead through Ghostex, run:

```bash
gx board start-work <bead-id> [--agent <agentId>] [--project-id <id>] [--json]
```

- **The command is the dispatch.** It creates and starts the visible worker
  session in the bead's board project, sends the canonical bead work prompt,
  and links the conversation to the card. Call it _instead of_ launching a
  worker session yourself — it is not a preparation step before separately
  starting another worker. Calling it and then starting your own worker puts
  two workers on the same card.
- **Already working the bead? Do not call it.** An agent that is itself doing
  the bead's work must not run the command; that would create an additional
  worker for work that is already underway.
- **Repeated calls are safe.** If the bead already has a usable linked
  conversation — live, sleeping, or restorable — the command returns it with
  `{ "projectId": ..., "sessionId": ..., "created": false }` instead of creating another
  worker, so automation can call it idempotently.
- Without `--agent`, the bead's assignee is matched case-insensitively against
  the configured agents' ids and names; an assignee that matches no configured
  agent falls back to the default prompt agent.

## Create A Review Bead

Use a review bead when the implementation is ready for another pass:

```bash
bd create "Review <specific change>" \
  --type task \
  --priority P2 \
  --labels review,<area> \
  --external-ref "codex-thread:$CODEX_THREAD_ID" \
  --description "<review focus, files or areas, verification, known blockers>" \
  --json
bd update <id> --status review
```

If `CODEX_THREAD_ID` is missing, omit the external ref rather than inventing
one.

## Record Other Session Ids On A Bead

`gx board associate` owns the Ghostex link the board reads, so a comment only
has to carry what the board does not know — a Codex thread, for instance:

```bash
bd comment <id> "Codex thread ${CODEX_THREAD_ID:-unknown}. <brief work summary and verification status>."
```

Useful environment variables when present:

- `GHOSTEX_GLOBAL_SESSION_REF`: full Ghostex session reference, such as
  `S90:P3lv0:G5jjo`.
- `GHOSTEX_NATIVE_SESSION_ID`: native project/session id, such as
  `P3lv0:G5jjo`.
- `GHOSTEX_SESSION_ID`: provider session id, such as `G5jjo`.
- `CODEX_THREAD_ID`: current Codex thread id.

When creating a new bead for the current agent session, set
`--external-ref "codex-thread:$CODEX_THREAD_ID"`, and run
`gx board associate <new-id>` so the card carries the Ghostex session too.

## Example: Session-Associated Review Bead

```bash
bd create "Review companion CEF flicker layout-key fix" \
  --type task \
  --priority P2 \
  --labels cef,native-sidebar,review \
  --external-ref "codex-thread:$CODEX_THREAD_ID" \
  --description "Review the geometry-only native layout-key extraction for companion terminal focus changes. Verify focused tests, typecheck, and any known unrelated blockers." \
  --json
bd update <new-id> --status review
gx board associate <new-id>
bd comment <new-id> "Codex thread ${CODEX_THREAD_ID:-unknown}. Implemented geometry-only native layout-key extraction so companion session clicks no longer classify active-tab focus changes as AppKit layout changes; focused tests and typecheck passed."
```

## Safety

- Do not delete or close beads unless the user explicitly asks or the work is
  genuinely done.
- Do not overwrite unrelated bead descriptions or labels when a comment is
  enough.
- Keep bead comments free of secrets, command output, private file contents, and
  unnecessary paths.
