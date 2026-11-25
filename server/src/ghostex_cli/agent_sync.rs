use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::paths::get_gxserver_paths;
use ghostex_agent_sync::{
    apply, build_plan, resolve_seed_source, scan, AgentStatus, PlanGroupKind, PlanOptions,
    PlanVerb, PointerPolicy, SyncPlan, SyncReport, SyncScope,
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
  gx agent-sync plan [--agent <id>] [--group <group>...] [--prune-lock] [--seed-main-from <agent|path>] [--pointer-policy <policy>] [--json]
  gx agent-sync apply --yes [--agent <id>] [--group <group>...] [--prune-lock] [--seed-main-from <agent|path>] [--pointer-policy <policy>] [--json]
  gx agent-sync agents [--json]

What it does:
  ~/.agents is the source of truth: skills/, main.md and the other rule files,
  hooks/, and .skill-lock.json. Sync creates one relative symlink per skill in
  every agent's global skills folder, converts whole-folder links into folders
  of per-skill links, writes the one-line instruction pointer file each agent
  reads, and links the hooks folder and lock file into Claude Code and Codex.
  Pointer files are never written while ~/.agents/main.md is missing or empty;
  pass --seed-main-from <agent|path> (e.g. --seed-main-from claude) to create
  main.md from an existing instruction file as the first step.
  Agents that read ~/.agents/skills themselves (Codex, GitHub Copilot, Pi,
  Cursor, OpenCode, Amp) get no links, and links already in their
  folders are removed so they do not load each skill twice.
  Nothing is deleted: anything in the way of a link or pointer, and any leftover
  <name>.pre-sync-<stamp>.bak folder in a skills folder, is moved to Ghostex's
  state folder (agent-sync-backups/<stamp>/, same path as under HOME), and
  dangling links are removed.

Groups (all on by default except prune-lock):
  seed-main-md           Create ~/.agents/main.md from --seed-main-from
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
  --agent <id>                One agent (see `gx agent-sync agents`), default all
  --group <group>             Run only these groups (repeatable); default: every group
                              that is on by default
  --prune-lock                Also run the prune-lock group
  --seed-main-from <source>   Seed ~/.agents/main.md from an existing instruction file
                              (agent id, alias like `claude`, `richest`, or file path)
  --pointer-policy <policy>   How to handle existing instruction files:
                              replace (default, backup first), prepend (backup first,
                              keep existing lines below pointer), or leave-alone
  --yes                       Required by apply
  --json                      Print JSON
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

fn parse_pointer_policy(value: Option<&str>) -> CliResult<PointerPolicy> {
    match value {
        None => Ok(PointerPolicy::Replace),
        Some(raw) => PointerPolicy::parse(raw).ok_or_else(|| {
            CliError::Other(format!(
                "Unknown --pointer-policy `{raw}`. Use replace, prepend, or leave-alone."
            ))
        }),
    }
}

fn parse_groups(
    args: &[String],
    flags_prune: bool,
    has_seed: bool,
) -> CliResult<Vec<PlanGroupKind>> {
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
    } else if has_seed && !groups.contains(&PlanGroupKind::SeedMainMd) {
        groups.insert(0, PlanGroupKind::SeedMainMd);
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
    let pointer_policy = parse_pointer_policy(parsed.flags.string_value("pointerPolicy"))?;
    let seed_main_from = parsed
        .flags
        .string_value("seedMainFrom")
        .map(str::to_string);
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
            if let Some(selector) = seed_main_from.as_deref() {
                resolve_seed_source(&home, &report, selector).map_err(CliError::Other)?;
            }
            let options = PlanOptions {
                scope,
                pointer_policy,
                seed_main_from: seed_main_from.clone(),
                ..PlanOptions::default()
            };
            let plan = build_plan(&home, &report, &options);
            let groups = parse_groups(
                &rest,
                parsed.flags.truthy("pruneLock"),
                seed_main_from.is_some(),
            )?;
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
            let groups = parse_groups(
                &rest,
                parsed.flags.truthy("pruneLock"),
                seed_main_from.is_some(),
            )?;
            let report = scan(&home);
            if let Some(selector) = seed_main_from.as_deref() {
                resolve_seed_source(&home, &report, selector).map_err(CliError::Other)?;
            }
            let options = PlanOptions {
                scope,
                pointer_policy,
                seed_main_from,
                ..PlanOptions::default()
            };
            if !parsed.flags.truthy("yes") {
                let plan = build_plan(&home, &report, &options);
                print_plan(&plan, &groups);
                return Err(CliError::Other(
                    "Nothing was changed. Re-run with --yes to apply this plan.".to_string(),
                ));
            }
            let result = apply(&home, &options, &groups);
            if json {
                print_json(
                    &serde_json::to_value(&result)
                        .map_err(|error| CliError::Other(error.to_string()))?,
                );
            } else {
                println!(
                    "Applied {}, {} failed, {} already correct.",
                    ghostex_gx_protocol::plural(result.done.len(), "operation", "operations"),
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
                    "{} failed.",
                    ghostex_gx_protocol::plural(result.failed.len(), "operation", "operations")
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
    let main_md_status = if !source.main_md_exists {
        "missing"
    } else if source.main_md_empty {
        "empty"
    } else {
        "ok"
    };
    println!(
        "Source {}: {}, {} (main.md {}), {}, lock {}, git {:?}",
        source.path,
        ghostex_gx_protocol::plural(source.skills.len(), "skill", "skills"),
        ghostex_gx_protocol::plural(source.md_files.len(), "md file", "md files"),
        main_md_status,
        ghostex_gx_protocol::plural(source.hook_script_count, "hook script", "hook scripts"),
        ghostex_gx_protocol::plural(source.lock.entry_count, "entry", "entries"),
        source.git
    );
    let summary = &report.summary;
    println!(
        "Agents: {} detected, {} linked, {}. Dangling links {}, dangling pointers {}, copied folders {}, whole-folder links {}, missing pointers {}, stale lock entries {}, unneeded links {}, backup folders {}.",
        summary.agents_detected,
        summary.agents_linked,
        ghostex_gx_protocol::plural(
            summary.agents_attention,
            "needs attention",
            "need attention"
        ),
        summary.dangling_links,
        summary.dangling_pointers,
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
            if !problem.detail.is_empty() {
                println!("  {}", problem.detail);
            }
        }
    }
}

fn print_plan(plan: &SyncPlan, enabled: &[PlanGroupKind]) {
    println!(
        "Plan ({}, pointer policy {}): {}, {}, {}, {}, {}; backups go to {}",
        plan.scope,
        plan.pointer_policy.as_str(),
        ghostex_gx_protocol::plural(plan.summary.links, "link", "links"),
        ghostex_gx_protocol::plural(plan.summary.writes, "write", "writes"),
        ghostex_gx_protocol::plural(plan.summary.backups, "backup", "backups"),
        ghostex_gx_protocol::plural(plan.summary.unlinks, "unlink", "unlinks"),
        ghostex_gx_protocol::plural(plan.summary.drops, "lock drop", "lock drops"),
        plan.backup_dir
    );
    if !plan.warnings.is_empty() {
        println!();
        for warning in &plan.warnings {
            println!("WARNING: {warning}");
        }
    }
    if plan.main_md_blocked {
        println!();
        if let Some(best) = &plan.seed_recommendation {
            println!(
                "First step: create ~/.agents/main.md from {} ({}, {} bytes):",
                best.display_name, best.path, best.bytes
            );
            println!(
                "  ghostex agent-sync plan --seed-main-from {}",
                best.agent_id
            );
            println!(
                "  ghostex agent-sync apply --yes --seed-main-from {}",
                best.agent_id
            );
            if plan.seed_candidates.len() > 1 {
                let others = plan
                    .seed_candidates
                    .iter()
                    .skip(1)
                    .map(|c| format!("{} ({}, {} bytes)", c.agent_id, c.path, c.bytes))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("  Other seed sources: {others}");
            }
        } else {
            println!(
                "First step: create ~/.agents/main.md with your shared instructions, then re-run Agent Sync."
            );
        }
    }
    for group in &plan.groups {
        if group.kind == PlanGroupKind::SeedMainMd && group.ops.is_empty() {
            continue;
        }
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
