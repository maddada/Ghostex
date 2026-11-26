# Pre-release macOS smoke test (runbook)

Run from step 3a of [SKILL.md](SKILL.md), before `release:actions` is dispatched.
Written 2026-11-17 from the user's picks on the checklist page
(`docs/2026-11-17/smoke-test/index.html`, Q3734 to Q3736). The page shows the
checklist; this file is what a thread follows.

It has run on every release since 11.9.0. Where a selector, a column or a
command below turns out wrong, fix it in this file in the after-release commit
(see "Keeping the list").

**The script does the CLI half.** `scripts/smoke.sh` (beside this file) makes
the 2.1, 2.2, 3.1, 3.2 and 4.1 sessions, sends their messages, waits for the
replies, takes the crash baseline, publishes the 5.3 page, puts Claude's and
Codex's defaults back after 6.1 and 6.2, and cleans up, with every ref in one
`$W/refs.json`. The agent does only the GUI presses. `smoke.sh help` lists its
commands; each step below says which one to run.

## The rules (the user's decisions, 2026-11-17)

- **An agent runs every step**, including the one that only needs eyes (5.3),
  driving the GUI with the `cua-driver` skill. The user does none of them
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
  stops, the fault is fixed and pushed, the app is rebuilt, and every step runs
  again. Warnings alone never stop it, and don't need a rerun.
- **Press only what a `query` read returned** (user, 2026-11-20, after the
  11.12.0 run: `run_steps` matched a bare "Open" to "Start your last agent in
  the open project" and started a new agent). Find every control with
  `get_window_state({pid, window_id, query:"<exact name>"})` and press its
  `element_token`; never pass a bare word to a press or a `run_steps` step.
  Name the exact control: a page card's `Open page …` button, the coordinator
  card's `Start` (not the sidebar's `Start your last agent …`). Read the
  matched row's name before pressing; if the query returned more than one, or
  the wrong one, narrow the query instead of guessing.

## Where, when, who

- **Where:** the user's Mac, in the real Ghostex (it has the real accounts, so
  Claude and Codex both work). The app is rebuilt from the release commit, the
  one that `release:actions` will tag, so it must already be committed and
  pushed (step 2 and 3 of SKILL.md are done: `HEAD == origin/main`).
- **Who:** one thread, started by the Main Coordinator. Not the coordinator
  itself.
- **How long:** about 10 minutes after the build (the build adds about 1 to 6,
  the typecheck before it about 1 to 3, the all-targets check not timed
  yet).
- **Windows and Linux** run a short GUI pass of their own in parallel (see
  "Windows pass" and "8. Linux GUI pass" below), and the phone gets its own
  step (7), on the Android emulator on m7-arch.

### When the brief says "on your own test copy" (12.2.0)

