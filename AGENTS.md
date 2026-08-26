# Rules for Agents working in this Repository

### General notes

- Multiple sub-agents are working in this repository. Don't be alarmed if something gets changed around your code. This is normal. Just get your work done without affecting the work of other sub-agents or breaking their work.
- Don't get stuck on stale git locks. You can delete those and continue on your work without confirmation.

### Repository layout (restructured 2026-08-22)

The repository root was restructured on 2026-08-22. Old top-level folders
(`gpui/`, `native/`, `ghostex-web/`, `gxserver-rs/`, `sidebar/`, `shared/`,
`components/`, `lib/`, `src/`, `zehn-rs/`, `ghostex-paths/`, `ghostex-history/`,
`mobile-chat/`, `mobile-find/`, `ghostty/`, `tui2/`, `zmx/`, `code-server/`,
`zehn/`) no longer exist at the root. If you are working from an old plan,
transcript, or memory file, re-derive the path before you search or edit.

One-line vocabulary:

- **`apps/`** = deliverables (things that ship and have an entry point).
- **`views/`** = embedded pages an app ships (never call these "webviews" or "surfaces").
- **`packages/`** = libraries imported by apps and by the server.
- **`.dependencies/`** = ALL external-origin code, _including code we edit_.

Current root:

```
Ghostex/
├── .dependencies/     # ALL external-origin code (edited or not)
│   ├── ghostty/  ghostty-patches/  code-server/  zmx/
│   └── zed/  cef-rs/  gpui-component/
├── apps/
│   ├── desktop/       # Rust/GPUI desktop app (crate ghostex-gpui)
│   │   ├── src/       # Rust
│   │   ├── sidebar/   # CEF entry modules (main, chat, find, kanban, manage)
│   │   └── views/     # embedded pages: modal-host, titlebar-host, manage, kanban, meo
│   ├── web/           # static browser build of the shared workspace UI
│   ├── mobile/
│   │   ├── app/       # React Native / Expo submodule
│   │   └── views/     # chat/ + find/ view bundles embedded by the RN app
│   ├── editor/        # GhostexEditor daemon (Monaco prompt editor)
│   └── history-cli/   # `ghostex-history` CLI crate
├── server/            # gxserver crate (binaries: gxserver, ghostex)
├── packages/
│   ├── shared/        # cross-app contracts + logic
│   ├── core-ui/       # the shared React app UI (sidebar, chat, find, settings, assets)
│   ├── components/    # shadcn primitives (ui/) + utils.ts
│   ├── find/          # Rust prompt-history search (crate ghostex-find)
│   └── paths/         # Rust path resolution (crate ghostex-paths)
├── tooling/  media/  skills/  docs/
└── package.json  tsconfig.json  AGENTS.md  CHANGELOG.md  appcast*.xml  bun.lock …
```

Imports: the `@/` alias maps to the **repo root only**, and every import uses the
real path — `@/packages/shared/…`, `@/packages/core-ui/…`,
`@/packages/components/…`, `@/packages/components/utils`. There are no
per-package alias remaps, so every import is grep-able as a literal path.

The full move map, per-file referencer inventory, and split log live in
`docs/2026-08-22/repo-restructure/` (`PLAN.md`, `PROGRESS.md`, `REFERENCERS.md`,
`SPLITS.md`). Read those before assuming a file is missing.

#### Migration: submodules stranded at their old top-level path

The restructure also moved the `code-server` and `zmx` **submodule**
gitlinks into `.dependencies/`. Git cannot move a submodule's working tree as
part of a gitlink rename, so a checkout that had any of them initialized before
2026-08-22 keeps the real tree at the old top-level path (now untracked) and
gets an empty directory at the new one. A fresh clone is unaffected.
`prepare-macos-runtime.sh` now hard-fails on this signature instead of
packaging an app with a dead Code tab.

Fast unblock, no move needed (`ZMX_ROOT=` for zmx):

```sh
GHOSTEX_CODE_SERVER_ROOT=$PWD/code-server bun run start
```

Proper repair — move the tree and fix its git pointers. code-server needs
**four** fixes because of the nested `lib/vscode` submodule; repairing them is
not cosmetic, since a broken gitdir degrades `rev-parse HEAD` to `development`
in the build fingerprint and forces a full VS Code rebuild:

