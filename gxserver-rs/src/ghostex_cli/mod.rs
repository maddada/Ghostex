pub mod actions;
pub mod args;
pub mod attach;
pub mod automations;
pub mod board;
pub mod browser_mcp;
pub mod diagnostics;
pub mod editors;
pub mod launchers;
pub mod output;
pub mod picker;
pub mod rpc;
pub mod selector;
pub mod sessions;
pub mod skills;
pub mod usage;
pub mod wait;
pub mod web;

use rpc::{CliError, CliResult};

/*
CDXC:GhostexRustCli 2026-07-13:
Rust replacement for scripts/ghostex-cli.mjs. The dispatch table, bare-`ghostex`
desktop launch, VS Code-style bare-path open, help gating, and the JSON error
shape (`{ error, ok: false }` + exit 1 when --json) are preserved verbatim so
every existing consumer (skills, agents, React Native Android automation, remote hosts)
sees identical behavior after the Node CLI deletion.
*/

pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let result = crate::paths::migrate_legacy_storage()
        .map_err(|error| {
            CliError::Other(format!("Could not migrate legacy Ghostex storage: {error}"))
        })
        .and_then(|_| dispatch(&argv));
    rpc::stop_all_gxserver_ssh_tunnels();
    match result {
        Ok(code) => code,
        Err(error) => {
            let message = error.to_string();
            if output::cli_args_want_json(&argv) {
                output::print_json(&serde_json::json!({ "error": message, "ok": false }));
            } else {
                eprintln!("{message}");
            }
            1
        }
    }
}

/// Commands whose own `-h/--help` handling must not be swallowed by the
/// global help gate.
const HELP_GATE_EXCLUDED: &[&str] = &[
    "agent-orchestration",
    "automations",
    "bd",
    "beads",
    "board",
    "browser",
    "browser-use",
    "computer-use",
    "cli",
    "editor-daemon",
    "f",
    "fable-5.6-orchestration",
    "find",
    "find-prev-session",
    "generate-title",
    "h",
    "history",
    "move-codex-session",
    "quick-actions",
    "server",
    "web",
];

fn dispatch(argv: &[String]) -> CliResult<i32> {
    if argv.is_empty() {
        launchers::ghostex_desktop_command()?;
        return Ok(exit_code());
    }
    let command_name = argv[0].as_str();
    let args: Vec<String> = argv[1..].to_vec();
    if command_name == "-h" || command_name == "--help" {
        println!("{}", usage::usage());
        return Ok(0);
    }
    if !is_known_command(command_name) {
        if launchers::is_existing_bare_path_argument(command_name) {
            actions::run_bridge_action(
                "openPaths",
                actions::Parser::OpenPaths,
                actions::BridgeOptions {
                    fail_on_not_ok: true,
                    assert_ok: false,
                },
                argv,
            )?;
            return Ok(exit_code());
        }
        return Err(CliError::Other(format!(
            "Unknown command: {command_name}\n\n{}",
            usage::usage()
        )));
    }
    if automations::is_automation_command(command_name)
        && args.iter().any(|arg| arg == "-h" || arg == "--help")
    {
        println!("{}", usage::automations_usage());
        return Ok(0);
    }
    if !HELP_GATE_EXCLUDED.contains(&command_name)
        && args.iter().any(|arg| arg == "-h" || arg == "--help")
    {
        println!("{}", usage::usage());
        return Ok(0);
    }
    run_command(command_name, &args)?;
    Ok(exit_code())
}

/// The Node CLI signals failure by setting process.exitCode inside a few
/// commands (assert-card, wait-for, failed attach) instead of throwing.
/// Commands record that here.
static EXIT_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn set_exit_code(code: i32) {
    EXIT_CODE.store(code, std::sync::atomic::Ordering::SeqCst);
}

fn exit_code() -> i32 {
    EXIT_CODE.load(std::sync::atomic::Ordering::SeqCst)
}

