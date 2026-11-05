use super::{
    omp_command_outcome, omp_effort_answers, omp_switch_answers, omp_transcript_holds,
    pi_family_errors, pi_family_leftover_input, CodexPickerPlan, OmpRecord, PickerDriver,
    AGENT_TUI_CLEAR_INPUT_LINE, UNSUPPORTED_SELECTION_CODE,
};
use crate::session_chat_send::test_terminal::{self, TestTerminal};

const MODEL: &str = "anthropic/claude-opus-5-5";
/// OMP 18.13.3's `/switch` notice in its TUI (`selector-controller.ts`), drawn as a dim line
/// with a one-column margin after a blank row.
const SWITCH_NOTICE: &str =
    " Session-only model: anthropic/claude-opus-5-5. Use \u{2325}M or /model for roles.";
const RULE: &str = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

/// The three composer shapes OMP draws below the conversation: `pi` (rules around the input,
/// the status line below) and `borderless` (`❯`, the status line below) as captured with OMP
/// 18.13.3, and `box` with a custom status line that wraps inside the frame's top edge.
fn composers(input: &str) -> [String; 3] {
    [
        format!("{RULE}\n{input}\n{RULE}\n \u{2b22} Opus 5.5 \u{b7} \u{25d2} high \u{b7} \u{1f4c1} ~/project \u{b7} keel: no active thread \u{b7} \u{3c0}"),
        format!("\u{276f} {input}\n [M] Opus 5.5 - [high] \u{b7} [D] ~/project \u{b7} keel: no active thread \u{b7} pi"),
        format!("\u{256d}\u{2500}\u{2500} \u{25cf} Opus 5.5 \u{b7} \u{25d1} med  \u{1f4c1} ~/project  keel: no active thr\nead  \u{3c0} \u{2500}\u{2500}\u{256e}\n\u{2502} {input} \u{2502}\n\u{2570}\u{2500}{RULE}\u{2500}\u{256f}"),
    ]
}

fn screen(chat: &[&str], composer: &str) -> String {
    format!("{}\n\n{composer}", chat.join("\n\n"))
}

#[test]
fn a_switch_counts_once_its_answer_is_printed_whatever_the_composer_shape() {
    for (typed, answered) in composers("/switch anthropic/claude-opus-5-5")
        .into_iter()
        .zip(composers(""))
    {
        let before = screen(&[" Ready when you are."], &typed);
        let after = screen(&[" Ready when you are.", SWITCH_NOTICE], &answered);
        assert_eq!(omp_switch_answers(&before, MODEL), 0, "{before}");
        assert_eq!(omp_switch_answers(&after, MODEL), 1, "{after}");
    }
}

#[test]
fn a_repeated_switch_answer_is_one_more_line() {
    let [composer, ..] = composers("");
    let before = screen(&[SWITCH_NOTICE], &composer);
    let after = screen(&[SWITCH_NOTICE, SWITCH_NOTICE], &composer);
    assert!(omp_switch_answers(&after, MODEL) > omp_switch_answers(&before, MODEL));
}

#[test]
fn an_answer_in_place_of_the_previous_notice_still_counts() {
    let [composer, ..] = composers("");
    let before = screen(&[" Thinking set to high."], &composer);
    let after = screen(&[SWITCH_NOTICE], &composer);
    assert_eq!(omp_switch_answers(&before, MODEL), 0);
    assert_eq!(omp_switch_answers(&after, MODEL), 1);
    assert_eq!(omp_effort_answers(&after, "high"), 0);
}

#[test]
fn only_the_exact_model_counts() {
    let [composer, ..] = composers("");
    let chat = [
        " Session-only model: anthropic/claude-opus-5-5-fast. Use \u{2325}M or /model for roles.",
        " Session-only model: openai/gpt-5.5. Use Option+M or /model for roles.",
        // The answer of OMP's text command (`builtin-modes.ts`), without the key hint.
        " Session-only model: openai/gpt-5.",
    ];
    let shown = screen(&chat, &composer);
    assert_eq!(omp_switch_answers(&shown, MODEL), 0);
    assert_eq!(omp_switch_answers(&shown, "openai/gpt-5.5"), 1);
    assert_eq!(omp_switch_answers(&shown, "openai/gpt-5"), 1);
}

#[test]
fn only_the_whole_effort_line_counts() {
    let [composer, ..] = composers("");
    let chat = [
        " Thinking set to xhigh.",
        " Thinking set to high. Not an answer.",
        " Thinking set to high.Not an answer.",
    ];
    let shown = screen(&chat, &composer);
    assert_eq!(omp_effort_answers(&shown, "high"), 0);
    assert_eq!(omp_effort_answers(&shown, "xhigh"), 1);
}