```sh
rmdir .dependencies/code-server
mv code-server .dependencies/code-server
echo 'gitdir: ../../.git/modules/code-server' > .dependencies/code-server/.git
git config -f .git/modules/code-server/config \
  core.worktree ../../../.dependencies/code-server
echo 'gitdir: ../../../../.git/modules/code-server/modules/lib/vscode' \
  > .dependencies/code-server/lib/vscode/.git
git config -f .git/modules/code-server/modules/lib/vscode/config \
  core.worktree ../../../../../../.dependencies/code-server/lib/vscode
```

`zmx` has no nested submodule, so it needs only the first two:

```sh
rmdir .dependencies/zmx && mv zmx .dependencies/zmx
echo 'gitdir: ../../.git/modules/zmx' > .dependencies/zmx/.git
git config -f .git/modules/zmx/config core.worktree ../../../.dependencies/zmx
```

Verify: `git -C .dependencies/code-server rev-parse HEAD` prints
`390f119a145e…`, and `git submodule status .dependencies/code-server` shows a
leading space (not `-` or `+`).

### Active apps vs deprecated apps

Only three Ghostex apps are active development targets:

1. **Desktop app** — `apps/desktop/` (Rust/GPUI shell + CEF React views). This is _the_ desktop app. `bun run start`, `bun run build`, and every `release:*` script in `package.json` target it.
2. **Web app** — `apps/web/` (static browser build of the shared workspace/Agents UI, talks to gxserver).
3. **Mobile app** — `apps/mobile/` (React Native/Expo submodule in `apps/mobile/app`, ships Android).

Deprecated. Never route new features, refactors, parity work, or bug fixes to these:

- **macOS Swift/AppKit app** — removed on 2026-08-20. The Swift sources and their WKWebView sidebar host are gone; do not restore them, re-add a macOS Swift target, or treat the old app's behavior as the spec for new work.
- **Native iOS app** and **Termux-fork Android app** — already removed from this checkout; they live under `/Users/madda/dev/_active/ghostex-deprecated/` and must never be restored as active release inputs.

Everything under `apps/`, `packages/`, and `server/` is active. `.dependencies/`
is external-origin code: some of it we edit (ghostty, zmx, code-server),
and some of it is a pure build input (zed, cef-rs, gpui-component).

- **`ghostex-tui` terminal app (`.dependencies/tui2`)** — deleted on 2026-08-23,
  together with `gx tui`, its build/staging plumbing, and `bin/ghostex-tui` in
  the macOS and remote Linux packages. Do not restore the vendored tree or
  re-add a `tui` CLI verb; the replacement is a herdr plugin, specified in
  `docs/2026-08-23/tui2-herdr-plugin/TUI2-AS-HERDR-PLUGIN.md`.

### Extensions system

Ghostex extensions are separately shipped, hash-verified packages that can add
full views, chat-bar panels, terminal panes, titlebar popups, and app modals.
The gxserver registry, store, catalog, static serving, command lifecycle, and
CLI live in `server/src/extensions/`. Desktop hosting, bridge context, launch
routing, and runtime snapshots live in `apps/desktop/src/app/extensions/`.
The Store and Installed UI lives in `packages/core-ui/extensions-modal/`, and
the shared wire contract is `packages/shared/ghostex-extensions.ts`.

Extension source, manifests, schemas, publishing tools, and example extensions
live in the separate sibling checkout at
`/Users/madda/dev/_active/Ghostex-extensions`; do not add them to this repo or
`.dependencies/`. Installed payloads are runtime data owned by gxserver, not
source trees to edit in either checkout.

### `apps/desktop/views/` — the desktop app's embedded pages

`apps/desktop/views/` holds the React pages the desktop app ships inside CEF.
`apps/desktop/vite.config.ts` builds them, together with the CEF entry modules in
`apps/desktop/sidebar/`, into the app's HTML bundles:

- `apps/desktop/views/modal-host.tsx` → `modal-host.html` (app modals, dropdowns, toasts).
- `apps/desktop/views/titlebar-host.tsx` → `titlebar-host.html`, with its implementation split across `apps/desktop/views/titlebar/`. The desktop app only loads this page for the Tips and Resources dropdown panels.
  **The gpui titlebar itself is native Rust, not this page.** The project name, the Agents/Code/Browser/Kanban/Automate/Docs mode tabs, the buttons, and the tooltips are drawn by `render_titlebar` / `render_mode_tab` in `apps/desktop/src/app/render/mode_switcher_and_titlebar.rs`; the titlebar menus, popups, tips and resources behaviour live in `apps/desktop/src/app/titlebar/`; the mode-tab list is built by `titlebar_mode_switcher_items` in `apps/desktop/src/app/helpers/titlebar.rs` (with thin wrappers in `apps/desktop/src/app/workarea.rs` and `apps/desktop/src/app/model/runtime_state.rs`). Titlebar work for the desktop app belongs in those Rust files, not in `titlebar-host.tsx`.
- `apps/desktop/views/manage.tsx` (+ `apps/desktop/views/manage/`) is the Docs surface, loaded through `apps/desktop/sidebar/manage-main.tsx`.
- `apps/desktop/views/tasks-placeholder.tsx` (+ `apps/desktop/views/project-board/`) is the Kanban surface, loaded through `apps/desktop/sidebar/kanban-main.tsx`.
- `apps/desktop/views/meo/` is the markdown editor behind the Docs surface, reached through `manage.tsx` → `meo/editor.ts`.
- `apps/desktop/views/project-board-shared.ts` and `apps/desktop/views/combined-sidebar-mode.ts` are shared logic consumed by those pages.

Shared gxserver logic lives in `packages/shared/` (for example
`packages/shared/gxserver-presentation-cache.ts`); the desktop runtime client is
`apps/desktop/sidebar/gxserver-runtime.ts` (+ `gxserver-runtime/`), and the web
app has its own client at `apps/web/src/connections/gxserver-client.ts`.

The shared React app UI is `packages/core-ui/` (`packages/core-ui/sidebar-app.tsx`),
mounted by the desktop app through `apps/desktop/sidebar/main.tsx` and by the web
app. Its icons are in `packages/core-ui/assets/`.

### Repository Search Routing

This repository contains Ghostex app code plus large external terminal/editor
code. Start searches in the smallest app-owned area that matches the task, and
only expand after the first pass doesn't find what you need.

Default search posture:

- **`.dependencies/**` is THE exclusion for external code.** Everything imported or vendored now lives there, so a single `-g '!.dependencies/**'` replaces the old per-tree ghostty/tui2-vendor/code-server excludes. Also exclude build, dependency, and cache trees: `node_modules/**`, `.git/**`, `dist/**`, `build/**`, `out/**`, `target/**`, `storybook-static/**`, `tmp/**`, `artifacts/**`, `.cache/**`, `.turbo/**`, `.vite/**`, `.zig-cache/**`, `zig-out/**`, and `DerivedData/**`.
- Do not search `.dependencies/ghostty/` first just because a symbol, setting, file, or bug report mentions "ghostty", "terminal", "session", "restore", "fork", "launch", or "pane"; many Ghostex-owned files use those words.
- If a targeted app-owned search misses, expand one layer at a time and explain why the next folder is relevant before searching large external trees.

Search these app-owned areas first by task:

- Desktop app shell, window lifecycle, app startup, terminals/panes, titlebar, session restore/fork launch plans, terminal host integration: `apps/desktop/src/`, `apps/desktop/sidebar/`, `apps/desktop/native/macos/`, `apps/desktop/scripts/`, `packages/core-ui/`, `packages/shared/`, and `tooling/`.
- Frontend UI, React components, settings, project/sidebar interactions, Storybook stories: `packages/core-ui/`, `packages/components/`, `packages/components/ui/`, `packages/shared/`, `apps/desktop/sidebar/`, `apps/desktop/views/` (for the modal host, titlebar host, Docs/manage, Kanban, and `meo` pages listed above).
- Web app: `apps/web/src/`, then the shared `packages/core-ui/` and `packages/shared/` code it builds on.
- Session grid, prompts, agent metadata, workspace/project state, contracts, shared tests: `packages/shared/`, then the consuming surface in `packages/core-ui/`, `apps/desktop/sidebar/`, `apps/desktop/views/`, `apps/mobile/views/`, or `server/src/`.
- Server, remote protocol, hooks, authentication, remote setup: `server/src/`, `packages/shared/`, `tooling/`. The server crate is heavily modularized: `server/src/server/` (HTTP/WS core in `mod.rs` plus per-concern submodules), `server/src/agents/`, the flat `server/src/session_chat_*.rs` family, `server/src/domain/`, `server/src/zmx/`, `server/src/typed_operations/`, `server/src/portless/`, and `server/src/agent_hooks/`. Crate name is `gxserver`; it builds the `gxserver` and `ghostex` binaries.
- Extensions: start with `server/src/extensions/` for registry, install, serving, lifecycle, API, and CLI behavior; `apps/desktop/src/app/extensions/` for desktop hosting, bridge context, and launch routing; `packages/core-ui/extensions-modal/` for Store and Installed UI; and `packages/shared/ghostex-extensions.ts` for the shared contract. Search `/Users/madda/dev/_active/Ghostex-extensions` only for extension manifests, authoring/publishing tooling, or example-extension code.
- zmx behavior: `.dependencies/zmx/src/` + `.dependencies/zmx/test/`. This is the deliberate exception to the `.dependencies/**` exclusion — Ghostex edits it.
- Prompt-history search (`gx f`, the Find surface): `packages/find/` for the engine, `server/src/agent_prompt_search.rs` for the API, `packages/core-ui/find/` for the shared UI.
- Mobile app work: `apps/mobile/` is the only active mobile app and releases Android through the React Native/Expo project in `apps/mobile/app` (a git submodule). Its embedded chat and find pages are `apps/mobile/views/chat/` and `apps/mobile/views/find/`, bundled by `bun run build:mobile-chat` / `bun run build:mobile-find`. The retired native iOS and Termux-fork Android repositories live under `/Users/madda/dev/_active/ghostex-deprecated/` and must not be restored as active release inputs.
- Assets, sounds, icons, and release tooling: `media/`, `apps/desktop/assets/`, `packages/core-ui/assets/`, `tooling/`, and `tooling/release-gpui/`.

Search external Ghostty code only when the task is explicitly about upstream
Ghostty behavior, the embedded Ghostty source, Zig terminal internals, Ghostty
macOS internals, or a build/test failure whose failing file is already under
`.dependencies/ghostty/**`. Even then, target the relevant subfolder such as
`.dependencies/ghostty/src/`, `.dependencies/ghostty/macos/`,
`.dependencies/ghostty/pkg/`, or `.dependencies/ghostty/test/`, and continue
excluding `.dependencies/ghostty/.zig-cache/**` and
`.dependencies/ghostty/zig-out/**`. Ghostex's own patch series on top of
upstream is `.dependencies/ghostty-patches/`, re-applied by
`tooling/sync-ghostty.sh`.

Preferred `rg` shape for first-pass searches:

```bash
rg -n "pattern" apps/desktop/src apps/desktop/sidebar packages/core-ui packages/shared \
  server/src apps/web/src tooling \
  -g '!.dependencies/**' -g '!node_modules/**' -g '!storybook-static/**' -g '!tmp/**' \
  -g '!dist/**' -g '!build/**' -g '!out/**' -g '!target/**' -g '!artifacts/**' -g '!.git/**'
```

Add `apps/desktop/views` to that list only when the task is about the desktop
modal host, titlebar host, Docs/manage, Kanban, or `meo` pages, or about the
shared `apps/desktop/views/*.ts` logic. Add `packages/components` for shadcn
primitives, and `apps/mobile/views` for the mobile embedded pages.

### Prompt-history search: it is Rust; the old Zig Zehn source is gone

`gx f` used to spawn a bundled Zig binary built from the `zehn` submodule. It
does not any more. Prompt-history search is the `packages/find/` Rust crate
(crate name `ghostex-find`), compiled into gxserver and the `ghostex` CLI, so:

- `gx f` runs the picker **in-process**. There is no `bin/zehn` to stage, no
  `GHOSTEX_ZEHN_BIN`, and no `ZEHN_ZIG`. (Releases do still require Zig 0.16 —
  for ghostty and zmx, not for zehn — and 0.16 is now the repo's _only_ Zig
  toolchain.)
- The old Zig `zehn` submodule was removed after the Rust port replaced it.
  Never restore it, build it, bundle it, or treat it as the spec for new work —
  change `packages/find/` instead.
- Two hotkeys moved in **both** the terminal picker and the GUI so the surfaces
  share one key map: agents is `^g` (was `^t`) and projects is `^j` (was `^r`),
  because browsers reserve Ctrl+T and Ctrl+R and will not hand them to a page.
- The GUI (`packages/core-ui/find/`) and `gx f` share the same scanner, matcher,
  Codex cache, and favorites file, so a prompt starred in one is starred in the
  other. Anything that would make them rank or star differently is a bug.

### Don't write any tests at all except if explicitly asked to do so by the user

### Never generate fallbacks when the right solution is to actually correct the behavior itself to fix the issue. Fallbacks should be used in rare cases only because they add complexity and hide issues and introduce useless logic.

Example of adding bad fallback code:

Agent: I found the likely root cause: the Ghostty/Restty path is generating local font sources from your configured terminal font family, and VS Code webviews are blocking the local-fonts permission. I'm patching that helper to fall back cleanly instead of passing unusable local-font sources into Restty.

Example of what you should do instead:

We should make it not fall back but instead just do the right thing from the start.
Yes. The clean fix is to stop generating local font sources at all when the current webview environment can't use the local-fonts capability. I'm wiring that check into the Restty font-source helper so Ghostty starts in the correct mode instead of trying-and-failing first.

### Native layout and hit-testing discipline

This applies to the active desktop app (GPUI views, CEF pages, AppKit shims, Ghostty terminal hosts). The historical WKWebView wording below refers to the deprecated macOS Swift app; the rule itself is unchanged for the desktop app.

Ghostex native UI should be built with strict normal layout ownership: lay out interactive AppKit, WKWebView, CEF, Ghostty, sidebar, titlebar, pane, and divider regions as non-overlapping sibling or child frames wherever possible. Do not solve click, drag, hover, or focus bugs by stacking transparent views, extending webviews under native chrome, adding broad parent/window hit-test routing, or creating hidden overlap between interactive regions.

Use real, exact native views for interactive boundaries such as splitters and sidebar dividers. If a divider should be easy to understand, make the visible divider itself the grab target rather than adding invisible overlap over adjacent content. Keep visual-only chrome as non-interactive layers or non-overlapping decoration instead of views that can compete for input.

Before adding any `hitTest` override, NSWindow pre-dispatch mouse routing, synthetic coordinate rerouting, invisible interactive overlay, or intentional overlap between interactive regions, the agent must stop and explain the proposed exception to the user, including why strict normal layout cannot solve it. The agent must get explicit user confirmation before implementing that exception.

Native child windows are the accepted pattern for app modals, dropdowns, command palette, rename, Resources, Tips & Tricks, and similar overlay surfaces. Those windows own their own frames and input, so they should not be replaced with main-window transparent webview overlays or root-level hit-test shields.

### Shared UI controls: one component per control kind

Some controls are deliberately owned by a single shared component so every
surface renders the same thing. Use them instead of hand-rolling a lookalike
out of `Button`s or raw `ToggleGroup`s, and change the shared component (plus
its story) when the look must change:

- **Segmented single-select** ("pick exactly one of N", e.g. Sidebar version,
  Preset, Add Worktree mode, Automate schedule/execution):
  `packages/components/ui/segmented-control.tsx` — `SegmentedControl` /
  `SegmentedControlItem`. It renders the stock shadcn ButtonGroup shape: one
  bordered rounded container, flat segments sharing a hairline, only the outer
  corners rounded, and a highlighted fill on the selected segment. Story:
  `Components/Segmented Control`. Its canonical CSS lives unlayered in
  `packages/core-ui/styles.css` and is mirrored in
  `apps/desktop/views/project-board/styles.ts` because the Kanban/Automate page
  loads only `shadcn.generated.css`.
