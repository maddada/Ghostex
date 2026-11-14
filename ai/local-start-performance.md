# Local start performance (`cargo xtask start`, macOS and Linux)

macOS: measured on the maintainer's M5 Pro (18 cores, 64GB, macOS 27) on 2026-09-22/23. The build half of the start (everything except install and launch) was run directly with the start's environment.

| Case | Before | After |
|---|---|---|
| Unchanged start, build half | nested items re-signed every run | ~5s, 19 of 20 signatures reused |
| One-line Rust edit in the app crate, build half | ~20s compile at opt-level 3 plus full re-signing | ~10s (6s compile) |
| Cold compile of the app crate | 315 CPU-seconds (opt-level 3) | 71 CPU-seconds (opt-level 0) |
| Dev bundle size | 1.7GB | 1.1GB |

The launcher moved from `tooling/start-gpui.mjs` (bun) to `cargo xtask start` on 2026-10-01 without changing either case. Measured back to back with `--build-only` while other agents were building (load 10 to 15): unchanged build half 6.2 to 6.5s with bun, 5.9 to 6.6s with the xtask; one-line Rust edit 17.5 to 18.6s with bun, 17.6 to 18.2s with the xtask. `cargo xtask`'s own freshness check costs about 0.4s, which the old launcher spent starting bun twice (once under lockf) and loading its modules.

Linux: measured on the user's Ryzen 9 5900XT (16 cores, 32 threads, 62GB, Omarchy) on 2026-11-14, running the start's own build scripts with its environment (no install). Until then the Linux start built both leaf crates as plain opt-level 3 releases, one after the other, and every `cargo xtask rebuild` took about 380s before the relaunch.

| Case (Linux) | Before | After |
|---|---|---|
| One-line edit in the app crate | ~185 to 205s | 19.5s |
| One-line edit in gxserver (whole package step) | ~165 to 170s | 15.2s (cargo 14s) |
| Both edited, both builds | ~380s, one after the other | 20.5s, side by side |
| Cold compile of both leaf crates | | 86.5s, side by side |

## What the start does

`cargo xtask start` (`tooling/xtask/src/start/`) takes an flock on `build/ghostex-gpui-local-start.lock`, then runs, on macOS:

1. `apps/desktop/scripts/build-macos-rust.sh` and `build-macos-sidebar.sh` in the background.
2. `prepare-macos-runtime.sh` in the foreground: zmx (Zig), code-server staging, gxserver (cargo), into `apps/desktop/runtime/macos/Web`.
3. `build-macos-app.sh` with the prebuilt Rust binaries: stages `apps/desktop/build/macos.noindex/Ghostex.app` and signs it with `codesign-gpui-app.sh`.
4. Closes the installed app, keeps or stops gxserver, syncs the bundle to `/Applications/Ghostex.app` with rsync, checks the signature, and opens it.

Every expensive step sits behind a content-hash stamp in `build/<arch>/build-cache/`.

On Linux it runs:

1. `apps/desktop/scripts/build-linux-rust.sh` in the background (the app and the CEF helper, dev settings).
2. `server/package-remote-linux.mjs --local-start` in the foreground: gxserver (cargo), zmx (Zig, cached by Zig when unchanged), tailcat (built once into `build/remote-gxserver-linux/<arch>/tailcat-cache/`), into `build/remote-gxserver-linux/<arch>/package`.
3. `build-linux-app.sh` with the prebuilt Rust binaries (`GHOSTEX_GPUI_USE_PREBUILT_RUST=1`): the sidebar pages (Vite), and stages `apps/desktop/build/linux/Ghostex` (CEF payload, pages, sounds, the gxserver package). Release packaging runs this script without `GHOSTEX_LOCAL_START` and keeps its own `cargo build --release --bins`.
4. Closes the installed app, keeps or stops gxserver, syncs into `/opt/ghostex` (or XDG data) and opens it with `systemd-run --user --scope`.

