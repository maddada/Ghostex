//! `cargo xtask setup-worktree`: everything a fresh `git worktree` of this repository needs before `cargo xtask start`, `web-build` or `test-server` work in it.
//!
//! CDXC:Build 2026-11-13 WHY: a fresh worktree has empty `.dependencies` submodules, no `node_modules`, no CEF download and no built zmx. `git submodule update --init` from GitHub crawls through zed, `--reference <main checkout>` is refused because the main checkout's submodules are shallow, and cef-dll-sys hangs downloading CEF. So each submodule's pinned commit is fetched from the main checkout's own copy (seconds instead of minutes; GitHub only for a pin the main checkout lacks), the main checkout's CEF and Ghostty Zig package caches are reused, and zmx is built the way the start builds it. It never runs `git submodule deinit`: worktrees share `.git/config`, so that would unregister the main checkout's submodules.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::bail;
use crate::util::{self, check, command, output, root, stdout_if_ok, Res};

/// The submodules every cargo build here reads: the desktop and web builds compile zed, gpui-component and cef-rs from `.dependencies`, and the start and test-server run zmx.
const BUILD_SUBMODULES: &[&str] = &[
    ".dependencies/zed",
    ".dependencies/gpui-component",
    ".dependencies/cef-rs",
    ".dependencies/zmx",
];

const USAGE: &str = "Usage: cargo xtask setup-worktree [--with <submodule path>]... [--no-zmx]

Prepares this worktree for `cargo xtask start`, `web-build` and `test-server`:
  - checks out the build submodules (zed, gpui-component, cef-rs, zmx) at their pins, fetched from
    the main checkout's copies when they have the pinned commit, else from GitHub (shallow); the
    main checkout's .git/config is never written
  - reuses the main checkout's CEF download (apps/desktop/build/cef-cache) and Ghostty's Zig
    package cache, so no build downloads them again
  - runs `bun install --frozen-lockfile`
  - builds zmx (ReleaseSafe), which the start and `cargo xtask test-server` need

  --with <path>   also check out another submodule, e.g. apps/mobile/app or .dependencies/wmx
  --no-zmx        skip the zmx build";

pub fn run(args: &[String]) -> Res<i32> {
    let mut extra: Vec<String> = Vec::new();
    let mut build_zmx = true;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--with" => match rest.next() {
                Some(path) => extra.push(path.trim_end_matches('/').to_string()),
                None => bail!("--with needs a submodule path.\n\n{USAGE}"),
            },
            "--no-zmx" => build_zmx = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            other => bail!("unknown argument `{other}`.\n\n{USAGE}"),
        }
    }

    preflight()?;
    let main = main_checkout()?;
    match &main {
        Some(main) => println!("Main checkout: {}", main.display()),
        None => println!("This is the main checkout; nothing to borrow from."),
    }

    let names = submodule_names()?;
    for path in BUILD_SUBMODULES
        .iter()
        .map(|path| path.to_string())
        .chain(extra)
    {
        let Some(name) = names.iter().find(|(_, p)| *p == path).map(|(n, _)| n) else {
            bail!("{path} is not a submodule in .gitmodules.");
        };
        checkout_submodule(name, &path, main.as_deref())?;
    }

    if let Some(main) = &main {
        reuse_caches(main)?;
    }

    println!("==> bun install --frozen-lockfile");
    check(&mut util::bun(["install", "--frozen-lockfile"]))?;

    if build_zmx {
        build_zmx_release()?;
    }

    println!(
        "\nWorktree ready. Next: `cargo xtask test-server start`, `cargo xtask web-build`, or `cargo xtask start`."
    );
    Ok(0)
}

/// The tools every build needs, checked before anything is downloaded so a missing one fails in the first second.
fn preflight() -> Res {
    let mut missing = Vec::new();
    for (tool, hint) in [
        ("git", "install Git"),
        ("bun", "https://bun.sh"),
        (
            "kache",
            "brew install kunobi-ninja/kunobi/kache && kache init (CONTRIBUTING.md, Building from source)",
        ),
    ] {
        if stdout_if_ok(Command::new(tool).arg("--version")).is_none() {
            missing.push(format!("  {tool}: {hint}"));
        }
    }
    if !missing.is_empty() {
        bail!("Missing build tools:\n{}", missing.join("\n"));
    }
    Ok(())
}

