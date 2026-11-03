//! "Set up cloud box" in the selected project's Cloud box section: a new chat in this project with
//! the Cloud Box segment on a blank box and a setup prompt staged in its message box (Q1003, guided
//! agent). Nothing starts or bills until the user sends it; the box chip then offers Save as
//! snapshot. The host side is `apps/desktop/src/app/gx_store/create/cloud_box_setup.rs`.
use super::super::super::fields::{ButtonVariant, settings_button};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::{ProjectItem, ProjectsTab, plain_row};
use gpui::{AnyElement, Context, SharedString};
use serde_json::json;

impl ProjectsTab {
    pub(super) fn cloud_box_setup_row(
        &mut self,
        p: &SettingsPalette,
        project: &ProjectItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_id = project.project_id.clone();
        let button = settings_button(
            p,
            "project-cloud-box-set-up",
            "Set up cloud box",
            Some("titlebar/cloud.svg"),
            ButtonVariant::Outline,
            false,
            None,
            move |this: &mut ProjectsTab, _, cx| {
                post_store_message(
                    &this.store,
                    json!({ "type": "setUpCloudBox", "projectId": project_id }),
                    cx,
                );
            },
            cx,
        );
        plain_row(
            p,
            &SharedString::from("project-cloud-box-set-up"),
            "Set up cloud box",
            Some(
                "Opens a new chat on a blank box with a setup prompt: the agent installs the project, runs its tests, writes .ghostex/cloud-setup.sh and signs in to GitHub with you. Save it as the Default snapshot from the box chip when it's done.",
            ),
            None,
            false,
            button,
            cx,
        )
    }
}
