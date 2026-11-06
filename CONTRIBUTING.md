## Contributing

Ghostex is moving quickly, and help is welcome on platform ports, missing agent CLI integrations, docs, testing, and feature polish.

Join the Discord: https://discord.gg/df7b3G92CS

### Web app source

The browser app is `apps/gpui-web`: the desktop's own GPUI source compiled to wasm. Build it and
serve it from the Ghostex root (gxserver must be running):

```sh
cargo xtask start-web
```

Its README covers the toolchain it needs (`wasm-bindgen-cli`, Zig 0.17) and the dev server.

### Building from source

`cargo xtask start` builds and launches the desktop app. On macOS, `cargo xtask build` only packages it.
The desktop crate (`apps/desktop/`) and the gxserver crate (`server/`) both pin Rust 1.99.1 in
their `rust-toolchain.toml` files. Besides Bun, Rust, CMake, Ninja, and Zig 0.17, local
Rust builds require **kache** (the Rust build cache; v2.0.0 or newer):

```sh
brew install kunobi-ninja/kunobi/kache   # Linux: cargo binstall kache, or the APT/AUR packages
kache init
kache doctor
```

#### Windows

Install Git for Windows, Bun, Node.js, rustup, and Visual Studio Build Tools with the
**Desktop development with C++** workload, a Windows SDK, and CMake tools for Windows.
Then run these commands from a native PowerShell window in the repository root:

```powershell
bun install --frozen-lockfile
cargo xtask setup-windows
cargo xtask start
```

`setup-windows` installs the pinned Rust toolchain without changing your global default,
downloads and verifies Zig 0.17.1 into `build/toolchains/`, and installs kache through
WinGet if it is missing. Without WinGet, install kache with `cargo install kache --locked`.
It also prepares the pinned desktop submodules and downloads the published WSL components.
It does not build, install, or launch the desktop app. Use `cargo xtask start --prepare-only` to
repeat source preparation without launching the app.
Open a new PowerShell window after installing kache if it is not yet on PATH.
The build loads the installed Visual Studio environment, so a Developer PowerShell window
is not required. Set `GHOSTEX_ZIG` to use an existing Zig 0.17.1 executable.

`start` initializes missing desktop submodules at their pinned revisions and preserves
existing checkouts. It checks the build tools before downloading the WSL runtime components.
Use `cargo xtask start-web` for the browser build, and `cargo xtask help` to list every command.

On Windows, `cargo xtask start` (and `bun run start`) installs the build the way the release
installer does: per user, without an administrator prompt, into `%LOCALAPPDATA%\Ghostex\current`,
with the `Ghostex` shortcut at the top of your Start Menu and the same app identity
(`velopack.Ghostex`) a release install has, so notifications, taskbar pins and the processes
(`current\resources\native\gxserver.exe`, `wmx.exe`, `ghostex.exe`) match what users run. It does not
install Velopack's `Update.exe`, so a development build never updates itself from the release
feed. `cargo xtask start --machine` installs to `C:\Program Files\Ghostex` instead (with an
administrator prompt) for testing a machine-wide install.

The first per-user start on a computer that used the older Program Files install closes that app,
stops its gxserver, and points your Desktop shortcut, taskbar pin and login item at the new copy.
Running terminal sessions keep going. It leaves `C:\Program Files\Ghostex` and the machine-wide
`Ghostex` Start Menu folder in place and names them in its output: delete them yourself once
no session started before the switch is still running.

To keep a development installation inside the checkout, set
`$env:GHOSTEX_INSTALL_DIR = "$PWD\build\local"` before `cargo xtask start` (Windows ignores the
generic `INSTALL_DIR`, so a toolchain's value cannot move the app). The app will run from
`build/local/Ghostex`, with a current-user Start Menu shortcut. Keep this directory separate from
`apps/desktop/build/windows/Ghostex`, which is the staging output replaced by the next build. To
persist the choice for later starts, put `GHOSTEX_INSTALL_DIR=D:/Ghostex/build/local` (using your
checkout path) in an untracked `.env.local` and exclude that file in `.git/info/exclude`.

Windows builds also prepare the native Code editor. A clean editor checkout can reuse its
published component when an authenticated GitHub CLI is available. To build it from source, initialize
`.dependencies/code-server` and its nested VS Code submodule, and install the
Node version pinned in `.dependencies/code-server/.node-version`, Python 3,
Git for Windows with Git LFS, jq, and Visual Studio C++ Build Tools with a Windows
SDK and the matching x64/x86 or ARM64 Spectre libraries. Keep these tools on the build
shell's PATH; `PYTHON` and `npm_config_msvs_version` can select a specific Python
executable and Visual Studio installation. The Windows build invokes
`apps/desktop/scripts/build-windows-code-server.ps1` and reuses its output when
the editor sources and toolchain have not changed.

#### Building Linux on a remote machine

On a Linux x64 computer that is too slow to build Ghostex, `cargo xtask remote-start` builds
the desktop app on a rented Blacksmith machine, downloads only the files that changed, and
installs and launches the result like `cargo xtask start`. Setup and costs:
`tooling/remote-build/README.md`.

#### Shared Rust build cache

`server/`, `apps/desktop/` and `apps/gpui-web/` set `rustc-wrapper = "kache"` in their
`.cargo/config.toml`, so every `cargo` invocation run from inside them (the build scripts,
`bun run release:preflight --cargo`, rust-analyzer, your shell) compiles each crate once and restores
it from the local kache store afterwards, including after `cargo clean` and in other worktrees. On
APFS and other copy-on-write file systems the restored files are clones, so the store and the target
folders share their disk blocks. If kache is missing, cargo fails with
`could not execute process 'kache'` instead of silently building without it.

`kache init` also points `~/.cargo/config.toml` at kache for your other Rust projects and starts
its background daemon. Set the store's size limit with
`kache config set --global cache.local_max_size 30GiB`, check setup with `kache doctor`, and see
hits and cache size with `kache stats`. `KACHE_DISABLED=1 cargo ...` builds once without the cache.

The root `.cargo/config.toml` uses `line-tables-only` debug information for development builds
and their derived test profiles. This keeps file/line backtraces while reducing compiler output;
local-variable debug information is omitted. Incremental compilation keeps its existing settings.

On macOS, `python3 tooling/clean-build-caches.py` previews cleanup of a fixed list of generated
Rust, Zig, Xcode, and Android caches. Add `--apply` to clean, or `--install` to register a user
LaunchAgent that checks daily at 04:30 local time and at login. It removes trees unchanged for
14 days, or the oldest eligible trees above a combined 10 GiB cache budget, after at least six
hours without changes. Build locks, detected compiler activity, open files, and Git tracking
checks protect active work. This is a cache budget, not a hard limit on the entire checkout.
The latest scheduled result replaces `~/Library/Application Support/Ghostex/build-cache-cleanup/last-run.json`.
Time Machine snapshots and backup settings are not part of this maintenance job.
