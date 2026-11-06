use super::*;

#[test]
fn format_help_command_pads_to_58_columns_with_min_gap() {
    let line = format_help_command("state | dump-state", "Print sidebar state as JSON");
    assert_eq!(
        line,
        format!(
            "  state | dump-state{}Print sidebar state as JSON",
            " ".repeat(58 - "state | dump-state".len())
        )
    );
    // Signatures longer than the column keep a minimum two-space gap.
    let long = "wait-for-text <selector> <regex> [--timeout-seconds n] [--interval-seconds n] [--lines n] [--json]";
    let long_line = format_help_command(long, "desc");
    assert_eq!(long_line, format!("  {long}  desc"));
}

#[test]
fn usage_contains_expected_sections_and_tab_indented_usage_lines() {
    let text = usage();
    assert!(text.starts_with("Ghostex CLI - manage running Ghostex terminal sessions\n"));
    assert!(text.contains("\n\t  ghostex\n\t  gx\n\t  ghostex <path...>\n"));
    for section in [
        "\nCommands:\n",
        "\nWorkspace:\n",
        "\nAutomations:\n",
        "\nInput:\n",
        "\nUI:\n",
        "\nServer:\n",
        "\nEvidence:\n",
        "\nSelectors:\n",
        "\nSessions:\n",
        "\nAttach:\n",
        "\nGlobal flags:\n",
    ] {
        assert!(text.contains(section), "missing section {section:?}");
    }
    assert!(text.contains("automations --help"));
    assert!(text.contains("cli --help"));
    assert!(!text.contains("tui"));
    assert!(text.contains("no subcommand launches or activates the Ghostex desktop app"));
    assert!(!text.contains("gx 2"));
    assert!(text.contains("automation-mark-run-read --run-id id --path path"));
    assert!(text.ends_with("  -h, --help            Show this help\n"));
}

#[test]
fn focused_automation_help_documents_skill_and_definition_contract() {
    let text = automations_usage();
    assert!(!text.contains("install-skill"));
    assert!(text.contains("automation-save --path path --definition-json json"));
    assert!(text.contains("Repeating definition JSON:"));
    assert!(text.contains(r#"{"kind":"timer","delayMs":1800000}"#));
    assert!(text.contains(r#"{"kind":"once","runAt":"2026-08-14T09:30:00.000Z"}"#));
    assert!(text.contains("AUTOMATION_RESULT"));
    assert!(text.contains("delayed-send"));
}

#[test]
fn cli_skill_help_routes_to_specialized_workflows() {
    let text = cli_usage();
    assert!(text.contains("gx cli install-skill [--json]"));
    assert!(!text.contains("$ghostex-manage-automations"));
    assert!(!text.contains("$ghostex-agent-orchestration"));
    assert!(text.contains("$ghostex-manage-beads"));
    assert!(text.contains("$ghostex-embedded-browser-use"));
}

#[test]
fn generate_title_usage_keeps_literal_shell_parameter_expansion() {
    assert!(generate_title_usage().contains(
        "ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\""
    ));
}
