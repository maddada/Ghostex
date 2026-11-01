// ---------------------------------------------------------------------------
// Inline tests — fixtures are VERBATIM `ghostex read-text <id> --lines 15`
// dumps captured from live sessions on 2026-08-01.
// ---------------------------------------------------------------------------

use super::*;

/// G1ipk — claude, this user's custom statusline, with prose and sub-agent
/// rows around it that must never match.
const CLAUDE_CUSTOM_STATUSLINE: &str = concat!(
    "  \u{25fc} Show actual current model+effort in chat pills via zmx scrollback detection\n",
    "  \u{25fc} RN: merge the two top-right more-options menus; rename attach option\n",
    "  \u{25fb} Rebuild, restart, E2E verify, Fable verifier, commit\n",
    "                                current: 2.1.220 \u{b7} latest: 2.1.220 \u{2718} Auto-update failed \u{b7} Run claude doctor\n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} multi-agent-terminal-architecture \u{2500}\u{2500}\n",
    "\u{276f} \n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
    "  Ctx Used: 11.0% | 13.5% | $261.54 | Fable 5 | high\n",
    "  fb7572ef-2965-4e5e-b21e-bb0e3c455b66 | .../Ghostex | xyzt71@gmail.com\n",
    "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
    "\n",
    "  \u{23fa} main\n",
    "  \u{25ef} general-purpose  Diagnose stale chat identity          11m 33s \u{b7} \u{2193} 58.7k tokens\n",
    "  \u{25ef} general-purpose  Design scrollback model detection     11m 15s \u{b7} \u{2193} 58.9k tokens\n",
    "\u{276f} \u{25ef} general-purpose  RN merge header menus              11m 1s \u{b7} \u{2193} 38.1k tokens\n",
);

/// G1htq — claude, effort `medium`.
const CLAUDE_MEDIUM: &str = concat!(
    "                                                                   66936 tokens\n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} sync-local-to-remote-main \u{2500}\u{2500}\n",
    "\u{276f} \n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
    "  Ctx Used: 7.0% | 8.3% | $2.42 | Fable 5 | medium\n",
    "  b6672e82-b770-411b-b7b7-17b0449ad9c5 | .../Ghostex | xyzt71@gmail.com\n",
    "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
);

/// G6l3p — claude, with assistant prose above the statusline.
const CLAUDE_WITH_PROSE: &str = concat!(
    "  The high-effort path is what Opus would pick here, but gpt-5.6 also works.\n",
    "\u{273b} Brewed for 12m 23s\n",
    "                                        new task? /clear to save 120.7k tokens\n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} bg-image-file-picker \u{2500}\u{2500}\n",
    "\u{276f} \n",
    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
    "  Ctx Used: 12.0% | 15.1% | $10.12 | Fable 5 | high\n",
    "  1636419d-1d52-42b4-a546-c4db8fdfcfed | .../Ghostex | xyzt71@gmail.com\n",
    "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
);

/// G2a9p — codex, the primary codex footer sample.
const CODEX_FOOTER: &str = concat!(
    "  - Final verification passed: signed app bundle, versions, APK checksum, release notes.\n",
    "  - Windows packages remain unsigned beta builds and may display SmartScreen warnings.\n",
    "\n",
    "  Summary: Ghostex 6.13.0 is live and verified across GitHub, Sparkle, Homebrew, and gxserver.\n",
    "\n",
    "\u{2500} Worked for 1h 47m 48s \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
    "\n",
    "\n",
    "\u{203a} Summarize recent commits\n",
    "\n",
    "  GPUI MacOS Release \u{b7} gpt-5.6-sol high \u{b7} 19.8M used \u{b7} Ghostex \u{b7} codex/fix-agent-skill-settings-controls \u{b7} Context 29% used \u{b7} weekly 99% left\n",
);

/// G8q7x — codex with the trailing `fast` modifier.
const CODEX_FAST: &str = concat!(
    "\u{203a} Explain this codebase\n",
    "\n",
    "  APK Release Server \u{b7} gpt-5.6-sol high fast \u{b7} 225K used \u{b7} Ghostex \u{b7} main \u{b7} Context 26% used \u{b7} weekly 99% left\n",
);

/// G5w59 — codex, effort `xhigh`.
const CODEX_XHIGH: &str = concat!(
    "\u{203a}  xxxxhjg tersseeeegrssss\n",
    "\n",
    "  Cloud Code Cursor Bug \u{b7} gpt-5.6-sol xhigh \u{b7} 746K used \u{b7} Ghostex \u{b7} codex/fix-agent-skill-settings-controls \u{b7} Context 54% used \u{b7} weekly 96% left\n",
);

