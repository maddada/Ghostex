# Changelog

## 8.6.0 - 2026-09-02

- Major
  - The sidebar shows one machine at a time: machine tabs sit under Search, a remote tab's cloud icon connects or retries and explains a failure in its tooltip, right-clicking a tab hides the machine or opens its settings, and Add Project, Sort & Filter, Collapse All and Edit Machine now live in the More menu.
  - Spaces end with a built-in Other space that holds every project no Space claims, replacing the All Projects view on desktop, web and mobile.
  - Claude conversations can be rewound from Session Chat: every prompt has a Rewind to here button that drives Claude Code's own restore flow, the transcript then shows only the active branch, and the rewound prompt lands back in the composer for editing.
  - The Android app gains machine tabs and Spaces, a Web Preview that opens any port listening on the computer through the SSH connection with a picker of live ports, QR scanning to pair a tailcat machine from the desktop's Remote settings, and Hide from Sessions for machines you do not want polled.
  - Model, reasoning effort and Fast mode choices in the chat composer come from a published catalog that updates without an app release, and Codex sessions gain a Fast mode pill.
  - Sessions whose agent process dies outside Ghostex, including Quit Ghostex & BG Service, a crash or a reboot, now go to sleep instead of disappearing into history, keep their chat, and wake on the next launch.
- Minor
  - Native alert dialogs such as the paste-protection and close-terminal confirmations appear again instead of invisibly holding keyboard focus, thanks to banozz.
  - Codex hooks are now marked trusted when installed, so Codex actually runs them instead of showing Installed while silently skipping them, and Settings says when a hook update is required.
  - A session started from Handoff or Export is created as a draft, so Chat View is available immediately and, when your default agent view is Chat, it opens straight into chat with the handed-off prompt in the composer.
  - Forked Claude and Codex sessions no longer share the parent's identity, so the fork and its parent stop showing the same chat and title.
  - The Session Chat context menu adds Locate File on paths and Copy URL, Open in Embedded Browser and Open in External Browser on links, and Save Image now works for large images.
  - Sleep and Wake are only offered for sessions that are actually running or sleeping, and Sleep All, Wake All and Pop Out Pane skip stopped history rows.
  - Waking a session or opening a new terminal takes one probe instead of three, session listing no longer reloads the whole registry on every request, and long zmx operations no longer stall unrelated actions.
  - The sidebar reconnects on its own with backoff after a gxserver restart, and remote machines stop refetching a full snapshot for every stale update over SSH.
  - Tailcat connections from Android 11 and newer work again, connection failures show the real cause, and newly paired machines no longer need to reach the relay map before the first tunnel byte.
  - Bundled skills install without Node or npm on the PATH, the Ghostex CLI link refuses to overwrite an unrelated ghostex or gx command, and first-launch setup waits for the daemon before reporting hook status.
  - Claude status rows are retired against the transcript so near-duplicate lines no longer linger, a /compact typed in the terminal shows the compacting card, and a sent message no longer reappears as an unsent draft on the next start.
  - The new `ghostex ports` command lists every listening TCP port on the machine, and `ghostex rewind-session-chat` drives a Claude rewind from the CLI.

## 8.5.0 - 2026-09-01

- Major
  - Cursor is now a full Chat View agent: sessions read their model, context window, reasoning effort and Fast mode live off the terminal, thinking shows as an activity card, Cursor's own questions are answered inline with Space and Enter, and transcripts export like any other agent.
  - Remote machines can be reached without a Tailscale account: gxserver runs its own tailcat tunnel, owns its key and address, comes back automatically after a restart, and is set up from the Remote tab in Settings.
- Minor
  - Cursor, Grok, Hermes, OMP and Pi sessions now name the screen that is blocking the composer — a picker, approval, sign-in or editor — instead of silently swallowing the message.
  - Codex and Claude always launch with their unattended permission flag, so turning Accept All off no longer leaves them stuck on an approval prompt nobody can answer.
  - Resuming or forking a Codex or Claude session no longer replays a stale session id from an earlier one-time command.
  - Session notes are edited in the same Monaco editor as the composer.
  - File and skill references appear as pills in the mobile and embedded chat composers instead of raw Markdown, and copy back out as plain text.
  - Opening a file at a specific line in an already-running Code tab works again, and falls back to copying the path with a notice when it cannot.
  - A single click on the collapsed command pane expands it, and the double-click-for-a-new-terminal gesture now applies only once it is open.
  - Close After Done and Sleep Inactive only sleep sessions that are actually running, so pinned, tagged and favorited sessions are no longer swept away.
  - The sidebar has symmetric gutters, separators that run its full width, and a plain gap against the workspace in place of the stacked separator strip.
  - The mobile chat view can switch an unprompted draft session's agent and send Enter or a shifted option key without typing into the terminal.
  - The transcript no longer shows a second typing indicator while the agent works, since the pinned working strip already reports it.

## 8.4.0 - 2026-08-31

- Major
  - Session Chat now shows what the agent's terminal is doing right now: compaction and other live activity appear as a card pinned above the composer, stay visible wherever the transcript is scrolled, and clear on their own when the agent goes idle.
  - Codex sessions that open a chooser — model, effort, permissions, approvals, or any other numbered picker — now say so in chat with the choice on screen, instead of refusing your message with a generic "input box is not on screen" error.
- Minor
  - The `ghostex` and `gx` commands keep working after an app update; a packaged launch refreshes command wrappers that were still pointing at the previous install's binaries.
  - Close After Done sits beside Delayed Send in the session menus on desktop, web, and the terminal action bar, and both countdowns tick live instead of only moving when the session refreshes.
  - First launch asks whether the terminal view should use the full pane or match the chat width, and new installs now default to full width.
  - The agents tab context menu opens with Rename and Sleep for the session you clicked, above the scoped Sleep Right, Sleep Left, and Sleep Others actions.
  - File references in the transcript read as ordinary links, and composer reference pills show their full path in a tooltip rather than a native title popup.
  - Installed extensions show their author, and version labels are written the same way across the Store, your installed list, and Settings.
  - Hermes sessions running under a named profile are recognized again, so chat no longer refuses every send while the composer is plainly on screen, thanks to @banozz.
  - Agent commands that hop to another user or host, such as through `ssh`, no longer leave the agent CLI wedged in an editor at launch, thanks to @banozz.
  - Project Board `start-work` dispatches the worker into the project you name instead of whichever project was touched most recently, thanks to @banozz.

## 8.3.0 - 2026-08-30

- Major
  - Hermes Agent is now a first-class Session Chat agent, with its own transcript reader, model pill, welcome art, automatic session titles, hook allowlisting, and handoff hints.
  - Agent questions asked by Pi, Hermes, and oh-my-pi are answerable straight from the chat card, each driven through that CLI's own picker, alongside the existing Claude and Codex support.
  - Settings has a single Extensions page: the built-in features Ghostex ships sit above the extension store and your installed list, replacing the separate Customize page and the standalone Extensions window, and every menu, shortcut, and command-palette entry now opens it.
  - A remote machine's project Actions load in the sidebar and run in the desktop command pane, so remote work no longer has to be started from a terminal by hand.
- Minor
  - Session Chat can give the transcript its own width, and the terminal view can use the full pane, match the chat width, or take an independent width of its own.
  - Terminal notices can be answered with number keys, single-select questions submit as soon as you pick an option, and Claude's model-switch and paused pickers are answerable from chat.
  - The composer's model and options pills stay as loading placeholders until the agent's screen is actually readable, instead of flashing empty labels while a CLI is still starting.
  - Files and folders dragged from the desktop attach through their native paths, so local drops land as real attachments rather than unresolved names.
  - Composer reference pills stay atomic, wrap with their own text, and no longer leak into the message the agent receives; pasting long text into the composer is noticeably faster.
  - Empty draft sessions now stay until you delete or send them, instead of disappearing on their own.
  - New installs open with an ice-tinted app chrome and a lighter accent color, and Accent Color now also marks advanced rows in Settings.
  - First-launch setup finishes only when your first project starts — Escape and the close button no longer end it halfway — and its Skills step sets up the Ghostex CLI for you when a selected skill needs it.
  - Chinese, Japanese, and Korean input works properly in the terminal: the composition caret is drawn in place, and Backspace edits the composition instead of deleting text behind it.
  - Opening a file in Code from agent chat or the Agents Hub is queued until the editor is ready, instead of failing when the request beats the workbench.
  - Spaces respond to trackpad swipes, and the Space switcher and its overflow menu are rounder and easier to hit; Settings moved into the sidebar's More menu.
  - Prompt search no longer fills up with the title, commit-message, and sub-agent scaffolding turns that agents write into the same transcripts.
  - Kanban card conversation links display again on large shared boards, and jumping to a dead linked session resumes the agent conversation rather than an empty shell.
  - Custom agents keep their activity across identity updates and resume through their own CLI family.
  - Saved Prompts lets you change tags and favorites on prompts you already saved, not only while saving them.
  - Session Chat pins an agent-working indicator above the composer, so a running turn stays visible no matter where the transcript is scrolled.
  - Interrupting a Codex session no longer trips a hook-output error, because the Ghostex notify hook now returns the response Codex's Interrupt event accepts.
  - Long automation prompts stay inside their detail cards, chat surfaces no longer show stray browser focus rings, and child windows open centered on the main display.
  - The Terminal View button previews the agent's CLI on hover with a readable tail, and Summary mode shows its state as its own icon.
  - Markdown ordered lists keep the numbering the agent wrote instead of being merged into one run.
  - macOS 26 and later no longer crash the zmx daemon on exit, and Codex session files resolve correctly on macOS.
  - Android and iOS gain the custom transcript width toggle; on iOS the chat view loads reliably again and Send & Attach File opens the document picker as expected.

## 8.2.0 - 2026-08-28

- Major
  - Spaces let you organize projects into named working sets and filter each local or remote gxserver independently, while machine tabs make it quick to move between this computer and connected machines without losing each machine's selected Space.
  - New agent sessions can begin as durable drafts: choose the agent, model, effort, view, and first prompt before Ghostex launches a terminal, then recover the draft across desktop, web, and Android after navigation, reconnects, or crashes.
  - Forked conversations now appear as a family across Session Chat and Previous Sessions, with branch badges and an in-chat switcher that follows successor sessions without losing the shared transcript.
  - Arch Linux users now get an official, hash-verified `ghostex-8.2.0-linux-x64.tar.zst` package alongside the DEB and RPM downloads; its prefix-preserving layout installs Ghostex under `/opt` with the `ghostex` and `gx` commands on `PATH`.
- Minor
  - Session Chat preserves atomic file references in Monaco, keeps synced drafts until delivery is confirmed, restores unsent Saved Prompt drafts, shows clearer terminal readiness and refusal feedback, and presents compaction summaries and file-open paths more clearly.
  - Agent and remote-machine settings are simpler to scan and configure, terminal width can apply to command panes, and sidebar tooltip delay is adjustable down to instant display.
  - Docs can delete folders, style supported documents, and include temporary project files that were previously hidden.
  - Quick Access includes already-open projects and offers direct Close and Remove actions, while sidebar grouping, active-session state, and empty states stay consistent across local and remote sections.
  - Export Transcript is now Handoff, with a clearer agent action bar and shortcuts for carrying a session into another agent or conversation.
  - Extensions can open approved remote URL surfaces while preserving the extension host's launch, consent, and bridge boundaries.
  - Quitting the desktop app can shut down the full Ghostex service stack, including gxserver and its managed sessions, when you explicitly choose that option.
  - Usage analytics now has a complete public field inventory, immediate opt-out controls, and clear disclosure of the one-way hashed identity used to group a person's own machines; retired sidebar-layout dimensions are no longer reported.
  - Titlebar dropdowns keep their final row visible and return focus reliably, empty command-pane titlebars explain their controls, and action tooltips keep their intended spacing and hit area.
  - Android preserves sent drafts across navigation, adds a default agent-view preference, and uses the same Handoff terminology as the desktop and web apps.

## 8.1.0 - 2026-08-27

- Major
  - Extensions now run as first-class Ghostex experiences: install hash-verified packages from the redesigned Store, pin live titlebar launchers, and open extension views in the chat bar, terminal panes, popups, or app modals, with gxserver managing their processes and lifecycle.
  - Session Chat can attach multiple images, files, and whole folders as compact pills, walk dropped folders, open file references at the right line in Docs or Code, save transcript images to Downloads, and reach the agent's model, effort, delayed actions, transcript export, terminal handoff, and other session controls without leaving the conversation.
  - The embedded Ghostty engine moves to a substantially newer upstream build that cuts terminal memory by roughly 75% in its libghostty path, releases GPU resources for hidden surfaces, accelerates terminal string parsing, and improves Kitty graphics, clipboard, paste, Unicode, and IME behavior.
- Minor
  - Kimi, Campfire, OpenClaude, Command Code, and Devin join the built-in agent catalog with their own icons, status recognition, launch and resume behavior, and title cleanup; Settings can choose Chat or Terminal per agent and guides you through installing required hooks before a supported agent opens in Chat.
  - Agent terminals now share a focused bottom action bar across desktop and web, can be centered at a configurable width with horizontal and vertical padding, and can apply that layout to command and editor companion terminals too.
  - Previous Sessions shows transcript sizes, session rows surface note and Saved Prompt counts, and prompt search produces more relevant matches and clearer excerpts while ignoring empty Cursor projects and injected envelopes.
  - Session and project actions are grouped into clearer menus, app dialogs share one rounded modal shell, Docs rename uses that shell, and Quick Access, Add Project, missing-folder, extension, and titlebar panels have more consistent spacing and labels.
  - Parked browser, chat, command, and terminal runtimes restore more reliably, terminal writers are serialized to prevent dropped input, forks wait for their composers, F12 minimizes a focused command pane, and agent status stays tied to the session's actual agent.
  - Linux embeds its browser views as native Alloy child windows and stages the Code runtime on demand; Arch Linux is included through the official prefix-preserving `ghostex-8.1.0-linux-x64.tar.zst` package alongside the DEB and RPM downloads.
  - Android gains streamed macOS uploads, machine-level session menus, a kill-session action, Saved Prompt draft handoff, stable session collection ordering, and refreshed adaptive icons.
  - Anonymous usage analytics now report only coarse OS, feature, and count data, never prompts, paths, project names, or personal content, and can be disabled immediately in Settings > Privacy.
  - File drops can be saved as message markdown, terminal media opens with the operating system, browser home URLs persist with project snapshots, remote Saved Prompts keep their tag IDs, and transcript exports and project pickers correctly correlate concurrent requests.

## 8.0.0 - 2026-08-25

- Major
  - Every session can now carry a note — what you were doing, what to pick up next — attached to the agent's own conversation so it survives closing, resuming, and compaction, shown as a dot with a hover preview on the session row across the desktop, web, and Android apps, editable from the chat composer and the session menu, and readable from `ghostex session-note`.
  - Stashed Prompts is now Saved Prompts and gained colored tags, a tag rail with an Untagged filter, and a Go to session action that takes you back to the conversation a prompt came from; saved prompts stay attached to that conversation even after the agent compacts it, Alt+S saves what is in the composer, and Cmd+Alt+S opens the list.
  - Sessions you are not working on right now can be parked instead of closed: they leave the active list, collect in a collapsible Parked section at the bottom of their project, and keep their terminal — turn it on with Enable session parking.
  - Scheduled automations now resume the durable agent session they created instead of starting a new conversation on every run, so a recurring job keeps its context, and an agent can link the session it is working in to the board card it came from.
  - Session Chat shows the sub-agents Claude is running as their own strip with per-agent status and token counters, reports what the terminal is doing while a turn is in flight, and shows Claude's current permission mode as a pill you can change from the composer.
  - Prompt search opens as its own window over whatever you were doing, on Cmd+Shift+F, instead of taking over the focused terminal, and now finds text inside prompts you pasted into Claude Code rather than only the collapsed "Pasted text" marker.
  - Closing browser panes, chat surfaces, and terminals no longer leaves their renderer processes behind, so a long day of work releases memory instead of climbing into tens of gigabytes, and a chat surface you have not looked at for twenty minutes is unloaded and restored the moment you return to it.
  - First launch now walks through a six-step setup that skips the steps your machine already satisfies and ends by creating your first project and session, so a new install is usable without a trip through Settings.
  - Ghostex now has an extensions system with a Store and Installed browser, audited hash-verified packages, a titlebar launcher with pinnable live-badge icons, and view, chat-bar, terminal-pane, popup, and modal placements; the first catalog includes Storybook, Session Scratchpad, Lazygit, btop, Claude Usage, and Git extensions.
  - The Linux app draws the same integrated titlebar and window controls as Windows and macOS, and Linux downloads now include a portable `.tar.zst` archive for Arch Linux and other distributions alongside the DEB and RPM packages.
  - Ghostex ships a refreshed look: a new app icon, darker app chrome, one toggle and focus-ring shape across every control, restyled Kanban, Automate, and Settings surfaces, and new defaults for fresh installs — Session Chat as the agent interface, sessions grouped by project in the sidebar, and colored agent logos.
