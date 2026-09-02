# Agent model catalog: how to update it and how to re-collect it

`agent-model-catalog.json` at the repository root lists, for every agent CLI
Ghostex drives (Claude Code, Codex, Cursor CLI, Grok Build), the models its
`/model` picker offers, the effort levels each model accepts, and whether the
CLI has a fast mode. It is published from `main` at

    https://raw.githubusercontent.com/maddada/Ghostex/main/agent-model-catalog.json

and every Ghostex client (desktop CEF chat view, web app, mobile embedded chat)
renders the chat composer's model dropdown and effort selector from it, so a
CLI shipping a new model needs this file edited and pushed, not an app release.

Read this whole page before touching the file.

## How clients load it

1. The same JSON is bundled with every build (imported at build time by
   `packages/shared/agent-model-catalog-store.ts`), so the dropdown is complete
   offline and on first launch.
2. On each page load the client fetches the GitHub copy once. If it is
   reachable and parses, it is the source of truth: it replaces what is showing
   and is cached in localStorage.
3. If the fetch fails, the newer (by `updatedAt`) of the bundled and cached
   copies stays in effect.

The schema, the validator, and the label truncation live in
`packages/shared/agent-model-catalog.ts`. The per-agent option descriptors
(what each dropdown row types into the TUI) are built from the catalog in
`packages/core-ui/chat/session-chat-session-options.ts`; that module never
names a model or an effort level itself.

## Schema (schemaVersion 1)

Top level:

| Field           | Meaning                                                                                                  |
| --------------- | -------------------------------------------------------------------------------------------------------- |
| `schemaVersion` | Must be `1`. A document with another version is rejected by every client, so never bump it casually.    |
| `updatedAt`     | ISO date. Bump it on every edit; it decides between the bundled and cached copies.                       |
| `effortLabels`  | Effort id to display label (`xhigh` to "Extra high"). Add an entry for every effort id used below.       |
| `agents`        | Keyed by Ghostex agent id: `claude`, `codex`, `cursor`, `grok`. Other keys are ignored.                  |

Per agent (`agents.<id>`):

| Field           | Meaning                                                                                        |
| --------------- | ---------------------------------------------------------------------------------------------- |
| `name`          | Display name of the CLI.                                                                       |
| `efforts`       | Every effort id the agent knows, lowest first. Used for a model the catalog does not list.     |
| `defaultEffort` | Optional.                                                                                      |
| `fastMode`      | `{ available, command, scope }`. `scope` is `"model"` or `"session"`.                          |
| `models`        | Ordered list of rows; the dropdown shows them in this order.                                   |

Per model (`agents.<id>.models[]`):

| Field           | Meaning                                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `value`         | Stable id. It is what gets typed (`/model <value>` for Claude), persisted, and matched against gxserver's detection.      |
| `label`         | What the pill and dropdown show. Keep it short; anything over 20 characters is cut with an ellipsis in the pill.           |
| `pickerLabel`   | The CLI's literal row text when it differs from `label`. Cursor's `/model` filter needs "Cursor Grok 4.6", not "Grok 4.6". |
| `description`   | Optional muted line under the row.                                                                                        |
| `efforts`       | Effort ids this model accepts; `[]` means the model has no effort pill. Omit to inherit the agent list.                   |
| `defaultEffort` | Optional.                                                                                                                 |
| `fastMode`      | Whether this model can run in fast mode.                                                                                  |
| `default`       | Optional; marks the CLI's own default row.                                                                                |

Every other field (`cliVersion`, `modelCommand`, `notes`, `contextWindows`,
`thinking`, `_readme`) is documentation for humans and is ignored by clients.

Label conventions already applied, keep them:

- Codex ids read as words: "GPT 5.6 Sol", "GPT 5.4 Mini", "GPT 5.3 Codex Spark".
- Cursor rows drop the "Claude" and "Cursor" words ("Opus 5", "Grok 4.6") and
  keep the literal row text in `pickerLabel`.
- Claude's "Default (recommended)" row is Opus 5 with 1M context, the same
  model as the `opus` row, so it is not listed twice. Haiku 4.5 is given
  `efforts: []`.

## Editing the file

1. Edit `agent-model-catalog.json` at the repo root.
2. Bump `updatedAt`.
3. Validate: `bun run test -- packages/core-ui/chat/session-chat-session-options.test.ts`
   and `bun run typecheck` (the bundled import fails the build if the document
   does not parse).