/// G83ih — codex in a narrow pane: the footer is width-clipped.
const CODEX_CLIPPED: &str = concat!(
    "\u{203a} Run /review on my current changes\n",
    "\n",
    "  Command Pane Border Fix \u{b7} gpt-5.6-sol high \u{b7} 484K used \u{b7} G\u{2026}\n",
);

const CURSOR_WITHOUT_CONTEXT: &str = concat!(
    "  \u{2192} Plan, search, build anything\n",
    "  New Agent \u{b7} Cursor Grok 4.6 Medium \u{b7} 0 used\n",
    "  Ghostex \u{b7} main \u{b7} Ctx 0% used \u{b7} +6702 -472\n",
);

fn claude(text: &str) -> Option<SessionChatDetectedSelection> {
    detect_session_chat_selection(SessionChatOptionAgent::Claude, text)
}

fn codex(text: &str) -> Option<SessionChatDetectedSelection> {
    detect_session_chat_selection(SessionChatOptionAgent::Codex, text)
}

fn cursor(text: &str) -> Option<SessionChatDetectedSelection> {
    detect_session_chat_selection(SessionChatOptionAgent::Cursor, text)
}

fn pair(selection: &SessionChatDetectedSelection) -> (Option<&str>, Option<&str>) {
    (
        selection.model.as_ref().map(|choice| choice.value.as_str()),
        selection
            .effort
            .as_ref()
            .map(|choice| choice.value.as_str()),
    )
}

#[test]
fn detects_claude_custom_statusline_model_and_effort() {
    let selection = claude(CLAUDE_CUSTOM_STATUSLINE).expect("claude statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    // The RAW rendered text is preserved so the pill can show the real
    // version instead of the catalog's.
    assert_eq!(selection.model.as_ref().unwrap().label, "Fable 5");
    assert_eq!(selection.effort.as_ref().unwrap().label, "high");
    assert_eq!(selection.fast, None);
}

#[test]
fn detects_claude_medium_effort() {
    let selection = claude(CLAUDE_MEDIUM).expect("claude statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("medium")));
}

#[test]
fn claude_prose_mentioning_models_never_matches() {
    let selection = claude(CLAUDE_WITH_PROSE).expect("claude statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
}

#[test]
fn claude_thread_title_divider_is_skipped() {
    // A title that IS a model family name still cannot win: the line is a
    // `─` rule, and rules are skipped before segmenting.
    let text = concat!(
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} Opus 4.8 \u{2500}\u{2500}\n",
        "\u{276f} \n",
    );
    assert_eq!(claude(text), None);
}

#[test]
fn claude_without_a_statusline_detects_nothing() {
    let text = concat!(
        "\u{276f} \n",
        "  Ready. Ask me anything about high availability or sonnet forms.\n",
    );
    assert_eq!(claude(text), None);
}

#[test]
fn claude_version_variants_map_to_the_family_value() {
    for (segment, value) in [
        ("Opus 4.5", "opus"),
        ("Opus", "opus"),
        ("Sonnet 5.5", "sonnet"),
        ("Sonnet 5", "claude-sonnet-5"),
        ("Haiku", "haiku"),
    ] {
        let text = format!("  Ctx Used: 1.0% | 2.0% | $1.00 | {segment} | max\n");
        let selection = claude(&text).expect("statusline detected");
        assert_eq!(pair(&selection), (Some(value), Some("max")));
        assert_eq!(selection.model.as_ref().unwrap().label, segment);
    }
}

#[test]
fn claude_rejects_lookalike_segments() {
    let text = concat!(
        "  Ctx Used: 1.0% | Opusculum | opus | Fable five | HIGH | Sonnet 5x\n",
        "\u{276f} \n",
    );
    assert_eq!(claude(text), None);
}

#[test]
fn detects_codex_footer_model_and_effort() {
    let selection = codex(CODEX_FOOTER).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
    assert_eq!(selection.fast, None);
}

#[test]
fn detects_codex_fast_modifier() {
    let selection = codex(CODEX_FAST).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
    assert_eq!(selection.fast, Some(true));
}

#[test]
fn detects_codex_xhigh_effort() {
    let selection = codex(CODEX_XHIGH).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("xhigh")));
}

#[test]
fn detects_codex_footer_clipped_after_the_model_segment() {
    let selection = codex(CODEX_CLIPPED).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
}

