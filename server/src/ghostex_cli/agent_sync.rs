use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::paths::get_gxserver_paths;
use ghostex_agent_sync::{
    apply, build_plan, scan, AgentStatus, PlanGroupKind, PlanOptions, PlanVerb, SyncPlan,
    SyncReport, SyncScope,
};

/*
CDXC:AgentSync 2026-09-16 DECISION:
User: the CLI verb ships together with the Agents Hub tab and both share one crate (3A),
so an agent, a remote host, or a script gets exactly the scan and plan the Hub shows.
`apply` refuses to run without --yes because it renames folders and writes files.
*/

pub fn agent_sync_usage() -> String {
    "Ghostex Agent Sync - point every agent at the shared ~/.agents folder

Usage:
  gx agent-sync status [--json]
  gx agent-sync plan [--agent <id>] [--group <group>...] [--prune-lock] [--json]
  gx agent-sync apply --yes [--agent <id>] [--group <group>...] [--prune-lock] [--json]
  gx agent-sync agents [--json]

What it does:
  ~/.agents is the source of truth: skills/, main.md and the other rule files,
  hooks/, and .skill-lock.json. Sync creates one relative symlink per skill in
  every agent's global skills folder, converts whole-folder links into folders
  of per-skill links, writes the one-line instruction pointer file each agent
  reads, and links the hooks folder and lock file into Claude Code and Codex.
  Agents that read ~/.agents/skills themselves (Codex, GitHub Copilot, Pi,
  Cursor, OpenCode, Amp) get no links, and links already in their
  folders are removed so they do not load each skill twice.
  Nothing is deleted: anything in the way of a link, and any leftover
  <name>.pre-sync-<stamp>.bak folder in a skills folder, is moved to Ghostex's
  state folder (agent-sync-backups/<stamp>/, same path as under HOME), and
  dangling links are removed.

Groups (all on by default except prune-lock):
  remove-dangling        Remove symlinks whose target is gone
  remove-redundant-links Remove links into ~/.agents/skills from agents that
                         read that folder themselves
  move-backups           Move .pre-sync-*.bak folders out of skills folders
  convert-whole-folder   Replace a skills folder symlink with per-skill links
  per-skill-links        Link every source skill into the agent folder
  pointer-files          Write the instruction pointer files
  hooks                  Link the hooks folder and .skill-lock.json
  prune-lock             Drop lock entries that have no skill folder (opt-in)

Options:
  --agent <id>           One agent (see `gx agent-sync agents`), default all
  --group <group>        Run only these groups (repeatable); default: every group
                         that is on by default
  --prune-lock           Also run the prune-lock group
  --yes                  Required by apply
  --json                 Print JSON
"
    .to_string()
}

fn wants_help(args: &[String]) -> bool {
    args.is_empty() || matches!(args[0].as_str(), "help" | "-h" | "--help")
}

fn home_dir() -> CliResult<std::path::PathBuf> {
    let paths = get_gxserver_paths(None);
    let home = paths.agent_config_home_dir().to_path_buf();
    if home.as_os_str().is_empty() {
        return Err(CliError::Other(
            "Could not determine the home folder.".to_string(),
        ));
    }
    Ok(home)
}

fn parse_groups(args: &[String], flags_prune: bool) -> CliResult<Vec<PlanGroupKind>> {
    let mut groups: Vec<PlanGroupKind> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--group" {
            if let Some(value) = args.get(index + 1) {
                let kind = PlanGroupKind::parse(value)
                    .ok_or_else(|| CliError::Other(format!("Unknown agent-sync group: {value}")))?;
                if !groups.contains(&kind) {
                    groups.push(kind);
                }
                index += 2;
                continue;
            }
        }
        if let Some(value) = args[index].strip_prefix("--group=") {
            let kind = PlanGroupKind::parse(value)
                .ok_or_else(|| CliError::Other(format!("Unknown agent-sync group: {value}")))?;
            if !groups.contains(&kind) {
                groups.push(kind);
            }
        }
        index += 1;
    }
    if groups.is_empty() {
        groups = PlanGroupKind::ALL
            .iter()
            .copied()
            .filter(|kind| *kind != PlanGroupKind::PruneLock)
            .collect();
    }
    if flags_prune && !groups.contains(&PlanGroupKind::PruneLock) {
        groups.push(PlanGroupKind::PruneLock);
    }
    Ok(groups)
}

