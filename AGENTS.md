# Rules for Agents working in this Repository

Longer procedures live in the tracked `ai/` folder; each section below names its reference file and when to read it. `docs/` is gitignored local material and never holds agent rules.

### Where a fix or feature belongs (decide before you edit)

Fix the place where the wrong decision is made, not the place where the symptom shows. Ask first: would the web build, the mobile app, the `ghostex` CLI or a remote client hit the same problem? If yes, the fix does not belong in one app's UI. Stop at the first layer that can own the change:

1. **gxserver** (`server/src/`, Rust only; the old TypeScript gxserver is gone): anything about an agent session: what is typed into its terminal, reading its screen, hooks, activity, transcripts, lifecycle, git and worktrees, settings, CLI verbs. One change covers every client.
2. **Session daemons**: zmx (macOS, Linux, Windows WSL) and wmx (native Windows, PowerShell) change together; Ghostex startup and paths stay in gxserver.
3. **Rust cores**: `packages/gx-core` (sidebar), `packages/gx-chat-core` (chat brain, which mobile will use through UniFFI), `packages/gx-protocol` (wire types).
4. **Renderers** (`native_chat/`, `native_sidebar/`, `native_kanban/`, `native_automate/`): layout and input only; `apps/gpui-web` compiles the same files.
5. **Host glue** (`apps/desktop/src/app/gx_chat/`, `gx_store/`): platform I/O only.

Chat rules go only into `gx-chat-core`, never also into the TypeScript chat brain. Start no new work in React chat, the React Kanban/Automate pages, or the QuickJS app runtime (bug fixes only). When a fix lands higher than a layer that could own it, say why in your report. Full guide with examples: `ai/where-to-fix.md`.

### General notes

- Multiple sub-agents work in this repository at the same time. Files changing around your code is normal; get your work done without affecting or breaking theirs.
- Don't get stuck on stale git locks: delete them and continue without confirmation.
- Don't write any tests unless the user explicitly asks for them.
- Never run `bun run start` or any command that would restart the app unless the user asks you to.
- **Exception: `bun run start:server` (macOS).** Run it without asking whenever a gxserver change (`server/src/`, or a crate it compiles in such as `packages/find` or `packages/paths`) has to be proven against the live app, then test the fix for real (the GPUI web build, `ghostex` CLI verbs, `zmx history`) instead of reporting it untested. It rebuilds only gxserver, signs it and installs it into `/Applications/Ghostex.app` in place, then restarts just the daemon: the app window stays open and reconnects, and zmx sessions and their agents keep running through the ~2s gap. It never replaces zmx (it refuses when zmx changed; that needs `bun run start`). Mention in your report that you ran it. Desktop-app changes still need `bun run start`.
- Never switch this folder to another branch. Several agents share one worktree, so it stays on `main` unless the user explicitly requests otherwise; work that needs another branch goes in a temporary copy-on-write folder copy.

### Always provide clickable artifact links

Whenever you create or update an HTML file, Markdown file, Storybook story, or image, include a direct clickable `[descriptive label](target)` Markdown link in your final response: never a bare URL, plain path, or link inside backticks, and absolute paths for local HTML and Markdown files. For Storybook, link the rendered story on the running Storybook server (not only its source file or navigation instructions) and verify the server is reachable and the story exists first. If it cannot be served, say so clearly and provide the source link.

### Repository layout (restructured 2026-08-22)

The old top-level folders (`gpui/`, `native/`, `ghostex-web/`, `gxserver-rs/`, `sidebar/`, `shared/`, `components/`, `lib/`, `src/`, `zehn-rs/`, `ghostex-paths/`, `ghostex-history/`, `mobile-chat/`, `mobile-find/`, `ghostty/`, `tui2/`, `zmx/`, `code-server/`, `zehn/`) no longer exist. If you are working from an old plan, transcript, or memory file, re-derive the path before you search or edit.

Vocabulary: **`apps/`** = deliverables (things that ship and have an entry point). **`views/`** = embedded pages an app ships (never "webviews" or "surfaces"). **`packages/`** = libraries imported by apps and by the server. **`.dependencies/`** = ALL external-origin code, _including code we edit_.

```
Ghostex/
├── .dependencies/     # ALL external-origin code (edited or not)
│   ├── ghostty/  ghostty-patches/  code-server/  zmx/  wmx/
│   └── zed/  cef-rs/  gpui-component/
├── apps/
│   ├── desktop/       # Rust/GPUI desktop app (crate ghostex-gpui)
│   │   ├── src/       # Rust
│   │   ├── sidebar/   # QuickJS service + CEF entries (find, kanban, manage)
│   │   └── views/     # embedded pages: modal-host, titlebar-host, manage, kanban, meo
│   ├── gpui-web/      # the desktop's GPUI UI compiled to wasm and served by `ghostex web`
│   ├── mobile/
│   │   ├── app/       # React Native / Expo submodule
│   │   └── views/     # chat/ + find/ view bundles embedded by the RN app
│   ├── editor/        # GhostexEditor daemon (Monaco prompt editor)
│   └── history-cli/   # `ghostex-history` CLI crate
├── server/            # gxserver crate (binaries: gxserver, ghostex)
├── packages/
│   ├── shared/        # cross-app contracts + logic
│   ├── core-ui/       # the shared React UI (chat, find, settings, modals, assets)
│   ├── components/    # shadcn primitives (ui/) + utils.ts
│   ├── find/          # Rust prompt-history search (crate ghostex-find)
│   └── paths/         # Rust path resolution (crate ghostex-paths)
├── ai/  tooling/  media/  skills/  docs/
└── package.json  tsconfig.json  AGENTS.md  CHANGELOG.md  appcast*.xml  bun.lock …
```

