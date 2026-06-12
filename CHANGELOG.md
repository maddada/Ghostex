# Changelog

<!-- CDXC:Distribution 2026-05-30-09:52: Release notes for 3.26.0 must include all user-facing commits after v3.25.0 so README, GitHub, Sparkle, and Homebrew describe the same shipped behavior. -->
<!-- CDXC:BetaDistribution 2026-06-05-22:26: Release notes for 4.0.0-beta.1 must make clear that the nightly beta is distributed through GitHub Releases and Homebrew without moving Sparkle users onto the beta feed. -->
<!-- CDXC:BetaDistribution 2026-06-06-06:35: Release notes for 4.0.0-beta.2 must keep the nightly beta visible in GitHub Releases and Homebrew DMG installs while leaving Sparkle automatic-update feeds on the current public release. -->
<!-- CDXC:BetaDistribution 2026-06-06-07:41: The final beta.2 package includes diagnostic throttling, generated-shell-input history suppression, and clone/rename/session timer fixes, so release notes must describe those user-visible stability changes before GitHub and Homebrew publication. -->
<!-- CDXC:BetaDistribution 2026-06-07-00:09: Release notes for 4.0.0-beta.3 must describe the nightly beta as GitHub Releases and Homebrew DMG distribution only, with Sparkle automatic-update feeds left on the current public release. -->
<!-- CDXC:BetaDistribution 2026-06-07-00:09: The beta.3 package includes sidebar focus stability, editor startup failure surfacing, title-storm throttling, packaged gxserver Node-runtime validation, native web bundle compiler support, and privacy-tightened debug logs, so release notes must stay user-facing while covering those shipped behaviors. -->
<!-- CDXC:BetaDistribution 2026-06-07-02:47: The final beta.3 package also includes color-capable subprocess launches, internal Codex title-generation isolation, safer Codex resume lookup, zmx title-observer health for Auto Sleep, focused-agent auto-sleep protection, T3 startup retention, packaged T3 runtime path resolution, and native-only T3/browser sidebar row restoration. -->
<!-- CDXC:BetaDistribution 2026-06-07-08:57: The final beta.3 package additionally includes gxserver-owned agent hook installation and hook-event ingestion, duplicate attention side-effect suppression, in-place zmx Full reload, bundled-zmx prompt-editor capability checks, Previous Sessions command filtering, client-clock relative-time ticking, and zehn day-grouped title rows. -->
<!-- CDXC:Distribution 2026-06-08-12:00: Release notes for 4.0.0 must consolidate the shipped nightly beta work into one public Sparkle, GitHub, and Homebrew release without beta-only distribution bullets. -->
<!-- CDXC:Distribution 2026-06-08-10:02: Release notes for 4.0.1 must cover the post-4.0.0 nightly merge as user-facing upgrade recovery, terminal focus stability, Git review polish, Source tab startup validation, update UI persistence, and local Apple Silicon startup correctness. -->
<!-- CDXC:Distribution 2026-06-08-14:24: Release notes for 4.0.2 must cover packaged Beads/code-server runtime fixes, terminal image paste controls, gxserver-backed attention side effects, command completion sound reliability, T3 runtime startup stability, Git workflow toast cleanup, and Android scrollback behavior. -->
<!-- CDXC:Distribution 2026-06-08-20:20: Release notes for 4.0.3 must cover remote attach terminals, remote setup feedback, Quick agent launching, titlebar mode guards, Sparkle update UI behavior, bundled editor defaults, sidebar resize polish, and smaller macOS app bundles while keeping internal release validation details out of public notes. -->
<!-- CDXC:Distribution 2026-06-09-23:55: Release notes for 4.1.0 must cover the reused public version, new Android APK link, remote SSH password auth, session identity protections, prompt-editor focus repair, sidebar live-update stability, TUI refresh, and packaged editor runtime fixes without exposing implementation-only release details. -->
<!-- CDXC:Distribution 2026-06-10-21:05: Release notes for 4.1.5 must cover bundled-runtime hook reliability, Claude wake recovery for migrated sessions, sidebar sleep semantics, project-board Beads prefix alignment, Chromium zoom controls, Show less/Close menu settings, remote edit entry points, stale zmx wake activity suppression, and arm64-only macOS distribution without implementation-only bullets. -->

## 4.1.5 - 2026-06-10

