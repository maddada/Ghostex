//! An orchestrator's Memory and Goal & Instructions preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`):
//! default (Memory with more notes than the window holds, so the list scrolls), `few` (three
//! notes), `goal` (Goal & Instructions).
use super::coordinator_details_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

const NOTES: &[&str] = &[
    "Commit message generation models: Codex gpt-6-luna at high reasoning; Claude Sonnet 5.5 at medium effort.",
    "Threads needing the same files run one after another; use a worktree only when running them in parallel really matters.",
    "Thread models: substantial work = Opus 5.5 high; difficult but small-scope work = Opus 5.5 medium; smaller contained work = Sonnet 5.5 high. Never change a running thread's model; start a new thread instead.",
    "Only the coordinator rebuilds or restarts the app; threads never do. Give running threads a heads-up if needed.",
    "Before pushing from the shared folder, pull first and check git log origin/main..main; never push other sessions' local commits without asking.",
    "iOS TestFlight builds go to the external Discord testers and are built from main only.",
    "Release notes are written for customers: no file paths, no internal names.",
    "Ask before deleting anything that is not the thread's own work.",
    "Keep the sidebar's session titles short: five words at most.",
    "The phone and the desktop chat change together unless the user says otherwise.",
    "Screenshots for reviews go in ~/.local/share/ghostex/i/, never in /tmp.",
    "When a test fails on main too, say so instead of fixing it in an unrelated thread.",
];

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let notes: Vec<String> = match demo.state.as_str() {
        "few" => NOTES.iter().take(3).map(|note| note.to_string()).collect(),
        _ => NOTES.iter().map(|note| note.to_string()).collect(),
    };
    let host: CoordinatorDetailsModalHost = Rc::new(|command, cx| {
        match command {
            CoordinatorDetailsModalCommand::SaveGoal { reply, .. } => {
                eprintln!("save goal");
                reply.finish(Ok(()), cx);
            }
            CoordinatorDetailsModalCommand::SaveMemory { notes, reply } => {
                eprintln!("save memory: {} notes", notes.len());
                reply.finish(Ok(()), cx);
            }
            CoordinatorDetailsModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let config = CoordinatorDetailsModalConfig {
        mode: if demo.state == "goal" {
            CoordinatorDetailsMode::Goal
        } else {
            CoordinatorDetailsMode::Memory
        },
        title: "Main Coordinator".to_string(),
        goal: "Ship the Session details panel.".to_string(),
        instructions: "Keep threads small and report back with marked handovers.".to_string(),
        notes,
        palette: demo.palette,
    };
    let _ = super::open_modal_window(
        COORDINATOR_DETAILS_MODAL_WIDTH,
        COORDINATOR_DETAILS_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiCoordinatorDetailsModalWindow::new(config, host, window, cx))
        },
        cx,
    );
}
