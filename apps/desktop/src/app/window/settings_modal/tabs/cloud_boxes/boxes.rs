//! "Your boxes": every agentbox box on this computer's account (`/api/agentbox list`), with Open
//! Web App, Stop and Destroy (after a confirmation row, since it deletes the box's work).
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_icon, settings_list_item,
    settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::model::{BoxRow, agent_label, provider_label, state_label};
use super::{CloudBoxesTab, info_row};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const ICON_BOX: &str = "modals/settings/box.svg";
const ICON_EXTERNAL: &str = "modals/settings/external-link.svg";
const ICON_STOP: &str = "modals/settings/player-pause.svg";
const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";

/// `agentbox url` may open an SSH forward first.
const OPEN_TIMEOUT: Duration = Duration::from_secs(70);
/// `agentbox stop` / `destroy` wait for the provider.
const STOP_TIMEOUT: Duration = Duration::from_secs(130);

fn is_running(row: &BoxRow) -> bool {
    matches!(row.state.as_deref(), Some("running") | Some("starting"))
}

impl CloudBoxesTab {
    /// Opens the box's web app on this computer. A box Ghostex launched asks gxserver for the URL
    /// (`agentbox url`, which also opens the SSH forward); another box uses the URL it reported.
    fn open_box_web_app(&mut self, row: BoxRow, cx: &mut Context<Self>) {
        let (Some(project_id), Some(session_id)) = (row.project_id.clone(), row.session_id.clone())
        else {
            if let Some(url) = &row.web_url {
                self.open_url(url, cx);
            }
            return;
        };
        self.busy_boxes.insert(row.name.clone());
        cx.notify();
        let this = cx.weak_entity();
        let name = row.name.clone();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/agentbox",
            json!({
                "action": "openTarget",
                "projectId": project_id,
                "sessionId": session_id,
                "target": "web",
            }),
            OPEN_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.busy_boxes.remove(&name);
                    match result
                        .as_ref()
                        .ok()
                        .and_then(|answer| answer.get("url"))
                        .and_then(Value::as_str)
                        .filter(|url| !url.trim().is_empty())
                    {
                        Some(url) => page.open_url(url, cx),
                        None => page.toast(
                            "error",
                            "Couldn't open the box's web app",
                            result
                                .as_ref()
                                .err()
                                .map(String::as_str)
                                .unwrap_or("The box has no web app running."),
                            cx,
                        ),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// `stop` keeps the box's work; `destroy` removes the box.
    fn change_box(&mut self, name: String, action: &'static str, cx: &mut Context<Self>) {
        self.busy_boxes.insert(name.clone());
        self.confirm_destroy = None;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/agentbox",
            json!({ "action": action, "boxName": name }),
            STOP_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.busy_boxes.remove(&name);
                    let (done, failed) = if action == "destroy" {
                        ("destroyed", "Couldn't destroy")
                    } else {
                        ("stopped", "Couldn't stop")
                    };
                    match result {
                        Ok(answer) if answer["ok"] != false => {
                            page.toast("success", &format!("{name} {done}"), "", cx)
                        }
                        Ok(answer) => page.toast(
                            "error",
                            &format!("{failed} {name}"),
                            answer["output"]
                                .as_str()
                                .unwrap_or("agentbox reported a failure."),
                            cx,
                        ),
                        Err(error) => page.toast("error", &format!("{failed} {name}"), &error, cx),
                    }
                    page.load_boxes(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    pub(super) fn boxes_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !show("agentboxBoxes") {
            return None;
        }
        let refresh = settings_button(
            p,
            "cloud-boxes-list-refresh",
            "Refresh",
            Some(ICON_REFRESH),
            ButtonVariant::Ghost,
            self.boxes_loading,
            Some("Reading your boxes…".into()),
            |page: &mut Self, _window, cx| page.load_boxes(cx),
            cx,
        );
        let mut rows: Vec<AnyElement> = Vec::new();
        match (&self.boxes, &self.boxes_error) {
            (_, Some(error)) => rows.push(info_row(
                p,
                "boxes-error",
                Some(ListItemStatus::Warning),
                ICON_BOX,
                "Boxes",
                format!("Ghostex could not read your boxes: {error}"),
                Some(format!("Ghostex could not read your boxes: {error}")),
                Vec::new(),
            )),
            (None, None) => rows.push(settings_list_item(
                p,
                None,
                Some(settings_icon(ICON_BOX, 17.0, p.muted).into_any_element()),
                "Boxes",
                Some(div().child("Reading your boxes…").into_any_element()),
                None,
            )),
            (Some(boxes), None) if boxes.is_empty() => rows.push(info_row(
                p,
                "boxes-empty",
                None,
                ICON_BOX,
                "No boxes yet",
                "Pick a box location under Run on when you start a new thread.",
                None,
                Vec::new(),
            )),
            (Some(boxes), None) => {
                for row in boxes.clone() {
                    let confirming = self.confirm_destroy.as_deref() == Some(row.name.as_str());
                    rows.push(self.box_row(p, &row, cx));
                    if confirming {
                        rows.push(self.destroy_confirmation(p, &row, cx));
                    }
                }
            }
        }
        settings_section(p, "Your agentbox boxes", None, Some(refresh), rows)
            .map(IntoElement::into_any_element)
    }

    fn box_row(&mut self, p: &SettingsPalette, row: &BoxRow, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.busy_boxes.contains(&row.name);
        let running = is_running(row);
        let mut facts: Vec<String> = Vec::new();
        if let Some(agent) = &row.agent {
            facts.push(agent_label(agent));
        }
        if let Some(provider) = &row.provider {
            facts.push(provider_label(provider));
        }
        if let Some(state) = &row.state {
            facts.push(state_label(state));
        }
        if row.session_id.is_some() {
            facts.push(match &row.session_title {
                Some(title) => format!("Session: {title}"),
                None => "Linked to a Ghostex session".to_string(),
            });
        }
        let name = row.name.clone();
        let has_web_app = row.session_id.is_some() || row.web_url.is_some();
        let open_row = row.clone();
        let mut controls = vec![settings_button(
            p,
            SharedString::from(format!("cloud-boxes-box-{name}-open")),
            "Open Web App",
            Some(ICON_EXTERNAL),
            ButtonVariant::Outline,
            busy || !has_web_app,
            Some(if busy {
                "Working…".into()
            } else {
                "This box has no web app.".into()
            }),
            move |page: &mut Self, _window, cx| page.open_box_web_app(open_row.clone(), cx),
            cx,
        )];
        if running {
            let stop_name = name.clone();
            controls.push(settings_button(
                p,
                SharedString::from(format!("cloud-boxes-box-{name}-stop")),
                "Stop",
                Some(ICON_STOP),
                ButtonVariant::Ghost,
                busy,
                Some("Working…".into()),
                move |page: &mut Self, _window, cx| page.change_box(stop_name.clone(), "stop", cx),
                cx,
            ));
        }
        let destroy_name = name.clone();
        controls.push(settings_button(
            p,
            SharedString::from(format!("cloud-boxes-box-{name}-destroy")),
            "Destroy",
            Some(ICON_TRASH),
            ButtonVariant::Ghost,
            busy,
            Some("Working…".into()),
            move |page: &mut Self, _window, cx| {
                page.confirm_destroy = if page.confirm_destroy.as_deref() == Some(&destroy_name) {
                    None
                } else {
                    Some(destroy_name.clone())
                };
                cx.notify();
            },
            cx,
        ));
        info_row(
            p,
            &format!("box-{name}"),
            Some(if running {
                ListItemStatus::Success
            } else {
                ListItemStatus::Neutral
            }),
            ICON_BOX,
            name.clone(),
            format!(
                "The agentbox box {name}. Open Web App opens the app it serves on this computer. Stop keeps its work; Destroy deletes it, and a cloud box stops billing."
            ),
            Some(facts.join(" · ")),
            controls,
        )
    }

    fn destroy_confirmation(
        &mut self,
        p: &SettingsPalette,
        row: &BoxRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = row.name.clone();
        let cancel = settings_button(
            p,
            SharedString::from(format!("cloud-boxes-box-{name}-destroy-cancel")),
            "Cancel",
            None,
            ButtonVariant::Ghost,
            false,
            None,
            |page: &mut Self, _window, cx| {
                page.confirm_destroy = None;
                cx.notify();
            },
            cx,
        );
        let confirm_name = name.clone();
        let confirm = settings_button(
            p,
            SharedString::from(format!("cloud-boxes-box-{name}-destroy-confirm")),
            "Destroy",
            Some(ICON_TRASH),
            ButtonVariant::Destructive,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.change_box(confirm_name.clone(), "destroy", cx)
            },
            cx,
        );
        settings_list_item(
            p,
            Some(ListItemStatus::Warning),
            None,
            format!("Destroy {name}?"),
            Some(
                div()
                    .whitespace_normal()
                    .child("Deletes the box and any work in it that was not pushed.")
                    .into_any_element(),
            ),
            Some(
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(cancel)
                    .child(confirm)
                    .into_any_element(),
            ),
        )
    }
}
