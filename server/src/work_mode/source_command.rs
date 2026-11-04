//! A workspace's ticket command as a ticket source: one `CommandSource` per id the store interned
//! (`CommandSourceId`). Its name, ID keys and credential fields are its workspace's own accepted
//! `info` (the scope's `effective_workspace_id`, else the project's workspace as last placed,
//! `cached_project_workspace_id`); only `ids.format` is id-wide.
//!
//! A session's card shows the tickets its branch names: the card pass queues exact reads
//! (`jobs.rs`) and the card reads what they kept (`caches.rs`), the ticket's last title when the
//! command can't be asked. A ticket's details are asked of the command while someone waits for
//! them (`detail.rs`), and the card pass checks the command's `info` again hourly
//! (`info_check.rs`). A workspace whose tracker it is lists its open tickets on the Work list
//! (`open_list.rs`). Creating a ticket comes later; a request for one gets a refusal in its own
//! words.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

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

    /// The scope's workspace: its own, else the project's as last placed (the projection's scopes
    /// name none).
    fn workspace_of(&self, scope: &TicketScope) -> Option<String> {
        scope
            .effective_workspace_id
            .map(str::to_string)
            .or_else(|| cached_project_workspace_id(scope.project_id))
    }

    /// The scope's workspace's command, when it pins this id.
    fn pinned(&self, scope: &TicketScope) -> Option<Arc<WorkspaceCommand>> {
        workspace_command(&self.workspace_of(scope)?).filter(|command| command.source_id == self.id)
    }

    /// The exact reads a card pass queues: per workspace that pins this id, the tickets its
    /// projects' sessions name that were never read at the workspace's generation or are no longer
    /// fresh, a session still waiting for its PR first, at most `MAX_COMMAND_CALLS_PER_PASS` calls
    /// of `MAX_EXACT_IDS`. Each project's placement is read once, and goes with the read, which
    /// runs only while the project is still placed there. Remembers whether each workspace's
    /// command can run, which the card reads.
    pub(crate) fn exact_reads_for(&self, pass: &TicketPass) -> Vec<ExactRead> {
        let Some(source) = CommandSourceId::known(self.id).map(WorkSourceId::Command) else {
            return Vec::new();
        };
        let placed: BTreeMap<&str, Option<String>> = pass
            .projects
            .keys()
            .map(|project_id| (*project_id, cached_project_workspace_id(project_id)))
            .collect();
        #[derive(Default)]
        struct Wanted {
            ids: Vec<String>,
            awaiting_pull_request: Vec<String>,
            projects: Vec<String>,
        }
        let mut wanted: BTreeMap<String, Wanted> = BTreeMap::new();
        for session in pass.sessions {
            let Some((project_id, project)) = session
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|project_id| pass.projects.get_key_value(project_id))
            else {
                continue;
            };
            let Some(Some(workspace_id)) = placed.get(*project_id) else {
                continue;
            };
            if !work_sources_of(Some(workspace_id)).contains(&source) {
                continue;
            }
            let targets = work_targets_in(project, session, Some(workspace_id));
            let ids = targets.ids_of(source);
            let entry = wanted.entry(workspace_id.clone()).or_default();
            if targets.pull_request.is_none() {
                entry.awaiting_pull_request.extend(ids.iter().cloned());
            }
            if !ids.is_empty() && !entry.projects.iter().any(|known| known == *project_id) {
                entry.projects.push(project_id.to_string());
            }
            entry.ids.extend(ids);
        }
        let mut reads = Vec::new();
        for (workspace_id, mut wanted) in wanted {
            let Some(plan) = self.plan_in(&workspace_id) else {
                continue;
            };
            let can_run = self.can_read(&TicketScope {
                effective_workspace_id: Some(&workspace_id),
                credentials: Some(pass.credentials),
                ..TicketScope::default()
            });
            remember_command_readiness(&workspace_id, plan.generation, can_run);
            if !can_run {
                continue;
            }
            let now = Instant::now();
            wanted.ids.sort();
            wanted.ids.dedup();
            wanted.ids.retain(|id| command_ticket_due(&plan, id, now));
            wanted
                .ids
                .sort_by_key(|id| !wanted.awaiting_pull_request.contains(id));
            for chunk in wanted
                .ids
                .chunks(MAX_EXACT_IDS)
                .take(MAX_COMMAND_CALLS_PER_PASS)
            {
                reads.push(ExactRead {
                    plan: plan.clone(),
                    ids: chunk.to_vec(),
                    projects: wanted.projects.clone(),
                    paths: pass.paths.clone(),
                });
            }
        }
        reads
    }

    /// The hourly checks a card pass queues: per workspace its projects are placed in that pins
    /// this id and whose command can run, when its accepted `info` is due (`info_check_wanted`).
    pub(crate) fn info_checks_for(&self, pass: &TicketPass) -> Vec<InfoCheck> {
        let mut workspaces: Vec<String> = pass
            .projects
            .keys()
            .filter_map(|project_id| cached_project_workspace_id(project_id))
            .collect();
        workspaces.sort();
        workspaces.dedup();
        workspaces
            .into_iter()
            .filter_map(|workspace_id| {
                let command = workspace_command(&workspace_id)
                    .filter(|command| command.source_id == self.id)?;
                let plan = self.plan_in(&workspace_id)?;
                let can_run = self.can_read(&TicketScope {
                    effective_workspace_id: Some(&workspace_id),
                    credentials: Some(pass.credentials),
                    ..TicketScope::default()
                });
                (can_run && info_check_wanted(&plan, &command.checked_at)).then(|| InfoCheck {
                    plan,
                    paths: pass.paths.clone(),
                })
            })
            .collect()
    }

    /// The workspace's plan for this id at its generation now.
    fn plan_in(&self, workspace_id: &str) -> Option<JobPlan> {
        Some(JobPlan {
            workspace_id: workspace_id.to_string(),
            source: CommandSourceId::known(self.id)?,
            generation: work_source_generation(workspace_id),
        })
    }

    /// The Work list's open list of each workspace whose tracker this command is, for its projects
    /// there.
    fn open_lists(&self, projects: &[WorkProjectInput]) -> Vec<OpenList> {
        let Some(source) = CommandSourceId::known(self.id) else {
            return Vec::new();
        };
        let mut by_workspace: BTreeMap<&str, Vec<&WorkProjectInput>> = BTreeMap::new();
        for input in projects
            .iter()
            .filter(|input| input.tracker == WorkSourceId::Command(source))
        {
            by_workspace
                .entry(input.workspace_id.as_str())
                .or_default()
                .push(input);
        }
        by_workspace
            .into_iter()
            .filter_map(|(workspace_id, inputs)| {
                let plan = self.plan_in(workspace_id)?;
                let credentials = &inputs.first()?.credentials;
                let blocked = self.blocked_reason(&TicketScope {
                    effective_workspace_id: Some(workspace_id),
                    credentials: Some(credentials),
                    ..TicketScope::default()
                });
                let projects = inputs
                    .iter()
                    .map(|input| CommandProject {
                        id: input.project_id.clone(),
                        name: input.name.clone(),
                        path: input.path.clone(),
                        repo: input.repo.as_deref().map(str::to_ascii_lowercase),
                    })
                    .collect();
                Some(OpenList::new(
                    plan,
                    projects,
                    blocked,
                    credentials.paths().clone(),
                ))
            })
            .collect()
    }

    /// Why the command can't be asked for the scope right now, in words for the Work page; `None`
    /// when it can (`can_read`). Through the command's mask, since it names a credential's label.
    fn blocked_reason(&self, scope: &TicketScope) -> Option<String> {
        let name = self.display_name(scope);
        let (Some(command), Some(workspace_id), Some(credentials)) = (
            self.pinned(scope),
            self.workspace_of(scope),
            scope.credentials,
        ) else {
            return Some(format!("{name} isn't this workspace's ticket command."));
        };
        let workspace_id = workspace_id.as_str();
        if credentials.workspace_has_team(workspace_id) {
            return Some(TEAM_REFUSAL.to_string());
        }
        if !ghostex_paths::powershell::launchable(&command.resolved_path) {
            return Some(format!(
                "{name} can't be started: its program is gone or can't run. Set its path again with `ghostex issues source set <path>`."
            ));
        }
        let stored = credentials.source_fields(workspace_id, self.id);
        let missing = command
            .info
            .credentials
            .iter()
            .filter(|field| field.required)
            .find(|field| {
                stored
                    .and_then(|stored| stored.get(&field.name))
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.trim().is_empty())
            })?;
        Some(
            command_env_of(credentials.paths(), workspace_id, Some(&command)).shown(&format!(
                "Set {name}'s {} first: `ghostex issues source credential {}`.",
                missing.label, missing.name
            )),
        )
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

    /// The open list of each workspace whose tracker this command is (`open_lists`).
    fn plan_list(&self, projects: &[WorkProjectInput], plan: &mut WorkFeedPlan) {
        plan.command_lists.extend(self.open_lists(projects));
    }

    fn list_stale(&self, plan: &WorkFeedPlan) -> bool {
        plan.command_lists
            .iter()
            .filter(|list| list.plan.source.as_str() == self.id)
            .any(open_list_stale)
    }

    /// Queues each of this command's lists that is stale (all of them when forced, back-offs
    /// aside); never waits for the command.
    fn refresh_list(&self, plan: &WorkFeedPlan, force: bool) {
        for list in plan
            .command_lists
            .iter()
            .filter(|list| list.plan.source.as_str() == self.id)
            .filter(|list| list.blocked.is_none() && (force || open_list_stale(list)))
        {
            queue_open_list(list.clone());
        }
    }

    /// Queues the exact reads the pass's sessions need (`exact_reads_for`) and the hourly checks
    /// that are due (`info_checks_for`); never waits for the command.
    fn refresh_tickets(&self, pass: &TicketPass) {
        for read in self.exact_reads_for(pass) {
            queue_exact_read(read);
        }
        for check in self.info_checks_for(pass) {
            queue_info_check(check);
        }
    }

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
        self.blocked_reason(scope).is_none()
    }

    /// What each of this command's lists last answered at its workspace's generation: its tickets,
    /// each with the workspace it was read in, or why it can't be read.
    fn cached_list(&self, _projects: &[WorkProjectInput], plan: &WorkFeedPlan) -> SourceList {
        let mut list = SourceList::default();
        let Some(source) = CommandSourceId::known(self.id) else {
            return list;
        };
        for open in plan
            .command_lists
            .iter()
            .filter(|open| open.plan.source == source)
        {
            if let Some(blocked) = &open.blocked {
                list.errors.push(blocked.clone());
                continue;
            }
            match cached_open_list(&open.plan) {
                None => {}
                Some(Err(error)) => list.errors.push(error),
                Some(Ok(tickets)) => {
                    list.tickets
                        .extend(tickets.into_iter().map(|ticket| ListedTicket {
                            project_id: ticket.project_id.clone(),
                            ticket: ticket_of_command(source, ticket),
                            repo: String::new(),
                            workspace_id: Some(open.plan.workspace_id.clone()),
                        }))
                }
            }
        }
        list
    }

    /// What the last exact read at the workspace's generation kept. The command can't be asked
    /// (no longer pinned here, the workspace in a team, the last pass found it unable to run, or
    /// its read failed before it ever found the ticket): `Unavailable`, with the ticket's last
    /// title. Nothing read yet at this generation: `Unread`.
    fn cached_ticket(&self, scope: &TicketScope, id: &str) -> TicketRead {
        let Some(workspace_id) = self.workspace_of(scope) else {
            return TicketRead::Unavailable { title: None };
        };
        let unavailable = || TicketRead::Unavailable {
            title: last_ticket_title(&workspace_id, self.id, id),
        };
        let Some(plan) = self
            .pinned(scope)
            .filter(|_| !cached_workspace_in_team(&workspace_id))
            .and_then(|_| self.plan_in(&workspace_id))
        else {
            return unavailable();
        };
        if command_readiness(&workspace_id, plan.generation) == Some(false) {
            return unavailable();
        }
        match cached_command_ticket(&plan, id) {
            None => TicketRead::Unread,
            Some(CachedTicket::Found(ticket)) => TicketRead::Found(ticket),
            Some(CachedTicket::NotFound) => TicketRead::NotFound,
            Some(CachedTicket::Failed) => unavailable(),
        }
    }

    /// Asks the scope's workspace's command (`run_detail`), through the details cache at the
    /// workspace's generation and one call for every caller asking at the same time. Each caller's
    /// own plan is checked again after that answers (`detail_still_planned`): a cached answer, or
    /// one read for another project of the workspace, reaches only a project still placed there.
    fn read_detail(&self, scope: &TicketScope, id: &str, force: bool) -> Result<Value, String> {
        let name = self.display_name(scope);
        let read = match (self.workspace_of(scope), scope.credentials) {
            (Some(workspace_id), Some(credentials)) if self.can_read(scope) => {
                self.plan_in(&workspace_id).map(|plan| DetailRead {
                    plan,
                    id: id.to_string(),
                    project: [scope.project_id.to_string()],
                    paths: credentials.paths().clone(),
                })
            }
            _ => None,
        }
        .ok_or_else(|| format!("Ghostex can't ask {name} for this ticket right now."))?;
        let key = read.cache_key();
        let changed_meanwhile = |answer: &Result<Value, String>| {
            answer
                .as_ref()
                .is_err_and(|message| message == DETAIL_CHANGED_MEANWHILE)
        };
        let answer = cached_detail(key.clone(), force, || {
            single_flight(&key, || run_detail(&read, &name))
        });
        // A "changed meanwhile" can be another project's, one that moved while the shared call
        // read; the generation doesn't move with a project, so it is never kept for the others.
        let shared_changed = changed_meanwhile(&answer);
        if shared_changed {
            forget_detail(&key);
        }
        // Checked for a failure too: its words are about the workspace's command.
        if !detail_still_planned(&read) {
            return Err(DETAIL_CHANGED_MEANWHILE.to_string());
        }
        if !shared_changed {
            return answer;
        }
        // This caller is still placed, so it reads once more, on its own.
        let answer = cached_detail(key.clone(), true, || run_detail(&read, &name));
        if changed_meanwhile(&answer) {
            forget_detail(&key);
        }
        answer
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

/// The tickets a branch names of a ticket command the workspace no longer reads (not among
/// `sources`: cleared, switched from, or stopped in a team): only those it answered for there,
/// which have a last title, by the format its id still has on this computer. Its ID keys went with
/// its `info`, so a key-number id's key is taken from the titled tickets themselves.
pub(crate) fn unread_command_tickets_in_branch(
    workspace_id: &str,
    sources: &[WorkSourceId],
    branch: &str,
) -> Vec<TicketRef> {
    let mut tickets = Vec::new();
    for (source_id, ids) in titled_tickets(workspace_id) {
        let Some(id) = CommandSourceId::known(&source_id) else {
            continue;
        };
        let source = WorkSourceId::Command(id);
        if sources.contains(&source) {
            continue;
        }
        let named: Vec<String> = match id_format(id) {
            Some(IdFormat::KeyNumber) => {
                let mut keys: Vec<String> = ids
                    .iter()
                    .filter_map(|ticket| ticket.split_once('-').map(|(key, _)| key.to_string()))
                    .collect();
                keys.sort();
                keys.dedup();
                linear_identifiers_in_branch(branch, &keys)
            }
            Some(IdFormat::Number) => github_issue_in_branch(branch)
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            None => Vec::new(),
        };
        tickets.extend(
            named
                .into_iter()
                .filter(|ticket| ids.contains(ticket))
                .map(|ticket| TicketRef::new(source, ticket)),
        );
    }
    tickets
}