- **Toggle switch**: `packages/components/ui/switch.tsx` — one shape app-wide
  (6px track, 4px thumb). Don't reintroduce per-surface pill overrides.
- **Focus ring**: the chat composer's ring is the reference — 3px at
  `ring-ring/20` plus `border-ring`. Every shared primitive uses that value;
  never raise it back to `ring-ring/50` or `/30`. Surfaces that deliberately
  have no ring (the modal tab rails) stay ringless.

### Project board beads workflow

When working from a Ghostex Project board ticket, use the `bd` CLI installed in the environment running that project—macOS, Linux, or the selected WSL distribution—and move the bead through the project swimlanes instead of leaving it in `open`/Todo. Ghostex's Kanban runtime uses this same system binary, so do not depend on a separate `gx bd` wrapper or a bundled Ghostex copy. If `bd` is missing or a board command fails, ask the user to install or update to the latest Beads release in that same environment before continuing.

- Put your session on the card: `gx board associate <id>` — run this first, with no
  other arguments, whenever you are asked to work a bead. It links the session you are
  running in to the card, which is otherwise only linked when the work was dispatched
  from the card's own "Start work" button, so a hand-prompted agent leaves the card
  looking unworked. It creates no session and is safe to repeat; `gx board start-work`
  is the opposite command (it dispatches a card to a *new* worker) and must not be run
  for a bead you are working yourself.
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

Example of what this rule prevents (happened on 2026-07-09): one agent added the desktop sidebar persistence fix (`cef_app_ui_profile_cache_path`, in the CEF shell code that is now `apps/desktop/src/cef/shell/` — its pre-restructure path was `gpui/src/cef/shell.rs`) as uncommitted working-tree state. Later that day, a concurrent agent's titlebar/attention work was committed in an automated batch that wrote that file from a version without the fix. The fix had never been committed anywhere, so it vanished without a trace, the user's bug came back, and the fix had to be re-diagnosed and re-applied from scratch.

Corollary: after you verify a surgical bug fix, tell the user it should be committed promptly (or commit it when they ask) so concurrent agents cannot wipe it.

### Rules for running commands

- Never run "bun run start" or any command that would restart the app unless I ask you to.
- TypeScript is gated by three configs, not one: `bun run typecheck` (root — `packages/shared`, `packages/core-ui`, `packages/components`, `apps/desktop/views`, `apps/mobile/views`), `bun run web:typecheck` (`apps/web/tsconfig.json`), and `bun run desktop:typecheck` (`apps/desktop/tsconfig.json`, which covers `apps/desktop/sidebar/` and `apps/desktop/views/`). A change under `apps/desktop/sidebar/` is only checked by `desktop:typecheck`.
- Run desktop-crate cargo commands **from inside `apps/desktop/`**, not with `--manifest-path` from the repo root. The crate pins its own toolchain in `apps/desktop/rust-toolchain.toml` (1.95.0), and `--manifest-path` from the root resolves the root toolchain instead and fails on dependency code that needs the pin.

### Before committing and pushing: formatting and file-size upkeep

Before you commit and push, check whether other agents are currently working in
**this same worktree/folder**: run `ghostex sessions` (see `ghostex --help`).
The list is grouped by project path — only look at the group whose path matches
the folder you are working in; sessions in other projects or other worktrees do
not block anything. Your own session counts as one `running` entry in that
group.

- **If any _other_ session in this worktree is `running`**: do NOT run a
  repo-wide formatting pass or start file splits — that would sweep their
  uncommitted work into your commit or create churn under them. Format only the
  files you yourself changed, then commit path-scoped as usual.
- **If no other session in this worktree is `running`** (everything else is
  `sleep` or the list is empty): run the full-repo formatting pass and the
  file-size upkeep pass below, then commit (your changes plus the formatting
  together, or as a separate `chore: Formatting` commit) and push.

The full-repo formatting pass is:

```bash
# Rust — each crate separately; the desktop crate MUST run from inside its folder
(cd apps/desktop && cargo fmt)
(cd server && cargo fmt)
(cd apps/history-cli && cargo fmt)
(cd packages/find && cargo fmt)
(cd packages/paths && cargo fmt)

# TS/JS/JSON/MD/YAML — app-owned trees only, never .dependencies/ or generated output
bunx prettier --write "apps/desktop/{sidebar,views,test,scripts}/**/*.{ts,tsx,mjs,md}" \
  "apps/web/src/**/*.{ts,tsx}" "apps/mobile/views/**/*.{ts,tsx}" \
  "packages/{shared,core-ui,components}/**/*.{ts,tsx,md}" \
  "server/**/*.mjs" "tooling/**/*.{mjs,ts}" "*.{json,md,ts}" ".github/**/*.yml"
```

Never format `.dependencies/**`, `node_modules/**`, `apps/mobile/app/**` (the
submodule), generated files (`*.generated.*`, `dist/`, `build/`, `target/`,
`apps/desktop/runtime/`), or `bun.lock`. After a repo-wide pass, run the
typecheck/test gates before pushing, and review `git status` so you only commit
formatting deltas plus your own work — if the pass touched a file with foreign
uncommitted hunks, leave that file out of your commit.

**File-size upkeep pass (same quiet-worktree window only).** A repo-wide split
wave finished on 2026-08-24: every app-owned source file is under ~2,000 lines
except a handful of deliberate keeps, and the big Rust god-files (`render.rs`,
`terminal_sync.rs`, `presentation.rs`, `os_cli.rs`, `remote_conn.rs`, the
`helpers/*` monoliths, `gxserver-runtime/git.ts`, …) are per-concern module
directories. Do not regress:

- **Add new code to the module that owns the concern, not to whichever file is
  open.** When a split directory exists (e.g. `apps/desktop/src/app/render/`,
  `helpers/os_cli/`, `server/src/presentation/`, `gxserver-runtime/git/`), new
  functions go into the matching per-concern file, or a new sibling file —
  never into `mod.rs`/`index.ts`, which stay as thin re-export barrels. This
  applies always, agents running or not.
- **Don't let files grow back.** If an app-owned source file you touched has
  grown past ~1,500 lines, split it during this quiet window — never while
  other agents are running in the same worktree — using the established
  recipe: a directory with a `mod.rs` of flat `pub(crate) mod x;` +
  `pub(crate) use x::*;` re-exports (Rust) or a barrel `index.ts` (TS), moving
  code **verbatim** so no caller changes. For files that are one big
  `impl GhostexGpuiApp`, split into sibling files each with their own `impl`
  block. See `docs/2026-08-22/repo-restructure/SPLITS.md` for the proven
  pattern. If the window never opens during your session, tell the user the
  file needs a split instead of skipping it silently.
- **Splits must be motion, not rewrites.** A split carries a zero-logic-change
  burden: bodies move byte-identically, item counts match, and any raw-source
  test or comment citation pointing at the old path gets retargeted in the
  same commit.
- Deliberate exception: `apps/desktop/src/terminal_element.rs` (~4.6k lines)
  stays whole for perf-critical locality. Don't split it, and don't cite it as
  precedent for letting other files grow.

### Diagnostic logging workflow

- Routine disk logs must have an explicit **Diagnostic disk logging scenario** and may write only while both **Show debug UI controls** and that unexpired scenario are enabled. Do not add unscoped routine disk logging. Errors, crashes, and important warnings remain unconditional.
- Before testing or requesting a reproduction that needs diagnostic logs, record the current logging settings, enable only the smallest set of scenarios needed, and prefer the shortest useful expiry.
- Reproduce the issue yourself when authorized and practical. Otherwise, ask the user to reproduce it after confirming the required scenarios are enabled.
- As soon as the needed evidence is collected—or the logging attempt is abandoned—restore the previous settings and turn off every scenario and debug switch that you enabled. Never leave extra diagnostics running because they can consume disk, CPU, and make the user's computer lag while they continue working.
- Do not turn off scenarios or debug settings that were already enabled by the user; restore exactly the state observed before the diagnostic session.

### Don't switch the repo to another branch ever

- We run multiple agents at a time on 1 worktree so agents should never switch the branch this folder is on away from main
- If you need to do work that requires switching to a new branch then please create a temp worktree and do the needed work there.