#[test]
fn detects_cursor_effort_without_a_context_token_and_hides_the_brand_prefix() {
    let selection = cursor(CURSOR_WITHOUT_CONTEXT).expect("cursor footer detected");
    assert_eq!(pair(&selection), (Some("cursor-grok-4.6"), Some("medium")));
    assert_eq!(selection.model.as_ref().unwrap().label, "Grok 4.6");
    assert_eq!(selection.effort.as_ref().unwrap().label, "Medium");
    assert_eq!(selection.context_window, None);
    assert_eq!(
        selection.terminal_status_line.as_deref(),
        Some(concat!(
            "New Agent \u{b7} Cursor Grok 4.6 Medium \u{b7} 0 used\n",
            "Ghostex \u{b7} main \u{b7} Ctx 0% used \u{b7} +6702 -472"
        ))
    );
}

#[test]
fn detects_hermes_context_from_its_statusline() {
    // Captured from `hermes -p harry` (2026-09-26) after one exchange.
    let text = concat!(
        "\u{2624} gpt-6-sol-900k \u{2502} 24.1K/872K \u{2502} [\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}] 3% \u{2502} ",
        "\u{25f7} 2.7s \u{2502} \u{2191} 2 t/s \u{2502} 13s \u{2502} \u{23f2} 3s \u{2502} \u{2713} 1s          \u{2500} OK\n",
    );
    let selection = detect_session_chat_selection(SessionChatOptionAgent::Hermes, text)
        .expect("hermes statusline detected");
    assert_eq!(selection.model.as_ref().unwrap().value, "gpt-6-sol-900k");
    assert_eq!(
        selection.context_usage,
        Some(SessionChatContextUsage {
            used_percentage: Some(3),
            used_tokens: Some(24_100),
            window_size: Some(872_000),
        })
    );
    // A running turn draws the live timer glyph; an estimate and a pinned window mark the figure.
    let marked = text
        .replace('\u{23f2}', "\u{23f1}")
        .replace("24.1K/872K", "~24.1K/872K pinned")
        .replace("] 3%", "] ~3%");
    assert_eq!(
        detect_session_chat_selection(SessionChatOptionAgent::Hermes, &marked)
            .and_then(|marked| marked.context_usage),
        selection.context_usage
    );
    let fresh = detect_session_chat_selection(
        SessionChatOptionAgent::Hermes,
        "\u{2624} gpt-6-sol-900k \u{2502} ctx -- \u{2502} [\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}] -- \u{2502} 2s \u{2502} \u{23f2} 0s\n",
    )
    .expect("fresh hermes statusline detected");
    assert_eq!(fresh.context_usage, None);
    // A model Hermes resolved to its 1M variant keeps the tag Hermes shows.
    let tagged = text.replace("gpt-6-sol-900k", "claude-opus-5-5[1m]");
    assert_eq!(
        detect_session_chat_selection(SessionChatOptionAgent::Hermes, &tagged)
            .and_then(|tagged| tagged.model)
            .map(|model| model.value),
        Some("claude-opus-5-5[1m]".to_string())
    );
}

#[test]
fn detects_every_cursor_effort_spelling() {
    for (label, value) in [
        ("Low", "low"),
        ("Medium", "medium"),
        ("Med", "medium"),
        ("High", "high"),
        ("xHigh", "xhigh"),
        ("Extra High", "xhigh"),
        ("Max", "max"),
        ("Ultra", "ultra"),
    ] {
        let text = format!("New Agent \u{b7} GPT-5.6 Sol {label} \u{b7} 0 used\n");
        let selection = cursor(&text).expect("cursor footer detected");
        assert_eq!(
            selection
                .effort
                .as_ref()
                .map(|choice| choice.value.as_str()),
            Some(value)
        );
    }
}

#[test]
fn codex_model_id_outside_the_catalog_is_reported_verbatim() {
    let text = "  Some Title \u{b7} gpt-9.1-nova medium \u{b7} 1K used \u{b7} Context 3% used \u{b7} weekly 99% left\n";
    let selection = codex(text).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-9.1-nova"), Some("medium")));
    assert_eq!(selection.model.as_ref().unwrap().label, "gpt-9.1-nova");
}

