use serde_json::{json, Value};

use crate::ghostex_cli::{
    actions::send_gxserver_cli_action,
    args::parse_args,
    output::print_json,
    rpc::{CliError, CliResult},
    usage::resources_usage,
};

/*
CDXC:Resources 2026-09-04 WHY:
`ghostex resources` prints the desktop Resources panel so RAM/CPU figures can
be audited from a terminal instead of screenshots. It does not sample anything
itself: the snapshot is produced by the running desktop app through the
`readResourcesSnapshot` renderer command, so every number here is the number
the panel would draw at the same instant. Formatting below mirrors the panel's
compact CPU/RAM chips so the two can be compared line by line.
SEE-ALSO: apps/desktop/src/app/titlebar/resources_snapshot_export.rs,
apps/desktop/sidebar/gxserver-runtime/resources-snapshot.ts.
*/
pub fn resources_command(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| arg == "help" || arg == "-h" || arg == "--help")
    {
        println!("{}", resources_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    if let Some(argument) = parsed.rest.first() {
        return Err(CliError::Other(format!(
            "Unknown resources argument: {argument}\n\n{}",
            resources_usage()
        )));
    }
    let snapshot = send_gxserver_cli_action("readResourcesSnapshot", &json!({}), &parsed.flags)?;
    if parsed.flags.truthy("json") {
        print_json(&snapshot);
        return Ok(());
    }
    print_snapshot(&snapshot);
    Ok(())
}

fn print_snapshot(snapshot: &Value) {
    let header = snapshot.get("header").cloned().unwrap_or(Value::Null);
    println!(
        "Resources  sampled {}  memory metric: {}",
        text(snapshot, "sampledAt"),
        text(snapshot, "memoryMetric")
    );
    println!(
        "CPU {}  RAM {}  sleep inactive: {}  sleep all: {}",
        format_cpu(number(&header, "cpu")),
        format_memory(number(&header, "memoryMb")),
        integer(&header, "inactiveTerminalSleepCount"),
        integer(&header, "sleepAllSessionCount"),
    );

    let sections = snapshot
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if sections.is_empty() {
        println!("\nNo grouped sessions matched running processes.");
    }
    for section in &sections {
        println!(
            "\n{}  CPU {}  RAM {}  {} rows",
            text(section, "label"),
            format_cpu(number(section, "cpu")),
            format_memory(number(section, "memoryMb")),
            integer(section, "rowCount"),
        );
        let rows = section
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for row in &rows {
            println!(
                "  {:<48} {:<40} CPU {:>4}  RAM {:>8}  pids {}{}",
                truncate(&text(row, "label"), 48),
                truncate(&text(row, "detail"), 40),
                format_cpu(number(row, "cpu")),
                format_memory(number(row, "memoryMb")),
                row.get("pids")
                    .and_then(Value::as_array)
                    .map(|pids| pids.len())
                    .unwrap_or(0),
                if row.get("sleepCandidate") == Some(&Value::Bool(true)) {
                    "  [idle]"
                } else {
                    ""
                },
            );
            let children = row
                .get("children")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for child in &children {
                println!(
                    "      {:<36} pid {:>6}  CPU {:>4}  RAM {:>8}",
                    truncate(&text(child, "label"), 36),
                    integer(child, "pid"),
                    format_cpu(number(child, "cpu")),
                    format_memory(number(child, "memoryMb")),
                );
            }
        }
    }

    let processes = snapshot
        .get("processes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!(
        "\nPROCESSES ({} sampled pids behind the rows above)",
        processes.len()
    );
    println!(
        "{:>7} {:>7} {:>6} {:>10}  NAME",
        "PID", "PPID", "CPU", "RAM"
    );
    for process in &processes {
        println!(
            "{:>7} {:>7} {:>6} {:>10}  {}",
            integer(process, "pid"),
            integer(process, "ppid"),
            format_cpu(number(process, "cpu")),
            format_memory(number(process, "memoryMb")),
            text(process, "name"),
        );
    }
}

/// Same rounding as the panel's compact CPU chip.
fn format_cpu(cpu: f64) -> String {
    format!("{:.0}%", cpu.max(0.0).trunc())
}

/// Same rounding as the panel's compact RAM chip.
fn format_memory(memory_mb: f64) -> String {
    let memory_mb = memory_mb.max(0.0);
    if memory_mb >= 1024.0 {
        let gb = (memory_mb / 1024.0 * 10.0).round() / 10.0;
        if gb.fract() == 0.0 {
            format!("{gb:.0} GB")
        } else {
            format!("{gb:.1} GB")
        }
    } else {
        format!("{memory_mb:.0} MB")
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn integer(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Null) | None => "-".to_string(),
        Some(other) => other.to_string(),
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}
