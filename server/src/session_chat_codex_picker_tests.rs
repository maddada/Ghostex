use super::*;

#[test]
fn ultra_composer_is_ready_but_advanced_picker_is_not() {
    use crate::session_chat_composer::{
        detect_session_chat_composer_ready, SessionChatComposerState,
    };
    assert_eq!(
        detect_session_chat_composer_ready(
            Some("codex"),
            "» Ask Codex to do anything\n  gpt-6-astra ultra"
        )
        .state,
        SessionChatComposerState::Ready
    );
    let advanced = "Advanced Reasoning\n⚠ Consumes usage limits faster\n› 1. Max  Higher usage\n  2. Ultra  Highest usage\nPress enter to confirm or esc to go back";
    assert!(any_picker_open(advanced));
    assert_eq!(advanced_effort_picker_rows(advanced).unwrap().len(), 2);
    assert_eq!(
        detect_session_chat_composer_ready(Some("codex"), advanced).state,
        SessionChatComposerState::NotReady
    );
}

/// Run this test binary from the packaged Web directory so bundled zmx resolves.
/// Set `GHOSTEX_CODEX_PICKER_TEST_ZMX=ghostex-effort-check-...` to a dedicated idle Codex TUI and pass `live_zmx_effort_round_trip --ignored --nocapture`.
#[tokio::test]
#[ignore = "requires an explicitly supplied disposable Codex zmx session"]
async fn live_zmx_effort_round_trip() {
    let name = std::env::var("GHOSTEX_CODEX_PICKER_TEST_ZMX")
        .expect("set the dedicated test session name");
    assert!(name.starts_with("ghostex-effort-check-"));
    let screen = capture_session_terminal_text(&name).await.unwrap();
    let initial = detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
    let model = initial.model.unwrap().value;
    let original_effort = initial.effort.unwrap().value;
    let warning = "Max and Ultra are available under";
    let warning_count = screen.matches(warning).count();
    let mut failure = None;
    for effort in [
        "high",
        "xhigh",
        "max",
        "ultra",
        "max",
        "low",
        "ultra",
        "high",
        original_effort.as_str(),
    ] {
        let started = std::time::Instant::now();
        let job_id = register_job(CodexPickerPlan {
            model: model.clone(),
            effort: effort.to_string(),
            ..Default::default()
        });
        let send = execute_session_chat_send(
            "effort-check",
            &name,
            &name,
            "effort-check",
            vec![SessionChatSendStep::DriveCodexModelPicker { job_id }],
        )
        .await;
        let result = take_job_outcome(job_id).expect("picker worker outcome");
        if let Err(error) = result {
            failure = Some(error.message);
            break;
        }
        assert!(send.is_ok());
        let screen = capture_session_terminal_text(&name).await.unwrap();
        let selection =
            detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
        assert_eq!(selection.model.unwrap().value, model);
        assert_eq!(selection.effort.unwrap().value, effort);
        assert_eq!(
            screen.matches(warning).count(),
            warning_count,
            "shifted arrows must not attempt Max/Ultra"
        );
        assert!(!any_picker_open(&screen));
        println!(
            "{model} {effort}: {}ms, footer confirmed and picker closed",
            started.elapsed().as_millis()
        );
    }
    if let Some(error) = failure {
        panic!(
            "{error}\n{}",
            capture_session_terminal_text(&name)
                .await
                .unwrap_or_default()
        );
    }
}

/// GitHub #290: no model pick may type into a screen without the agent's input box, and a screen
/// that shows nothing of the agent (blank, a shell prompt with its start command unrun) must read
/// as a missing input box, which the queue gives up on, rather than as a dialog it waits behind.
#[test]
fn a_shell_prompt_or_blank_screen_is_a_missing_input_box() {
    use crate::session_chat_composer::{
        detect_session_chat_composer_readiness, SessionChatComposerState,
    };
    let unrun = "Last login: Fri Oct 24 12:59:01 on ttys004\n~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low^G^G";
    for (agent, screen) in [
        ("codex", unrun),
        ("codex", ""),
        ("claude", "~/dev/app ❯  claude --model sonnet --effort high"),
        ("claude", "~/dev/app ❯"),
    ] {
        let readiness = detect_session_chat_composer_readiness(Some(agent), screen, None);
        assert_ne!(
            readiness.state,
            SessionChatComposerState::Ready,
            "{agent}: {screen}"
        );
        assert!(readiness.input_box_missing(), "{agent}: {screen}");
        assert_eq!(
            input_box_refusal(&readiness, "no input box").code,
            INPUT_BOX_MISSING_CODE
        );
    }
}

#[test]
fn a_dialog_in_front_of_the_input_box_is_busy_not_missing() {
    use crate::session_chat_composer::{
        detect_session_chat_composer_readiness, SessionChatComposerState,
    };
    const RULE: &str = "────────────────────────────────────────";
    let dashboard = [
        RULE,
        "❯ describe a task for a new session",
        RULE,
        "  ⏵⏵ auto mode · enter to return · space to reply · ctrl+x to delete · ? for shortcuts",
    ]
    .join("\n");
    let readiness = detect_session_chat_composer_readiness(Some("claude"), &dashboard, None);
    assert_eq!(readiness.state, SessionChatComposerState::NotReady);
    assert!(!readiness.input_box_missing());
    assert_eq!(input_box_refusal(&readiness, "busy").code, "agentBusy");
}

