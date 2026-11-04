//! Execute a plan. The plan is recomputed from a fresh scan right before it
//! runs so what executes matches the disk, and only the caller's enabled groups
//! run. Failures are recorded and never roll back earlier steps: the plan is
//! idempotent, so rerunning finishes the job. The one exception is a link that
//! replaces something this run moved to its backup: when the link cannot be made,
//! the original goes back (CDXC:AgentSync 2026-10-27 in fsx.rs).

use crate::fsx;
use crate::plan::{build_plan, PlanGroupKind, PlanOp, PlanOptions, PlanVerb, SyncPlan};
use crate::scan::scan;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The OS calls that make links: the real ones, or a test's.
pub(crate) struct Linker {
    pub dir: fn(&Path, &Path) -> io::Result<fsx::LinkKind>,
    pub file: fn(&Path, &Path) -> io::Result<fsx::LinkKind>,
}

const REAL_LINKER: Linker = Linker {
    dir: fsx::create_dir_link,
    file: fsx::create_file_link,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyFailure {
    pub op: PlanOp,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub generated_at: String,
    pub scope: String,
    pub enabled_groups: Vec<PlanGroupKind>,
    pub done: Vec<PlanOp>,
    pub failed: Vec<ApplyFailure>,
    pub skipped_keeps: usize,
    /// The plan that was executed, for display.
    pub plan: SyncPlan,
}

fn run_op(op: &PlanOp, linker: &Linker) -> Result<(), String> {
    let path = &op.abs_path;
    match op.verb {
        PlanVerb::Keep => Ok(()),
        PlanVerb::Unlink => fsx::remove_link(path).map_err(|e| e.to_string()),
        PlanVerb::Mkdir => fs::create_dir_all(path).map_err(|e| e.to_string()),
        PlanVerb::Backup => {
            let target = op
                .abs_target
                .as_ref()
                .ok_or("backup without a destination")?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::rename(path, target).map_err(|e| e.to_string())
        }
        PlanVerb::Link => {
            let target = op.abs_target.as_ref().ok_or("link without a target")?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if fs::symlink_metadata(path).is_ok() {
                return Err("something is already at this path".to_string());
            }
            let resolved = if target.is_absolute() {
                target.clone()
            } else {
                path.parent()
                    .map(|p| p.join(target))
                    .unwrap_or_else(|| target.clone())
            };
            let is_dir = fs::metadata(&resolved).map(|m| m.is_dir()).unwrap_or(true);
            if is_dir {
                (linker.dir)(target, path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                (linker.file)(target, path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
        PlanVerb::Write => {
            let content = op.content.as_deref().ok_or("write without content")?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, content).map_err(|e| e.to_string())
        }
        PlanVerb::Drop => Ok(()),
    }
}

fn prune_lock(lock_path: &Path, names: &[String]) -> Result<(), String> {
    let text = fs::read_to_string(lock_path).map_err(|e| e.to_string())?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let Some(skills) = value.get_mut("skills").and_then(|s| s.as_object_mut()) else {
        return Err("lock file has no skills object".to_string());
    };
    for name in names {
        skills.remove(name);
    }
    let rendered = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(lock_path, format!("{rendered}\n")).map_err(|e| e.to_string())
}

/// Puts back what this run moved to `backup` from `path` before a link there failed, and says so
/// after the link's own error.
pub(crate) fn restore_after_failed_link(error: String, path: &Path, backup: &Path) -> String {
    if fs::symlink_metadata(path).is_ok() {
        return format!(
            "{error}; the original is still in its backup at {} because something else is at this path now",
            backup.display()
        );
    }
    match fs::rename(backup, path) {
        Ok(()) => format!("{error}; the original was put back"),
        Err(restore) => format!(
            "{error}; the original is still in its backup at {} ({restore})",
            backup.display()
        ),
    }
}

/// Scan, plan, and run the enabled groups of the plan.
pub fn apply(home: &Path, options: &PlanOptions, enabled: &[PlanGroupKind]) -> ApplyResult {
    apply_with(home, options, enabled, &REAL_LINKER)
}

pub(crate) fn apply_with(
    home: &Path,
    options: &PlanOptions,
    enabled: &[PlanGroupKind],
    linker: &Linker,
) -> ApplyResult {
    let report = scan(home);
    let plan = build_plan(home, &report, options);
    let mut done = Vec::new();
    let mut failed = Vec::new();
    let mut skipped_keeps = 0;
    let mut drops: Vec<(std::path::PathBuf, String)> = Vec::new();
    // What this run moved aside, by the path it came from, to put back if its link fails.
    let mut backed_up: HashMap<PathBuf, PathBuf> = HashMap::new();
    for group in &plan.groups {
        if !enabled.contains(&group.kind) {
            continue;
        }
        for op in &group.ops {
            match op.verb {
                PlanVerb::Keep => skipped_keeps += 1,
                PlanVerb::Drop => {
                    if let Some(name) = &op.target {
                        drops.push((op.abs_path.clone(), name.clone()));
                    }
                }
                _ => match run_op(op, linker) {
                    Ok(()) => {
                        if op.verb == PlanVerb::Backup {
                            if let Some(target) = &op.abs_target {
                                backed_up.insert(op.abs_path.clone(), target.clone());
                            }
                        }
                        done.push(op.clone());
                    }
                    Err(error) => {
                        let backup = (op.verb == PlanVerb::Link)
                            .then(|| backed_up.remove(&op.abs_path))
                            .flatten();
                        let error = match backup {
                            Some(backup) => {
                                let message =
                                    restore_after_failed_link(error, &op.abs_path, &backup);
                                // A backup that went back is no longer done.
                                if fs::symlink_metadata(&backup).is_err() {
                                    done.retain(|earlier: &PlanOp| {
                                        !(earlier.verb == PlanVerb::Backup
                                            && earlier.abs_path == op.abs_path)
                                    });
                                }
                                message
                            }
                            None => error,
                        };
                        failed.push(ApplyFailure {
                            op: op.clone(),
                            error,
                        });
                    }
                },
            }
        }
    }
    if let Some((lock_path, _)) = drops.first().cloned() {
        let names: Vec<String> = drops.iter().map(|(_, name)| name.clone()).collect();
        match prune_lock(&lock_path, &names) {
            Ok(()) => {
                for group in &plan.groups {
                    for op in group.ops.iter().filter(|op| op.verb == PlanVerb::Drop) {
                        done.push(op.clone());
                    }
                }
            }
            Err(error) => {
                for group in &plan.groups {
                    for op in group.ops.iter().filter(|op| op.verb == PlanVerb::Drop) {
                        failed.push(ApplyFailure {
                            op: op.clone(),
                            error: error.clone(),
                        });
                    }
                }
            }
        }
    }
    ApplyResult {
        generated_at: crate::now_iso8601(),
        scope: options.scope.as_str(),
        enabled_groups: enabled.to_vec(),
        done,
        failed,
        skipped_keeps,
        plan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsx::tests::TempDir;
    use crate::plan::PointerPolicy;
    use crate::SyncScope;

    fn refuse(_: &Path, _: &Path) -> io::Result<fsx::LinkKind> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "A required privilege is not held by the client. (os error 1314); a directory junction did not work either: not a local volume",
        ))
    }

    fn options(home: &Path) -> PlanOptions {
        PlanOptions {
            scope: SyncScope::All,
            stamp: "test".to_string(),
            backup_root: home.join("backups"),
            pointer_policy: PointerPolicy::Replace,
            link_hooks: true,
            link_lock: false,
        }
    }

    /// Claude Code with its own hooks folder, and a shared `~/.agents/hooks` to link it to.
    fn home_with_agent_hooks(label: &str) -> TempDir {
        let home = TempDir::new(label);
        fs::create_dir_all(home.0.join(".agents/hooks")).expect("shared hooks");
        fs::write(home.0.join(".agents/hooks/shared.sh"), "echo shared").expect("shared");
        fs::create_dir_all(home.0.join(".claude/hooks")).expect("agent hooks");
        fs::write(home.0.join(".claude/hooks/mine.sh"), "echo mine").expect("mine");
        fs::write(home.0.join(".claude/settings.json"), "{}").expect("settings");
        home
    }

    #[test]
    fn a_hooks_folder_moved_to_its_backup_goes_back_when_the_link_fails() {
        let home = home_with_agent_hooks("restore");
        let linker = Linker {
            dir: refuse,
            file: refuse,
        };
        let result = apply_with(&home.0, &options(&home.0), &[PlanGroupKind::Hooks], &linker);
        let hooks = home.0.join(".claude/hooks");
        assert_eq!(
            fs::read_to_string(hooks.join("mine.sh")).ok().as_deref(),
            Some("echo mine")
        );
        assert!(!fsx::is_symlink(&hooks));
        let failure = result
            .failed
            .iter()
            .find(|failure| failure.op.abs_path == hooks)
            .expect("the link failed");
        // The error the user already saw, then what happened to their folder.
        assert!(failure
            .error
            .starts_with("A required privilege is not held by the client."));
        assert!(failure.error.ends_with("the original was put back"));
        assert!(!result
            .done
            .iter()
            .any(|op| op.verb == PlanVerb::Backup && op.abs_path == hooks));
    }

    #[test]
    fn a_link_that_works_keeps_the_backup() {
        let home = home_with_agent_hooks("linked");
        let result = apply(&home.0, &options(&home.0), &[PlanGroupKind::Hooks]);
        let hooks = home.0.join(".claude/hooks");
        assert!(result.failed.is_empty(), "{:?}", result.failed);
        assert!(fsx::is_symlink(&hooks));
        assert!(fsx::same_entry(&hooks, &home.0.join(".agents/hooks")));
        assert!(result
            .done
            .iter()
            .any(|op| op.verb == PlanVerb::Backup && op.abs_path == hooks));
    }
}