- Minor
  - Exporting a transcript now asks what to include first — the commands the agent ran, the patches it applied, and its reasoning — instead of always writing everything.
  - Trycua can be installed from Settings on Windows and Linux, not only macOS, and Settings shows the exact command the button runs.
  - Ghostex's bundled agent skills are consolidated behind one Ghostex CLI skill that covers orchestration, prompt-history search, and automations, and Project Board Beads is installable alongside it.
  - A session that runs `/compact` no longer gets stuck reporting work forever, so its queued prompts keep being delivered.
  - A Delayed Send now delivers the message staged in the chat composer instead of pressing Enter on an empty line and dropping it.
  - Large tool outputs no longer disappear from the middle of a transcript, and very long ones are shortened for display with the full text still one click away in the file.
  - Scrolling up in a chat keeps loading older messages on its own instead of stopping at a Load earlier button, and jumping back to the newest message animates instead of snapping.
  - A session forked with `codex fork` keeps its chat instead of freezing on the conversation it was forked from.
  - A terminal title that only repeats the working directory no longer becomes the session's name, and Grok's status prefixes are stripped out of generated titles.
  - Renaming a session from chat now shows the name the agent picked as its own card in the conversation, and a command you type in the composer reads as your own turn instead of a system line.
  - Claude's rotating status markers are recognized again, so live status shows while it works, and the messages it prints when it rejects a prompt — an unknown command, for example — now appear in the chat instead of the prompt silently vanishing.
  - Images in a transcript render as compact thumbnails with a copy-path action on the thumbnail and in the preview, and Codex's generated images show the same way.
  - Typing in the chat composer no longer loses the keyboard to the terminal behind it, and a chat draft is kept until the terminal confirms it was pasted.
  - Ghostex's own bundled command-line tools are recognized on Linux, so the CLI installs and repairs correctly there.
  - Sending a prompt now waits for the terminal to confirm it was queued, so a message can no longer be reported as sent while it was dropped.
  - The macOS titlebar zooms on a double-click, the traffic lights and project name sit at a comfortable inset, and workarea switch hotkeys work from every surface.
  - Resources now attributes an orphaned listener to the project that owns it.
  - Browser and editor panes wait ten minutes before sleeping instead of five, and an agent terminal that has never been prompted is never put to sleep.
  - Card association from an agent session contributed by [@banozz](https://github.com/banozz).

## 7.13.0 - 2026-08-22

- Major
  - Prompts can now be queued from the chat composer with Tab or a long press on Send, and Ghostex delivers one each time the agent stops — with every client closed, the phone locked, or the desktop app quit — so you can line up a session's next few turns and walk away; queued prompts can be edited, sent immediately, deleted, or reordered, the sidebar and the terminal show how many are waiting, and a session with a queue is never put to sleep automatically.
  - Your unsent chat draft now follows you between the desktop, web, and Android apps, and a newer draft from another device offers itself in a bar above the composer instead of overwriting what you were typing.
  - A new Find Prompts surface, opened with Alt+F or from the command palette, searches every prompt this machine has ever sent to an agent — across Claude Code, Codex, grok, Pi, OpenCode, and Cursor Agent — and resumes or forks the session it came from, on the desktop, web, and Android apps, sharing its index and starred prompts with `gx f` in the terminal.
  - grok sessions can now be opened in Session Chat, which follows its conversation, tool calls, and results as they happen, reads the model and reasoning effort off its statusline, and exports its transcript.
  - The project board now draws your own custom bd statuses as their own columns instead of showing those cards as fresh work, and columns can be added, renamed, and removed from the board itself.
  - Adding a project and cloning a repository are now one dialog: paste a repository or pick a folder, and a confirmation step shows where the clone will land, warns when the destination is already taken, and carries the branch name, main-only, and shallow options through to the clone.
- Minor
  - Cmd+K clears the focused terminal on macOS the way Ghostty's own binding does, dropping the scrollback and then the rows above the cursor so a half-typed command line lifts to the top, and Settings names the chord as reserved rather than letting a command take it.
  - The chat transcript is set in one deliberate type scale instead of four sizes with per-host line heights, and opening chat no longer flashes a lighter panel before the conversation appears.
  - Fenced code in a conversation is syntax highlighted, GitHub-style alerts render as callouts, and tables, file chips, and collapsible details read properly.
  - A reasoning turn is now headed by its own opening line instead of a column of identical "Thinking" rows.
  - The transcript shows what Claude Code is doing while it works — the status line it repaints on its terminal, and compaction with its progress — instead of staying empty until the turn lands.
  - An on-screen picker the agent puts up, such as Claude Code's resume chooser, can now be answered from the chat surface, and it delivers the row you actually chose.
  - The chat composer has a cut, copy, paste, and select-all context menu.
  - The model and effort pills no longer sit under a loading skeleton forever on a session whose agent names neither, or on a session that has stopped.
  - A message the agent CLI handles itself, such as `/usage` or `!ls`, is no longer reported as having failed to reach the agent.
  - One "Open links in" setting under Browser now decides where every web link an agent sends opens — terminal Command-clicks, chat links, and detected dev servers — replacing two settings that answered the same question with opposite defaults.
  - Agent menus mark every agent whose sessions can be opened in Session Chat, on the desktop and Android apps.
  - A custom agent built on a supported provider now keeps that provider's icon on its sessions and can be opened in Session Chat.
  - Browser tabs show the page's favicon in the sidebar, and a page that asks to close its own tab now closes it.
  - The project board can be filtered by tag alongside priority and estimate, and the choice follows you into every project.
  - Android loads Session Chat from assets bundled in the app, keeps Send Answer above the software keyboard, opens transcript search from the terminal overflow menu, and shows the sessions list with the same project rails, icons, tags, and grouping as the desktop sidebar.
  - Creating a terminal on a remote machine now checks what that machine's gxserver can actually do and says which side needs updating, instead of failing with the daemon's own error text.
  - Sidebar hover styling works again after switching back to the window, and tooltips dismiss when the pointer leaves.
  - A project ticket taller than the window now scrolls inside it instead of losing its top and bottom past the screen edges.
  - The Delayed Send dialog no longer sits low in its own window.
  - The Stashed Prompts search field takes focus when the dialog opens.
  - Browser Profiles is a normal Browser feature now rather than an experimental one.

## 7.12.0 - 2026-08-20

- Major
  - A new Export Transcript agent action writes the session's conversation to a markdown file — your messages, the agent's replies, the commands it ran with their output, and the files it changed — and offers to start a fresh conversation that already mentions the export, so you can hand a session's context to another agent without copying anything by hand. Available on the desktop, web, and Android apps and from the `ghostex` command line.
- Minor
  - Automatic sleep no longer retires a session you are looking at, including one showing its chat surface with the terminal parked behind it, or a session with a Delayed Send still waiting to fire.
  - Removing an installed agent hook or bundled skill now happens in Settings > Agents next to the control that installed it, with a remove action on each row and one Uninstall All for the whole set.
  - Remote settings now labels a saved machine's action as Install or Update depending on whether it already runs gxserver, and shows the installed version on the same row.
  - The project board now explains a refused action — closing an issue that still has open blockers or children, an impossible dependency, an issue id that does not exist — and offers the step that resolves it, instead of reporting the board as unavailable and telling you to reinstall Beads.
  - The Code tab is now disabled for remote projects, where it would have opened this machine's files rather than the remote ones, and offers to copy the remote path instead.
  - The session persistence warning in Tips now opens the settings page that actually holds that setting.
  - Desktop Control settings now reads the real accessibility and screen recording permission state instead of reporting it incorrectly.
  - The sidebar's scroll fade now settles correctly after a project or collection finishes animating open.

## 7.11.0 - 2026-08-19

- Major
  - Chat now tells you when the agent's terminal needs you, showing a notice for the CLI screen that is waiting and flagging a message the CLI swallowed instead of leaving it silently unanswered.
  - Back and forward navigation moves through recently visited sessions from the sidebar buttons or a hotkey, on both the desktop and web apps.
  - The chat composer can now attach project files by typing `@` and picking from the session's project, with the picker opening wherever the mention is typed in the draft.
  - The Monaco prompt editor now ships inside the macOS app, so Ctrl+G opens it on installed builds instead of quietly falling back to the machine's default terminal editor.
  - A new Command Pane Side setting docks the command pane as a full-height column to the right of the workspace instead of a strip along the bottom, remembering its own width per project.
- Minor
  - Programs running in the terminal can copy to the system clipboard again.
  - Switching a session from the terminal to chat now carries whatever you had typed into the agent CLI over to the chat composer, on the desktop and Android apps.
  - Pictures in a conversation can now be saved to disk from the chat image viewer.
  - Long thinking blocks are capped with an expand toggle, so a single reasoning dump no longer owns the whole transcript scroll.
  - An agent's background task notifications now read as one status row with its result, instead of a block of raw tags.
  - The chat composer no longer behaves like a code editor, and F1 opens its command palette instead of typing a stray character.
  - Sessions you are viewing from your phone are no longer put to sleep by the owning machine's automatic sleep sweep.
  - Reconnect on Android now replaces the dead terminal instead of reporting success while the exited process stayed on screen.
  - The Add Project dialog can create a new folder in place and start the project in it.
  - Renaming a Claude Code session from Ghostex now updates the sidebar card as soon as Claude confirms the new name.
  - Settings sorts the bundled agent skills into recommended and optional, adds a Ghostex Computer Use skill, and offers the one-time Cua Driver setup the machine-automation skills need.
  - First launch now shows a single setup flow that opens with the tutorial video, instead of a separate video window followed by setup.
  - Notifications stay attached to the Ghostex window instead of floating above whatever app you switch to.
  - Opening a link from a browser page in a new tab now reveals that tab's row in the sidebar, including background middle-click tabs.
  - Launching an agent session on a remote machine works again instead of reporting a failure and dropping the workflow prompt.
  - Resources now reports process memory the way Activity Monitor does and lists one row per dev server, so its totals stop being inflated.
  - Delayed Send in the web app now runs on the machine hosting the session, matching the desktop and phone apps.
  - Project bodies and sidebar sections animate open and closed again instead of snapping shut.
  - Chat no longer shows prompts that were revised or re-sent before the agent replied, so the conversation matches what the agent actually ran.
  - Sending a chat message to a resumed Claude session now answers its resume-usage prompt first, so the message reaches the agent instead of being swallowed as menu keystrokes.
  - Clicking a picture in chat now toggles between fitted and full size, zooming to the spot you clicked and panning by scroll.
  - Interactive prompt cards now ask one question at a time with numbered shortcut keys, a collapsible header, and a free-text answer row in the composer's place.
  - Sending a chat message scrolls it into view without leaving an empty gap above the composer.
  - Copying one of your own chat turns now includes its image references, so pasting it into another composer still attaches the same pictures.
  - The sidebar session filter menu gained a No tag row for isolating sessions that carry no marker.
  - Sessions from agents that use half-circle spinner glyphs now report working status correctly and keep those glyphs out of the session title.
  - The Docs pane in Manage now fills the full height of the pane and follows the shared app theme.

## 7.10.0 - 2026-08-17

- Major
  - Delayed Send is now owned by the machine hosting the session, so timers and agent-stop triggers keep counting down and still fire after Ghostex is closed or reconnected, including for remote sessions.
- Minor
  - Agent sessions set to open in Chat now start directly in Chat with the composer focused instead of briefly showing the terminal first.
  - Clicking inside a chat now moves keyboard focus there instead of leaving typed keys going to the terminal behind it.
  - Narrow chat panes now place session options and prompt actions on separate rows and widen chat search, so the controls stay reachable instead of crowding each other.
  - Android chat now uses the full screen width and reserves Enter for new lines, keeping sending a deliberate tap.
  - Opening an existing conversation no longer flashes the new-session welcome screen while the transcript loads.

## 7.9.1 - 2026-08-16

- Major
  - Windows upgrades and same-version repairs now install without the extra already-installed confirmation and reopen Ghostex automatically.
- Minor
  - Windows keeps downloaded runtime components across installer replacement and fetches them with its built-in HTTPS stack instead of relying on a system curl executable.

## 7.9.0 - 2026-08-16

- Major
  - Remote terminals now reconnect automatically through network interruptions and system sleep, preserving the remote session and scrollback instead of dropping to a local shell, thanks to @NiTE.
- Minor
  - Session Chat adds a per-session Verbose control that remembers whether thinking and tool activity should start expanded, thanks to @banozz.
  - Default Agent View now switches compatible agents into Chat automatically, remembers each session's chosen view across restarts, and keeps the terminal live in the background.
  - Session Chat detects model and reasoning options during agent startup instead of leaving them blank until a later refresh.
  - Supported agents launched inside an existing terminal are recognized as soon as their identity appears, making Chat available without reopening the session.
  - Chat and terminal action tooltips now show the configured keyboard shortcuts for faster discovery.
  - Large sidebars reduce CPU and remote traffic by rate-limiting project Git-stat checks and updating only rows whose state changed.
  - Remote project groups show only the actually focused session instead of also exposing an unrelated first session, thanks to @NiTE.
  - GitHub releases now link directly to every customer installer and explain how to request the iOS TestFlight through the Ghostex Discord.

## 7.8.0 - 2026-08-15

- Major
  - Ghostex now uses the latest upstream Ghostty terminal engine to improve terminal performance while lowering RAM and CPU use.
  - Session Chat can search the full conversation and jump between matching results on desktop, web, and Android.
  - Session Chat can mention installed agent skills by typing `$` and choosing from the session's available skills.
  - Session Chat now puts Claude and Codex model, reasoning effort, Fast Mode, permission mode, and Plan mode controls directly in the chat workflow.
  - Automations can run once after a countdown timer or at a chosen date and time.
  - Recent Projects can open a real project-scoped terminal on a connected remote machine.
  - Project Board can initialize, update, and migrate Beads in the connected environment.
- Minor
  - The Ghostty update fixes a terminal crash that could occur when hyperlinks reflowed.
  - Windows ARM64 now uses an optimized native Ghostty terminal build.
  - Session Chat makes agent activity easier to scan with grouped thinking, compact tool summaries, and optional Verbose Mode.
  - Session Chat appearance settings now cover theme, custom font, and transcript width on desktop and Android.
  - Session Chat drafts can be stashed directly from the composer.
  - Failed Session Chat sends restore the unsent message and show a clear error.
  - Accepted Session Chat sends clear the composer and show the working state immediately.
  - Session Chat keeps keyboard focus and Select All inside the composer on the desktop app.
  - Expansion for sidebar collections and session groups stays local to each window and device.
  - Sidebar item tooltips wait longer before appearing so they do not interrupt normal navigation.
  - Toggle Chat View now defaults to `Alt+G`, and customized hotkeys can be reset individually.
  - Manually launched Codex sessions reconnect to the correct conversation when process detection is delayed.
  - macOS no longer lets Chromium clone the app bundle on every launch and leave gigabytes of temporary data behind.
  - On-demand component cleanup now removes obsolete versions and abandoned installation artifacts.
  - Source mode is disabled for remote projects where it cannot operate correctly.
  - One-shot automations preserve their deadline across restarts and disable themselves after the due run is queued.

## 7.7.1 - 2026-08-13

- Major
  - Ghostex releases are now planned around what actually changed, so updates are published faster and reach you sooner after a fix lands.
- Minor
  - Each release now includes a published record of how every download was produced, so a build can be traced back to its exact source.
  - Release problems are detected before packaging begins, which reduces the chance of an incomplete or inconsistent update.
  - The project README presents Ghostex more clearly for people evaluating it for the first time.

## 7.7.0 - 2026-08-13

- Major
  - Windows now guides first-time setup by checking WSL, helping you choose a distribution, and preparing the Ghostex runtime before opening the workspace.
  - Remote machines reconnect more reliably after sleep, wake, tunnel interruptions, and temporary network failures, while remote session renames stay synchronized.
  - Command panes restore their linked sessions and Action tabs more faithfully, including live working and completion status when you return to the app.
- Minor
  - On-demand component installs now show clearer checking, download progress, and size information instead of an indeterminate wait.
  - Project Board uses the Beads installation from each local, Linux, or WSL environment and provides clearer installation or migration guidance when attention is needed.
  - First-launch setup, recent-project recovery, Session Chat controls, titlebar customization, and Windows terminal startup are more dependable.

## 7.6.0 - 2026-08-12

- Major
  - Docs can mount an additional folder beside each project's own files, with clear copyable paths for everything in that collection, thanks to @banozz.
  - Worktrees can rename both their folder and branch directly from the sidebar, thanks to @banozz.
  - Project sessions are organized into collapsible Browser, Pinned, and Sessions sections so busy workspaces stay easier to scan.
- Minor
  - Session Chat presents tool activity and hidden work more clearly, keeps long output manageable, and adds a focused copy action to each final response.
  - Browser panes can open their current page in the system browser, and titlebar customization is easier to reach from its context menu.
  - Windows runtime setup, WSL terminal integration, session recovery, and on-demand component handling are more dependable.

## 7.5.0 - 2026-08-10

- Major
  - Session Chat now protects unfinished drafts, moves prompts safely between chat and terminal, detects agent model and effort details, and keeps completed work compact until you choose to expand it.
  - Project Board can start work with the agent assigned to a ticket, making it faster to move directly from planning into the right conversation, thanks to @banozz.
- Minor
  - Docs now discovers files in artifact and AI folders by default and handles find, replace, redo, and common editing shortcuts more naturally.
  - Quick Access and sidebar modals rank results more clearly, preserve pinned-section boundaries, group stashed prompts by day, and make keyboard selection more dependable.
  - Settings adds a preferred interface for newly launched agents, chat appearance controls, optional titlebar chrome, and more reliable plugin reinstalls.
  - Focused terminals can be zoomed more easily, while Windows runtime startup, on-demand components, and WSL packaging are more dependable.
  - The Android app adds light and dark Session Chat themes and a clearer outline around the active session.

## 7.4.0 - 2026-08-08

- Major
  - Project Board now remembers your filters and sorting, offers both sort directions, shows ticket creators and assignees, and can resume linked conversations even after project or ticket names change.
  - Quick Access can progressively load older sessions, while command panes and existing agent conversations reconnect more reliably when you return to them.
- Minor
  - Global Actions are available directly from project rows and refresh immediately after changes, thanks to @banozz.
  - Stashed Prompts, Previous Sessions, project collections, session groups, and sidebar drag-and-drop have clearer controls and more dependable behavior.
  - Project Board scrollbars can be clicked and dragged normally, and completed lanes default to showing the newest work first, thanks to @banozz.
  - Session Chat loads images and completed Codex messages more smoothly, while the Android chat stays ready in the background and handles the on-screen keyboard more reliably.
  - Windows terminal sessions, agent hooks, remote cloning, project icons, support diagnostics, and long-running gxserver connections are more dependable.

## 7.3.0 - 2026-08-07

- Major
  - Quick Access now brings the Command Pane, Recent Projects, and Recent Sessions into one fast, tabbed workflow with consistent search and keyboard shortcuts.
  - Remote projects can open Source and Prompt Editor through their connected machine, while Docs can browse and manage files on that remote workspace.
- Minor
  - Remote machines can be reordered directly in the sidebar, sidebar sections collapse more smoothly, and active sessions stay clearly visible through collapsed groups.
  - Previous-session search preserves exact word and phrase matches, and choosing an already-running conversation focuses its existing terminal instead of opening a duplicate agent.
  - Windows browser keyboard input and popup focus are more reliable, while menus, tooltips, and scrollbars have cleaner sizing and placement across the desktop app.
  - Terminal selection, agent waits, prompt delivery, support logs, and live session tracking are more dependable during longer-running work.
  - The Android app shows useful working, attention, and awake counts when machine and project sections are collapsed.

## 7.2.0 - 2026-08-06

- Major
  - Switching between projects and machines now preserves each workspace's live terminal panes, tabs, scrollback, and chat state, so returning to a project restores the view exactly where you left it without dropping sessions.
  - Project groups now offer quiet, header, and branched visual styles, a broader color palette across desktop and Android, and a new CLI command that can create a group or move a project into one by name.
- Minor
  - Remote projects can create and open workspace terminals more reliably from the desktop app, with safer connection and session validation.
  - Existing installations get a safer storage migration that keeps legacy gxserver and log paths working while moving data into the operating system's standard locations.
  - Web terminals keep their connections mounted across tab and chat switches while keyboard focus follows only the active terminal.
  - OMP sessions now keep the correct conversation identity, show cleaner live titles, and use refreshed full-color branding across desktop and Android.
  - Popup menus on macOS stay open reliably when invoked from the active Ghostex window.

## 7.1.0 - 2026-08-05

- Major
  - Ghostex now keeps settings, app data, caches, logs, and runtime files in the standard locations for each operating system, automatically migrating existing installations while preserving compatibility with saved prompts, attachments, agent hooks, and bundled tools.
- Minor
  - Stashed prompts save and refresh more reliably, including prompts tied to a specific project or session.
  - New sidebar opacity controls let you tune group and project surfaces independently on desktop and Android.
  - Remote chat attachments and pasted images consistently use the connected machine's configured Ghostex storage location.
  - Tooltips and compact controls are clearer and more consistent across the desktop workspace, Project Board, Settings, and the web app.

## 7.0.0 - 2026-08-04

- Major
  - Ghostex for Windows now uses installable x64 and ARM64 packages with built-in automatic updates from GitHub Releases; existing 6.x Windows users need to install 7.0.0 once to move onto the new updater.
  - Session Automations can wait for agents to finish before sending a prompt, while missing project folders are detected and can be relocated without losing the project setup.
  - Session Chat adds attach-anything uploads, larger image previews, broader agent transcript support, continuation tracking, and clearer live model and action state across desktop, web, and Android.
  - Ghostex can download large app components only when needed and provides a dedicated Plugins window, reducing the size of the core desktop installation.
- Minor
  - Existing Codex and other agent integrations automatically repair Ghostex hook paths after the storage-folder migration, preventing PreToolUse and UserPromptSubmit hook failures.
  - Global Actions can appear directly in the tab strip, and Global Defaults can configure common project settings once for every project, thanks to @banozz.
  - Remote project collections, flexible GitHub clone inputs, terminal background images, multi-monitor popup placement, session reconciliation, and Windows startup reliability make everyday workspace management steadier, with Windows startup improvements from @yossifyahya16.
  - The Android app adds Session Automations, chat attachment uploads, a simpler unified terminal menu, clearer quick actions, and the latest session-status improvements.

## 6.13.0 - 2026-08-01

- Major
  - Session Chat lets you read and reply to agent conversations directly from Ghostex on desktop, web, and Android without switching back to the terminal.
  - Chat conversations update live with complete transcripts, tool activity, working status, question choices, and session actions so you can follow and guide agents more naturally.
  - The chat composer adds rich editing, slash commands, image paste, and attachment support for faster, more expressive prompts.
- Minor
  - Mobile adds clearer Session Chat entry points, agent actions, live host state, and context menus that stay compact and fit the available screen.
  - Shared Project Boards keep their established Beads issue prefix when the same board is opened from multiple projects.

## 6.11.0 - 2026-07-31

- Major
  - Add Project now guides you through opening or cloning repositories with source-control discovery and destination selection across the desktop, web, and Android apps.
  - Automations now reliably deliver their prompts to newly created agent sessions and record the agent’s real result instead of timing out or mistaking echoed instructions for completion.
- Minor
  - Remote projects remain selected while you work, and on macOS, opening a Browser pane for a remote project now shows that machine’s listening ports.
  - Mouse clicks and selections in terminal panes stay aligned with the pointer more reliably.
  - Windows and Linux use platform-appropriate default shortcuts and display shortcut labels in their familiar native style.
  - Working sessions keep a steadier order based on when the current work began, while project menus and Add Project dialogs remain better positioned and easier to use.

## 6.10.0 - 2026-07-30

- Major
  - The new optional Inbox sidebar provides a position-stable view of sessions across every project and machine, with the Classic sidebar remaining the default.
  - Inbox sessions can be settled, snoozed until a chosen time, restored, or automatically moved aside after inactivity while working and blocked sessions remain visible.
  - Inbox can group sessions into collapsible projects, combine the same repository across machines, and keep pinned, browser, worktree, and Quick sessions easy to reach.
  - New worktree sessions can be created directly from Inbox with an agent, base branch, optional first prompt, automatic project setup, and safe cleanup when the last session closes.
  - Prompt Editor drafts can now be stashed, searched across the current project or all projects, and inserted back into a terminal without sending them.
- Minor
  - Session cards can show branch, changed-line totals, pull request state, working duration, machine, and worktree details without reordering as activity changes.
  - Project icons are discovered from repository metadata, while project grouping and custom ordering make larger workspaces easier to scan.
  - Switching projects and refreshing Git information causes fewer stalls, and Settings opens reliably even when older saved preferences need normalization.
  - Session renaming is more reliable and the new Ghostex Auto Rename Session skill can name the current session from the work it contains.

## 6.9.0 - 2026-07-28

- Major
  - Focused agent terminals now provide quick actions for opening Prompt Editor and attaching a file or folder without leaving the terminal.
  - File and folder attachments work with local sessions and remote machines, including automatic upload and path handling for WSL workflows.
- Minor
  - Session renaming now reaches the active agent more reliably, including Pi’s dedicated naming command.
  - App dialogs fit their content more naturally while keeping long prompts and rename text accessible.
  - The first-run mobile guidance now points directly to the current React Native Android app.

## 6.8.0 - 2026-07-28

- Major
  - Ghostex for Windows now ships beta x64 and ARM64 installer EXEs designed for running agent terminals and project tools through WSL2.
  - Browser camera and microphone access now uses clear permission prompts, while browser and terminal keyboard input behaves more reliably.
  - Creating several project terminals at once and receiving live session updates is more dependable under heavy activity.
  - The Android app adds customizable agent hotkeys, better scrollback controls, and more reliable terminal interactions.
- Minor
  - Project headings can create WSL-backed terminals directly, Settings can remember a preferred WSL distribution, and Automate is available without the experimental-features switch.
  - Linux x64 users can install the current GPUI app through updated DEB and RPM packages.
  - Fixed app dialogs scroll more naturally, session tooltips align to their rows, and sidebar search and remote-machine controls are easier to use.
  - Resource usage is attributed to the most specific matching project, including nested worktrees.

## 6.7.0 - 2026-07-25

> **A quick note:** Ghostex now runs on its cross-platform, Rust-based app framework instead of Swift and AppKit. There may still be a few small issues, so please report anything you find on the [Ghostex Discord](https://discord.gg/df7b3G92CS) and I’ll get it sorted as soon as possible.

- Major
  - Workspace tabs now keep the order you chose instead of reshuffling as agent activity changes, and new terminals are added predictably at the end of their pane.
  - Terminal panes now show quick buttons for jumping to the top or bottom when you are far into the scrollback.
  - Delayed Send and Close After Done actions from the mobile app now reach the connected machine reliably.
  - Disabled Settings controls now explain why they are unavailable and what needs to change before you can use them.
- Minor
  - Mobile context menus stay inside the visible screen more consistently, and terminal key feedback uses the platform’s native haptics.
  - Git status and changed-file information preserve meaningful whitespace more accurately when read from remote machines.
  - Settings and update actions from the macOS app menu open more reliably, including when Ghostex starts outside a normal terminal environment.
  - Git commit dialogs, command-pane controls and drag feedback, and session tooltips have cleaner sizing and placement.

## 6.6.0 - 2026-07-24

> **A quick note:** Ghostex now runs on its cross-platform, Rust-based app framework instead of Swift and AppKit. There may still be a few small issues, so please report anything you find on the [Ghostex Discord](https://discord.gg/df7b3G92CS) and I’ll get it sorted as soon as possible.

- Major
  - Keyboard shortcuts, terminal input, Tab navigation, and app commands now reach the correct terminal, browser, or Ghostex surface more reliably.
  - New terminals and splits open more consistently inside the active project or worktree, including restored and persistent sessions.
  - The Source editor starts more predictably and now offers a clear retry when its embedded code workspace cannot load.
  - The mobile app now updates session actions and project organization immediately while safely reconciling them with the connected machine.
  - Mobile terminal controls add expanded agent hotkeys, pageable extra keys, clearer modifier state, and optional haptic feedback.
- Minor
  - New mobile terminals can start in the current session’s folder, while Tailscale status, logs, attention clearing, and live session indicators are easier to use.
  - Mobile session menus stay within the visible screen and add richer actions, project appearance controls, and clearer session details.
  - Projects can be dragged out of collections more naturally, and nested sidebar scrolling, tooltips, and project-list visuals feel steadier.
  - Session search shows clearer loading feedback while older sessions are being resolved.
  - Remote terminals now honor the user’s configured login shell more consistently.

## 6.5.1 - 2026-07-23

- Major
  - The new Ghostex mobile app is now available as a signed Android APK, with a fresh React Native experience replacing the old Termux-based package.
  - Each project and worktree now remembers its own terminal pane arrangement, active tabs, and visible sessions when you switch away and return.
  - Mobile sessions now have long-press action menus, a cleaner terminal header and tab bar, clearer live status indicators, and more reliable keyboard composition and terminal sizing.
  - Delayed Send and Close After Done can now be controlled from the Ghostex command line, making it easier to queue follow-ups and automatically close finished sessions.
- Minor
  - Mobile session details and project information now more closely match the desktop sidebar, including richer remote-session context.
  - The mobile app can show Tailscale connection status, open Tailscale directly, and fully quit its Android background session from a confirmation prompt.
  - Project cards and collection panels are easier to distinguish, while Settings navigation follows a more natural page order.
  - Terminal views refresh more reliably after attaching to an existing persistent session.

## 6.4.0 - 2026-07-22

- Major
  - Settings search now works across every page and guides you directly to matching sections, including Agents, Integrations, Remote, Projects, Actions, Open In, and About.
  - Delayed Send can now wait until every agent in a project has finished working, while active schedules and countdowns survive workspace refreshes.
  - Project collections can be reordered directly in the sidebar with clearer drag previews and more reliable project and group dragging.
  - The mobile terminal now keeps its controls above Android and iOS keyboards more reliably and can explicitly show or dismiss the keyboard from its toolbar.
- Minor
  - Clicking non-editable browser and sidebar chrome no longer pulls typing focus away from the active terminal.
  - Settings pages share a more consistent content width, search layout, empty-state guidance, and navigation behavior.
  - Sidebar section labels, remote-machine controls, session lists, scrollbars, drag feedback, and custom titlebar gradients are easier to read and use.
  - Delayed Send opens more reliably from the command palette and more clearly shows whether a timer or agent-status trigger is active.

## 6.3.0 - 2026-07-21

- Major
  - The sidebar now has a clearer visual hierarchy for machines, project groups, projects, browsers, and sessions, with remote project groups and connection status shown consistently.
  - Saved remote machines reconnect automatically after Ghostex launches, and newly created remote sessions open their terminal immediately.
  - Delayed Send can now wait until an agent has stopped working before sending, while keeping the session awake and showing its live status.
  - Android’s project sidebar now more closely matches the desktop experience with grouped projects and quick session actions.
- Minor
  - Sleeping browser tabs return more reliably, and remote machine controls are easier to reach directly from sidebar headers.
  - Terminal keyboard navigation, macOS Retina layout, updater controls, notifications, and other native integrations behave more consistently.
  - Sidebar menus open in more natural positions, project cards are easier to scan, and terminal sections use the clearer “Sessions” label.

## 6.2.1 - 2026-07-20

- Major
  - This macOS maintenance release republishes the latest Ghostex app with a smoother, more reliable update package.
- Minor
  - Remote connections continue using the proven gxserver packages from Ghostex 6.2.0, so remote machines do not need a server upgrade.

## 6.2.0 - 2026-07-16

> **A quick note:** Ghostex has moved from Swift and AppKit to a cross-platform, Rust-based app framework. There may be a few small issues in this release, so please report anything you find on the [Ghostex Discord](https://discord.gg/df7b3G92CS) and I’ll get it sorted as soon as possible.

- Major
  - Ghostex for macOS now runs on its new Rust-based foundation while keeping the terminal, browser, project, agent, and workspace experience in one app.
  - Connecting to a Linux remote can now install the matching gxserver automatically on x64 and ARM64 machines, making first-time remote setup simpler and keeping the app and server in sync.
  - Workspaces now preserve project groups, selected panes, visible sessions, browser state, and terminal state more consistently across launches and remote connections.
  - A new gxserver-hosted web workspace lets you reach Ghostex sessions and terminals from a browser, including connections to multiple machines.
- Minor
  - Terminal focus, keyboard input, selection, themes, links, copy and paste, and restored-session behavior are more reliable in the new macOS app.
  - Sidebar collections are easier to scan and organize with session counts, reordering, appearance controls, machine-based recent projects, and collapse-all controls.
  - Titlebar panels, prompt editing, keyboard zoom, menus, settings, update controls, and native macOS integration now behave more consistently.
  - Android remote sessions attach more reliably and stay compatible with the updated gxserver and persistent-session connection flow.

## 6.0.1 - 2026-07-15

- Major
  - Windows now prefers persistent WSL2 terminals with gxserver and zmx when available, while retaining native PowerShell as an automatic fallback and explicit setting.
  - The modular nightly release now publishes separate Debian x64, Fedora x64, Windows x64 and ARM64, Android, macOS ARM64, Linux gxserver, and Windows WSL bootstrap artifacts.
  - Ghostex-owned zmx attachments now require gxserver to initialize the provider first, preventing attach races from creating incomplete shell sessions.
  - The GPUI app now fully adopts the Ghostex name across its macOS bundle, helper apps, window titles, and packaged surfaces.
- Minor
  - GPUI sidebar, titlebar, session loading, prompt-editor focus, zoom, and workspace visibility behavior is steadier across native and embedded surfaces.
  - Settings adds a Windows terminal backend selector, an About page, and the updated Fable 5.6 orchestration skill.
  - macOS release packaging consumes the same checksum-pinned Linux gxserver assets published for remote and WSL use.
  - This 6.0.1 distribution is a nightly prerelease and does not notify existing macOS installations through Sparkle.

## 6.0.0 - 2026-07-13

- Major
  - Ghostex now ships the GPUI app across macOS, Linux x64, Windows x64 and ARM64, and Android from one modular GitHub Actions release pipeline.
  - GPUI workspace, browser, agent GUI, terminal, project grouping, and remote-session workflows are substantially closer to the established macOS experience.
  - The Rust gxserver and Ghostex CLI foundations now support slimmer cross-platform runtime packages and improved session wake and resume behavior.
- Minor
  - GPUI restores per-project titlebar selections and provides steadier dropdown, focus, reload, and workspace-session handling.
  - Sidebar project headers show awake terminal and browser counts, keep browser sessions ordered consistently, and dismiss menus when focus leaves the sidebar.
  - Terminal input, selection, clipboard, IME, links, rendering, cursor behavior, and Ghostty-host integration are improved across the GPUI app.
  - This initial 6.0.0 distribution is published as a nightly prerelease and does not advance the production Sparkle feed.

## 5.6.1 - 2026-07-07

- Major
  - Native terminal panes avoid redundant Ghostty resize and content-scale refreshes, improving stability during mode switches and restored zmx sessions.
  - Project Board issue details now include comment bodies when opened through gxserver-backed actions.
- Minor
  - Surfaced zmx terminals now skip unchanged grid refreshes so mode switches are quieter and steadier.
  - Terminal pane layout now avoids unnecessary frame relayouts when pane geometry has not changed.

## 5.6.0 - 2026-07-06

- Major
  - Hotkeys now prioritize VS Code when VS Code is visible.
  - Prompt Editor is now a separate helper app, so it launches faster.
  - Prompt Editor content is preserved if the main Ghostex app crashes.
  - App stability is improved.
  - Changing Ghostex settings no longer rewrites unrelated Ghostty settings.
  - The sidebar Git commit workflow now opens through the app modal host bridge for a more native review experience.
- Minor
  - Prompt Editor windows now use the Ghostex app title.
  - Prompt Editor native tabs now show the originating terminal session name.
  - Native terminal scrollbars are easier to see while scrolling.
  - CEF crash reports capture more useful artifacts.
  - The `ghostex` and `gx` CLIs can now set or clear sidebar session tags directly.
  - Sidebar group panels have calmer styling.
  - Bundled agent brand icons are refreshed.
  - Duplicate toast descriptions are omitted.
- GPUI
  - Ghostex is starting to work on Linux and Windows now, and help is welcome on the GPUI project using Rust and Zed's UI framework to make Ghostex cross-platform.
  - GPUI has new Windows CEF adapter groundwork.
  - GPUI has new Linux CEF adapter groundwork.
  - GPUI browser shells now share more plumbing across platforms.
  - GPUI terminal engine can now be enabled per session.
  - GPUI terminal panes support richer input.
  - GPUI terminal panes support selection.
  - GPUI terminal panes support clipboard operations.
  - GPUI terminal panes support IME input.
  - GPUI terminal panes support more Ghostty surface behavior.
  - GPUI terminal rendering is improved.
  - GPUI terminal file-path links can now be opened.
  - GPUI titlebar controls are closer to the macOS app.
  - GPUI hotkeys are closer to the macOS app.
  - GPUI Automate workarea access is closer to the macOS app.
  - GPUI sidebar collapse behavior is closer to the macOS app.
  - GPUI first-responder pane borders are closer to the macOS app.
  - GPUI command panes can attach to gxserver.
  - GPUI engine terminals focus more reliably after keyboard tab cycling.
  - GPUI engine terminals sleep cleanly with command terminals.
  - GPUI Cmd+F terminal search receives typed keys.

## 5.5.0 - 2026-07-03

- Major
  - Releases now use a phased, resumable flow with fast preflight checks, live verification, and checksum-sealed on-demand assets for remote gxserver packages and the Project board `bd` tool.
  - GPUI now covers much more desktop parity, including Sparkle updates, Developer ID signing, native app menus, window-frame restore, quit behavior, support logs, crash reports, `ghostex://` links, Finder file opens, and titlebar update controls.
  - GPUI browser, board, portless, agent, session-history, onboarding, and terminal workflows now behave closer to the macOS app while the new libghostty-vt terminal engine groundwork moves the shell toward cross-platform terminals.
  - Terminal and sidebar workflows are steadier, with embedded-browser link opens, terminal link diagnostics, stabilized sidebar drag selection, project ghost fixes, better GPUI IME handling, and first-prompt rename reliability.
  - Manage file context menus now support Reveal in Finder, Add to Session Context, create-here actions, and clearer relative path copying.
- Minor
  - Automate WKWebViews now receive the Project Board bridge and automation agent lists exclude unlaunchable agents.
  - Sidebar multi-select avoids accidental drag capture, keeps pane-hidden rendered rows selectable, and adds New Group / Move to New Group affordances when supported.
  - Hermes Agent is now available in the default sidebar list with matching Android and iOS session icons.
  - GPUI hides the App Icon picker where that native subsystem is unavailable and records GPUI-specific diagnostic log files alongside the existing macOS scenarios.

## 5.4.0 - 2026-07-02

- Major
  - Sidebar lag was fixed by reducing repeated gxserver Git checks, duplicate native status updates, and fast zmx title-observer retries.
  - Sidebar session cards now support multi-select bulk actions for sleep, wake, pin, unpin, tag, full reload, and close.
  - GPUI now has more app parity, including named session groups, local gxserver startup feedback, app toasts, configured hotkeys, folder picking, Close After Done, and previous-session text search.
  - Manage HTML previews now run as interactive browser documents, so generated docs can use their own scripts, forms, frames, and fullscreen behavior.
- Minor
  - GPUI terminals now handle link opens, bells, title updates, and working-directory updates from Ghostty runtime actions.
  - GPUI project and session workflows now cover more repository folder and workspace folder picker paths.
  - README wording around Excalidraw docs is clearer.

## 5.3.0 - 2026-07-01

- Major
  - `ghostex` and `gx` now open the promoted GX 2 terminal UI by default, with `gx 2` kept as a compatibility alias.
  - Automations Overview and project Automate pages now stay behind Enable Experimental Features while picker state and agent lists load more reliably.
  - Docs and Manage are smoother, with duplicate file actions, isolated HTML previews, markdown annotations, and a folders shortcut.
- Minor
  - The menu-bar status dropdown now puts attention sessions first, working sessions second, and idle sessions by recent activity.
  - Sidebar and modal chrome are calmer, with steadier pinned-session dragging, modal scroll caps, tooltip radius, session list polish, and stable Keep Awake styling.
  - Remote and mobile flows are steadier, with Android attach latency improvements and iOS Zen bubble and scrolling polish.
  - Terminal bell attention, remote edit drafts, color environment forwarding, and native workspace behavior are more consistent.

## 5.2.0 - 2026-06-30

- Major
  - Automations now run through gxserver and can be launched from the sidebar, Project Board, and `ghostex`/`gx` CLI.
  - Docs can scan additional project folders, collapse or expand the tree, copy file and folder paths, and show cleaner root actions.
  - HTML Docs now open in isolated previews with better starter pages, Agentation feedback support, and slimmer embedded scrollbars.
  - Settings has a simpler General layout, a clearer app icon picker, remembered page position, and a lighter prompt editor setup.
  - Remote setup and attach flows show clearer install diagnostics, package selection, and session state.
- Minor
  - Sidebar project headers, empty states, context menus, scroll glow, tooltips, and session agent icons are easier to scan.
  - Project editor chrome, sticky navigation, update download feedback, and first-launch dismissal are steadier.
  - Prompt editing now uses the configured machine editor path when needed instead of relying on `gte`.
  - Remote rows use cleaner zmx provider metadata, and zmx advertises editor capability correctly.
  - Android SSH transport is more reliable, and iOS branding plus remote-session sidebar behavior are refreshed. Thanks @wiedymi.
  - Apple Silicon releases now validate and bundle the Linux remote gxserver packages used for remote hosts.

## 5.1.0 - 2026-06-29

- Major
  - App Shots now support both Shift and both Option hotkeys, return Ghostex to the front after capture, and keep metadata optional.
  - Docs can show root Markdown, HTML, and Excalidraw files, plus rename or delete folders from the file tree.
  - Settings and Keep Awake now live in the sidebar shortcut row with compact dropdowns.
  - Settings uses faster native scrolling and keeps hook setup focused in Settings > Agents.
- Minor
  - Factory Droid session titles no longer show the status marker prefix.
  - Sidebar session focus borders stay steadier during WebKit-to-native focus handoff.
  - Sleeping pane placeholders can receive keyboard focus from directional hotkeys without waking early.
  - Docs sidebar and editor header chrome are tighter and easier to scan.
  - GPUI CEF bridge ownership is cleaner for the sidebar and helper process.

## 5.0.0 - 2026-06-28

- Major
  - Docs replaces Manage as the project document workarea with folders, Markdown, HTML explainers, Excalidraw drawings, review annotations, and Agentation feedback.
  - Settings are simpler, with persisted Show Advanced, clearer Enable Experimental Features naming, visible Agents Hub access, and calmer app-icon controls.
  - Source and project editor panes switch more reliably, show native load errors clearly, and avoid shared runtime port conflicts.
  - The GPUI app continues moving toward cross-platform parity with sidebar, Source, browser, settings, remote, and command workflow progress.
  - The Rust gxserver path now covers more session status, title, renderer-command, activity, and lifecycle behavior.
- Minor
  - Custom app icons can update the running app, Dock tile, and bundle icon more consistently. Thanks @NiTE.
  - Sidebar search, collapsed project drawers, and first-responder focus handoff are faster and steadier.
  - Agent history search finds newer Codex prompts and very large Codex transcripts more reliably.
  - Homebrew installs no longer show the old macOS requirement warning. Thanks @Yabuku-xD.
  - Tips now point users toward Ghostex Browser Use, Ghostex Computer Use, and Faster Chrome DevTools setup.

## 4.21.4 - 2026-06-21

- Major
  - Manage adds a beta-gated project workarea for file browsing, previews, editing, review annotations, and Excalidraw drawings.
  - Settings native windows open more reliably after hydration.
  - Settings > Integrations keeps hook and skill install, update, and uninstall actions together.
  - Project Board now shows a first-open loading overlay and records safer title-generation diagnostics.
- Minor
  - Sidebar toggle and native pane-tab chrome are cleaner and easier to read.
  - Selected sleeping tabs keep the active-tab visual treatment before wake.
  - Browser page appearance now follows the current system behavior more consistently.
  - First-launch setup buttons are disabled when there is no setup action to run.

## 4.21.3 - 2026-06-19

- Major
  - Settings text fields, dropdowns, and color pickers keep focus while changes save.
  - Browser panes follow the current macOS light or dark appearance again.
  - Resources now shows running terminal sessions even when their pane is not loaded.
  - The experimental Rust gxserver path now covers more agent, hook, skill, log, session, and project operations.
- Minor
  - Status menu right-click and Control-click actions work again from the macOS menu bar.
  - Agent prompts send in smaller chunks so Cursor is less likely to collapse them into paste chips.
  - The titlebar Resources and update controls have cleaner icon alignment.

## 4.21.1 - 2026-06-19

- Major
  - `ghostex create-agent` now starts the agent process immediately after creating the session.
  - Fresh agent panes no longer resume against raw Ghostex `G...` session IDs.
  - Sleep, Wake, Close, and Close After Done are available from the command palette and Hotkeys.
  - Browser panes use a light page canvas for transparent public pages.
- Minor
  - Option+Shift+S sleeps the focused terminal by default.
  - Focused-session actions target command-pane terminals correctly.
  - The Git commit review sidebar is quieter and easier to scan.

## 4.21.0 - 2026-06-19

- Major
  - Agent setup is simpler, with gxserver-owned skill installs and Default Prompt Agent settings.
  - Browser appearance and titlebar settings are cleaner and persist per project and origin.
  - Sidebar search, session drag-and-drop, and gap right-click menus are easier to use.
  - Project Board labels load faster and Kanban cards are easier to read.
- Minor
  - First-launch and Settings screens are calmer and easier to scan.
  - Browser panes open the requested initial URL more reliably. Thanks @cuttothechaseo.
  - Sidebar section header actions no longer overlap their labels. Thanks @cuttothechaseo.
  - Source panes wait for code-server readiness before opening.
  - Delayed Send timers restore correctly after restarting Ghostex.
  - Scroll fades and debug diagnostics are cleaner while keeping private data redacted.

## 4.20.1 - 2026-06-19

- Major
  - Titlebar Settings menu
    - Global app actions now live in the far-right titlebar Settings menu instead of the sidebar overflow menu.
    - The menu includes Settings, Commands, Hotkeys, Wake Pet, Pinned Prompts, Scratch Pad, Running when debugging UI is enabled, and Join Discord.
    - The sidebar is simpler, with the Commands Pane launcher moved onto the Recent Projects header as a hover action.
  - Browser appearance
    - Chromium browser panes now default pages to Light mode, even when the hidden browser toolbar color-scheme control is not visible.
    - System, Light, and Dark browser appearance choices now update page `prefers-color-scheme` behavior in Chromium panes instead of only storing the menu value.
- Minor
  - Source pane settings
    - Changing VS Code settings-link options now waits briefly before restarting the shared code-server runtime, so rapid Settings changes apply once using the final choice.
    - Only awake Source panes trigger that restart, reducing unnecessary runtime churn.
  - Native pane polish
    - The native pane action button now uses a compact layout icon instead of the old hamburger glyph.
    - First-launch setup copy points users back to Settings > Integrations for later setup tasks.

## 4.20.0 - 2026-06-18

- Major
  - Command palette and session search
    - Cmd+Shift+P now exposes more app-level commands, including Previous Sessions, pinned prompts, running sessions, Scratch Pad, Agents Hub, Actions, Open Targets, Hotkeys, setup, changelog, quick terminal, quick browser tab, Automations, and project actions.
    - Open Current Project in Finder and visible Open In targets can be launched from command mode without leaving the current workspace.
    - Cmd+P session search is focused on visible session titles, so hidden metadata such as project paths and generic default agent titles no longer pull unrelated sessions into results.
    - Previous-session search ranks stopped sessions by close time and hides placeholder agent names while keeping meaningful user and agent titles searchable.
  - Setup, Tips, and tutorial flow
    - First-launch setup is simplified to Welcome, Agent Hooks, and Bundled Agent Skills.
    - Skipping hook or bundled-skill setup now shows a focused warning with the install action beside the deliberate continue action.
    - The Features entry points now open a dedicated Ghostty tutorial video modal, with the older Highlighted Features tour kept out of the main flow.
    - The titlebar Tips menu is shorter and clearer, adds Docs, keeps Features, Setup, and Changelog handy, and can install missing agent hooks directly from its warning notice.
  - Agent hooks and recovery controls
    - Advanced Settings adds searchable Uninstall Hooks and Uninstall Skills actions for removing Ghostex-owned setup artifacts.
    - gxserver can uninstall Ghostex-owned hook entries from supported agent CLI configs without touching user-managed provider hooks.
    - Agent hook status checks prioritize Codex, Claude, and Pi first, then continue through the rest of the supported agent CLIs so setup status appears progressively.
    - Hook warnings now explain that automatic session naming, In Progress/Needs Attention state, and sleep/resume reliability depend on current hooks.
- Minor
  - Native pane and titlebar polish
    - Sleeping a terminal now keeps its split slot as a wake placeholder instead of collapsing the layout.
    - Blank titlebar dragging is more reliable across the WebKit-to-native handoff.
    - Open Folder now opens the selected workspace folder itself in Finder instead of revealing it from the parent folder.
    - Native app modal sizing and tutorial video loading are tuned for the new help surfaces.
  - Sidebar and settings polish
    - Empty sidebars with no projects now guide first-time users toward the Projects plus button.
    - Settings Actions editors can delete both custom actions and default actions from the edit surface.
    - Projects settings include a project removal control.
    - Highlighted Features arrow and keyboard navigation stop at the first and last item instead of wrapping.
  - CLI, delayed send, and docs
    - The CLI preserves live-written Monaco prompt edits as saved if the native bridge closes before the normal status callback.
    - Delayed Send focuses the minutes field more reliably in native child windows and pressing Enter in the duration fields schedules the timer.
    - README screenshots, Android/iOS download presentation, previous-session search docs, and feature-gallery copy were refreshed for the current app.
    - The local agent instructions now explicitly avoid restarting the app unless requested.

## 4.14.2 - 2026-06-17

- Major
  - Highlighted Features and screenshots
    - The macOS app bundle now includes the latest supplied Highlighted Features screenshots instead of the older package images.
    - The feature tour and README media use the exact committed PNG captures for the agent splits, Chromium design mode, embedded editor, Kanban board, and rich prompt editor views.
  - Titlebar Actions
    - Custom titlebar Actions are preserved on startup when gxserver has not imported action content yet.
    - Legacy saved Action icon colors are stripped so Action icons inherit the native chrome color consistently.
- Minor
  - Native pane chrome
    - Focused pane borders keep their top and right edges visible against clipped pane edges.

## 4.14.0 - 2026-06-16

- Major
  - UX and settings
    - Settings and many parts of the app have been simplified.
    - Sidebar and titlebar controls are easier to scan and tune.
    - Discover Ghostex now includes clearer mobile download and onboarding entry points.
  - Performance and reliability
    - Performance has been improved substantially across app startup, sidebar refreshes, session restore, and packaged runtime flows.
    - Creating, restoring, waking, and closing panes is steadier.
    - New terminals show a mounting placeholder while the terminal surface catches up.
    - Startup focus targets are preserved more reliably.
  - Sessions and gxserver
    - Close After Done timers can close completed sessions after they stay done.
    - Previous-session search, restore, project jumps, rename focus, and command-palette focus are more stable.
    - gxserver status now distinguishes running, missing, sleeping, and persistence-disabled sessions more clearly.
    - Packaged gxserver now includes the Rust daemon packaging path and stricter packaged-runtime validation.
  - Theme and onboarding
    - Dark theme colors are now in place.
    - Light theme colors are coming soon.
    - First-run and Highlighted Features now show a replayable feature tour before setup.
- Minor
  - CLI, TUI, and automations
    - The installed CLI adds the experimental `gx 2` / `ghostex 2` TUI path.
    - The default `gx` TUI remains unchanged.
    - Automations are coming soon.
    - The TUI session switcher clamps vertical navigation at list edges.
    - Held Up and Down movement repeats faster without raw terminal repeat bursts.
  - Browser, projects, and App Shots
    - 4.14.1 keeps image thumbnails in the floating prompt editor clickable across the whole thumbnail shelf.
    - Project editor, browser, and App Shots surfaces have tighter lifecycle and persistence behavior.
    - Browser and project surfaces avoid blank new-tab states.
    - Floating prompt-editor work is preserved during app lifecycle closes.
    - Newly created project sessions appear more reliably.
  - Git, history, and diagnostics
    - Titlebar Git controls add a direct remote sync action.
    - `zehn` history search now opens as a flat relevance list by default.
    - Day-group browsing remains available behind Ctrl-D.
    - Support logs trim old history, sample high-volume diagnostics, and keep privacy checks tightened.
  - Mobile, docs, and community
    - Android includes the centered factory droid icon.
    - Android download links now point at the latest release.
    - Checked-in README media was sanitized for core Ghostex workflows.
    - Huge thank you to @saleem-hadad for helping a ton with improving the app's UX.

## 4.12.0 - 2026-06-14

- Major
  - Command palette search is now centered on sessions by default. Cmd+P opens session search, Cmd+Shift+P opens command mode with `>`, and results include current sessions, active projects, collapsed projects, and previous sessions ranked by recent activity.
  - Native pane focus follows the surface that actually owns keyboard input. Focused borders track the AppKit first responder, directional focus uses rendered pane geometry, and command panels plus project-editor companion panes avoid stale retargeting.
  - Sleeping panes behave like selected panes instead of empty gaps. Their tab chrome remains visible, placeholders show "Press Any Key to Wake", and normal key presses wake the sleeping terminal in place.
  - Sidebar tag filters can be reordered, hidden, disabled, and reset from Settings. Hidden or disabled tags are removed from active filtering so old filter state does not keep sessions invisible.
- Minor
  - Sparkle's titlebar update button now fades only while an accepted update is actively downloading.
  - App Shots is opt-in by default and marked as Beta in Settings.
  - The TUI session switcher throttles held Up and Down arrow repeats while still responding immediately to the first key press.
  - Long project session lists use a regular "Show N more" session row and restore scroll position more predictably when collapsed.
  - Session card tooltips include clearer active, sleeping, and not-loaded state text plus colored metadata rows.
  - Native modal and window polish tightens Add Worktree padding and close behavior, startup overlay layering, Exit Focus styling, command-panel resize hover feedback, toast entry animation, and browser profile routing.

## 4.11.0 - 2026-06-13

- Major
  - Native workspace layout now keeps titlebar, sidebar, divider, pane, webview, and terminal regions in stricter non-overlapping frames, reducing click and focus misses.
  - Sidebar dividers, native pane tabs, and titlebar chrome have more predictable hit targets because each surface owns its own input region.
  - New terminals, forks, and restored panes wait for terminal-ready state before focus moves, so keyboard focus lands in the intended session.
  - Pane tabs recover more reliably while gxserver presentation reconnects, including sleeping, waking, closing, and remote-backed rows.
  - Previous Sessions and session search preserve provider, agent, and restore identity more accurately when ranking, restoring, or reattaching to older sessions.
  - Stale local gxserver rows are pruned from the native sidebar so dead sessions do not compete with live provider state.
- Minor
  - Sidebar session cards, overflow menus, command palette input, and search surfaces received tighter interaction polish for dense daily use.
  - Project Board and Tickets cards are denser, placeholders are clearer, and bulk actions and group labels are easier to scan.
  - First-launch preferences and skill-install status rows are simpler, with status presented in the action area.
  - Claude Code and Codex terminal keybindings line up with the current prompt-editor and terminal shortcut model.
  - Android startup recovery handles empty-service startup more reliably.

## 4.10.0 - 2026-06-12

- Native workspace panes are better integrated with the macOS host, reducing click-routing misses.
- Direct project-tab titlebars, app modal windows, and toast routing are better integrated with the macOS host, reducing titlebar/sidebar focus churn.
- Passive sidebar terminal restore preserves split layouts while the sidebar catches up.
- Workspace-pane materialization preserves restored split layouts instead of merging panes while the sidebar catches up.
- Project Board focus is steadier during passive refreshes.
- GitHub mode focus is steadier during passive refreshes.
- Directly mounted project tab titlebars route clicks to the intended workspace surface.
- Worktree deletion can clean up related branch metadata from the UI.
- Prompt-editor capability routing reduces stale agent attention after dismissed work.
- Terminal Escape reporting reduces stale agent attention after dismissed work.
- The titlebar adds a compact sidebar collapse button beside the project name.
- Empty Tips unread sections stay hidden.
- Resources recovery is clearer with Restart plus Reload App when gxserver is off.
- Add Worktree, Git Commit, and other native child-window modals have tighter macOS sizing and padding.
- Compact dialogs stay compact while commit review gains room on the right diff side.
- The `ghostex` and `gx` CLI commands use gxserver session inventory.
- The `ghostex` and `gx` CLI commands add a sidebar toggle.
- The `ghostex` and `gx` CLI commands install as wrapper commands outside `Ghostex.app` so macOS policy assessment does not kill direct app-bundled script execution.
- Agent support expands with Kiro CLI.
- Agent support expands with OMP hook sidecars.
- Agent support expands with mobile session status ingestion.
- Agent support expands with Claude bare `/rename` staging for first-prompt titles.
- Agent support expands with path-based live-process identity repair.
- Agent support expands with a default Accept All mode setting.
- App Shots now stage captured desktop context in the focused or recent live agent session instead of being Codex-only.
- App Shots create the configured default prompt-agent session only when no agent target is available.
- Sidebar and settings polish adds a collapse command and hotkey.
- Sidebar and settings polish adds Copy Session Details.
- Ghostty settings are folded into the main settings sections.
- Agent Hub file contents load only when opened.
- iOS refresh indicators stay tied to active refresh requests.
- Packaged macOS runtime validation checks the bundled code-server Node 22 runtime.
- Packaged macOS runtime validation no longer executes sealed native modules during validation.

## 4.1.5 - 2026-06-10

- Installed agent hooks now run through Ghostex-owned bundled runtimes instead of `/usr/bin/python3` or user-installed Node interpreters.
- Hook sidecars and command status updates keep working on machines without Python.
- Claude sessions migrated from Ghostex 3.6 can wake more reliably because gxserver repair backfills transcript paths and saved resume commands.
- Wake resolves Claude's real session id before running `claude --resume` instead of trusting a sidebar title.
- Context-menu Sleep no longer parks a row as sleeping while the zmx provider is still alive.
- Wake and intentional close flows still show immediate sleeping feedback until the host snapshot confirms the same state.
- Project board ticket creation reconciles each project's Beads issue prefix before mutations.
- Local board actions send both project id and project path so gxserver can reject stale URL/id mismatches.
- The sidebar adds a configurable Show less row count.
- The sidebar adds a Close menu visibility setting.
- The sidebar adds remote-session edit entry points for quicker day-to-day session management.
- Chromium-embedded panes support standard zoom in, zoom out, and reset shortcuts from the toolbar.
- Waking zmx sessions no longer replays stale working/attention activity from the pre-sleep snapshot.
- Project and Kanban flows require Ghostex's bundled Beads CLI and ignore unrelated `bd` binaries already on PATH.
- Future macOS Sparkle, GitHub, and Homebrew releases ship Apple Silicon builds only.

## 4.1.0 - 2026-06-09

- Remote machines can now save SSH passwords in macOS Keychain.
- Remote machines can use saved SSH passwords for SSH, SCP, and tunnel connections without storing raw passwords in settings.
- Remote machines show clearer saved-password state and authentication guidance.
- Agent hook updates now reject cross-wired agent identities.
- Session-state updates now reject cross-wired agent identities.
- Cross-wired identity protection reduces cases where one agent terminal could inherit another row's title, status, completion state, or resume identity.
- Ctrl+G prompt editing returns focus to the correct terminal more reliably.
- Monaco prompt-editor dismissal returns focus to the correct terminal more reliably.
- Prompt-editor focus repair includes sessions launched through gxserver global references.
- Sidebar presentation updates apply smaller live patches for session groups.
- Sidebar presentation updates apply smaller live patches for HUD chrome.
- Smaller sidebar live patches reduce refresh churn and terminal focus steals while sessions are added, removed, reordered, or updated.
- Session context menus hide Copy Resume by default behind an explicit setting.
- Session context menus hide Copy Attach Command by default behind an explicit setting.
- Sleep Below and Close Below now target the rendered rows beneath the clicked card across project groups.
- Remote sections received visual polish for denser daily use.
- Recent Projects search received visual polish for denser daily use.
- Active sidebar search received visual polish for denser daily use.
- Titlebar resource copy received visual polish for denser daily use.
- Command icons, tag menus, drag handles, and sidebar panel spacing received visual polish for denser daily use.
- The Ghostex TUI now uses a neutral gray-blue default theme.
- The Ghostex TUI has clearer Help/Hotkeys and Quit Ghostex labels.
- The Ghostex TUI has broader built-in agent labels so restored desktop sessions are easier to recognize from the terminal switcher.
- Embedded code-server packaging is more reliable across Apple Silicon and Intel builds.
- Embedded code-server packaging includes target-architecture ripgrep materialization.
- Embedded code-server packaging includes authenticated GitHub artifact fetches during release builds.
- The Android download badge now points at the 4.1.0 release APK.

## 4.0.3 - 2026-06-08

- Remote session and group clicks now open a local Ghostty terminal that SSH-attaches to the selected remote session with the stable `ghostex attach` contract.
- Copy Attach Command still copies the SSH command for external terminals.
- Remote attach carrier terminals stay hidden from the local Quick section, so focus and active styling remain on the owning remote machine row.
- Remote machine setup failures now show more actionable stage-specific messages for SSH, install, token, tunnel, streaming, and transport problems instead of raw loopback or WebKit errors.
- gxserver request failures now show more actionable stage-specific messages for SSH, install, token, tunnel, streaming, and transport problems instead of raw loopback or WebKit errors.
- Remote settings are easier to scan with compact saved-machine cards.
- Remote settings include inline Tailscale setup help.
- Remote settings include clearer optional SSH identity-file guidance.
- The Quick section header can launch the selected agent directly.
- The Quick section header uses the same agent picker as project headers.
- New Quick agent chats stay projectless.
- The titlebar now disables GitHub mode when the active project has no GitHub remote.
- The titlebar now disables GitHub and Kanban mode for Quick sessions.
- Embedded code-server editor panes now use Ghostex-owned bundled settings by default.
- Embedded code-server editor panes start with the Dark 2026 theme on new profiles.
- Embedded code-server editor panes keep local VS Code settings as an explicit opt-in.
- Sparkle update checks repeat quietly while Ghostex is running.
- The titlebar update button can appear on first render.
- Update download and extraction progress windows stay hidden while the release notes and relaunch prompts remain available.
- The native sidebar/workarea divider keeps its resize cursor and visible separator aligned during hover and live resizing.
- Installed macOS app bundles are smaller because release packaging prunes duplicate Beads payloads before notarization.
- Installed macOS app bundles are smaller because release packaging prunes wrong-architecture node-pty prebuilds before notarization.

## 4.0.2 - 2026-06-08

- Installed macOS builds now package the full embedded code-server runtime.
- Installed macOS builds reuse the embedded code-server Node 22 binary for gxserver.
- Installed macOS builds include the bundled Beads CLI.
- Installed macOS builds validate the packaged runtime during release builds.
- Source tab packaging is more reliable because the embedded VS Code runtime carries its ripgrep helper files.
- Source tab packaging cleans up temporary build metadata after packaging.
- Terminal image paste can convert clipboard images into previewable Markdown links with Cmd+V or Ctrl+V.
- Settings -> Terminal Behavior now includes a Paste previewable images toggle for users who want normal clipboard behavior.
- gxserver presentation updates now carry stable attention event IDs.
- macOS can play completion sounds and notifications once for fresh attention events.
- Completion sounds and notifications do not replay during startup or stream recovery.
- Command-pane completions keep using the action completion sound path.
- Command-pane completions write status updates through per-process temp files.
- Command-pane completions reduce missed completion sounds during concurrent status updates.
- Git agent workflows no longer pin duplicate persistent "running" toasts when the visible agent terminal already shows the workflow progress.
- Ghostex Android auto-scroll now follows new output only from the actual live bottom row.
- Scrolling up even one row keeps history anchored without selecting text first.

## 4.0.1 - 2026-06-08

- Upgrades from Ghostex 3.x can now recover missing gxserver project and session rows even when a completed migration marker was already written, including last-resort recovery from the pre-cutover shared-state backup.
- Passive terminal-title and sidebar refreshes no longer steal keyboard focus from the terminal while you are typing, and sidebar session clicks start native focus/layout work before the React sidebar highlight catches up.
- Git commit review is more useful as a workspace: Show All can concatenate changed-file diffs, diff display preferences persist across app restarts, changed files can copy their path from a right-click menu, and the modal opens directly into review content.
- Quick terminal/browser/file containers no longer trigger project-scoped Git status probes or Git error toasts, and worktree project header menus prioritize Copy Path over a redundant Open action.
- Source tab startup in local development now validates the embedded VS Code payload and Git extension native module before opening, showing actionable setup guidance instead of a raw code-server 500 page or delayed Git activation failure.
- Local starts on Apple Silicon build and launch arm64 Ghostex resources even when the invoking shell is running under Rosetta, and stale zmx/zehn artifacts are rebuilt when their Mach-O architecture does not match.
- The titlebar update button stays available until Sparkle confirms the installed app is current, so opening or closing the update dialog no longer hides a still-applicable update.
- Sparkle appcasts generated by the release flow now embed the matching changelog notes so the update dialog can show release details directly.
- Project editor companion switching avoids unnecessary editor host relayout when the editor is already stable, reducing flashes while moving between companion sessions.
- The transparent native sidebar resize strip keeps the left-right resize cursor while hovered or dragged.

## 4.0.0 - 2026-06-08

- Session tags can be applied, displayed on cards, filtered in Active and Previous Sessions, and preserved in manual sidebar order across restore and Previous Sessions.
- Git commit review adds inline changed-file diff inspection so review prompts can inspect file patches without leaving the modal.
- First-prompt title generation is more reliable, including Grok Build support, staged rename handling, guards for skipped or stale generated titles, and retry after cancellation.
- zmx Ctrl+G prompt editing follows the currently attached client capability, keeping desktop Monaco available while SSH, mobile, and TUI attaches use terminal-native `gte`.
- Cmd+T creates a terminal tab next to the focused tab, Cmd+N opens a browser tab next to the focused tab, and Option+1 through Option+4 switch Agents, Source, GitHub, and Kanban views.
- Closing the active tab in a split pane promotes the adjacent tab in that pane before layout materialization, preserving split layout instead of collapsing unrelated panes.
- Sleep Inactive and Agent Auto Sleep keep terminals with active Delayed Send timers awake until the scheduled send fires, and focused agent sessions are always excluded from Agent Auto Sleep.
- Agent working indicators and session titles are steadier during spinner-heavy Codex, Claude, Cursor, and Pi activity, reducing attention flicker and repeated no-op sidebar refreshes.
- Background sleep, close, and auto-sleep transitions preserve the focused pane/tab instead of pulling focus away from the active session.
- Agent hook installation covers supported CLIs through gxserver, and installed hooks can report working, attention, idle, first-prompt, and resume metadata directly to gxserver for more reliable status across clients.
- Duplicate completion sounds and macOS notifications are suppressed when the same attention event is replayed from hook or gxserver state.
- Codex-powered title generation, board-title generation, and other internal prompt jobs run as ephemeral/internal work so they do not create restorable Codex sessions or overwrite a real session's resume identity.
- Codex resume validates exact ids and falls back through filtered title lookup, avoiding internal `codex exec` title-generation transcripts.
- Agent Auto Sleep waits when zmx title-observer health is starting, retrying, or failed instead of treating unavailable working-status detection as idle.
- Full reload for zmx sessions reloads the clicked session in place instead of creating a duplicate sidebar row, and Ctrl+G prompt editing checks the bundled zmx binary instead of a stale PATH zmx.
- New projects and embedded editor panes appear in the sidebar earlier, and code-server startup failures surface as row errors and toasts instead of failing silently.
- Installed macOS builds validate the packaged gxserver Node 22 native-module runtime and show actionable reinstall or Node setup guidance when the runtime does not match.
- Installed macOS builds bundle `ghostex` and `gx` CLI binaries with session display-title support and packaged runtime roots.
- Previous Sessions hides command-pane runs, ranks rows by true last activity, and restores durable session tags, restored-from identity, and saved manual sidebar order.
- Sidebar Last Active labels keep ticking from the client clock even when React Compiler caches the row render.
- `gx find` / zehn history results are grouped by last-active day, show source session titles above matched prompt text, include compact last-active times, and stay quiet unless the user explicitly runs `zehn update`.
- Provider session ids in terminal panes are hidden by default and remain available through the explicit session-id overlay setting.
- Native terminal Cmd+C uses Ghostty's copy action so selected terminal text reaches the system clipboard consistently.
- Native workspace focus, pane-tab close button chrome, centered sidebar context menus, and visible-row Cmd+number shortcuts are tighter across nightly sidebar interactions.
- Rename Session > Generate Name keeps the visible "Generating title" overlay active until the generated rename is applied or submitted.
- Clone & Add can be submitted as soon as locally valid repository and destination fields are present, while existing-destination previews still block cloning.
- Delayed Send timers keep the leading clock visible over tags and deadline-only projections, and native terminal badges relayout immediately when timers start or cancel.
- Sleep and close actions for presentation-backed zmx sessions use gxserver provider transitions even when older local session metadata is incomplete.
- Ghostex-launched app, gxserver, zmx, agent-hook, Git, Beads, clone, and local dev subprocesses keep ANSI color capability even when the parent shell exports `NO_COLOR`.
- Native sidebar web bundles use the React Compiler build path for smoother sidebar interactions.
- Debug logs stay quiet in normal use, rotate before growing too large, and show a titlebar warning while Debug logging and UI is enabled.
- Support diagnostics avoid writing raw title previews, command output previews, session id lists, paths, and stderr snippets while keeping counts and timing useful for troubleshooting.
- Dragging images onto inactive terminal panes accepts drops reliably, and restarting Ghostex no longer relaunches the app when closing an installed build.
- Project board and Tasks flows improve ticket routing, comments, placeholders, and Create & Start handoff behavior.

## 4.0.0-beta.3 - 2026-06-07

- Beta distribution remains available through GitHub Releases and Homebrew DMG installs while Sparkle automatic-update feeds stay on the current public release.
- Agent working indicators and session titles are steadier during spinner-heavy Codex, Claude, Cursor, and Pi activity, reducing attention flicker and repeated no-op sidebar refreshes.
- Background sleep, close, and auto-sleep transitions preserve the focused pane/tab instead of pulling focus away from the active session, and focused agent sessions are always excluded from Agent Auto Sleep.
- New projects and embedded editor panes appear in the sidebar earlier, and code-server startup failures now surface as row errors and toasts instead of failing silently.
- Installed macOS builds validate the packaged gxserver Node 22 native-module runtime and show actionable reinstall or Node setup guidance when the runtime does not match.
- Codex-powered title generation, board-title generation, and other internal prompt jobs now run as ephemeral/internal work so they do not create restorable Codex sessions or overwrite a real session's resume identity.
- Codex resume now validates exact ids and falls back through filtered title lookup, avoiding internal `codex exec` title-generation transcripts.
- Cancelling first-prompt title generation no longer lets a stale result rename the session, and a later user prompt can retry title generation.
- Agent Auto Sleep waits when zmx title-observer health is starting, retrying, or failed instead of treating unavailable working-status detection as idle.
- Agent hook installation now covers supported CLIs through gxserver, and installed hooks can report working, attention, idle, first-prompt, and resume metadata directly to gxserver for more reliable status across clients.
- Duplicate completion sounds and macOS notifications are suppressed when the same attention event is replayed from hook or gxserver state.
- Full reload for zmx sessions now reloads the clicked session in place instead of creating a duplicate sidebar row, and Ctrl+G prompt editing checks the bundled zmx binary instead of a stale PATH zmx.
- Previous Sessions hides command-pane runs and ranks rows by true last activity instead of recent metadata refreshes.
- Sidebar Last Active labels keep ticking from the client clock even when React Compiler caches the row render.
- `gx find` / zehn history results are grouped by last-active day, show source session titles above matched prompt text, include compact last-active times, and stay quiet unless the user explicitly runs `zehn update`.
- Ghostex-launched app, gxserver, zmx, agent-hook, Git, Beads, clone, and local dev subprocesses keep ANSI color capability even when the parent shell exports `NO_COLOR`.
- Native sidebar web bundles are compiled through the React Compiler build path for smoother nightly sidebar interactions.
- Support diagnostics avoid writing raw title previews, command output previews, session id lists, paths, and stderr snippets while still keeping counts and timing useful for troubleshooting.

## 4.0.0-beta.2 - 2026-06-06

- Beta distribution remains available through GitHub Releases and Homebrew DMG installs while Sparkle automatic-update feeds stay on the current public release.
- Ctrl+G prompt editing in zmx sessions now follows the currently attached client capability, so desktop Monaco remains available while SSH, mobile, and TUI attaches stay on terminal-native `gte`.
- Restoring Previous Sessions preserves session tags, restored-from identity, and saved manual sidebar order when that order was explicitly stored.
- Cmd+T now creates a terminal tab next to the focused tab, Cmd+N opens a browser tab next to the focused tab, and Option+1 through Option+4 switch Agents, Source, GitHub, and Kanban views.
- Closing the active tab in a split pane now promotes the adjacent tab in that pane before layout materialization, preserving split layout instead of collapsing unrelated panes.
- Sleep Inactive and Agent Auto Sleep now keep terminals with active Delayed Send timers awake until the scheduled send fires.
- Default terminal panes no longer show provider session ids unless the session-id overlay setting is explicitly enabled.
- Native terminal Cmd+C now uses Ghostty's copy action directly so selected terminal text reaches the system clipboard consistently.
- Reference-sidebar Previous Sessions rows now align with normal project-session row spacing.
- Debug logs are quieter in normal use, rotate before growing too large, and show a titlebar warning while Debug logging and UI is enabled.
- zmx title updates keep working-state heartbeats alive without flooding gxserver or sidebar presentation with repeated spinner frames.
- Ghostex-generated launch, resume, fork, restore, Search by Text, and command-pane scripts now avoid being saved into Atuin shell history.
- Rename Session > Generate Name keeps the visible "Generating title" overlay active until the generated rename is applied or submitted.
- Clone & Add enables as soon as locally valid repository and destination fields are present, while existing-destination previews still block cloning.
- Delayed Send timers now keep the leading clock visible even when a session is tagged or only the deadline is projected, and native badges relayout immediately when timers start or cancel.
- Sleep and close actions for presentation-backed zmx sessions now use gxserver provider transitions even when older local session metadata is incomplete.

## 4.0.0-beta.1 - 2026-06-05

- Beta distribution is available through GitHub Releases and Homebrew DMG installs while Sparkle automatic-update feeds remain on the current public release.
- Session tags can now be applied, displayed on cards, filtered in Active and Previous Sessions, and kept in manual order without unexpected resorting.
- Git commit review adds inline changed-file diff inspection so review prompts can inspect file patches without leaving the modal.
- First-prompt title generation is more reliable, including Grok Build support, staged rename handling, and guards that avoid submitting skipped or stale generated titles.
- Native workspace focus, tab chrome, and sidebar shortcuts are tighter: visible Cmd+number slots match painted session rows, session-click focus is reinforced, context menus are centered, and close buttons use cleaner pane-tab chrome.
- Zmx-backed terminals can refresh stale persisted pane state for resize repair, and gxserver no longer carries legacy zmux chat project behavior into nightly sessions.
- Project board and Tasks flows improve ticket routing, comments, placeholders, and create/start handoff behavior.
- Window geometry, sidebar default width, title-agent previews, and `gx find` Accept All policy handling have been improved for nightly builds.

## 3.26.2 - 2026-06-02

- Native command bridge probes the login shell PATH once at launch so GUI-started agents can find OpenCode, mise, npm, and other tools installed through shell startup files.
- OpenCode integration setup refreshes the session plugin for newer OpenCode event APIs and reports installed when the Ghostex plugin file is present.

## 3.26.1 - 2026-06-01

- Mobile and remote CLI session commands fall back to persisted sidebar session state when the live Ghostex bridge is unavailable, so Android and other clients no longer show a misleading empty session list.
- Sidebar CLI bridge failures now return clearer JSON errors and more helpful guidance when a stale bridge token or closed socket causes the command to fail.

## 3.26.0 - 2026-05-30

- Project board adds a Backlog swim lane before Todo, per-lane + ticket creation, status selects with friendly labels, and more reliable Create & Start that launches the agent session before secondary board refresh work.
- Start Work prompts now ask agents to leave bead comments after each turn and include backlog/in-progress/test/review workflow commands.
- Starting work from the Kanban page focuses the created agent session immediately, matching sidebar session-card behavior.
- Command pane defaults restore to 125px (up from the prior smaller default), can grow up to 90% of the workspace height, and native Beads updates accept the backlog status.
- Dropdowns, selects, popovers, and tooltips share the same visible border as sidebar tooltips.
- Titlebar Tips & Tricks copy was refreshed for pinning sessions and using the Kanban board with agents.

## 3.25.0 - 2026-05-30

- Added a titlebar Tips & Tricks menu with unread tracking, read-all, and persistent read state for built-in workflow hints.
- Project board filtering now uses Priority and Estimate controls instead of lane status, with the search icon inside the search field and cleaner ticket metadata layout.
- Ghostty terminal scrollbars and scroll-to-top/bottom overlay buttons are square instead of rounded.
- Session rows show only one Delayed Send countdown clock in the leading identity slot instead of duplicating it in the header agent area.

## 3.24.0 - 2026-05-30

- Collapsed macOS agent and priority selects now show friendly labels instead of raw persisted values in Git commit review, session rename, worktree creation, agent configuration, settings, and Project board dialogs.
- Project name hovers in the sidebar now show a richer tooltip with project kind, path, git file counts, and current session/worktree totals.
- Native workarea, commands pane, and titlebar button separators use a subtler shared boundary color for cleaner chrome alignment.
- The Commands pane footer tooltip opens to the left of its icon so it no longer covers footer controls while the rest of the sidebar keeps below-trigger labels.
- First-prompt title generation overlay copy now reads "Generating title" without trailing ellipsis.

## 3.23.0 - 2026-05-30

- Migrated the sidebar to Base UI and refreshed the app theme styling for a more consistent control surface across modals, menus, and session chrome.
- Added a first-launch preferences page so new installs can set common defaults before opening sessions.
- Improved Git workflows with Sync with Main, a split Git menu by action type, prompt-agent Git PR review, and a unified merge flow.
- Added first-prompt title generation with a native terminal overlay, tighter auto-rename behavior for agents and slash commands, and sidebar wiring for the new flow.
- Improved Git and worktree status toasts with persistent running notices, spinner styling, success/error tints, and clearer completion when sessions close or worktrees delete.
- Removed the macOS Pane Gap setting and tightened native workspace chrome with flush tab bars, square status indicators, workarea separators, and zero default pane spacing.
- Improved sidebar and project-header tooltips so labels open below their triggers with a consistent square bordered surface.
- Refined session working/attention indicators so they sit closer to the row edge and the working spinner renders as a rounded ring again.
- Improved the titlebar update tooltip placement and aligned right-side titlebar controls flush with the window edge.

## 3.22.0 - 2026-05-29

- Fixed Homebrew installs on newer macOS releases by keeping the Ghostex cask minimum requirement at macOS Ventura.
- Improved the titlebar update button tooltip so it opens to the side and no longer sits under the promoted sidebar layer.

## 3.21.0 - 2026-05-29

- Added Cursor Agent support for prompt generation so session rename, Git review, worktree prompts, and Project board title generation work when Cursor is the selected prompt agent.
- Improved agent prompt staging so Ghostex waits for the terminal to be ready and uses consistent step delays before sending rename and prompt commands.
- Fixed Search by Text in Previous Sessions so it opens in the active project instead of the Quick/projectless terminal area.
- Improved project header action tooltips with portaled labels that stay visible inside narrow sidebar webviews.
- Shortened the Commands Pane footer hover label while keeping the full accessible button name.

## 3.20.0 - 2026-05-29

- Fixed session attention updates so they refresh pane chrome without stealing keyboard focus from the terminal you are typing in.
- Fixed Git commit review and New Worktree modals so session activity updates no longer reset in-progress drafts or agent selections.
- Improved sidebar session snapshots so unchanged HUD data keeps stable references across attention and activity updates.

## 3.19.0 - 2026-05-29

- Added Clone Repository from the Projects header and command palette, including native folder picking, flexible repository URL paste formats, and automatic project creation after a successful clone.
- Added bundled zehn prompt-history search through `gx find` and `gx f`, while keeping `gx s` as the sessions alias.
- Added Search by Text in Previous Sessions to open a fresh terminal running `gx f` beside the existing agent prompt workflow.
- Added per-modal prompt agent selection so Git commit review and other modals remember their own agent choice until Settings changes the default.
- Improved new-install defaults with completion bell and Accept All enabled, longer default auto-sleep for code, Git, and project panes, and tighter workspace chrome defaults.
- Improved the macOS titlebar on narrow layouts by hiding crowded controls below 620px, compacting Git primary labels, and counting agent-owned process trees correctly in Resources.
- Improved session focus so keyboard focus stays on visible panes and reference sidebar rows no longer show a passive working timer spinner.
- Improved the Project board with a create-and-start flow, clearer missing-Beads setup guidance, and Cua Driver permission status in Integrations.
- Improved Git commit review by moving the prompt agent selector into the footer and removing the duplicate review toast when the modal opens.
- Improved project header action tooltips so labels open below their buttons without clipping at the sidebar edge.
- Bundled Ghostex agent skills with the CLI install path.
- Removed session-card shortcut badges and the unused show-hotkeys-on-cards setting.

## 3.18.0 - 2026-05-28

- Added bundled zehn prompt-history search through `gx find` and `gx f`, while keeping `gx s` as the existing sessions alias.
- Added pinned sessions so important agent terminals stay at the top of a project, remain manually reorderable in last-activity mode, and can be toggled from the CLI with `pin-session`.
- Added auto-sleep controls for browser and project panes so idle embedded surfaces follow the same sleep policies as terminals.
- Improved the Project board with pasted image path storage, clearer ticket editor layout, and grouped ticket actions.
- Improved macOS worktree flows with tighter OS integration and clearer worktree delete confirmation copy.
- Improved browser session cards, tooltips, and the rich prompt editor by trimming trailing blank lines on save.
- Stopped auto-installing agent hooks on app startup; hook installation now requires explicit consent from first-launch setup or Settings -> Integrations.

## 3.17.0 - 2026-05-27

- Improved command-panel terminals so they appear in project session tracking, CLI session lists, project batch actions, and favorite controls instead of drifting outside the normal project session model.
- Improved project Sleep Inactive, Wake, and Reload actions so idle zmx-backed command terminals are included while working or attention sessions stay awake.
- Improved restored and restarted terminal handling so Ghostex waits for native terminal surfaces to finish attaching before treating a temporary missing-surface report as a failed pane.

## 3.16.0 - 2026-05-27

- Improved Cursor Agent resume so transcript paths captured from Cursor hooks are recognized as Cursor sessions even when a terminal previously inherited another agent identity.
- Improved agent activity status during launch, resume, fork, and manual startup so transient spinner or done titles are less likely to leave sessions stuck in attention state.
- Added an Editor setting to show untracked line counts in project-header diff stats only when there are no tracked line changes, while keeping tracked-only Starship-style counts as the default.

## 3.15.0 - 2026-05-27

- Added Ghostex-named agent skills for Browser Use, Computer Use, Agent Orchestration, and Generate Title, with CLI install commands and bundled app resources so agents can discover the right Ghostex workflows after Homebrew install.
- Improved first-launch setup and Settings -> Integrations with the public Ghostex Browser Use and Ghostex Computer Use names, including Desktop Control readiness checks for both Cua Driver and the Ghostex Computer Use skill.
- Improved `ghostex browser open` / `gx browser open` so agent-created browser panes are scoped to the current project or worktree and reuse existing same-origin or exact tabs instead of creating duplicates.
- Added Recent Projects right-click actions for Copy Path, Open in Finder, and Remove Project.
- Added Power Settings access from the titlebar keep-awake menu, icon-only keep-awake controls, and an option to hide the keep-awake titlebar control.
- Improved project headers by allowing larger four-digit changed-line counts before compact diff stats are capped.
- Removed legacy IDE and Canary attachment paths so browser and workspace actions stay centered on Ghostex's own panes.

## 3.14.0 - 2026-05-27

- Changed the short Ghostex CLI command from `gtx` to `gx`, with Homebrew setup checking for an existing non-Ghostex `gx` command before linking the alias.
- Added Ghostex browser DevTools MCP support so agents can inspect embedded browser panes, read console logs, take snapshots and screenshots, and interact with pages through the bundled CLI skill.
- Expanded first-launch setup with CLI, mobile app, and browser-skill guidance, including installed-CLI detection so Homebrew users are not asked to reinstall unnecessarily.
- Improved browser feedback tooling so browser panes honor the selected Agentation or React Grab tool and Agentation opens directly into feedback mode.
- Improved sleeping-session wake and focus behavior so pane tabs, command tabs, focus mode, and restored zmx/tmux/zellij sessions reopen in the expected pane instead of reshuffling visible layouts.
- Improved sidebar polish with unified tooltip styling, tighter Storybook/native layout matching, literal Show less limits for long project lists, and broader Sleep Inactive coverage for idle terminals.
- Improved Android companion behavior with background session-status refresh, attention notifications, sleeping-session icons, persisted project disclosure, and long-list Show more / Show less controls.
- Improved iOS companion builds with Ghostex-branded local device installs, safer CloudKit handling for debug builds, and a two-row customizable terminal accessory bar.

## 3.13.0 - 2026-05-25

- Added the Ghostex terminal TUI as the default `ghostex` / `gtx` experience, while keeping direct session attach available through the attach shortcuts.
- Added a first-launch setup experience with Ghostex workspace artwork, agent hook readiness, and install/refresh actions for supported agents.
- Added Git release workflow actions for reviewing changes, creating split commits, and handing off multicommit release work from the app.
- Improved Git review flows with a richer commit modal, changed-file diff inspection, and clearer project/worktree ordering.
- Improved Project board refresh behavior so ticket moves, edits, copied work prompts, and nearby Beads changes show up without manual refresh while large boards stay capped for smoother scrolling.
- Improved the Ghostex terminal TUI with the Herdr-backed terminal runtime, hotkey overlay polish, full zmx replay, and synced working/attention/idle status from persisted sessions.
- Improved titlebar and workspace chrome behavior with cleaner resource controls, tighter traffic-light alignment, and safer modal/menu click handling above native panes.
- Improved cross-platform Ghostex parity for project workflows, workspace persistence, browser/code/git panes, settings, modals, and release packaging.
- Improved mobile and persistent-session stability with clearer Android remote-session activity, more responsive iOS direct attach, and better cleanup for zmx, zellij, and other persisted terminal sessions.
- Restored Monaco as the default local Ctrl+G prompt editor while preserving terminal-native prompt editing for SSH sessions.

## 3.12.0 - 2026-05-23

- Improved restored persistent sessions so focusing an already-running tmux, zmx, or zellij-backed terminal no longer sends restore text or resume commands into the live prompt.
- Improved app modal click handling so Settings and Agents Hub stay fully interactive above native pane tabs and workspace chrome in narrow layouts.
- Improved Ghostex iOS direct attach with paced terminal rendering, smoother scrollback gestures, and better responsiveness during animated terminal output.

## 3.11.0 - 2026-05-23

- Added a beads-backed Project board in Project mode with kanban lanes, full-text search, status filters, comments, labels, image previews, and Linear-style ticket keys.
- Added reversible session focus mode with pane-tab controls, session-card focus behavior, and a titlebar Exit Focus action.
- Added Ghostex CLI session selectors plus read/send message commands for scripting live sessions from the terminal.
- Improved Code, Git, and Project side-pane behavior so clicking a session while the companion pane is hidden returns to Agents view and focuses the selected session.
- Improved Settings on narrow windows so top tabs and section navigation wrap cleanly instead of clipping.
- Improved zmx and iOS remote attach stability with visible-only replay, better resume fallback behavior, and smoother direct SSH terminal rendering.

## 3.10.0 - 2026-05-23

- Added a beads-backed Project board in Project mode with draggable kanban columns for creating, moving, and commenting on project issues.
- Added reversible session focus mode from pane tabs and session cards so one pane tab group can be isolated while Code, Git, or Project surfaces restore on unfocus.
- Added agent hook status and install controls in Settings -> Agents so reliable-resume agents show machine-local hook setup and can be installed from the app.
- Improved titlebar Quit actions so Resources can terminate the live PIDs shown in the menu instead of relying only on sidebar sleep.
- Improved zmx attach and resume through visible-only replay for live sessions, saved fallback resume commands, and clearer failed-resume panes.
- Improved Settings modals hosted in the sidebar with a dimmed backdrop that dismisses on click, matching full-window modal behavior.
- Improved Ghostex iOS direct SSH attach responsiveness with batched terminal output and clearer attach progress.

## 3.9.1 - 2026-05-23

- Made `gte` the default Ctrl+G prompt editor so new installs open the terminal-native editor without extra setup.
- Improved terminal shortcut routing so Cmd+G opens agent prompt editing, and common Mac editing shortcuts reach terminal apps such as `gte` instead of being swallowed by the app menu.
- Improved sidebar and toast layering so sidebar navigation stays clickable while app toasts are visible and toast-only overlays no longer steal workspace clicks.
- Improved the Delayed Send terminal countdown badge with more padding for better readability.
- Improved iOS direct SSH attach responsiveness by batching remote terminal output, reducing render spam, and showing clearer attach progress.

## 3.9.0 - 2026-05-23

- Added hidden restorable launchers for Rovo Dev, Hermes Agent, CodeBuddy, and Qoder with matching icons and cleaner session-title restore support.
- Added Agentation as the default browser-pane feedback tool, with a Settings option to switch the browser action back to React Grab.
- Improved agent session restore by capturing native agent session ids through installed hooks and showing the captured id in session-card tooltips.
- Improved terminal defaults with the GitHub Dark profile, JetBrains Mono, a lighter font weight, more scrollback, protected clipboard behavior, and one-to-one mouse scrolling.
- Improved rich prompt editing with the renamed `gte` terminal editor option, system-inherited editing, and custom editor commands.
- Improved terminal mouse behavior so Command-clicks and modifier changes are reported reliably to terminal apps.
- Improved app modal and workspace chrome behavior so Escape closes full-window modals reliably and native pane controls keep receiving clicks.
- Improved project drag previews so insertion lines stay stable while dragging across expanded project groups.
- Added native Ghostty terminal groundwork for the iOS app and local iPhone build/install scripts.
- Added mobile app availability notes for TestFlight and Android APK downloads.

## 3.8.0 - 2026-05-21

- Added Quit controls to the titlebar Resources menu so individual resource groups or all managed sessions can be closed from one place.
- Improved the Resources menu for zmx users with clearer persistence guidance, less crowded header actions, and collapsed diagnostic resources by default.
- Improved the pet overlay so attention/completed session cards appear ahead of active working cards while preserving sidebar order within each status.
- Improved embedded Code, Git, and browser panes so command-pane resizing stays visually stable during live drag gestures.
- Improved compact sidebar layouts so collapsed Quick and Projects sections no longer create extra hidden scroll space.

## 3.7.0 - 2026-05-21

- Improved embedded Code, Git, and browser panes so showing or hiding the command pane no longer shifts page content upward over repeated resizes.
- Improved Delayed Send with a clearer floating countdown badge in terminal panes, non-blocking scheduling feedback, and a timer dialog that selects the minutes field when opened.
- Improved restart recovery so sessions that quit while working or needing attention wake again on the next launch.
- Simplified the sidebar by removing hidden legacy Agents, Actions, Browsers, and project-header surfaces from the React sidebar.
- Improved project group reordering with a compact cursor-following drag preview for large expanded projects.

## 3.6.0 - 2026-05-21

- Added Cursor CLI, Antigravity CLI, and Amp CLI as built-in agents with matching icons, launch commands, title cleanup, and working/done detection.
- Added global and per-agent Accept All controls for supported agent CLIs, including a first-launch setup surface for choosing the default behavior.
- Added project worktree workflows for creating a new worktree from a prompt, launching an agent into it, reviewing changed files, and optionally cleaning up the worktree after git actions.
- Improved sidebar Actions so each project can keep its own action list while worktrees inherit their parent project's actions.
- Improved the Ghostex CLI so running `ghostex` or `gtx` with no subcommand lists sessions in the same project order as the app, and zmx-backed resume can recreate a missing named session when possible.
- Improved the pet overlay with actionable status badges, a Go to Ghostex menu action, a Sleep Pet action, and an additional pet sprite.
- Made the floating prompt editor open faster after startup by warming the editor host before the first real prompt edit.
- Improved Delayed Send reliability by preserving pending deadlines with restored terminal sessions.
- Added a Storybook regression story for large real project lists so sidebar scrolling and project reachability are easier to verify.

## 3.5.0 - 2026-05-18

- Added a much more complete Ghostex Android remote workflow with built-in connection handling, session attach, remote actions, session creation, file upload, and shareable phone-side diagnostics without requiring phone-side OpenSSH or sshpass setup.
- Improved Ghostex Android navigation with a cleaner drawer, quick refresh, explicit Machines, Settings, and Exit controls, collapsible project groups, project reordering, and project-level session creation.
- Added Ghostex Android terminal settings inside the drawer for common terminal behavior and display options.
- Improved zmx-backed terminal stability on macOS so panes refresh more reliably after resize, mode switches, pop-out changes, and sleeping-session wake.
- Improved sleeping-session wake behavior so restored sessions return to the focused tab group instead of unexpectedly reappearing as separate split panes.

## 3.4.1 - 2026-05-17

- Added visible Delayed Send countdowns in the sidebar, native pane tabs, and terminal panes so scheduled sends are easy to spot before they fire.
- Improved Delayed Send controls so reopening an active timer shows the remaining time and lets you cancel or reschedule it.

## 3.4.0 - 2026-05-17

- Added a native right-click menu in terminal panes with Copy and Paste actions.
- Improved Last Active timestamps so sleeping and restored terminal sessions keep accurate sidebar times and sorting after restart.
- Improved the titlebar Resources menu so Browser Tabs only count Ghostex embedded browser helpers and use clearer browser process labels.

## 3.3.0 - 2026-05-17

- Added image paste support to the rich prompt editor, including durable local image references, thumbnail previews, full-size preview popups, and one-click image removal.
- Improved Open In menus and Settings with recognizable editor brand icons for Cursor, VS Code, Zed, Antigravity, VSCodium, and JetBrains-family editors.
- Added Show less / Show more controls for long project session lists so large projects stay easier to scan.
- Added configurable hotkeys for pane actions and the first five custom Actions, including browser pane, rotate panes, merge tabs, delayed send, fork, reload, and pop out.
- Improved sleeping persisted sessions so sleeping releases the underlying provider runtime and external `ghostex` / `gtx` attach resumes sleeping sessions correctly.
- Improved the titlebar Resources menu so browser memory rows show the actual tab title and URL instead of raw browser process labels.
- Moved the Wake/Sleep Pet control into the sidebar overflow menu so pet controls live next to session tools while the titlebar stays focused on workspace actions.
- Improved rich prompt editor focus so clicking blank editor chrome reliably keeps typing inside the prompt editor.

## 3.2.0 - 2026-05-16

- Added a titlebar Resources menu that shows live CPU and memory usage grouped by project, session, Ghostex runtime, and browser tabs.
- Added a one-click titlebar action to sleep inactive agent sessions from the Resources menu.
- Added persistent hide/show behavior for the agent side pane in Code, Git, and Project modes, with a titlebar restore button when the pane is hidden.
- Improved custom terminal actions so command panes are reused by action title and duplicate action titles are blocked before saving.
- Improved project headers with more reliable right-click menus and easier-to-scan agent launcher icons.

## 3.1.0 - 2026-05-16

- Added a full-window Command Palette on `Cmd+K` for Ghostex actions, project actions, Settings, pane controls, and pet controls.
- Added sidebar display presets for Codex, Minimal, and Detailed layouts so users can quickly choose how quiet or information-rich the sidebar should be.
- Improved Agents Hub editing so saved files update immediately in the open modal and external editor buttons open the right folder with the selected file focused.
- Moved browser pane sessions into their project groups and removed the separate Browsers sidebar section so project work stays grouped together.
- Improved project Git/browser panes with project tabs, browser toolbar support, and more reliable active project selection.
- Improved Previous Sessions search with the newer modal styling and multiline query input.
- Improved the `ghostex` / `gtx` CLI help and session listing so commands are easier to scan and sessions follow the sidebar's Last Active order.

## 3.0.0 - 2026-05-15

- Added a Tips & Tricks guide inside Ghostex with practical pages for workspace basics, agents and sessions, actions and browsers, Codex setup, and remote access from a phone or another machine.
- Added bundled `ghostex` and `gtx` command-line launchers to the app so Homebrew installs both commands automatically for session listing, attach, wake, focus, and sleep workflows.
- Changed new session labels to shorter `g-MMDD-HHMMSS` identities so sidebar numbers and tmux, zmx, or zellij session names are easier to read and reuse.
- Improved sleeping-session restore behavior so waking a session puts it back into the active tab group instead of disrupting the current split layout.
- Polished project workspace controls with clearer titlebar modes, better project panel behavior, and a cleaner path from empty project headers into a first terminal.
- Improved native pane and browser stability around focus, tab scrolling, titlebar actions, and embedded browser resizing.
- Improved rename handling so pasted titles are cleaned into readable session names before they are saved.

## 2.7.0 - 2026-05-15

- Completed the Ghostex public naming cleanup across release, app, Homebrew, and generated CEF helper surfaces.
- Added Factory Droid and Grok Build as built-in agent options with bundled icons, sidebar labels, and session metadata support.
- Renamed the default Pi launch option to Pi Agent while keeping `pi` as the command.
- Changed directional pane focus defaults to `Cmd+Alt+Arrow` so normal `Cmd+Arrow` text-editing behavior is not stolen by workspace navigation.
- Added a searchable action icon picker in Settings so custom sidebar actions can choose icons faster and keep accessible labels visible.
- Moved project git diff stats into project headers and removed the separate code-editor sidebar row so project groups scan more compactly.
- Added titlebar modes for Agents, Code, Git, and Project so project/editor surfaces can be reached from the native titlebar without crowding the sidebar.
- Renamed the tasks-backed titlebar surface to Project while keeping its placeholder bundled locally and preserving existing internal mode IDs.
- Removed the Back to Agents View button from code/git companion panes so the companion titlebar only exposes pane dismissal.
- Kept the titlebar mode switcher's active pill animation visible while moving between distant modes.
- Moved Rotate Panes into the pane overflow menu, added Merge All Tabs, and improved command-panel tab creation in clicked tab groups.
- Kept workspace pane tabs readable with a wider minimum tab width while preserving horizontal scrolling in narrow multi-tab panes.
- Improved prompt editor hit routing, collapsed command-panel sizing, previous-session restore targeting, favorite backfill, and semantic Last Active tracking.
- Preserved full generated session titles in stored titles and hover tooltips even when live terminal titles report an ellipsized prefix.
- Hid tmux, zmx, and zellij persistence-provider letters from session-card agent icons while keeping provider metadata available for attach commands and tooltips.
- Fixed reference-sidebar primary labels so descenders remain visible in New Session, Agents Hub, Plugins, Search, Settings, and Recent Projects rows.
- Added Tasks placeholder bundling and Git/browser mode helpers for the new native workspace modes.
- Ignored legacy pre-rename generated web assets so old `zmuxHost` build output does not appear as source work.

## 2.6.0 - 2026-05-15

- Improved Agents Hub profile tooltips with structured profile labels, instruction file paths, target paths, and Finder actions that stay readable for dense local agent configurations.
- Added a setting to hide Last Active timestamps on active session cards while letting titles use the full card width without overlapping status dots or close buttons.
- Hid browser page history from the Previous Sessions modal so the restore flow stays focused on agent sessions.
- Polished Previous Sessions and session-card metadata alignment, including fixed timestamp columns, project-label positioning, and clearer brand-colored agent icons.
- Simplified the sidebar overflow menu by removing completion sound, persistence, and remote-access controls from that compact menu while keeping scratch tools, running state, hotkeys, and help.
- Updated Next Tab and Previous Tab navigation to follow the visible sorted sidebar order, including collapsed Combined sections.
- Improved project-editor and command-pane hit routing so active editor surfaces, companion panes, command tabs, and resize handles receive the intended native clicks.
- Added native activation diagnostics and CEF layout logging to investigate focus steals and browser-pane geometry drift from app lifecycle and native frame snapshots.
- Added pane-tab geometry diagnostics, adjusted workspace tab-bar sizing, and made non-command tab add buttons use square chrome.
- Added provider/session context labels and first-run persistence notices for tmux, zmx, and zellij-backed terminal sessions.
- Kept macOS attention notifications minimal by using the session name as the title and project name as the body.

## 2.5.1 - 2026-05-15

- Published a native Intel x86_64 build beside the Apple Silicon build, with a separate Intel Sparkle feed and an architecture-aware `ghostex` Homebrew cask.
- Clarified the README install flow so the same `brew install --cask maddada/tap/ghostex` command automatically selects Apple Silicon or Intel.
- Changed sidebar actions to always use an explicit icon, defaulting new and legacy actions to the Play glyph with editable color.
- Added a titlebar pet control that toggles the floating pet overlay through persisted settings and keeps the overlay state synchronized.
- Resized the floating pet overlay to fit the sprite when no activity bubbles are visible while preserving the wider activity panel when messages appear.
- Improved Commands panel focus restoration so collapsing command terminals returns keyboard focus to the previous workspace terminal.
- Improved Reload Session placement so reloaded terminals replace the clicked pane/tab instead of appending as a new split.
- Polished Previous Sessions rows with centered restore content, an X delete control, and active-session icon hover behavior that does not dim the focused row.
- Consolidated sidebar resize ownership, aligned pane tab heights, and hid active pane borders in single-pane workspaces.
- Updated local native launch behavior so `bun run start` uses architecture-specific DerivedData paths for arm64 and x86_64 builds.

## 2.5.0 - 2026-05-14

- Added dual-architecture release pipeline support for separate Apple Silicon and Intel DMGs, separate Sparkle feeds, and an architecture-aware Homebrew cask.
- Renamed the public app surface from Ghostex to Ghostex while keeping internal repository, code, storage, bundle id, and historical asset names under `ghostex`.
- Changed the public CLI command to `ghostex`, with `gtx` as the short alias, and intentionally stopped documenting `ghostex` as a CLI compatibility command.
- Updated README install and CLI examples so `brew install --cask maddada/tap/ghostex` is the public install command.
- Updated the reference sidebar workflows with a combined-only layout, improved command panel behavior, searchable settings sections, refined hotkey navigation, and cleaner Previous Sessions rows.
- Improved Agents Hub so it loads the real local catalog, supports in-place saving, and avoids bundling private placeholder profile data.
- Added floating Monaco prompt editing with resize/move behavior, save/cancel status handling, and safer terminal-close persistence.
- Improved native pane chrome, focus/resize hit ownership, project editor routing, commands panel tab controls, and embedded browser pane handling.
- Restored direct native terminal scrollbar behavior so embedded Ghostty surfaces keep scrollback geometry, scrollbar rendering, and precise trackpad momentum.
- Added a floating pet overlay with clickable activity bubbles that bring Ghostex forward and focus the exact session shown above the pet.
- Added release handover docs and updated the release workflow so future agents keep GitHub Releases, Sparkle, and Homebrew aligned for both architectures.

## 2.3.2 - 2026-05-10

This patch release adds session attention notifications and tightens project editor row alignment.

- Added optional macOS attention banners for sessions that need attention, including Settings control, native notification permission handling, click-to-focus routing, and sidebar rate limiting.
- Kept attention notifications separate from completion sounds so users can enable clickable system routing without audible alerts.
- Improved project editor diff-row alignment in the reference sidebar and expanded Storybook fixture coverage for open editor rows with diff stats.

SHA256: `61d2d71547b492eb732483d09193df3cb3de2b475f86f7916f75344d89daf220`

## 2.3.0 - 2026-05-10

This minor release improves the 2.x workspace with stronger hotkey editing, richer prompt editing, native runtime fixes, and more predictable sidebar behavior.

- Added a shortcut recorder for hotkey settings so Command chords are captured directly instead of typed into text fields.
- Updated split-count shortcuts to single-chord defaults and added direct Split More / Split Less actions for faster workspace layout control.
- Added opt-in Rich Prompt Editing with gte, including Settings UI, native install routing, environment injection, and zsh startup shims that keep gte in charge after shell profiles load.
- Added installed-app CLI proxying so terminal commands such as `ghostex --help` and `ghostex sessions` run the bundled Node CLI before the macOS app starts.
- Improved native command execution by normalizing GUI-launched process `PATH` values so background commands can find common developer tools.
- Improved terminal search keyboard behavior, centering, and neutral styling for embedded Ghostty panes.
- Added active-project names to the macOS title bar while keeping chat workspaces labeled as Ghostex.
- Added focused native pane-reorder diagnostics and rejected stale title-bar hits so bottom-edge terminal selection does not become pane dragging.
- Changed provider-backed terminal recreation so reload, wake, restore, and previous-session restore follow the current Settings provider while attach-command inspection still uses stored provider metadata.
- Separated project agent launching from plain terminal creation so project headers have distinct agent and terminal controls.
- Changed the Combined sidebar top row to New Session so it creates in the active project/chat context while chat creation stays in the Chats section.
- Added persistent sidebar collapse state and a Settings toggle for showing project editor changed-file counts.
- Polished sidebar spacing and session-title truncation so reference layout controls and session cards scan more cleanly.
- Updated README development setup and feature wording for the current Ghostty fork and 2.3 workflow.

SHA256: `aabfea87f042ab59e1eb8aabd371226108df5a980edccbee80f58b26d7a80d70`

## 2.2.0 - 2026-05-09

This minor release tightens the new 2.x interface with the latest workspace, settings, and release workflow polish.

- Added a unified tabbed Settings dialog that brings Settings, Agents, Actions, and Hotkeys into one configuration surface.
- Added lazy `~/.ghostex` folder usage stats and an Open ghostex Folder action from Settings.
- Added menu bar session status indicators while making floating desktop indicators independently optional.
- Renamed orange agent status from running to working so agent activity is distinct from live runtime state.
- Improved project editor rows so opening and error states stay visible, show diagnostics, and can be retried instead of disappearing.
- Improved session card and Previous Sessions row chrome with hover close controls, clearer last-active placement, and refined editor diff labels.
- Added a separate `start:dev` app startup path for `ghostex-dev` so normal `bun s` keeps release-like behavior.
- Updated README presentation and feature wording for the current Ghostex positioning.

SHA256: `73340ec06d57c3b16a585ee9c5566513c91fd5e0a6cba9477ae5982a122521c9`

## 2.1.0 - 2026-05-08

- Continued the 2.x UI refresh messaging: ghostex now presents the redesigned simplified Codex-style workspace, refreshed project groups, action controls, tooltips, session cards, settings surfaces, and updated screenshots.
- Continued the 2.x stability and performance focus across native sidebar sync, AppKit relayout avoidance, shared storage writes, diagnostic filtering, and workspace visibility.
- Added the macOS application icon from agent-manager-x so Finder, Dock, app switcher, and signed release builds use the intended branded icon instead of a generic app icon.
- Compiled the icon through Xcode's `AppIcon` asset catalog so signed and notarized release bundles carry the same icon metadata as local builds.

SHA256: `6bbd2a95f1f585df20a2811c8f2cae492ad53492bc13814b4b085c5a906e9ced`

Install with Homebrew: `brew install --cask maddada/tap/ghostex`

## 2.0.0 - 2026-05-08

- Changed the whole ghostex UI around the simplified Codex-style workspace: refreshed top chrome, project groups, action controls, tooltips, session cards, Previous Sessions rows, settings surfaces, icons, and README screenshots.
- Added native workspace visibility helpers and tests so sidebar/native sync can avoid unnecessary workspace work while preserving visible pane behavior.
- Improved restore and fork actions for native terminal title bars, including Codex and Claude fork command paths.
- Fixed first-prompt auto-rename so meaningful terminal-synced titles are preserved instead of being overwritten by redundant generated rename commands.
- Updated Storybook sidebar scenarios, interaction readiness, and fixtures so visual checks match current local settings and the redesigned sidebar behavior.

SHA256: `da519a720e65a955ce182f0655ba36a6cb02c188aab441142dc2bf9747f70456`

Install with Homebrew: `brew install --cask maddada/tap/ghostex`

## 1.4.11 - 2026-05-08

- Added reference-style sidebar action flows, modal flows, story fixtures, and Combined layout refinements.
- Added Pi as a supported agent option with icon assets, tests, and agent configuration UI wiring.
- Improved sidebar group, session-card, search, modal, and scroll styling to better match the reference layout.
- Improved floating session status indicators with refined drawing, attention/working visual treatment, and additional settings support.
- Improved session title, activity, rename, and first-prompt metadata handling so loading and restored-title states are more reliable.

## 1.4.10 - 2026-05-08

- Added human-facing `ghostex` CLI session commands for listing, attaching, resuming, killing, sleeping, waking, and focusing running terminal sessions.
- Added provider-backed attach metadata so tmux, zmx, and zellij sessions keep their stored provider, show sidebar badges, and expose copyable attach commands.
- Added a Settings control for floating session status indicator size, plus updated indicator drawing, tooltip wrapping, and settings-control polish.
- Fixed main window chrome restore so ghostex reopens at the prior size, position, and display while avoiding offscreen IDE-attachment coordinates.
- Fixed Find Previous Session routing so the footer button opens the prompt even with an empty modal search field and logs the modal/native bridge path.
- Improved session title sync by rejecting Ghostty ghost placeholder titles and protecting trusted restored titles from automatic rename overwrite.

## 1.4.9 - 2026-05-07

- Improved embedded code-server editor panes so VS Code panel/sidebar drag and drop keeps live hover and drop targeting while using CEF.
- Fixed embedded browser/editor pane teardown so closing a pane from the sidebar does not close the top-level app window.
- Improved project editor persistence so VS Code workbench layout survives app restarts without putting code-server into a fresh Chromium profile.
- Improved zmx session persistence so empty sessions attach directly, startup commands run only for new sessions, and inherited zmx session variables do not hijack app-managed names.
- Improved zellij session persistence so generated session names stay within provider limits and new sessions launch under the same name used for restart attach.
- Enlarged README screenshots for clearer GitHub documentation.

## 1.4.8 - 2026-05-06

- Added embedded code-server editor panes so project groups can open a native CEF-backed code editor surface.
- Added project header controls for opening project-scoped browser panes and project editor panes from the clicked group.
- Added zellij as an opt-in terminal session persistence provider alongside tmux and zmx.
- Added a sidebar side setting so users can choose left or right placement from Settings, including startup restore and legacy side migration.
- Added modified-setting indicators with per-setting reset-to-default tooltips in Settings.
- Replaced native `title` attributes across sidebar controls with shadcn/Radix app tooltips and shared local brand icons.
- Improved project editor panes so middle-click closes the editor surface while preserving project diff stats and runtime sleep behavior.
- Improved code-server editor drag/drop by disabling native pane resize/header reorder interception while editor panes are visible and logging passive CEF drag diagnostics.
- Fixed right-side sidebar layout so the resize divider sits between the workspace and sidebar instead of on the outside edge.
- Fixed Combined-mode project groups so empty project groups remain expandable for editor cards while browser and non-project groups still auto-collapse.
- Removed versioned Sparkle release-note markdown files from the repository.

## 1.4.7 - 2026-05-06

- Added persistent terminal session providers so terminal metadata, restore inputs, and provider state can survive app restarts.
- Added Chromium CEF native browser support with vendored CEF build wiring, persistent browser storage, and cookie flushing on app termination.
- Added shared Ghostty settings so terminal configuration can be reused across the native host and sidebar settings surfaces.
- Added native floating session status indicators for working, attention, and available session counts, including click-to-focus routing back into the workspace.
- Added a Configure Actions modal with readable action rows plus create, edit, and delete flows for sidebar project actions.
- Added Previous Sessions restore for archived terminal session records so restored sessions keep agent identity, first-message metadata, title provenance, favorites, and resume inputs.
- Filtered placeholder Previous Sessions entries so default titles such as `Terminal Session` and `Codex Session` are not saved as low-signal history cards.
- Improved Previous Sessions project restore by switching back to the original project, reviving Recent Projects entries, or recreating the project when needed.
- Fixed sparse Combined sidebar scrolling so empty/collapsed project lists stay pinned instead of rubber-banding or preserving stale scroll offsets.
- Fixed Combined-mode Chats grouping so the synthetic Chats group marker survives sidebar-store normalization.
- Improved native pane drag and reorder handling so hit testing stays scoped to pane headers while terminal/body interactions keep their expected routing.
- Improved terminal close cleanup by skipping redundant Ghostty close requests once a process has already exited.
- Adjusted native sidebar and Storybook layout so project panels can use the right edge rail without being clipped.

## 1.4.6 - 2026-05-05

- Replaced native title-bar action controls with compact sidebar Actions dropdowns for project commands and Open In targets.
- Added explicit Open In choices for Finder, Visual Studio Code, and Zed, including brand icons and persisted primary target selection.
- Added removable Actions dropdown rows so configured project actions can be deleted from the same menu that runs them.
- Moved custom workspace color selection into the workspace Theme context menu with a recent-color palette, removing the separate workspace config modal.
- Improved empty Combined-mode Chats and project groups so they auto-collapse while empty, expand when sessions appear, and show static folder/chat icons instead of inactive chevrons.
- Improved Recent Projects styling to match normal sidebar group rows and show preserved session counts inline.
- Expanded Codex first-prompt hook installation to existing Codex profile homes so first-prompt auto-title capture works when `CODEX_HOME` points at a profile directory.
- Finished native-only cleanup by removing the retired VS Code extension/workspace webview sources from Storybook and TypeScript configuration.

## 1.4.5 - 2026-05-05

- Added native title-bar split controls for primary Actions and Open In commands while keeping empty title-bar space draggable.
- Added React-rendered title-bar dropdown menus for configured ghostex actions and Open In targets, reusing the existing sidebar command and selected-IDE state.
- Improved terminal focus sync so passive layout/status updates no longer steal focus from the terminal or modal the user is actively typing in.
- Improved embedded Ghostty terminal color handling by removing inherited color-disabling environment keys at the native surface boundary.
- Added optional CEF prototype scaffolding for future Chromium browser panes while keeping the default WKWebView build path buildable without the Chromium SDK.

## 1.4.4 - 2026-05-04

- Added Combined sidebar mode so native ghostex can show one project group per project across all projects, while preserving Separated mode for the previous per-project layout.
- Added a Recent Projects drawer with fuzzy project/path search and startup cleanup for empty combined-mode projects.
- Added project context actions for opening project config, setting project theme, copying the project path, opening the folder in Finder, opening it in the selected IDE, and closing projects into Recent Projects.
- Fixed sidebar resize drags to use stable window coordinates so the sidebar does not jump while dragging.
- Added color-environment diagnostics for agent launches so monochrome CLI sessions can be traced to inherited terminal environment values.
- Added long-paste rename handling that summarizes pasted session text before syncing the rename into the agent CLI.

## 1.4.3 - 2026-05-03

- Added an opt-in Browser Panes mode that opens browser actions as first-class workspace panes instead of Chrome Canary windows.
- Added native browser pane controls for address navigation, reload, DevTools, React Grab, and profile selection.
- Persisted browser pane URLs, favicons, and browser-auto titles so sidebar cards and app restarts reflect the current page.

## 1.4.2 - 2026-05-02

- Fixed Sparkle update detection by publishing releases with a monotonic `CFBundleVersion` build number.
- Kept the native AppKit pane resizing changes from 1.4.1 available in the update feed.

## 1.4.1 - 2026-05-02

- Moved split pane resizing into the native AppKit terminal workspace so Ghostty and WKWebView panes resize from the same layout owner.
- Removed the React workspace resize overlay and tests that no longer apply to native pane sizing.
- Removed whole-cell terminal body stepping so pane chrome and terminal renderer widths stay aligned during native resize.

## 1.4.0 - 2026-05-02

- Added Sparkle appcast update support with signed appcast metadata for native macOS updates.
- Added draggable workspace pane resizing with double-click equalize behavior for pane rows and columns.
- Added a standard native macOS app menu with About, Check for Updates, Settings, Services, Hide, and Quit.
- Added a setting to hide the native IDE title-bar attach button without disabling IDE attachment.
- Improved IDE attachment behavior so the floating Show IDE button raises or launches the configured IDE for the current workspace.
- Kept the local release workflow skill available on this machine while removing it from the public repository tree.

## 1.3.0 - 2026-04-30

- Added Ghostty config actions and a recommended Ghostty config that includes ghostex-managed color, cursor, font, scroll, and split-opacity settings.
- Added a cyan Ghostty palette default to improve terminal color readability with the recommended ghostex-managed config.
- Added a local agent release skill for repeatable split commits, release notes, GitHub releases, and Homebrew cask publishing.
- Added Generate Name diagnostics across the sidebar, bridge, and controller paths so silent session-name failures are easier to trace.
- Fixed terminal title bars so long titles are measured from raw text and use available pane width before truncating.
- Improved attached IDE refocus timing so ghostex resurfaces faster when the IDE is already active or when activation retries succeed quickly.
- Hid bare agent status words such as `Working`, `Done`, `Idle`, `Thinking`, and `Error` from visible terminal titles.

## 1.2.0 - 2026-04-29

- Added terminal scroll multiplier settings for precision devices and discrete mouse wheels.
- Synced Ghostty mouse-scroll-multiplier values into the shared Ghostty config and reloads scroll-only changes immediately.
- Added native AVFoundation sound playback for completion/action sounds and settings previews, with sound assets bundled in the app.
- Gated non-error native/sidebar diagnostics behind Debugging Mode and reduced high-frequency focus/title logging.
- Improved terminal close cleanup by terminating processes still attached to the closed terminal tty.
- Improved embedded terminal search behavior so Escape closes search before reaching terminal programs.
- Changed embedded terminal cursor rects to use the default pointer cursor instead of always showing the I-beam.

## 1.1.0 - 2026-04-29

- Improved previous-session search launching by routing modal input through the sidebar/native command bridge.
- Fixed agent wrapper process launch so interactive CLIs stay attached to the foreground terminal TTY and receive resize signals.
- Added agent wrapper debug logging for TTY/process details used to diagnose resize and child-process issues.
- Fixed native embedded terminal layout to step pane sizes to whole Ghostty character cells, including configured terminal padding.
- Expanded native terminal resize diagnostics with core Ghostty grid, padding, backing-pixel, and pane geometry metrics.

## 1.0.4 - 2026-04-28

- Added configurable app hotkeys, including native AppKit handling while terminal panes have focus.
- Added saved first-message metadata for agent sessions and a copyable "View 1st Message" modal in active and previous session flows.
- Added terminal workspace background color settings and native pane-gap/background rendering.
- Added automatic Zed workspace syncing after ghostex workspace switches, controlled by a setting.
- Added native main-window size persistence between launches.
- Added native terminal search bar rendering and focus preservation improvements for modal workflows.
- Improved sidebar sessions to default to last-activity ordering and keep agent-icon mode blank for iconless sessions until hover.
- Expanded command/workspace icon choices and kept the icon picker search fixed while the icon list scrolls.
- Improved Previous Sessions by using the search field for "Find Session" prompts and keeping the native full-window modal compact.
- Added Scratch Pad focus diagnostics to help trace terminal-first-responder focus steals without logging note text.

## 1.0.3 - 2026-04-28

- Added native terminal title bars with rename, fork, reload, sleep, and close actions.
- Added visible native Ghostty scrollbars and disabled middle-click paste in embedded terminals.
- Added workspace configuration for dock name, theme, Tabler icon, and uploaded image.
- Added `ghostex-dev` build/run flavor with separate diagnostics storage and shared workspace/session state.
- Added shared sidebar storage files for projects, previous sessions, and settings outside WKWebView localStorage.
- Added managed native sidebar action sessions with command run indicators and close-on-exit behavior.
- Improved first-prompt auto-title logic so meaningful existing titles are not overwritten.
- Improved session rename modal Enter-key submission.
- Improved IDE attachment settings so Zed and Zed Preview are distinguishable.
- Removed the browser section tab-count badge.
- Removed persistent helper terminal mode in favor of the embedded Ghostty SurfaceView backend.