/// CDXC:AgentScreenDetection 2026-09-03 WHY: `tui.status_line` is user
/// ordered, so a footer that lists `model-with-reasoning` first must read
/// the same as the default order. Live capture from a session whose config
/// puts the model before the thread title.
#[test]
fn codex_model_first_footer_is_read() {
    let text = "  gpt-5.6-sol high \u{b7} Fix status line parsing \u{b7} 89K used \u{b7} Ghostex \u{b7} main \u{b7} Context 34% used \u{b7} weekly 34% left\n";
    let selection = codex(text).expect("codex footer detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
}

#[test]
fn codex_prose_mentioning_a_model_never_matches() {
    let text = concat!(
        "  I switched the worker to gpt-5.6-sol high because it is faster.\n",
        "\u{203a} \n",
    );
    assert_eq!(codex(text), None);
}

#[test]
fn codex_worked_for_rule_line_is_skipped() {
    let text = concat!(
        "\u{2500} Worked for 2m \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} gpt-5.6-sol high \u{2500}\u{2500}\n",
        "\u{203a} \n",
    );
    assert_eq!(codex(text), None);
}

#[test]
fn bottom_most_statusline_wins() {
    let text = concat!(
        "  Ctx Used: 1.0% | 2.0% | $1.00 | Sonnet 5 | low\n",
        "\u{276f} \n",
        "  Ctx Used: 1.0% | 2.0% | $1.00 | Fable 5 | high\n",
    );
    let selection = claude(text).expect("statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
}

#[test]
fn only_the_tail_window_is_scanned() {
    let mut text = String::from("  Ctx Used: 1.0% | 2.0% | $1.00 | Fable 5 | high\n");
    for index in 0..SESSION_CHAT_OPTION_SCAN_LINES {
        text.push_str(&format!("  filler line {index}\n"));
    }
    assert_eq!(claude(&text), None);
}

/// Captured live from G1ipk on 2026-08-02: Claude Code's statusline is
/// rendered with NON-BREAKING spaces, which the parser must fold.
#[test]
fn claude_statusline_rendered_with_non_breaking_spaces_matches() {
    let text = concat!(
        "  \u{25fb} Rebuild, restart, E2E verify, Fable verifier, commit\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} multi-agent-terminal-architecture \u{2500}\u{2500}\n",
        "\u{276f}\u{a0}\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "  Ctx\u{a0}Used:\u{a0}13.0%\u{a0}|\u{a0}16.5%\u{a0}|\u{a0}$286.90\u{a0}|\u{a0}Fable\u{a0}5\u{a0}|\u{a0}high\n",
        "  fb7572ef-2965-4e5e-b21e-bb0e3c455b66\u{a0}|\u{a0}.../Ghostex\u{a0}|\u{a0}xyzt71@gmail.com\n",
        "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} \u{2190} for agents\n",
    );
    let selection = claude(text).expect("statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    assert_eq!(selection.model.as_ref().unwrap().label, "Fable 5");
}

#[test]
fn ansi_colour_codes_are_stripped_before_matching() {
    let text = "  Ctx Used: 1.0% | \u{1b}[32m$1.00\u{1b}[0m | \u{1b}[1mFable 5\u{1b}[0m | \u{1b}[33mhigh\u{1b}[0m\n";
    let selection = claude(text).expect("statusline detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
}

#[test]
fn agents_without_a_table_detect_nothing() {
    assert_eq!(
        session_chat_option_agent(Some("cursor")),
        Some(SessionChatOptionAgent::Cursor)
    );
    assert_eq!(session_chat_option_agent(None), None);
    assert_eq!(
        session_chat_option_agent(Some("openclaude")),
        Some(SessionChatOptionAgent::Claude)
    );
    assert_eq!(
        session_chat_option_agent(Some("grok")),
        Some(SessionChatOptionAgent::Grok)
    );
}

#[test]
fn effort_only_statuslines_are_reported() {
    let text = "  Ctx Used: 1.0% | 2.0% | $1.00 | high\n";
    let selection = claude(text).expect("statusline detected");
    assert_eq!(pair(&selection), (None, Some("high")));
}

#[test]
fn detects_claude_model_and_effort_from_structured_transcript_rows() {
    let text = concat!(
        "{\"type\":\"assistant\",\"effort\":\"high\",\"message\":{\"model\":\"claude-fable-5\",\"content\":[]}}\n",
        "{\"type\":\"user\",\"message\":{\"content\":\"next\"}}\n",
    );
    let selection = detect_session_chat_transcript_selection(SessionChatOptionAgent::Claude, text)
        .expect("claude transcript options detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
    let model = selection.model.as_ref().unwrap();
    assert_eq!(model.label, "Fable 5");
    assert_eq!(model.source, SessionChatOptionEvidence::Transcript);
}

#[test]
fn ignores_claude_sidechain_models_when_resolving_the_main_session() {
    let text = concat!(
        "{\"type\":\"assistant\",\"isSidechain\":false,\"effort\":\"high\",\"message\":{\"model\":\"claude-fable-5\"}}\n",
        "{\"type\":\"assistant\",\"isSidechain\":true,\"effort\":\"low\",\"message\":{\"model\":\"claude-haiku-4-5\"}}\n",
    );
    let selection = detect_session_chat_transcript_selection(SessionChatOptionAgent::Claude, text)
        .expect("main claude transcript options detected");
    assert_eq!(pair(&selection), (Some("fable"), Some("high")));
}

#[test]
fn detects_codex_model_and_effort_from_latest_turn_context() {
    let text = concat!(
        "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.5\",\"effort\":\"medium\"}}\n",
        "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.6-sol\",\"effort\":\"high\"}}\n",
    );
    let selection = detect_session_chat_transcript_selection(SessionChatOptionAgent::Codex, text)
        .expect("codex transcript options detected");
    assert_eq!(pair(&selection), (Some("gpt-5.6-sol"), Some("high")));
    assert_eq!(
        selection.model.as_ref().unwrap().source,
        SessionChatOptionEvidence::Transcript
    );
}

#[test]
fn terminal_values_override_transcript_values_per_option() {
    let transcript = SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: "fable".to_string(),
            label: "Fable 5".to_string(),
            source: SessionChatOptionEvidence::Transcript,
        }),
        effort: Some(SessionChatDetectedChoice {
            value: "high".to_string(),
            label: "high".to_string(),
            source: SessionChatOptionEvidence::Transcript,
        }),
        mode: None,
        permissions: None,
        permissions_recorded_at: None,
        context_window: None,
        terminal_status_line: None,
        fast: None,
        context_usage: None,
        claude_status: None,
        codex_status: None,
        cursor_status: None,
        hermes_status: None,
        pi_status: None,
        checkout_status: None,
        model_catalog: None,
    };
    let terminal = claude("Ctx Used: 1% | Opus 4.8").unwrap();
    let merged = merge_session_chat_option_selections(None, Some(transcript), None, Some(terminal))
        .expect("merged options");
    assert_eq!(pair(&merged), (Some("opus"), Some("high")));
    assert_eq!(
        merged.model.as_ref().unwrap().source,
        SessionChatOptionEvidence::Terminal
    );
    assert_eq!(
        merged.effort.as_ref().unwrap().source,
        SessionChatOptionEvidence::Transcript
    );
}

#[test]
fn option_command_text_is_recognised_per_agent() {
    assert!(is_session_chat_option_command_text(
        Some("claude"),
        "/model opus"
    ));
    assert!(is_session_chat_option_command_text(Some("claude"), "/fast"));
    assert!(is_session_chat_option_command_text(Some("codex"), "/model"));
    assert!(!is_session_chat_option_command_text(
        Some("claude"),
        "please /model opus"
    ));
    // Grok has a `/model` picker of its own, so typing it still earns the
    // post-dispatch screen re-read.
    assert!(is_session_chat_option_command_text(Some("grok"), "/model"));
}

#[test]
fn detected_options_serialize_to_the_shared_contract_shape() {
    let options = SessionChatDetectedOptions {
        selection: SessionChatDetectedSelection {
            model: Some(SessionChatDetectedChoice {
                value: "fable".to_string(),
                label: "Fable 5".to_string(),
                source: SessionChatOptionEvidence::Transcript,
            }),
            effort: Some(SessionChatDetectedChoice {
                value: "high".to_string(),
                label: "high".to_string(),
                source: SessionChatOptionEvidence::Terminal,
            }),
            mode: None,
            permissions: None,
            permissions_recorded_at: None,
            context_window: None,
            terminal_status_line: None,
            fast: Some(true),
            context_usage: None,
            claude_status: None,
            codex_status: None,
            cursor_status: None,
            hermes_status: None,
            pi_status: None,
            checkout_status: None,
            model_catalog: None,
        },
        detected_at: "2026-08-01T12:00:00.000Z".to_string(),
    };
    assert_eq!(
        options.to_value(),
        json!({
            "model": { "value": "fable", "label": "Fable 5", "source": "transcript" },
            "effort": { "value": "high", "label": "high", "source": "terminal" },
            "fast": true,
            "detectedAt": "2026-08-01T12:00:00.000Z",
        })
    );
}

#[test]
fn same_selection_ignores_the_timestamp() {
    let selection = SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: "fable".to_string(),
            label: "Fable 5".to_string(),
            source: SessionChatOptionEvidence::Transcript,
        }),
        ..SessionChatDetectedSelection::default()
    };
    let first = SessionChatDetectedOptions {
        selection: selection.clone(),
        detected_at: "2026-08-01T12:00:00.000Z".to_string(),
    };
    let second = SessionChatDetectedOptions {
        selection,
        detected_at: "2026-08-01T12:00:05.000Z".to_string(),
    };
    assert!(first.same_selection(Some(&second)));
    assert!(!first.same_selection(None));
}