## Decisions and why

- **opt-level 0 for the two leaf crates** (`CDXC:Build 2026-09-23 DECISION` in `build-macos-rust.sh`; Linux since 2026-11-14, `CDXC:Build 2026-11-14 DECISION` in `tooling/xtask/src/start/mod.rs`). For a one-line edit the rebuild took ~5s at opt-level 0, ~17s at 1 and ~20s at 3. opt-level 1 is not worth it. `--optimized` (or `GHOSTEX_START_OPTIMIZED=1`) restores release optimization for both crates. Incremental compilation stays on for local starts only. On Linux, gxserver gets the flags from `package-remote-linux.mjs --local-start`, not from `GHOSTEX_LOCAL_START`, because the macOS start also runs that script (the Cloud Box package) with the variable set.
- **kache must be told to keep incremental** (`KACHE_INCREMENTAL_CRATES`, set by `util::kache_incremental_crates_environment` for every start and `start-server`). kache strips `-C incremental` from what it compiles unless the crate is on that list, so the incremental setting above did nothing: a one-line gxserver edit at opt-level 0 took 71s through kache and 14s with the crates listed (Linux, 2026-11-14). kache then keeps the cache in `incremental.kache-auto/` beside cargo's `incremental/`, one folder per build unit, and cleans it up itself. Without kache the variable is ignored.
- **The two Rust builds run side by side on both platforms.** They share no target folder, and each spends most of its time in one leaf crate that uses only a few cores.
- **Linux builds only the shipped binaries in a local start** (the app and its CEF helper, plus the CEF bootstrap with `GHOSTEX_ON_DEMAND_ASSETS=1`). `--bins` also compiled the demo binaries, and `native-modal-demo` alone cost up to 45s when shared modules changed.
- **code-server outside the bundle** (`CDXC:CodeEditor 2026-09-23 DECISION`). Its 5.5k files were re-sealed by the outer signature, verified, synced and malware-scanned on every start. The app resolves the folder named in `Web/local-start-code-server-root` right after the bundle candidates (`source_code_server_repo_root_candidates`). The start's process matching also covers that folder when it closes the installed app. The two newest folders are kept.
- **Copy only on source change.** Signing modifies staged binaries, so they never matched their sources and were re-copied unsigned and re-signed on every start. `stage_file_if_changed` / `stage_tree_if_changed` stamp the source identity instead.
- **Parallel inside-out signing** (`GHOSTEX_GPUI_SIGN_JOBS`, default 8). Local starts skip the post-sign strict deep verify because the start deep-verifies the installed copy right after the sync.
- **gxserver kept when unchanged.** The build identity hashes the whole staged package, so a match means the running daemon is already the code the start would launch.
- **Linux gxserver packages** stay in dev bundles because remote install reads them from there; they are rsynced instead of deleted and re-copied.
- **Installs stay in place, never hand-made.** On 2026-09-21 macOS App Management blocked the sync into `/Applications/Ghostex.app` halfway. An agent then hand-installed every later build as `Ghostex-new.app` and moved the old app aside as `Ghostex.old-<time>.app`, leaving five 1.7GB copies in two days. The start now probes write access before closing the app and stops with the App Management fix if it is blocked. It deletes leftover copies that carry the app's bundle id and are not running. It does not install by swapping in a new bundle because a kept gxserver and live zmx sessions run from files inside the installed one.
- **Incremental cache pruning** (`CDXC:Build 2026-09-23 DECISION` in `tooling/xtask/src/start/mod.rs`). Every build configuration gets its own rustc incremental cache and nothing removed old ones: 26GB across 387 caches on 2026-09-23. Each start keeps the newest cache per crate in every `incremental` folder of both targets and leaves anything touched in the last 30 minutes alone. The trade-off is that switching between configurations (for example `--optimized` and back) starts that configuration's cache from cold.
- **Windows stale-artifact sweep** (`CDXC:Build 2026-10-11 DECISION` in `tooling/xtask/src/start/sweep.rs`). Cargo never deletes old units, and on Windows the release target folders kept every copy left by dependency, feature and flag changes (the desktop folder held 2,090 rlibs for 579 crates). `build-windows-app.ps1` records the units each release build used in `target/release/ghostex-build-units.jsonl`; after a successful build the start deletes, at most once a day per target folder and only while it holds cargo's lock there, every `deps/` file and `build/` folder whose unit that build did not use and nothing touched for three days.
- Smaller items: the storage check and signing-identity probe run once, under the lock; app PIDs come from `lsappinfo` (~0.01s) instead of `osascript` + System Events (~0.27s) in the 100ms polls; the app icon is stamped; `reportCompressedSize` is off in `apps/desktop/vite.config.ts`.

