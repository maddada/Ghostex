use std::collections::HashSet;

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{CliError, CliResult},
    usage::ports_usage,
};
use crate::portless::listener_discovery::{read_all_tcp_listeners, TcpListenerDetail};

/*
CDXC:Resources 2026-09-02:
The mobile Web preview picker runs `ghostex ports --json` over SSH and forwards
the port the user taps, so this verb lists every listening TCP socket on the
machine, not only the ones a Ghostex session started. It reads the same
lsof/ss output Portless parses, so the two surfaces can never disagree about
which ports exist.
*/
pub fn ports_command(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| arg == "help" || arg == "-h" || arg == "--help")
    {
        println!("{}", ports_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    if let Some(argument) = parsed.rest.first() {
        return Err(CliError::Other(format!(
            "Unknown ports argument: {argument}\n\n{}",
            ports_usage()
        )));
    }

    let listeners = read_all_tcp_listeners()
        .map_err(|error| CliError::Other(format!("{error:#}")))
        .map(dedupe_listeners)?;
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ports": listeners.iter().map(listener_json).collect::<Vec<Value>>(),
        }));
        return Ok(());
    }
    print_listener_table(&listeners);
    Ok(())
}

/// One row per (port, address): a process listening on both IPv4 and IPv6, or
/// two processes sharing a port through SO_REUSEPORT, is still one thing the
/// phone can forward per address.
fn dedupe_listeners(listeners: Vec<TcpListenerDetail>) -> Vec<TcpListenerDetail> {
    let mut seen = HashSet::new();
    let mut unique = listeners
        .into_iter()
        .filter(|listener| seen.insert((listener.port, listener.address.clone())))
        .collect::<Vec<_>>();
    unique.sort_by(|left, right| {
        left.port
            .cmp(&right.port)
            .then_with(|| left.address.cmp(&right.address))
    });
    unique
}

fn listener_json(listener: &TcpListenerDetail) -> Value {
    json!({
        "address": listener.address,
        "command": listener.command,
        "pid": listener.pid,
        "port": listener.port,
    })
}

fn print_listener_table(listeners: &[TcpListenerDetail]) {
    if listeners.is_empty() {
        println!("No TCP ports are listening on this machine.");
        return;
    }
    let rows = listeners
        .iter()
        .map(|listener| {
            [
                listener.port.to_string(),
                listener.address.clone(),
                listener
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                listener.command.clone().unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    let headers = ["PORT", "ADDRESS", "PID", "COMMAND"];
    let widths = headers
        .iter()
        .enumerate()
        .map(|(column, header)| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
                .max(header.chars().count())
        })
        .collect::<Vec<_>>();

    println!("{}", format_row(&headers.map(String::from), &widths));
    for row in &rows {
        println!("{}", format_row(row, &widths));
    }
}

fn format_row(cells: &[String; 4], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(column, cell)| {
            if column + 1 == cells.len() {
                cell.clone()
            } else {
                format!("{cell:<width$}", width = widths[column])
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}