#[test]
fn cursor_launch_model_survives_its_context_only_statusline() {
    let launch = launch_command_selection(
        SessionChatOptionAgent::Cursor,
        "cursor-agent --workspace '--model x' --model grok-4.7",
    )
    .expect("cursor launch selection");
    assert_eq!(
        launch.model.as_ref().map(|model| model.value.as_str()),
        Some("grok-4.7")
    );
    let statusline = SessionChatDetectedSelection {
        context_usage: Some(SessionChatContextUsage {
            used_percentage: Some(3),
            used_tokens: None,
            window_size: None,
        }),
        ..SessionChatDetectedSelection::default()
    };
    let merged = merge_session_chat_option_selections(Some(launch), None, Some(statusline), None)
        .expect("merged options");
    assert_eq!(
        merged
            .model
            .as_ref()
            .map(|model| (model.value.as_str(), model.source)),
        Some(("grok-4.7", SessionChatOptionEvidence::Launch))
    );
}

#[test]
fn launch_flags_of_the_newer_agents_fill_the_pills() {
    for (agent, command, model, effort) in [
        (
            SessionChatOptionAgent::Omp,
            "omp --model openai/gpt-5.5 --thinking high",
            "openai/gpt-5.5",
            "high",
        ),
        (
            SessionChatOptionAgent::Copilot,
            "copilot --model claude-opus-5.5 --reasoning-effort xhigh",
            "claude-opus-5.5",
            "xhigh",
        ),
        (
            SessionChatOptionAgent::Grok,
            "grok -m grok-build --effort medium",
            "grok-build",
            "medium",
        ),
        (
            SessionChatOptionAgent::Grok,
            "grok --model grok-build --reasoning-effort low",
            "grok-build",
            "low",
        ),
        (
            SessionChatOptionAgent::Hermes,
            "hermes -m anthropic/claude-sonnet-4.6 --reasoning high",
            "anthropic/claude-sonnet-4.6",
            "high",
        ),
    ] {
        let launch = launch_command_selection(agent, command).expect(command);
        assert_eq!(
            launch.model.as_ref().map(|choice| choice.value.as_str()),
            Some(model),
            "{command}"
        );
        assert_eq!(
            launch.effort.as_ref().map(|choice| choice.value.as_str()),
            Some(effort),
            "{command}"
        );
    }
}