Imports: the `@/` alias maps to the **repo root only**, and every import uses the real path (`@/packages/shared/…`, `@/packages/core-ui/…`, `@/packages/components/…`, `@/packages/components/utils`). There are no per-package alias remaps, so every import is grep-able as a literal path.

The full move map, per-file referencer inventory, and split log live in `docs/2026-08-22/repo-restructure/` (`PLAN.md`, `PROGRESS.md`, `REFERENCERS.md`, `SPLITS.md`). Read those before assuming a file is missing.

**Submodules stranded at their old path.** A checkout that had the `code-server` or `zmx` submodule initialized before 2026-08-22 keeps the real tree at the old top-level path (now untracked) and an empty directory under `.dependencies/`; `prepare-macos-runtime.sh` hard-fails on this. Fast unblock: `GHOSTEX_CODE_SERVER_ROOT=$PWD/code-server bun run start` (`ZMX_ROOT=` for zmx). The proper repair (move the tree and fix its gitdir pointers, four steps for code-server because of the nested `lib/vscode` submodule) is in `ai/submodule-repair.md`.

### Extensions system

Ghostex extensions are separately shipped, hash-verified packages that can add full views, chat-bar panels, terminal panes, titlebar popups, and app modals. Registry, store, catalog, static serving, command lifecycle, and CLI: `server/src/extensions/`. Desktop hosting, bridge context, launch routing, runtime snapshots: `apps/desktop/src/app/extensions/`. Store and Installed UI: `packages/core-ui/extensions-modal/`. Shared wire contract: `packages/shared/ghostex-extensions.ts`.

Extension source, manifests, schemas, publishing tools, and example extensions live in the separate sibling checkout `/Users/madda/dev/_active/Ghostex-extensions`; do not add them to this repo or `.dependencies/`. Installed payloads are runtime data owned by gxserver, not source trees to edit in either checkout.

### `apps/desktop/views/`: the desktop app's embedded pages

`apps/desktop/views/` holds the React pages the desktop app ships inside CEF; `apps/desktop/vite.config.ts` builds them, together with the CEF entry modules in `apps/desktop/sidebar/`, into the app's HTML bundles:

- `modal-host.tsx` → `modal-host.html` (app modals, dropdowns, toasts).
- `titlebar-host.tsx` → `titlebar-host.html` (implementation in `views/titlebar/`), loaded **only** for the Tips and Resources dropdown panels. **The window chrome itself is native Rust, not this page**: there is no titlebar row, and the work area header (project breadcrumb, Start/Open/Commit, the `⋯` menu, the panel toggles) is drawn by `apps/desktop/src/app/render/workarea_header/`; which views are open is the view panel's own tab strip, `apps/desktop/src/app/render/view_tab_strip.rs`, with its menus in `app/view_tab_menus.rs` and its model in `app/view_panel.rs`. Menus, popups, tips and resources behaviour live in `apps/desktop/src/app/titlebar/`; the view list comes from `titlebar_mode_switcher_items` in `apps/desktop/src/app/helpers/titlebar.rs` (thin wrappers in `app/workarea.rs` and `app/model/runtime_state.rs`). This work belongs in those Rust files.
- `manage.tsx` (+ `manage/`) is the Docs surface, loaded through `apps/desktop/sidebar/manage-main.tsx`; `meo/` is the markdown editor behind it (`manage.tsx` → `meo/editor.ts`).
- `tasks-placeholder.tsx` (+ `project-board/`) is the Kanban surface, loaded through `apps/desktop/sidebar/kanban-main.tsx`.
- `project-board-shared.ts` and `combined-sidebar-mode.ts` are shared logic consumed by those pages.

Shared gxserver logic lives in `packages/shared/` (for example `gxserver-presentation-cache.ts`); the desktop runtime client is `apps/desktop/sidebar/gxserver-runtime.ts` (+ `gxserver-runtime/`), and the GPUI web build talks to gxserver from Rust (`apps/gpui-web/src/app/gx_store/`). The React web app was deleted on 2026-09-24; `packages/core-ui/` is the React UI still used by the desktop's CEF pages and the mobile views, and the React sidebar that lived there (`sidebar-app.tsx` and its cards, sections and stories) was deleted with it; find it in git history when a native port cites it. The desktop sidebar is native Rust (`apps/desktop/src/app/native_sidebar/` for the renderer, `apps/desktop/src/app/gx_store/` for the store that feeds it), the TypeScript sidebar page and its `index.html` entry were deleted on 2026-09-21, and what is left of the desktop's QuickJS service is `apps/desktop/sidebar/service/`, `gxserver-runtime/`, `native-quick-access/` and `sidebar-store-feed.ts`; the chat's socket to gxserver is Rust too (`packages/gx-chat-client`, deleted from QuickJS on 2026-09-25).

