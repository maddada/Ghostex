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
- The linked session nests under its lead in the sidebar.

Links are one level deep: a lead can't be linked to another lead, and a
coordinator or a coordinator's thread can't take part (a coordinator has
threads: `ghostex coordinator start-thread`). Both sessions are on this
computer.

## Commands

```bash
ghostex agents create <agent-id> --lead self --task "..."   # start one linked to you
ghostex agents link <session-ref> --lead self               # link one that already runs
ghostex agents links                                        # the sessions linked to you
ghostex agents unlink <session-ref>                         # end one link
ghostex agents unlink --all                                 # end every link to you
```

A session can only link others to itself (`--lead self`); only the user (from
their own terminal or the app) can link a session to another lead. Send the
linked session its work with `ghostex agents send <ref> "..."` (`--queue` and
`--with-next` too): the turn that message starts is reported. Don't wait in a
loop for reports: each arrives as a message when the turn ends.

## Undoing

The user can end links from the sidebar: Unlink from Lead on a linked
session, Stop leading on the lead. An unlinked session keeps running as an
ordinary session and simply stops reporting; nothing is closed.

If the user wants a session that only delegates and follows the work, a
coordinator fits better (`ghostex coordinator promote`).