fn is_known_command(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "sessions",
        "tui",
        "s",
        "list-sessions",
        "ls",
        "find",
        "f",
        "history",
        "h",
        "android-check",
        "attach",
        "a",
        "resume",
        "r",
        "kill",
        "k",
        "sleep",
        "hold-sessions-awake",
        "wake",
        "focus",
        "floating-editor",
        "fe",
        "floating-monaco-editor",
        "fme",
        "editor-daemon",
        "prompt-editor",
        "state",
        "dump-state",
        "open",
        "o",
        "edit",
        "e",
        "terminal",
        "t",
        "create-session",
        "create-chat",
        "create-agent",
        "run-agent",
        "run-command",
        "run-action",
        "quick-actions",
        "click-button",
        "save-command",
        "save-agent",
        "focus-session",
        "acknowledge-session-attention",
        "ack-session-attention",
        "focus-group",
        "switch-project",
        "move-project",
        "add-project",
        "group-project",
        "browse-directories",
        "discover-source-control",
        "lookup-repository",
        "clone-repository",
        "remove-project",
        "restore-recent-project",
        "read-sidebar-project-collections",
        "update-sidebar-project-collections",
        "close-session",
        "restart-session",
        "fork-session",
        "reload-session",
        "rename-session",
        "request-session-rename",
        "sleep-session",
        "tag-session",
        "pin-session",
        "delayed-send",
        "close-after-done",
        "send-text",
        "send-enter",
        "send-key",
        "send-message",
        "message",
        "msg",
        "read-text",
        "read-messages",
        "read-thread",
        "read-session-chat",
        "read-session-chat-skills",
        "read-session-chat-files",
        "send-session-chat-message",
        "answer-session-chat-prompt",
        "interrupt-session-chat",
        "handoff-session-chat-draft",
        "wait-for-text",
        "rename-command",
        "set-visible-count",
        "set-view-mode",
        "open-browser",
        "open-browser-pane",
        "browser",
        "browser-use",
        "browser-devtools-mcp",
        "browser-mcp",
        "bd",
        "beads",
        "board",
        "server",
        "web",
        "install-browser-skill",
        "install-browser-mcp-skill",
        "install-browser-use-skill",
        "computer-use",
        "cli",
        "automations",
        "install-computer-use-skill",
        "agent-orchestration",
        "install-agent-orchestration-skill",
        "fable-5.6-orchestration",
        "install-fable-5.6-orchestration-skill",
        "find-prev-session",
        "install-find-prev-session-skill",
        "generate-title",
        "install-generate-title-skill",
        "move-codex-session",
        "install-move-codex-session-skill",
        "toggle-sidebar",
        "move-sidebar",
        "assert-card",
        "wait-for",
        "screenshot",
        "logs",
        "bundle",
        "help",
    ];
    NAMES.contains(&name) || automations::is_automation_command(name)
}