4. If a `value` was added or renamed, update the matching detection table in
   `server/src/session_chat_options.rs` (`CLAUDE_MODEL_FAMILIES`,
   `CLAUDE_EFFORTS`, `CODEX_EFFORTS`, `CURSOR_MODEL_LABELS`, `GROK_EFFORTS`)
   so gxserver keeps mapping what the TUI prints onto the new value, then run
   `cargo check` from inside `server/`.
5. Commit and push to `main`. Clients pick the change up on their next page
   load; a new build also carries it bundled.

## Re-collecting the data from the CLIs

Do this whenever a CLI update changes its picker. Everything is read from the
real TUIs and driven through the Ghostex CLI, so it is reproducible and leaves
the shared checkout alone. The 2026-09-02 run, with raw transcripts and the
walk scripts, is in `docs/2026-09-02/agent-model-catalog/` (local, `docs/` is
gitignored).

### 1. Scratch sessions

Use a throwaway folder so the CLIs' trust prompts and any stray prompt never
touch a real project. `ghostex terminal` only registers a card; use
`create-session --start` so the pty exists for headless driving.

```sh
mkdir -p /tmp/model-inventory && git -C /tmp/model-inventory init -q
gx terminal --cwd /tmp/model-inventory --title seed -- true   # registers the project; note its projectId
gx create-session inv-claude --start --project-id <projectId>
gx send-message inv-claude "claude"
# same for inv-codex ("codex"), inv-cursor ("cursor-agent"), inv-grok ("grok")
```

Answer the first-run trust prompts: Claude `send-key arrow-down` then
`send-enter`; Codex `send-enter`; Cursor `send-text a`; Grok has none.

### 2. Model list

```sh
gx send-text inv-claude "/model"; gx send-enter inv-claude
gx read-text inv-claude --visible
```

- Claude shows three rows with a "… +N models" footer: press `arrow-down`
  until every row has been seen. The effort row under the list is driven by
  `arrow-left` / `arrow-right` and wraps; `/effort` opens the same scale.
- Codex opens with the CURRENT model highlighted (`›`), so navigate relative
  to that marker. `Enter` on a model opens its reasoning list; a "More
  reasoning…" row opens the Max / Ultra submenu. Confirming a row writes
  `~/.codex/config.toml`, so note the banner's model and effort first and
  restore them at the end (or always leave with `escape`).
- Cursor shows ten rows with an "a-b of 35" counter; the list wraps at both
  ends. Rows ending in "(Tab to modify)" open an "Edit Parameters" panel with
  Context / Effort or Reasoning / Thinking / Fast; `escape` closes the panel,
  and `escape` on a row without a panel closes the whole picker. Cross-check
  with `cursor-agent --list-models`.
- Grok: `Enter` on a model shows its effort rows; the composer placeholder
  documents the direct form `/model <model> [effort]`. `grok models` lists ids.

### 3. Fast mode

Type `/` and read the command list, then run the command on a scratch session:

- Claude: `/fast` says "Toggle fast mode (Opus 5)" and needs usage credits.
- Codex: `/fast` toggles "Service tier set to priority / default"; the footer
  appends "fast".
- Cursor: the "Fast" checkbox in the Tab panel; `--list-models` mirrors it as
  `-fast` ids.
- Grok: no fast entry.

### 4. Gotchas

- `gx send-key` accepts `arrow-up`, `arrow-down`, `arrow-left`, `arrow-right`,
  `escape`, `tab`, `ctrl-c`; not `down`, `ctrl-u`, `ctrl-a`, `ctrl-k`.
- `escape` does not clear a half-typed slash command in Codex or Grok; the
  leftover text is sent as a prompt on the next Enter, and Codex then renames
  its thread (Ghostex renames the card to match). Address sessions by id, or
  start a fresh one.
- Kill the scratch sessions (`gx kill <title>`) and remove the scratch project
  when done. Never run this against a real project's sessions.

### 5. Write it back

Fill the JSON from what the TUIs showed, not from memory or docs. Keep the
`value` of an existing row stable unless the CLI itself renamed the model:
persisted sessions and gxserver's detection both key on it. Then follow
"Editing the file" above.