pub fn agent_sync_command(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        println!("{}", agent_sync_usage());
        return Ok(());
    }
    let verb = args[0].as_str();
    let rest: Vec<String> = args[1..].to_vec();
    let parsed = parse_args(&rest);
    let json = parsed.flags.truthy("json");
    let home = home_dir()?;
    let scope = SyncScope::parse(parsed.flags.text("agent").as_deref());
    match verb {
        "status" => {
            let report = scan(&home);
            if json {
                print_json(
                    &serde_json::to_value(&report)
                        .map_err(|error| CliError::Other(error.to_string()))?,
                );
            } else {
                print_status(&report);
            }
            Ok(())
        }
        "agents" => {
            let report = scan(&home);
            if json {
                let agents: Vec<Value> = report
                    .agents
                    .iter()
                    .map(|agent| {
                        json!({
                            "id": agent.id,
                            "displayName": agent.display_name,
                            "detected": agent.detected,
                            "status": agent.status,
                            "root": agent.root,
                        })
                    })
                    .collect();
                print_json(&Value::Array(agents));
            } else {
                for agent in report.agents.iter().filter(|agent| agent.detected) {
                    println!(
                        "{:<28} {:<10} {}",
                        agent.id,
                        status_label(agent.status),
                        agent.root
                    );
                }
            }
            Ok(())
        }
        "plan" => {
            let report = scan(&home);
            let options = PlanOptions {
                scope,
                ..PlanOptions::default()
            };
            let plan = build_plan(&home, &report, &options);
            let groups = parse_groups(&rest, parsed.flags.truthy("pruneLock"))?;
            if json {
                print_json(
                    &serde_json::to_value(&plan)
                        .map_err(|error| CliError::Other(error.to_string()))?,
                );
            } else {
                print_plan(&plan, &groups);
            }
            Ok(())
        }
        "apply" => {
            let groups = parse_groups(&rest, parsed.flags.truthy("pruneLock"))?;
            if !parsed.flags.truthy("yes") {
                let report = scan(&home);
                let options = PlanOptions {
                    scope,
                    ..PlanOptions::default()
                };
                let plan = build_plan(&home, &report, &options);
                print_plan(&plan, &groups);
                return Err(CliError::Other(
                    "Nothing was changed. Re-run with --yes to apply this plan.".to_string(),
                ));
            }
            let options = PlanOptions {
                scope,
                ..PlanOptions::default()
            };
            let result = apply(&home, &options, &groups);
            if json {
                print_json(
                    &serde_json::to_value(&result)
                        .map_err(|error| CliError::Other(error.to_string()))?,
                );
            } else {
                println!(
                    "Applied {} operation(s), {} failed, {} already correct.",
                    result.done.len(),
                    result.failed.len(),
                    result.skipped_keeps
                );
                for failure in &result.failed {
                    println!(
                        "  FAILED {:<7} {} ({})",
                        verb_label(failure.op.verb),
                        failure.op.path,
                        failure.error
                    );
                }
            }
            if result.failed.is_empty() {
                Ok(())
            } else {
                Err(CliError::Other(format!(
                    "{} operation(s) failed.",
                    result.failed.len()
                )))
            }
        }
        other => Err(CliError::Other(format!(
            "Unknown agent-sync command: {other}\n\n{}",
            agent_sync_usage()
        ))),
    }
}