fn run_command(name: &str, args: &[String]) -> CliResult<()> {
    use actions::{run_bridge_action, run_resolved_session_bridge_action, BridgeOptions, Parser};
    let plain = BridgeOptions {
        fail_on_not_ok: false,
        assert_ok: false,
    };
    let fail_on_not_ok = BridgeOptions {
        fail_on_not_ok: true,
        assert_ok: false,
    };
    let assert_ok = BridgeOptions {
        fail_on_not_ok: false,
        assert_ok: true,
    };
    match name {
        "sessions" | "s" | "list-sessions" | "ls" => sessions::sessions_command(args),
        "tui" => launchers::ghostex_tui_command(args),
        "find" | "f" => launchers::zehn_search_command(args),
        "history" | "h" => launchers::history_command(args),
        "android-check" => diagnostics::android_check_command(args),
        "attach" | "a" | "resume" | "r" => attach::attach_session_command(args),
        "kill" | "k" => {
            wait::session_action_command("closeSession", "killed", &serde_json::json!({}), args)
        }
        "sleep" => wait::session_action_command(
            "sleepSession",
            "slept",
            &serde_json::json!({ "sleeping": true }),
            args,
        ),
        "wake" => wait::session_action_command(
            "sleepSession",
            "woke",
            &serde_json::json!({ "sleeping": false }),
            args,
        ),
        "focus" => wait::focus_smart_session_command(args),
        "floating-editor" | "fe" => editors::floating_editor_command(args),
        "floating-monaco-editor" | "fme" => editors::floating_monaco_editor_command(args),
        "editor-daemon" => editors::editor_daemon_command(args),
        "prompt-editor" => editors::prompt_editor_command(args),
        "state" => run_bridge_action("state", Parser::None, plain, args),
        "dump-state" => run_bridge_action("dumpState", Parser::None, plain, args),
        "open" | "o" => run_bridge_action("openPaths", Parser::OpenPaths, fail_on_not_ok, args),
        "edit" | "e" => run_bridge_action("openPaths", Parser::EditPaths, fail_on_not_ok, args),
        "terminal" | "t" => run_bridge_action(
            "createQuickTerminal",
            Parser::QuickTerminal,
            fail_on_not_ok,
            args,
        ),
        "create-session" => {
            run_bridge_action("createSession", Parser::CreateSession, fail_on_not_ok, args)
        }
        "create-chat" => run_bridge_action(
            "createChatSession",
            Parser::CreateSession,
            fail_on_not_ok,
            args,
        ),
        "create-agent" => run_bridge_action("createAgentSession", Parser::Agent, plain, args),
        "run-agent" => run_bridge_action("runAgent", Parser::Agent, plain, args),
        "run-command" => run_bridge_action("runCommand", Parser::CommandButton, plain, args),
        "run-action" => sessions::run_quick_action_command(args),
        "quick-actions" => quick_actions_command(args),
        "click-button" => run_bridge_action("clickButton", Parser::ClickButton, plain, args),
        "save-command" => {
            run_bridge_action("saveCommand", Parser::SaveCommand, fail_on_not_ok, args)
        }
        "save-agent" => run_bridge_action("saveAgent", Parser::SaveAgent, fail_on_not_ok, args),
        "focus-session" => run_bridge_action("focusSession", Parser::SessionSelector, plain, args),
        "acknowledge-session-attention" | "ack-session-attention" => run_bridge_action(
            "acknowledgeSessionAttention",
            Parser::SessionSelector,
            plain,
            args,
        ),
        "focus-group" => run_bridge_action("focusGroup", Parser::Group, plain, args),
        "switch-project" => run_bridge_action("switchProject", Parser::Project, plain, args),
        "move-project" => {
            run_bridge_action("moveProject", Parser::ProjectMove, fail_on_not_ok, args)
        }
        "add-project" => run_bridge_action("addProject", Parser::ProjectPath, plain, args),
        "group-project" => run_bridge_action(
            "assignProjectToSidebarCollection",
            Parser::ProjectCollection,
            fail_on_not_ok,
            args,
        ),
        /*
        CDXC:AddProjectDialog 2026-07-30:
        The Add Project flow's four gxserver reads/writes are exposed as CLI
        verbs so Ghostex mobile can run the same daemon-owned logic over SSH
        that gpui and the web app reach over the wire protocol.
        */
        "browse-directories" => run_bridge_action(
            "browseDirectories",
            Parser::BrowseDirectories,
            fail_on_not_ok,
            args,
        ),
        "discover-source-control" => {
            run_bridge_action("discoverSourceControl", Parser::None, fail_on_not_ok, args)
        }
        "lookup-repository" => run_bridge_action(
            "lookupRepository",
            Parser::LookupRepository,
            fail_on_not_ok,
            args,
        ),
        "clone-repository" => run_bridge_action(
            "cloneRepository",
            Parser::CloneRepository,
            fail_on_not_ok,
            args,
        ),
        "remove-project" => {
            run_bridge_action("removeProject", Parser::Project, fail_on_not_ok, args)
        }
        "restore-recent-project" => run_bridge_action(
            "restoreRecentProject",
            Parser::Project,
            fail_on_not_ok,
            args,
        ),
        "read-sidebar-project-collections" => run_bridge_action(
            "readSidebarProjectCollections",
            Parser::None,
            fail_on_not_ok,
            args,
        ),
        "update-sidebar-project-collections" => run_bridge_action(
            "updateSidebarProjectCollections",
            Parser::SidebarProjectCollectionsState,
            fail_on_not_ok,
            args,
        ),
        "close-session" => run_bridge_action("closeSession", Parser::SessionSelector, plain, args),
        "restart-session" => {
            run_bridge_action("restartSession", Parser::SessionSelector, plain, args)
        }
        "fork-session" => wait::fork_session_command(args),
        "reload-session" => {
            run_bridge_action("fullReloadSession", Parser::SessionSelector, plain, args)
        }
        "rename-session" => {
            run_bridge_action("renameSession", Parser::Rename, fail_on_not_ok, args)
        }
        /*
        CDXC:MobileAgentActions 2026-08-01:
        Agent-aware rename for clients that only reach gxserver through this
        CLI (Ghostex mobile over SSH). `rename-session` writes the title with
        updateSession; this verb goes through the agent rename request so the
        caller learns `shouldSendAgentRenameCommand` and can stage `/rename`
        into the agent TUI exactly like the desktop and web chat surfaces.
        */
        "request-session-rename" => run_bridge_action(
            "requestSessionRename",
            Parser::RenameRequest,
            fail_on_not_ok,
            args,
        ),
        /*
        CDXC:MobileKeepAwake 2026-08-19:
        Ghostex mobile has no HTTP path to gxserver, so the keep-awake lease is a
        CLI verb it SSH-execs on a timer while its attached tabs are on screen.
        */
        "hold-sessions-awake" => run_bridge_action(
            "holdSessionsAwake",
            Parser::KeepSessionsAwake,
            fail_on_not_ok,
            args,
        ),
        "sleep-session" => run_bridge_action(
            "sleepSession",
            Parser::SessionBoolean("sleeping"),
            plain,
            args,
        ),
        "tag-session" => run_bridge_action("tagSession", Parser::SessionTag, plain, args),
        "pin-session" => {
            run_bridge_action("pinSession", Parser::SessionBoolean("pinned"), plain, args)
        }
        "delayed-send" => {
            // `--cancel` clears the armed automation; otherwise the parser
            // accepts the timer and both agent-completion trigger modes.
            if args.iter().any(|arg| arg == "--cancel") {
                run_bridge_action("cancelDelayedSend", Parser::SessionSelector, plain, args)
            } else {
                run_bridge_action("scheduleDelayedSend", Parser::DelayedSend, plain, args)
            }
        }
        "close-after-done" => {
            run_bridge_action("toggleCloseAfterDone", Parser::SessionSelector, plain, args)
        }
        "send-text" => {
            run_resolved_session_bridge_action("sendText", Parser::SendText, plain, args)
        }
        "send-enter" => {
            run_resolved_session_bridge_action("sendEnter", Parser::SessionSelector, plain, args)
        }
        "send-key" => run_resolved_session_bridge_action("sendKey", Parser::SendKey, plain, args),
        "send-message" | "message" | "msg" => wait::send_message_command(args),
        "read-text" | "read-messages" | "read-thread" => wait::read_session_text_command(args),
        /*
        CDXC:SessionChatMobileCli 2026-07-31:
        Session Chat over SSH for Ghostex mobile: the chat endpoints as CLI
        verbs, mirroring the Add Project pattern. read-session-chat carries
        the --wait-ms/--fingerprint long-poll pair for transcript tailing
        without an /api/events socket.
        */
        "read-session-chat" => {
            run_bridge_action("readSessionChat", Parser::SessionChatRead, plain, args)
        }
        "read-session-chat-skills" => run_bridge_action(
            "readSessionChatSkills",
            Parser::SessionSelector,
            plain,
            args,
        ),
        "read-session-chat-files" => {
            run_bridge_action("readSessionChatFiles", Parser::SessionSelector, plain, args)
        }
        "send-session-chat-message" => run_bridge_action(
            "sendSessionChatMessage",
            Parser::SendText,
            fail_on_not_ok,
            args,
        ),
        "answer-session-chat-prompt" => run_bridge_action(
            "answerSessionChatPrompt",
            Parser::SessionChatAnswer,
            fail_on_not_ok,
            args,
        ),
        "interrupt-session-chat" => {
            run_bridge_action("interruptSessionChat", Parser::SessionSelector, plain, args)
        }
        "handoff-session-chat-draft" => run_bridge_action(
            "handoffSessionChatDraft",
            Parser::SessionSelector,
            fail_on_not_ok,
            args,
        ),
        "wait-for-text" => wait::wait_for_text_command(args),
        "rename-command" => {
            run_resolved_session_bridge_action("renameCommand", Parser::Rename, plain, args)
        }
        "set-visible-count" => {
            run_bridge_action("setVisibleCount", Parser::VisibleCount, plain, args)
        }
        "set-view-mode" => run_bridge_action("setViewMode", Parser::ViewMode, plain, args),
        "open-browser" => run_bridge_action("openBrowser", Parser::Url, plain, args),
        "open-browser-pane" => run_bridge_action("openBrowserPane", Parser::None, plain, args),
        "browser" => browser_mcp::browser_command(args),
        "browser-use" => skills::browser_use_command(args),
        "browser-devtools-mcp" | "browser-mcp" => browser_mcp::browser_devtools_mcp_command(args),
        "bd" | "beads" => launchers::beads_command(args),
        "board" => board::board_command(args),
        "server" => server_command(args),
        "web" => web::web_command(args),
        "install-browser-skill" | "install-browser-mcp-skill" => {
            skills::install_browser_skill_command(args)
        }
        "install-browser-use-skill" => skills::install_browser_use_skill_command(args),
        "computer-use" => skills::computer_use_command(args),
        "install-computer-use-skill" => skills::install_computer_use_skill_command(args),
        "cli" => skills::cli_command(args),
        "automations" => skills::automations_command(args),
        "agent-orchestration" => skills::agent_orchestration_command(args),
        "install-agent-orchestration-skill" => {
            skills::install_agent_orchestration_skill_command(args)
        }
        "fable-5.6-orchestration" => skills::fable56_orchestration_command(args),
        "install-fable-5.6-orchestration-skill" => {
            skills::install_fable56_orchestration_skill_command(args)
        }
        "find-prev-session" => skills::find_prev_session_command(args),
        "install-find-prev-session-skill" => skills::install_find_prev_session_skill_command(args),
        "generate-title" => skills::generate_title_command(args),
        "install-generate-title-skill" => skills::install_generate_title_skill_command(args),
        "move-codex-session" => skills::move_codex_session_command(args),
        "install-move-codex-session-skill" => {
            skills::install_move_codex_session_skill_command(args)
        }
        "toggle-sidebar" => run_bridge_action("toggleSidebarCollapsed", Parser::None, plain, args),
        "move-sidebar" => run_bridge_action("moveSidebar", Parser::None, plain, args),
        "assert-card" => {
            run_bridge_action("assertSidebarCard", Parser::AssertCard, assert_ok, args)
        }
        "wait-for" => run_bridge_action("waitFor", Parser::WaitFor, assert_ok, args),
        "screenshot" => diagnostics::screenshot_command(args),
        "logs" => diagnostics::logs_command(args),
        "bundle" => diagnostics::bundle_command(args),
        "help" => {
            println!("{}", usage::usage());
            Ok(())
        }
        other if automations::is_automation_command(other) => {
            automations::run_automation_command(other, args)
        }
        other => Err(CliError::Other(format!("Unknown command: {other}"))),
    }
}

fn quick_actions_command(args: &[String]) -> CliResult<()> {
    if args.is_empty()
        || matches!(
            args.first().map(String::as_str),
            Some("help") | Some("-h") | Some("--help")
        )
    {
        println!("{}", usage::quick_actions_usage());
        return Ok(());
    }
    Err(CliError::Other(format!(
        "Unknown quick-actions command: {}\n\n{}",
        args[0],
        usage::quick_actions_usage()
    )))
}

fn server_command(args: &[String]) -> CliResult<()> {
    /*
    `gx server ...` stays a thin launcher over the real gxserver binary
    (resolved next to this CLI or via dev fallbacks) exactly like the Node
    CLI. It must NOT run in-process: `gxserver start` respawns
    current_exe --foreground, which would be the ghostex CLI here.
    */
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("{}", usage::server_usage());
        return Ok(());
    }
    launchers::run_gxserver_cli_command(args)
}

#[cfg(test)]
mod tests {
    use super::is_known_command;

    #[test]
    fn tui_is_explicit_and_the_numeric_alias_is_removed() {
        assert!(is_known_command("tui"));
        assert!(!is_known_command("2"));
    }
}
