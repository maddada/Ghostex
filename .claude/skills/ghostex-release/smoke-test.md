# Pre-release macOS smoke test (runbook)

Run from step 3a of [SKILL.md](SKILL.md), before `release:actions` is dispatched.
Written 2026-11-17 from the user's picks on the checklist page
(`docs/2026-11-17/smoke-test/index.html`, Q3734 to Q3736). The page shows the
checklist; this file is what a thread follows.

Nothing here has been run yet. The first real run is a trial: where a selector,
a column or a command below turns out wrong, fix it in this file in the
after-release commit (see "Keeping the list").

## The rules (the user's decisions, 2026-11-17)

- **An agent runs every step**, including the two that only need eyes (1.4 and
  5.3), driving the GUI with the `cua-driver` skill. The user does none of them
  ("no man i won't do 1.4 5.3, you do those too using /cua-driver").
- **The agent may restart the app itself** ("u can restart fam"). Sessions live
  in zmx, so they survive an app restart. This overrides AGENTS.md's "never run
  `cargo xtask start`" for this run only; say so in the report.
- **Background first, foreground only where needed** (user, 2026-11-17: "go
  with bg tools first, use foreground for what needs foreground tools").
  Drive every step with background tools. Only a part that background input
  can't reach (a field it can't focus, like 5.3's text box) may use
  foreground input: bring Ghostex to the front, act, then put the frontmost
  app, the pointer and the open chat back as they were. Say in the report
  which steps used foreground input.
- **Stop or warn (Q3736).** Only a crash or a failure of typing stops the
  release. Every other failed step is a warning in the report (see the "On
  failure" column below). A step that is marked Stop stops the release only for
  the part named there.
- **Crash checks stay for three releases (Q3734)**, then are dropped. See
  "Keeping the list".
- **A fix means a whole new run.** If anything stops the release: the release
  stops, the fault is fixed and pushed, the app is rebuilt, and all 16 steps run
  again. Warnings alone never stop it, and don't need a rerun.

## Where, when, who

- **Where:** the user's Mac, in the real Ghostex (it has the real accounts, so
  Claude and Codex both work). The app is rebuilt from the release commit, the
  one that `release:actions` will tag, so it must already be committed and
  pushed (step 2 and 3 of SKILL.md are done: `HEAD == origin/main`).
- **Who:** one thread, started by the Main Coordinator. Not the coordinator
  itself.
- **How long:** about 15 minutes after the build (the build adds about 1 to 6).

## Before the first step

1. `git rev-parse HEAD` is the release point. Write it in the report.
2. Rebuild and relaunch the app from it. In the background:
   `cargo xtask rebuild` (committed code only, log under
   `~/.local/share/agent-work/Ghostex/rebuilds/`). It is a fast dev build
   (opt-level 0), not the signed release build; say so in the report. Add
   `--optimized` (check `cargo xtask help` for how `rebuild` passes it) only
   when the user asks for a build closer to what ships. If it exits 3 (can't
   sign), run it from a shell started since the last login, or ask the user.
3. Record the baseline for crash detection:
   `ls ~/.local/state/ghostex/logs/crashes > $W/crashes-before.txt`,
   `wc -l ~/.local/state/ghostex/logs/gpui-crash-reports.log` and
   `ls ~/Library/Logs/DiagnosticReports | grep -i ghostex`, and (from 11.9.0)
   `ls -la ~/.local/state/ghostex/logs/crash-reports/`. `$W` is
   `~/.local/share/agent-work/Ghostex/smoke/<release-commit-short>-run<N>/`.
   The dev build carries no commit stamp and still reports the previous version
   (the version bump is the prepare commit), so match the release point by the
   rebuild log's start time against the commit time.
4. Resolve your project: `ghostex agents whoami --json` (project id `$P`).
5. Name every session you create `smoke-<step>-run<N>`. Clean up at the end.
6. Look at what the cua-driver skill says first (`list_windows({app:"Ghostex"})`
   gives the `pid` and `window_id`; read narrowly with `get_window_state` and a
   `query`; act with `run_steps`; check with `verify_state`). Work in the
   background. Save a screenshot per area into `$W`.
   - GPUI stops painting a window that isn't on screen (another Space, Stage
     Manager), and background clicks then do nothing. Put Ghostex on a SpaceO
     display: make another app frontmost (SpaceO refuses while Ghostex owns the
     menu bar), `spaceo_session_create` (preset `exclusive`), quit Ghostex with
     `spaceo_menu ["Ghostex","Quit Ghostex"]`, start it with cua-driver
     `launch_app({name:"/Applications/Ghostex.app"})` (`spaceo_open_app` fails
     with "browser did not publish its private DevTools endpoint" and kills the
     launch), then `spaceo_adopt_app(pid)`. Drive it with `spaceo_*` tools.
   - Background clicks press buttons and rows but never move keyboard focus
     into a text input (chat box, Settings fields, the browser address bar, web
     content). Put focus where you type with `ghostex focus <ref>` (chat box or
     terminal) or by opening a surface that focuses its own box (Settings
     search, the Search palette, a page with autofocus). Foreground input
     doesn't help on a SpaceO display: SpaceO hands activation straight back,
     and it keeps doing so while any SpaceO session still holds Ghostex, so
     end the SpaceO session (`spaceo_session_destroy`) before using foreground
     input on the main display. cua-driver's per-action `delivery_mode:
     "foreground"` gives focus back to the previous app after every action;
     use `bring_to_front` first, then foreground actions.
   - A cua-driver pixel click can leave the real pointer on the SpaceO display;
     SpaceO then pauses ("cursor is currently on an agent screen"). Move it back
     with `move_cursor` (scope desktop) and `spaceo_session_resume`.
   - `read-session-chat --json` is huge; read replies with
     `ghostex read-session-chat $ref --all --last 3 --format text`.
   - Popovers and menus (where pill, model list, session menu, page window)
     are their own windows; `spaceo_screenshot` of such a window returns a
     scaled copy of the whole display. Read them with `spaceo_read_screen`
     on that window id and picture them with `spaceo_screenshot full=true`.
     A SpaceO pixel right-click on a sidebar row does nothing (unconfirmed);
     cua-driver `right_click` with x,y from a `get_window_state` screenshot
     opens the session menu.
   - If SpaceO reports `input: PAUSED (operator has Control)`, a person took
     the session: stop driving it. If Ghostex then shows a new pid, tell a
     quit from a crash with `log show --predicate 'process == "Ghostex"'`
     around that time ("Handling Quit AppleEvent" and launchd's
     `termination reported by launchd (0, 0, 0)` mean a clean quit) plus the
     crash baseline, and finish the remaining GUI steps on the main display
     with background accessibility presses, checking `HIDIdleTime` first.
7. **A crash at any time** (the app disappears from `list_apps`, a new file in
   `crashes/`, a new panic line, a new DiagnosticReport, or `ghostex sessions`
   hangs more than 15 seconds) is a **Stop**, whichever step was running. Log it
   with the step number and carry on only to gather evidence.

## The steps

"Latest" model: the first non-alias entry per agent in
`ghostex coordinator options --json` (today `claude-fable-5-2` for Claude; read
the Codex one the same way) and say which you used. `$ref` is the `globalRef`
from the create result.

| Step | On failure |
| --- | --- |
| 1.1 to 1.3 | Stop |
| 1.4 | Warn |
| 2.1, 2.2 | Warn |
| 3.1, 3.2 | Warn |
| 4.1, 4.2 | Stop if keys don't land; otherwise Warn |
| 5.1 to 5.4 | Stop |
| 6.1, 6.2 | Stop if stray text is left in the input box; otherwise Warn |

### 1. The last crash (crash checks; see "Keeping the list")

**1.1 Chat ending in one-click answers.** Fix `f95220d0d` (v11.8.2; #638, #640, #642).
1. `ghostex agents create claude --project-id $P --title smoke-1.1-runN --task 'Reply with one line, then these two links on their own line: [Yes](ghostex-answer:Q9001) [No](ghostex-answer:Q9001)'`
   (`create-agent` has no `--title`). The agent's auto title replaces the
   `--title` within a minute (a Claude one may copy the sender's session name),
   so track every session by its `$ref`, not its title.
2. Wait until `ghostex read-session-chat $ref --limit 3 --json` shows the reply.
3. Open that chat so it is the front chat (`ghostex focus $ref`), quit Ghostex
   (menu Ghostex > Quit), start it again (cua-driver `launch_app`, then
   `spaceo_adopt_app` if you use SpaceO), and check the same chat is open.
4. **Pass:** the app is running again with the chat open, the Yes and No buttons
   show (`get_window_state` with `query:"Yes"`), and the crash baseline from step
   3 is unchanged. **Stop** on a quit, a panic, or no buttons.

**1.2 Restart with a coordinator chat open.** Fix `71d23ed20`, `4f9c7dcc6` (v11.8.1, v11.8.2).
1. Use the coordinator that step 3.1 makes (do 3.1 first if it helps, or make
   one: `ghostex coordinator create --agent claude --title smoke-1.2-runN --task "say hello"`).
   Open its chat in the app.
2. Restart the app: quit and `launch_app`, as in 1.1.
3. Within 15 seconds of the window appearing: `ghostex sessions --json` returns,
   and a click on the sidebar row and on the chat's transcript still works
   (`click`, then `verify_state`). A coordinator that hasn't started yet (see
   3.1) shows only the welcome screen, which has no element to press; press
   its card's Edit/Start area or, once it has replied, a transcript button
   such as `Worked for … · N tool calls` (`spaceo_find "Worked for"`).
4. **Pass:** all three answer. **Stop** if any hangs (the freeze).

**1.3 A chat with no saved Reply draft.** Fixes `dbcb1b304`, `52100362b` (v11.8.0).
1. `ghostex agents create codex --project-id $P --title smoke-1.3-runN` (no task,
   so it never had a Reply draft).
2. Click its row in the sidebar to open its chat.
3. **Pass:** the chat opens with its transcript or an empty chat, the app stays
   up, no new crash file or panic line. **Stop** otherwise.

**1.4 Ghostex's notes stay hidden.** Fixes `275b1fc7d`, `71d23ed20` (#627; v11.8.0, v11.8.2).
1. Send the coordinator from 1.2 a message: `ghostex agents send $coord 'Reply with: smoke-note-ok'`.
2. `ghostex read-session-chat $coord --limit 6 --json`: the assistant reply must
   contain `smoke-note-ok`.
3. With cua-driver, `get_window_state` on that chat and also take a screenshot:
   no text containing `ghostex-system`, `<ghostex-` or a raw note block shows in
   the transcript.
4. **Pass:** the reply is there and no note text is visible. **Warn** otherwise.

### 2. Threads

**2.1 Latest Claude.** `ghostex create-agent claude --project-id $P --model <latest Claude> --effort low`
(no `--title` there: `ghostex rename-session <sessionId> smoke-2.1-runN`, which
the auto title may replace again), then `ghostex agents send $ref 'Reply with exactly: smoke-ok-2.1'`.
**Pass:** within 120 seconds `ghostex read-session-chat $ref --limit 4 --json`
has that text from the assistant. **Warn** otherwise (say if it is the account:
`ghostex accounts`).

**2.2 Latest Codex.** The same with `create-agent codex --model <latest Codex>`
and `smoke-ok-2.2`.

### 3. Coordinators

**3.1 With Claude.** `ghostex coordinator create --project-id $P --agent claude --title smoke-3.1-runN --task 'Reply with exactly: smoke-ok-3.1'`.
From 11.10.0 a new coordinator does not start by itself: its first message
(the skill line plus `--task`) waits as a "First message" card in its chat box
with Edit and Start. Open its chat (`ghostex focus $ref`) and press the card's
Start (`spaceo_find "Start"` role Button; not the sidebar's "Start your last
agent" one), or send it any message. Time the 120 seconds from Start.
**Pass:** it shows in the sidebar (`ghostex coordinator list --project-id $P --json`
lists it) and `read-session-chat` has the reply within 120 seconds. **Warn**
otherwise.

**3.2 With Codex.** The same with `--agent codex` and `smoke-ok-3.2`.

### 4. Terminal and browser

**4.1 Terminal command.**
1. `ghostex create-session smoke-4.1-runN --start --project-id $P` (its
   `session.globalRef` is the id). Not `ghostex terminal`: run from `$W` it adds
   a new project named after that folder, and its terminal never starts.
2. `ghostex send-text <id> 'echo smoke-ok-4.1'` and `ghostex send-enter <id>`.
3. `ghostex focus <id>` (keyboard focus to the pane), then type a second
   command with real keys (`spaceo_type` `echo smoke-keys-4.1` with submit, or
   cua-driver `type_text` and `press_key` Return), so real keys are tested, not
   only the CLI. Also paste one line: save the user's clipboard (`pbpaste >
   $W/clipboard-saved.txt`), put the line on it, cua-driver `hotkey` cmd+v on
   the window (it runs Edit > Paste), Return, then restore the clipboard.
4. **Pass:** `ghostex read-text <id> --lines 20` shows `smoke-ok-4.1` and
   `smoke-keys-4.1`, and the paste lands with no "can't be delivered" message.
   **Stop** if typed keys or the paste don't arrive (`dd5fd7d46`, next release).

**4.2 Browser: open it and search.**
1. Open a browser pane: `ghostex browser open https://duckduckgo.com --project-id $P --reuse none`.
2. Type `ghostex terminal` and press Return. A background click can't focus the
   address bar, but DuckDuckGo focuses its own search box, so the keys land
   there; say which box you typed in.
