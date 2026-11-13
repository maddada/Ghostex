//! Claude's tab of the model picker: the newest model of each family leads (Fable, Opus, Sonnet,
//! Haiku), every older version sits only in the Legacy models list whether or not it is starred,
//! and a new Fable pushes the last one into Legacy models with no edit to its row.

use ghostex_gx_chat_core::menus::catalog::{parse_agent_model_catalog, LEGACY_GROUP_ID};
use ghostex_gx_chat_core::menus::picker::model_menu::{
    model_menu_entries, model_menu_legacy_count, model_menu_rows, ModelChoice, ModelMenuCatalog,
    ModelMenuCatalogs, ModelMenuCurrent, ModelMenuTabId,
};
use ghostex_gx_chat_core::menus::picker::ModelPickerProvider;
use serde_json::{json, Value};

const CATALOG: &str = include_str!("../../../agent-model-catalog.json");
const CLAUDE: ModelMenuTabId = ModelMenuTabId::Provider(ModelPickerProvider::Claude);

/// The Claude tab's rows as "label ⌥n", with "★" on a starred row and "legacy" on one inside the
/// Legacy models list.
fn claude_rows(
    document: &Value,
    favorites: &[&str],
    current: &str,
    legacy_open: bool,
) -> Vec<String> {
    let catalog = parse_agent_model_catalog(document).expect("catalog parses");
    let claude = &catalog.agents["claude"];
    let mut catalogs = ModelMenuCatalogs::new();
    catalogs.insert(
        "claude".into(),
        ModelMenuCatalog {
            model_icon: "claude.svg".into(),
            choices: claude
                .models
                .iter()
                .map(|model| ModelChoice {
                    value: model.value.clone(),
                    label: model.label.clone(),
                    legacy: model.group.as_deref() == Some(LEGACY_GROUP_ID),
                    ..ModelChoice::default()
                })
                .collect(),
            agent_name: None,
        },
    );
    let entries = model_menu_entries(&catalogs);
    let favorites: Vec<String> = favorites.iter().map(|key| (*key).to_string()).collect();
    let rows = model_menu_rows(
        &entries,
        CLAUDE,
        "",
        &favorites,
        &ModelMenuCurrent {
            provider: Some("claude".into()),
            model: Some(current.into()),
        },
        legacy_open,
    );
    let mut shown: Vec<String> = rows
        .into_iter()
        .map(|row| {
            let mut text = row.entry.label.clone();
            if let Some(key) = row.shortcut {
                text.push_str(&format!(" ⌥{key}"));
            }
            if row.favorite {
                text.push_str(" ★");
            }
            if row.entry.legacy {
                text.push_str(" legacy");
            }
            text
        })
        .collect();
    shown.push(format!(
        "Legacy models ({})",
        model_menu_legacy_count(&entries, CLAUDE, "")
    ));
    shown
}

#[test]
fn a_starred_or_running_older_fable_stays_in_legacy_models() {
    let document: Value = serde_json::from_str(CATALOG).unwrap();
    // The stars saved on the user's Mac on 2026-11-12, with a session still running Fable 5.1.
    let favorites = ["claude:fable", "claude:opus[1m]"];
    assert_eq!(
        claude_rows(&document, &favorites, "fable", false),
        [
            "Opus 5.5 ⌥1 ★",
            "Fable 5.2 ⌥2",
            "Sonnet 5.5 ⌥3",
            "Haiku 5.5 ⌥4",
            "Legacy models (9)",
        ]
    );
    assert_eq!(
        claude_rows(&document, &[], "opus[1m]", false),
        [
            "Fable 5.2 ⌥1",
            "Opus 5.5 ⌥2",
            "Sonnet 5.5 ⌥3",
            "Haiku 5.5 ⌥4",
            "Legacy models (9)",
        ]
    );
    assert_eq!(
        claude_rows(&document, &favorites, "fable", true),
        [
            "Opus 5.5 ⌥1 ★",
            "Fable 5.2 ⌥2",
            "Sonnet 5.5 ⌥3",
            "Haiku 5.5 ⌥4",
            "Fable 5.1 ⌥5 ★ legacy",
            "Fable 5 ⌥6 legacy",
            "Opus 5 ⌥7 legacy",
            "Opus 4.8 ⌥8 legacy",
            "Opus 4.7 ⌥9 legacy",
            "Opus 4.6 legacy",
            "Sonnet 5 legacy",
            "Sonnet 4.6 legacy",
            "Haiku 4.5 legacy",
            "Legacy models (9)",
        ]
    );
}

#[test]
fn a_new_fable_moves_the_last_one_into_legacy_models() {
    let mut document: Value = serde_json::from_str(CATALOG).unwrap();
    let models = document["agents"]["claude"]["models"]
        .as_array_mut()
        .unwrap();
    // Fable 5.1 loses its hand-set group too: the rule needs none.
    for model in models.iter_mut() {
        model.as_object_mut().unwrap().remove("group");
    }
    models.push(json!({"value": "claude-fable-6", "label": "Fable 6"}));
    assert_eq!(
        claude_rows(&document, &[], "opus[1m]", true),
        [
            "Fable 6 ⌥1",
            "Opus 5.5 ⌥2",
            "Sonnet 5.5 ⌥3",
            "Haiku 5.5 ⌥4",
            "Fable 5.2 ⌥5 legacy",
            "Fable 5.1 ⌥6 legacy",
            "Fable 5 ⌥7 legacy",
            "Opus 5 ⌥8 legacy",
            "Opus 4.8 ⌥9 legacy",
            "Opus 4.7 legacy",
            "Opus 4.6 legacy",
            "Sonnet 5 legacy",
            "Sonnet 4.6 legacy",
            "Haiku 4.5 legacy",
            "Legacy models (10)",
        ]
    );
}