#[test]
fn a_command_counts_only_on_a_fresh_answer_or_a_fresh_refusal() {
    let [composer, ..] = composers("");
    let older = [
        SWITCH_NOTICE,
        " Error: Unknown model: anthropic/claude-opus-9",
    ];
    let answers = |screen: &str| omp_switch_answers(screen, MODEL);
    let baseline = screen(&older, &composer);
    let (errors_before, answers_before) = (pi_family_errors(&baseline).len(), answers(&baseline));
    assert_eq!((errors_before, answers_before), (1, 1));
    assert_eq!(
        omp_command_outcome(&baseline, errors_before, answers, answers_before),
        None
    );
    let answered = screen(&[older[0], older[1], SWITCH_NOTICE], &composer);
    assert_eq!(
        omp_command_outcome(&answered, errors_before, answers, answers_before),
        Some(Ok(()))
    );
    let refused = screen(
        &[
            older[0],
            older[1],
            " Error: No API key for anthropic/claude-opus-5-5",
        ],
        &composer,
    );
    assert_eq!(
        omp_command_outcome(&refused, errors_before, answers, answers_before),
        Some(Err("No API key for anthropic/claude-opus-5-5".to_string()))
    );
}

#[test]
fn omp_refusals_of_switch_and_effort_are_errors() {
    let [composer, ..] = composers("");
    // `showError` in OMP 18.13.3 (`modes/utils/ui-helpers.ts`), with the messages of `/switch`
    // (`builtin-modes.ts`, `model-controls.ts`) and `/effort` (`helpers/effort.ts`).
    let chat = [
        " Error: Unknown model: anthropic/claude-opus-9",
        " Error: No API key for anthropic/claude-opus-5-5",
        " Error: Unknown thinking level: max. Available: off, auto, minimal, low, medium, high",
    ];
    assert_eq!(
        pi_family_errors(&screen(&chat, &composer)),
        [
            "Unknown model: anthropic/claude-opus-9",
            "No API key for anthropic/claude-opus-5-5",
            "Unknown thinking level: max. Available: off, auto, minimal, low, medium, high",
        ]
    );
}

/// OMP's input holding `input` in three `composer.shape`s: `pi` (one-space padded rows between
/// rules), `borderless` (`❯`), and `box`, which merges the input's last row into its foot.
fn inputs(input: &str) -> [String; 3] {
    let rule = RULE.repeat(5);
    [
        format!("{rule}\n {input}\n{rule}\n \u{2b22} Opus 5.5 \u{b7} \u{25d2} high"),
        format!("\u{276f} {input}\n [M] Opus 5.5 - [high] \u{b7} pi"),
        format!("\u{256d}\u{2500}\u{2500} \u{25cf} Opus 5.5 \u{2500}\u{2500}\u{256e}\n\u{2570}\u{2500} {input} \u{2500}\u{256f}"),
    ]
}

#[test]
fn a_completed_line_after_a_delivered_command_is_left_over_in_every_shape() {
    let typed = "/switch openrouter/qwen3.8-max";
    // Enter accepted OMP's completion row (with its trailing space) instead of submitting.
    for composer in inputs("/switch openrouter/qwen/qwen3.8-max ") {
        let shown = screen(&[" Ready when you are."], &composer);
        assert!(
            pi_family_leftover_input("omp", &shown, typed, Some("/switch ")),
            "{shown}"
        );
        assert!(
            !pi_family_leftover_input("omp", &shown, typed, None),
            "{shown}"
        );
    }
    for composer in inputs("/effort high") {
        let shown = screen(&[" Ready when you are."], &composer);
        assert!(
            pi_family_leftover_input("omp", &shown, "/effort xhigh", Some("/effort ")),
            "{shown}"
        );
    }
    for composer in inputs("fix the failing test") {
        let shown = screen(&[" Ready when you are."], &composer);
        assert!(
            !pi_family_leftover_input("omp", &shown, typed, Some("/switch ")),
            "{shown}"
        );
    }
}

fn never() -> bool {
    false
}

fn driver(zmx_name: &str) -> PickerDriver<'_> {
    PickerDriver {
        project_id: "project",
        session_id: "session",
        zmx_name,
        source: "test",
        cancelled: &never,
    }
}