fn status_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Linked => "linked",
        AgentStatus::Attention => "attention",
        AgentStatus::NotInstalled => "missing",
    }
}

fn verb_label(verb: PlanVerb) -> &'static str {
    match verb {
        PlanVerb::Link => "link",
        PlanVerb::Write => "write",
        PlanVerb::Backup => "backup",
        PlanVerb::Unlink => "unlink",
        PlanVerb::Keep => "keep",
        PlanVerb::Mkdir => "mkdir",
        PlanVerb::Drop => "drop",
    }
}

fn print_status(report: &SyncReport) {
    let source = &report.source;
    println!(
        "Source {}: {} skills, {} md files, {} hook scripts, lock {} entries, git {:?}",
        source.path,
        source.skills.len(),
        source.md_files.len(),
        source.hook_script_count,
        source.lock.entry_count,
        source.git
    );
    let summary = &report.summary;
    println!(
        "Agents: {} detected, {} linked, {} need attention. Dangling links {}, copied folders {}, whole-folder links {}, missing pointers {}, stale lock entries {}, unneeded links {}, backup folders {}.",
        summary.agents_detected,
        summary.agents_linked,
        summary.agents_attention,
        summary.dangling_links,
        summary.copied_skill_folders,
        summary.whole_folder_links,
        summary.missing_pointers,
        summary.stale_lock_entries,
        summary.redundant_links,
        summary.backup_folders
    );
    println!();
    for agent in report.agents.iter().filter(|agent| agent.detected) {
        let skills = agent
            .skills
            .as_ref()
            .map(|skills| {
                let c = &skills.counts;
                format!(
                    "skills {:?} linked {} dangling {} copies {} only-here {} missing {}",
                    skills.dir_state,
                    c.linked + c.via_whole_folder,
                    c.dangling,
                    c.copies_identical + c.copies_drifted,
                    c.only_here,
                    c.missing
                )
            })
            .unwrap_or_else(|| "skills n/a".to_string());
        let instructions = agent
            .instructions
            .as_ref()
            .map(|item| format!("md {:?}", item.state))
            .unwrap_or_else(|| "md n/a".to_string());
        let hooks = agent
            .hooks
            .as_ref()
            .map(|item| format!("hooks {:?}", item.state))
            .unwrap_or_else(|| "hooks n/a".to_string());
        println!(
            "{:<28} {:<10} {} | {} | {}",
            agent.id,
            status_label(agent.status),
            skills,
            instructions,
            hooks
        );
    }
    if !report.problems.is_empty() {
        println!();
        for problem in &report.problems {
            println!("- {}", problem.title);
        }
    }
}

fn print_plan(plan: &SyncPlan, enabled: &[PlanGroupKind]) {
    println!(
        "Plan ({}): {} links, {} writes, {} backups, {} unlinks, {} lock drops; backups go to {}",
        plan.scope,
        plan.summary.links,
        plan.summary.writes,
        plan.summary.backups,
        plan.summary.unlinks,
        plan.summary.drops,
        plan.backup_dir
    );
    for group in &plan.groups {
        let on = enabled.contains(&group.kind);
        println!();
        println!(
            "[{}] {} ({} change{})",
            if on { "x" } else { " " },
            group.title,
            group.change_count,
            if group.change_count == 1 { "" } else { "s" }
        );
        for op in &group.ops {
            if op.verb == PlanVerb::Keep {
                continue;
            }
            let target = match (&op.target, op.verb) {
                (Some(target), PlanVerb::Link) => format!(" -> {target}"),
                (Some(target), PlanVerb::Backup) => format!(" -> {target}"),
                (Some(target), PlanVerb::Drop) => format!(" {target}"),
                _ => String::new(),
            };
            let note = op
                .note
                .as_ref()
                .map(|note| format!("  ({note})"))
                .unwrap_or_default();
            println!("  {:<7} {}{}{}", verb_label(op.verb), op.path, target, note);
        }
    }
}
