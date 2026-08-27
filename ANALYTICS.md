# Analytics in Ghostex

Ghostex sends a small amount of usage data so we can tell how many people use
it, on which platforms, and which parts of the app actually get used. The data
itself is never personal — it is counts and values from fixed lists — but it is
tied to a stable, one-way hashed id, described under [Identity](#identity), so
the same person is not counted twice. This page describes exactly what that
means, what is never sent, and how to turn it off.

Analytics are on by default and can be switched off at any time. Turning them
off stops collection immediately; nothing is queued or sent later.

## Where the data goes

All analytics traffic comes from `gxserver`, the local Ghostex server process.
The desktop, web, and mobile apps never talk to an analytics service directly.
Events are sent in batches to PostHog (US cloud, `us.i.posthog.com`).

PostHog keeps a **person profile** for each `distinct_id` (see below), so these
events accumulate into a persistent per-user record rather than staying a stream
of unconnected hits. What that record contains is listed under
[Person profiles](#person-profiles). Ghostex never calls PostHog's identify or
alias APIs, so a profile is never merged with an identity from anywhere else.

## Identity

Every event carries a `distinct_id`. It exists so that one person using Ghostex
on a laptop and on a desktop counts as one user instead of two, which means it
is derived per person, not per installation.

Ghostex uses the first of these values it can read on your machine:

1. `tokens.account_id` from `~/.codex/auth.json` — your Codex account id
2. `userID` from `~/.claude.json` — your Claude Code user id
3. the random install id gxserver generated for itself on first run — the
   fallback when neither of the above is present

Whichever value is found is hashed before it is used:

```
distinct_id = sha256("ghostex-analytics-v1:" + value)
```

**Only that hash ever leaves your machine.** The underlying account id is never
sent, never logged, and never stored: Ghostex reads it, hashes it, and drops the
raw value. The hash is one-way and cannot be turned back into an account id.

The `ghostex-analytics-v1:` prefix is deliberate. Other tools hash the same
account ids without a product-specific prefix; without ours, Ghostex's
`distinct_id` for you would be byte-identical to theirs and the two records
could be matched against each other. With the prefix, the id is meaningless
outside Ghostex's own analytics.

Things worth knowing about this id:

- It is stable across reinstalls, and identical on every machine where you are
  signed in to the same agent CLI account. Only in the fallback case does
  reinstalling produce a new id.
- If you sign out of the agent CLI that produced it, Ghostex falls to the next
  source in the list and you appear as a new person from then on.
- Every event records *which* source was used, as `identity_source`
  (`codex` / `claude` / `install`). That is the name of the source only, never
  the value it produced.
- It is resolved once when gxserver starts and held in memory for that process.
  The resolved id is not written to disk.

## What is collected

Base properties attached to every event:

- Ghostex version, and whether the build is a development build
- Operating system (`macos` / `linux` / `windows`), CPU architecture, and the
  major OS version
- Country, derived by PostHog from the IP address of the request. Ghostex never
  sends your location, IP address, or any network identifier itself.
- Which identity source produced the `distinct_id` (`codex` / `claude` /
  `install`)
- A small profile of your setup, so any event can be broken down by it: whether
  your preferred interface is chat or terminal, sidebar version (v1/v2), your
  default agent CLI, and a **bucketed** project count (`0`, `1-2`, `3-5`,
  `6-10`, `10+`). The exact number of projects appears only on the daily
  heartbeat.

Events:

- `heartbeat` — sent at most once per 24 hours (see the field list below)
- `app.launched` — the desktop app started, plus its version
- `client.connected` — a web or mobile client attached to the server
- `session.started` — a session was created, with the agent CLI it uses
  (`codex`, `claude`, … — anything outside the known list is sent as `custom`)
- `prompt.sent` — a prompt was dispatched, with the agent and where it came
  from (chat, terminal, queue, quick action, board, CLI, automation). The prompt
  text itself is never included.
- `surface.opened` — which surface was opened (Agents, Code, Browser, Kanban,
  Automate, Docs, Find, Extensions store, Settings)
- `extension.installed` / `extension.uninstalled` — that an extension was
  installed or removed, and whether it came from the store or a local file.
  Never which extension.

Heartbeat fields, all counts or fixed values:

- number of projects, number of sessions, number of running sessions
- which known agent CLIs appear in your sessions, and your default agent
- whether your preferred interface is chat or terminal
- sidebar version (v1/v2) and v2 layout (flat / by project)
- number of installed extensions
- number of configured remote machines
- whole days since this installation was first set up

Every property is a number, a boolean, or a value from a fixed list. Free-form
text cannot be sent: the validation happens before anything leaves the process,
and anything that does not match the fixed list is dropped.

## Person profiles

Because every event carries the same `distinct_id`, PostHog keeps a person
record for it, and the daily heartbeat refreshes that record with the current
value of the properties above: OS, architecture, Ghostex version, country,
interface, sidebar version and layout, default agent, the project, session,
extension and remote-machine counts, days since install, and identity source.

That record persists between sessions and between machines that resolve to the
same id — that is the point of it. But it holds nothing beyond those fields: no
email, no name, no machine name, no path, no prompt, and no free-form text of
any kind, because the same validation applies to it as to events. Ghostex never
calls identify or alias, so the profile is never linked to another product's
profile or to another Ghostex user's.

Enabling profiles is one-way for a given id: once an id has sent an identified
event, PostHog treats it as identified from then on.

## What is never collected

Ghostex never sends:

- prompts, replies, or any text derived from them
- session titles, project names, worktree names, branch names
- file paths, directory names, or file contents
- URLs, hostnames, machine names, SSH or remote-machine configuration
- git identity, remotes, or commit data
- custom agent ids, custom commands, extension ids, or skill names
- environment variable values, tokens, credentials, or license keys
- email addresses, account names, or any other account details — including the
  raw agent-CLI account id the `distinct_id` is derived from, of which only the
  salted hash is ever sent
- error messages, stack traces, or crash dumps
- your IP address or location (beyond the country PostHog derives server-side)

There is no disk spool: if events cannot be sent, they are dropped.

## Turning it off

Any one of these disables analytics completely — capture stops and any pending
queue is discarded:

1. **Settings → General → Privacy → "Usage analytics"** — switch it
   off. This is stored as `analyticsEnabled: false` in Ghostex's settings file
   and applies without restarting anything.
2. **`GHOSTEX_TELEMETRY_DISABLED=1`** in the environment gxserver runs in.
3. **`DO_NOT_TRACK=1`** (the cross-application
   [Do Not Track](https://consoledonottrack.com) convention). `true` works too.

In addition, a gxserver installed as a remote helper on another machine never
sends analytics at all. The remote install records that role on the remote box
itself, so it stays silent across every later restart, reboot, and upgrade, and
a remote setup never double-counts you or reports that machine's identity.