## Debug info (off by default)

Every Rust build in the repository has no debug info and no symbol table, and on Windows the linker writes no `.pdb` (`CDXC:Build 2026-10-11 DECISION` in the root `.cargo/config.toml`). A panic still logs its message and file:line; backtrace frames print as `<unknown>`.

When the user asks for crash-debugging info, build with it for that one build:

- `cargo xtask start --debug-info` (or `GHOSTEX_DEBUG_INFO=1 cargo xtask start`) gives function names and file:line in backtraces and symbols for crash dumps. `--debug-info=full` adds variables for a debugger.
- Plain cargo commands take the same environment the flag sets: `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, `CARGO_PROFILE_RELEASE_STRIP=none` (the `DEV` pair for dev builds), and on Windows `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C link-arg=/DEBUG"` so the linker writes the `.pdb` again.

Switching in either direction recompiles everything once (kache helps); the next build without the flag goes back to no debug info.

## Rejected or deferred (checked 2026-09-22)

- **Other linkers.** Apple's ld-prime is the fastest macOS linker today. lld cannot link against the macOS 27 SDK yet (rust#162928), and wild and mold have no usable macOS backend. On Linux, Rust already links with `rust-lld`, and linking the app takes about a second, so mold or wild would save under 2s (checked 2026-11-14).
- **Cranelift and the parallel front end (`-Zthreads`).** Nightly only. Cranelift also has macOS ABI problems with `objc_msgSend`.
- **kache** (replaced sccache on 2026-11-04). Keep it. It caches bin crates too and shares outputs across worktrees; the two leaf crates compile incrementally outside the cache (once they are on `KACHE_INCREMENTAL_CRATES`, above), so a low hit rate on everyday rebuilds is expected. It pays off after a clean, a branch switch, a new worktree or a toolchain change.
- **Splitting the app crate.** At opt-level 0 a cold compile of the app crate is ~60s of single-threaded front-end work, which only splitting the ~170k-line crate into several crates would parallelize. That is the long-term fix, and a large refactor.
- **`taskpolicy -b` / `nice` on builds you wait for.** Background QoS slowed one measured job from 7.4s to 115s, and `nice` has no measurable effect on Apple silicon.

## Machine settings that help (user's choice, not repo changes)

- Privacy & Security > Developer Tools: add the terminal and Ghostex itself (agents build inside Ghostex sessions). This exempts their child processes from XProtect scans of freshly built build scripts; `sudo spctl developer-mode enable-terminal` makes the pane appear.
- Spotlight > Search Privacy: add `apps/desktop/target`, `server/target` and `build/` (`.metadata_never_index` no longer works; a `.noindex` folder suffix does).
- Time Machine: `sudo tmutil addexclusion -p /Applications/Ghostex.app` (cargo already excludes `target/`).
- Linux: nothing measured beats the repo settings. The CPU governor was already `performance`; a tmpfs target folder, mold and more RAM were not worth it (2026-11-14). From an agent shell, `~/.cargo/bin` must be on PATH, or kache's cargo shim fails with "finding Cargo behind the kache shim".