Then nothing runs in the user's Ghostex: a test server (`cargo xtask
test-server start --name smoke-<ver>`, as in step 7; `cargo xtask test-server
remove` at clean-up) and an app bundle built from the release worktree, driven
only by its own pid and window ids. What 12.2.0's run learned:

- **Launch the app with `env -i`** and only the test server's variables
  (`GHOSTEX_HOME`, `GHOSTEX_GXSERVER_DEV_PORT`, `GHOSTEX_GXSERVER_BIN`/`_CLI`,
  `ZMX_DIR`, `GHOSTEX_OWNER_KEYS_STORE=file`, a free
  `GHOSTEX_GPUI_CEF_REMOTE_DEBUGGING_PORT` and `GHOSTEX_CODE_SERVER_PORT`)
  plus `HOME`, `USER`, `PATH`, `LANG`, `TMPDIR`.
  Started from an agent's shell, it otherwise inherits that agent's
  `GHOSTEX_*` variables and talks to the user's real server (12.2.0 run 1 made
  a launchd job for the real server's port that had to be removed). Write its
  pid to a file and stop only that pid, and the `GhostexEditor --daemon` it
  leaves (parent 1, path inside the test bundle).
- **A test copy never touches the login Keychain.** `GHOSTEX_OWNER_KEYS_STORE=file`
  (in the test server's `env.sh` since 2026-11-26, so copy it from there) keeps
  its owner, approver and salt keys as 0600 files in `$GHOSTEX_HOME/owner-keys/`
  and never calls the Keychain or Touch ID; a presence prompt it would show
  fails closed ("can't confirm it's you"), so a test copy can't approve
  anything that needs Touch ID (approvals need it only with "Approvals
  require presence" on, off in a fresh home). Without the variable the copy asks the login
  Keychain for `dev.ghostex.keys` as it starts and the prompt stalls the run
  (#772): if one ever appears, answer nothing, stop the copy by its pid and
  check its environment. Never set it on the user's own app.
- **Run `smoke.sh` from a shell that sourced the test server's `env.sh`**, and
  after a `test-server` restart run `hash -r` and check `which ghostex` is
  the test server's `gxserver/bin/ghostex`: a runner shell that survived the
  restart lost that `PATH` and run 2 quietly used the Homebrew CLI.
- **Agent-only behaviour needs the agent's token, not its session id.** The
  server tells an agent from the owner by the auth token
  (`GHOSTEX_SESSION_TOKEN`), so `GHOSTEX_SESSION_ID` alone acts as the owner:
  `ghostex browser open` switches the view at once, with no page toast. To act
  as a test agent, copy `GHOSTEX_SESSION_ID`, `GHOSTEX_GLOBAL_SESSION_REF`,
  `GHOSTEX_SESSION_TOKEN`, `GHOSTEX_GXSERVER_AUTH_TOKEN_FILE` and
  `GHOSTEX_GXSERVER_BASE_URL` from that agent's process (`ps eww`, matched by
  its ref) into the command's environment, printing none of them. Test
  servers only.
- **Without a working Claude login on the test server**, 2.1, 3.1 and 6.1's
  reply are Warn, and 5.3 runs on the 2.2 Codex chat (`$S page 2.2`).
- **Agent browser tools** (`ghostex browser mcp`) are checked against a local
  fixture page served by `python3 -m http.server` from the scratch folder: a
  button, a link, a text field, a file input, a button under an overlay and a
  button below a tall spacer, each logging `event.isTrusted`. Kill the server
  by its pid at clean-up.

What 12.3.0's run added:

- **`$!` after `cd dir && nohup python3 -m http.server … &` is the shell's
  pid, not the server's.** Find the listener with
  `lsof -nP -iTCP:<port> -sTCP:LISTEN` and stop that pid. Before sending
  anything to a port, check its owner the same way: 12.3.0 curled a port that
  belonged to an old extension server in the user's real home.
- **Click a fresh background page, not only the one on screen.** Open the
  fixture with `ghostex_open_page` (`reuse: none`; it opens hidden) and click
  at once: every click came back `method: script` because the hidden page was
  still laid out at 1x1 (`innerWidth` 1), fixed in 519e741ef. A
  `ghostex_screenshot` sizes the page, so take none before the clicks. A
  `<select>` opens its list only on a page on screen: check "list opened,
  script click; next click real" on the visible tab (`ghostex_navigate` it to
  the fixture), since in a hidden tab the select press is an ordinary real
  click.
- **cua-driver reports wrong bounds for the app's child windows** (Settings,
  the Theme panel, menus). Press by `element_token`; for pixels, measure on
  `screencapture -x -o -l <main window id>` (the main window with its child
  windows) and convert to the reported bounds. Never use `screencapture -R`:
  a region takes whatever app is on top. cua-driver drags never reach the
  app, so dragging and swiping Spaces stay untested.
- **Settings opened from the app menu can show an empty accessibility tree.**
  Open it from the rail's gear instead.
- **`ghostex extensions install-local` grants no permissions,** and the
  folder name must equal the manifest's `name`. Grant what the test needs
  with `ghostex extensions state <id> --set grantedPermissions='[…]'`; an
  extension's own `openView` answers 403 until then. Turn an extension off in
  Settings > Extensions: off through `extensions state` the rail pin stayed.
- **Inbox Snoozed and Recent rows** come from `ghostex needs-you add` run as
  the test agent: `needs-you done Q<n>` puts one under Recent, and the row's
  ⋯ > Snooze > 1 hour puts one under Snoozed.

## Before the first step

1. `git rev-parse HEAD` is the release point. Write it in the report.
   The coordinator may build from a release worktree
   (`~/.local/share/agent-work/Ghostex/release-<X>/ghostex`, see `git worktree
   list`) while the main checkout's `main` has drifted from `origin/main`
   (11.11.1 run: ahead 73, behind 79). Then the release point is that
   worktree's HEAD, not the main checkout's. To prove the installed app is
   that build, find a string literal that only one of the two trees has
   (`git diff --stat <main-HEAD> <release>`, then grep a removed or added
   file) and `LC_ALL=C grep -c -a '<string>'
   /Applications/Ghostex.app/Contents/MacOS/Ghostex`.
2. **Typecheck first, then rebuild.** In the release point's tree (the release
   worktree when it builds from there), run `cargo xtask typecheck` before the
   rebuild. It also fails on unformatted app-owned files and stale generated
   Help files, which the prepare step (SKILL.md step 3) would otherwise find
   only after the smoke passed (11.12.0: 10 unformatted files, a formatting
   commit on top of the smoked build). Then, in `apps/desktop`, run `cargo
   check --locked --all-targets`: the typecheck doesn't build the app's other
   targets (the `native-modal-demo` bin, the CEF helper and bootstrap, the
   smoke and demo bins, the tests), and the release's `cargo check
   --all-targets` job does (11.13.0: a `native-modal-demo` compile error the
   typecheck passed, found by CI, about 50 minutes lost). If either fails,
   don't rebuild: report it to the coordinator, who commits the fix (`cargo
   xtask format` for formatting) and pushes; that commit is the new release
   point, and the smoke starts again from step 1. Then rebuild and relaunch the app from it, in the
   background:
   `cargo xtask rebuild` (committed code only, log under
   `~/.local/share/agent-work/Ghostex/rebuilds/`). It is a fast dev build
   (opt-level 0), not the signed release build; say so in the report. Add
   `--optimized` (check `cargo xtask help` for how `rebuild` passes it) only
   when the user asks for a build closer to what ships. If it exits 3 (can't
   sign), run it from a shell started since the last login, or ask the user.
   The dev build carries no commit stamp and still reports the previous version
   (the version bump is the prepare commit), so match the release point by the
   rebuild log's start time against the commit time.
3. **Start the script:** `S=.claude/skills/ghostex-release/scripts/smoke.sh;
   SMOKE_RELEASE=<release point> $S run` (from the main checkout; leave
   `SMOKE_RELEASE` out when the release point is this checkout's HEAD). It
   picks `$W` (`~/.local/share/agent-work/Ghostex/smoke/<release-short>-run<N>/`,
   the next free N, remembered for the later commands), resolves your project
   (`ghostex agents whoami --json`), reads the latest models, saves the crash
   baseline (`crashes/`, the panic log's line count, Ghostex's
   DiagnosticReports, `crash-reports/`) and copies of
   `~/.claude/settings.json` and `~/.codex/config.toml`, creates the 2.1, 2.2,
   3.1, 3.2 and 4.1 sessions (named `smoke-<step>-run<N>`), skips Codex's
   update picker where it shows, and sends 2.1's, 2.2's and 4.1's first
   message. It prints every ref; `$S show` prints them again. Anything the GUI
   steps make by hand goes in the same file for the clean-up: `$S add session
   <ref>`, `$S add page <id>`, `$S add tab <devtools-port> <target-id>`, `$S add
   project <id>`. The crash checks run with `$S check crash` (any time; the
   default `$S check` includes it).
4. **Size the window and the capture once, now** (11.12.0 run: mixing the
   default 1366 px capture with full-size ones sent a click to the wrong
   place, because cua-driver maps pixels to the last screenshot's scale).
   Write the main window's frame down (`list_windows({pid})`, into
   `$W/window-frame-before.txt`), then `set_window_frame` it to **1680 by
   1024**, and pass `max_image_dimension: 0` (full size) on **every**
   `get_window_state` and screenshot for the whole run, never the default.
   That also meets the 1650 by 962 screenshot rule. Put the frame back at
   the end.
5. Name every session you create by hand `smoke-<step>-run<N>` and record it
   with `$S add session <ref>`.
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
   - **Without SpaceO** (11.11.0 run: not installed, `ghostex spaceo status`
     says so): drive the main display with background cua-driver only; it
     covers every step except the Search palette half of 5.2 (see there).
     cua-driver's foreground and desktop-scope input (`delivery_mode:
     "foreground"`, `scope: "desktop"`) does not reach Ghostex at all: with
     Ghostex frontmost, desktop keys and Escape left the open palette
     untouched and foreground pixel clicks pressed nothing. Don't spend time
     on it. If the user is active (`HIDIdleTime` resets while you are idle),
     stay in the background.
   - **What reaches Ghostex from cua-driver, and why (2026-11-19 study).**
     GPUI publishes accessibility through AccessKit
     (`.dependencies/zed/crates/gpui/src/_accessibility.rs`): an element is
     in the tree only if it sets `.role(...)` (a `div` without one is
     filtered out even with `on_click`); it gets AXPress only from
     `.on_click(...)` or `.on_a11y_action(AccessibleAction::Click, …)`
     (`on_mouse_down` alone gives none); Focus is answered for focusable
     elements. Text fields (gpui-component `Input`) list SetValue too, and
     it fires the same change event typing does, but the 11.11.0 run saw
     `set_value` do nothing on the Search palette's box (5.2): read the
     value back after every `set_value`. A background `click` with x,y is
     often turned into an AX press of whatever the hit-test finds ("PX
     hit-test pressed the background element via AX") and then misses
     controls that have no press.
   - **Presses added after 11.11.0** (2026-11-19, first in 11.11.1, where
     the run confirmed the Work button, the menu rows and the Close Group
     dialog; an older build still needs the `drag` below): the header `Work`
     button (main window and a popup's), every frosted sidebar menu row
     (role MenuItem, labelled with its text: `Snooze`, `Park`, `Advanced`,
     `Move to New Group`, a group's `Ungroup` and `Close`), the Needs you
     rows and Work panel cards (a Button labelled with the to-do's title)
     and a Work panel card's `More actions` (⋯). Find each with
     `get_window_state({pid, window_id, query:"<label>"})` and press it with
     `click({element_token:"<snapshot_id>:<row>"})`. The Close Group dialog
     (`Cancel`, `Close`) and the floating ask panel (a Dialog labelled with
     the to-do's title: `Works`-style answers, `Reply`, `More actions`,
     `Close`, the command's `Copy` and `Run in terminal`) already answered a
     press; the panel is its own window, so `list_windows({pid})` for its
     window_id first. Pressing an answer there closes the panel as a mouse
     press does. On a session's menu, `Move to New Group` and `Move to Group`
     (a submenu of group names) sit under `Advanced`, which opens as a
     second menu window. A group's `Close` asks only when the group holds
     two or more sessions; with one it closes the group and its session at
     once (`sidebar_menu/project.rs`). The Close Group dialog is its own
     window (about 460 by 167): read it by window_id, not with a `Cancel`
     query on the main window, which only matches menu-bar items.
   - **Not in the tree yet (11.11.1):** the header ⋯ menu (`More header
     controls`) lists its rows as unlabelled `AXMenuItem`s, the pin on each
     row is an invisible 24 px target at the row's right end that answers
     mouse-down only (`header_pins_look.rs`), a pinned header control shows
     as a button with no press, and its right-click menu's `Unpin from
     header` is one unlabelled `AXMenuItem` (pressing it works). Read the ⋯
     menu from its screenshot, pin with a zero-length `drag` about 26 px in
     from the row's right edge (window pixels of the menu window), and open
     the Unpin menu with a pixel `right_click` on the pinned control.
   - **Real pointer events: a zero-length background `drag`.** `drag` is
     pixel-only (no AX substitute), so it delivers a real mouse down/up to
     the window in the background, and it takes `modifier`:
     `run_steps({steps:[{tool:"drag",args:{pid,window_id,from_x:X,from_y:Y,
     to_x:X,to_y:Y,duration_ms:60,steps:1}}]})`, coordinates in the last
     `get_window_state` screenshot of that window. Seen working on the
     11.11.0 follow-up: it collapsed the chat's Needs you dock (a GPUI
     control) and pressed the native red close button of a second window.
     Use it for a Cmd+click (`modifier:["cmd"]`, the Files table link;
     confirmed on 11.11.1: it opened the linked file. `ghostex open <file>`
     switches the window to that file's project Space and opens Files
     beside that Space's open chat; close the Files tab with a zero-length
     `drag` on its ×, which isn't in the tree, and switch back with the
     sidebar's `Space <name>` button), and
     on a build from before the presses above for the header Work button
     and the frosted sidebar menu rows. Read the result from the
     accessibility tree too: the Work panel shows as an element labelled
     `Work` about 380 wide at the chat's right edge, and the button carries
     `aria_expanded`. Some windows' screenshots don't line up with where a
     pixel lands (11.11.1 run: Settings, the Search palette, page and menu
     windows return a picture of a larger area): take pixel targets from
     `get_window_state` with `tree_format:"elements"`, whose
     `screenshot_frame` is in the coordinates `drag` and `click` use, not
     from what the picture shows. A drag aimed by the picture at Settings'
     Session notes card toggled Voice agent instead (put back at once).
   - **Check the window is on screen before trusting a screenshot**:
     `list_windows({pid})` `is_on_screen` and `on_current_space` for the
     main window. If the user moves to another Space, the window keeps
     taking events but its screenshots are the last frame drawn, so a
     change looks like nothing happened. Don't switch Spaces for them; a
     File > New Window opens on Ghostex's own Space too (close it with a
     zero-length `drag` on its red button, about x 21, y 13 at 900 wide).
     Don't loop waiting; report the GUI parts as not yet run.
   - Frosted sidebar menus (session and project right-click menus, window
     id `ghostex.frostedSidebarMenu`): open one with cua-driver `right_click`
     at the row's x,y in a `get_window_state` screenshot (as above), then
     `list_windows({pid})` for the menu's window and
     `get_window_state({pid, window_id:<menu>, query:"Advanced"})`. A row
     with a submenu (`Advanced`, `Snooze`, `Park`) opens it when pressed;
     re-read the menu window with `since:"latest"` for its rows. On 11.11.0
     and older only Rename, Pin and Open in a Popup were in the tree; for
     the others there, use the pixel route: its
     `get_window_state` screenshot is a scaled copy of the whole display,
     but it is still twice the window's point size, so a row at screen
     point (sx, sy) is window pixel ((sx − wx)·2, (sy − wy)·2) with the
     menu window's origin (wx, wy) from `list_windows`; take the row's
     screen point from `get_desktop_state` and press it with the
     zero-length `drag` above (untried on screen). Press Escape on the main
     window (`press_key` pid + its window_id) to dismiss a menu; a
     background click elsewhere does not.
   - In `run_script`, `cua.listWindows()` returns `{windows:[…]}`, not an
     array.
   - Codex sessions can open on Codex's own "Update available" picker
     (11.11.0 run: 0.169.1 → 0.170.0); `agents send` then refuses with
     "Update Codex to …? Answer it in chat before sending". Press Escape in
     each Codex session (`ghostex send-key <ref> escape`, which picks Skip;
     never "Update now") and resend with the `--request-id` it printed. The
     coordinator's First message card survives it. `smoke.sh` does this
     itself for the sessions it makes (after creating them, before each
     send and before each reply wait); do it by hand only for a Codex
     session you made yourself.
7. **A crash at any time** (the app disappears from `list_apps`, a new file in
   `crashes/`, a new panic line, a new DiagnosticReport, or `ghostex sessions`
   hangs more than 15 seconds) is a **Stop**, whichever step was running. Log it
   with the step number and carry on only to gather evidence. `$S check crash`
   compares all four against the baseline and times `ghostex sessions`; run
   it after any step that looked wrong, and once at the end.

## The steps

"Latest" model: the first non-alias entry per agent in
`ghostex coordinator options --json` (11.12.0: `claude-fable-5-2` for Claude,
`gpt-6.2-sol` for Codex). `smoke.sh run` reads them and prints which it used;
say them in the report. `$ref` is a step's `globalRef` (`$S show`).

| Step | On failure |
| --- | --- |
| 1.x crash checks | none at the moment (see "Keeping the list") |
| 2.1, 2.2 | Warn |
| 3.1, 3.2 | Warn |
| 4.1, 4.2 | Stop if keys don't land; otherwise Warn |
| 5.1 to 5.4 | Stop |
| 6.1, 6.2 | Stop if stray text is left in the input box; otherwise Warn |
| 7.1 Phone launch | Stop on a crash |
| 7.2 Phone connect and chat | Warn; NOT RUN only if the pairing itself fails |
| 7.3 Phone chat (demo) | Warn |

### 1. The last crash (crash checks; see "Keeping the list")

None at the moment. 1.1 to 1.4 (one-click answers, restart with a coordinator
chat, no saved Reply draft, notes hidden) passed in three smoke-tested
releases and were dropped after 11.12.0; the table under "Keeping the list"
has their record. When a release fixes a crash, its check goes here as 1.1
with the same columns, and `smoke.sh` gets its session if the check needs one.

### 2. Threads

**2.1 Latest Claude, 2.2 latest Codex.** `smoke.sh run` made them
(`create-agent <agent> --model <latest> --effort low`, renamed
`smoke-2.x-runN`; the auto title may replace that, so go by `$ref`) and sent
each `Reply with exactly: smoke-ok-2.x`. Run `$S check 2.1 2.2` (the default
`$S check` includes them): it waits for each reply side by side and prints
PASS with the seconds since the send, or WARN.
**Pass:** within 120 seconds the assistant's reply has that text. **Warn**
otherwise (say if it is the account: `ghostex accounts`). A message that could
not be sent at all (the agent didn't start) shows as WARN with the send's
error.

### 3. Coordinators

**3.1 With Claude, 3.2 with Codex.** `smoke.sh run` made them with
`coordinator create --task 'Reply with exactly: smoke-ok-3.x'`. From 11.10.0 a
new coordinator does not start by itself: its first message (the skill line
plus `--task`) waits as a "First message" card in its chat box with Edit and
Start. Open its chat (`ghostex focus $ref`), read the card's `Start` button by
query (role Button; not the sidebar's `Start your last agent …`) and press its
token, then straight away `$S check 3.1` (or `3.2`): the 120 seconds run from
that call. Without the GUI, `$S run --start-coordinators` starts them at
creation instead (a test server).
**Pass:** it shows in the sidebar (the check also finds it in `ghostex
coordinator list`) and the reply comes within 120 seconds. **Warn** otherwise.

### 4. Terminal and browser

**4.1 Terminal command.**
1. `smoke.sh run` made the terminal (`create-session smoke-4.1-runN --start`;
   not `ghostex terminal`, which run from `$W` adds a new project named after
   that folder) and sent `echo smoke-ok-4.1` with `send-text` and
   `send-enter`.
2. `ghostex focus $ref` (keyboard focus to the pane), then type a second
   command with real keys: `echo smoke-keys-4.1` (`spaceo_type` with submit, or
   cua-driver `type_text` and `press_key` Return), so real keys are tested, not
   only the CLI. Without SpaceO, `type_text` with only pid/window is refused
   (`no_focused_text_input`); pass the token of the `AXTextArea "Terminal"`
   row (`get_window_state` query `Terminal`). Press Return with `press_key`
   on pid and window_id, not on that token (refused: "the addressed element
   did not acquire keyboard focus"). Then paste `echo smoke-paste-4.1`: save
   the user's clipboard, put the line on it, cua-driver `hotkey` cmd+v on
   the window (it runs Edit > Paste), Return, then restore the clipboard.
   `pbpaste` saves text only: when `osascript -e 'clipboard info'` shows an
   image or HTML, save and restore every type with
   `~/.local/share/agent-work/Ghostex/smoke/scripts/pbarchive save|restore
   <file>` (Swift, NSPasteboard; `cmp` a second save against the first to
   prove the restore).
3. `$S check 4.1` reads the terminal: PASS when all three lines echoed
   (`smoke-ok-4.1`, `smoke-keys-4.1`, `smoke-paste-4.1`), PENDING when only the
   CLI line is there yet, STOP when a typed line is missing after the GUI part
   or "can't be delivered" shows.
4. **Pass:** all three echoed and no "can't be delivered" message.
   **Stop** if typed keys or the paste don't arrive (`dd5fd7d46`).

**4.2 Browser: open it and search.**
1. Open a browser pane: `ghostex browser open https://duckduckgo.com --project-id $P --reuse none`.
   It can open on `about:blank` (11.13.0): the address bar empty and the page
   white. Press the pane's Reload (by its `query` token) once, and it loads
   the address. Say in the report that it needed the Reload; a pane that
   stays blank after it is a **Warn**.