/// Runs `command` through the OMP driver on a scripted terminal, which never answers, and
/// returns the terminal once the driver gave up and cleaned up.
async fn failed_omp_command(name: &str, terminal: TestTerminal, command: &str) -> TestTerminal {
    test_terminal::open(name, terminal);
    let outcome = driver(name)
        .run_omp_command(
            "applied model",
            command,
            |screen| omp_switch_answers(screen, "openrouter/qwen3.8-max"),
            None,
        )
        .await;
    assert!(!matches!(outcome, Ok(Ok(()))));
    test_terminal::terminal(name)
}

#[tokio::test]
async fn a_switch_omp_completed_after_the_paste_landed_is_cleared() {
    let terminal = failed_omp_command(
        "omp-completed-switch",
        TestTerminal {
            after_enter: "/switch openrouter/qwen/qwen3.8-max ".to_string(),
            ..TestTerminal::default()
        },
        "/switch openrouter/qwen3.8-max",
    )
    .await;
    assert_eq!(
        terminal.writes.last().map(String::as_str),
        Some(AGENT_TUI_CLEAR_INPUT_LINE)
    );
    assert_eq!(terminal.input, "");
}

#[tokio::test]
async fn another_command_typed_during_the_wait_stays() {
    let terminal = failed_omp_command(
        "omp-user-effort",
        TestTerminal {
            after_enter: "/effort high".to_string(),
            ..TestTerminal::default()
        },
        "/switch openrouter/qwen3.8-max",
    )
    .await;
    assert!(!terminal
        .writes
        .iter()
        .any(|write| write == AGENT_TUI_CLEAR_INPUT_LINE));
    assert_eq!(terminal.input, "/effort high");
}

#[tokio::test]
async fn a_line_in_the_input_when_the_paste_failed_stays() {
    let terminal = failed_omp_command(
        "omp-paste-refused",
        TestTerminal {
            input: "/switch openai/gpt-5.5".to_string(),
            refuse_paste: true,
            ..TestTerminal::default()
        },
        "/switch openrouter/qwen3.8-max",
    )
    .await;
    assert!(!terminal
        .writes
        .iter()
        .any(|write| write == AGENT_TUI_CLEAR_INPUT_LINE));
    assert_eq!(terminal.input, "/switch openai/gpt-5.5");
}

#[tokio::test]
async fn pi_cleanup_clears_only_its_exact_command() {
    for (input, cleared) in [
        ("/model openai/gpt-5.5", true),
        ("/model openai/gpt-5.5-mini", false),
        ("/thinking high", false),
    ] {
        let name = format!("pi-cleanup-{input}");
        test_terminal::open(
            &name,
            TestTerminal {
                input: input.to_string(),
                ..TestTerminal::default()
            },
        );
        driver(&name)
            .abandon_pi_family_command("pi", "/model openai/gpt-5.5", None)
            .await;
        let terminal = test_terminal::terminal(&name);
        assert_eq!(!terminal.writes.is_empty(), cleared, "{input}");
        assert_eq!(terminal.input.is_empty(), cleared, "{input}");
    }
}

#[tokio::test]
async fn an_id_missing_from_omps_lineup_is_refused_before_anything_is_typed() {
    let pick = |model: &str| CodexPickerPlan {
        provider: "omp".to_string(),
        model: model.to_string(),
        effort: "high".to_string(),
        ..CodexPickerPlan::default()
    };
    let lineup = crate::session_chat_pi_models::serve_test_omp_lineup(Some(&[serde_json::json!({
        "provider": "openrouter",
        "id": "qwen/qwen3.8-max",
        "name": "Qwen3.8 Max",
        "thinking": ["low", "medium", "high"],
    })]));
    test_terminal::open("omp-lineup", TestTerminal::default());
    let refused = driver("omp-lineup")
        .drive_omp(&pick("openrouter/qwen3.8-max"))
        .await
        .unwrap_err();
    assert_eq!(refused.code, UNSUPPORTED_SELECTION_CODE);
    assert_eq!(
        refused.message,
        "openrouter/qwen3.8-max is not in OMP's model list."
    );
    assert!(test_terminal::terminal("omp-lineup").writes.is_empty());
    // No terminal answers under this name, so a pick that passes the check fails on its first
    // read of the input: a listed id, and any id while no lineup has been read.
    let listed = driver("gx-test-omp-lineup-no-terminal")
        .drive_omp(&pick("openrouter/qwen/qwen3.8-max"))
        .await
        .unwrap_err();
    assert_eq!(listed.code, "sessionNotRunning");
    drop(lineup);
    let _unread = crate::session_chat_pi_models::serve_test_omp_lineup(None);
    let unread = driver("gx-test-omp-lineup-no-terminal")
        .drive_omp(&pick("openrouter/qwen3.8-max"))
        .await
        .unwrap_err();
    assert_eq!(unread.code, "sessionNotRunning");
}

