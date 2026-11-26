//! A workspace's rules for agent calls that would send, post, create, change or delete
//! something: one for every such call (Ask, Allow, Never) and the tools with their own rule.
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    ButtonVariant, settings_icon_button, settings_list_item, settings_section, settings_segmented,
};
use super::super::super::palette::SettingsPalette;
use super::ConnectorsTab;
use super::data::text;
use super::widgets::detail;
use gpui::{
    AnyElement, Context, Div, IntoElement, ParentElement as _, SharedString, Styled as _, px,
};
use gpui_component::h_flex;
use serde_json::Value;

fn rule_options() -> Vec<SettingOption> {
    [("ask", "Ask"), ("allow", "Allow"), ("never", "Never")]
        .into_iter()
        .map(|(value, label)| SettingOption {
            label: label.to_string(),
            value: value.to_string(),
        })
        .collect()
}

fn explain(rule: &str) -> &'static str {
    match rule {
        "allow" => "Runs without asking.",
        "never" => "Never runs, so agents here can only read.",
        _ => "Ghostex asks on a card in the agent's chat first.",
    }
}

/// `composio:GMAIL_SEND_EMAIL` as (`GMAIL_SEND_EMAIL`, `Composio`).
fn tool_label(key: &str) -> (String, &'static str) {
    match key.split_once(':') {
        Some(("composio", tool)) => (tool.to_string(), "Composio"),
        Some(("executor", tool)) => (tool.to_string(), "Executor"),
        _ => (key.to_string(), ""),
    }
}

impl ConnectorsTab {
    pub(super) fn rules_section(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        title: String,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let rules = workspace.get("rules")?;
        let id = text(workspace, "workspaceId");
        let options = rule_options();
        let default = Some(text(rules, "default"))
            .filter(|rule| !rule.is_empty())
            .unwrap_or_else(|| "ask".to_string());
        let target = id.clone();
        let mut rows: Vec<AnyElement> = vec![settings_list_item(
            p,
            None,
            None,
            "Every call that changes something",
            Some(detail(explain(&default))),
            Some(settings_segmented(
                p,
                &format!("connectors-{id}-rule-all"),
                &options,
                Some(default.as_str()),
                move |page: &mut Self, value, _window, cx| page.set_rule(&target, None, &value, cx),
                cx,
            )),
        )];
        let tools: Vec<(String, String)> = rules
            .get("tools")
            .and_then(Value::as_object)
            .map(|tools| {
                tools
                    .iter()
                    .map(|(key, rule)| (key.clone(), rule.as_str().unwrap_or("ask").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        for (key, rule) in tools {
            let (name, provider) = tool_label(&key);
            let (set_workspace, set_key) = (id.clone(), key.clone());
            let (reset_workspace, reset_key) = (id.clone(), key.clone());
            let controls = h_flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_segmented(
                    p,
                    &format!("connectors-{id}-rule-{key}"),
                    &options,
                    Some(rule.as_str()),
                    move |page: &mut Self, value, _window, cx| {
                        page.set_rule(&set_workspace, Some(&set_key), &value, cx)
                    },
                    cx,
                ))
                .child(settings_icon_button(
                    p,
                    SharedString::from(format!("connectors-{id}-rule-reset-{key}")),
                    "modals/settings/trash.svg",
                    16.0,
                    32.0,
                    ButtonVariant::Ghost,
                    Some("Follow the workspace's rule again".into()),
                    !self.rpc_available(cx),
                    move |page: &mut Self, _window, cx| {
                        page.set_rule(&reset_workspace, Some(&reset_key), "default", cx)
                    },
                    cx,
                ))
                .into_any_element();
            rows.push(settings_list_item(
                p,
                None,
                None,
                name,
                (!provider.is_empty()).then(|| detail(format!("{provider} · {}", explain(&rule)))),
                Some(controls),
            ));
        }
        settings_section(
            p,
            title,
            Some(
                "Composio reads run on their own; every Executor call follows these rules. Always allow on a card adds that tool here.".into(),
            ),
            None,
            rows,
        )
    }
}
