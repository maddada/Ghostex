//! "Your boxes" (Q1017 "chip and settings"): every Cloud Box with its state, project, age and
//! cost, Stop, and Delete after a confirmation. A box with unpushed work is not deleted until the
//! user picks Delete Anyway (D13 "keep and say").
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_icon, settings_list_item,
    settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::cloud_spend::usd;
use super::{CloudBoxesTab, info_row};
use ghostex_gx_protocol::cloud_box::{BoxResult, CloudBox, CloudBoxState};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::h_flex;
use serde_json::json;
use std::time::Duration;

const ICON_BOX: &str = "modals/settings/box.svg";
const ICON_STOP: &str = "modals/settings/player-pause.svg";
const ICON_TRASH: &str = "modals/settings/trash.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";

/// Stopping and deleting wait for the provider.
const CHANGE_TIMEOUT: Duration = Duration::from_secs(130);

fn state_label(state: &CloudBoxState) -> String {
    match state {
        CloudBoxState::Pending => "Waiting for the first message",
        CloudBoxState::Creating => "Creating",
        CloudBoxState::Booting => "Starting",
        CloudBoxState::Running => "Running",
        CloudBoxState::Idle => "Idle",
        CloudBoxState::Paused => "Paused",
        CloudBoxState::Stopped => "Stopped",
        CloudBoxState::Deleting => "Deleting",
        CloudBoxState::Deleted => "Deleted",
        CloudBoxState::Failed => "Failed",
        CloudBoxState::Other(other) => other.as_str(),
    }
    .to_string()
}

fn dot(state: &CloudBoxState) -> ListItemStatus {
    match state {
        CloudBoxState::Running => ListItemStatus::Success,
        CloudBoxState::Idle | CloudBoxState::Failed => ListItemStatus::Warning,
        _ => ListItemStatus::Neutral,
    }
}

/// "3d", "5h", "12m" since an ISO-8601 time.
fn age(since: Option<&str>) -> Option<String> {
    let since = chrono::DateTime::parse_from_rfc3339(since?).ok()?;
    let minutes = (chrono::Utc::now() - since.with_timezone(&chrono::Utc))
        .num_minutes()
        .max(0);
    Some(if minutes >= 24 * 60 {
        format!("{}d", minutes / (24 * 60))
    } else if minutes >= 60 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    })
}