### Repository Search Routing

Start in the smallest app-owned area that matches the task and expand one layer at a time, saying why the next folder is relevant before searching a large external tree.

- **`.dependencies/**` is THE exclusion for external code**: one `-g '!.dependencies/**'` replaces the old per-tree ghostty/tui2-vendor/code-server excludes. Also exclude `node_modules/**`, `.git/**`, `dist/**`, `build/**`, `out/**`, `target/**`, `storybook-static/**`, `tmp/**`, `artifacts/**`, `.cache/**`, `.turbo/**`, `.vite/**`, `.zig-cache/**`, `zig-out/**`, and `DerivedData/**`.
- Do not search `.dependencies/ghostty/` first just because a symbol, setting, file, or bug report mentions "ghostty", "terminal", "session", "restore", "fork", "launch", or "pane"; many Ghostex-owned files use those words. Search it only when the task is explicitly about upstream Ghostty behaviour, the embedded Ghostty source, Zig terminal internals, Ghostty macOS internals, or a build/test failure already under `.dependencies/ghostty/**`, and then target its `src/`, `macos/`, `pkg/`, or `test/`. Ghostex's own patch series on top of upstream is `.dependencies/ghostty-patches/`, re-applied by `tooling/sync-ghostty.sh`.

Where to look first, by task:

- **Desktop app shell** (window lifecycle, startup, terminals/panes, titlebar, session restore/fork launch plans, terminal host integration): `apps/desktop/src/`, `apps/desktop/sidebar/`, `apps/desktop/native/macos/`, `apps/desktop/scripts/`, `packages/core-ui/`, `packages/shared/`, `tooling/`.
- **Frontend UI** (React components, settings, project/sidebar interactions, Storybook stories): `packages/core-ui/`, `packages/components/` (+ `ui/`), `packages/shared/`, `apps/desktop/sidebar/`, `apps/desktop/views/` (modal host, titlebar host, Docs/manage, Kanban, `meo`).
- **Web app**: `apps/gpui-web/` (the desktop's GPUI source compiled to wasm; see its README), then the desktop files it symlinks.
- **Session grid, prompts, agent metadata, workspace/project state, contracts, shared tests**: `packages/shared/`, then the consuming surface in `packages/core-ui/`, `apps/desktop/sidebar/`, `apps/desktop/views/`, `apps/mobile/views/`, or `server/src/`.
- **Server, remote protocol, hooks, authentication, remote setup**: `server/src/`, `packages/shared/`, `tooling/`. The crate (`gxserver`; binaries `gxserver` and `ghostex`) is heavily modularized: `server/src/server/` (HTTP/WS core plus per-concern submodules), `agents/`, the flat `session_chat_*.rs` family, `domain/`, `zmx/`, `typed_operations/`, `portless/`, `agent_hooks/`.
- **Extensions**: the folders in the Extensions section above. Search `/Users/madda/dev/_active/Ghostex-extensions` only for manifests, authoring/publishing tooling, or example-extension code.
- **zmx behaviour**: `.dependencies/zmx/src/` + `.dependencies/zmx/test/`, the deliberate exception to the `.dependencies/**` exclusion because Ghostex edits it. The canonical contract for the Ghostex private OSCs (`ZMX_REFRESH`, `ZMX_VISIBLE=<rows>,<cols>`, `ZMX_CHAT=<rows>,<cols>`, `ZMX_HIDDEN=<rows>,<cols>`) is `appendClientInputMessages` in `.dependencies/zmx/src/loop.zig`; the three emitters (`apps/desktop/src/terminal_model.rs`, which the GPUI web build compiles too, `server/src/terminal_ws.rs`, `apps/mobile/app/src/terminal/zmxDisplay.ts`) must keep byte-identical sequences and a 200-column constant equal to `RESTING_GRID_COLS` in `.dependencies/zmx/src/ipc.zig`.
- **Prompt-history search** (`ghostex f`, the Find surface): `packages/find/` engine, `server/src/agent_prompt_search.rs` API, `packages/core-ui/find/` shared UI.
- **Mobile**: `apps/mobile/` is the only active mobile app (Android, via the React Native/Expo submodule `apps/mobile/app`); its embedded pages `apps/mobile/views/chat/` and `find/` are bundled by `bun run build:mobile-chat` / `build:mobile-find`. The retired iOS and Termux-fork Android repos under `/Users/madda/dev/_active/ghostex-deprecated/` must not be restored as release inputs.
- **Assets, sounds, icons, release tooling**: `media/`, `apps/desktop/assets/`, `packages/core-ui/assets/`, `tooling/`, `tooling/release-gpui/`.

Preferred first-pass `rg` shape (add `apps/desktop/views`, `packages/components`, or `apps/mobile/views` only when the task is about those):

```bash
rg -n "pattern" apps/desktop/src apps/desktop/sidebar packages/core-ui packages/shared \
  server/src tooling \
  -g '!.dependencies/**' -g '!node_modules/**' -g '!storybook-static/**' -g '!tmp/**' \
  -g '!dist/**' -g '!build/**' -g '!out/**' -g '!target/**' -g '!artifacts/**' -g '!.git/**'
```

### Prompt-history search is Rust; the old Zig Zehn source is gone

`ghostex f` runs the picker **in-process** from the `packages/find/` Rust crate (`ghostex-find`), compiled into gxserver and the `ghostex` CLI. There is no `bin/zehn` to stage, no `GHOSTEX_ZEHN_BIN`, and no `ZEHN_ZIG` (releases still need Zig 0.16 for ghostty and zmx, and 0.16 is the repo's only Zig toolchain). The Zig `zehn` submodule was removed: never restore, build, or bundle it, or treat it as the spec for new work; change `packages/find/` instead. The terminal picker and the GUI (`packages/core-ui/find/`) share one key map (agents `^g`, projects `^j`, moved from `^t`/`^r` because browsers reserve Ctrl+T and Ctrl+R) and the same scanner, matcher, Codex cache, and favorites file, so a prompt starred in one is starred in the other. Anything that makes them rank or star differently is a bug.

### Session daemons: zmx (POSIX, including WSL) and wmx (native Windows)

`.dependencies/wmx/` is the independent [maddada/wmx](https://github.com/maddada/wmx) submodule, the native Windows ConPTY counterpart to `.dependencies/zmx/`; its README holds the shared API/behaviour table and its AGENTS.md the maintenance rules. App-specific startup and paths belong in `server/src/zmx/scripts_windows.rs`, not wmx. Whenever you change a Ghostex-consumed zmx feature, inspect the matching wmx implementation and update both providers or explain why the other is unaffected; visibility OSCs, the 200-column resting grid, client leadership, attach/detach persistence, history/refresh, title coalescing, and prompt-editor capabilities must stay aligned. Run wmx's real Windows smoke script after changing these contracts. Each provider has its own wire generation; bump only for incompatible IPC changes. Existing `nativeSessionProtocol: 1` sessions migrate as wmx generation 1 without being killed.

**zmx wire generation.** A zmx daemon keeps running the binary that spawned it and talks to the bundled client over the private IPC contract in `.dependencies/zmx/src/ipc.zig`, versioned by `WIRE_GENERATION` (printed by `zmx version`, recorded per session). On every gxserver start, the wire-cycle pass in `server/src/zmx/wire_cycle.rs` kills every live daemon whose recorded generation differs from the bundled binary's and lets the session resume lazily through wake-on-open. **Cycling kills the agent running inside the session** (in-flight subagents, background jobs, and unfinished tool calls are lost), which is why only a generation bump, never binary identity, may cycle daemons. Full mechanism and history: `ai/zmx-wire-generation.md`. Rules when editing zmx:

- Bump `WIRE_GENERATION` exactly when an old daemon can no longer serve a new client: a `Tag` renumbered or removed, an existing tag's payload layout or meaning changed, or a client that requires a reply to a new tag without a compatibility probe. The bump is the whole mechanism; add no cycling code. Update the frozen-tag tests and the `CDXC:ZmxWireGeneration` comment in `ipc.zig` and the four emitters listed under "zmx behaviour" above.
- Do not bump for additive or internal changes (a new tag old daemons drop through `_` while clients tolerate the silence, a daemon-side fix, a log line, a perf change, an upstream merge that leaves the framing alone). If the new client needs an answer, do what `SendAcked` does: probe first and treat no reply as "old daemon".
- A bump restarts every live session on the next `bun run start`: say so in your report, check `ghostex sessions` for `running` entries, and let the user pick a quiet moment to install.
- Never skip the stamp or bypass the pass (env switch, build flag, or leaving `wire_generation` out of `zmx version`); an unreadable generation cycles nothing and turns the next real wire break into blank panes. Sessions stamped by the retired `zmxBinaryStamp` scheme count as generation 1; do not "clean up" that migration.
- Verify with `zmx version` from `.dependencies/zmx/zig-out/bin/zmx` and `zig build test` inside `.dependencies/zmx` before shipping; on macOS build the way `prepare-macos-runtime.sh` does rather than patching the SDK.
- `ghostex server stop` / `stop-all` do not replace the pass and must not be used to "test" a zmx change on a machine with live agents.

### CDXC comments: why the code exists, and what the user decided

`CDXC:<Area> <yyyy-MM-dd> <KIND>:` comments are the codebase's memory of non-obvious reasons and of decisions the user made while prompting agents. They are greppable (`rg 'CDXC:RemotePairing'`; `rg 'CDXC:.* DECISION:'` lists every user decision) and are the first thing to read before changing behaviour in an area. Read them first, write them sparingly, keep them true. Full text with good and bad examples: `ai/cdxc-comments.md`.

- **Kinds.** `DECISION` is an instruction the user gave: quote or closely paraphrase their words; only the user creates decisions, and an agent's own design choice is a `WHY`. `WHY` is a non-obvious reason, an external constraint, or an approach that was tried, failed, and must not be retried. `SEE-ALSO` lists the other files, tags, or contracts that must stay in lockstep, used only where a feature spans crates, apps, or the zmx/ghostty trees. Comments written before 2026-09-03 without a kind read as `WHY`; do not retrofit them.
- **Write one only if** a reader of the diff would ask "why not the obvious way?", the user gave an explicit instruction the code implements, an earlier approach was removed and must not come back, or the file is one of several that must change together. Every explicit product or UX decision from the user gets one `DECISION` comment next to the code; routine requests ("fix this bug", "rename this") do not. Never write: what the code does, "never touches / must not expose" disclaimers, a new dated entry per iteration on the same day (collapse into the final decision), or a tag naming a single change, PR, task, or file.
- **Areas.** `<Area>` names a user-facing feature or a shared contract and must come from `ai/AREAS.md`; use the existing area even for a sub-feature and put the detail in the text. Never derive a tag from a file, struct, surface (`GPUI…`, `React…`), or task name, and never create variants of an existing area. Create a new area only when nothing covers the feature, it is user-visible or a cross-crate contract, and several comments will share it; add its line to `ai/AREAS.md` in the same commit and say so in your report.
- **Format.** One line `CDXC:<Area> <yyyy-MM-dd> <KIND>:` then plain sentences (date only, no manual wrapping), as a doc comment on the item that owns the behaviour (`///`/`//!` in Rust, `/** */` in TS, `///` in Zig, block comment in CSS), in the per-concern file, never `mod.rs` or `index.ts`. When requirements change, replace the comment (new date, one sentence on what it supersedes) instead of stacking; delete it when the code is gone.
- **Conflicts with a DECISION.** When your task would change behaviour covered by one, stop and tell the user: quote the comment, state what the task wants instead, and ask which wins. If the user confirms the change, update the comment in the same commit. Never delete or weaken a `DECISION` without that exchange.

### Ghostex Help must stay true to the product

`skills/ghostex-help/` is what an agent reads when a user asks "how do I…" or "change X for me" inside Ghostex (`ghostex guide`, `ghostex settings`, the titlebar Help button). A wrong or missing answer there is a customer-facing bug, so the guide is part of the feature, not documentation to fill in later. **The test:** would a customer plausibly ask about it, or ask an agent to set it up? If yes, the guide must answer; internal mechanisms, refactors, fixes that restore documented behaviour, and details nobody would ask about leave it alone.

- Update it in the same commit when you add, remove, or rename a titlebar view, sidebar surface, Settings page, or titlebar button; add or change something a user does on purpose (agents, sessions, chats, worktrees, the board, automations, remote or mobile access, notifications, the browser and editor); add or change a `ghostex` CLI verb users or agents run by hand; change how one of the seven Help sample questions is answered; or add, rename, or retire a setting, option value, default, or hotkey.
- Hand-written: `references/features.md` (one paragraph per feature in the product's own words, ending with the settings keys or CLI commands; edit the existing section, never append a changelog) and `references/overview.md` (only when the window layout or a core concept changes). Generated from the Settings modal search rows, defaults, and hotkey catalog by `bun run help:generate` (commit the output; `bun run typecheck` fails when stale; never hand-edit): `settings.md`, `hotkeys.md`, `settings-catalog.json`. Setting titles and subtitles come from those search rows, so write them for a customer. The seven sample questions in `apps/desktop/src/app/titlebar/help_menu.rs` are a user decision: change them only on instruction.
- Do not add implementation details, file paths, internal state, debugging rows, or anything a customer would never ask about. When in doubt, write the one sentence a support person would say and stop. gxserver embeds the reference files with `include_str!`, so `ghostex guide` always matches the installed CLI. Full text: `ai/ghostex-help-upkeep.md`.

### Bundled agent skills ship from GitHub main, not only from releases

gxserver installs the bundled skills under `skills/` by downloading them from this repository's `main` branch (verified against git blob shas, app-bundle copy as the offline source) and refreshes installed skills that differ from `main` on every start (`server/src/agent_skills_remote.rs`, `server/src/agent_skills.rs`). So a push to `main` touching `skills/**` reaches every installed Ghostex on its next start: treat skill edits as customer-facing and never push a half-finished skill. A skill must keep working with the CLI verbs of the oldest release still in use; when it needs a new verb, say so in the skill text and prefer `ghostex guide` over copying details in. `bundled_cli_skill_assets` in `apps/desktop/scripts/build-macos-app.sh` still needs every skill name for offline installs. `GHOSTEX_AGENT_SKILLS_REMOTE=off` (or `gxserver agent-skills install --offline` for one command) turns the download off; use it when testing local skill edits so a Reinstall does not fetch `main` over them.

### Chat: one Rust brain, native renderers

The sidebar's behaviour is Rust (`packages/gx-core/` for the rules, `apps/desktop/src/app/gx_store/` for the store, `apps/desktop/src/app/native_sidebar/` for the renderer); the only half still in QuickJS is the gxserver runtime that feeds it (`apps/desktop/sidebar/gxserver-runtime/`). Chat follows the same shape. Port plan and status: `docs/2026-09-21/rust-chat/PLAN.md`.

- Chat rules (messages, streaming, tool grouping, questions, approvals, drafts, queues, errors, settings, transcript presentation) go only into `packages/gx-chat-core/`. The desktop app and the GPUI web build both run it through the same chat host, `apps/desktop/src/app/gx_chat/` (the web build links those files and swaps in its own `worker.rs`, `storage_backend.rs` and `platform.rs`), and both reach gxserver through `packages/gx-chat-client` (one chat socket per machine, native or browser). The QuickJS chat runtime, the `chatBrain` setting, the chat broker and the web build's TypeScript chat were deleted on 2026-09-25 (user decisions). Do not patch the TypeScript chat brain (`packages/shared/session-chat-controller/`, `packages/shared/session-chat-presentation/`): it survives only for the phone's previous web chat and the replay gate (`tooling/gx-chat-core/`), and nothing on desktop or in `apps/gpui-web` may import it.
- Drawing changes go in `apps/desktop/src/app/native_chat/`, which renders the core's document; `apps/gpui-web` compiles the same files. Keep the document's JSON contract described in `packages/gx-chat-core/src/lib.rs`. Chat must work without a CEF page: rendering, background work, subscriptions, timers and persistence.
- Mobile uses the Rust chat core: native React Native views fed by `gx-chat-core` through UniFFI (user decision 2026-09-21). React chat (`packages/core-ui/chat/`, `apps/mobile/views/chat/`) is going away; add no features or parity work there.
- A chat bug about the session itself (what reaches the terminal, what the agent's screen shows, what the transcript records) is a gxserver fix, not a chat-core fix, so it works for every client at once.
- Keep shared storage ownership, validation, budgets, revision checks, and recovery rules in `packages/client-storage/`. Native persistence uses the native adapter and must preserve existing saved data when migrating from browser storage.
- Preserve the chat's theme, font, zoom, transcript width, verbose/simple modes, file previews, keyboard controls, scrolling and composer actions. Verify the result in the GPUI chat and report any unverified interaction explicitly.
- **Test the GPUI chat in a browser (Ghostex web GPUI), not the user's window.** `apps/gpui-web` compiles the desktop's own `native_chat/`, sidebar and terminal files (symlinked) to wasm against the live gxserver. Run `bun run web:build` and `ghostex web --dist-dir "$PWD/apps/gpui-web/www/dist" --no-open` (the built page and its bootstrap on :4173), or keep that running and use `bun run web:dev` for the Vite dev server on :4174, and open `http://localhost:4174/?session=<projectId>:<sessionId>` (or :4173 for the built page). It runs the same Rust chat host and core as the desktop, so a chat fix can be proven there; drafts live in the page's own client storage (a fresh headless profile starts empty, so test a reload inside one run). It also compiles the desktop's own store executor files (`gx_store/`: sidebar actions, create, groups, the Git menu, Quick Access, the native dialogs), so a sidebar action can be proven there too; what a page cannot do (Finder, panes, CEF dialogs, remote tunnels) answers with a toast. If desktop changes broke its build, add the missing symlinks or web stand-ins as its README describes. Drive a throwaway agent with `ghostex create-agent`, `ghostex send-session-chat-message` and `answer-session-chat-prompt`, and read its terminal with the bundle's `zmx history <zmxName>`. The page is a canvas: `node shot.mjs out.png --url … --click x,y --wheel x,y,dy --type …` clicks, scrolls and screenshots headlessly (set `SHOT_CHROME` to Playwright's `chrome-headless-shell` when Chrome for Testing never commits a navigation); cua-driver `browser_click` refs land on the accessibility mirror, and foreground clicks or typing steal the user's keystrokes.

### Never generate fallbacks when the right solution is to correct the behaviour itself

Fallbacks add complexity, hide issues, and introduce useless logic, so they are for rare cases only. Bad: "VS Code webviews block the local-fonts permission, so I'm patching the helper to fall back cleanly instead of passing unusable local-font sources into Restty." Right: stop generating local font sources at all when the current webview environment cannot use the local-fonts capability, so Ghostty starts in the correct mode instead of trying and failing first.

### Native layout and hit-testing discipline

Applies to the active desktop app (GPUI views, CEF pages, AppKit shims, Ghostty terminal hosts; WKWebView wording refers to the deprecated Swift app, rule unchanged). Lay out interactive AppKit, WKWebView, CEF, Ghostty, sidebar, titlebar, pane, and divider regions as non-overlapping sibling or child frames. Do not solve click, drag, hover, or focus bugs by stacking transparent views, extending webviews under native chrome, adding broad parent/window hit-test routing, or hiding overlap between interactive regions. Use real, exact native views for interactive boundaries such as splitters and sidebar dividers: make the visible divider itself the grab target rather than adding invisible overlap, and keep visual-only chrome non-interactive.

Before adding any `hitTest` override, NSWindow pre-dispatch mouse routing, synthetic coordinate rerouting, invisible interactive overlay, or intentional overlap between interactive regions, stop, explain to the user why strict normal layout cannot solve it, and get explicit confirmation. Native child windows are the accepted pattern for app modals, dropdowns, command palette, rename, Resources, Tips & Tricks, and similar overlays; they own their own frames and input and must not be replaced with main-window transparent webview overlays or root-level hit-test shields.

### Shared UI controls: one component per control kind

Some controls are deliberately owned by a single shared component so every surface renders the same thing. Use them instead of hand-rolling a lookalike out of `Button`s or raw `ToggleGroup`s, and change the shared component (plus its story) when the look must change:

- **Segmented single-select** ("pick exactly one of N": Sidebar version, Preset, Add Worktree mode, Automate schedule/execution): `packages/components/ui/segmented-control.tsx` (`SegmentedControl` / `SegmentedControlItem`), the stock shadcn ButtonGroup shape with a highlighted fill on the selected segment. Story: `Components/Segmented Control`. Its canonical CSS lives unlayered in `packages/core-ui/styles.css` and is mirrored in `apps/desktop/views/project-board/styles.ts` because the Kanban/Automate page loads only `shadcn.generated.css`.
- **Toggle switch**: `packages/components/ui/switch.tsx`, one shape app-wide (6px track, 4px thumb). Don't reintroduce per-surface pill overrides.
- **Focus ring**: the chat composer's ring is the reference, 3px at `ring-ring/20` plus `border-ring`. Every shared primitive uses that value; never raise it back to `ring-ring/50` or `/30`. Surfaces that deliberately have no ring (the modal tab rails) stay ringless.

### UX mockups: one HTML file per screen, annotation-friendly classes

When asked to mock up a UI or a flow, build static HTML under `docs/<YYYY-MM-DD>/<topic>/`, never Storybook stories or product code: one `.html` per screen or state plus an `index.html` hub in flow order, shared `shared.css` / `shared.js`, a device frame on the left and short design notes on the right. Feedback arrives as CSS selector paths, so every landmark the user might point at (frames, cards, rows, steps, buttons, sheets, notes) gets a descriptive class or data attribute on the element they would click, plus a selector cheat sheet in `index.html`. Copy says "computer", never "Mac", and uses user-facing feature names ("Easy Connect", not "Tailcat"). Match the Kanban / Automate look (near-black page, `#161616` panels, `#1d1d1d` cards, hairline borders) unless told otherwise, and screenshot with headless Chrome to fix clipping and overflow before reporting. Full conventions: `ai/ux-mockups.md`.

### Destructive git/file operations safety rule

"Revert your changes" never means resetting, restoring, cleaning, or deleting the whole worktree; other agents and the user have unrelated uncommitted and untracked work here. Before any destructive command (`git restore .`, `git checkout -- .`, `git reset --hard`, `git clean`, `rm -rf`, deleting untracked files, and the like): show the user the exact files/directories affected, say whether each is tracked or untracked, confirm they are definitely your own changes and not user work, and get explicit approval. To revert only your own changes, inspect diffs and revert exactly the hunks/files you changed. When uncertain, stop and ask; never use broad restore/clean commands as a shortcut.

### Never lose other agents' uncommitted work

Files you touched earlier in your session, or read a while ago, may have been changed by someone else since. Treat every uncommitted change you did not make yourself as protected user work.

- Before editing a file you last read a while ago (or carry from an earlier plan, worktree, or thread), re-read its current on-disk content and apply your change to that as a targeted edit. Never write back a whole file from a stale copy in your context: that silently erases every change other agents made in between, with no way to recover it from git. (On 2026-07-09 an automated batch commit did exactly this to the uncommitted CEF sidebar persistence fix in what is now `apps/desktop/src/cef/shell/`; the fix vanished without a trace and had to be re-diagnosed from scratch.)
- Never run `git checkout`, `git restore`, `git stash`, or `git reset` on a path that has hunks you did not author.
- When committing, never selectively drop pending hunks in files you commit. Include a file's whole pending diff, or split hunk-by-hunk only if you verify afterwards (`git status` + `git diff`) that every excluded hunk still exists in the working tree. A batch "split the working tree into topical commits" pass must end with zero silently-vanished hunks.
- Changes you cannot attribute to your own task stay intact; mention them to the user instead of "cleaning them up".
- After you verify a surgical bug fix, tell the user it should be committed promptly (or commit it when they ask) so concurrent agents cannot wipe it.

### Rules for running commands

- TypeScript is gated by two configs, not one: `bun run typecheck` (root: `packages/shared`, `packages/core-ui`, `packages/components`, `apps/desktop/views`, `apps/mobile/views`) and `bun run desktop:typecheck` (`apps/desktop/tsconfig.json`, covering `apps/desktop/sidebar/` and `apps/desktop/views/`). A change under `apps/desktop/sidebar/` is only checked by `desktop:typecheck`.
- Run desktop-crate cargo commands **from inside `apps/desktop/`**, never with `--manifest-path` from the repo root: the crate pins its toolchain in `apps/desktop/rust-toolchain.toml` (1.95.0), and `--manifest-path` from the root resolves the root toolchain and fails on dependency code that needs the pin.
- Local Rust builds of `apps/desktop/` and `server/` require `sccache` on PATH (`rustc-wrapper = "sccache"` in each crate's `.cargo/config.toml`, which cargo reads only when run from inside the crate directory). If cargo fails with `could not execute process 'sccache'`, run `brew install sccache`; never delete the config or build with `--manifest-path` from the root. Setup details: README.md, "Building from source".

### `bun run start` is a fast dev build, not a release build (macOS)

The local start is tuned so a small edit rebuilds and relaunches in about 10 seconds. Keep it that way, and don't mistake its shortcuts for bugs. Details, measurements and the rejected alternatives: `ai/local-start-performance.md`.

- **The `ghostex-gpui` and `gxserver` crates compile at opt-level 0** (their dependencies stay fully optimized). Never judge performance (scroll stutter, frame times, CPU use, startup time) from a default start: rebuild with `bun run start --optimized` first, and say which build you measured. `bun run build` and releases are always fully optimized.
- **code-server is not inside local-start bundles.** Each build is cloned once into `build/dev-components.noindex/code-server/<arch>-<hash>/`; the bundle keeps only `Web/code-server/lib/node` and a `Web/local-start-code-server-root` pointer. Editing `.dependencies/code-server` still works: the next start rebuilds it into a new folder. Don't "restore" the missing payload to the bundle.
- **Staging copies binaries only when their source changed**, so existing signatures are reused. Never go back to `rm -rf` + `cp` or a plain `rsync` for signed payloads in `build-macos-app.sh`; use `stage_file_if_changed` / `stage_tree_if_changed`.
- **The start keeps the running gxserver when its build identity is unchanged** and stops it only when the bundled gxserver changed. `server/rust-toolchain.toml` pins the same Rust version as `apps/desktop/rust-toolchain.toml`; bump both together.
- **Never install a build into `/Applications` by hand** (`ditto` to `Ghostex-new.app`, `mv Ghostex.app Ghostex.old-<time>.app`, and the like). Every hand install left a 1.7GB copy behind. `bun run start` is the only install path. If it reports that macOS App Management blocks it, tell the user to turn App Management on for Ghostex or their terminal and stop there. The start deletes leftover `Ghostex.old-*`, `Ghostex.broken-*` and `Ghostex-new` copies, and prunes rustc incremental caches to the newest one per binary.
- **Adding work to the start path:** put it behind a content-hash stamp (`build-cache.sh`) so an unchanged start skips it, and measure an unchanged start (target: under 10s) and a one-line Rust edit before and after.

### Before committing and pushing: formatting and file-size upkeep

Before you commit and push, run `ghostex sessions` (see `ghostex --help`) and look only at the group whose project path matches the folder you are working in; your own session is one `running` entry there, and sessions in other projects or worktrees do not block anything. Full procedure and the exact commands: `ai/formatting-and-file-size.md`.

- **Another session in this worktree is `running`**: format only the files you yourself changed and commit path-scoped. No repo-wide formatting pass and no file splits, which would sweep their uncommitted work into your commit or create churn under them.
- **No other session here is `running`** (everything else is `sleep` or the list is empty): run the full-repo formatting pass (`cargo fmt` per crate, `apps/desktop` from inside its folder; `bunx prettier --write` over the app-owned trees only) and the file-size upkeep pass, then commit (with your changes or as a separate `chore: Formatting` commit) and push. Never format `.dependencies/**`, `node_modules/**`, the `apps/mobile/app/**` submodule, generated files (`*.generated.*`, `dist/`, `build/`, `target/`, `apps/desktop/runtime/`), or `bun.lock`. Afterwards run the typecheck/test gates, review `git status`, and commit only formatting deltas plus your own work; leave out any file the pass touched that has foreign uncommitted hunks.

**File-size upkeep (same quiet-worktree window only).** Since the 2026-08-24 split wave every app-owned source file is under ~2,000 lines (a few deliberate keeps aside) and the big Rust god-files are per-concern module directories. Do not regress:

- **Add new code to the module that owns the concern, not to whichever file is open.** Where a split directory exists, new functions go into the matching per-concern file or a new sibling, never into `mod.rs`/`index.ts`, which stay thin re-export barrels. This applies always, agents running or not.
- **Don't let files grow back.** If an app-owned file you touched has grown past ~1,500 lines, split it during the quiet window (never while other agents run here) using the established recipe (directory + `mod.rs` re-export barrel or `index.ts`; one big `impl GhostexGpuiApp` becomes sibling files with their own `impl` blocks; see `docs/2026-08-22/repo-restructure/SPLITS.md`). Splits are motion, not rewrites: bodies move byte-identically, item counts match, and raw-source tests or comment citations of the old path are retargeted in the same commit. If the window never opens during your session, tell the user the file needs a split instead of skipping it silently.
- Deliberate exception: `apps/desktop/src/terminal_element.rs` (~4.6k lines) stays whole for perf-critical locality. Don't split it, and don't cite it as precedent.

### Diagnostic logging workflow

- Routine disk logs must have an explicit **Diagnostic disk logging scenario** and may write only while both **Show debug UI controls** and that unexpired scenario are enabled. Do not add unscoped routine disk logging; errors, crashes, and important warnings remain unconditional.
- Before testing or requesting a reproduction that needs diagnostic logs, record the current logging settings, then enable only the smallest set of scenarios needed with the shortest useful expiry. Reproduce the issue yourself when authorized and practical; otherwise ask the user to reproduce it after confirming the required scenarios are enabled.
- As soon as the evidence is collected or the attempt is abandoned, restore exactly the state you observed: turn off every scenario and debug switch you enabled (extra diagnostics consume disk and CPU and make the user's computer lag), but do not turn off anything the user had already enabled.

### Ghostex app debugging: build the code being diagnosed

Ordinary authorization to inspect or operate Ghostex applies only to the instance the user already has running. Do not launch, restart, replace, or open another copy unless the user explicitly requests it; outside an explicit skill workflow, never use `cua-driver launch_app`, Computer Use launch/open actions, macOS `open`, `bun run start`, or another app-start operation merely to discover or attach to Ghostex. If no instance is running, ask before launching anything. If more than one Ghostex process or app copy is present, do not guess: identify the newly rebuilt or user-selected instance from process and window state, and ask the user when it remains ambiguous.
