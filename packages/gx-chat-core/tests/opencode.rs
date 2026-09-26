use ghostex_gx_chat_core::menus::{
    catalog::parse_agent_model_catalog,
    option_catalog::{OptionDispatch, session_option_catalog},
    picker::{
        model_picker::{ModelPickerProvider, ModelSelectionScope, model_pick_scope},
        request::create_model_picker_request,
    },
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
    let request = create_model_picker_request(
        &catalog,
        ModelPickerProvider::OpenCode,
        Some("provider/model"),
        Some("high"),
        "test".into(),
    )
    .unwrap();
    assert_eq!(request.model, "provider/model");
    assert_eq!(request.effort, "high");
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