- Installed agent hooks, gxserver resume lookup, native sidebar state helpers, and Electron command-pane updates now run through Ghostex-owned bundled runtimes instead of `/usr/bin/python3` or user-installed Node interpreters, so hook sidecars and command status updates keep working on machines without Python.
- Claude sessions migrated from Ghostex 3.6 can wake more reliably because gxserver repair backfills transcript paths and saved resume commands, and wake resolves Claude's real session id before running `claude --resume` instead of trusting a sidebar title.
- Context-menu Sleep no longer parks a row as sleeping while the zmx provider is still alive; Wake and intentional close flows still show immediate sleeping feedback until the host snapshot confirms the same state.
- Project board ticket creation reconciles each project's Beads issue prefix before mutations, and local board actions send both project id and project path so gxserver can reject stale URL/id mismatches.
- The sidebar adds a configurable Show less row count, a Close menu visibility setting, and remote-session edit entry points for quicker day-to-day session management.
- Chromium-embedded panes support standard zoom in, zoom out, and reset shortcuts from the toolbar.
- Waking zmx sessions no longer replays stale working/attention activity from the pre-sleep snapshot.
- Project and Kanban flows require Ghostex's bundled Beads CLI and ignore unrelated `bd` binaries already on PATH.
- Future macOS Sparkle, GitHub, and Homebrew releases ship Apple Silicon builds only.

## 4.1.0 - 2026-06-09

- Remote machines can now save SSH passwords in macOS Keychain, use them for SSH/SCP/tunnel connections without storing raw passwords in settings, and show clearer saved-password state and authentication guidance.
- Agent hook and session-state updates now reject cross-wired agent identities, reducing cases where one agent terminal could inherit another row's title, status, completion state, or resume identity.
- Ctrl+G prompt editing and Monaco prompt-editor dismissal return focus to the correct terminal more reliably, including sessions launched through gxserver global references.
- Sidebar presentation updates apply smaller live patches for session groups and HUD chrome, reducing sidebar refresh churn and terminal focus steals while sessions are added, removed, reordered, or updated.
- Session context menus are safer and cleaner: Copy Resume and Copy Attach Command are hidden by default behind an explicit setting, and Sleep Below / Close Below now target the rendered rows beneath the clicked card across project groups.
- Remote sections, Recent Projects search, active sidebar search, titlebar resource copy, command icons, tag menus, drag handles, and sidebar panel spacing received visual polish for denser daily use.
- The Ghostex TUI now uses a neutral gray-blue default theme, clearer Help/Hotkeys and Quit Ghostex labels, and broader built-in agent labels so restored desktop sessions are easier to recognize from the terminal switcher.
- Embedded code-server packaging is more reliable across Apple Silicon and Intel builds, including target-architecture ripgrep materialization and authenticated GitHub artifact fetches during release builds.
- The Android download badge now points at the 4.1.0 release APK.

## 4.0.3 - 2026-06-08

- Remote session and group clicks now open a local Ghostty terminal that SSH-attaches to the selected remote session with the stable `ghostex attach` contract, while Copy Attach Command still copies the SSH command for external terminals.
- Remote attach carrier terminals stay hidden from the local Quick section, so focus and active styling remain on the owning remote machine row.
- Remote machine setup and gxserver request failures now show more actionable stage-specific messages for SSH, install, token, tunnel, streaming, and transport problems instead of raw loopback or WebKit errors.
- Remote settings are easier to scan, with compact saved-machine cards, inline Tailscale setup help, and clearer optional SSH identity-file guidance.
- The Quick section header can launch the selected agent directly, with the same agent picker used by project headers and new Quick agent chats kept projectless.
- The titlebar now disables GitHub mode when the active project has no GitHub remote and disables GitHub/Kanban mode for Quick sessions.
- Embedded code-server editor panes now use Ghostex-owned bundled settings by default, start with the Dark 2026 theme on new profiles, and keep local VS Code settings as an explicit opt-in.
- Sparkle update checks repeat quietly while Ghostex is running, the titlebar update button can appear on first render, and update download/extraction progress windows stay hidden while the release notes and relaunch prompts remain available.
- The native sidebar/workarea divider keeps its resize cursor and visible separator aligned during hover and live resizing.
- Installed macOS app bundles are smaller because release packaging prunes duplicate Beads payloads, wrong-architecture node-pty prebuilds, and T3 Code source maps before notarization.

## 4.0.2 - 2026-06-08