/// Verbatim from a Codex 0.164 session started with `--yolo` whose mode was then changed with
/// `/permissions` (2026-10-27).
const CODEX_PERMISSIONS_HISTORY: &str = concat!(
    ">_ OpenAI Codex (v0.164.0)\n",
    "     ~/proj\n",
    "  permissions: YOLO mode\n",
    "• Permission selection requested: Read Only\n",
    "• Permission selection requested: Approve for me\n",
    "■ Unknown permission profile `:bogus`. Run `/permissions` to see available profiles.\n",
    "› Ask Codex to do anything\n",
    "  GPT-6-Sol high · 01a17bd8-8057-74b1-b65d-2ba26d2f0fc9 · proj · main · Context 0% used · weekly 99% left\n",
);

#[test]
fn codex_permissions_come_from_the_newest_line_codex_printed() {
    let detected =
        detect_session_chat_selection(SessionChatOptionAgent::Codex, CODEX_PERMISSIONS_HISTORY)
            .expect("codex selection");
    let permissions = detected.permissions.expect("permissions");
    assert_eq!(permissions.value, "approve-for-me");
    assert_eq!(permissions.label, "Approve for me");
    assert_eq!(permissions.source, SessionChatOptionEvidence::Terminal);
    // The YOLO header alone names Full Access.
    let header_only =
        ">_ OpenAI Codex (v0.164.0)\n  permissions: YOLO mode\n› Ask Codex to do anything\n";
    assert_eq!(
        codex_permissions_on_screen(header_only).map(|choice| choice.value),
        Some("full-access".to_string())
    );
    // A line an earlier Codex run printed above this run's title says nothing about this run.
    let restarted = format!(
        "{CODEX_PERMISSIONS_HISTORY}>_ OpenAI Codex (v0.164.0)\n› Ask Codex to do anything\n"
    );
    assert_eq!(codex_permissions_on_screen(&restarted), None);
    // An assistant sentence that mentions permissions is not Codex's own line.
    assert_eq!(
        codex_permissions_on_screen("• I changed the permissions to Read Only for you.\n"),
        None
    );
}