const HAIKU: &str = "anthropic/claude-haiku-5-5";
const SONNET: &str = "anthropic/claude-sonnet-5-5";

/// OMP 18.13.3's transcript records of an applied `/switch` and level change, as written in
/// the live session S7h:P0gy7:G7v3w on 2026-11-02.
fn model_change(model: &str) -> String {
    format!(
        r#"{{"type":"model_change","id":"5d1c","parentId":"4e0b","timestamp":"2026-11-02T11:30:55.333Z","runtime":{{"version":"18.13.3"}},"model":"{model}","role":"temporary","resolvedModelIsFallback":false}}"#
    )
}

fn level_change(level: &str) -> String {
    format!(
        r#"{{"type":"thinking_level_change","id":"6f2d","parentId":"5d1c","timestamp":"2026-11-02T11:30:55.409Z","runtime":{{"version":"18.13.3"}},"thinkingLevel":"{level}","configured":"{level}"}}"#
    )
}

/// OMP's record while its Auto chooses the level: `level` is only what Auto resolved to
/// (`session/model-controls.ts` `setThinkingLevel`, the `AUTO_THINKING` branch).
fn auto_level_change(level: &str) -> String {
    format!(
        r#"{{"type":"thinking_level_change","id":"7a3e","parentId":"5d1c","timestamp":"2026-11-02T11:30:55.409Z","runtime":{{"version":"18.13.3"}},"thinkingLevel":"{level}","configured":"auto"}}"#
    )
}

/// A transcript holding `records`, and a terminal that never prints an answer but appends
/// `on_enter[n]` to it on its n-th Enter: OMP's answer replaced a notice that already read the
/// same, or went into a notice the terminal no longer repaints.
fn silent_omp(records: &[String], on_enter: &[String]) -> (tempfile::TempDir, TestTerminal) {
    let folder = tempfile::tempdir().expect("transcript folder");
    let path = folder.path().join("omp-session.jsonl");
    let mut text = records.join("\n");
    text.push('\n');
    std::fs::write(&path, text).expect("write the transcript");
    let terminal = TestTerminal {
        transcript: Some((path, on_enter.to_vec())),
        ..TestTerminal::default()
    };
    (folder, terminal)
}

fn transcript_of(terminal: &TestTerminal) -> std::path::PathBuf {
    terminal
        .transcript
        .as_ref()
        .expect("a transcript")
        .0
        .clone()
}

fn omp_pick(model: &str, effort: &str, transcript: std::path::PathBuf) -> CodexPickerPlan {
    CodexPickerPlan {
        provider: "omp".to_string(),
        model: model.to_string(),
        effort: effort.to_string(),
        omp_transcript: Some(transcript),
        ..CodexPickerPlan::default()
    }
}