3. **Pass:** every letter shows in the box (screenshot) and a results page loads
   with links (read its title or text). **Stop** if keys don't land; **Warn** if
   the page doesn't load (network). The native window title stays "Ghostex",
   so `spaceo_wait_for window_title_contains` never fires; read the results
   from a screenshot (the tab title and address bar change).

### 5. Typing in fields (all Stop)

Use `type_text` through cua-driver, then read the value back with
`get_window_state` (`query`) or `verify_state`. If the field's value can't be
read from the accessibility tree, compare a zoomed screenshot (`zoom`).

**5.1 A Settings field.** Open Settings (`spaceo_menu ["Ghostex","Settings…"]`;
if Ghostex takes activation and SpaceO pauses, `spaceo_session_resume` and open
it again; on the 11.10.0 run it opened as window `[focused]` with no pause, so
check `spaceo_list_windows` first). There is no General name box. The field that takes background
focus is Settings' own search box (others, like Accounts > Name, need a real
click). Type `smoke-field-5.1`.
**Pass:** the same text reads back (`spaceo_find` TextField shows its value).
Press Escape and check the old value is back.

**5.2 A search box.** Type `term` into the sidebar search and `font` into
Settings' search. **Pass:** each shows every letter and the list narrows. Clear
both. The sidebar has no filter box of its own: its search is the Search
palette, the sidebar's `Search (⌘P)` button (`spaceo_find "search"`), which
opens on its Sessions tab with the box focused (a TextField labelled
`Search sessions...`); Escape closes it.

