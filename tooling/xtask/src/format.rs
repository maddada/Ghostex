//! `cargo xtask format` (the full formatting pass) and the format check at the end of `cargo xtask typecheck`.
//! Both cover the app-owned files in tooling/format/scope.json: `cargo fmt` per crate, each from inside its own folder so it
//! uses that crate's pinned toolchain and edition, and prettier over the globs. Agents' edits are formatted as they happen by
//! tooling/format/format-edited-file.mjs; this is the pass for everything else and the gate that nothing slipped through.

use std::process::{Command, Output};
use std::thread::{self, JoinHandle};

use crate::bail;
use crate::util::{bun_x, command, describe, root, spawn_error, Res};

struct Scope {
    crates: Vec<String>,
    prettier_globs: Vec<String>,
}

fn scope() -> Res<Scope> {
    let path = root().join("tooling/format/scope.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let list = |key: &str| -> Res<Vec<String>> {
        let Some(items) = value[key].as_array() else {
            bail!("{} has no `{key}` list.", path.display());
        };
        Ok(items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect())
    };
    Ok(Scope {
        crates: list("rustCrates")?,
        prettier_globs: list("prettierGlobs")?,
    })
}

fn cargo_fmt(krate: &str, extra: &[&str]) -> Command {
    let mut cargo = command("cargo");
    cargo.current_dir(root().join(krate)).arg("fmt").args(extra);
    cargo
}

fn prettier(scope: &Scope, mode: &str) -> Command {
    let mut prettier = bun_x("prettier", [mode]);
    prettier.args(&scope.prettier_globs);
    prettier
}

/// Every formatter at once, each on its own thread, so the slowest crate sets the time.
fn run_all(commands: Vec<Command>) -> Vec<JoinHandle<Result<(String, Output), String>>> {
    commands
        .into_iter()
        .map(|mut command| {
            thread::spawn(move || {
                let name = describe(&command);
                let output = command
                    .output()
                    .map_err(|error| spawn_error(&command, error).to_string())?;
                Ok((name, output))
            })
        })
        .collect()
}

/// `cargo xtask format`: formats every app-owned file in place.
pub fn run(args: &[String]) -> Res<i32> {
    if let Some(arg) = args.first() {
        bail!("`cargo xtask format` takes no arguments (got {arg}).");
    }
    let scope = scope()?;
    let mut commands: Vec<Command> = scope
        .crates
        .iter()
        .map(|krate| cargo_fmt(krate, &[]))
        .collect();
    commands.push(prettier(&scope, "--write"));
    let mut failed = 0;
    for handle in run_all(commands) {
        let (name, output) = handle.join().map_err(|_| "a formatter thread panicked")??;
        if !output.status.success() {
            eprintln!(
                "{name} failed:\n{}",
                String::from_utf8_lossy(&output.stderr).trim_end()
            );
            failed = 1;
        }
    }
    if failed == 0 {
        // prettier --write lists every file it read; the summary is enough.
        println!(
            "Formatted {} Rust crates and the prettier globs in tooling/format/scope.json.",
            scope.crates.len()
        );
    }
    Ok(failed)
}

/// The format check started in the background, so it runs while the rest of the typecheck does.
pub struct PendingCheck(Vec<JoinHandle<Result<(String, Output), String>>>);

pub fn start_check() -> Res<PendingCheck> {
    let scope = scope()?;
    let mut commands: Vec<Command> = scope
        .crates
        .iter()
        .map(|krate| cargo_fmt(krate, &["--check", "--", "--files-with-diff"]))
        .collect();
    commands.push(prettier(&scope, "--list-different"));
    Ok(PendingCheck(run_all(commands)))
}

impl PendingCheck {
    /// Prints the unformatted files and the command that fixes them; 0 when everything is formatted.
    pub fn finish(self) -> Res<i32> {
        let mut unformatted = Vec::new();
        let mut errors = Vec::new();
        for handle in self.0 {
            let (name, output) = handle
                .join()
                .map_err(|_| "a format check thread panicked")??;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let files: Vec<String> = stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| {
                    // cargo fmt names files through the target's folder (src/bin/../app/x.rs), so resolve before trimming the root.
                    let path = std::fs::canonicalize(root().join(line))
                        .unwrap_or_else(|_| std::path::PathBuf::from(line));
                    let root =
                        std::fs::canonicalize(root()).unwrap_or_else(|_| root().to_path_buf());
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .display()
                        .to_string()
                })
                .collect();
            // Both report differences with exit code 1; anything else (or exit 1 with no files) is the tool failing.
            let code = output.status.code().unwrap_or(-1);
            if code != 0 && (code != 1 || files.is_empty()) {
                errors.push(format!(
                    "{name} failed:\n{}",
                    String::from_utf8_lossy(&output.stderr).trim_end()
                ));
            }
            unformatted.extend(files);
        }
        unformatted.sort();
        unformatted.dedup();
        for error in &errors {
            eprintln!("{error}");
        }
        if !unformatted.is_empty() {
            eprintln!("These files are not formatted:");
            for file in &unformatted {
                eprintln!("  {file}");
            }
            eprintln!("Fix them with: cargo xtask format");
        }
        Ok(if errors.is_empty() && unformatted.is_empty() {
            0
        } else {
            1
        })
    }
}

/// `cargo xtask format-check`: the typecheck's format check on its own.
pub fn check(args: &[String]) -> Res<i32> {
    if let Some(arg) = args.first() {
        bail!("`cargo xtask format-check` takes no arguments (got {arg}).");
    }
    start_check()?.finish()
}
