//! Taking a renderer command from the store's socket and answering it.
//!
//! CDXC:CefRuntime 2026-09-25 WHY:
//! gxserver dispatches a CLI renderer command to the FIRST open socket that subscribed with
//! `rendererCommands: true` (server/src/events.rs). Until this port that was the QuickJS runtime's
//! presentation socket; now it is the store's gx-client socket (host.rs turns the flag on for the
//! local client only), and the runtime's socket stopped asking in the same commit, so a command is
//! never answered twice and never reaches nobody. Each command is performed on the main thread with
//! the window at hand, in the order the daemon sent them, and every one gets an answer: a command
//! the app cannot perform is answered with its error rather than left to time out.
//! SEE-ALSO: packages/gx-client/src/worker.rs, packages/gx-core/src/renderer_commands/,
//! server/src/server/mod.rs (`RENDERER_COMMAND_ACTIONS`).

use ghostex_gx_core::RendererCommandError;
use ghostex_gx_core::protocol::RendererCommand;
use serde_json::Value;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Performs the commands one pump delivered, each in its own main-thread turn so it can use
    /// the window, in the order they arrived.
    pub(in crate::app::gx_store) fn gx_store_take_renderer_commands(
        &mut self,
        commands: Vec<RendererCommand>,
        cx: &mut gpui::Context<Self>,
    ) {
        for command in commands {
            cx.spawn(async move |this, cx| {
                let command_id = command.command_id.clone();
                let performed = this.update_in(cx, |this, window, cx| {
                    let result = this.gx_store_perform_renderer_command(&command, window, cx);
                    this.gx_store_answer_renderer_command(&command, result);
                });
                if performed.is_err() {
                    // No window to perform it in: say so instead of letting the CLI time out.
                    let _ = this.update(cx, |this, _| {
                        this.gx_store_answer_renderer_command_id(
                            &command_id,
                            Err(RendererCommandError::BridgeUnavailable),
                        );
                    });
                }
            })
            .detach();
        }
    }

    fn gx_store_answer_renderer_command(
        &mut self,
        command: &RendererCommand,
        result: Result<Value, RendererCommandError>,
    ) {
        self.gx_store_answer_renderer_command_id(&command.command_id, result);
    }

    fn gx_store_answer_renderer_command_id(
        &mut self,
        command_id: &str,
        result: Result<Value, RendererCommandError>,
    ) {
        let Some(client) = &self.gx_store.client else {
            return;
        };
        client.answer_renderer_command(
            command_id,
            result.map_err(|error| error.message().to_string()),
        );
    }
}
