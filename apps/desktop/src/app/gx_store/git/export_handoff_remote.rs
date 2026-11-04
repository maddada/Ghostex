//! A handoff to somewhere else: Handoff to an agent with another destination picked, which starts the
//! follow-up session in another project, on this computer or another one.
//!
//! The transcript is written where the session runs (that computer's gxserver), its text comes
//! back with the export (`includeMarkdown`), and this app saves it on the target computer
//! (`saveSessionChatAttachment` there) before it creates the session there with a link to that
//! copy typed into its composer. The app already holds a connection to every paired computer, so
//! one path covers every direction: this computer to a remote, a remote to this computer, and one
//! remote to another; the computers never talk to each other.
//!
//! CDXC:TranscriptExport 2026-10-25 DECISION:
//! User (Q546): only the transcript travels for now; (Q548): the old session is kept as it is, with no option to sleep it.
//! TODO(Handoff): carry the chat's attached images and files to the target computer too, next to the transcript, with its links pointing at the copies (user, Q546: "later").
//!
//! SEE-ALSO: apps/desktop/src/app/window/export_transcript_modal.rs (the dialog),
//! packages/gx-core/src/handoff_target.rs (project ranking and the branch notice),
//! server/src/session_transcript_export/output_file.rs (`includeMarkdown`),
//! apps/desktop/src/app/add_project_modal_lifecycle.rs (the clone round trip).

use base64::Engine as _;
use ghostex_gx_core::git_menu::GitToastLevel;
use ghostex_gx_core::handoff_target::{
    HandoffComputerRanking, HandoffProjectCandidate, branch_difference_line, rank_handoff_computer,
};
use ghostex_gx_core::{MachineId, ProjectKey};
use serde_json::{Value, json};

use super::calls::{self, Remote};
use super::scope::GitScope;
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;
use crate::app::window::{
    ExportTranscriptComputer, ExportTranscriptPicks, ExportTranscriptProject,
    ExportTranscriptRemoteTarget,
};

/// Add Project's id for this computer (`GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID`).
const LOCAL_MACHINE_ID: &str = "local";

/// `saveSessionChatAttachment`'s own limit (`SESSION_CHAT_ATTACHMENT_MAX_BYTES`).
const ATTACHMENT_MAX_BYTES: usize = 32 * 1024 * 1024;

/// A project Add Project registered while the dialog waited on it.
#[derive(Clone, Debug)]
pub(crate) struct AddedProject {
    project_id: String,
    title: String,
    path: Option<String>,
}

/// Add Project opened from a handoff's destination.
#[derive(Clone, Debug)]
pub(crate) struct HandoffAddProject {
    machine_id: String,
    /// Add Project is still open; its Close reopens Handoff / Export.
    waiting: bool,
    added: Option<AddedProject>,
}

