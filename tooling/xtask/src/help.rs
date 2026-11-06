//! `cargo xtask help-generate` and `help-check`: the Ghostex Help reference files, written from the
//! Settings catalog crate (`packages/settings-catalog`).
//!
//! CDXC:AgentSkills 2026-10-01 WHY:
//! `skills/ghostex-help/references/settings.md`, `hotkeys.md` and `settings-catalog.json` are committed because gxserver embeds the Markdown in `ghostex guide` and the skill ships from GitHub `main`; `help-check` (part of `cargo xtask typecheck`) fails while they differ from what the crate renders. This replaces `tooling/ghostex-help/generate.ts`, whose output it reproduces byte for byte.
//! SEE-ALSO: packages/settings-catalog/src/help/, server/src/ghostex_cli/guide.rs.

use std::fs;

use ghostex_settings_catalog::help;

use crate::util::{root, Res};

/// Each generated file and its rendered content.
fn outputs() -> [(&'static str, String); 3] {
    [
        (
            "skills/ghostex-help/references/settings-catalog.json",
            help::published_catalog_json(),
        ),
        (
            "skills/ghostex-help/references/hotkeys.md",
            help::hotkeys_markdown(),
        ),
        (
            "skills/ghostex-help/references/settings.md",
            help::settings_markdown(),
        ),
    ]
}

/// The relative paths whose committed content differs from the rendered one; rewrites them unless `check`.
fn sync(check: bool) -> Res<Vec<&'static str>> {
    let mut stale = Vec::new();
    for (path, rendered) in outputs() {
        let absolute = root().join(path);
        if fs::read_to_string(&absolute).ok().as_deref() == Some(rendered.as_str()) {
            continue;
        }
        stale.push(path);
        if !check {
            if let Some(parent) = absolute.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&absolute, rendered)?;
        }
    }
    Ok(stale)
}

fn counts() -> String {
    let catalog = help::catalog();
    format!(
        "{} settings, {} hotkeys",
        catalog.settings.len(),
        catalog.hotkeys.len()
    )
}

pub fn generate() -> Res<i32> {
    let stale = sync(false)?;
    if stale.is_empty() {
        println!("Ghostex Help references are up to date ({}).", counts());
    } else {
        println!(
            "Wrote {} Ghostex Help reference file(s) ({}).",
            stale.len(),
            counts()
        );
    }
    Ok(0)
}

pub fn check() -> Res<i32> {
    let stale = sync(true)?;
    if stale.is_empty() {
        println!("Ghostex Help references are up to date ({}).", counts());
        return Ok(0);
    }
    eprintln!(
        "Ghostex Help references are stale. Run `cargo xtask help-generate` and commit:\n  {}",
        stale.join("\n  ")
    );
    Ok(1)
}
