//! `--computer <name>` and session refs of another computer: requests go to this computer's
//! gxserver, which relays them over the connection the desktop app already has to that computer.

use serde_json::{json, Value};

use super::args::Flags;
use super::rpc::{request_gxserver_rpc, CliError, CliResult, Target};

/// `ghostex computers [--json]`: the computers this one's agents can reach.
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{}", HELP);
        return Ok(());
    }
    let json = args.iter().any(|arg| arg == "--json");
    let local = super::rpc::resolve_local_gxserver_target()?;
    let computers = connected_computers(&local);
    if json {
        super::output::print_json(&json!({
            "ok": true,
            "computers": computers
                .iter()
                .map(|computer| json!({
                    "name": computer.name,
                    "serverId": computer.server_id,
                    "agentAccess": computer.agent_access,
                }))
                .collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    if computers.is_empty() {
        println!("No computer is connected to this one. Connect one in Settings > Remote > Remote machines.");
        return Ok(());
    }
    println!("NAME\tSERVER ID\tAGENT ACCESS");
    for computer in &computers {
        println!(
            "{}\t{}\t{}",
            computer.name,
            computer.server_id,
            if computer.agent_access { "on" } else { "off" }
        );
    }
    Ok(())
}

const HELP: &str = "Usage: ghostex computers [--json]

Lists the computers this one is connected to (Settings > Remote > Remote machines on either
computer, while the desktop app keeps the connection). Agents on this computer can start, message
and read sessions on each one, and its agents can do the same here, unless its Agent access switch
is off.

  ghostex sessions --computer <name>                     Its sessions and projects (with ids)
  ghostex agents create <agent> --computer <name> --project-id <id> --task \"...\"
  ghostex agents send <ref> \"...\"                       A ref from that computer goes there by itself
  ghostex read-session-chat <ref> --last 2 --format text
  ghostex coordinator start-thread --computer <name> --title \"...\" --task \"...\" [--project-id <id>]
  ghostex coordinator promote <ref> --computer <name>   Make a session there a coordinator

--computer takes the computer's name (or the start of it) or its server id. Both computers need
Ghostex 10.44 or later; agents there can start, message, read, sleep, wake and close sessions,
and make one a coordinator, nothing else.";

/// A computer this one is connected to, as `listConnectedComputers` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectedComputer {
    pub(crate) server_id: String,
    pub(crate) name: String,
    pub(crate) agent_access: bool,
}

/// Every computer the local gxserver can relay to. An older gxserver without the endpoint has none.
pub(crate) fn connected_computers(local: &Target) -> Vec<ConnectedComputer> {
    let Ok(answer) = request_gxserver_rpc(
        local,
        "/api/listConnectedComputers",
        &json!({}),
        &Flags::default(),
    ) else {
        return Vec::new();
    };
    answer
        .get("computers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|computer| {
            Some(ConnectedComputer {
                server_id: computer.get("serverId")?.as_str()?.to_string(),
                name: computer
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                agent_access: computer
                    .get("agentAccess")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            })
        })
        .collect()
}

/// The target that relays to `server_id` through the local gxserver.
pub(crate) fn relay_target(local: &Target, server_id: &str) -> Target {
    Target {
        base_url: crate::connected_computers::relay_base_url(&local.base_url, server_id),
        forward_plan: None,
        kind: "computer".to_string(),
        profile_id: None,
        server_id: Some(server_id.to_string()),
        tailcat_plan: None,
        token: local.token.clone(),
    }
}

/// Picks the computer `--computer` names: its server id, or its name (exactly, then the one
/// whose name starts with or contains it). `None` means this computer.
pub(crate) fn pick_computer<'a>(
    computers: &'a [ConnectedComputer],
    wanted: &str,
    this_server_id: Option<&str>,
) -> CliResult<Option<&'a ConnectedComputer>> {
    let wanted = wanted.trim();
    if matches!(wanted, "local" | "this") || Some(wanted) == this_server_id {
        return Ok(None);
    }
    if let Some(computer) = computers.iter().find(|c| c.server_id == wanted) {
        return Ok(Some(computer));
    }
    let lower = wanted.to_lowercase();
    let exact: Vec<_> = computers
        .iter()
        .filter(|c| c.name.to_lowercase() == lower)
        .collect();
    let matches = if exact.is_empty() {
        let starts: Vec<_> = computers
            .iter()
            .filter(|c| c.name.to_lowercase().starts_with(&lower))
            .collect();
        if starts.is_empty() {
            computers
                .iter()
                .filter(|c| c.name.to_lowercase().contains(&lower))
                .collect()
        } else {
            starts
        }
    } else {
        exact
    };
    match matches.as_slice() {
        [one] => Ok(Some(one)),
        [] => Err(CliError::Other(if computers.is_empty() {
            "No computer is connected to this one. Connect one in Settings > Remote > Remote machines, and open it in the sidebar.".to_string()
        } else {
            format!(
                "No connected computer is called \"{wanted}\". Connected: {}.",
                describe(computers)
            )
        })),
        several => Err(CliError::Other(format!(
            "\"{wanted}\" matches more than one computer: {}. Use the full name or its server id.",
            several
                .iter()
                .map(|c| format!("{} ({})", c.name, c.server_id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn describe(computers: &[ConnectedComputer]) -> String {
    computers
        .iter()
        .map(|c| format!("{} ({})", c.name, c.server_id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The target for `--computer <wanted>`, or `None` when it names this computer.
pub(crate) fn computer_flag_target(local: &Target, wanted: &str) -> CliResult<Option<Target>> {
    let this_server_id = super::rpc::local_gxserver_server_id();
    let computers = connected_computers(local);
    let Some(computer) = pick_computer(&computers, wanted, this_server_id.as_deref())? else {
        return Ok(None);
    };
    require_agent_access(computer)?;
    Ok(Some(relay_target(local, &computer.server_id)))
}

/// A computer whose Agent access switch is off is refused here, plainly, before a verb looks for
/// a session there and finds none.
fn require_agent_access(computer: &ConnectedComputer) -> CliResult<()> {
    if computer.agent_access {
        return Ok(());
    }
    Err(CliError::Other(format!(
        "Agent access between this computer and {name} is off, so agents can't use its sessions. It is turned back on with the Agent access switch in {name}'s Edit dialog (Settings > Remote > Remote machines) on the computer that connected to the other.",
        name = computer.name
    )))
}

/// `args` with `--computer <server id>` added when the session they name (what the verb's own
/// `session_selector_from_args` reads: `--session-id`, `--selector`, `--session`,
/// `--session-title`, `--target`, else the first positional) is a ref of a connected computer
/// and nothing already says where to go; `None` leaves them as they are.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// Most session verbs (`read-text`, `send-message`, `send-key`, `wait-for-text`, the session actions) resolve their selector in a session list and then call gxserver with only the session's project and session ids, so a ref of another computer was looked up on this one ("No matching session found"). Saying `--computer` for them before they run sends both the lookup and the call there, for every verb at once. The selector is read with the same parser the verbs use, so a flag's value (`--body-file x`) is never taken for it. Only a ref whose server is not this computer's costs the one extra local call that lists the connected computers.
pub(crate) fn args_for_session_on_another_computer(
    name: &str,
    args: &[String],
) -> Option<Vec<String>> {
    // These choose their computer themselves (agents, coordinator) or take no session.
    if matches!(
        name,
        "computers" | "coordinator" | "orchestrator" | "agents" | "server" | "help"
    ) {
        return None;
    }
    let parsed = super::args::parse_args(args);
    if parsed.flags.contains("computer") || parsed.flags.contains("server") {
        return None;
    }
    let reference = super::selector::session_selector_from_args(&parsed.rest, &parsed.flags)?;
    if !super::rpc::is_gxserver_global_session_ref(&reference) {
        return None;
    }
    let server_id = reference.split(':').next()?.to_string();
    if super::rpc::local_gxserver_server_id().as_deref() == Some(server_id.as_str()) {
        return None;
    }
    let local = super::rpc::resolve_local_gxserver_target().ok()?;
    connected_computers(&local)
        .iter()
        .any(|computer| computer.server_id == server_id)
        .then(|| {
            let mut routed = args.to_vec();
            routed.push("--computer".to_string());
            routed.push(server_id);
            routed
        })
}

/// The session ref `args` name, read the way the verbs read it (for the routing test).
#[cfg(test)]
fn named_session(args: &[&str]) -> Option<String> {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let parsed = super::args::parse_args(&args);
    super::selector::session_selector_from_args(&parsed.rest, &parsed.flags)
}

/// The target for a session ref of `server_id` when it is a connected computer.
pub(crate) fn ref_target(local: &Target, server_id: &str) -> CliResult<Option<Target>> {
    let computers = connected_computers(local);
    let Some(computer) = computers
        .iter()
        .find(|computer| computer.server_id == server_id)
    else {
        return Ok(None);
    };
    require_agent_access(computer)?;
    Ok(Some(relay_target(local, server_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn computers() -> Vec<ConnectedComputer> {
        vec![
            ConnectedComputer {
                server_id: "S4k".into(),
                name: "Omarchy".into(),
                agent_access: true,
            },
            ConnectedComputer {
                server_id: "S7a".into(),
                name: "Windows PC".into(),
                agent_access: true,
            },
            ConnectedComputer {
                server_id: "S2b".into(),
                name: "Windows Laptop".into(),
                agent_access: false,
            },
        ]
    }

    #[test]
    fn a_computer_is_named_by_id_name_or_the_start_of_its_name() {
        let computers = computers();
        let pick = |wanted| {
            pick_computer(&computers, wanted, Some("S9l"))
                .unwrap()
                .map(|c| c.server_id.clone())
        };
        assert_eq!(pick("S4k").as_deref(), Some("S4k"));
        assert_eq!(pick("omarchy").as_deref(), Some("S4k"));
        assert_eq!(pick("oma").as_deref(), Some("S4k"));
        assert_eq!(pick("pc").as_deref(), Some("S7a"));
        assert_eq!(pick("local"), None);
        assert_eq!(pick("S9l"), None);
    }

    #[test]
    fn an_unknown_or_ambiguous_computer_says_which_ones_there_are() {
        let computers = computers();
        let error = pick_computer(&computers, "mac", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Omarchy (S4k)"), "{error}");
        let error = pick_computer(&computers, "windows", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("more than one"), "{error}");
        let error = pick_computer(&[], "omarchy", None).unwrap_err().to_string();
        assert!(error.contains("Settings > Remote"), "{error}");
    }

    #[test]
    fn the_session_a_verb_names_is_found_after_flags_that_take_a_value() {
        assert_eq!(
            named_session(&["--body-file", "x.md", "S4k:P1:G2", "--lines", "3"]).as_deref(),
            Some("S4k:P1:G2")
        );
        assert_eq!(
            named_session(&["--lines", "3", "S4k:P1:G2"]).as_deref(),
            Some("S4k:P1:G2")
        );
        assert_eq!(
            named_session(&["hello", "--session-id", "S4k:P1:G2"]).as_deref(),
            Some("S4k:P1:G2")
        );
        assert_eq!(
            named_session(&["S4k:P1:G2", "some text"]).as_deref(),
            Some("S4k:P1:G2")
        );
    }

    #[test]
    fn a_relay_target_goes_to_the_local_gxserver_with_its_own_token() {
        let local = Target {
            base_url: "http://127.0.0.1:58744".into(),
            forward_plan: None,
            kind: "local".into(),
            profile_id: None,
            server_id: None,
            tailcat_plan: None,
            token: "local-token".into(),
        };
        let target = relay_target(&local, "S4k");
        assert_eq!(
            target.base_url,
            "http://127.0.0.1:58744/api/computers/S4k/relay"
        );
        assert_eq!(target.token, "local-token");
        assert!(!super::super::rpc::is_local_gxserver_target(&target));
    }
}
