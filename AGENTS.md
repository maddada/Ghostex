# Rules for Agents working in this Repository

### General notes

- Multiple sub-agents are working in this repository. Don't be alarmed if something gets changed around your code. This is normal. Just get your work done without affecting the work of other sub-agents or breaking their work.
- Don't get stuck on stale git locks. You can delete those and continue on your work without confirmation.

### Active apps vs deprecated apps

Only three Ghostex apps are active development targets:

1. **gpui desktop app** — `gpui/` (Rust/GPUI shell + CEF React surfaces). This is *the* desktop app. `bun run start`, `bun run build`, and every `release:*` script in `package.json` target it.
2. **Web app** — `ghostex-web/` (static browser build of the shared sidebar/Agents workspace, talks to gxserver).
3. **Mobile app** — `mobile/` (React Native/Expo, ships Android).

Deprecated, kept in-tree but **not** development targets. Never route new features, refactors, parity work, or bug fixes to these:

- **macOS Swift/AppKit app** — `native/` and `src/`. Superseded by the gpui app. Do not add features here, do not "keep it in sync", and do not treat its behavior as the spec for new work.
- **Native iOS app** and **Termux-fork Android app** — already removed from this checkout; they live under `/Users/madda/dev/_active/ghostex-deprecated/` and must never be restored as active release inputs.

Important: "deprecated app" does not mean "dead directory". Some files under the deprecated trees are still compiled into active apps and are fair game when the task is about an active app:

- `native/sidebar/modal-host.tsx` and `native/sidebar/titlebar-host.tsx` are built by `gpui/vite.config.ts` into gpui's CEF `modal-host.html` / `titlebar-host.html` surfaces. Note that gpui only loads `titlebar-host.html` for the Tips/info dropdown panel: the gpui titlebar itself (project name, Agents/Code/Browser/Kanban/Automate/Docs mode tabs, buttons, tooltips) is drawn natively in Rust by `render_titlebar` / `render_mode_tab` in `gpui/src/main.rs`. Titlebar work for the desktop app belongs there, not in the React mode switcher inside `titlebar-host.tsx`.
- `native/sidebar/manage.tsx` and `native/sidebar/tasks-placeholder.tsx` are loaded by `gpui/sidebar/manage-main.tsx` and `gpui/sidebar/kanban-main.tsx`.
- Many `native/sidebar/*.ts` modules (for example `gxserver-client.ts`, `project-board-shared.ts`) are shared logic consumed by active surfaces.
- `shared/` is shared contract/logic code used by gpui, web, mobile, and gxserver. It is active.
- `sidebar/` is the shared React sidebar (`sidebar/sidebar-app.tsx`), mounted by gpui through `gpui/sidebar/main.tsx` and by the web app. It is active.
- `native/sidebar/native-sidebar.tsx` is specifically the deprecated Swift app's host adapter. That one is not an active surface.

When in doubt about a file under `native/`, check whether an active app imports it before deciding it is dead or before "fixing" it for the Swift app's benefit.

### Repository Search Routing

This repository contains Ghostex app code plus large imported/vendored terminal code. Start searches in the smallest app-owned area that matches the task, and only expand after the first pass doesn't find what you need.

Default search posture:

- For broad text/file searches, exclude imported, vendored, dependency, build, and cache trees unless the task specifically targets them. At minimum exclude `ghostty/**`, `tui2/vendor/**`, `code-server/**`, `node_modules/**`, `.git/**`, `dist/**`, `build/**`, `out/**`, `storybook-static/**`, `tmp/**`, `.cache/**`, `.turbo/**`, `.vite/**`, `.zig-cache/**`, `zig-out/**`, `DerivedData/**`, and `target/**`.
- Treat `ghostty/**` as imported upstream Ghostty code. Do not search it first just because a symbol, setting, file, or bug report mentions "ghostty", "terminal", "session", "restore", "fork", "launch", or "pane"; many Ghostex-owned files use those words.
- If a targeted app-owned search misses, expand one layer at a time and explain why the next folder is relevant before searching large imported trees.

Search these app-owned areas first by task:

- Desktop app shell, window lifecycle, app startup, terminals/panes, titlebar, session restore/fork launch plans, terminal host integration: `gpui/src/`, `gpui/sidebar/`, `gpui/native/macos/`, `gpui/scripts/`, `sidebar/`, `shared/`, and `scripts/`.
- Frontend UI, React components, settings, project/sidebar interactions, Storybook stories: `sidebar/`, `components/`, `components/ui/`, `shared/`, `gpui/sidebar/`, `native/sidebar/` (for the gpui-owned modal/titlebar/manage/kanban hosts listed above).
- Web app: `ghostex-web/`, then the shared `sidebar/` and `shared/` code it builds on.
- Session grid, prompts, agent metadata, workspace/project state, contracts, shared tests: `shared/`, then the consuming surface in `sidebar/`, `gpui/sidebar/`, `native/sidebar/`, `mobile/`, or `gxserver-rs/`.
- Server, remote protocol, hooks, authentication, remote setup: `gxserver-rs/`, `shared/`, `scripts/`.
- TUI or zmx behavior: `tui2/`, `zmx/src/`, and `zmx/test/`; keep `tui2/vendor/**` excluded unless the task is specifically about the vendored VT library.
- Mobile app work: `mobile/` is the only active mobile app and releases Android through the React Native/Expo project. The retired native `iOS/` and Termux-fork `android/` repositories live under `/Users/madda/dev/_active/ghostex-deprecated/` and must not be restored as active release inputs.
- Assets, sounds, icons, and release notes: `media/`, `gpui/assets/`, `src/assets/`, `release/`, and the relevant script under `scripts/`.

Search imported Ghostty code only when the task is explicitly about upstream Ghostty behavior, the embedded Ghostty source, Zig terminal internals, Ghostty macOS internals, or a build/test failure whose failing file is already under `ghostty/**`. Even then, target the relevant subfolder such as `ghostty/src/`, `ghostty/macos/`, `ghostty/pkg/`, or `ghostty/test/`, and continue excluding `ghostty/.zig-cache/**` and `ghostty/zig-out/**`.

Preferred `rg` shape for first-pass searches:

```bash
rg -n "pattern" gpui/src gpui/sidebar sidebar shared scripts gxserver-rs ghostex-web \
  -g '!ghostty/**' -g '!tui2/vendor/**' -g '!code-server/**' \
  -g '!node_modules/**' -g '!storybook-static/**' -g '!tmp/**' \
  -g '!dist/**' -g '!build/**' -g '!out/**' -g '!target/**' -g '!.git/**'
```

Add `native/sidebar` to that list only when the task is about the gpui-owned modal
host, titlebar host, manage, or kanban surfaces, or about shared `native/sidebar/*.ts`
logic. Do not add `native/` or `src/` when the task is new desktop-app work.

### Don't write any tests at all except if explicitly asked to do so by the user

### Never generate fallbacks when the right solution is to actually correct the behavior itself to fix the issue. Fallbacks should be used in rare cases only because they add complexity and hide issues and introduce useless logic.

Example of adding bad fallback code:

Agent: I found the likely root cause: the Ghostty/Restty path is generating local font sources from your configured terminal font family, and VS Code webviews are blocking the local-fonts permission. I'm patching that helper to fall back cleanly instead of passing unusable local-font sources into Restty.

Example of what you should do instead:

We should make it not fall back but instead just do the right thing from the start.
Yes. The clean fix is to stop generating local font sources at all when the current webview environment can't use the local-fonts capability. I'm wiring that check into the Restty font-source helper so Ghostty starts in the correct mode instead of trying-and-failing first.

### Native layout and hit-testing discipline

This applies to the active gpui desktop app (GPUI views, CEF surfaces, AppKit shims, Ghostty terminal hosts). The historical WKWebView wording below refers to the deprecated macOS Swift app; the rule itself is unchanged for gpui.

Ghostex native UI should be built with strict normal layout ownership: lay out interactive AppKit, WKWebView, CEF, Ghostty, sidebar, titlebar, pane, and divider regions as non-overlapping sibling or child frames wherever possible. Do not solve click, drag, hover, or focus bugs by stacking transparent views, extending webviews under native chrome, adding broad parent/window hit-test routing, or creating hidden overlap between interactive regions.

Use real, exact native views for interactive boundaries such as splitters and sidebar dividers. If a divider should be easy to understand, make the visible divider itself the grab target rather than adding invisible overlap over adjacent content. Keep visual-only chrome as non-interactive layers or non-overlapping decoration instead of views that can compete for input.

Before adding any `hitTest` override, NSWindow pre-dispatch mouse routing, synthetic coordinate rerouting, invisible interactive overlay, or intentional overlap between interactive regions, the agent must stop and explain the proposed exception to the user, including why strict normal layout cannot solve it. The agent must get explicit user confirmation before implementing that exception.

Native child windows are the accepted pattern for app modals, dropdowns, command palette, rename, Resources, Tips & Tricks, and similar overlay surfaces. Those windows own their own frames and input, so they should not be replaced with main-window transparent webview overlays or root-level hit-test shields.

### Project board beads workflow

