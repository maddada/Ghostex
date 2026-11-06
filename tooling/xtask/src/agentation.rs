//! `cargo xtask build-agentation`: the Annotate tool's script, built from Ghostex's fork of
//! Agentation (`.dependencies/agentation`, branch `ghostex`) into
//! `apps/desktop/vendor/agentation/agentation.generated.js`, which the app bundles (the Browser
//! toolbar and Storybook through `helpers/browser.rs`, HTML files in Files through
//! `views/manage/preview/html-viewer.tsx`). The file is committed so a build needs no Node
//! toolchain for it; its first line names the fork commit it was built from, and
//! `cargo xtask typecheck` fails when that is not the commit the submodule is pinned to.
//!
//! SEE-ALSO: .gitmodules (CDXC:Browser 2026-11-07), THIRD_PARTY_NOTICES.md (Agentation).

use std::fs;
use std::process::Command;

use crate::bail;
use crate::util::{check, output, root, Res};

const SUBMODULE: &str = ".dependencies/agentation";
const BUNDLE: &str = "apps/desktop/vendor/agentation/agentation.generated.js";
const BUILT: &str = "package/dist-ghostex/agentation-ghostex.js";

pub fn build(args: &[String]) -> Res<i32> {
    if let Some(arg) = args.first() {
        bail!("`cargo xtask build-agentation` takes no arguments (got {arg}).");
    }
    let fork = root().join(SUBMODULE);
    if !fork.join("package/package.json").exists() {
        bail!("{SUBMODULE} is not checked out (git submodule update --init {SUBMODULE}).");
    }
    let package = fork.join("package");
    check(
        Command::new("pnpm")
            .current_dir(&package)
            .args(["install", "--frozen-lockfile"]),
    )?;
    check(
        Command::new("pnpm")
            .current_dir(&package)
            .args(["run", "build:ghostex"]),
    )?;
    let bundle = fs::read_to_string(fork.join(BUILT))?;
    let out = root().join(BUNDLE);
    fs::create_dir_all(out.parent().unwrap())?;
    fs::write(&out, &bundle)?;
    fs::copy(fork.join("LICENSE"), out.with_file_name("LICENSE"))?;
    println!("Wrote {} ({} bytes).", out.display(), bundle.len());
    check_pinned()?;
    Ok(0)
}

/// Fails when the committed bundle was not built from the commit the submodule is pinned to.
pub fn check_pinned() -> Res {
    let index = output(
        Command::new("git")
            .current_dir(root())
            .args(["ls-files", "-s", "--", SUBMODULE]),
    )?;
    let listing = String::from_utf8_lossy(&index.stdout);
    let Some(pinned) = listing.split_whitespace().nth(1) else {
        bail!("{SUBMODULE} is not a submodule in the index.");
    };
    let bundle = fs::read_to_string(root().join(BUNDLE))
        .map_err(|error| format!("{BUNDLE}: {error} (run `cargo xtask build-agentation`)."))?;
    let first_line = bundle.lines().next().unwrap_or_default();
    let built_from = first_line
        .split("maddada/agentation ")
        .nth(1)
        .and_then(|rest| rest.split([')', ' ']).next())
        .unwrap_or_default();
    if built_from != pinned {
        bail!(
            "{BUNDLE} was built from agentation {built_from}, but {SUBMODULE} is pinned to {pinned}. Run `cargo xtask build-agentation`."
        );
    }
    Ok(())
}