impl CloudBoxesTab {
    /// The project name a box's sessions are listed under (`projectSettingsProjects`), else the
    /// last part of its project key.
    fn box_project_name(&self, row: &CloudBox, cx: &Context<Self>) -> Option<String> {
        let named = row.home_project_id.as_deref().and_then(|project_id| {
            self.store
                .read(cx)
                .hud()
                .and_then(|hud| hud.get("projectSettingsProjects"))
                .and_then(serde_json::Value::as_array)?
                .iter()
                .find(|project| project["projectId"] == project_id)
                .and_then(|project| project["name"].as_str())
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        });
        named.or_else(|| {
            row.project_key
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
    }

    fn change_cloud_box(
        &mut self,
        box_id: String,
        action: &'static str,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        self.cloud.busy.insert(box_id.clone());
        self.cloud.confirm_delete = None;
        cx.notify();
        let params = if action == "delete" {
            json!({ "boxId": box_id, "force": force })
        } else {
            json!({ "boxId": box_id })
        };
        self.cloud_rpc(
            action,
            params,
            CHANGE_TIMEOUT,
            move |page, result, cx| {
                page.cloud.busy.remove(&box_id);
                match result.and_then(|value| {
                    serde_json::from_value::<BoxResult>(value)
                        .map_err(|error| format!("Unexpected answer: {error}"))
                }) {
                    Ok(answer) => {
                        page.cloud.needs_force.remove(&box_id);
                        if let Some(boxes) = page.cloud.boxes.as_mut() {
                            if answer.cloud_box.state == CloudBoxState::Deleted {
                                boxes.retain(|row| row.box_id != box_id);
                            } else if let Some(row) =
                                boxes.iter_mut().find(|row| row.box_id == box_id)
                            {
                                *row = answer.cloud_box;
                            }
                        }
                    }
                    // A delete refused for unpushed work offers Delete Anyway (D13); the
                    // refusal's own sentence says what is left.
                    Err(error) if action == "delete" && !force => {
                        page.cloud.needs_force.insert(box_id.clone(), error);
                    }
                    Err(error) => page.toast(
                        "error",
                        if action == "delete" {
                            "Couldn't delete the box"
                        } else {
                            "Couldn't stop the box"
                        },
                        &error,
                        cx,
                    ),
                }
                page.load_cloud_boxes(false, cx);
            },
            cx,
        );
    }

    pub(super) fn cloud_box_list_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !show("cloudBoxesBoxes") {
            return None;
        }
        let refresh = settings_button(
            p,
            "cloud-boxes-cloud-list-refresh",
            "Refresh",
            Some(ICON_REFRESH),
            ButtonVariant::Ghost,
            self.cloud.loading.contains("list") || !self.rpc_available(cx),
            Some("Reading your boxes…".into()),
            |page: &mut Self, _window, cx| page.load_cloud_boxes(true, cx),
            cx,
        );
        let mut rows: Vec<AnyElement> = Vec::new();
        match (self.cloud.boxes.clone(), self.cloud.boxes_error.clone()) {
            (_, Some(error)) => rows.push(info_row(
                p,
                "cloud-boxes-error",
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
                "cloud-boxes-empty",
                None,
                ICON_BOX,
                "No boxes yet",
                "Pick Cloud Box in a new chat; its first message starts the box.",
                None,
                Vec::new(),
            )),
            (Some(boxes), None) => {
                for row in boxes {
                    rows.push(self.cloud_box_row(p, &row, cx));
                    if self.cloud.confirm_delete.as_deref() == Some(row.box_id.as_str())
                        || self.cloud.needs_force.contains_key(&row.box_id)
                    {
                        rows.push(self.cloud_delete_confirmation(p, &row, cx));
                    }
                }
            }
        }
        settings_section(p, "Your boxes", None, Some(refresh), rows)
            .map(IntoElement::into_any_element)
    }

    fn cloud_box_row(
        &mut self,
        p: &SettingsPalette,
        row: &CloudBox,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.cloud.busy.contains(&row.box_id);
        let mut facts = vec![state_label(&row.state)];
        if let Some(detail) = row.state_detail.as_ref().filter(|text| !text.is_empty()) {
            facts.push(detail.clone());
        }
        if let Some(project) = self.box_project_name(row, cx) {
            facts.push(project);
        }
        facts.push(row.provider.label().to_string());
        if let Some(age) = age(row.created_at.as_deref()) {
            facts.push(age);
        }
        if let Some(cost) = row.cost_total_usd {
            facts.push(format!(
                "{}{}",
                if row.cost_estimated { "≈" } else { "" },
                usd(cost)
            ));
        }
        if row.has_unpushed_work == Some(true) {
            facts.push("Has unpushed work".to_string());
        }
        let mut controls: Vec<AnyElement> = Vec::new();
        let box_id = row.box_id.clone();
        if row.state.is_reachable() && row.provider_box_id.is_some() {
            let id = box_id.clone();
            controls.push(settings_button(
                p,
                SharedString::from(format!("cloud-box-stop-{box_id}")),
                "Stop",
                Some(ICON_STOP),
                ButtonVariant::Ghost,
                busy,
                Some("Working…".into()),
                move |page: &mut Self, _window, cx| {
                    page.change_cloud_box(id.clone(), "stop", false, cx)
                },
                cx,
            ));
        }
        if row.state != CloudBoxState::Deleting {
            let id = box_id.clone();
            controls.push(settings_button(
                p,
                SharedString::from(format!("cloud-box-delete-{box_id}")),
                "Delete…",
                Some(ICON_TRASH),
                ButtonVariant::Ghost,
                busy,
                Some("Working…".into()),
                move |page: &mut Self, _window, cx| {
                    page.cloud.confirm_delete = Some(id.clone());
                    page.cloud.needs_force.remove(&id);
                    cx.notify();
                },
                cx,
            ));
        }
        let tooltip = match (&row.size, row.usd_per_hour) {
            (Some(size), Some(rate)) => format!(
                "{} · {} on {} · about {}/h. Stop keeps its work; sending a message in one of its sessions wakes it.",
                row.name,
                size,
                row.provider.label(),
                usd(rate)
            ),
            _ => format!(
                "{} on {}. Stop keeps its work; sending a message in one of its sessions wakes it.",
                row.name,
                row.provider.label()
            ),
        };
        info_row(
            p,
            &format!("cloud-box-{box_id}"),
            Some(dot(&row.state)),
            ICON_BOX,
            row.name.clone(),
            tooltip,
            Some(facts.join(" · ")),
            controls,
        )
    }

    fn cloud_delete_confirmation(
        &mut self,
        p: &SettingsPalette,
        row: &CloudBox,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refused = self.cloud.needs_force.get(&row.box_id).cloned();
        let text = match &refused {
            Some(reason) => reason.clone(),
            None => format!(
                "Delete {}? Its machine and disk go; work you pushed stays on its branch.",
                row.name
            ),
        };
        let cancel_id = row.box_id.clone();
        let cancel = settings_button(
            p,
            SharedString::from(format!("cloud-box-delete-cancel-{}", row.box_id)),
            "Cancel",
            None,
            ButtonVariant::Ghost,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.cloud.confirm_delete = None;
                page.cloud.needs_force.remove(&cancel_id);
                cx.notify();
            },
            cx,
        );
        let id = row.box_id.clone();
        let force = refused.is_some();
        let confirm = settings_button(
            p,
            SharedString::from(format!("cloud-box-delete-confirm-{}", row.box_id)),
            if force { "Delete Anyway" } else { "Delete" },
            Some(ICON_TRASH),
            ButtonVariant::Destructive,
            self.cloud.busy.contains(&row.box_id),
            Some("Deleting…".into()),
            move |page: &mut Self, _window, cx| {
                page.change_cloud_box(id.clone(), "delete", force, cx)
            },
            cx,
        );
        h_flex()
            .w_full()
            .px(px(20.0))
            .py(px(12.0))
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .whitespace_normal()
                    .text_size(px(13.0))
                    .text_color(gpui::Hsla::from(if force {
                        p.destructive
                    } else {
                        p.muted
                    }))
                    .child(text),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(cancel)
                    .child(confirm),
            )
            .into_any_element()
    }
}