2. Type `ghostex terminal` and press Return. A background click can't focus the
   address bar, but DuckDuckGo focuses its own search box, so the keys land
   there; say which box you typed in. Without SpaceO the box isn't focused
   by itself: `type_text` with x,y on DuckDuckGo's box (no element_token)
   pixel-focuses it in the background and types; check with `zoom`.
3. **Pass:** every letter shows in the box (screenshot) and a results page loads
   with links (read its title or text). **Stop** if keys don't land; **Warn** if
   the page doesn't load (network). The native window title stays "Ghostex",
   so `spaceo_wait_for window_title_contains` never fires; read the results
   from a screenshot (the tab title and address bar change).
   When the network fails (an error page, or a screenshot that can't show
   whether results came), read the page through DevTools instead of guessing:
   find Ghostex's DevTools port as 5.3 does (`lsof -nP -iTCP -sTCP:LISTEN -p
   <Ghostex pid>`, the one whose `/json/version` says Chrome), then
   `curl -s http://127.0.0.1:<port>/json/list` gives the pane's `url` and
   `title`, and `Runtime.evaluate` on its `webSocketDebuggerUrl` reads
   `document.querySelector('input[name=q]').value` (the typed letters) and
   `document.body.innerText` (results or the error). Report what it read:
   letters there and an error page is a **Warn** (network), not a Stop.

