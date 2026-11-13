//! `cargo xtask <command>`: the repository's dev, start and build commands. The `xtask` alias lives in the root `.cargo/config.toml`, so every command runs from the repository root on macOS, Linux and Windows.
//! The packaging shell scripts under apps/desktop/scripts/ still do the building; JavaScript tools (bun, vite, tsc, vitest, tailwind) still run for the pages and checks that are TypeScript.

mod agentation;
mod codesign;
mod format;
mod gxserver;
mod help;
mod http;
mod isolated;
mod mobile_chat_wasm;
mod rebuild;
mod remote_build;
#[cfg(unix)]
mod setup_worktree;
mod start;
mod start_server;
mod tasks;
#[cfg(unix)]
mod test_server;
mod util;

const USAGE: &str = "Usage: cargo xtask <command> [arguments]

Desktop app
  start [--optimized] [--debug-info[=<level>]] [--verbose] [--profile] [--build-only] [--install-only] [--isolated[=<variant>]] [--prepare-only]
                          build, install and launch the desktop app (a fast dev build; --optimized for performance work;
                          --debug-info builds with symbols for crash debugging, off by default;
                          --install-only installs the already staged build on Linux and Windows)
  rebuild [--dry-run] [--allow-dirty] [--no-health-check] [--wait[=<minutes>]] [--log-dir <dir>] [--optimized]
                          macOS, for a coordinator's background shell: start from committed code only, into a numbered
                          log (~/.local/share/agent-work/Ghostex/rebuilds), retrying kache's known flake once, then
                          print a short summary (result, time, gxserver and app pids, a health check a minute later, log, last errors)
  remote-start [--repo <owner/name>] [--build-only]
                          Linux x64: build the desktop app on a Blacksmith machine, then install and launch it here
                          (setup: tooling/remote-build/README.md)
  start-server [--optimized]
                          macOS: rebuild only gxserver and swap it into /Applications/Ghostex.app
  build                   macOS: package the app bundle without installing it
  gx-isolated [--isolated=<variant>] [--print | <ghostex arguments>]
                          run an isolated variant's ghostex CLI
  setup-windows           Windows: install the build prerequisites and prepare sources
  setup-worktree [--with <submodule path>]... [--no-zmx]
                          macOS and Linux: prepare a fresh `git worktree` for building: build submodules at their pins (fetched from the
                          main checkout's copies), its CEF and Zig package caches, bun install, a ReleaseSafe zmx

Testing
  test-server start [--name <n>] [--projects <path>...] [--port <p>] [--no-build] [--dry-run] | env <name> | stop <name> | remove <name> | list
                          a throwaway gxserver on its own GHOSTEX_HOME and port for agents to test against
                          (~/.local/share/agent-work/Ghostex/test-servers/<name>); prints the env lines for the ghostex CLI

Web build
  web-build [--debug]     build apps/gpui-web into apps/gpui-web/www/dist
  web-dev [--debug]       build the wasm and run the Vite dev server on :4174
  web-test [<cargo test arguments>]
                          macOS and Linux: run apps/gpui-web's tests on this computer (native, not wasm)
  start-web [<ghostex web arguments>]
                          build the web app and serve it with `ghostex web` on :4173

Checks and generators
  typecheck               root gate (storage lint, Help files, tsc, release scripts, formatting)
  format                  format every app-owned file (cargo fmt per crate, prettier; tooling/format/scope.json)
  format-check            fail when an app-owned file is not formatted (part of typecheck)
  desktop-typecheck       apps/desktop/tsconfig.json (CEF entry modules, Files embed page)
  test [<vitest arguments>]
                          the vitest suites
  storage-check           client-storage access lint
  help-generate           regenerate the skills/ghostex-help references from packages/settings-catalog
  help-check              fail when those generated files are stale
  build-sidebar-css       regenerate packages/core-ui/styles/shadcn.generated.css
  build-editor            build the GhostexEditor page and app
  generate-mobile-chat-agents
                          write the phone's chat-capable agent list
  generate-mobile-settings-search-words
                          write the phone Settings search's copy of the desktop's words per setting
  generate-mobile-tabler-icons
                          regenerate the phone's Tabler icon set
  build-agentation        build the Annotate tool (the Agentation fork in .dependencies/agentation)
                          into apps/desktop/vendor/agentation/agentation.generated.js (needs pnpm)
  build-mobile-chat-wasm  build the phone's chat core as WebAssembly into
                          apps/mobile/app/assets/chat-core/chat-core.html (needs binaryen's wasm-opt)

Other
  gxserver-remote-linux [--arch x64|arm64|all] [--allow-cross] | --release
                          package gxserver for remote Linux hosts
  history [<arguments>]   run the ghostex-history CLI
  cli [<arguments>]       run the checkout's ghostex CLI
  remote-build-pack --stage <dir> --base <manifest> --out <dir>
                          (runs on the remote build machine) pack the files that differ from the laptop's staged app

Release commands stay in package.json for now (`bun run release:*`).";

fn main() {
    util::sanitize_cargo_run_environment();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        std::process::exit(if args.is_empty() { 2 } else { 0 });
    }
    let name = args.remove(0);
    let result = match name.as_str() {
        "start" => start::run(&args),
        "start-server" => start_server::run(&args),
        "rebuild" => rebuild::run(&args),
        "remote-start" => remote_build::run(&args),
        "remote-build-pack" => remote_build::pack(&args),
        "build" => tasks::build(&args),
        "gx-isolated" => isolated::run_cli(&args),
        #[cfg(unix)]
        "test-server" => test_server::run(&args),
        #[cfg(not(unix))]
        "test-server" => {
            Err("test-server runs on macOS and Linux (gxserver's zmx sessions).".into())
        }
        "setup-windows" => tasks::setup_windows(&args),
        #[cfg(unix)]
        "setup-worktree" => setup_worktree::run(&args),
        #[cfg(not(unix))]
        "setup-worktree" => Err(
            "setup-worktree runs on macOS and Linux. On Windows, `cargo xtask setup-windows` prepares the sources."
                .into(),
        ),
        "web-build" => tasks::web_build(&args),
        "web-dev" => tasks::web_dev(&args),
        "web-test" => tasks::web_test(&args),
        "start-web" => tasks::start_web(&args),
        "typecheck" => tasks::typecheck(&args),
        "format" => format::run(&args),
        "format-check" => format::check(&args),
        "desktop-typecheck" => tasks::desktop_typecheck(&args),
        "test" => tasks::test(&args),
        "storage-check" => tasks::storage_check(&args),
        "help-generate" => tasks::help_generate(&args),
        "help-check" => tasks::help_check(&args),
        "build-sidebar-css" => tasks::build_sidebar_css(&args),
        "build-editor" => tasks::build_editor(&args),
        "generate-mobile-chat-agents" => {
            tasks::mobile_script(&name, "generate-chat-agents.mjs", &args)
        }
        "generate-mobile-settings-search-words" => {
            tasks::mobile_script(&name, "generate-settings-search-words.mjs", &args)
        }
        "generate-mobile-tabler-icons" => {
            tasks::mobile_script(&name, "generate-tabler-icons.mjs", &args)
        }
        "build-mobile-chat-wasm" => mobile_chat_wasm::run(&args),
        "build-agentation" => agentation::build(&args),
        "gxserver-remote-linux" => tasks::gxserver_remote_linux(&args),
        "history" => tasks::history(&args),
        "cli" => tasks::cli(&args),
        other => Err(format!("unknown command `{other}`.\n\n{USAGE}").into()),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("cargo xtask {name}: {error}");
            std::process::exit(1);
        }
    }
}