**5.3 An inline HTML page with inputs.**
1. Write a throwaway page to `$W/smoke-page.html` from the `ghostex-visuals`
   skeleton (`<meta name="ghostex-page" content="v2">`): a `gx-questions` page
   with one text question (`type="text"`) and one choice question. It is a test
   fixture, not a page for the user: publish it with
   `ghostex show $W/smoke-page.html --no-check --json` and retire it at the end with
   `ghostex page retire <page>`, so nothing stays open. (Builds of 11.8.0 to
   11.8.2, and 11.9.0 builds older than the fix, answer "No gxserver endpoint
   for POST /api/retireVisualPages"; there, closing the session that shows
   the card clears its open questions.)
2. Open the page window in the app: the card needs to be in a chat, so have
   the 2.1 session echo the ```visual block (`agents send $ref --body-file`
   with "Reply with exactly the following block and nothing else"), open that
   chat and press the card's Open. Then click into the text box, type
   `smoke-page-5.3`, pick a choice. Background input never reaches the page
   window's web view (clicks, keys, Tab and the a/b shortcuts all do nothing),
   and neither does a DevTools click (`ghostex browser mcp`
   `ghostex_click`): it focuses the field in the page, not the native view.
   This step needs foreground input on the main display (see the rules):
   end any SpaceO session, `bring_to_front` the page window, click into the
   box, type, click a choice, then give focus back. On the 11.9.0 run even
   that delivered nothing to Ghostex (cua-driver's foreground clicks and
   keys didn't reach the chat box either, so the tool route, not the page,
   was at fault). OS-level input from the agent's own shell is no way out
   either: there `AXIsProcessTrusted()` and `CGPreflightPostEventAccess()`
   are both false (check with a two-line Swift script; it doesn't prompt), so
   CGEvent posting is dropped and System Events `keystroke` would need an
   Accessibility prompt, which the agent must not answer.
   **What works: the DevTools protocol on the page window's webview.** Ghostex's
   CEF answers on a local DevTools port (find it with `lsof -nP -iTCP
   -sTCP:LISTEN -p <Ghostex pid>`, the one whose `/json/version` says
   Chrome; it was 9334). `/json/list` lists the open page by its title; connect
   to its `webSocketDebuggerUrl` (Node's built-in `WebSocket`) and: focus the
   text box (`Runtime.evaluate` `box.focus()`), `Input.insertText` most of the
   text, a few `Input.dispatchKeyEvent` keys (a letter, then `x` and
   Backspace), click the choice with `Input.dispatchMouseEvent`, read back
   the box's value, the radio's `aria-checked` and the "N of 2 answered"
   counter, and save `Page.captureScreenshot` to `$W`. With `Runtime.enable`
   and `Log.enable` on, `Page.reload` the page and collect console errors
   (the per-box Dictate buttons must load without errors; don't press one,
   it asks for the microphone). Report 5.3 as PASS (DevTools input): native
   key routing into the app is covered by 5.4. The Send button is always
   enabled in this kit; the counter is what changes. Don't press Send.
3. **Pass:** the typed text shows in the box letter by letter, the pick shows,
   and the Send button enables. Do **not** press Send (it would send an answer).
   Retire the page.

**5.4 The chat box: type and send.**
1. `ghostex focus $ref` (the 2.1 chat; this puts keyboard focus in its chat
   box), type a short message `smoke-chat-5.4` with real keys, press Return.
   `ghostex read-session-chat-queue $ref --json` `.draft.content` shows what
   the box holds.
2. Then a long one-line message (about 600 characters of `smoke ` repeated, no
   newline), press Return (`b89d89a10`, next release).
3. **Pass:** each time the box empties and the message shows in the transcript
   (`read-session-chat $ref --limit 6 --json`), and never stays on "sending".

### 6. Change the model in the chat view (picker, by cua-driver)

Fixes `5b865f3db` (Claude) and `73f6975d9` (Codex), both in the next release.

**6.1 Claude.** Open the 2.1 chat. Click the model picker in the chat view
(`get_window_state` query `model`), pick a different model than the current one.
Check: the picker label and the transcript show the new model, the input box is
empty (no `/model` text left), and one message afterwards gets an answer
(`smoke-ok-6.1`). CLI cross-check: `ghostex read-session-chat $ref --limit 3 --json`.
**Stop** if stray text sits in the input box; **Warn** if the model doesn't
change or the next message gets no answer.

**6.2 Codex.** The same on the 2.2 chat, `smoke-ok-6.2`. Codex writes no
"Set model" line into the transcript; check the picker label and the next
reply's `"model"` in `read-session-chat $ref --limit 4 --json`.

The picker is a button `Model: <name>` (`spaceo_find "Model:"`); its list is a
separate window with the models as MenuItems. Picking a model also changes the
agent's saved default: Claude's `/model` writes `model` in
`~/.claude/settings.json` ("saved as your default for new sessions"), and
Codex writes `model_reasoning_effort` in `~/.codex/config.toml`. Copy both
files to `$W` before 6.1 and put the old values back after 6.2.

### Clean up

Close every `smoke-*-runN` session (`ghostex agents close <ref>` for threads and
coordinators, which then stay in `coordinator list` as `stopped`; `ghostex kill`
for terminals), retire the test page, close the browser tabs you opened (the
tab's x), remove any project a step added (`ghostex remove-project --project-id <id>`),
put any setting you touched back (Claude and Codex defaults, the clipboard),
and end the SpaceO session (`spaceo_session_destroy` with `keep_apps`; a
hover tooltip left on its display blocks that until Ghostex next restarts). List what you removed in the report (AGENTS.md:
"Clean up what you create to test").

## The report

Post it to the coordinator as the thread's final message, and paste it in the
release report (step 9 of SKILL.md). One line per step, and a verdict first.

```text
Smoke test for X.Y.Z: PASS | PASS WITH WARNINGS | STOP
Release point: <commit>   App: rebuilt <time>, dev build / optimized   Run: N   Took: NN min
1.1 PASS
1.2 PASS
1.3 PASS
1.4 WARN  note text "<...>" visible in the transcript (screenshot: $W/1.4.png)
...
6.2 PASS
Crashes: none | <step, file or panic line>
Stops: none | <step list>     Warnings: none | <step list>
Cleaned up: <sessions, page, browser pane>
Crash checks: kept ... | dropped ... (see "Keeping the list")
```

- **STOP:** any Stop failure or a crash. Name the step and the evidence. The
  release does not dispatch. After the fix, run everything again as "Run N+1".
- **PASS WITH WARNINGS:** no Stop. The release may dispatch; the warnings go in
  the release report to the user so they can decide to fix next time.
- Never mark a step PASS that you could not check. Use `NOT RUN: <why>`. A
  `NOT RUN` on a Stop step counts as a Stop.

## Keeping the list

A crash check (section 1) stays for **three releases** (Q3734, user, 2026-11-17),
then is dropped, because every crash fix adds a step and the run only grows.

| Check | Fix shipped in | First smoke-tested in | Drop after |
| --- | --- | --- | --- |
| 1.1 One-click answers (`f95220d0d`) | v11.8.2 | next release after v11.8.2 | its third smoke-tested release |
| 1.2 Restart with a coordinator chat (`4f9c7dcc6`, `71d23ed20`) | v11.8.1, v11.8.2 | same | same |
| 1.3 No saved Reply draft (`dbcb1b304`, `52100362b`) | v11.8.0 | same | same |
| 1.4 Ghostex's notes hidden (`275b1fc7d`, `71d23ed20`) | v11.8.0, v11.8.2 | same | same |

- "Smoke-tested" counts a release whose run reached PASS or PASS WITH WARNINGS.
  A STOP run doesn't count.
- When a release is out, fill in "First smoke-tested in" for any check that was
  just run for the first time, and delete rows whose third release was this one
  (also delete the step and renumber the rest). The page and counts in the
  chips (`docs/2026-11-17/smoke-test/index.html`) are a snapshot; this file is
  the source.
- **Do it after the release, in its own commit** (`chore: smoke test list`),
  never in the prepare commit, which changes only `package.json` and
  `CHANGELOG.md`, and never before the dispatch, which needs a clean tree.
- A crash fixed in a release adds a step in section 1 with the same columns.
  Steps 2 to 6 are permanent: the user's list.
- The model-change, terminal-paste and long-message fixes live in 4.1, 5.4, 6.1
  and 6.2, which stay because they are the user's permanent list.