#[test]
fn codex_turn_context_names_its_permission_mode() {
    let read = |line: &str| {
        detect_session_chat_transcript_selection(SessionChatOptionAgent::Codex, line)
            .and_then(|selection| selection.permissions)
            .map(|choice| (choice.value, choice.source))
    };
    let transcript = SessionChatOptionEvidence::Transcript;
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"on-request","approvals_reviewer":"auto_review","active_permission_profile":{"id":":workspace"},"sandbox_policy":{"type":"workspace-write"}}}"#
        ),
        Some(("approve-for-me".to_string(), transcript))
    );
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"on-request","approvals_reviewer":"user","active_permission_profile":{"id":":workspace"},"sandbox_policy":{"type":"workspace-write"}}}"#
        ),
        Some(("ask-for-approval".to_string(), transcript))
    );
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"never","sandbox_policy":{"type":"danger-full-access"}}}"#
        ),
        Some(("full-access".to_string(), transcript))
    );
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"on-request","active_permission_profile":{"id":":read-only"},"sandbox_policy":{"type":"read-only"}}}"#
        ),
        Some(("read-only".to_string(), transcript))
    );
    // A profile from the user's own config keeps its name.
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"on-request","active_permission_profile":{"id":"locked-down"}}}"#
        ),
        Some(("locked-down".to_string(), transcript))
    );
    // A mix none of Codex's built-in modes describes is not guessed.
    assert_eq!(
        read(
            r#"{"type":"turn_context","payload":{"model":"gpt-6-sol","approval_policy":"never","sandbox_policy":{"type":"workspace-write"}}}"#
        ),
        None
    );
}

#[test]
fn codex_launch_flags_name_a_permission_mode_only_when_whole() {
    let read = |command: &str| {
        launch_command_selection(SessionChatOptionAgent::Codex, command)
            .and_then(|selection| selection.permissions)
            .map(|choice| choice.value)
    };
    assert_eq!(
        read("codex --model gpt-6-sol --yolo"),
        Some("full-access".to_string())
    );
    assert_eq!(
        read("codex --approve-for-me"),
        Some("approve-for-me".to_string())
    );
    assert_eq!(
        read("codex -s read-only -a on-request"),
        Some("read-only".to_string())
    );
    assert_eq!(
        read("codex --sandbox workspace-write --ask-for-approval on-request"),
        Some("ask-for-approval".to_string())
    );
    // The approval policy comes from Codex's config when only the sandbox is named.
    assert_eq!(read("codex -s read-only"), None);
    // A prompt that mentions the flag is not the flag.
    assert_eq!(read("codex 'run with --yolo'"), None);
}

#[test]
fn codex_launch_permissions_hold_until_the_transcript_names_one() {
    let launch = launch_command_selection(SessionChatOptionAgent::Codex, "codex --yolo");
    let transcript = SessionChatDetectedSelection {
        model: Some(SessionChatDetectedChoice {
            value: "gpt-6-sol".to_string(),
            label: "gpt-6-sol".to_string(),
            source: SessionChatOptionEvidence::Transcript,
        }),
        ..SessionChatDetectedSelection::default()
    };
    let merged =
        merge_session_chat_option_selections(launch, Some(transcript), None, None).expect("merged");
    assert_eq!(
        merged
            .permissions
            .map(|choice| (choice.value, choice.source)),
        Some(("full-access".to_string(), SessionChatOptionEvidence::Launch))
    );
}

