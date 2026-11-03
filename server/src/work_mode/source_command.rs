//! A workspace's ticket command as a ticket source: one `CommandSource` per id the store interned
//! (`CommandSourceId`). Its name, ID keys and credential fields are its workspace's own accepted
//! `info` (the scope's `effective_workspace_id`); only `ids.format` is id-wide.
//!
//! It is not yet among the sources a session's card or the Work list reads (`BUILT_IN`), so
//! nothing shows its tickets until it can read them. A request that names its id gets its rules
//! (a reference, a key, its name) and a refusal for what it can't do yet.

use std::borrow::Cow;
use std::sync::Arc;

use serde_json::Value;

use super::*;

pub(crate) struct CommandSource {
    id: &'static str,
}

impl CommandSource {
    pub(crate) fn new(id: &'static str) -> Self {
        Self { id }
    }

    pub(crate) fn id(&self) -> &'static str {
        self.id
    }

    fn format(&self) -> Option<IdFormat> {
        CommandSourceId::known(self.id).and_then(id_format)
    }

    /// The scope's workspace's command, when it pins this id.
    fn pinned(&self, scope: &TicketScope) -> Option<Arc<WorkspaceCommand>> {
        workspace_command(scope.effective_workspace_id?)
            .filter(|command| command.source_id == self.id)
    }

    /// The workspace's `info.name`; else the name its retired record kept; else the id.
    fn display_name(&self, scope: &TicketScope) -> String {
        self.pinned(scope)
            .map(|command| command.info.name.clone())
            .or_else(|| {
                CommandSourceId::known(self.id)
                    .and_then(retired_source)
                    .map(|retired| retired.name.clone())
            })
            .unwrap_or_else(|| self.id.to_string())
    }
}

impl WorkSource for CommandSource {
    fn name(&self, scope: &TicketScope) -> Cow<'static, str> {
        Cow::Owned(self.display_name(scope))
    }

    fn link_kind(&self) -> Cow<'static, str> {
        Cow::Borrowed(self.id)
    }

    fn key(&self, id: &str, _repo: &str) -> String {
        format!("{}:{id}", self.id)
    }

    fn label(&self, id: &str) -> String {
        id.to_string()
    }

    fn untitled(&self, id: &str) -> String {
        id.to_string()
    }

    fn keys_name_a_repo(&self) -> bool {
        false
    }

    /// Never quotes what was typed: a request error's message goes to the request log, and a
    /// typed id can be a credential the command keeps.
    fn invalid_ref(&self, _text: &str) -> String {
        match self.format() {
            Some(IdFormat::Number) => {
                format!("That isn't a {} ticket ID: a number, like 218.", self.id)
            }
            _ => format!(
                "That isn't a {} ticket ID: a key and a number, like ABC-12.",
                self.id
            ),
        }
    }

    /// A key-number ticket's team is its id's key (`ABC` of `ABC-12`); a number source's ids name
    /// no team.
    fn team_of(&self, id: &str) -> Option<String> {
        match self.format()? {
            IdFormat::KeyNumber => id.split_once('-').map(|(team, _)| team.to_string()),
            IdFormat::Number => None,
        }
    }

    fn parse_ref(&self, text: &str) -> Option<String> {
        self.format()?.parse_ref(text)
    }

    /// The scope workspace's ID keys for a key-number source; the issue number a branch starts
    /// with for a number source.
    fn tickets_in_branch(&self, scope: &TicketScope, branch: &str) -> Vec<String> {
        let Some(command) = self.pinned(scope) else {
            return Vec::new();
        };
        match command.info.id_format {
            IdFormat::KeyNumber => linear_identifiers_in_branch(branch, &command.info.id_keys),
            IdFormat::Number => github_issue_in_branch(branch)
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
        }
    }

    fn branch_names(&self, branch: &str, id: &str) -> bool {
        match self.format() {
            Some(IdFormat::KeyNumber) => self.team_of(id).is_some_and(|team| {
                linear_identifiers_in_branch(branch, &[team]).contains(&id.to_string())
            }),
            Some(IdFormat::Number) => {
                github_issue_in_branch(branch).map(|number| number.to_string())
                    == Some(id.to_string())
            }
            None => false,
        }
    }

    /// The PR's head branch only, never its description.
    fn tickets_in_pull_request(
        &self,
        known_keys: &[String],
        pull_request: &PullRequestText,
    ) -> Vec<String> {
        match self.format() {
            Some(IdFormat::KeyNumber) => {
                linear_identifiers_in_branch(pull_request.head, known_keys)
            }
            Some(IdFormat::Number) => github_issue_in_branch(pull_request.head)
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            None => Vec::new(),
        }
    }

    /// A ticket command's open list is not read yet.
    fn plan_list(&self, _projects: &[WorkProjectInput], _plan: &mut WorkFeedPlan) {}

    fn list_stale(&self, _plan: &WorkFeedPlan) -> bool {
        false
    }

    fn refresh_list(&self, _plan: &WorkFeedPlan, _force: bool) {}

    /// Nothing reads its tickets yet.
    fn refresh_tickets(&self, _pass: &TicketPass) {}

    fn known_keys(&self, scope: &TicketScope, _plan: &WorkFeedPlan) -> Vec<String> {
        self.pinned(scope)
            .map(|command| command.info.id_keys.clone())
            .unwrap_or_default()
    }

    fn list_holds_every_open_ticket(&self) -> bool {
        false
    }

    fn status_follows_pull_request(&self) -> bool {
        false
    }

    /// Pinned in the scope's workspace, which has no team, its program still there, and every
    /// credential its `info` requires set.
    fn can_read(&self, scope: &TicketScope) -> bool {
        let (Some(command), Some(workspace_id), Some(credentials)) = (
            self.pinned(scope),
            scope.effective_workspace_id,
            scope.credentials,
        ) else {
            return false;
        };
        if credentials.workspace_has_team(workspace_id)
            || !ghostex_paths::powershell::launchable(&command.resolved_path)
        {
            return false;
        }
        let stored = credentials.source_fields(workspace_id, self.id);
        command
            .info
            .credentials
            .iter()
            .filter(|field| field.required)
            .all(|field| {
                stored
                    .and_then(|stored| stored.get(&field.name))
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            })
    }

    fn cached_list(&self, _projects: &[WorkProjectInput], _plan: &WorkFeedPlan) -> SourceList {
        SourceList::default()
    }

    /// Nothing has read it.
    fn cached_ticket(&self, _scope: &TicketScope, _id: &str) -> TicketRead {
        TicketRead::Unavailable { title: None }
    }

    fn read_detail(&self, scope: &TicketScope, _id: &str, _force: bool) -> Result<Value, String> {
        Err(format!(
            "Ghostex can't open {} tickets yet.",
            self.display_name(scope)
        ))
    }

    fn create(&self, scope: &TicketScope, _draft: &TicketDraft) -> Result<CreatedTicket, String> {
        Err(format!(
            "Ghostex can't create {} tickets yet.",
            self.display_name(scope)
        ))
    }

    fn branch_for(&self, scope: &TicketScope, _id: &str) -> Result<String, String> {
        Err(format!(
            "Ghostex can't start work on {} tickets yet.",
            self.display_name(scope)
        ))
    }
}
