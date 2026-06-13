# Rules for Agents working in this Repository

## Repository Search Routing

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

## IMPORTANT Logging Rules

Logs may be requested from users for debugging, so persistent logs must be safe for users to zip and send.

1. Do not log PII or user-owned content. Logs must not include project names, session names, terminal titles, user text, command text, raw command arguments, workspace paths, file paths, full URLs, URL query strings, browser/page titles, tokens, cookies, credentials, environment values, stdout/stderr content, or other private data. Prefer stable IDs, counts, booleans, enum-like states, timings, dimensions, exit codes, and redacted summaries.

2. When Debugging Mode is off, logs should only record warnings, errors, and crashes. Routine diagnostic or lifecycle logs should be skipped unless Debugging Mode is enabled.

3. Keep logs for distinct product areas or high-volume diagnostic flows in separate files under the shared support-bundle folder, `~/.ghostex/logs/`, so users can zip one directory while support can inspect the relevant flow without unrelated noise. When a flow uses shared JSONL, use clear event names and stable IDs so entries are filterable without exposing user data.

4. New persistent app logs should live under `~/.ghostex/logs/`, not scattered across separate app-specific folders, so support can ask for one zipped directory.

5. Sanitize at the writer boundary. Logging helpers should sanitize payloads immediately before writing to disk so future call sites cannot bypass privacy rules.

6. Prefer structured logs. Use JSON or JSONL payloads where practical; avoid free-form strings except for fixed event messages because structured fields are easier to sanitize, filter, and test.

7. When adding or changing persistent logging, add or update tests proving raw names, paths, URLs, command text, and secrets do not appear in written log output.

## CDX_LOG comments:
   - Please whenever you're working on a codebase. I want you to add comments describing the date of the change (must be in this format yyyy-MM-dd-hh:mm) and describing the requirements or the change in requirements that made you implement certain functionality.
   - I want you to write CDXC:Area-of-product in front of all your comments so they can be grepped.
   - Most of this should be written as jsdocs but you can add short comments around for the important variables and more complex parts of the codebase.
   - The idea is to encode the requiements of the system (especially software behavior, UX, and important technical decisions) into the code so it's clearer later why a certain piece of code was written.
   - Always make sure to keep these comments updated as you work in the codebase and requirements change.
   - Use technical writing principles to write non-verbose comments that convey the important info without fluff.
   - Keep in mind that ALL of the important user facing requirements sent by the user must be written as comments somewhere in the codebase.
   - There's no need to add line breaks in CDXC comments to stay under a certain character width. Just add line breaks normally at the ened of sentences.

   Good Example for a CDXC Comment:
   ```
   /*
   CDXC:SettingsNavigation 2026-05-13-08:05:
   The Settings dialog needs enough horizontal room for a main-tab section sidebar while Ghostty settings live in their own second tab.
   Use scoped CSS so the native modal host and Storybook share the same width without relying on newly generated utilities.

   CDXC:SettingsNavigation 2026-05-13-08:11:
   The modal should be 20% wider than the first section-sidebar layout and use a taller viewport so more settings remain visible without scrolling.
   */
   ```

## Please never generate fallbacks when the right solution is to actually correct the behavior itself to fix the issue. Fallbacks should be used in rare cases only because they add complexity and hide issues and introduce useless logic.

Example of adding bad fallback code:

Agent: I found the likely root cause: the Ghostty/Restty path is generating local font sources from your configured terminal font family, and VS Code webviews are blocking the local-fonts permission. I'm patching that helper to fall back cleanly instead of passing unusable local-font sources into Restty.

Example of what you should do instead:

We should make it not fall back but instead just do the right thing from the start.
Yes. The clean fix is to stop generating local font sources at all when the current webview environment can't use the local-fonts capability. I'm wiring that check into the Restty font-source helper so Ghostty starts in the correct mode instead of trying-and-failing first.

## Native layout and hit-testing discipline

Ghostex native UI should be built with strict normal layout ownership: lay out interactive AppKit, WKWebView, CEF, Ghostty, sidebar, titlebar, pane, and divider regions as non-overlapping sibling or child frames wherever possible. Do not solve click, drag, hover, or focus bugs by stacking transparent views, extending webviews under native chrome, adding broad parent/window hit-test routing, or creating hidden overlap between interactive regions.

Use real, exact native views for interactive boundaries such as splitters and sidebar dividers. If a divider should be easy to understand, make the visible divider itself the grab target rather than adding invisible overlap over adjacent content. Keep visual-only chrome as non-interactive layers or non-overlapping decoration instead of views that can compete for input.

Before adding any `hitTest` override, NSWindow pre-dispatch mouse routing, synthetic coordinate rerouting, invisible interactive overlay, or intentional overlap between interactive regions, the agent must stop and explain the proposed exception to the user, including why strict normal layout cannot solve it. The agent must get explicit user confirmation before implementing that exception.

Native child windows are the accepted pattern for app modals, dropdowns, command palette, rename, Resources, Tips & Tricks, and similar overlay surfaces. Those windows own their own frames and input, so they should not be replaced with main-window transparent webview overlays or root-level hit-test shields.

## Running and refreshing the app

- The Ghostex app does not have hot reload. After frontend or native-sidebar changes, run `bun run start` again to refresh the running app before verifying UI behavior with Browser, Chrome, Cua Driver, or manual testing.

## Project board beads workflow

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

## Destructive git/file operations safety rule

Never interpret "revert your changes" or "revert what you did" as permission to reset, restore, clean, delete, or otherwise discard the whole worktree. Other agents and the user may have unrelated uncommitted or untracked work in the same repo.

Before running any destructive command, including but not limited to `git restore .`, `git checkout -- .`, `git reset --hard`, `git clean`, `rm -rf`, or deleting untracked files, you must:

1. Show the user the exact files/directories that would be affected.
2. Explain whether each file is tracked or untracked.
3. Confirm that those files are definitely your own changes, not user work.
4. Ask for explicit approval before executing the destructive command.

If the user asks to revert only the agent's changes, use surgical reversal: inspect diffs, identify the exact hunks/files you changed, and revert only those. When uncertain, stop and ask. Never use broad restore/clean commands as a shortcut.
