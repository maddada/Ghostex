/*
CDXC:GPUITerminalModel 2026-07-03:
Smoke/demo binary for the P1b terminal model — the deliverable proving
PTY spawn → vt feed → coalesced wakeups → owned snapshots end to end against
a real shell, including PTY auto-replies (DSR via write_pty), bell/title
hooks, write_input, mid-session resize (TIOCSWINSZ observed via `stty size`),
and exit detection. A printed walkthrough, not a test harness; keep it a
single small file. Run with:

    cargo run --bin terminal-model-smoke
*/

#[path = "../ghostty_vt.rs"]
mod ghostty_vt;
#[path = "../terminal_environment.rs"]
mod terminal_environment;
#[path = "../terminal_model.rs"]
mod terminal_model;

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use terminal_model::{
    TerminalEvent, TerminalEventSink, TerminalModel, TerminalSnapshot, TerminalSpawnConfig,
};

// Script beats: report size, change title, ring bell, force a DSR auto-reply
// (exercises the write_pty hook), echo a line typed via write_input, then
// report size again after the host resized mid-sleep, and exit 7.
const SCRIPT: &str = r#"
stty -echo
printf 'size-1:%s\n' "$(stty size)"
printf '\033]2;smoke-title\007'
printf '\a'
printf '\033[6n'
IFS= read -r -d R reply
printf 'dsr-reply:%s\n' "${reply#*\[}"
IFS= read -r line
printf 'stdin-echo:%s\n' "$line"
sleep 1
printf 'size-2:%s\n' "$(stty size)"
exit 7
"#;

fn print_snapshot(label: &str, snapshot: &TerminalSnapshot) {
    println!(
        "{label}: dirty = {:?}, size = {}x{}, cursor = {:?} (visible: {})",
        snapshot.dirty,
        snapshot.cols,
        snapshot.rows.len(),
        snapshot.cursor,
        snapshot.cursor_visible,
    );
    for (y, row) in snapshot.rows.iter().enumerate() {
        let text = row.text();
        if row.dirty || !text.is_empty() {
            println!(
                "  row {y} [{}] {:?}",
                if row.dirty { "dirty" } else { "clean" },
                text
            );
        }
    }
}

fn snapshot_has_line(snapshot: &TerminalSnapshot, prefix: &str) -> bool {
    snapshot
        .rows
        .iter()
        .any(|row| row.text().starts_with(prefix))
}

fn main() -> anyhow::Result<()> {
    let (event_tx, event_rx) = mpsc::channel::<TerminalEvent>();
    let event_tx = Mutex::new(event_tx);
    let sink: TerminalEventSink = Arc::new(move |event| {
        let _ = event_tx
            .lock()
            .expect("event sender lock poisoned")
            .send(event);
    });

    let mut model = TerminalModel::spawn(
        TerminalSpawnConfig {
            program: "/bin/zsh".into(),
            args: vec!["-c".into(), SCRIPT.into()],
            env: vec![("TERM".into(), "xterm-256color".into())],
            cwd: None,
            cols: 80,
            rows: 12,
            cell_width_px: 8,
            cell_height_px: 16,
            max_scrollback: 10_000,
        },
        sink,
    )?;

    let mut wakeups = 0u32;
    let mut wrote_input = false;
    let mut resized = false;
    let mut exited = None;
    loop {
        // After exit, keep draining briefly so the final output lands.
        let timeout = if exited.is_some() {
            Duration::from_millis(300)
        } else {
            Duration::from_secs(10)
        };
        let event = match event_rx.recv_timeout(timeout) {
            Ok(event) => event,
            Err(_) if exited.is_some() => break,
            Err(_) => anyhow::bail!("timed out waiting for terminal events"),
        };
        match event {
            TerminalEvent::Bell => println!("event: bell"),
            TerminalEvent::TitleChanged => println!("event: title changed"),
            TerminalEvent::ClipboardWriteRequested => {
                // Standalone consumer: report and drop; only the GPUI host
                // performs real clipboard access.
                let count = model.take_clipboard_write_requests().len();
                println!("event: clipboard write requested ({count} pending)");
            }
            TerminalEvent::Exited(exit) => {
                println!("event: exited with {exit:?}");
                exited = Some(exit);
            }
            TerminalEvent::Wakeup => {
                wakeups += 1;
                let snapshot = model.snapshot()?;
                print_snapshot(&format!("wakeup {wakeups}"), &snapshot);
                if !wrote_input && snapshot_has_line(&snapshot, "dsr-reply:") {
                    println!("host: writing input line to pty");
                    model.write_input(b"typed-into-pty\r")?;
                    wrote_input = true;
                }
                if !resized && snapshot_has_line(&snapshot, "stdin-echo:") {
                    println!("host: resizing 80x12 -> 100x16");
                    model.resize(100, 16, 8, 16)?;
                    resized = true;
                }
            }
        }
    }

    // A snapshot taken with no new output since the last one must read clean.
    let final_snapshot = model.snapshot()?;
    print_snapshot("final (expect clean)", &final_snapshot);
    println!(
        "summary: wakeups = {wakeups}, wrote_input = {wrote_input}, resized = {resized}, exit = {:?}",
        model.exit_status(),
    );
    println!("smoke complete");
    Ok(())
}
