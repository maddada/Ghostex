use super::{arguments, delivery, identity, lifecycle, main_coordinator};
use crate::ghostex_cli::{output::print_json, rpc::CliResult, sessions};
use serde_json::json;

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let args = arguments::parse(args)?;
    if args.help {
        print!("{}", include_str!("help.txt"));
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
        "send" => {
            let result = if args.main_coordinator {
                main_coordinator::send(&args)?
            } else {
                delivery::send(&args)?
            };
            match delivery::send_answer_line(&result).filter(|_| !args.json) {
                Some(line) => println!("{line}"),
                None => print_json(&result),
            }
        }
        "create" => {
            print_json(&lifecycle::create(&args)?);
        }
        "close" => {
            print_json(&lifecycle::close(&args)?);
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
