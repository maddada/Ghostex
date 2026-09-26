//! Registers as a daemon's renderer-command target and answers every command with its action, so the
//! renderer-command path (CLI -> gxserver -> this socket -> answer -> CLI) can be exercised against an
//! ISOLATED daemon. Never point it at the daemon of a running Ghostex app: the first registered
//! socket gets the CLI's commands.
//!
//! Usage: `cargo run --example renderer_echo -- <base url> <token> [seconds]`.

use std::sync::atomic::AtomicI64;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use ghostex_gx_client::{ClientOutput, GxClient, GxClientConfig};
use ghostex_gx_core::MachineId;
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let base_url = args.next().expect("base url");
    let auth_token = args.next().expect("token");
    let seconds: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(30);
    let (wake_sender, wakes) = mpsc::channel::<()>();
    let client = GxClient::start(
        GxClientConfig {
            machine: MachineId::Local,
            base_url,
            auth_token,
            client_id: "ghostex-gx-client-renderer-echo".to_string(),
            held_revision: Arc::new(AtomicI64::new(0)),
            forward_chat_frames: false,
            renderer_commands: true,
        },
        move || {
            let _ = wake_sender.send(());
        },
    )
    .expect("client starts");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if wakes.recv_timeout(Duration::from_millis(500)).is_err() {
            continue;
        }
        for output in client.drain() {
            if let ClientOutput::RendererCommand(command) = output {
                println!("renderer command: {}", command.action);
                let answer = if command.action == "openSettings" {
                    Err("Invalid settings tab.".to_string())
                } else {
                    Ok(json!({ "ok": true, "echo": command.action }))
                };
                client.answer_renderer_command(&command.command_id, answer);
            }
        }
    }
    let stats = client.stats();
    println!(
        "renderer commands {}, answers {}",
        stats.renderer_commands, stats.renderer_answers
    );
}
