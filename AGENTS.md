# Rules for Agents working in this Repository

### General notes

- Don't get stuck on stale git locks. You can delete those and continue on your work without confirmation.

### Repository Search Routing

This repository contains Ghostex app code plus large imported/vendored terminal code. Start searches in the smallest app-owned area that matches the task, and only expand after the first pass doesn't find what you need.

Default search posture:

- For broad text/file searches, exclude imported, vendored, dependency, build, and cache trees unless the task specifically targets them. At minimum exclude `ghostty/**`, `tui/vendor/**`, `iOS/Vendor/**`, `node_modules/**`, `.git/**`, `dist/**`, `build/**`, `out/**`, `storybook-static/**`, `tmp/**`, `.cache/**`, `.turbo/**`, `.vite/**`, `.zig-cache/**`, `zig-out/**`, `DerivedData/**`, and `target/**`.
- Treat `ghostty/**` as imported upstream Ghostty code. Do not search it first just because a symbol, setting, file, or bug report mentions "ghostty", "terminal", "session", "restore", "fork", "launch", or "pane"; many Ghostex-owned files use those words.
- If a targeted app-owned search misses, expand one layer at a time and explain why the next folder is relevant before searching large imported trees.

Search these app-owned areas first by task:

- macOS app, native host, window lifecycle, app startup, session restore/fork launch plans, native sidebar behavior, terminal host integration: `native/`, `native/macos/`, `native/sidebar/`, `src/`, `sidebar/`, `shared/`, `scripts/`, and `release/`.
- Frontend UI, React components, settings, project/sidebar interactions, Storybook stories: `src/`, `sidebar/`, `components/`, `components/ui/`, `shared/`, `config/`, and `docs/`.
- Session grid, prompts, agent metadata, workspace/project state, contracts, shared tests: `shared/`, then the consuming surface in `src/`, `sidebar/`, `native/`, or `gxserver/`.
- Server, remote protocol, hooks, authentication, remote setup: `gxserver/`, `shared/`, `scripts/`, and `docs/`.
- TUI or zmx behavior: `tui/src/`, `tui/tests/`, `tui/scripts/`, `zmx/src/`, and `zmx/test/`; keep `tui/vendor/**` excluded unless the task is specifically about the vendored VT library.
- Mobile app work: `iOS/VVTerm*`, `iOS/web/`, `iOS/scripts/`, `android/app/`, `android/terminal-*`, `android/termux-shared/`, and mobile docs; keep `iOS/Vendor/**` excluded unless the task is specifically about vendored mobile dependencies.
- Cross-platform Electron or shared packaging work: `crossplatform/`, `shared/`, `scripts/`, `release/`, and `docs/`.
- Assets, sounds, icons, docs, and release notes: `media/`, `src/assets/`, `docs/`, `release/`, and the relevant script under `scripts/`.

Search imported Ghostty code only when the task is explicitly about upstream Ghostty behavior, the embedded Ghostty source, Zig terminal internals, Ghostty macOS internals, or a build/test failure whose failing file is already under `ghostty/**`. Even then, target the relevant subfolder such as `ghostty/src/`, `ghostty/macos/`, `ghostty/pkg/`, or `ghostty/test/`, and continue excluding `ghostty/.zig-cache/**` and `ghostty/zig-out/**`.

Preferred `rg` shape for first-pass searches:

```bash
rg -n "pattern" native src sidebar shared scripts gxserver \
  -g '!ghostty/**' -g '!tui/vendor/**' -g '!iOS/Vendor/**' \
  -g '!node_modules/**' -g '!storybook-static/**' -g '!tmp/**' \
  -g '!dist/**' -g '!build/**' -g '!out/**' -g '!.git/**'
```

### Don't write any test in the macOS app

This project is being replaced by the ./gpui project in the future, so these tests aren't helpful. No testing code here for now. If a test failes due to a change just delete it.

### Don't write any test for the code in the gpui app

Things are in a lot of flux in the ./gpui project
We will write tests later to lock down working parts of it.
No testing code here for now.

### Never generate fallbacks when the right solution is to actually correct the behavior itself to fix the issue. Fallbacks should be used in rare cases only because they add complexity and hide issues and introduce useless logic.

Example of adding bad fallback code:

Agent: I found the likely root cause: the Ghostty/Restty path is generating local font sources from your configured terminal font family, and VS Code webviews are blocking the local-fonts permission. I'm patching that helper to fall back cleanly instead of passing unusable local-font sources into Restty.

Example of what you should do instead:

We should make it not fall back but instead just do the right thing from the start.
Yes. The clean fix is to stop generating local font sources at all when the current webview environment can't use the local-fonts capability. I'm wiring that check into the Restty font-source helper so Ghostty starts in the correct mode instead of trying-and-failing first.

### Native layout and hit-testing discipline

Ghostex native UI should be built with strict normal layout ownership: lay out interactive AppKit, WKWebView, CEF, Ghostty, sidebar, titlebar, pane, and divider regions as non-overlapping sibling or child frames wherever possible. Do not solve click, drag, hover, or focus bugs by stacking transparent views, extending webviews under native chrome, adding broad parent/window hit-test routing, or creating hidden overlap between interactive regions.

Use real, exact native views for interactive boundaries such as splitters and sidebar dividers. If a divider should be easy to understand, make the visible divider itself the grab target rather than adding invisible overlap over adjacent content. Keep visual-only chrome as non-interactive layers or non-overlapping decoration instead of views that can compete for input.

Before adding any `hitTest` override, NSWindow pre-dispatch mouse routing, synthetic coordinate rerouting, invisible interactive overlay, or intentional overlap between interactive regions, the agent must stop and explain the proposed exception to the user, including why strict normal layout cannot solve it. The agent must get explicit user confirmation before implementing that exception.

Native child windows are the accepted pattern for app modals, dropdowns, command palette, rename, Resources, Tips & Tricks, and similar overlay surfaces. Those windows own their own frames and input, so they should not be replaced with main-window transparent webview overlays or root-level hit-test shields.

### Project board beads workflow

When working from a Ghostex Project board ticket, move the bead through the project swimlanes with `bd` instead of leaving it in `open`/Todo:

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
