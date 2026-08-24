---
name: ghostex-find-prev-session
description: Find, inspect, resume, or fork a previous AI-agent session with Ghostex's bundled Zehn history search. Use when a user asks to find an earlier thread or conversation, remembers part of an old prompt, wants to resume past work, or wants to reuse a previous prompt across Claude Code, Codex, Pi, OpenCode, Cursor Agent, or Grok.
---

# Ghostex Find Previous Session

Use `ghostex find` (alias `gx find` or `gx f`) so the search runs through
Ghostex's bundled Zehn binary and its supported transcript stores.

## Supported agents

| Agent        | History source                        | Inactive-session resume command |
| ------------ | ------------------------------------- | ------------------------------- |
| Claude Code  | `~/.claude`                           | `claude --resume <id>`          |
| Codex        | `~/.codex`                            | `codex resume <id>`             |
| Pi           | `~/.pi`                               | `pi --session <id>`             |
| OpenCode     | `~/.local/share/opencode/opencode.db` | `opencode --session <id>`       |
| Cursor Agent | `~/.cursor/projects`                  | `cursor-agent --resume <id>`    |
| Grok         | `~/.grok/sessions`                    | `grok --resume <id>`            |

OpenCode history requires the `sqlite3` CLI. Zehn reports and skips OpenCode
history when that database exists but `sqlite3` is unavailable.

## Workflow

1. Read the installed command contract before searching:

   ```bash
   ghostex find --help
   ```

2. Use the most distinctive phrase the user remembers. Prefer an error string,
   filename, feature name, or quoted fragment over a broad word.

3. If the source agent is known, restrict the picker:

   ```bash
   ghostex find --agent codex
   ```

   Accepted filters are `claude`, `codex`, `pi`, `opencode`, `cursor`, and
   `grok`. Omit `--agent` to search every supported store.

4. In the Zehn picker, type the phrase to filter. Use Up/Down or Ctrl+P/Ctrl+N
   to select a result. Ctrl+R filters projects, Ctrl+T filters agents, Ctrl+D
   changes day grouping, and Page Up/Page Down moves between days.

5. Match the user's intent:

   - To resume or continue the session, press Enter. If that agent conversation
     already owns a live Ghostex session, Zehn asks Ghostex to focus its
     existing pane and does not start another writer. Otherwise, Zehn starts
     the correct agent in the recorded project directory.
   - To identify the result without resuming, launch with `--project`, select
     the result, and report the returned agent, project, and prompt snippet:

     ```bash
     ghostex find --project --agent codex
     ```

   - To print only the selected prompt, use `--print`.
   - To copy the selected prompt, press Ctrl+Y.
   - To inspect the full prompt in `$EDITOR`, press Ctrl+E.
   - To reuse the prompt in a new session, press Ctrl+O and select the target
     agent. Only fork when the user asks to create a new session.

## Non-interactive shortlisting

Use `--list` only to narrow a large history set before opening the picker:

```bash
ghostex find --agent codex --list | rg -i --fixed-strings "distinctive phrase"
```

The list output contains the agent, project, and prompt text, but not the
session id. Do not invent a resume command from `--list`; return to the Zehn
picker and let it resume the selected record.

## Live-session handoff

When Zehn is launched through `ghostex find`, Ghostex passes its exact CLI
executable to Zehn. On Enter, Zehn calls the CLI with the selected conversation
id and agent id. Ghostex resolves that identity against its live session list
and sends the existing focus request to the desktop app. A distinct
"not running" result permits the normal agent resume command; a focus or
control-plane error must not fall through to resume because that could create a
second writer for the same conversation.

If a user reports `already has an active writer`, do not retry the provider's
resume command directly. Reopen the result through `ghostex find`; if the
handoff still fails, report the focus error and inspect `ghostex sessions
--json` for the selected `agentSessionId` before changing any session state.

## Rules

- Use a PTY or visible Ghostex terminal for the interactive picker.
- Do not press Enter unless the user asked to resume or continue the session.
- Do not bypass Zehn with a provider resume command merely because the matching
  conversation is already open; let Ghostex focus its existing owner.
- Confirm close matches from their prompt text and project before choosing.
- Prefer the newest matching session unless the user specifies another date.
- Do not run `zehn update` unless the user explicitly asks to update Zehn.
- If no result matches, report the searched phrase, agent filter, and stores;
  then ask for a more distinctive phrase instead of choosing an unrelated row.