- Installed macOS builds now package the full embedded code-server runtime, reuse its Node 22 binary for gxserver, include the bundled Beads CLI, and validate that packaged runtime during release builds.
- Source tab packaging is more reliable because the embedded VS Code runtime carries its ripgrep helper files and cleans up temporary build metadata after packaging.
- Terminal image paste can convert clipboard images into previewable Markdown links with Cmd+V or Ctrl+V, and Settings -> Terminal Behavior now includes a Paste previewable images toggle for users who want normal clipboard behavior.
- gxserver presentation updates now carry stable attention event IDs, so macOS can play completion sounds and notifications once for fresh attention events without replaying them during startup or stream recovery.
- Command-pane completions keep using the action completion sound path and write status updates through per-process temp files, reducing missed completion sounds during concurrent status updates.
- T3 Code runtime startup is coalesced while Project Editor panes are retargeting, reducing duplicate startup probes and layout churn when opening, closing, or switching embedded T3 panes.
- Git agent workflows no longer pin duplicate persistent "running" toasts when the visible agent terminal already shows the workflow progress.
- Ghostex Android auto-scroll now follows new output only when the terminal is already near the bottom, so users can scroll through history without selecting text first.

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
- Native T3 Code panes package the managed runtime in installed macOS builds, keep it alive from live pane ownership, resolve the packaged runtime path, and show bundled, development, or missing status in Settings -> Integrations.
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
- T3 Code panes in installed macOS builds now resolve the packaged runtime path, retain an in-flight startup instead of spawning duplicates, and keep local T3/browser pane rows visible inside gxserver-backed project groups.
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
- T3 Code panes now keep the managed runtime alive from live native pane ownership, including inactive tab siblings, so sidebar projection gaps no longer stop an open embedded T3 tab.
- Installed macOS builds now package the managed T3 Code server runtime and show its bundled/development/missing status in Settings -> Integrations.
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
- Updated the default agent picker order to T3 Code, Codex, Claude, Pi Agent, OpenCode, Gemini, Copilot, Factory Droid, and Grok Build.
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
- Kept embedded VS Code open when creating new terminal, browser, T3, or command sessions from the sidebar.
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
- Improved T3/code-server runtime stability, including runtime liveness repair and correct macOS elapsed-time parsing for startup grace decisions.
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
- Improved workspace stability and performance by suppressing byte-identical native storage writes, skipping metadata-only AppKit relayouts, reducing high-frequency native diagnostics, and filtering noisy T3/focus logs.
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
- Improved native editor pane handling, pane resize routing, T3 diagnostics, and accessibility-permission driven controls.
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
- Improved native T3 Code runtime handling so fresh supervised runtimes are retained during startup/auth races, with explicit stop still available for recovery.
- Improved T3 thread changes by creating and syncing sidebar cards when the native host receives thread-change events.
- Fixed sidebar resize drags to use stable window coordinates so the sidebar does not jump while dragging.
- Added color-environment diagnostics for agent launches so monochrome CLI sessions can be traced to inherited terminal environment values.
- Added long-paste rename handling that summarizes pasted session text before syncing the rename into the agent CLI.

## 1.4.3 - 2026-05-03

- Added an opt-in Browser Panes mode that opens browser actions as first-class workspace panes instead of Chrome Canary windows.
- Added native browser pane controls for address navigation, reload, DevTools, React Grab, profile selection, and browser-data import messaging.
- Persisted browser pane URLs, favicons, and browser-auto titles so sidebar cards and app restarts reflect the current page.
- Added native pane header drag-to-reorder support across terminal, T3, and browser panes without surfacing hidden sessions.

## 1.4.2 - 2026-05-02

- Fixed Sparkle update detection by publishing releases with a monotonic `CFBundleVersion` build number.
- Kept the native AppKit pane resizing changes from 1.4.1 available in the update feed.

## 1.4.1 - 2026-05-02

- Moved split pane resizing into the native AppKit terminal workspace so Ghostty and WKWebView panes resize from the same layout owner.
- Removed the React workspace resize overlay and tests that no longer apply to native pane sizing.
- Removed whole-cell terminal body stepping so pane chrome and terminal renderer widths stay aligned during native resize.

## 1.4.0 - 2026-05-02

- Added Sparkle appcast update support with signed appcast metadata for native macOS updates.
- Added native T3 Code panes with managed runtime bootstrap, authentication, thread routing, and runtime diagnostics.
- Added T3 remote/browser access links for native panes, including local-network and Tailscale-friendly pairing URLs.
- Added draggable workspace pane resizing with double-click equalize behavior for pane rows and columns.
- Added a standard native macOS app menu with About, Check for Updates, Settings, Services, Hide, and Quit.
- Added a setting to hide the native IDE title-bar attach button without disabling IDE attachment.
- Improved IDE attachment behavior so the floating Show IDE button raises or launches the configured IDE for the current workspace.
- Improved T3 runtime startup by rebuilding the local t3code-embed checkout only when source fingerprints or build output require it.
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

- Added full-window native modals for Find Previous Session and T3 Thread ID entry.
- Improved previous-session search launching by routing modal input through the sidebar/native command bridge.
- Improved T3 session thread binding controls in the native sidebar workflow.
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
- Removed T3 Code from default sidebar agents while preserving existing T3 session recognition.
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