#[test]
fn a_codex_title_without_its_yolo_line_rules_out_full_access() {
    // After `/clear` in Read Only: Codex redraws its title with no permissions line.
    let cleared = ">_ OpenAI Codex (v0.164.0)\n     ~/proj\n› Ask Codex to do anything\n";
    assert!(codex_title_rules_out_full_access(cleared));
    let yolo = ">_ OpenAI Codex (v0.164.0)\n  permissions: YOLO mode\n› Ask Codex to do anything\n";
    assert!(!codex_title_rules_out_full_access(yolo));
    // A change printed after the title is the newest sign.
    assert!(!codex_title_rules_out_full_access(
        CODEX_PERMISSIONS_HISTORY
    ));
    // The detector's capture keeps Codex's colours (verbatim `zmx history --vt`).
    let styled = "\u{1b}[0m\u{1b}[38;2;99;168;248m>_ \u{1b}[0m\u{1b}[1mOpenAI Codex\u{1b}[0m\u{1b}[2m (v0.164.0)\u{1b}[0m\r\n› Ask Codex to do anything\n";
    assert!(codex_title_rules_out_full_access(styled));
    // No title on screen says nothing.
    assert!(!codex_title_rules_out_full_access(
        "› Ask Codex to do anything\n"
    ));
}

/// DeepSeek Harness (dsh-tui 0.17.0), captured with `ghostex read-text` on 2026-10-28: the chat
/// page, after a reply.
const DSH_CHAT_PAGE: &str = concat!(
    "  ⏺ The command printed WORKING_CHECK.\n",
    "  ╭────────────────────────────────────────────────────────────╮\n",
    "  ⌸ ❯                                                          ⛶\n",
    "  ╰────────────────────────────────────────────────────────────╯\n",
    "                                                 9.5k/1.0M 1.0%\n",
    "   deepseek-flash · max · cache 98.3% · ≈¥0.02 peak · dsh-throwaway      ctx 1.0% (9.5k/1.0m)\n",
);

/// The launchpad a new session opened on before Ghostex turned it off, with the header line that
/// names the model too but is not the status line.
const DSH_LAUNCHPAD: &str = concat!(
    "                        deepseek-flash · Max effort\n",
    "   ╭──────────────────────────────────────────╮\n",
    "   │ ❯ Say something, or type / for commands… │\n",
    "   ╰──────────────────────────────────────────╯\n",
    "     deepseek-flash  ·  High  ·  standard  ·  Workspace read/write\n",
    "         Sessions & workspaces    Settings    Kernel · DSH    Help\n",
);

#[test]
fn deepseek_harness_status_line_names_the_model_and_level() {
    let chat = detect_session_chat_selection(SessionChatOptionAgent::Dsh, DSH_CHAT_PAGE).unwrap();
    let model = chat.model.unwrap();
    assert_eq!(model.value, "deepseek-official/deepseek-flash");
    assert_eq!(model.label, "deepseek-flash");
    assert_eq!(chat.effort.unwrap().value, "max");
    let launchpad =
        detect_session_chat_selection(SessionChatOptionAgent::Dsh, DSH_LAUNCHPAD).unwrap();
    assert_eq!(
        launchpad.model.unwrap().value,
        "deepseek-official/deepseek-flash"
    );
    assert_eq!(launchpad.effort.unwrap().value, "high");
}

#[test]
fn deepseek_harness_prose_and_menus_are_not_its_status_line() {
    for line in [
        "Sessions & workspaces    Settings    Kernel · DSH    Help",
        "deepseek-flash · Max effort",
        "We compared a · b · c in the notes",
        "deepseek-v4-pro · turbo · cache 10%",
    ] {
        assert!(
            agent_matchers::match_dsh_statusline(line).is_none(),
            "{line}"
        );
    }
    let pro = agent_matchers::match_dsh_statusline(
        " deepseek-v4-pro · off · cache 0.0% · ≈¥0.00 peak · repo",
    )
    .unwrap();
    assert_eq!(
        pro.model.unwrap().value,
        "deepseek-official/deepseek-v4-pro"
    );
    assert_eq!(pro.effort.unwrap().value, "off");
}

/// A new Codex 0.164 session whose config names no reasoning effort, as it painted on Linux.
#[test]
fn a_codex_footer_with_the_default_effort_still_names_its_model() {
    let screen = ">_ OpenAI Codex (v0.164.0)\n     ~/Projects/ftue-demo\n› Ask Codex to do anything\n  GPT-6.1-Sol default · ~/Projects/ftue-demo\n  ? for shortcuts\n";
    let detected = detect_session_chat_selection(SessionChatOptionAgent::Codex, screen)
        .expect("codex selection");
    assert_eq!(
        detected.model.map(|choice| choice.value),
        Some("gpt-6.1-sol".to_string())
    );
    assert_eq!(detected.effort, None);
    let fast = "› Ask Codex to do anything\n  GPT-6.1-Sol default fast · ~/proj\n";
    let detected = detect_session_chat_selection(SessionChatOptionAgent::Codex, fast)
        .expect("codex selection with fast");
    assert_eq!(detected.fast, Some(true));
}
