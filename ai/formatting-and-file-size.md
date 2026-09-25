# Before committing and pushing: formatting and file-size upkeep

Referenced from `AGENTS.md` ("Before committing and pushing"). Full procedure and the exact formatting commands, moved here verbatim on 2026-09-18.

Before you commit and push, check whether other agents are currently working in **this same worktree/folder**: run `ghostex sessions` (see `ghostex --help`). The list is grouped by project path — only look at the group whose path matches the folder you are working in; sessions in other projects or other worktrees do not block anything. Your own session counts as one `running` entry in that group.

- **If any _other_ session in this worktree is `running`**: do NOT run a repo-wide formatting pass or start file splits — that would sweep their uncommitted work into your commit or create churn under them. Format only the files you yourself changed, then commit path-scoped as usual.
- **If no other session in this worktree is `running`** (everything else is `sleep` or the list is empty): run the full-repo formatting pass and the file-size upkeep pass below, then commit (your changes plus the formatting together, or as a separate `chore: Formatting` commit) and push.

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
  "apps/mobile/views/**/*.{ts,tsx}" \
  "packages/{shared,core-ui,components}/**/*.{ts,tsx,md}" \
  "server/**/*.mjs" "tooling/**/*.{mjs,ts}" "*.{json,md,ts}" ".github/**/*.yml"
```

Never format `.dependencies/**`, `node_modules/**`, `apps/mobile/app/**` (the submodule), generated files (`*.generated.*`, `dist/`, `build/`, `target/`, `apps/desktop/runtime/`), or `bun.lock` as part of a parent-repo pass. Format files intentionally changed inside a submodule within its own commit. After a repo-wide pass, run the typecheck/test gates before pushing, and review `git status` so you only commit formatting deltas plus your own work; if the pass touched a file with foreign uncommitted hunks, leave that file out of your commit.

**File-size upkeep pass (same quiet-worktree window only).** A repo-wide split wave finished on 2026-08-24: every app-owned source file is under ~2,000 lines except a handful of deliberate keeps, and the big Rust god-files (`render.rs`, `terminal_sync.rs`, `presentation.rs`, `os_cli.rs`, `remote_conn.rs`, the `helpers/*` monoliths, …) are per-concern module directories. Do not regress:

- **Add new code to the module that owns the concern, not to whichever file is open.** When a split directory exists (e.g. `apps/desktop/src/app/render/`, `helpers/os_cli/`, `server/src/presentation/`, `apps/desktop/src/app/gx_store/git/`), new functions go into the matching per-concern file, or a new sibling file — never into `mod.rs`/`index.ts`, which stay as thin re-export barrels. This applies always, agents running or not.
- **Don't let files grow back.** If an app-owned source file you touched has grown past ~1,500 lines, split it during this quiet window — never while other agents are running in the same worktree — using the established recipe: a directory with a `mod.rs` of flat `pub(crate) mod x;` + `pub(crate) use x::*;` re-exports (Rust) or a barrel `index.ts` (TS), moving code **verbatim** so no caller changes. For files that are one big `impl GhostexGpuiApp`, split into sibling files each with their own `impl` block. See `docs/2026-08-22/repo-restructure/SPLITS.md` for the proven pattern. If the window never opens during your session, tell the user the file needs a split instead of skipping it silently.
- **Splits must be motion, not rewrites.** A split carries a zero-logic-change burden: bodies move byte-identically, item counts match, and any raw-source test or comment citation pointing at the old path gets retargeted in the same commit.
- Deliberate exception: `apps/desktop/src/terminal_element.rs` (~4.6k lines) stays whole for perf-critical locality. Don't split it, and don't cite it as precedent for letting other files grow.
