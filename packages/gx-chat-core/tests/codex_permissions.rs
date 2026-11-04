use ghostex_gx_chat_core::menus::{
    catalog::parse_agent_model_catalog,
    option_catalog::{session_option_catalog, CODEX_PERMISSIONS_ID},
    option_dispatch::{queue_session_chat_option, QueuedOption},
    option_menus::{native_option_menus, OptionMenuParams},
    option_pills::option_pill_values,
    option_values::{apply_detected_options, DetectedOptions, OptionState},
};
use serde_json::json;

fn codex_catalog() -> ghostex_gx_chat_core::menus::catalog::AgentModelCatalog {
    parse_agent_model_catalog(&json!({"schemaVersion":1,"updatedAt":"2026-10-27","effortLabels":{},"agents":{"codex":{"name":"Codex","efforts":["low","high"],"models":[{"value":"gpt-6-sol","label":"GPT-6-Sol","efforts":["low","high"]}]}}})).unwrap()
}

#[test]
fn codex_mode_button_lists_codex_permission_modes_and_checks_the_detected_one() {
    let model_catalog = codex_catalog();
    let catalog = session_option_catalog(&model_catalog, Some("codex")).unwrap();
    let descriptors = catalog.options_for_model("gpt-6-sol");
    let detected = DetectedOptions::from_value(&json!({
        "model": {"value": "gpt-6-sol", "label": "GPT-6-Sol", "source": "terminal"},
        "permissions": {"value": "read-only", "label": "Read Only", "source": "terminal"},
        "detectedAt": "2026-10-27T10:00:00.000Z",
    }))
    .unwrap();
    let state = apply_detected_options(&catalog, &OptionState::new(), Some(&detected));
    let menus = native_option_menus(
        Some(&catalog),
        &state,
        &descriptors,
        &OptionMenuParams::default(),
    );
    let rows = serde_json::to_value(&menus.mode).unwrap();
    assert_eq!(rows[0]["label"], "Mode");
    assert_eq!(
        rows[0]["description"],
        "Choose what Codex is allowed to do."
    );
    let labels: Vec<&str> = rows.as_array().unwrap()[1..]
        .iter()
        .map(|row| row["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Ask for approval",
            "Approve for me",
            "Full Access",
            "Read Only"
        ]
    );
    let checked: Vec<&str> = rows.as_array().unwrap()[1..]
        .iter()
        .filter(|row| row["checked"] == true)
        .map(|row| row["label"].as_str().unwrap())
        .collect();
    assert_eq!(checked, ["Read Only"]);
    assert_eq!(
        rows[2]["description"],
        "Only ask for actions detected as potentially unsafe"
    );
    // The permission mode lives on the Mode button, not in the options pill.
    let options = serde_json::to_value(&menus.options).unwrap();
    assert!(!options.to_string().contains("Ask for approval"));
    let pills = option_pill_values(&model_catalog, Some(&catalog), &descriptors, &state);
    assert_eq!(pills.mode.as_deref(), Some("Read Only"));
    assert_eq!(pills.mode_value.as_deref(), Some("read-only"));
    // Plan mode stays a toggle in the options pill.
    assert!(options.to_string().contains("Plan mode"));
}

#[test]
fn picking_a_codex_mode_queues_it_for_gxserver() {
    let catalog = session_option_catalog(&codex_catalog(), Some("codex")).unwrap();
    let descriptor = catalog
        .options_for_model("gpt-6-sol")
        .into_iter()
        .find(|descriptor| descriptor.id == CODEX_PERMISSIONS_ID)
        .unwrap();
    assert_eq!(
        queue_session_chat_option(
            &descriptor,
            Some("full-access"),
            Some(&catalog),
            &OptionState::new(),
            true,
            true,
            None,
        ),
        Some(QueuedOption::SelectOptions {
            mode: None,
            fast_mode: None,
            permissions: Some("full-access".to_string()),
        })
    );
}
