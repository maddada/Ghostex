use ghostex_gx_chat_core::menus::{
    catalog::parse_agent_model_catalog,
    option_catalog::{session_option_catalog, OptionDispatch},
    picker::model_picker::{model_pick_scope, ModelPickerProvider, ModelSelectionScope},
};
use ghostex_gx_chat_core::questions::model::InteractivePrompt;
use serde_json::json;

#[test]
fn live_catalog_drives_opencode_model_effort_and_agent_controls() {
    assert_eq!(
        serde_json::to_value(ModelPickerProvider::OpenCode).unwrap(),
        "opencode"
    );
    assert_eq!(
        serde_json::from_value::<ModelPickerProvider>(json!("opencode")).unwrap(),
        ModelPickerProvider::OpenCode
    );
    let catalog=parse_agent_model_catalog(&json!({"schemaVersion":1,"updatedAt":"2026-09-25","effortLabels":{},"agents":{"opencode":{"name":"OpenCode","efforts":["low","high"],"models":[{"value":"provider/model","label":"Test model","efforts":["low","high"]}]}}})).unwrap();
    let options = session_option_catalog(&catalog, Some("opencode")).unwrap();
    assert!(matches!(
        options.model.dispatch,
        OptionDispatch::ModelPicker
    ));
    let rows = options.options_for_model("provider/model");
    assert!(rows.iter().any(|row| row.id == "mode"));
    assert!(rows.iter().any(|row| row.id == "effort"));
    assert_eq!(
        model_pick_scope(Some(ModelPickerProvider::OpenCode), false),
        ModelSelectionScope::Default
    );
    assert_eq!(
        model_pick_scope(Some(ModelPickerProvider::OpenCode), true),
        ModelSelectionScope::Session
    );
}

#[test]
fn editing_a_draft_preserves_its_revision_for_reload_and_recovery() {
    use ghostex_gx_chat_core::composer::{actions, storage::decode_stored_draft};
    use ghostex_gx_chat_core::{ChatContext, ChatState, Effect, UserAction};
    let mut state = ChatState::default();
    state.identity.session_key = "project:session".into();
    let action:UserAction=serde_json::from_value(json!({"type":"editDraft","text":"unsent text","draftVersion":{"draftId":"draft-one","revision":7}})).unwrap();
    let effects = actions::handle(
        &mut state,
        &action,
        &ChatContext {
            now_ms: 1234.0,
            ..Default::default()
        },
    );
    let Effect::WriteStorage {
        key,
        value: Some(value),
        ..
    } = &effects[0]
    else {
        panic!("draft write missing")
    };
    assert_eq!(key.suffix, "project:session");
    let record = decode_stored_draft(value);
    assert_eq!(record.text, "unsent text");
    assert_eq!(record.version.unwrap().revision, 7);
    assert_eq!(record.updated_at, Some(1234.0));
    assert!(!record.submitted);
}

#[test]
fn question_and_permission_identities_survive_the_wire_contract() {
    for prompt in [
        json!({"kind":"question","questions":[{"question":"Choose","options":[{"label":"Yes"}]}],"toolUseId":"frm_exact"}),
        json!({"kind":"approval","tool":"edit","toolUseId":"per_exact"}),
    ] {
        let parsed = InteractivePrompt::parse(Some(&prompt)).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["toolUseId"],
            prompt["toolUseId"]
        );
    }
}

#[test]
fn the_bundled_catalog_offers_deepseek_harness_routes_and_levels() {
    use ghostex_gx_chat_core::menus::{
        catalog::bundled_agent_model_catalog,
        picker::model_picker::{model_pick_scope, model_picker_provider, ModelPickerProvider},
    };
    let catalog = bundled_agent_model_catalog().expect("the bundled catalog parses");
    let options = session_option_catalog(&catalog, Some("dsh")).unwrap();
    assert!(matches!(
        options.model.dispatch,
        OptionDispatch::ModelPicker
    ));
    let values: Vec<String> = options
        .model
        .choices
        .as_ref()
        .unwrap()
        .iter()
        .map(|choice| choice.value.clone())
        .collect();
    assert_eq!(values[0], "deepseek-official/deepseek-flash");
    assert!(values.contains(&"deepseek-account/deepseek-flash".to_string()));
    let effort = options
        .options_for_model("deepseek-official/deepseek-flash")
        .into_iter()
        .find(|row| row.id == "effort")
        .expect("an effort picker");
    let levels: Vec<String> = effort
        .choice_list()
        .iter()
        .map(|choice| choice.value.clone())
        .collect();
    assert_eq!(levels, ["off", "low", "high", "max"]);
    assert!(options.agent_keeps_effort());
    assert_eq!(
        model_picker_provider(Some("dsh")),
        Some(ModelPickerProvider::Dsh)
    );
    assert_eq!(
        model_pick_scope(Some(ModelPickerProvider::Dsh), true),
        ModelSelectionScope::Default
    );
}
