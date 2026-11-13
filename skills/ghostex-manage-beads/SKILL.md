---
name: ghostex-manage-beads
description: >-
  Use this skill whenever you are asked to work, tackle, pick up, or fix a
  Ghostex project board bead or card by id, and when managing beads with the
  normal machine-installed `bd` CLI: creating, updating, commenting on,
  reviewing, closing, or linking the session you are working in to a card. It
  covers linking this session to the bead, the project swimlane workflow,
  external refs, and safe examples for making review beads.
disable-model-invocation: true
---

# ghostex-manage-beads

Use this skill when a user asks you to work a bead from the Beads view or board
("tackle bead 12345"), to manage beads, create or update review tasks, move work through
bead statuses, add bead comments, or link a session to a card.

## Requirements

- Run bead commands from inside the project. Recent `bd` versions find the
  workspace from a subfolder or a git worktree, and follow `.beads/redirect` to
  a workspace kept outside the repository. `bd where` shows the workspace your
  installed `bd` resolved.
- Use the normal `bd` CLI installed in the environment running the project—
  macOS, Linux, or the selected WSL distribution.
- Prefer `bd --help` and `bd <command> --help` as the source of truth for the
  installed Beads version.
- Inspect nearby beads before creating a new one so title, labels, status, and
  external-ref style match the project.
- If the project has no Beads workspace yet (`bd where` reports no active beads
  workspace), stop and ask the user whether they want Beads set up in this
  project. Run `bd init`, or anything else that creates `.beads` files, only
  after they say yes. When a `bd` command fails for any other reason, report
  that error instead of offering setup.

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

   A project can hide Backlog, Test or Review on its Board
   (`bd config get custom.ghostex.board_hidden_columns` lists them). Skip the
   statuses it hides; a bead moved to one anyway makes that lane show again.

3. After each meaningful turn, add a short human-readable comment:

   ```bash
   bd comment <id> "<summary>"
   ```

Keep comments focused on user-facing requirements delivered and high-level
technical approach. Do not require humans to read the agent transcript to know
what changed.

## What The Beads View Shows (And The bd Commands Behind It)

Ghostex's Beads view (the side panel tab that used to be called Kanban) shows
these live, so whatever you change with `bd` appears there within seconds. Use
the same commands; never edit Beads files by hand.

- **Ready work**: `bd ready --json` lists what can be picked up now (no open
  blockers, not deferred). Pass `--limit 0` to see more than 100.
- **Epics and children**: `bd create --title "<title>" --type epic`, then
  `bd create --title "<title>" --parent <epic-id>` for a child. A child takes
  the epic's labels unless you pass `--no-inherit-labels`. `bd epic status
  --json` shows each epic's progress.
- **What blocks what**: `bd create ... --deps blocked-by:<id>,blocked-by:<id>`
  at creation, or `bd dep add <id> <blocker-id>` later. `bd blocked --json`
  lists blocked beads and what they wait on.
- **Claim**: `bd update <id> --claim` sets you as the assignee and the status to
  in progress (it refuses a bead someone else claimed; don't force it).
- **Defer**: `bd defer <id> --until tomorrow` (or a date like `2026-12-01`, or
  `+1w`) hides a bead from ready work until then; `bd defer <id>` with no date
  puts it on ice; `bd undefer <id>` brings it back. `bd reopen <id>` reopens a
  closed bead.
- **Needs a decision**: the `human` label flags a bead for a person
  (`bd label add <id> human`, or `--labels human` at creation; `bd human list`
  shows them); it shows pinned on top of the view. People answer it with `bd human respond <id> -r
  "<answer>"` (which comments and closes it) or `bd human dismiss <id>`. Flag a
  bead this way when you need the user to choose, instead of guessing.
- **Links**: `bd update <id> --external-ref <url>` sets a bead's external link
  (an issue or page) and `--spec-id <url-or-path>` its spec link; an empty value
  clears it. The view's Links row shows and edits both. Beads has no file
  attachments, so link to files instead.
- **Sync**: `bd dolt pull` brings in changes from the user's other computers
  and `bd dolt push --remote <name>` sends this computer's (the view names the
  workspace's own Dolt remote). Only sync when the user
  asks; a rejected push means pull first, then push again. When a Pull or Push
  from the view failed, the gxserver log (`gxserver.jsonl`) has a
  `beadsView.syncFailed` line with the kind, code and bd's first error line.

## Dispatch A Bead To A Worker Session (Coordinators)

This section is for coordinators and dispatchers deciding who works a bead,
not for an agent that is already doing the bead's work.

To dispatch a bead through Ghostex, run:

```bash
gx board start-work <bead-id> [--agent <agentId>] [--model <model>] [--effort <level>] [--project-path <path>|--project-id <id>] [--json]
```

- **The command is the dispatch.** It creates and starts the visible worker
  session, sends the canonical bead work prompt, and links the conversation to
  the card. Pass `--project-path <repo>` (or `--project-id`) so the worker
  starts in the project the card is about; without it the worker starts in the
  project whose own path is the Beads directory — the board itself — never in
  a sibling project that merely mounts the same board. Call it _instead of_ launching a
  worker session yourself — it is not a preparation step before separately
  starting another worker. Calling it and then starting your own worker puts
  two workers on the same card.
- **Pick the worker's model with `--model` and `--effort`** (Claude, Codex, Pi, OMP,
  Copilot, Grok Build, Hermes Agent, Empryo and Antigravity CLI workers, plus `--model` alone for Cursor CLI
  and Gemini, when `start-work` creates a new worker; a Pi or
  Empryo model is `<provider>/<model>`, a Pi effort is a thinking level such as `high`, and
  Ghostex releases before October 2026 refuse a Pi model). A reused linked worker
  keeps its existing model and effort. The choice applies to the new worker
  session only, a resume keeps it, and the user's default model is untouched.
  Ghostex releases whose
  `gx board --help` does not list them ignore the flags, and the worker starts
  on the default model.
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
