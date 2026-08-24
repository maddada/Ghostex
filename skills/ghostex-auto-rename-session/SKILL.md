---
name: ghostex-auto-rename-session
description: >-
  Generate a concise Ghostex session title for the current thread or
  conversation. Use when the user asks for a thread name, chat title, session
  title, resume title, or any concise label summarizing what was worked on; then
  submit the rename command in the current Ghostex session.
disable-model-invocation: true
---

# ghostex-auto-rename-session

Generate one title only.

After generating the title, submit the rename command in this same Ghostex
session:

```bash
ghostex_session_selector="${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}"
if [ -n "$ghostex_session_selector" ]; then
  ghostex send-message "$ghostex_session_selector" "/rename <generated title>"
fi
```

Use `send-message` for the current Ghostex session rename. It sends the full
`/rename <title>` command and submits Enter in one operation, so the agent
should not call `send-enter` separately after `send-message`.

Do not use `send-text` alone. If `send-message` is unavailable and `send-text`
must be used, call `send-enter` for the same selector immediately after
`send-text`.

Do not use `rename-command` as the default for this skill. It can report an
accepted renderer request without proving the command was delivered into a
hidden or unmounted current session.

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
- If `send-message` fails with no matching session, return the title only. Do
  not retry with a different selector.
