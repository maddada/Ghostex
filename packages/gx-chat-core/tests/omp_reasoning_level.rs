use ghostex_gx_chat_core::menus::{
    catalog::{parse_agent_model_catalog, AgentModelCatalog},
    option_catalog::session_option_catalog,
    option_store::OptionStore,
    option_values::{apply_detected_options, option_value_label, DetectedOptions, OptionState},
    picker::{
        inputs::menu_inputs,
        model_menu::{ModelChoice, ModelMenuCatalog, ModelMenuCatalogs, ModelMenuView},
        model_picker::{model_pick_scope, ModelPickerProvider, ModelPickerSelection},
        projection::{
            model_menu_pick, model_menu_projection, model_menu_visible_rows, ModelMenuContext,
            ModelMenuPick,
        },
        selection::model_selection_unchanged,
        traits::{OptionChoice, OptionRows, ResolvedOptionDescriptor},
    },
};
use ghostex_gx_chat_core::{ChatContext, ChatState, Tri};
use serde_json::{json, Value};

const HAIKU: &str = "anthropic/claude-haiku-5-5";

/// OMP's lineup as gxserver publishes it (Claude Haiku 5.5 offers low to max, so the middle of its
/// list is high), with Claude Code and Codex to cover both model-switch level policies.
fn catalog() -> AgentModelCatalog {
    let levels = ["low", "medium", "high", "max"];
    parse_agent_model_catalog(&json!({"schemaVersion":1,"updatedAt":"2026-11-03","effortLabels":{},"agents":{
        "omp":{"name":"OMP","efforts":["minimal","low","medium","high","xhigh","max"],"models":[
            {"value":HAIKU,"label":"Claude Haiku 5.5","efforts":levels},
            {"value":"anthropic/claude-sonnet-5-5","label":"Claude Sonnet 5.5","efforts":levels}
        ]},
        "claude":{"name":"Claude Code","efforts":levels,"models":[{"value":"opus","label":"Opus","efforts":levels}]},
        "codex":{"name":"Codex","efforts":levels,"models":[{"value":"gpt-6.1-sol","label":"GPT-6.1 Sol","efforts":levels}]}
    }}))
    .unwrap()
}

/// The model menu's context for a session of `agent` whose detection reported `detected` (the
/// `selectedOptions` gxserver publishes), resolved the way `picker::inputs` resolves it.
fn context(
    catalog: &AgentModelCatalog,
    agent: ModelPickerProvider,
    detected: Value,
) -> ModelMenuContext {
    let options = session_option_catalog(catalog, Some(agent.as_str())).unwrap();
    let state = apply_detected_options(
        &options,
        &OptionState::new(),
        DetectedOptions::from_value(&detected).as_ref(),
    );
    let value = |id: &str| state.get(id).map(|entry| entry.value.clone());
    let model_value = value(&options.model.id);
    let descriptors = options
        .options_for_model(model_value.as_deref().unwrap_or_default())
        .iter()
        .map(|descriptor| ResolvedOptionDescriptor {
            id: descriptor.id.clone(),
            label: descriptor.label.clone(),
            action_label: descriptor.action_label.clone(),
            default_value: descriptor.default_value.clone(),
            held_value: value(&descriptor.id),
            value_label: option_value_label(catalog, descriptor, &state),
            rows: OptionRows::Choices {
                choices: descriptor
                    .choice_list()
                    .iter()
                    .map(|choice| OptionChoice {
                        value: choice.value.clone(),
                        label: choice.label.clone(),
                    })
                    .collect(),
                current: value(&descriptor.id),
            },
        })
        .collect();
    ModelMenuContext {
        provider: Some(agent),
        model_id: Some(options.model.id.clone()),
        descriptors,
        effort_value: value("effort"),
        model_value,
        ..ModelMenuContext::default()
    }
}

fn catalogs(catalog: &AgentModelCatalog) -> ModelMenuCatalogs {
    [
        ModelPickerProvider::Omp,
        ModelPickerProvider::Claude,
        ModelPickerProvider::Codex,
    ]
    .into_iter()
    .map(|provider| {
        let options = session_option_catalog(catalog, Some(provider.as_str())).unwrap();
        let choices = options
            .model
            .choice_list()
            .iter()
            .map(|choice| ModelChoice {
                value: choice.value.clone(),
                label: choice.label.clone(),
                description: choice.description.clone(),
                legacy: false,
            })
            .collect();
        (
            provider.as_str().to_string(),
            ModelMenuCatalog {
                model_icon: options.model_icon.clone(),
                choices,
                agent_name: None,
            },
        )
    })
    .collect()
}

