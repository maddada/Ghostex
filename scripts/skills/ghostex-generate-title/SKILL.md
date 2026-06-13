---
name: ghostex-generate-title
description: >-
  Generate a concise Ghostex session title for the current thread or
  conversation. Use when the user asks for a thread name, chat title, session
  title, resume title, or any concise label summarizing what was worked on; then
  submit the rename command in the current Ghostex session.
---

# ghostex-generate-title

Generate one title only.

After generating the title, submit the rename command in this same Ghostex
session:

```bash
ghostex_session_selector="${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}"
if [ -n "$ghostex_session_selector" ]; then
  ghostex rename-command --session-id "$ghostex_session_selector" --title "<generated title>"
fi
```

Use `rename-command` instead of `send-text` alone. It stages `/rename <title>`
and submits Enter through Ghostex's supported session input path, so the agent
should not call `send-enter` separately.

## Rules

- Keep the title under 60 characters.
- Summarize the actual work done, not the whole conversation vibe.
- Use plain title case or compact phrase case.
- Avoid quotes, punctuation, emojis, and extra explanation.
- Use `GHOSTEX_GLOBAL_SESSION_REF` as the self-session selector when it is set.
  It is the exact `S:P:G` Ghostex session reference and avoids ambiguous bare
  session ids.
- If `GHOSTEX_GLOBAL_SESSION_REF` is missing, use `GHOSTEX_SESSION_ID` when it
  is set. Ghostex may export either a stable session id or provider persistence
  name, and the CLI resolves both directly.
- If both Ghostex selectors are missing, use `ZMX_SESSION` when it is set. zmx
  exports the provider session name for the current pane.
- If all three self-session selectors are missing, return the title only and do
  not guess a session by title, alias, project, or recent activity.
- If `rename-command` fails with no matching session, return the title only. Do
  not retry with a different selector.
