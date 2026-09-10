# PR #122 review and local verification

The archive-and-replay design is appropriate: neither Claude nor Codex reliably records every slash command and its output. The original implementation had correctness gaps in identity, persistence, pagination, and screen capture. Those have been corrected locally.

PR: https://github.com/maddada/Ghostex/pull/122

The PR is merged into local `main` at `32e1b8b2`. Existing uncommitted edits were preserved and byte-verified during integration. Nothing was pushed to GitHub.

## Findings fixed

- **Repeated commands and late output:** live rows now carry their archive identity. An older identical command cannot suppress a newer occurrence. Output arriving after the command-only archive row updates that row in place. Native transcript output remains authoritative.
- **Optimistic markers:** each send remembers the server command identities already visible. Retirement consumes individual new occurrences, without comparing clocks across computers.
- **Failed sends:** prepare an identity before dispatch, then persist only after successful dispatch. Remove the provisional acknowledgement on failure. Screen-output tracking stops in the serialized send queue rather than when a later request first arrives.
- **Archive integrity:** UUID identities prevent simultaneous sends from colliding. Reads, appends, output updates, and compaction share bounded lock stripes, preventing compaction from overwriting concurrent records. Archive reads run outside Tokio workers.
- **Pagination:** new records store the preceding transcript message identity. Timestamp-only bounds could drop commands in the gap between adjacent pages. Commands issued before the first transcript also replay.
- **Claude transcript overlap:** pair native envelopes with individual sends and retain their output, instead of dropping the whole archived command whenever its text appeared anywhere in the page.
- **Claude screen capture:** normalize raw captures consistently; recognize actual diff panels before removing a column; distinguish the framed composer from a picker selection and earlier command echoes. Longer completed panels can replace loading or clipped captures without letting a shorter dismissal erase a panel.
- **Text fidelity:** preserve repeated spaces in arguments, decode escaped markup based on tag attributes, and keep archived unknown-command envelopes out of skill-invocation rewriting.
- **Help:** describe persistent slash-command rows and expandable output in the Session Chat guide.

## Actual agent commands

Created dedicated Codex (`G54fk`) and Claude (`G3taf`) sessions. Both answered a short readiness prompt without tools or file edits.

| Agent | Commands exercised | Result |
| --- | --- | --- |
| Codex 0.153.4 | `/rename PR122 codex`, `/status` twice, `/mcp` | Captured rename and both separate status results; completed MCP inventory replaces its initial loading line. |
| Claude 2.1.260 | `/rename PR122 claude`, `/context`, `/status`, `/mcp` | Captured rename, context statistics, status panel, and MCP list. Live captures exposed the context-column and picker-truncation bugs fixed above. |

These commands ran through the installed Ghostex CLI into real agent terminals. Their before/after captures were processed by a standalone executable linked to the newly built gxserver library, exercising the changed extractor, archive writes, reload, and replay. This validates the changed code against real output, but it is not yet an end-to-end test of a rebuilt desktop app.

## Verification

- `cargo test --lib`: 781 passed, 3 ignored.
- `bun run test`: 1,271 passed across 138 files.
- Root, desktop, and web typechecks passed.
- Ad hoc client checks passed for repeated sends, clock-independent marker retirement, live-to-archive output handoff, whitespace, escaped attributes, unknown commands, and native compact output.
- Ad hoc Rust checks passed for pagination gaps, native-envelope output replay, and 144 concurrent unique commands with 720 output updates through compaction.
- The gxserver library and `ghostex` binary built successfully. The existing unused `HookPaths::new` warning remains.

## Remaining validation and improvements

The rebuilt desktop has not been launched. Repository and global instructions explicitly require permission before `bun run start`; approval was requested and remains pending. After approval, rebuild and restart, repeat the commands through the chat composer, and verify live rendering, expanded output, reload, and history pagination in the app.

Screen capture is inherently limited to what the agent terminal retains. A tall alternate-screen result can still show a marked partial result, as the PR explicitly specifies. A future agent-provided structured command-result API would be preferable if one becomes available.

`use-session-chat.ts` and `session_chat_queue_runtime.rs` exceed the repository's approximate 1,500-line upkeep threshold. Other sessions are actively editing this worktree, so module splitting is deferred according to the repository rule.
