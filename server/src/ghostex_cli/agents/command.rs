use super::{arguments, delivery, identity, leads, lifecycle, main_coordinator};
use crate::ghostex_cli::{output::print_json, rpc::CliResult, sessions};
use serde_json::json;

/// How to work with other agents in Ghostex (guide.md), which `ghostex agents guide` prints.
///
/// CDXC:AgentSkills 2026-11-20 DECISION:
/// User (Q4262 "B"): Ghostex's own skills are retired for CLI help. The `ghostex-agents` skill's habits (Copy Details, other computers, stuck sessions, reading another thread, renaming, the final report) live here now, with the title rules of the retired `ghostex-auto-rename-session` and the Codex fork steps of `ghostex-move-codex-session`; `ghostex agents --help` keeps the verbs.
pub(crate) const AGENTS_GUIDE_MD: &str = include_str!("guide.md");

/// `ghostex agents guide leads`: lead links, read only when the user asks for a lead.
///
/// CDXC:LeadLinks 2026-11-20 DECISION:
/// User, building #678: "I want to avoid agents using the lead things unless the user really asks
/// for this, and agents shouldn't have unneeded info/context about this." So lead links are taught
/// only here and in `ghostex agents link --help`, never in the always-read `ghostex agent-guide`
/// or the `ghostex` skill; `ghostex agents guide` names the topic in one line and `agents --help`
/// in one usage line.
pub(crate) const AGENTS_GUIDE_LEADS_MD: &str = include_str!("guide_leads.md");

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args.first().map(String::as_str) == Some("guide") {
        match args.get(1).map(String::as_str) {
            Some("leads") => print!("{AGENTS_GUIDE_LEADS_MD}"),
            _ => print!("{AGENTS_GUIDE_MD}"),
        }
        return Ok(());
    }
    let args = arguments::parse(args)?;
    if args.help {
        if matches!(args.command.as_str(), "link" | "unlink" | "links") {
            print!("{}", include_str!("help_link.txt"));
        } else {
            print!("{}", include_str!("help.txt"));
        }
        return Ok(());
    }
    match args.command.as_str() {
        "whoami" => {
            let row = identity::summary(&identity::caller()?);
            if args.json {
                print_json(&row);
            } else {
                for (label, key) in [
                    ("Session", "title"),
                    ("Session ID", "sessionId"),
                    ("Agent", "agentName"),
                    ("Agent ID", "agentId"),
                    ("Agent Session ID", "agentSessionId"),
                    ("Project", "projectName"),
                    ("Project ID", "projectId"),
                    ("Project path", "projectPath"),
                    ("Reply to", "globalRef"),
                ] {
                    println!("{label}: {}", identity::text(&row, key));
                }
            }
        }
        "list" => {
            let caller = if args.all {
                None
            } else {
                Some(identity::caller()?)
            };
            let flags = if let Some(caller) = &caller {
                identity::inventory_flags(&args.flags, identity::text(caller, "globalRef"))?
            } else {
                args.flags.clone()
            };
            let mut inventory = sessions::fetch_session_list(&flags, false)?;
            identity::resolve_names(&mut inventory, &flags);
            let rows: Vec<_> = inventory
                .iter()
                .filter(|row| {
                    identity::is_agent(row)
                        && caller
                            .as_ref()
                            .is_none_or(|caller| row["projectId"] == caller["projectId"])
                })
                .map(identity::summary)
                .collect();
            if args.json {
                print_json(&json!({"ok": true, "sessions": rows}));
            } else {
                println!("REFERENCE\tAGENT\tACTIVITY\tLIFECYCLE\tPROJECT\tTITLE");
                for row in rows {
                    println!(
                        "{}",
                        [
                            "globalRef",
                            "agentName",
                            "activity",
                            "lifecycleState",
                            "projectName",
                            "title"
                        ]
                        .map(|key| identity::text(&row, key).replace(['\n', '\r', '\t'], " "))
                        .join("\t")
                    );
                }
            }
        }
        // CDXC:AgentMessaging 2026-11-13 DECISION: PR #580 made `agents send` print one short line
        // (`--json` for the full answer); the user had that part taken out on merge ("I don't get
        // why he added this"), so send prints its full JSON answer like create and close.
        "send" if args.main_coordinator => {
            print_json(&main_coordinator::send(&args)?);
        }
        "send" => {
            print_json(&delivery::send(&args)?);
        }
        "held" => {
            print_json(&delivery::held(&args)?);
        }
        "unhold" => {
            print_json(&delivery::unhold(&args)?);
        }
        "create" => {
            print_json(&lifecycle::create(&args)?);
        }
        "close" => {
            print_json(&lifecycle::close(&args)?);
        }
        "link" => {
            print_json(&leads::link(&args)?);
        }
        "unlink" => {
            print_json(&leads::unlink(&args)?);
        }
        "links" => {
            let result = leads::links(&args)?;
            if args.json {
                print_json(&result);
            } else {
                leads::print_links(&result);
            }
        }
        "switch-account" => {
            let result = lifecycle::switch_account(&args)?;
            if args.json {
                print_json(&result);
            } else {
                let account = &result["account"];
                let target = format!(
                    "account {} ({})",
                    identity::text(account, "number"),
                    identity::text(account, "email")
                );
                let reference = identity::text(&result["session"], "globalRef");
                // The agent would not exit to its shell, so the switch restarted the session like
                // Full Reload (accounts/restart.rs).
                let restarted = if result["reloaded"] == true {
                    "its agent did not exit, so the session was reloaded on the same conversation"
                } else {
                    "its agent restarted on the same conversation"
                };
                println!(
                    "{}",
                    match identity::text(&result, "status") {
                        "unchanged" => format!("{reference} already runs on {target}."),
                        "switched" if result["forced"] == true => format!(
                            "{reference} now runs on {target}; {restarted}{}, and \".\" was sent so it carries on.",
                            if result["interruptedTurn"] == true {
                                " (its running turn was interrupted)"
                            } else {
                                ""
                            }
                        ),
                        "switched" => format!(
                            "{reference} now runs on {target}; {restarted}."
                        ),
                        "switchedWhileAsleep" => format!(
                            "{reference} is asleep; it starts on {target} the next time it wakes (a message to it wakes it)."
                        ),
                        "switching" => format!(
                            "{reference} is still restarting on {target}. Check it with ghostex read-session-chat {reference} --last 2 --format text."
                        ),
                        _ => format!(
                            "The switch of {reference} to {target} failed: {}",
                            identity::text(&result, "error")
                        ),
                    }
                );
            }
            if result["ok"] != json!(true) {
                crate::ghostex_cli::set_exit_code(1);
            }
        }
        "reload" => {
            let result = lifecycle::reload(&args)?;
            if args.json {
                print_json(&result);
            } else {
                let reference = identity::text(&result["session"], "globalRef");
                println!("{reference} reloaded: its agent was stopped and started again on the same conversation.");
            }
        }
        "types" => {
            let result = lifecycle::types(&args)?;
            if args.json {
                print_json(&result);
            } else {
                println!("AGENT ID\tNAME\tCOMMAND");
                if let Some(rows) = result["agents"].as_array() {
                    for row in rows {
                        println!("{}", ["agentId", "name", "command"].map(|key| identity::text(row, key).replace(['\n', '\r', '\t'], " ")).join("\t"));
                    }
                }
            }
        }
        _ => unreachable!("validated command"),
    }
    Ok(())
}