fn machine_key(machine_id: &str) -> MachineId {
    match machine_id {
        LOCAL_MACHINE_ID => MachineId::Local,
        remote => MachineId::Remote(remote.to_string()),
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn folder_title(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}

impl GhostexGpuiApp {
    fn git_export_remote_for(&self, machine: &MachineId) -> Option<Remote> {
        match machine.remote_id() {
            None => Some(None),
            Some(machine_id) => self
                .gpui_remote_gxserver_request_target(machine_id)
                .map(Some),
        }
    }

    /// The session's project's git remote and folder, for matching projects elsewhere.
    fn git_export_source_repository(&self) -> (Option<String>, Option<String>) {
        let Some(request) = self.gx_store.git.export.request.as_ref() else {
            return (None, None);
        };
        let presentation = self.gx_store.core.presentation();
        let project = presentation
            .loaded(&request.session.machine)
            .and_then(|loaded| loaded.project(&request.session.project_id));
        (
            project.and_then(|project| project.git_remote_origin_url.value().cloned()),
            project.and_then(|project| project.path.clone()),
        )
    }

    /// The computers a handoff can go to: the session's own first, starting on this project, then
    /// every other connected one, each with its projects ranked for this session.
    pub(crate) fn git_export_handoff_computers(&self) -> Vec<ExportTranscriptComputer> {
        let Some(source_project) = self
            .gx_store
            .git
            .export
            .request
            .as_ref()
            .map(|request| request.session.project_key())
        else {
            return Vec::new();
        };
        let (origin, source_path) = self.git_export_source_repository();
        let added = self
            .gx_store
            .git
            .export
            .add_project
            .as_ref()
            .and_then(|add| Some((add.machine_id.clone(), add.added.clone()?)));
        let presentation = self.gx_store.core.presentation();
        let options = self.gpui_add_project_dialog_machine_options();
        let mut computers: Vec<ExportTranscriptComputer> = options
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| {
                let machine_id = text(option, "machineId")?;
                let machine = machine_key(&machine_id);
                if self.git_export_remote_for(&machine).is_none() {
                    return None;
                }
                let is_source = machine == source_project.machine;
                let loaded = match &machine {
                    MachineId::Local => presentation.loaded(&machine),
                    MachineId::Remote(_) => presentation.loaded_live(&machine),
                }?;
                let mut candidates: Vec<HandoffProjectCandidate> = loaded
                    .projects()
                    .iter()
                    .filter(|project| project.bot_profile.is_none())
                    .filter(|project| {
                        project
                            .path
                            .as_deref()
                            .is_some_and(|path| !path.trim().is_empty())
                    })
                    .map(|project| HandoffProjectCandidate {
                        project_id: project.project_id.clone(),
                        title: project.title.clone(),
                        path: project.path.clone(),
                        origin_url: project.git_remote_origin_url.value().cloned(),
                        is_worktree: project.worktree.is_some(),
                    })
                    .collect();
                // A project Add Project just registered may not have reached the stream yet.
                if let Some((added_machine, project)) = &added
                    && added_machine == &machine_id
                    && !candidates
                        .iter()
                        .any(|candidate| candidate.project_id == project.project_id)
                {
                    candidates.push(HandoffProjectCandidate {
                        project_id: project.project_id.clone(),
                        title: project.title.clone(),
                        path: project.path.clone(),
                        origin_url: origin.clone(),
                        is_worktree: false,
                    });
                }
                let HandoffComputerRanking {
                    ranking,
                    missing_repository,
                } = rank_handoff_computer(
                    origin.as_deref(),
                    source_path.as_deref(),
                    &candidates,
                    is_source.then_some(source_project.project_id.as_str()),
                );
                Some(ExportTranscriptComputer {
                    label: text(option, "label").unwrap_or_else(|| machine_id.clone()),
                    machine_id,
                    is_source,
                    projects: ranking
                        .choices
                        .into_iter()
                        .map(|choice| ExportTranscriptProject {
                            project_id: choice.project_id,
                            title: choice.title,
                            path: choice.path,
                            same_repo: choice.same_repo,
                            is_own: choice.is_own,
                        })
                        .collect(),
                    preferred_project_id: ranking.preferred_project_id,
                    missing_repository,
                })
            })
            .collect();
        computers.sort_by_key(|computer| !computer.is_source);
        computers
    }

    /// Reads both checkouts' branches and gives the dialog the notice's first line (Q545).
    pub(crate) fn git_export_read_branches(
        &mut self,
        target: ExportTranscriptRemoteTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = self.gx_store.git.export.request.clone() else {
            return;
        };
        let target_machine = machine_key(&target.machine_id);
        let (Some(source_remote), Some(target_remote)) = (
            self.git_export_remote_for(&request.session.machine),
            self.git_export_remote_for(&target_machine),
        ) else {
            return;
        };
        let computer = self
            .git_export_handoff_computers()
            .into_iter()
            .find(|computer| computer.machine_id == target.machine_id);
        // Another repository on the session's own computer has no branch to compare with: only a
        // second checkout of this repository does.
        if computer.as_ref().is_some_and(|computer| {
            computer.is_source
                && !computer
                    .projects
                    .iter()
                    .any(|project| project.project_id == target.project_id && project.same_repo)
        }) {
            self.push_gpui_export_transcript_branch_notice(target, None, cx);
            return;
        }
        let label = computer
            .map(|computer| computer.label)
            .unwrap_or_else(|| target.machine_id.clone());
        let source_project = request.session.project_id.clone();
        let target_project = target.project_id.clone();
        cx.spawn(async move |this, cx| {
            let source = calls::read_git_state(source_remote, source_project, false).await;
            let destination = calls::read_git_state(target_remote, target_project, false).await;
            let branch = |read: &Result<ghostex_gx_core::git_menu::ProjectGitRead, _>| {
                read.as_ref()
                    .ok()
                    .filter(|read| read.is_repo)
                    .and_then(|read| read.branch.clone())
            };
            let line = branch_difference_line(
                branch(&source).as_deref(),
                branch(&destination).as_deref(),
                &label,
            );
            let _ = this.update(cx, |this, cx| {
                this.push_gpui_export_transcript_branch_notice(target, line, cx);
            });
        })
        .detach();
    }

    /// Q544: Add Project on that computer, prefilled with this session's repository. Handoff /
    /// Export has closed itself; Add Project's Close reopens it (`git_export_add_project_closed`).
    pub(crate) fn git_export_add_project_on(
        &mut self,
        machine_id: &str,
        picks: ExportTranscriptPicks,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store.git.export.request.is_none() {
            return;
        }
        let (origin, _) = self.git_export_source_repository();
        let host = &mut self.gx_store.git.export;
        host.restore = Some(picks);
        host.add_project = Some(HandoffAddProject {
            machine_id: machine_id.to_string(),
            waiting: true,
            added: None,
        });
        let mut message = json!({ "machineId": machine_id });
        if let Some(origin) = origin {
            message["cloneUrl"] = json!(origin);
        }
        self.open_gpui_add_project_modal(&message, cx);
    }

    /// Add Project is open for a handoff's destination: the project it registers is not activated (the
    /// handoff opens its own session there).
    pub(crate) fn git_export_add_project_waiting(&self) -> bool {
        self.gx_store
            .git
            .export
            .add_project
            .as_ref()
            .is_some_and(|add| add.waiting)
    }

    /// Add Project's `add` answer while the dialog waits on it.
    pub(crate) fn git_export_note_added_project(&mut self, machine_id: &str, answer: &Value) {
        let Some(add) = self
            .gx_store
            .git
            .export
            .add_project
            .as_mut()
            .filter(|add| add.waiting && add.machine_id == machine_id)
        else {
            return;
        };
        let Some(project) = answer.get("project") else {
            return;
        };
        let Some(project_id) = text(project, "projectId") else {
            return;
        };
        let path = text(project, "path");
        let title = text(project, "title")
            .or_else(|| text(project, "name"))
            .or_else(|| path.as_deref().map(folder_title))
            .unwrap_or_else(|| project_id.clone());
        add.added = Some(AddedProject {
            project_id,
            title,
            path,
        });
    }

    /// Add Project closed: Handoff / Export comes back with its destination, as the user left it,
    /// with the new project picked when one was added (cloned or opened).
    pub(crate) fn git_export_add_project_closed(&mut self, cx: &mut gpui::Context<Self>) {
        let host = &mut self.gx_store.git.export;
        let Some(add) = host.add_project.as_mut().filter(|add| add.waiting) else {
            return;
        };
        add.waiting = false;
        let added = add.added.clone();
        let machine_id = add.machine_id.clone();
        let Some(open_message) = host
            .request
            .as_ref()
            .map(|request| request.open_message.clone())
        else {
            return;
        };
        if let Some(picks) = host.restore.as_mut() {
            picks.machine_id = Some(machine_id);
            if let Some(added) = added {
                picks.project_id = Some(added.project_id);
            }
        }
        self.open_app_modal_from_bridge(open_message, cx);
    }

    /// Start for a handoff to somewhere else: on the session's own computer the transcript is already there;
    /// on another one it is saved there first. Then the session starts in the picked project.
    pub(super) fn git_start_remote_handoff(
        &mut self,
        request_id: &str,
        agent_id: Option<String>,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(exported) = self.git_take_exported(request_id) else {
            return;
        };
        let Some(agent) = self.git_handoff_agent(agent_id, &exported, cx) else {
            return;
        };
        let fail = |this: &mut Self, description: String, cx: &mut gpui::Context<Self>| {
            this.git_toast(
                GitToastLevel::Error,
                "Could not hand off to the other computer",
                ToastOptions::described(description),
                cx,
            );
        };
        let key = ProjectKey {
            machine: machine_key(machine_id),
            project_id: project_id.to_string(),
        };
        let Some(remote) = self.git_export_remote_for(&key.machine) else {
            return fail(self, "That computer is not connected.".to_string(), cx);
        };
        if key.machine == exported.project.machine {
            let Ok(scope) = self.git_scope_for_key(key) else {
                return self.git_toast(
                    GitToastLevel::Error,
                    "Could not start the conversation",
                    ToastOptions::described("That project is no longer available."),
                    cx,
                );
            };
            let path = exported.path.clone();
            return self.git_launch_handoff(&scope, agent, &exported, &path, cx);
        }
        let Some(markdown) = exported.markdown.clone() else {
            return fail(
                self,
                "The transcript did not come back from the export.".to_string(),
                cx,
            );
        };
        if markdown.len() > ATTACHMENT_MAX_BYTES {
            return fail(
                self,
                "This conversation is too long to carry to another computer. Turn off Commands & output or File changes and try again."
                    .to_string(),
                cx,
            );
        }
        // A project Add Project just registered can be missing from the stream for a moment;
        // gxserver there knows it, so the create goes ahead with the bare key.
        let scope = self.git_scope_for_key(key.clone()).unwrap_or(GitScope {
            key,
            remote: remote.clone(),
            title: String::new(),
            path: None,
            preferences: Default::default(),
            worktree: None,
            projectless: false,
        });
        let file_name = folder_title(&exported.path);
        cx.spawn(async move |this, cx| {
            let saved = calls::call(
                remote,
                "/api/saveSessionChatAttachment",
                json!({
                    "base64Data": base64::engine::general_purpose::STANDARD.encode(markdown.as_bytes()),
                    "suggestedName": file_name,
                }),
                Some(std::time::Duration::from_secs(60)),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                let path = saved
                    .map_err(|error| error.message)
                    .and_then(|answer| {
                        text(&answer, "path").ok_or_else(|| {
                            "The other computer did not say where it saved the transcript."
                                .to_string()
                        })
                    });
                match path {
                    Ok(path) => this.git_launch_handoff(&scope, agent, &exported, &path, cx),
                    Err(error) => fail(this, error, cx),
                }
            });
        })
        .detach();
    }
}
