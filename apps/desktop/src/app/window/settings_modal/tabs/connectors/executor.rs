//! The Executor section of a workspace: turning it on or off and its state, its apps with their
//! sign-ins, adding an app from Executor's catalog or a remote MCP server, and the sign-in
//! callback address an app's own OAuth client needs.
use super::super::super::fields::toggle_from_row;
use super::super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, ROW_PADDING_X, ROW_PADDING_Y, settings_button,
    settings_icon_button, settings_list_item, settings_section, settings_text_input,
    switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_copy_to_clipboard;
use super::ConnectorsTab;
use super::data::{busy_key, flag, list, text};
use super::widgets::{detail, note_row};
use gpui::{
    AnyElement, Context, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    div, px,
};
use gpui_component::h_flex;
use serde_json::Value;

/// The id of one of a workspace's Executor inputs (`catalog`, `mcp-name`, `mcp-url`).
pub(super) fn input_id(workspace: &str, field: &str) -> SharedString {
    SharedString::from(format!("connectors-{workspace}-executor-{field}"))
}

/// What Executor is doing, as one line.
fn state_line(executor: &Value) -> (ListItemStatus, String) {
    let installing = executor
        .get("install")
        .and_then(|install| install.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "queued" | "running"));
    let error = text(executor, "error");
    // Its saved keys can't be read anymore: gxserver's `error` says so in general words.
    if flag(executor, "unreadable") && !error.is_empty() {
        (ListItemStatus::Warning, error)
    } else if !flag(executor, "enabled") {
        (
            ListItemStatus::Neutral,
            "Off. Turn it on to add apps; Ghostex downloads Executor the first time.".to_string(),
        )
    } else if installing {
        (ListItemStatus::Neutral, "Downloading Executor…".to_string())
    } else if !error.is_empty() {
        (ListItemStatus::Warning, error)
    } else if flag(executor, "starting") {
        (ListItemStatus::Neutral, "Starting…".to_string())
    } else if flag(executor, "running") {
        (
            ListItemStatus::Success,
            "Running on this computer".to_string(),
        )
    } else {
        (
            ListItemStatus::Success,
            "On. It starts when an agent needs it and stops after 15 minutes unused.".to_string(),
        )
    }
}