When working from a Ghostex Project board ticket, use the `bd` CLI installed in the environment running that project—macOS, Linux, or the selected WSL distribution—and move the bead through the project swimlanes instead of leaving it in `open`/Todo. Ghostex's Kanban runtime uses this same system binary, so do not depend on a separate `gx bd` wrapper or a bundled Ghostex copy. If `bd` is missing or a board command fails, ask the user to install or update to the latest Beads release in that same environment before continuing.

- Park for later: `bd update <id> --status backlog`
- Claim work: `bd update <id> --status in_progress`
- Ready for test: `bd update <id> --status test`
- Ready for review: `bd update <id> --status review`
- Done: `bd close <id>`

After each turn where you made progress on the bead, add a comment so humans can follow the ticket without reading the full agent transcript:

- `bd comment <id> "<summary>"`
- Focus on user-facing requirements delivered and high-level technical approach.
- Do not list specific files or line numbers.

The Project board "Start work" action copies a prompt that includes these commands and the comment guidance.

### Destructive git/file operations safety rule

Never interpret "revert your changes" or "revert what you did" as permission to reset, restore, clean, delete, or otherwise discard the whole worktree. Other agents and the user may have unrelated uncommitted or untracked work in the same repo.

Before running any destructive command, including but not limited to `git restore .`, `git checkout -- .`, `git reset --hard`, `git clean`, `rm -rf`, or deleting untracked files, you must:

1. Show the user the exact files/directories that would be affected.
2. Explain whether each file is tracked or untracked.
3. Confirm that those files are definitely your own changes, not user work.
4. Ask for explicit approval before executing the destructive command.

If the user asks to revert only the agent's changes, use surgical reversal: inspect diffs, identify the exact hunks/files you changed, and revert only those. When uncertain, stop and ask. Never use broad restore/clean commands as a shortcut.


### Never lose other agents' uncommitted work

Multiple agents and the user work in this same checkout at the same time. Files you touched earlier in your session, or that you read a while ago, may have been changed by someone else since. Treat every uncommitted change you did not make yourself as protected user work.

- Before editing a file you last read a while ago (or that you carry from an earlier plan/worktree/thread), re-read its current on-disk content first and apply your change to that, as a targeted edit. Never write back a whole file from a stale copy in your context: that silently erases every change other agents made to it in between, with no way to recover it from git.
- Never run `git checkout`, `git restore`, `git stash`, or `git reset` on a path that has hunks you did not author.
- When committing, never selectively drop pending hunks in files you commit. Either include a file's whole pending diff, or split it hunk-by-hunk only if you verify afterwards (`git status` + `git diff`) that every hunk you excluded still exists in the working tree. A batch "split the working tree into topical commits" pass must end with zero silently-vanished hunks.
- If you find changes in a file you are about to modify that you cannot attribute to your own task, keep them intact and mention them to the user instead of "cleaning them up".

Example of what this rule prevents (happened on 2026-07-09): one agent added the gpui sidebar persistence fix (`cef_app_ui_profile_cache_path` in `gpui/src/cef/shell.rs`) as uncommitted working-tree state. Later that day, a concurrent agent's titlebar/attention work was committed in an automated batch that wrote `shell.rs` from a version without that fix. The fix had never been committed anywhere, so it vanished without a trace, the user's bug came back, and the fix had to be re-diagnosed and re-applied from scratch.

Corollary: after you verify a surgical bug fix, tell the user it should be committed promptly (or commit it when they ask) so concurrent agents cannot wipe it.

### Rules for running commands

- Never run "bun run start" or any command that would restart the app unless I ask you to.

### Diagnostic logging workflow

- Routine disk logs must have an explicit **Diagnostic disk logging scenario** and may write only while both **Show debug UI controls** and that unexpired scenario are enabled. Do not add unscoped routine disk logging. Errors, crashes, and important warnings remain unconditional.
- Before testing or requesting a reproduction that needs diagnostic logs, record the current logging settings, enable only the smallest set of scenarios needed, and prefer the shortest useful expiry.
- Reproduce the issue yourself when authorized and practical. Otherwise, ask the user to reproduce it after confirming the required scenarios are enabled.
- As soon as the needed evidence is collected—or the logging attempt is abandoned—restore the previous settings and turn off every scenario and debug switch that you enabled. Never leave extra diagnostics running because they can consume disk, CPU, and make the user's computer lag while they continue working.
- Do not turn off scenarios or debug settings that were already enabled by the user; restore exactly the state observed before the diagnostic session.

###  Don't switch the repo to another branch ever
  - We run multiple agents at a time on 1 worktree so agents should never switch the branch this folder is on away from main
  - If you need to do work that requires switching to a new branch then please create a temp worktree and do the needed work there.