fn claude_lineup() -> crate::session_chat_pi_models::TestLineup {
    let row = |id: &str, name: &str| {
        serde_json::json!({
            "provider": "anthropic",
            "id": id,
            "name": name,
            "thinking": ["low", "medium", "high", "max"],
        })
    };
    crate::session_chat_pi_models::serve_test_omp_lineup(Some(&[
        row("claude-haiku-5-5", "Claude Haiku 5.5"),
        row("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ]))
}

/// Whether the driver pasted exactly `commands`, in this order, and no other command.
fn pasted(terminal: &TestTerminal, commands: &[&str]) -> bool {
    let pastes: Vec<&String> = terminal
        .writes
        .iter()
        .filter(|write| write.contains('/'))
        .collect();
    pastes.len() == commands.len()
        && pastes.iter().zip(commands).all(|(write, command)| {
            **write == crate::session_chat_send::wrap_terminal_bracketed_paste_text(command)
        })
}

#[tokio::test]
async fn a_command_omp_recorded_counts_though_no_new_answer_line_shows() {
    // The session ran Haiku before: that record predates the command and is not its answer.
    let (_folder, terminal) = silent_omp(
        &[model_change(HAIKU), level_change("low")],
        &[model_change(HAIKU), level_change("max")],
    );
    let path = transcript_of(&terminal);
    test_terminal::open("omp-recorded-switch", terminal);
    let switched = driver("omp-recorded-switch")
        .run_omp_command(
            "applied model",
            &format!("/switch {HAIKU}"),
            |screen| omp_switch_answers(screen, HAIKU),
            Some((&path, OmpRecord::Model(HAIKU))),
        )
        .await;
    assert!(matches!(switched, Ok(Ok(()))), "{switched:?}");
    let (_folder, terminal) = silent_omp(&[level_change("max")], &[level_change("low")]);
    let path = transcript_of(&terminal);
    test_terminal::open("omp-recorded-effort", terminal);
    let set = driver("omp-recorded-effort")
        .run_omp_command(
            "applied thinking level",
            "/effort low",
            |screen| omp_effort_answers(screen, "low"),
            Some((&path, OmpRecord::Level("low"))),
        )
        .await;
    assert!(matches!(set, Ok(Ok(()))), "{set:?}");
}

#[tokio::test]
async fn only_a_record_of_the_picked_model_written_after_the_command_counts() {
    // An older record of the same model, and after Enter only a sibling model's.
    let (_folder, terminal) = silent_omp(
        &[model_change(HAIKU)],
        &[model_change("anthropic/claude-haiku-5-5-fast")],
    );
    let path = transcript_of(&terminal);
    test_terminal::open("omp-sibling-record", terminal);
    let outcome = driver("omp-sibling-record")
        .run_omp_command(
            "applied model",
            &format!("/switch {HAIKU}"),
            |screen| omp_switch_answers(screen, HAIKU),
            Some((&path, OmpRecord::Model(HAIKU))),
        )
        .await;
    assert_eq!(outcome.unwrap_err().code, "timeout");
}

#[tokio::test]
async fn a_repick_of_the_running_model_and_level_counts_without_typing_effort() {
    let _lineup = claude_lineup();
    // Sonnet 5.5 at medium, its default level: OMP's `/switch` records the model again and
    // leaves the level, so no level record follows (S7h:P0gy7:G7v3w at 11:32:54).
    let (_folder, terminal) = silent_omp(
        &[model_change(SONNET), level_change("medium")],
        &[model_change(SONNET)],
    );
    let path = transcript_of(&terminal);
    test_terminal::open("omp-repick", terminal);
    driver("omp-repick")
        .drive_omp(&omp_pick(SONNET, "medium", path))
        .await
        .expect("the re-pick counts");
    assert!(pasted(
        &test_terminal::terminal("omp-repick"),
        &[&format!("/switch {SONNET}")]
    ));
}

#[tokio::test]
async fn a_valid_pick_after_a_refused_one_counts_once_omp_records_it() {
    let _lineup = claude_lineup();
    let (_folder, terminal) = silent_omp(
        &[model_change(HAIKU), level_change("low")],
        &[
            // `/switch` to Haiku again resets the level to its default before `/effort`.
            format!("{}\n{}", model_change(HAIKU), level_change("max")),
            level_change("low"),
        ],
    );
    let path = transcript_of(&terminal);
    test_terminal::open("omp-after-refusal", terminal);
    let refused = driver("omp-after-refusal")
        .drive_omp(&omp_pick("nopkey/held-demo-model", "medium", path.clone()))
        .await
        .unwrap_err();
    assert_eq!(refused.code, UNSUPPORTED_SELECTION_CODE);
    driver("omp-after-refusal")
        .drive_omp(&omp_pick(HAIKU, "low", path))
        .await
        .expect("the pick OMP applied counts");
    assert!(pasted(
        &test_terminal::terminal("omp-after-refusal"),
        &[&format!("/switch {HAIKU}"), "/effort low"]
    ));
}

#[tokio::test]
async fn a_fixed_pick_leaves_omps_auto_even_when_auto_resolved_to_that_level() {
    let _lineup = claude_lineup();
    // Auto chose High for Sonnet 5.5; `/switch` keeps Auto and writes no level record. A pick
    // of fixed High must still send `/effort high`, which OMP records as the chosen level.
    let (_folder, terminal) = silent_omp(
        &[model_change(SONNET), auto_level_change("high")],
        &[model_change(SONNET), level_change("high")],
    );
    let path = transcript_of(&terminal);
    test_terminal::open("omp-leave-auto", terminal);
    driver("omp-leave-auto")
        .drive_omp(&omp_pick(SONNET, "high", path))
        .await
        .expect("the fixed pick counts once OMP records it");
    assert!(pasted(
        &test_terminal::terminal("omp-leave-auto"),
        &[&format!("/switch {SONNET}"), "/effort high"]
    ));
    // What Auto resolved to is not a fixed choice of that level.
    assert!(!omp_transcript_holds(
        &auto_level_change("high"),
        OmpRecord::Level("high")
    ));
    assert!(omp_transcript_holds(
        &level_change("high"),
        OmpRecord::Level("high")
    ));
}