### 5. Typing in fields (all Stop)

Use `type_text` through cua-driver, then read the value back with
`get_window_state` (`query`) or `verify_state`. If the field's value can't be
read from the accessibility tree, compare a zoomed screenshot (`zoom`).

**5.1 A Settings field.** Open Settings (`spaceo_menu ["Ghostex","Settings…"]`;
if Ghostex takes activation and SpaceO pauses, `spaceo_session_resume` and open
it again; on the 11.10.0 run it opened as window `[focused]` with no pause, so
check `spaceo_list_windows` first). Without SpaceO: `invoke_menu
["Ghostex","Settings…"]`; Settings is its own window (about 1188 by 876,
`list_windows({pid})`), with a 24 by 24 companion window whose `Close`
button closes it (Escape only clears the search). There is no General name box. The field that takes background
focus is Settings' own search box (others, like Accounts > Name, need a real
click). Type `smoke-field-5.1`.
**Pass:** the same text reads back (`spaceo_find` TextField shows its value).
Press Escape and check the old value is back.

**5.2 A search box.** Type `term` into the sidebar search and `font` into
Settings' search. **Pass:** each shows every letter and the list narrows. Clear
both. The sidebar has no filter box of its own: its search is the Search
palette, the sidebar's `Search (⌘P)` button (`spaceo_find "search"`), which
opens on its Sessions tab with the box focused (a TextField labelled
`Search sessions...`); Escape closes it. Without SpaceO (Ghostex not
the active app, 11.11.0 run) this half can't be driven: pressing the button
by accessibility opens the palette and it closes again at once; Cmd+P
(`hotkey` on the main window) keeps it open, but cua-driver refuses to type
into it (`window_not_key`: macOS still reports the main window as key), and
pid-targeted `press_key` letters close it and land in the open chat's box
as stray text (clear them and check `read-session-chat-queue` `.draft`).
`set_value` on its `Search sessions...` field did nothing that run,
although the field offers SetValue and a SetValue fires the same change
event typing does (gpui-component `handle_accessibility_set_value`; the web
build's mirror lists `Focus SetValue` on its text boxes); the cause wasn't
found. **What works (11.11.1 run):** `hotkey` cmd+p on the main window,
`list_windows({pid})` for the palette (about 692 by 510), then straight
away `get_window_state({pid, window_id:<palette>, query:"Search sessions",
tree_format:"elements"})`, `set_value({element_token, value:"term"})`: the
box read back `term` and the list narrowed to matching sessions. Clear it
with `set_value` `""` and close the palette with the `Close` button of its
24 by 24 companion window. Don't try `type_text` first: it is still refused
(`window_not_key`, also after a zero-length `drag` on the box) and the
palette closes after the refusal. Report this half as PASS (AX SetValue,
not keystrokes); Settings' search, the chat box (5.4) and the terminal
(4.1) cover native typing.

**5.3 An inline HTML page with inputs.**
1. `$S page`: it writes the throwaway fixture `$W/smoke-page.html` (the
   `ghostex-visuals` skeleton, a `gx-questions` page with one text question and
   one choice question), publishes it (`ghostex show … --no-check --json`),
   records it for the clean-up's `ghostex page retire`, asks the 2.1 chat to
   echo its ```visual block, and waits until the card is there. It is a test
   fixture, not a page for the user.
2. Open the page window in the app: open the 2.1 chat, read the card's button
   by query (`Open page`) and press that token, never a bare "Open" (see the
   rules). Then click into the text box, type
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
   the box holds. In the background, `type_text` needs the composer's
   `AXTextArea` token (query `AXTextArea`; after a send the query
   `Message composer` no longer returns the row).
2. Then a long one-line message (about 600 characters of `smoke ` repeated, no
   newline), press Return (`b89d89a10`, next release).
3. **Pass:** each time the box empties and the message shows in the transcript
   (`read-session-chat $ref --limit 6 --json`), and never stays on "sending".

### 6. Change the model in the chat view (picker, by cua-driver)

Fixes `5b865f3db` (Claude) and `73f6975d9` (Codex), both in the next release.

**6.1 Claude.** Open the 2.1 chat. Press the model picker in the chat view
(`get_window_state` query `Model:`), pick a different model than the current
one, then `$S check 6.1` at once.
Check: the picker label and the transcript show the new model, the input box is
empty (no `/model` text left), and the message the check sends gets an answer
(`smoke-ok-6.1`). CLI cross-check: `ghostex read-session-chat $ref --limit 3 --json`.
**Stop** if stray text sits in the input box; **Warn** if the model doesn't
change or the next message gets no answer.

**6.2 Codex.** The same on the 2.2 chat, then `$S check 6.2`. Codex writes no
"Set model" line into the transcript; check the picker label and the reply
model the check prints.

The picker is a button `Model: <name>` (`spaceo_find "Model:"`); its list is a
separate window with the models as MenuItems. Read that window with a query
naming the models (`Fable|Opus|Sonnet|Haiku`, `GPT`): a query of
`AXMenuItem` also returns the whole menu bar. Picking a model also changes the
agent's saved default: Claude's `/model` writes `model` in
`~/.claude/settings.json` ("saved as your default for new sessions"), and
Codex writes `model` and `model_reasoning_effort` in `~/.codex/config.toml`.
`smoke.sh run` copied both files to `$W`. **Right after each pick**, run
`$S check 6.1` (or `6.2`): it first puts that agent's saved default back
(only those keys, so anything else changed in the files meanwhile stays),
then sends `Reply with exactly: smoke-ok-6.x` to the chat and waits for the
answer, printing the reply's model. On the 11.12.0 run the defaults waited
until the clean-up and every new Claude session in between ran on Haiku.
The picker label and the empty input box are still yours to read in the GUI.

### 7. The phone (Android emulator on the Arch PC, m7-arch)

*From Arch Chungus's trial on m7-arch (2026-11-20), its text as written. The
scripts run on m7-arch from the paths below; this repo keeps copies in
`scripts/android/` (put them back at those paths if m7-arch loses them).*

Runs on the Arch machine, not the MacBook (user, 11.12.0 run). Everything lives under
`~/.local/share/agent-work/android/` (SDK, AVD `gx-smoke-api35`, scripts); no sudo, headless,
nothing on the user's screen. Its adb server listens on port 5047 and the emulator on 5682, so it
never touches `~/Android/Sdk` or another agent's adb.

1. First time only: `~/.local/share/agent-work/android/scripts/setup-sdk.sh` (cmdline-tools,
   platform-tools, emulator, `system-images;android-35;google_apis;x86_64`, the AVD). It stops
   with the exact `usermod` line if `/dev/kvm` isn't writable.
2. The published `ghostex-android.apk` is arm64-only and **can't run on the x86_64 emulator**: it
   dies at launch in the ARM translator (`berberis: Undefined instruction 0xd5380600`, an `MRS
   ID_AA64ISAR0_EL1` CPU-feature read in `libgojni.so`, the Easy Connect bridge). Build the
   same tag for x86_64: `scripts/build-x86_64-apk.sh vX.Y.Z` (about 10 minutes cold: shared
   clones of the tag, the chat core wasm, Gradle release build, a throwaway signing key). It
   needs the gomobile bridge staged in the main checkout
   (`apps/mobile/app/modules/ghostex-native/android/{libs,src/main/jniLibs}`) and the bridge
   unchanged since it was staged (`git log <stage date>.. -- apps/mobile/tailcat-bridge`).
   Say in the report that the phone ran this x86_64 build of the tag, not the shipped file.
3. `scripts/phone-smoke.sh up` (cold boot about 3 minutes; on a first boot tap Wait on "System
   UI isn't responding"), then `GX_SMOKE_DIR=$W phone-smoke.sh install ~/.local/share/agent-work/android/apk/ghostex-x86_64-vX.Y.Z.apk`
   and `phone-smoke.sh launch`. Screenshots: `phone-smoke.sh shot <name>` (full size,
   `adb exec-out screencap -p`); find tap targets with `phone-smoke.sh ui` (the uiautomator
   dump: `text=`/`content-desc=` and `bounds=`), type with `phone-smoke.sh text '...'`.
4. **7.1 Launch.** The first screen (Ghostex, Scan the code, Use Tailscale, Use SSH details, Try
   demo experience) shows and `adb logcat -b crash` has no `io.ghostex` line. **Stop** on a crash.
5. **7.2 Connect to a throwaway test server and chat** (**Warn**; NOT RUN only if the
   pairing itself fails). See "7.2 in full" below the list.
6. **7.3 Chat (demo).** Try demo experience > Atlas API > the session; type a message, Send;
   **Pass:** the bubble shows, a reply follows, the box empties. **Warn** otherwise.
7. Clean up: `phone-smoke.sh uninstall`, `phone-smoke.sh down`. Keep the SDK and AVD.

**7.2 in full: pair the emulator with a throwaway test server, then chat.**

**The one exception to "agents never approve".** User, 2026-11-20 (Q4320, asked
whether to pair the smoke emulator each release): "i can't click approve for
this so i allow u for the smoke to click on approve using /cua-driver please".
It is narrow:
- Only during a release smoke, and only to pair **the smoke's Android emulator
  on m7-arch** with **a throwaway test server started for that run**
  (`cargo xtask test-server … --isolated-home`). Never the user's real
  gxserver or their own Ghostex window, never a real phone or computer.
- For that pairing only, the smoke agent may make the Connect a Phone code in
  that test server's own window and press its **Approve** with cua-driver,
  after the checks in step 5 below, and it removes the pairing and the test
  server afterwards.
- Everything else stays forbidden: no Approve (or any other answer) on the
  user's real app or on any card that isn't this pairing, no other pairing, no
  `computer-code` approvals, and no passwords (an admin or SSH password prompt
  ends the step).

**Which window shows the Approve.** The desktop app, not the web build. The
web build never opens the Connect a device window (`apps/gpui-web/src/app/
window/mod.rs`, "the page never opens that window") and drops approval actions
(`web_host/toasts.rs`: approving "needs the computer's own key store and
presence prompt, which a page has not"), so the 8. Linux pass's webdrive can't
do this. The installed Linux app runs as an isolated instance attached to the
test server (`GHOSTEX_HOME` and `GHOSTEX_GXSERVER_DEV_PORT`, the "isolated
desktop instances" listener in `helpers/os_cli/process_and_constants.rs`), and
that instance's Connect a device window makes the code and shows "Is this your
phone?".

Written 2026-11-20 from the code; not run yet. Fix what the first run finds.
1. **Before:** write down `grep -c 'ghostex-paired:' ~/.ssh/authorized_keys`
   and the modification times of the real HOME's tailcat key folder (`ls -ld
   ~/.local/share/tailcat ~/.config/tailcat 2>/dev/null`) into `$W`, to prove
   afterwards that the pairing wrote nothing into the real HOME.
2. **Test server with its own HOME**, in the m7-arch checkout at the release
   point: `cargo xtask test-server start --name smoke-phone --projects
   <scratch folder> --isolated-home`, then in every shell `source
   ~/.local/share/agent-work/Ghostex/test-servers/smoke-phone/env.sh` and
   check `command -v ghostex` names that server's `gxserver/bin/`. Never pair
   with a test server started without `--isolated-home`: the approval would
   write the phone's SSH key line into the user's real
   `~/.ssh/authorized_keys` and tailcat's key into the real HOME (issue
   #674). Turn Easy Connect on there: `ghostex tailcat enable` (the
   isolated HOME is given the real helper's path). Make the session to chat
   with: an agent if one starts there (the isolated HOME has no Claude or
   Codex login, so most likely not), else a terminal:
   `ghostex create-session smoke-phone-7.2 --start --project-id <id>`.
3. **The isolated app.** In a shell with that `env.sh` sourced, start the
   installed app's binary (under `/opt/ghostex`; `ls /opt/ghostex`) on a
   display the user doesn't look at: a headless nested compositor with its
   own `WAYLAND_DISPLAY` that cua-driver can drive (check on the first run;
   if none works, ask the coordinator before opening it on the user's
   screen). Check it is its own instance on the test server, not a hand-off
   to the user's running app: a new pid that stays up, and a sidebar with only
   the scratch project. If it hands off (the user's own window comes up),
   touch nothing in that window; the step is NOT RUN.
   That `env.sh` carries `GHOSTEX_OWNER_KEYS_STORE=file`, so the isolated app
   keeps its owner keys in the test server's `GHOSTEX_HOME/owner-keys/` and
   never opens the user's Secret Service keyring; check `tr '\0' '\n' <
   /proc/<pid>/environ | grep OWNER_KEYS` names it before step 4. It shows no
   polkit prompt either: approvals ask for one only when "Approvals require
   presence" is on (off in a fresh home), and then the Approve fails with
   "can't confirm it's you" (NOT RUN; never type a password).
4. **The code.** In the isolated window, open Connect a Phone (sidebar More
   Options > Connect a Phone; press each control by its `query` token). It
   turns Easy Connect on; if it asks for an admin password (SSH access off),
   close it: NOT RUN. Press `Copy the code`, read it from that display's
   clipboard (`wl-paste` with the nested `WAYLAND_DISPLAY`) into
   `$W/pair-code.txt`. On the emulator: Add a computer > Paste a code instead,
   type it with `phone-smoke.sh text '<code>'` (check with `phone-smoke.sh ui`
   that the whole code landed), continue. The phone shows Check your
   computer with three words (`phone-smoke.sh shot 7.2-words`, words from
   `ui`). The code works once for five minutes.
5. **The checks, then Approve.** The isolated window turns into "Is this your
   phone?". Before pressing anything, check all of these from a `query` read
   of that window (`get_window_state` with its own pid and window_id, full
   size), and screenshot it:
   - The window belongs to the isolated app's pid from step 3, not the user's
     app.
   - The device it names is the emulator (the name the phone app reports;
     compare `adb shell getprop ro.product.model`).
   - The computer it names is the test server's instance (same machine name
     as m7-arch, so the pid and the scratch-only sidebar are what tell it
     apart), and its three words equal the phone's.
   Then press that card's `Approve` token. If any check fails, press `Not my
   phone` instead: the step is NOT RUN, and say why.
   **When Linux accessibility can't reach the button** (the read shows the
   card's text but no `Approve` token, or no tree at all), the same
   permission covers a press by screen position, on the isolated test-server
   app only (coordinator, 2026-11-21, from the 11.13.0 release). Every check
   above still comes first, read from the card itself: from the tree if it
   has the text, else from a full-size screenshot of that window (`zoom` on
   the card): it must name the emulator, and its three words must equal the
   phone's. Then take Approve's x,y from that same screenshot of the
   isolated window (its pid and window_id from step 3), click there with
   cua-driver, and screenshot again: the card is gone and the phone moves on.
   Never a screen-position press on the user's real app or on any window
   whose pid isn't step 3's; if the card can't be read, it is NOT RUN, not a
   blind press.
6. **7.2 Pass.** The phone opens the test server's sessions ("Connected to"
   on top). Open the step 2 session, send `Reply with exactly: smoke-ok-7.2`
   to an agent, or type `echo smoke-ok-7.2` and Enter in the terminal. Pass:
   it shows on the phone with the reply or the echoed line
   (`phone-smoke.sh shot 7.2-chat`), and `ghostex read-session-chat` /
   `read-text` on the test server confirms it. **Warn** otherwise (paired but
   the chat doesn't work); NOT RUN only when the pairing itself failed.
7. **Clean up.** On the phone, remove the computer (or uninstall the app, as
   step 7 does anyway). In the isolated window remove the paired emulator
   (Settings > Remote, its row's Remove), quit the isolated app (and its
   nested compositor), `cargo xtask test-server remove smoke-phone` (closes
   its sessions and deletes its home with the isolated `authorized_keys`),
   and delete `$W/pair-code.txt`. Then repeat step 1's two reads: the real
   `authorized_keys` count and the tailcat folders must be unchanged. If
   they changed, say so first in the report.

### 8. Linux GUI pass (GPUI web build, headless, Arch)

*From Arch Chungus's trial on m7-arch (2026-11-20), its text as written. The
`webdrive` scripts run on m7-arch from the paths below; this repo keeps copies
in `scripts/linux/`.*

Steps 2, 4.1 and 5 against a throwaway gxserver and the GPUI web build in headless Chromium.
1. `cargo xtask test-server start --name <n> --projects <scratch folder>`, `cargo xtask
   web-build`, then `source …/test-servers/<n>/env.sh && ghostex web --dist-dir
   "$PWD/apps/gpui-web/www/dist" --no-open --port <free>` (backgrounded). Each run prints a
   new `#bootstrap=` key; a second tab with the same key never loads ("Sessions are taking
   longer than usual to load"): restart `ghostex web` for a new tab.
2. Create Claude threads in a project folder Claude already trusts (the repo): in a new
   folder Claude stops at its own "trust this folder" prompt, which agents don't answer.
3. Send messages by typing in the page's chat box: from an agent's shell the CLI refuses
   `agents send` and `send-session-chat-message` on the test server ("its session can't be
   identified", the 2026-11-19 guard).
4. Drive it with `~/.local/share/agent-work/Ghostex/smoke/scripts/webdrive-chrome.sh start
   <url>` and `webdrive.mjs click|type|key|shot|a11y|act` (real CDP mouse and key events;
   `a11y` reads `window.gpuiA11y`). Paste: `webdrive.mjs eval` a `ClipboardEvent('paste')` with
   a `DataTransfer` on `document.activeElement` (the headless clipboard refuses writes).
5. Known limits on this machine: about 15 s after a chat whose transcript has a reply opens,
   the page's main thread pins at 100% after "Failed to poll device during resize: Timeout"
   (SwiftShader WebGL2; the real GPU path renders blank), so read replies with `ghostex
   read-session-chat` and screenshot only empty chats, the sidebar and terminals. The web build
   has no Settings window and no Browser pane (5.1 and 4.2 NOT RUN there), and Ctrl+P goes to
   a focused terminal.

**Stop rule for the Linux pass** (Main Coordinator's decision, 2026-11-20; the
user can override it): the Mac's rule. A crash, or typing that doesn't land
(4.1's typed or pasted line, 5.4's typing), stops the release, and every other
Linux failure is a warning. The steps the web build doesn't have (4.2 Browser,
5.1 Settings, 5.2 Search palette) are reported as `NOT RUN (web build)` and
don't count as a Stop; any other `NOT RUN` on a Linux Stop step counts as a
Stop, as on the Mac. Report the Linux lines under the Mac's, each marked
`Linux`.

### Windows pass: steps 2, 4 and 5 (runs beside the Mac smoke)

*From Win Chungus's trial; its text, with the script paths changed to the
repo's.*

Tried once for real on 2026-11-20 on M7-Desktop against the installed 11.12.0.
SpaceO is macOS only, so the GUI parts run on a **throwaway portable copy** of
the app on the main display, driven with background cua-driver (0.34) only. The
user's own Ghostex window and sessions are never clicked, focused or typed into.
Scripts: `.claude\skills\ghostex-release\scripts\windows\win-*.ps1` in the
Ghostex checkout on that machine (run them from there or by full path; the
names below are those files).

**Before the steps**
- Step 2 runs on the user's real gxserver by CLI, exactly as on the Mac
  (`create-agent … --model <latest> --effort low`, `rename-session`,
  `agents send`, `read-session-chat $ref --all --last 3 --format text`;
  pass = a `· assistant ──` row with the token), then `ghostex agents close`
  both. It needs the real accounts, which the test copy doesn't have.
- Start the test copy: `win-launch-test-copy.ps1 -Root <run>\gx` (copies
  `%LOCALAPPDATA%\Ghostex\current`, or `-From <staged release build>`; adds
  `.portable`; points GHOSTEX_HOME, USERPROFILE, HOME and CODEX_HOME inside the
  run folder, so its gxserver's skill/hook installs never touch the user's
  `~\.claude` or `~\.codex`; sets `GHOSTEX_OWNER_KEYS_STORE=file`, so its
  owner keys stay in `<run>\gx\home\owner-keys` and it never reads or writes
  the user's Credential Manager, and no Windows Hello prompt is shown). It
  prints the app pid. Find the main window with
  `list_windows({pid})` (the one wider than 600).
- Run its CLI only through `win-gx.ps1 -Root <run>\gx <args>`; it strips this
  session's GHOSTEX_*/WMX_* variables, so nothing reaches the real server.
  Add a project: `add-project <run>\smoke-project --name smoke-win`.
- **GPUI takes the foreground whenever it opens or focuses a window** (app
  start, `browser open`, `settings open`, the Search palette), even when it was
  started with SW_SHOWNOACTIVATE. Wrap every such step in
  `win-keep-fg.ps1 -TestPid <pid> -Run { … }`; it hands the foreground back to
  whatever had it. Before any step that needs the foreground, wait until
  `GetLastInputInfo` shows the user idle for 5 s or more.
- Pass the same `session` label on every cua-driver call: a pixel `click`
  without an earlier `get_window_state` in that session fails with
  `screenshot_context_missing`. Screenshots at full size:
  `get_window_state({…, max_image_dimension: 0, screenshot_out_file})`;
  native size is about 1922x1230 for a 1924x1232 window, and occluded windows
  capture fine.
- What works in the background on Windows: UIA presses (Close buttons, sidebar
  rows by `element_token`), a pixel `click` (PostMessage) to move GPUI focus,
  then `type_text` with pid and window_id only (it posts WM_CHAR, "unverifiable",
  so read the result back) and `press_key` return/escape/backspace. What doesn't:
  key combinations (`hotkey ctrl+v` arrives as a plain `v`) and the sidebar's
  `Sidebar menu` button (its menu never opens from a press).

**4.1 Terminal.** `create-session smoke-4.1-runN --start --project-id $P`,
`send-text` and `send-enter`; the pane opens in the test window. Background
`click` inside the pane (focuses it), `type_text "echo smoke-keys-4.1"`,
`press_key return`. **Paste:** `win-clipboard.ps1 save <run>\clip`, then
`win-clipboard.ps1 set "echo smoke-paste-4.1"`. `right_click` in the pane opens
a "Ghostex Menu" window (Copy, Paste: empty-labelled MenuItems, Paste is the
second). Press it by token, then `win-clipboard.ps1 restore <run>\clip`. Check
the restore by saving again and comparing (the PNG matched byte for byte), then
`press_key return`. **Pass:** `read-text <id> --lines 20` shows
`smoke-ok-4.1`, `smoke-keys-4.1` and `smoke-paste-4.1`.

**4.2 Browser.** In `win-keep-fg`: `browser open https://duckduckgo.com
--project-id $P --reuse none`. Background `click` on DuckDuckGo's box (about
x 1300, y 241 in the split), `type_text "ghostex terminal"`, `press_key
return`. **Pass:** the screenshot shows every letter, and a results page loads
(the tab title and address change).

**5.1 Settings field.** In `win-keep-fg`: `win-gx … settings open --json`.
This opens window "Ghostex Settings", with search box `Edit "Search settings"`
focused. `type_text "smoke-field-5.1"` (no token). **Pass:** the Edit's
`value` reads back `smoke-field-5.1`. `press_key escape` empties it. Close
Settings with its `Close` button (UIA).

**5.2 Search boxes.** In Settings, `type_text "font"`. **Pass:** the value
reads `font` and the page list narrows (four pages). Escape. **Search
palette:** a background pixel `click` on the sidebar's search icon (about
x 30, y 84) opens "Ghostex Quick Access". It takes the foreground, and it
**closes as soon as it loses it**, so do it in one burst while the user is
idle: click, `type_text "term"` on the palette's window, `get_window_state`
(the `Search sessions...` Edit reads `term`), `press_key escape`, then give the
foreground back. That took 6.8 s on the trial; keep it short. In a test copy
whose sessions are all named `smoke-*`, type `smoke` so the list visibly
narrows to rows rather than to "No sessions match".

