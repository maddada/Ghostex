//! The model picker's search field and starred rows: a tab with more than six models can be
//! filtered by label or id, and starred models sit alphabetically at the top.

use ghostex_gx_chat_core::menus::picker::model_menu::{
    model_menu_empty_text, model_menu_entries, model_menu_rows, model_menu_search_enabled,
    ModelChoice, ModelMenuCatalog, ModelMenuCatalogs, ModelMenuCurrent, ModelMenuEntries,
    ModelMenuTabId,
};
use ghostex_gx_chat_core::menus::picker::ModelPickerProvider;

fn catalog(models: &[(&str, &str)]) -> ModelMenuCatalog {
    ModelMenuCatalog {
        model_icon: "agent.svg".into(),
        choices: models
            .iter()
            .map(|(value, label)| ModelChoice {
                value: (*value).into(),
                label: (*label).into(),
                ..ModelChoice::default()
            })
            .collect(),
        agent_name: None,
    }
}

/// OpenCode with nine models, Claude with three.
fn entries() -> ModelMenuEntries {
    let mut catalogs = ModelMenuCatalogs::new();
    catalogs.insert(
        "opencode".into(),
        catalog(&[
            ("auto", "Auto"),
            ("anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ("anthropic/claude-opus-5-5", "Claude Opus 5.5"),
            ("openai/gpt-6-luna", "GPT-6 Luna"),
            ("openai/gpt-6-sol", "GPT-6 Sol"),
            ("google/gemini-4-pro", "Gemini 4 Pro"),
            ("xai/grok-5", "Grok 5"),
            ("zai/glm-5", "GLM 5"),
            ("deepseek/deepseek-v4", "DeepSeek V4"),
        ]),
    );
    catalogs.insert(
        "claude".into(),
        catalog(&[("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")]),
    );
    model_menu_entries(&catalogs)
}

const OPENCODE: ModelMenuTabId = ModelMenuTabId::Provider(ModelPickerProvider::OpenCode);
const CLAUDE: ModelMenuTabId = ModelMenuTabId::Provider(ModelPickerProvider::Claude);

fn labels(tab: ModelMenuTabId, query: &str, favorites: &[&str]) -> Vec<String> {
    let favorites: Vec<String> = favorites.iter().map(|key| (*key).to_string()).collect();
    model_menu_rows(
        &entries(),
        tab,
        query,
        &favorites,
        &ModelMenuCurrent::default(),
        false,
    )
    .into_iter()
    .map(|row| row.entry.label)
    .collect()
}

#[test]
fn search_field_shows_only_on_tabs_with_more_than_six_models() {
    let entries = entries();
    assert!(model_menu_search_enabled(&entries, OPENCODE, &[]));
    assert!(!model_menu_search_enabled(&entries, CLAUDE, &[]));
    // The favorites tab counts its starred models.
    let three: Vec<String> = ["claude:opus", "claude:sonnet", "opencode:xai/grok-5"]
        .map(String::from)
        .to_vec();
    assert!(!model_menu_search_enabled(
        &entries,
        ModelMenuTabId::Favorites,
        &three
    ));
}

#[test]
fn filter_matches_label_and_id_case_insensitively() {
    assert_eq!(
        labels(OPENCODE, "SONNET", &[]),
        vec!["Claude Sonnet 5.5".to_string()]
    );
    // Each word must appear, so "gpt 6" finds "GPT-6 …" and nothing else.
    assert_eq!(
        labels(OPENCODE, "gpt 6", &[]),
        vec!["GPT-6 Luna".to_string(), "GPT-6 Sol".to_string()]
    );
    // An id-only match ("google/…") still lists the row.
    assert_eq!(
        labels(OPENCODE, "google", &[]),
        vec!["Gemini 4 Pro".to_string()]
    );
    // A label prefix ranks above a match further in.
    assert_eq!(
        labels(OPENCODE, "claude", &[])[..2],
        [
            "Claude Sonnet 5.5".to_string(),
            "Claude Opus 5.5".to_string()
        ]
    );
}

#[test]
fn no_match_lists_nothing_and_says_so() {
    assert!(labels(OPENCODE, "zzz", &[]).is_empty());
    assert_eq!(model_menu_empty_text(OPENCODE, "zzz"), "No models match");
}

#[test]
fn starred_models_come_first_alphabetically_then_catalog_order() {
    let rows = labels(
        OPENCODE,
        "",
        &[
            "opencode:xai/grok-5",
            "opencode:anthropic/claude-sonnet-5-5",
            "opencode:deepseek/deepseek-v4",
        ],
    );
    assert_eq!(
        rows,
        [
            "Claude Sonnet 5.5",
            "DeepSeek V4",
            "Grok 5",
            "Auto",
            "Claude Opus 5.5",
            "GPT-6 Luna",
            "GPT-6 Sol",
            "Gemini 4 Pro",
            "GLM 5",
        ]
        .map(String::from)
    );
}

#[test]
fn matching_stars_stay_first_while_filtering() {
    // The starred GPT-6 Sol leads the matches although the catalog lists Luna first.
    let rows = labels(OPENCODE, "gpt", &["opencode:openai/gpt-6-sol"]);
    assert_eq!(rows, ["GPT-6 Sol", "GPT-6 Luna"].map(String::from));
}

#[test]
fn favorites_tab_is_alphabetical() {
    let rows = labels(
        ModelMenuTabId::Favorites,
        "",
        &["opencode:xai/grok-5", "claude:sonnet", "claude:opus"],
    );
    assert_eq!(rows, ["Grok 5", "Opus", "Sonnet"].map(String::from));
}