impl ConnectorsTab {
    fn input_value(&self, workspace: &str, field: &str, cx: &Context<Self>) -> String {
        self.fields
            .texts
            .get(&input_id(workspace, field))
            .map(|state| state.input.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    fn input(
        &mut self,
        workspace: &str,
        field: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<gpui_component::input::InputState> {
        FieldStates::text_state(
            self,
            &input_id(workspace, field),
            "",
            Some(placeholder),
            |_: &mut Self, _, _, _| {},
            window,
            cx,
        )
    }

    fn app_row(
        &mut self,
        p: &SettingsPalette,
        workspace: &str,
        app: &Value,
        offline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app_id = text(app, "id");
        let signed_in = flag(app, "signedIn");
        let connecting = self.is_busy(workspace, &format!("executor-connect-{app_id}"));
        let removing = self.is_busy(workspace, &format!("executor-remove-{app_id}"));
        let mut controls: Vec<AnyElement> = Vec::new();
        if !signed_in {
            let (target, connect_id) = (workspace.to_string(), app_id.clone());
            controls.push(settings_button(
                p,
                format!("connectors-{workspace}-executor-connect-{app_id}"),
                if connecting { "Opening…" } else { "Sign in" },
                None,
                ButtonVariant::Outline,
                offline || connecting,
                None,
                move |page: &mut Self, _window, cx| page.executor_connect(&target, &connect_id, cx),
                cx,
            ));
        }
        let (target, remove_id) = (workspace.to_string(), app_id.clone());
        controls.push(settings_icon_button(
            p,
            SharedString::from(format!("connectors-{workspace}-executor-remove-{app_id}")),
            "modals/settings/trash.svg",
            16.0,
            32.0,
            ButtonVariant::Ghost,
            Some("Remove this app".into()),
            offline || removing,
            move |page: &mut Self, _window, cx| page.executor_remove(&target, &remove_id, cx),
            cx,
        ));
        let needs = list(app, "needsSignIn")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        settings_list_item(
            p,
            Some(if signed_in {
                ListItemStatus::Success
            } else {
                ListItemStatus::Warning
            }),
            None,
            text(app, "name"),
            Some(detail(if signed_in {
                "Signed in".to_string()
            } else if needs.is_empty() {
                "Needs a sign-in".to_string()
            } else {
                format!("Needs a sign-in: {}", needs.join(", "))
            })),
            Some(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(controls)
                    .into_any_element(),
            ),
        )
    }

    fn catalog_rows(
        &mut self,
        p: &SettingsPalette,
        workspace: &str,
        offline: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let input = self.input(
            workspace,
            "catalog",
            "Search Executor's catalog, like linear or notion",
            window,
            cx,
        );
        let searching = self.is_busy(workspace, "executor-search");
        let target = workspace.to_string();
        let search = settings_button(
            p,
            format!("connectors-{workspace}-executor-search"),
            if searching { "Searching…" } else { "Search" },
            Some("modals/settings/search.svg"),
            ButtonVariant::Outline,
            offline || searching,
            None,
            move |page: &mut Self, _window, cx| {
                let query = page.input_value(&target, "catalog", cx);
                page.executor_search(&target, query, cx);
            },
            cx,
        );
        let mut rows = vec![
            h_flex()
                .w_full()
                .px(px(ROW_PADDING_X))
                .py(px(ROW_PADDING_Y))
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(settings_text_input(p, &input, None, false, window, cx)),
                )
                .child(search)
                .into_any_element(),
        ];
        for entry in self
            .catalog_results
            .get(workspace)
            .cloned()
            .unwrap_or_default()
        {
            let catalog_id = text(&entry, "id");
            let adding = self.is_busy(workspace, &format!("executor-add-{catalog_id}"));
            let (target, add_id) = (workspace.to_string(), catalog_id.clone());
            let category = text(&entry, "category");
            let description = text(&entry, "description");
            let line = match (category.is_empty(), description.is_empty()) {
                (false, false) => format!("{category} · {description}"),
                (false, true) => category,
                _ => description,
            };
            rows.push(settings_list_item(
                p,
                None,
                None,
                text(&entry, "name"),
                (!line.is_empty()).then(|| detail(line)),
                Some(settings_button(
                    p,
                    format!("connectors-{workspace}-executor-add-{catalog_id}"),
                    if adding { "Adding…" } else { "Add" },
                    Some("modals/settings/plus.svg"),
                    ButtonVariant::Outline,
                    offline || adding,
                    None,
                    move |page: &mut Self, _window, cx| page.executor_add(&target, &add_id, cx),
                    cx,
                )),
            ));
        }
        rows
    }

    fn mcp_row(
        &mut self,
        p: &SettingsPalette,
        workspace: &str,
        offline: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.input(workspace, "mcp-name", "Name", window, cx);
        let url = self.input(
            workspace,
            "mcp-url",
            "https://… address of a remote MCP server",
            window,
            cx,
        );
        let adding = self.is_busy(workspace, "executor-add-mcp");
        let target = workspace.to_string();
        let add = settings_button(
            p,
            format!("connectors-{workspace}-executor-add-mcp"),
            if adding { "Adding…" } else { "Add server" },
            Some("modals/settings/plus.svg"),
            ButtonVariant::Outline,
            offline || adding,
            None,
            move |page: &mut Self, _window, cx| {
                let name = page.input_value(&target, "mcp-name", cx);
                let url = page.input_value(&target, "mcp-url", cx);
                page.executor_add_mcp(&target, name, url, cx);
            },
            cx,
        );
        h_flex()
            .w_full()
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y))
            .gap(px(8.0))
            .items_center()
            .child(settings_text_input(
                p,
                &name,
                Some(140.0),
                false,
                window,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(settings_text_input(p, &url, None, false, window, cx)),
            )
            .child(add)
            .into_any_element()
    }

    pub(super) fn executor_section(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let executor = workspace.get("executor")?.clone();
        let id = text(workspace, "workspaceId");
        let enabled = flag(&executor, "enabled");
        let offline = !self.rpc_available(cx);
        let toggling = self.is_busy(&id, "executor-enable");
        let (status, line) = state_line(&executor);
        let version = text(&executor, "version");
        let target = id.clone();
        let executor_switch = switch_control(
            p,
            format!("connectors-{id}-executor-enabled"),
            "Executor",
            enabled,
            offline || toggling,
            None,
            move |page: &mut Self, on, _window, cx| page.executor_set_enabled(&target, on, cx),
            cx,
        );
        let executor_press = executor_switch.press();
        let mut rows: Vec<AnyElement> = vec![toggle_from_row(
            p,
            format!("connectors-{id}-executor-row"),
            settings_list_item(
                p,
                Some(status),
                None,
                if version.is_empty() {
                    "Executor".to_string()
                } else {
                    format!("Executor {version}")
                },
                Some(detail(line)),
                Some(executor_switch.into_any_element()),
            ),
            executor_press,
        )];
        if enabled {
            for app in list(&executor, "apps") {
                let row = self.app_row(p, &id, &app, offline, cx);
                rows.push(row);
            }
            rows.extend(self.catalog_rows(p, &id, offline, window, cx));
            rows.push(self.mcp_row(p, &id, offline, window, cx));
            let callback = text(&executor, "oauthCallbackUrl");
            if !callback.is_empty() {
                let store = self.store.clone();
                let copied = callback.clone();
                rows.push(settings_list_item(
                    p,
                    None,
                    None,
                    "Sign-in callback address",
                    Some(detail(callback)),
                    Some(settings_button(
                        p,
                        format!("connectors-{id}-executor-callback-copy"),
                        "Copy",
                        Some("modals/settings/copy.svg"),
                        ButtonVariant::Outline,
                        false,
                        None,
                        move |_page: &mut Self, _window, cx| {
                            store_copy_to_clipboard(&store, copied.clone(), cx)
                        },
                        cx,
                    )),
                ));
            }
        }
        if let Some(note) = self.notes.get(&busy_key(&id, "executor")) {
            rows.push(note_row(p, note));
        }
        rows.extend(self.sign_in_panel(p, &id, "executor", cx));
        settings_section(
            p,
            title,
            Some("Executor runs on this computer and links apps from its catalog or any remote MCP server. Some apps, Google's for example, first need your own sign-in app (OAuth client) from that app's developer console, using the callback address below.".into()),
            None,
            rows,
        )
    }
}