/// The main checkout this worktree belongs to (the folder holding the shared `.git`), or None when this is it.
fn main_checkout() -> Res<Option<PathBuf>> {
    let common = git_stdout(
        root(),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = PathBuf::from(common);
    let Some(main) = common.parent().filter(|_| common.ends_with(".git")) else {
        bail!(
            "{} is not inside a normal checkout's .git; run setup-worktree from a `git worktree add` folder.",
            common.display()
        );
    };
    let same = fs::canonicalize(main)? == fs::canonicalize(root())?;
    Ok((!same).then(|| main.to_path_buf()))
}

/// `(name, path)` of every submodule in `.gitmodules`.
fn submodule_names() -> Res<Vec<(String, String)>> {
    let listing = git_stdout(
        root(),
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    )?;
    Ok(listing
        .lines()
        .filter_map(|line| {
            let (key, path) = line.split_once(' ')?;
            let name = key.strip_prefix("submodule.")?.strip_suffix(".path")?;
            Some((name.to_string(), path.to_string()))
        })
        .collect())
}

/// CDXC:Build 2026-11-13 WHY: never `git submodule update --init` or `init` here. Linked worktrees share `.git/config`, and init writes `submodule.<name>.active`, the URL and the update mode into it, so it would change the main checkout's registration (Git documents the limitation in submodule--helper's init_submodule). The pinned commit is fetched into a repository in this worktree's own git folder (`<git dir>/modules/<name>`, where Git keeps a linked worktree's submodules) and is pointed at its path by hand, so every write lands in files that belong to this worktree.
fn checkout_submodule(name: &str, path: &str, main: Option<&Path>) -> Res {
    let pin = git_stdout(root(), &["rev-parse", &format!("HEAD:{path}")])?;
    let checkout = root().join(path);
    let url = git_stdout(
        root(),
        &[
            "config",
            "-f",
            ".gitmodules",
            &format!("submodule.{name}.url"),
        ],
    )?;
    let local = main
        .map(|main| main.join(path))
        .filter(|source| has_commit(source, &pin));
    if checkout.join(".git").exists() {
        // The checkout's own HEAD decides, never `git submodule status`: a module this command set up is not registered in the shared config, and status prints such a module as uninitialized whatever its HEAD is.
        let head = stdout_if_ok(command("git").args([
            "-C",
            path,
            "rev-parse",
            "--verify",
            "--quiet",
            "HEAD",
        ]));
        match head.as_deref().map(str::trim) {
            Some(head) if head == pin => println!("==> {path}: already checked out at its pin"),
            Some(head) => {
                println!("==> {path}: checked out at {head}, not its pin {pin}; left as it is")
            }
            // No commit yet: an earlier run stopped between creating the repository and checking out the pin (a failed fetch). With nothing in the folder but its `.git`, it is resumed.
            None => {
                let only_git_file = fs::read_dir(&checkout)?
                    .filter_map(Result::ok)
                    .all(|entry| entry.file_name() == ".git");
                if !only_git_file {
                    bail!("{path} has a repository with no commit and other files in it. Move them away, then run setup-worktree again.");
                }
                println!("==> {path}: finishing an earlier unfinished setup");
                let git_dir =
                    PathBuf::from(git_stdout(&checkout, &["rev-parse", "--absolute-git-dir"])?);
                fetch_pin(path, &pin, &url, local.as_deref())?;
                check_out_pin(path, &pin, &git_dir, &checkout)?;
            }
        }
        return Ok(());
    }
    if fs::read_dir(&checkout).is_ok_and(|mut entries| entries.next().is_some()) {
        bail!("{path} holds files but no checkout. Move them away, then run setup-worktree again.");
    }
    let git_dir = PathBuf::from(git_stdout(root(), &["rev-parse", "--absolute-git-dir"])?)
        .join("modules")
        .join(name);
    if git_dir.exists() {
        bail!(
            "{} is left from an earlier attempt. Delete that folder (it belongs to this worktree only), then run setup-worktree again.",
            git_dir.display()
        );
    }
    fs::create_dir_all(git_dir.parent().expect("a module folder has a parent"))?;
    check(
        command("git")
            .args(["init", "--quiet", "--separate-git-dir"])
            .arg(&git_dir)
            .arg(path),
    )?;
    check(command("git").args(["-C", path, "remote", "add", "origin", &url]))?;
    fetch_pin(path, &pin, &url, local.as_deref())?;
    check_out_pin(path, &pin, &git_dir, &checkout)
}

fn fetch_pin(path: &str, pin: &str, url: &str, local: Option<&Path>) -> Res {
    let mut fetch = command("git");
    fetch.args(["-C", path]);
    if let Some(source) = local {
        // The main checkout's copy may be shallow and its pin need not be a branch tip, so its upload-pack (started by this fetch, which is all these settings reach) may serve any commit it has.
        println!("==> {path}: fetching {pin} from {}", source.display());
        fetch
            .args(["-c", "protocol.file.allow=always"])
            .args(["-c", "uploadpack.allowAnySHA1InWant=true"])
            .args(["fetch", "--quiet", "--depth", "1"])
            .arg(format!("file://{}", source.display()));
    } else {
        println!("==> {path}: fetching {pin} from {url} (shallow)");
        fetch.args(["fetch", "--quiet", "--depth", "1", "origin"]);
    }
    check(fetch.arg(pin))
}

fn check_out_pin(path: &str, pin: &str, git_dir: &Path, checkout: &Path) -> Res {
    check(
        command("git")
            .arg("--git-dir")
            .arg(git_dir)
            .args(["config", "core.worktree"])
            .arg(fs::canonicalize(checkout)?),
    )?;
    check(command("git").args(["-C", path, "checkout", "--quiet", "--detach", pin]))
}

fn has_commit(repository: &Path, commit: &str) -> bool {
    repository.join(".git").exists()
        && stdout_if_ok(Command::new("git").arg("-C").arg(repository).args([
            "cat-file",
            "-e",
            &format!("{commit}^{{commit}}"),
        ]))
        .is_some()
}

/// CEF and Ghostty's Zig packages are large downloads keyed by version or hash, so a worktree uses the main checkout's.
fn reuse_caches(main: &Path) -> Res {
    // cef-dll-sys reads the CEF build named by CEF_PATH/<CEF build>/, which the build scripts point at apps/desktop/build/cef-cache. A link inside the real (ignored) build folder keeps git status clean, and a CEF bump adds its new versioned folder to the shared cache.
    let cef_source = main.join("apps/desktop/build/cef-cache");
    let cef_link = root().join("apps/desktop/build/cef-cache");
    if cef_source.is_dir() && fs::symlink_metadata(&cef_link).is_err() {
        fs::create_dir_all(cef_link.parent().expect("cef-cache has a parent"))?;
        std::os::unix::fs::symlink(&cef_source, &cef_link)?;
        println!(
            "==> apps/desktop/build/cef-cache -> {}",
            cef_source.display()
        );
    }
    // Ghostty's zig-pkg is copied, not linked: Zig writes into it, and a copy-on-write clone costs no disk on APFS.
    let pkg_source = main.join(".dependencies/ghostty/zig-pkg");
    let pkg_target = root().join(".dependencies/ghostty/zig-pkg");
    if pkg_source.is_dir() && fs::symlink_metadata(&pkg_target).is_err() {
        let mut copy = Command::new("cp");
        if cfg!(target_os = "macos") {
            copy.arg("-cR");
        } else {
            copy.args(["-R", "--reflink=auto"]);
        }
        check(copy.arg(&pkg_source).arg(&pkg_target))?;
        println!("==> .dependencies/ghostty/zig-pkg copied from the main checkout");
    }
    Ok(())
}

/// A ReleaseSafe zmx in `.dependencies/zmx/zig-out/bin`, where the start and `test-server` look for it. On macOS the start's own script builds it, so the SDK workarounds stay in one place.
fn build_zmx_release() -> Res {
    println!("==> building zmx (ReleaseSafe)");
    if cfg!(target_os = "macos") {
        check(
            command("bash")
                .arg(root().join("apps/desktop/scripts/prepare-macos-runtime.sh"))
                .env("GHOSTEX_LOCAL_START", "1")
                .env("GHOSTEX_MACOS_ZMX_ONLY", "1"),
        )
    } else {
        let zig = util::find_zig()?;
        check(
            Command::new(zig)
                .args(["build", "-Doptimize=ReleaseSafe"])
                .current_dir(root().join(".dependencies/zmx")),
        )
    }
}

fn git_stdout(dir: &Path, args: &[&str]) -> Res<String> {
    let out = output(Command::new("git").arg("-C").arg(dir).args(args))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