#[test]
fn the_agents_own_input_box_is_ready() {
    use crate::session_chat_composer::{
        detect_session_chat_composer_readiness, SessionChatComposerState,
    };
    let codex = ">_ OpenAI Codex (v0.164.0)\n\n› Ask Codex to do anything\n\n  GPT-6.1-Sol low · 01a16e25 · Ghostex · main · Context 0% used\n  ? for shortcuts";
    assert_eq!(
        detect_session_chat_composer_readiness(Some("codex"), codex, None).state,
        SessionChatComposerState::Ready
    );
    const RULE: &str = "────────────────────────────────────────";
    let claude = [RULE, "❯ ", RULE, "  Sonnet 5.5 | medium"].join("\n");
    assert_eq!(
        detect_session_chat_composer_readiness(Some("claude"), &claude, None).state,
        SessionChatComposerState::Ready
    );
}

#[test]
fn codex_permissions_menu_reads_its_current_row() {
    use super::codex_permissions::permissions_menu_current;
    // Verbatim `/permissions` menu, Codex 0.164 (2026-10-27).
    let menu = concat!(
        "• Permission selection requested: Read Only\n",
        "  Update Model Permissions\n",
        "  1. Ask for approval     Read and edit workspace files and run commands, with approval required for internet access or edits outside the workspace\n",
        "  2. Approve for me       Only ask for actions detected as potentially unsafe\n",
        "  3. Full Access          Use with caution: Codex can edit files outside this workspace and access the internet without approval\n",
        "› 4. Read Only (current)  Read workspace files, with approval required for edits or internet access\n",
        "  enter select · esc back\n",
    );
    assert_eq!(
        permissions_menu_current(menu),
        Some(Some("read-only".to_string()))
    );
    let loading = "  Update Model Permissions\n  Loading permission profiles…\n";
    assert_eq!(permissions_menu_current(loading), None);
    let none_current = menu.replace(" (current)", "");
    assert_eq!(permissions_menu_current(&none_current), Some(None));
    assert_eq!(
        permissions_menu_current("› Ask Codex to do anything\n"),
        None
    );
}

#[test]
fn codex_dialogs_drawn_beside_older_lines_are_still_read() {
    use super::codex_permissions::{full_access_confirmation_open, permissions_menu_current};
    // Verbatim desktop screen (Codex 0.164, 2026-10-27): the box is drawn over the right half,
    // beside the conversation's older lines.
    let beside = concat!(
        "• Permission selection requested: Approve for me\n",
        "• Permission selection requested: Read Only\n",
        "› Run exactly this one shell command and nothing else, then repl  Enable full access?\n",
        "                                                                  When Codex runs with full access, it can edit any file on your\n",
        "                                                                  computer and run commands with network, without your approval.\n",
        "• I’ll run the requested command.                                 Exercise caution when enabling full access. This significantly\n",
        "                                                                  increases the risk of data loss, leaks, or unexpected behavior.\n",
        "✗ You canceled the request to run touch proof-readonly.txt\n",
        "• Exited (status 1) touch proof-readonly.txt                    › 1. Yes, continue anyway  Apply full access for this session\n",
        "  └ (no output)                                                   2. Cancel                Go back without enabling full access\n",
        "■ Conversation interrupted - use /feedback if something went wro  enter select · esc back\n",
        "› Ask Codex to do anything\n",
        "  GPT-6-Sol high · 01a17e2e-bab7-7ca1-a2a0-575fa9dc1b69 · 11.2K used · proj2 · main · Context 5% used · weekly 99% left\n",
        "  ? for shortcuts                                                                                                                                                              ⚠ 1 warning · f2 to view\n",
    );
    assert!(full_access_confirmation_open(beside));
    let menu = concat!(
        "• Ran touch x                                    Update Model Permissions\n",
        "  └ (no output)                                    1. Ask for approval     Asks first\n",
        "• done                                           › 2. Read Only (current)  Reads only\n",
        "                                                   enter select · esc back\n",
    );
    assert_eq!(
        permissions_menu_current(menu),
        Some(Some("read-only".to_string()))
    );
}

/// Codex 0.164's /model list as it painted on Linux, with GPT-6.1 Terra's spaced name.
#[test]
fn gpt_6_1_terra_is_found_by_its_id_in_the_list_and_the_reasoning_title() {
    let screen = include_str!("../tests/fixtures/codex-model-picker-terra.txt");
    let rows = model_picker_rows(screen).expect("the model list is read");
    let find = |id: &str| {
        rows.iter()
            .find(|row| row_names(row, id))
            .map(|row| row.number)
    };
    assert_eq!(find("gpt-6.1-terra"), Some(2));
    assert_eq!(find("gpt-6.1-sol"), Some(1));
    assert_eq!(find("gpt-5.6-terra"), Some(7));
    let reasoning = "Select Reasoning Level for GPT-6.1 Terra\n\n› 1. Low (default)\n  2. Medium\n  3. High\n\n  enter default · s session · esc back";
    assert_eq!(
        effort_picker_rows(reasoning, "gpt-6.1-terra").map(|rows| rows.len()),
        Some(3)
    );
    assert!(effort_picker_offers_session(reasoning, "gpt-6.1-terra"));
    assert!(effort_picker_rows(reasoning, "gpt-6.1-sol").is_none());
}
