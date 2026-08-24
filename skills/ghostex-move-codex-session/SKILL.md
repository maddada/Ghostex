---
name: ghostex-move-codex-session
description: Explain how to fork an existing Codex CLI session into another folder by using `/status` to get the session id and `codex fork --yolo -C TARGET_FOLDER SESSION_ID`. Use when a user asks to move the current Codex session, change the Codex working directory, fork a Codex conversation into a different project folder, or continue a Codex thread under another workspace root as a separate session.
disable-model-invocation: true
---

# ghostex-move-codex-session

<!--
CDXC:CodexSessionMove 2026-06-26-13:18:
Users need a small bundled Ghostex skill for explaining the supported Codex session move workflow. Codex does not expose an in-place `/cd` command for changing an active running TUI workspace root, so the reliable path is to resume the saved session with `-C`.

CDXC:CodexSessionMove 2026-06-26-13:24:
Moving should create a separate Codex session instead of continuing the original session directly. Teach `codex fork --yolo -C <folder-path> <SESSION_ID>` as the primary command so the new thread inherits context, starts under the target working root, and uses full filesystem access when the user wants it.
-->

Explain the supported Codex CLI workflow in simple numbered steps.

## Steps

1. Ask the user to run this inside the current Codex session:

   ```text
   /status
   ```

2. Tell the user to copy the session id shown by `/status`.

3. Tell the user to open a shell and fork that session in the target folder:

   ```bash
   codex fork --yolo -C ~/dev/hackathon <SESSION_ID>
   ```

Replace `~/dev/hackathon` with the requested folder when the user gave a different target.

Tell the user that `--yolo` is the Codex alias for bypassing approvals and sandboxing. If they do not want full access, remove `--yolo`.

## Fallback

If the user does not want to copy the id, suggest the recent-session fallback:

```bash
codex fork --all --last --yolo -C ~/dev/hackathon
```

Explain that `--all` matters because Codex normally filters fork candidates by the current working directory.

## Rules

- Keep the answer short and procedural.
- Prefer `codex fork --yolo -C <folder-path> <SESSION_ID>` as the primary command.
- Mention that this creates a separate forked Codex session under the new working root.
- Mention that users can remove `--yolo` if they do not want full-access mode.
- Do not claim the active running TUI can be moved in place unless current Codex docs expose a real command for that.
- If the user asks for the exact command only, give only the command and one sentence.
