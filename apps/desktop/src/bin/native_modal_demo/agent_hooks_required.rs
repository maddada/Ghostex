//! Preview host for the Install Hooks prompt. States: `claude` (default),
//! `codex` (white logo), `copilot`, `omp` (multicolor logo), `unknown` (no logo).
use super::agent_hooks_required_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let (agent_name, hook_agent_id) = match demo.state.as_str() {
        "codex" => ("Codex", "codex"),
        "copilot" => ("Copilot", "copilot"),
        "omp" => ("OMP", "omp"),
        "unknown" => ("My Agent", "my-agent"),
        _ => ("Claude", "claude"),
    };
    let host: AgentHooksRequiredModalHost = Rc::new(move |command, cx: &mut App| {
        match command {
            AgentHooksRequiredModalCommand::Install => {
                eprintln!("confirmAgentHookLaunch installHooks=true")
            }
            AgentHooksRequiredModalCommand::Skip => {
                eprintln!("confirmAgentHookLaunch installHooks=false")
            }
            AgentHooksRequiredModalCommand::Close => eprintln!("close"),
        }
        cx.quit();
    });
    let config = AgentHooksRequiredModalConfig {
        agent_name: agent_name.to_string(),
        hook_agent_id: hook_agent_id.to_string(),
        palette: demo.palette,
    };
    super::open_modal_window(
        AGENT_HOOKS_REQUIRED_MODAL_WIDTH,
        AGENT_HOOKS_REQUIRED_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiAgentHooksRequiredModalWindow::new(config, host, window, cx))
        },
        cx,
    );
}
