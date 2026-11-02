//! The selected project's Cloud box section on the Projects page (`/api/cloudBoxes`
//! `projectSettings`, `snapshots`): the provider and size its boxes start on, its snapshots with
//! the Default (★), Update and Delete (Q1002, Q1004, D9), and "Send to the box", the ticked
//! `.env*` files and variable names a box gets at start (Q1020). Secret values never pass through
//! here; gxserver reads them at boot.
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    ButtonVariant, FieldStates, checkbox_control, settings_button, settings_select,
    settings_text_input,
};
use super::super::super::page::PageBlock;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::{ProjectItem, ProjectsTab, plain_row};
use ghostex_gx_protocol::cloud_box::{
    CLOUD_BOXES_ROUTE, CloudBox, CloudBoxSize, CloudProvider, CloudProviderConnectionState,
    CloudSnapshot, CloudSnapshotStatus, ListResult, ProjectCloudBoxSettings, ProvidersResult,
    SizesResult, SnapshotsResult,
};
use gpui::{
    AnyElement, App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Saving a snapshot asks the provider to start it; deleting one waits for the provider.
const CHANGE_TIMEOUT: Duration = Duration::from_secs(90);
const VARS_FIELD: &str = "project-cloud-box-vars";

#[derive(Default)]
pub(super) struct ProjectCloudBoxState {
    /// The project the data below belongs to; another project resets it.
    project_id: String,
    settings: Option<ProjectCloudBoxSettings>,
    env_files: Vec<String>,
    settings_error: Option<String>,
    snapshots: Option<Vec<CloudSnapshot>>,
    snapshots_error: Option<String>,
    providers: Vec<CloudProvider>,
    sizes: Vec<CloudBoxSize>,
    /// This project's boxes (Update needs a running one).
    boxes: Vec<CloudBox>,
    /// Snapshot ids with an action in flight, and `settings` while a save is out.
    busy: HashSet<String>,
    /// The variable names typed but not saved.
    vars_draft: Option<String>,
}

fn rpc_error(error: String) -> String {
    if error.trim().is_empty() {
        "gxserver did not answer.".to_string()
    } else {
        error
    }
}

fn parse<T: serde::de::DeserializeOwned>(result: Result<Value, String>) -> Result<T, String> {
    serde_json::from_value(result.map_err(rpc_error)?)
        .map_err(|error| format!("Unexpected answer: {error}"))
}

impl ProjectsTab {
    fn cloud_rpc(
        &self,
        action: &str,
        mut params: Value,
        timeout: Duration,
        reply: impl FnOnce(&mut Self, Result<Value, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(map) = params.as_object_mut() {
            map.insert("action".into(), json!(action));
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            CLOUD_BOXES_ROUTE,
            params,
            timeout,
            move |result, cx: &mut App| {
                let _ = this.update(cx, |page, cx| {
                    reply(page, result, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn load_cloud_box(&mut self, project_id: &str, cx: &mut Context<Self>) {
        self.cloud_box = ProjectCloudBoxState {
            project_id: project_id.to_string(),
            ..Default::default()
        };
        let id = project_id.to_string();
        self.cloud_rpc(
            "projectSettings",
            json!({ "projectId": id }),
            READ_TIMEOUT,
            move |page, result, _cx| {
                if page.cloud_box.project_id != id {
                    return;
                }
                match parse::<Value>(result) {
                    Ok(answer) => {
                        page.cloud_box.settings =
                            serde_json::from_value(answer["settings"].clone()).ok();
                        page.cloud_box.env_files = answer["envFiles"]
                            .as_array()
                            .map(|files| {
                                files
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default();
                        page.cloud_box.settings_error = None;
                    }
                    Err(error) => page.cloud_box.settings_error = Some(error),
                }
            },
            cx,
        );
        self.load_cloud_snapshots(cx);
        let id = project_id.to_string();
        self.cloud_rpc(
            "list",
            json!({ "projectId": id }),
            READ_TIMEOUT,
            move |page, result, _cx| {
                if let (true, Ok(answer)) =
                    (page.cloud_box.project_id == id, parse::<ListResult>(result))
                {
                    page.cloud_box.boxes = answer.boxes;
                }
            },
            cx,
        );
        self.cloud_rpc(
            "providers",
            json!({}),
            READ_TIMEOUT,
            |page, result, _cx| {
                if let Ok(answer) = parse::<ProvidersResult>(result) {
                    page.cloud_box.providers = answer.providers;
                }
            },
            cx,
        );
        self.cloud_rpc(
            "sizes",
            json!({}),
            READ_TIMEOUT,
            |page, result, _cx| {
                if let Ok(answer) = parse::<SizesResult>(result) {
                    page.cloud_box.sizes = answer.sizes;
                }
            },
            cx,
        );
    }

    fn load_cloud_snapshots(&mut self, cx: &mut Context<Self>) {
        let id = self.cloud_box.project_id.clone();
        self.cloud_rpc(
            "snapshots",
            json!({ "projectId": id }),
            READ_TIMEOUT,
            move |page, result, _cx| {
                if page.cloud_box.project_id != id {
                    return;
                }
                match parse::<SnapshotsResult>(result) {
                    Ok(answer) => {
                        page.cloud_box.snapshots = Some(answer.snapshots);
                        page.cloud_box.snapshots_error = None;
                    }
                    Err(error) => page.cloud_box.snapshots_error = Some(error),
                }
            },
            cx,
        );
    }

    /// Replaces the project's Cloud Box settings with `change` applied to the last read.
    fn save_cloud_box_settings(
        &mut self,
        change: impl FnOnce(&mut ProjectCloudBoxSettings),
        cx: &mut Context<Self>,
    ) {
        let Some(mut settings) = self.cloud_box.settings.clone() else {
            return;
        };
        change(&mut settings);
        self.cloud_box.settings = Some(settings.clone());
        self.cloud_box.busy.insert("settings".to_string());
        let id = self.cloud_box.project_id.clone();
        self.cloud_rpc(
            "projectSettings",
            json!({ "projectId": id, "settings": settings }),
            READ_TIMEOUT,
            move |page, result, cx| {
                page.cloud_box.busy.remove("settings");
                if let Err(error) = parse::<Value>(result) {
                    page.toast_error("Couldn't save the Cloud box settings", &error, cx);
                }
            },
            cx,
        );
    }

    fn change_snapshot(
        &mut self,
        action: &'static str,
        params: Value,
        busy_key: String,
        cx: &mut Context<Self>,
    ) {
        self.cloud_box.busy.insert(busy_key.clone());
        cx.notify();
        self.cloud_rpc(
            action,
            params,
            CHANGE_TIMEOUT,
            move |page, result, cx| {
                page.cloud_box.busy.remove(&busy_key);
                if let Err(error) = parse::<Value>(result) {
                    let title = match action {
                        "setDefaultSnapshot" => "Couldn't make it the Default",
                        "deleteSnapshot" => "Couldn't delete the snapshot",
                        _ => "Couldn't update the snapshot",
                    };
                    page.toast_error(title, &error, cx);
                }
                page.load_cloud_snapshots(cx);
            },
            cx,
        );
    }

    fn toast_error(&self, title: &str, description: &str, cx: &mut App) {
        self.store
            .update(cx, |store, cx| store.toast("error", title, description, cx));
    }

    /// The Cloud box section of the selected project.
    pub(super) fn cloud_box_blocks(
        &mut self,
        p: &SettingsPalette,
        project: &ProjectItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<PageBlock> {
        if !self.store.read(cx).request().gxserver_rpc_available {
            return Vec::new();
        }
        if self.cloud_box.project_id != project.project_id {
            self.load_cloud_box(&project.project_id, cx);
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(error) = self.cloud_box.settings_error.clone() {
            rows.push(note_row(
                p,
                "project-cloud-box-error",
                "Cloud box",
                &format!("Ghostex could not read this project's Cloud box settings: {error}"),
                cx,
            ));
        } else if self.cloud_box.settings.is_some() {
            rows.push(self.cloud_box_defaults_row(p, window, cx));
            rows.extend(self.cloud_box_snapshot_rows(p, cx));
            rows.push(self.cloud_box_setup_row(p, project, cx));
            rows.push(self.cloud_box_secrets_row(p, window, cx));
        } else {
            rows.push(note_row(
                p,
                "project-cloud-box-loading",
                "Cloud box",
                "Reading…",
                cx,
            ));
        }
        super::super::super::fields::settings_section(
            p,
            "Cloud box",
            Some(
                "How a Cloud Box for this project starts. Pick Cloud Box in a new chat to use one."
                    .into(),
            ),
            None,
            rows,
        )
        .map(|section| vec![PageBlock::section("projectCloudBox", section)])
        .unwrap_or_default()
    }

    fn cloud_box_defaults_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.cloud_box.settings.clone().unwrap_or_default();
        let usable: Vec<&CloudProvider> = self
            .cloud_box
            .providers
            .iter()
            .filter(|provider| provider.state == CloudProviderConnectionState::Connected)
            .collect();
        let mut provider_options = vec![SettingOption {
            label: "First connected".to_string(),
            value: String::new(),
        }];
        provider_options.extend(usable.iter().map(|provider| SettingOption {
            label: provider.label.clone(),
            value: provider.id.as_str().to_string(),
        }));
        let provider = settings
            .provider
            .as_ref()
            .map(|provider| provider.as_str().to_string())
            .unwrap_or_default();
        if !provider_options
            .iter()
            .any(|option| option.value == provider)
        {
            provider_options.push(SettingOption {
                label: format!("{provider} (not connected)"),
                value: provider.clone(),
            });
        }
        let size_provider = if provider.is_empty() {
            usable
                .first()
                .map(|provider| provider.id.as_str().to_string())
        } else {
            Some(provider.clone())
        };
        let mut size_options = vec![SettingOption {
            label: "Provider's default".to_string(),
            value: String::new(),
        }];
        size_options.extend(
            self.cloud_box
                .sizes
                .iter()
                .filter(|size| Some(size.provider.as_str()) == size_provider.as_deref())
                .map(|size| SettingOption {
                    label: match size.usd_per_hour {
                        Some(rate) if rate > 0.0 => format!("{} · ≈${rate:.2}/h", size.label),
                        _ => size.label.clone(),
                    },
                    value: size.id.clone(),
                }),
        );
        let size = settings.size.clone().unwrap_or_default();
        if !size_options.iter().any(|option| option.value == size) {
            size_options.push(SettingOption {
                label: size.clone(),
                value: size.clone(),
            });
        }
        let provider_select = settings_select(
            self,
            p,
            "project-cloud-box-provider",
            &provider_options,
            &provider,
            Some(170.0),
            false,
            None,
            |page: &mut Self, value, _window, cx| {
                page.save_cloud_box_settings(
                    |settings| {
                        let next = (!value.is_empty()).then(|| {
                            ghostex_gx_protocol::cloud_box::CloudProviderId::from_wire(&value)
                        });
                        if settings.provider != next {
                            settings.size = None;
                        }
                        settings.provider = next;
                    },
                    cx,
                )
            },
            window,
            cx,
        );
        let size_select = settings_select(
            self,
            p,
            "project-cloud-box-size",
            &size_options,
            &size,
            Some(200.0),
            false,
            None,
            |page: &mut Self, value, _window, cx| {
                page.save_cloud_box_settings(
                    |settings| settings.size = (!value.is_empty()).then_some(value),
                    cx,
                )
            },
            window,
            cx,
        );
        plain_row(
            p,
            &SharedString::from("project-cloud-box-defaults"),
            "Provider and size",
            Some(
                "Which provider and machine size a new Cloud Box for this project starts on. Connect providers in Settings › Cloud Boxes.",
            ),
            None,
            false,
            h_flex()
                .gap(px(8.0))
                .child(provider_select)
                .child(size_select)
                .into_any_element(),
            cx,
        )
    }

    fn cloud_box_snapshot_rows(
        &mut self,
        p: &SettingsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let snapshots = match (
            self.cloud_box.snapshots.clone(),
            self.cloud_box.snapshots_error.clone(),
        ) {
            (_, Some(error)) => {
                return vec![note_row(
                    p,
                    "project-cloud-box-snapshots-error",
                    "Snapshots",
                    &format!("Ghostex could not read this project's snapshots: {error}"),
                    cx,
                )];
            }
            (None, None) => {
                return vec![note_row(
                    p,
                    "project-cloud-box-snapshots",
                    "Snapshots",
                    "Reading…",
                    cx,
                )];
            }
            (Some(snapshots), None) => snapshots,
        };
        if snapshots.is_empty() {
            return vec![note_row(
                p,
                "project-cloud-box-snapshots-empty",
                "Snapshots",
                "None yet. Start a Cloud Box session, set the project up on the box, then use Save as snapshot on its box chip.",
                cx,
            )];
        }
        // Update saves the Default again from a box of this project that is running now (D9).
        let running_box = self
            .cloud_box
            .boxes
            .iter()
            .find(|row| row.state.is_reachable())
            .map(|row| row.box_id.clone());
        snapshots
            .into_iter()
            .map(|snapshot| self.cloud_box_snapshot_row(p, snapshot, running_box.clone(), cx))
            .collect()
    }

    fn cloud_box_snapshot_row(
        &mut self,
        p: &SettingsPalette,
        snapshot: CloudSnapshot,
        running_box: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = snapshot.snapshot_id.clone();
        let busy = self.cloud_box.busy.contains(&id);
        let mut facts = vec![snapshot.provider.label().to_string()];
        match snapshot.status {
            CloudSnapshotStatus::Pending => facts.push("Saving…".to_string()),
            CloudSnapshotStatus::Failed => facts.push(
                snapshot
                    .error
                    .clone()
                    .unwrap_or_else(|| "Failed".to_string()),
            ),
            _ => {}
        }
        if let Some(created) = snapshot.created_at.as_deref().and_then(|at| at.get(..10)) {
            facts.push(created.to_string());
        }
        if let Some(branch) = &snapshot.base_branch {
            facts.push(branch.clone());
        }
        if let Some(expires) = snapshot.expires_at.as_deref().and_then(|at| at.get(..10)) {
            facts.push(format!("Deleted on {expires}"));
        }
        if snapshot.shared_with_org {
            facts.push("Shared with your Instabox org".to_string());
        }
        let mut buttons: Vec<AnyElement> = Vec::new();
        if snapshot.is_default {
            let reason: SharedString = if running_box.is_some() {
                "Saving…".into()
            } else {
                "Start a Cloud Box session of this project first; Update saves that box as the new Default.".into()
            };
            let box_id = running_box.clone();
            let id = id.clone();
            buttons.push(settings_button(
                p,
                SharedString::from(format!("project-cloud-box-update-{id}")),
                "Update",
                Some("modals/settings/refresh.svg"),
                ButtonVariant::Ghost,
                busy || box_id.is_none(),
                Some(reason),
                move |page: &mut Self, _window, cx| {
                    if let Some(box_id) = box_id.clone() {
                        page.change_snapshot(
                            "saveSnapshot",
                            json!({ "boxId": box_id, "replaceDefault": true }),
                            id.clone(),
                            cx,
                        );
                    }
                },
                cx,
            ));
        } else if snapshot.status == CloudSnapshotStatus::Ready {
            let id = id.clone();
            buttons.push(settings_button(
                p,
                SharedString::from(format!("project-cloud-box-default-{id}")),
                "Make Default",
                Some("modals/settings/star.svg"),
                ButtonVariant::Ghost,
                busy,
                Some("Saving…".into()),
                move |page: &mut Self, _window, cx| {
                    page.change_snapshot(
                        "setDefaultSnapshot",
                        json!({ "snapshotId": id.clone() }),
                        id.clone(),
                        cx,
                    )
                },
                cx,
            ));
        }
        {
            let id = id.clone();
            buttons.push(settings_button(
                p,
                SharedString::from(format!("project-cloud-box-delete-{id}")),
                "Delete",
                Some("modals/settings/trash.svg"),
                ButtonVariant::Ghost,
                busy,
                Some("Deleting…".into()),
                move |page: &mut Self, _window, cx| {
                    page.change_snapshot(
                        "deleteSnapshot",
                        json!({ "snapshotId": id.clone() }),
                        id.clone(),
                        cx,
                    )
                },
                cx,
            ));
        }
        let title = if snapshot.is_default {
            format!("★ {} (Default)", snapshot.name)
        } else {
            snapshot.name.clone()
        };
        plain_row(
            p,
            &SharedString::from(format!(
                "project-cloud-box-snapshot-{}",
                snapshot.snapshot_id
            )),
            &title,
            Some(&facts.join(" · ")),
            None,
            false,
            h_flex().gap(px(6.0)).children(buttons).into_any_element(),
            cx,
        )
    }

    fn cloud_box_secrets_row(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.cloud_box.settings.clone().unwrap_or_default();
        let ticked: HashSet<String> = settings.secrets.files.iter().cloned().collect();
        // Files ticked earlier that no longer exist stay listed, so they can be unticked.
        let mut files = self.cloud_box.env_files.clone();
        for file in &settings.secrets.files {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
        let mut list = v_flex().w_full().gap(px(8.0));
        if files.is_empty() {
            list = list.child(
                div()
                    .text_size(px(13.0))
                    .text_color(gpui::Hsla::from(p.muted))
                    .child("This project has no .env files."),
            );
        }
        for file in files {
            let checked = ticked.contains(&file);
            let label = div()
                .text_size(px(13.0))
                .text_color(gpui::Hsla::from(p.foreground))
                .child(file.clone())
                .into_any_element();
            list = list.child(checkbox_control(
                p,
                SharedString::from(format!("project-cloud-box-file-{file}")),
                checked,
                Some(label),
                8.0,
                move |page: &mut Self, next, _window, cx| {
                    let file = file.clone();
                    page.save_cloud_box_settings(
                        |settings| {
                            settings.secrets.files.retain(|kept| *kept != file);
                            if next {
                                settings.secrets.files.push(file);
                                settings.secrets.files.sort();
                            }
                        },
                        cx,
                    );
                },
                cx,
            ));
        }
        let saved_vars = settings.secrets.vars.join(", ");
        let draft = self
            .cloud_box
            .vars_draft
            .clone()
            .unwrap_or_else(|| saved_vars.clone());
        let vars_id = SharedString::from(VARS_FIELD);
        let vars_input = FieldStates::text_state(
            self,
            &vars_id,
            &draft,
            Some("Variable names, e.g. DATABASE_URL, STRIPE_KEY"),
            |page: &mut Self, text, _window, cx| {
                page.cloud_box.vars_draft = Some(text);
                cx.notify();
            },
            window,
            cx,
        );
        let changed = draft.trim() != saved_vars;
        let save = settings_button(
            p,
            "project-cloud-box-vars-save",
            "Save Variables",
            None,
            ButtonVariant::Outline,
            !changed,
            Some("Type variable names first.".into()),
            |page: &mut Self, _window, cx| {
                let draft = page.cloud_box.vars_draft.take().unwrap_or_default();
                let mut vars: Vec<String> = draft
                    .split([',', ' ', '\n'])
                    .map(str::trim)
                    .filter(|name| {
                        !name.is_empty()
                            && name
                                .chars()
                                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                    })
                    .map(str::to_string)
                    .collect();
                vars.dedup();
                page.save_cloud_box_settings(|settings| settings.secrets.vars = vars, cx);
            },
            cx,
        );
        plain_row(
            p,
            &SharedString::from("project-cloud-box-secrets"),
            "Send to the box",
            Some(
                "The ticked files are copied into the box's checkout and the variables are set for its sessions, with values read from this computer each time a box starts. They are never saved in a snapshot.",
            ),
            None,
            true,
            v_flex()
                .w_full()
                .gap(px(12.0))
                .child(list)
                .child(
                    h_flex()
                        .w_full()
                        .gap(px(8.0))
                        .items_center()
                        .child(settings_text_input(p, &vars_input, None, true, window, cx))
                        .child(save),
                )
                .into_any_element(),
            cx,
        )
    }
}

fn note_row(
    p: &SettingsPalette,
    id: &str,
    label: &str,
    text: &str,
    cx: &mut Context<ProjectsTab>,
) -> AnyElement {
    plain_row(
        p,
        &SharedString::from(id.to_string()),
        label,
        None,
        None,
        true,
        div()
            .whitespace_normal()
            .text_size(px(13.0))
            .text_color(gpui::Hsla::from(p.muted))
            .child(text.to_string())
            .into_any_element(),
        cx,
    )
}