**5.3 Live page.** Mac-only (see "Mac-only steps and the stop rule" below);
NOT RUN on the trial. It needs an agent in the test copy to
show the page's card. Claude and Codex have no logins there, and OpenCode's
background service doesn't start in the isolated home ("serviceUnreachable",
"OpenCode's background service did not start").

**5.4 Chat box.** On the trial: open an agent's chat by pressing its sidebar
row (UIA `select`), background `click` in the box (about x 700, y 1062),
`type_text`, then `read-session-chat-queue <ref> --json` `.draft.content` holds
the text. `press_key return` empties the box and the message shows in the
transcript. Typing passed; sending failed for the reason in 5.3 (OpenCode).
The send half is Mac-only (see below); on Windows, pass is the typing half:
the draft holds the text and Return empties the box.

**Clean up.** `win-gx … kill all`, `win-gx … server stop-all`, then stop the
processes whose path is under `<run>\gx` (the app's Close leaves it running)
and delete `<run>\gx`. Close the real step 2 threads with
`ghostex agents close`. Delete the saved clipboard folder.

**Mac-only steps and the stop rule on Windows** (Main Coordinator's decision,
2026-11-20; the user can override it).
- **5.3 and the send half of 5.4 stay Mac-only.** The test copy has no agent
  that answers (no Claude or Codex logins, and OpenCode's background service
  doesn't start in its isolated home), and the Mac run covers both. Windows
  runs 5.4's typing half only. Report them as `5.3 MAC ONLY` and `5.4 PASS
  (typing; send is Mac-only)`. These are not `NOT RUN`s, so they don't count
  as a Stop.
- **Windows results use the Mac's stop rule.** A crash, or typing that doesn't
  land (4.1's typed or pasted line, 4.2's letters, 5.1, 5.2, 5.4's typing), stops
  the release. Every other Windows failure is a warning. A `NOT RUN` on a
  Windows Stop step counts as a Stop, as on the Mac. The palette half of 5.2
  needs the foreground (see 5.2 above): if the user is never idle long enough
  to run it, that is a `NOT RUN`, so wait for a quiet moment instead of
  skipping it.
- Put the Windows lines in the report under the Mac's, each marked `Windows`.

### One-time pop-ups (extras)

A release with a one-time pop-up (Meet the rail in 11.12.0) gets its screenshot
and checks without replaying the upgrade: never edit the settings file or move
the shown-once marker aside and restart (11.12.0 did that twice). Run
`ghostex debug show-modal rail-intro`; it opens the pop-up on the real window
as it shows after the upgrade, without touching its shown-once state.
`ghostex debug show-modal --list` lists the others it can open (the
remaining `native-modal-demo` previews act for real, so they open only in the
preview binary). Screenshot it at the run's window size, press
its buttons by query like any other, and close it with its own button.

### Clean up

`$S cleanup`: it closes every session the script made or you recorded with
`$S add session` (`ghostex agents close` for threads and coordinators, which
then stay in `coordinator list` as `stopped`; `ghostex kill` for terminals),
retires the 5.3 page and any you added, closes the browser tabs you recorded
(`$S add tab <port> <id>` with the DevTools target from `/json/list`: closing
the target drops the tab and the split cleanly, since the tab's x isn't in the
accessibility tree and File > Close Pane stays disabled), removes recorded
projects, puts back any Claude or Codex default not already restored by
`$S check 6.x`, and then checks none of those sessions still runs. It prints
what it removed (keep the list for the report) and exits 1 if anything was
left. Then, by hand: close the page window (its `Close` button is in the
tree), restore the clipboard, put the window frame back from
`$W/window-frame-before.txt`, and end the SpaceO session if you used one
(`spaceo_session_destroy` with `keep_apps`; a hover tooltip left on its
display blocks that until Ghostex next restarts). List what you removed in
the report (AGENTS.md: "Clean up what you create to test").

## The report

Post it to the coordinator as the thread's final message, and paste it in the
release report (step 10 of SKILL.md). One line per step, and a verdict first.

```text
Smoke test for X.Y.Z: PASS | PASS WITH WARNINGS | STOP
Release point: <commit>   App: rebuilt <time>, dev build / optimized   Run: N   Took: NN min
Typecheck: PASS, all-targets check: PASS (before the rebuild)   Models: Claude <model>, Codex <model>
2.1 PASS  smoke-ok-2.1 9 s after the send
2.2 WARN  no smoke-ok-2.2 120 s after the send (account #2 at its limit)
...
6.2 PASS
7 (phone), Windows, Linux: <their lines, or NOT RUN: why>
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
| (none at the moment) | | | |

**Dropped** (from the run reports under `~/.local/share/agent-work/Ghostex/smoke/`;
the 11.9.0 run, `3c8beb4b2-run1`, left screenshots but no report, so it isn't
counted; 11.11.0's run, `d7fcee755-run1`, was a formal STOP over 5.2's Search
palette half, so it doesn't count either, though all four checks passed in it):

| Check | Fix shipped in | Passed in (smoke-tested releases) | Dropped |
| --- | --- | --- | --- |
| 1.1 One-click answers (`f95220d0d`) | v11.8.2 | 11.10.0, 11.11.1, 11.12.0 | after 11.12.0 (2026-11-20) |
| 1.2 Restart with a coordinator chat (`4f9c7dcc6`, `71d23ed20`) | v11.8.1, v11.8.2 | 11.10.0, 11.11.1, 11.12.0 | after 11.12.0 |
| 1.3 No saved Reply draft (`dbcb1b304`, `52100362b`) | v11.8.0 | 11.10.0, 11.11.1, 11.12.0 | after 11.12.0 |
| 1.4 Ghostex's notes hidden (`275b1fc7d`, `71d23ed20`) | v11.8.0, v11.8.2 | 11.10.0, 11.11.1, 11.12.0 | after 11.12.0 |

- "Smoke-tested" counts a release whose run reached PASS or PASS WITH WARNINGS.
  A STOP run doesn't count.
- When a release is out, fill in "First smoke-tested in" for any check that was
  just run for the first time, and delete rows whose third release was this one
  (also delete the step, renumber the rest of section 1, and move the row to
  the Dropped table; sections 2 to 7 keep their numbers). The page and counts in the
  chips (`docs/2026-11-17/smoke-test/index.html`) are a snapshot; this file is
  the source.
- **Do it after the release, in its own commit** (`chore: smoke test list`),
  never in the prepare commit, which changes only `package.json` and
  `CHANGELOG.md`, and never before the dispatch, which needs a clean tree.
- A crash fixed in a release adds a step in section 1 with the same columns.
  Steps 2 to 6 are permanent: the user's list. Step 7 and the Windows and
  Linux pass are coverage the user added (2026-11-20), not crash checks, so
  they don't expire either.
- The model-change, terminal-paste and long-message fixes live in 4.1, 5.4, 6.1
  and 6.2, which stay because they are the user's permanent list.
