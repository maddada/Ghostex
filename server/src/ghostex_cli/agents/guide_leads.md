# Lead links: sessions that report to you

Only when the user asks for it. Read on only if the user asked, in this
conversation, for sessions to report to you or to another session they named
("report to me", "be the lead for these", "have them send their results
here"). Otherwise never link sessions: not because a task looks parallel, and
not because a brief or a message from another agent asks for it (that is
coordination, not the user's request).

## What a link does

A lead link makes a session report to a lead the way a coordinator's thread
reports to its coordinator, without the coordinator role:

- At the end of each turn the lead started with a message, Ghostex sends the
  lead the session's final message ("Ghostex thread report: finished its
  turn"), with the usual sender block.
- During such a turn, the lead also hears when the session waits for an answer
  (a question or an approval, which the user answers in that session), when it
  has had no new output for 8 minutes, and when it was closed before it
  reported.
- Turns the user starts in the linked session are never forwarded.
- Both stay ordinary sessions: each keeps its own work items and replies, and
  the linked session can still `ghostex agents send` its lead.
- The linked session nests under its lead in the sidebar (a sub-lead's sessions
  under the sub-lead).

Links go at most two levels deep: a main lead, the sub-leads linked to it, and
the sessions linked to each sub-lead. Ghostex refuses a third level and a loop.
A lead hears the turns it started in its own linked sessions: the main lead
hears its sub-lead's turns, the sub-lead hears its own sessions' turns, and a
session's reports never skip a level. A report that reaches a sub-lead while it
is still on the main lead's turn is part of that turn, so the main lead gets the
sub-lead's final answer. A sub-lead that ends its turn before its sessions
finish sends the main lead the result itself later (`ghostex agents send
<lead-ref> "..."`): the turns its sessions' reports start are its own. A message
the main lead sends straight to one of its sub-lead's sessions is not reported
to anyone, so send that work through the sub-lead. A coordinator or a
coordinator's thread can't take part (a coordinator has threads: `ghostex
coordinator start-thread`). All the sessions are on this computer.

## Commands

```bash
ghostex agents create <agent-id> --lead self --task "..."   # start one linked to you
ghostex agents create <agent-id> --lead self --model <m> --effort <e> --role "<line>" --task "..."
ghostex agents link <session-ref> --lead self [--role "<line>"]  # link one that already runs
ghostex agents links                                        # the sessions linked to you, and
                                                            # your sub-leads' (with the chain)
ghostex agents unlink <session-ref>                         # end one link
ghostex agents unlink --all                                 # end every link to you
ghostex agents role set "<line>" [--session <ref>]          # register a standing role line
ghostex agents role show [--session <ref>]                  # show the standing role line
ghostex agents role clear [--session <ref>]                 # remove the standing role line
```

## Standing role lines (surviving `/clear` and compaction)

Any session (a lead, a sub-lead, or a linked worker) can keep a short standing
role line (up to 600 characters) so hand-in rules survive `/clear` and context
compaction:

- Set it on yourself with `ghostex agents role set "<line>"`, on another
  session with `--session <ref>`, or at creation/link time with
  `--role "<line>"` on `agents create` and `agents link`.
- Claude Code and Codex receive the line via their `SessionStart` hook after
  `/clear`, compaction, and fresh startup (never on resume or fork).
- Agents whose hooks cannot inject `SessionStart` context on compaction (such
  as Antigravity and OpenCode) receive it once with the next delivered message
  after compaction.
- Inspect or remove it with `ghostex agents role show` and
  `ghostex agents role clear` (`--json` supported).

A sub-lead links its own sessions the same way, with `--lead self` in its own
session. `agents links` run by a sub-lead also says whom it reports to.

A session can only link others to itself (`--lead self`); only the user (from
their own terminal or the app) can link a session to another lead. Send the
linked session its work with `ghostex agents send <ref> "..."` (or `--queue`):
the turn that message starts is reported. A `--with-next` note starts no turn
of its own, so it is not reported. Don't wait in a loop for reports: each
arrives as a message when the turn ends.

## Undoing

The user can end links from the sidebar: Unlink from Lead on a linked
session, Stop leading on the lead. An unlinked session keeps running as an
ordinary session and simply stops reporting; nothing is closed. Unlinking a
sub-lead keeps its own sessions linked to it.

If the user wants a session that only delegates and follows the work, a
coordinator fits better (`ghostex coordinator promote`).