/// The `modelMenu` document key both renderers draw.
fn menu(catalog: &AgentModelCatalog, context: &ModelMenuContext) -> Value {
    model_menu_projection(
        context,
        &ModelMenuView::default(),
        &catalogs(catalog),
        &[],
        catalog,
    )
}

fn own_row(menu: &Value) -> &Value {
    menu["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["selected"] == true)
        .expect("the session's own model is listed")
}

fn reasoning(menu: &Value) -> &Value {
    menu["traits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "effort")
        .expect("the Reasoning button is drawn")
}

fn marked_levels(menu: &Value) -> Vec<&str> {
    reasoning(menu)["choices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|choice| choice["selected"] == true)
        .map(|choice| choice["value"].as_str().unwrap())
        .collect()
}

/// gxserver reads OMP's level from its transcript (`thinking_level_change`) after a typed
/// `/switch <model>:low` or a pick, and the menu shows that level as the one OMP runs at.
#[test]
fn omp_menu_shows_the_level_omps_transcript_reports() {
    let catalog = catalog();
    let context = context(
        &catalog,
        ModelPickerProvider::Omp,
        json!({
            "model": {"value": HAIKU, "label": "Claude Haiku 5.5", "source": "transcript"},
            "effort": {"value": "low", "label": "low", "source": "transcript"},
            "detectedAt": "2026-11-03T08:37:20.000Z",
        }),
    );
    let menu = menu(&catalog, &context);
    assert_eq!(own_row(&menu)["effort"], "low");
    assert_eq!(reasoning(&menu)["valueLabel"], "Low");
    assert_eq!(marked_levels(&menu), ["low"]);
    assert_eq!(menu["pill"]["suffix"], "Low");
}

/// Agents that keep their own level do not mark an unknown level or request a replacement level
/// when their own model row is picked without browsing one.
#[test]
fn agents_that_keep_effort_mark_no_level_while_theirs_is_unknown() {
    let catalog = catalog();
    for (provider, model, label) in [
        (ModelPickerProvider::Omp, HAIKU, "Claude Haiku 5.5"),
        (ModelPickerProvider::Claude, "opus", "Opus"),
    ] {
        let context = context(
            &catalog,
            provider,
            json!({
                "model": {"value": model, "label": label, "source": "transcript"},
                "detectedAt": "2026-11-03T08:37:20.000Z",
            }),
        );
        let menu = menu(&catalog, &context);
        let row = own_row(&menu);
        assert_eq!(row["effort"], "", "{provider:?}");
        assert_eq!(
            row["efforts"].as_array().unwrap().len(),
            4,
            "the levels stay pickable"
        );
        assert!(marked_levels(&menu).is_empty());
        assert_eq!(reasoning(&menu)["valueLabel"], Value::Null);
        assert_eq!(menu["pill"]["suffix"], Value::Null);

        // The phone's apply and the desktop's Enter send the row's own level when none was browsed.
        let rows = model_menu_visible_rows(
            &context,
            &ModelMenuView::default(),
            &catalogs(&catalog),
            &[],
        );
        let selected = rows.iter().find(|row| row.selected).unwrap();
        let pick = model_menu_pick(
            selected,
            &context,
            Some(row["effort"].as_str().unwrap().to_string()),
            |_, _, _| unreachable!("the session's own agent is no hand-off"),
        );
        let ModelMenuPick::Select {
            value,
            effort: Some(effort),
        } = pick
        else {
            panic!("a level-carrying pick of the session's own model: {pick:?}");
        };
        assert!(effort.is_empty(), "{provider:?}");
        // OMP's unchanged pick is a no-op; Claude's default scope can still update its saved model.
        if provider == ModelPickerProvider::Omp {
            assert!(model_selection_unchanged(
                &ModelPickerSelection {
                    model: value,
                    effort
                },
                None,
                context.model_value.as_deref(),
                context.effort_value.as_deref(),
                Some(provider),
                false,
                Some(model_pick_scope(Some(provider), false)),
            ));
        }
    }
}

/// An agent whose model switch sets a level keeps its own row's start when its level is unknown.
#[test]
fn codex_keeps_its_start_level_when_its_level_is_unknown() {
    let catalog = catalog();
    let context = context(
        &catalog,
        ModelPickerProvider::Codex,
        json!({
            "model": {"value": "gpt-6.1-sol", "label": "GPT-6.1 Sol", "source": "terminal"},
            "detectedAt": "2026-11-03T08:37:20.000Z",
        }),
    );
    let menu = menu(&catalog, &context);
    let row = own_row(&menu);
    let effort = row["effort"].as_str().unwrap();
    assert!(
        row["efforts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|level| level["value"] == effort),
        "Codex's own row starts on one of its levels, got {effort:?}"
    );
    assert_eq!(
        effort, "high",
        "the middle level remains the starting level"
    );
}

/// An open OMP chat as the core holds it: the session's option store, fed one gxserver reading
/// after another, and the model menu built from it the way every publish builds it.
struct OpenChat {
    state: ChatState,
}

impl OpenChat {
    fn omp() -> Self {
        let mut state = ChatState::default();
        state.session.agent = Some("omp".to_string());
        // gxserver publishes `pendingModelSelection` (null when none), which is what lets the chat
        // pick models at all.
        state.session.pending_model_selection = Tri::Null;
        state.menus.model_catalog = catalog();
        let options = session_option_catalog(&state.menus.model_catalog, Some("omp")).unwrap();
        state.menus.options = OptionStore::seeded(Some(&options), None, &OptionState::new(), 0);
        Self { state }
    }

    /// gxserver's `selectedOptions` for this session at `second`, naming `level` from `source`.
    fn read(&mut self, level: &str, source: &str, second: u8) {
        let options = session_option_catalog(&self.state.menus.model_catalog, Some("omp")).unwrap();
        let detected = json!({
            "model": {"value": HAIKU, "label": "Claude Haiku 5.5", "source": "transcript"},
            "effort": {"value": level, "label": level, "source": source},
            "detectedAt": format!("2026-11-03T10:00:{second:02}.000Z"),
        });
        self.state.menus.options.apply_detected(
            Some(&options),
            DetectedOptions::from_value(&detected).as_ref(),
        );
    }

    fn menu(&self) -> Value {
        let inputs = menu_inputs(&self.state, &ChatContext::at(0.0));
        model_menu_projection(
            &inputs.menu.expect("OMP is in the model catalog"),
            &ModelMenuView::default(),
            &inputs.catalogs,
            &[],
            &self.state.menus.model_catalog,
        )
    }
}

/// `/effort auto` after a fixed Low read from OMP's transcript: gxserver reports Auto as `auto`,
/// and Low is no longer marked or named as OMP's level.
#[test]
fn auto_after_a_fixed_level_unmarks_the_old_level_in_an_open_chat() {
    let mut chat = OpenChat::omp();
    chat.read("low", "transcript", 0);
    assert_eq!(own_row(&chat.menu())["effort"], "low");
    chat.read("auto", "transcript", 1);
    let menu = chat.menu();
    assert_eq!(own_row(&menu)["effort"], "");
    assert!(marked_levels(&menu).is_empty());
    assert_eq!(reasoning(&menu)["valueLabel"], "Auto");
    assert_eq!(menu["pill"]["suffix"], "Auto");
}

/// Terminal evidence keeps the same priority over transcript evidence as for every other agent.
#[test]
fn a_terminal_level_keeps_priority_over_a_later_transcript_level() {
    let mut chat = OpenChat::omp();
    chat.read("low", "terminal", 0);
    assert_eq!(own_row(&chat.menu())["effort"], "low");
    chat.read("max", "transcript", 1);
    let menu = chat.menu();
    assert_eq!(own_row(&menu)["effort"], "low");
    assert_eq!(marked_levels(&menu), ["low"]);
    assert_eq!(reasoning(&menu)["valueLabel"], "Low");
    chat.read("max", "terminal", 2);
    assert_eq!(own_row(&chat.menu())["effort"], "max");
    // An older terminal reading arriving late does not undo the newer one.
    chat.read("low", "terminal", 0);
    assert_eq!(own_row(&chat.menu())["effort"], "max");
}
