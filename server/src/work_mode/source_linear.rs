//! Linear as a ticket source: tickets named `ACM-12`, matched in branches by the team keys the
//! Linear key can see, listed from the viewer's and the linked teams' open issues.

use std::borrow::Cow;

use serde_json::{json, Value};

use super::*;

pub(crate) struct LinearSource;

impl LinearSource {
    fn fingerprint(scope: &TicketScope) -> Option<u64> {
        scope
            .linear_api_key
            .map(linear_key_fingerprint)
            .or_else(|| project_linear_key(scope.project_id))
    }
}

fn ticket_of_list_issue(issue: LinearListIssue) -> Ticket {
    Ticket {
        status: linear_status(issue.state_type.as_deref(), issue.state_name.as_deref()),
        ticket: TicketRef::new(WorkSourceId::Linear, issue.identifier),
        title: Some(issue.title),
        url: issue.url,
        updated_at: issue.updated_at,
        state: issue.state_type,
        assigned_to_me: issue.assignee.as_ref().is_some_and(|person| person.is_me),
        assignee: issue.assignee.map(|assignee| WorkItemPerson {
            name: assignee.name,
            avatar_url: assignee.avatar_url,
            is_me: assignee.is_me,
        }),
        labels: issue.labels,
        cycle: issue.cycle,
        branch_name: issue.branch_name,
        project: issue.project_name.map(|name| WorkItemLink {
            name,
            url: issue.project_url,
        }),
        pull_request_urls: issue.pull_request_urls,
        team: issue.team_key,
    }
}

pub(crate) fn ticket_of_linear_issue(info: LinearIssueInfo) -> Ticket {
    Ticket {
        status: linear_status(info.state_type.as_deref(), info.state_name.as_deref()),
        ticket: TicketRef::new(WorkSourceId::Linear, info.identifier),
        title: info.title,
        url: info.url,
        updated_at: None,
        state: info.state_type,
        assignee: None,
        assigned_to_me: false,
        labels: Vec::new(),
        cycle: None,
        branch_name: None,
        project: info.project_name.map(|name| WorkItemLink {
            name,
            url: info.project_url,
        }),
        pull_request_urls: info.pull_request_urls,
        team: None,
    }
}

const NO_LINEAR_KEY: &str = "Set a Linear API key first (Settings, or ghostex issues linear-key).";

impl WorkSource for LinearSource {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("Linear")
    }

    fn link_kind(&self) -> &'static str {
        "linear"
    }

    fn key(&self, id: &str, _repo: &str) -> String {
        format!("linear:{id}")
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

    fn invalid_ref(&self, text: &str) -> String {
        format!("\"{text}\" is not a Linear issue ID like SPX-1245.")
    }

    fn project_kind(&self) -> Option<TicketProjectKind> {
        Some(TicketProjectKind::Linear)
    }

    fn team_of(&self, id: &str) -> Option<String> {
        id.split_once('-').map(|(team, _)| team.to_string())
    }

    fn parse_ref(&self, text: &str) -> Option<String> {
        normalize_linear_identifier(text)
    }

    fn can_read(&self, scope: &TicketScope) -> bool {
        scope.linear_api_key.is_some()
    }

    fn tickets_in_branch(&self, scope: &TicketScope, branch: &str) -> Vec<String> {
        Self::fingerprint(scope)
            .and_then(cached_linear_team_keys)
            .map(|team_keys| linear_identifiers_in_branch(branch, &team_keys))
            .unwrap_or_default()
    }

    fn branch_names(&self, branch: &str, id: &str) -> bool {
        self.team_of(id).is_some_and(|team| {
            linear_identifiers_in_branch(branch, &[team]).contains(&id.to_string())
        })
    }

    fn tickets_in_pull_request(
        &self,
        known_keys: &[String],
        pull_request: &PullRequestText,
    ) -> Vec<String> {
        linear_identifiers_in_branch(pull_request.head, known_keys)
    }

    fn list_holds_every_open_ticket(&self) -> bool {
        false
    }

    fn status_follows_pull_request(&self) -> bool {
        false
    }

    fn cached_list(&self, _projects: &[WorkProjectInput], plan: &WorkFeedPlan) -> SourceList {
        let mut list = SourceList::default();
        for request in &plan.linear {
            list.known_keys.extend(request.team_keys.iter().cloned());
            match cached_linear_feed(request) {
                Some(Ok(feed)) => {
                    list.known_keys.extend(
                        feed.issues
                            .iter()
                            .filter_map(|issue| issue.team_key.clone()),
                    );
                    list.tickets
                        .extend(feed.issues.into_iter().map(|issue| ListedTicket {
                            ticket: ticket_of_list_issue(issue),
                            project_id: None,
                            repo: String::new(),
                        }))
                }
                Some(Err(error)) => list.errors.push(format!("Linear: {error}")),
                None => {}
            }
        }
        list
    }

    fn cached_ticket(&self, scope: &TicketScope, id: &str) -> TicketRead {
        match Self::fingerprint(scope).map(|key| cached_linear_issue_read(key, id)) {
            Some(LinearIssueRead::Found(info)) => TicketRead::Found(ticket_of_linear_issue(info)),
            Some(LinearIssueRead::NotFound) => TicketRead::NotFound,
            Some(LinearIssueRead::Unread) | None => TicketRead::Unread,
        }
    }

    fn read_detail(&self, scope: &TicketScope, id: &str, force: bool) -> Result<Value, String> {
        let api_key = scope.linear_api_key.ok_or(NO_LINEAR_KEY)?;
        cached_detail(
            format!("linear:{}:{id}", linear_key_fingerprint(api_key)),
            force,
            || fetch_linear_issue_detail(api_key, id),
        )
    }

    fn create(&self, scope: &TicketScope, draft: &TicketDraft) -> Result<CreatedTicket, String> {
        let api_key = scope.linear_api_key.ok_or(NO_LINEAR_KEY)?;
        let extra = |key: &str| draft.extra.get(key).and_then(Value::as_str);
        let ticket = create_linear_issue(
            api_key,
            &NewLinearTicket {
                team_id: extra("teamId").ok_or("Pick a Linear team.")?,
                title: draft.title,
                description: draft.description,
                linear_project_id: extra("linearProjectId"),
                assign_to_me: draft.assign_to_me,
            },
        )?;
        Ok(CreatedTicket {
            ticket: TicketRef::new(WorkSourceId::Linear, ticket.identifier),
            url: ticket.url,
            branch_name: ticket.branch_name,
            repo: None,
        })
    }

    fn branch_for(&self, scope: &TicketScope, id: &str) -> Result<String, String> {
        let api_key = scope.linear_api_key.ok_or(NO_LINEAR_KEY)?;
        linear_issue_ticket(api_key, id)?
            .branch_name
            .ok_or_else(|| format!("Linear gave no branch name for {id}."))
    }

    fn tickets_attached_to(
        &self,
        scope: &TicketScope,
        url: &str,
        force: bool,
    ) -> Result<Vec<String>, String> {
        let api_key = scope.linear_api_key.ok_or(NO_LINEAR_KEY)?;
        let attached = cached_detail(
            format!("linear-attached:{}:{url}", linear_key_fingerprint(api_key)),
            force,
            || linear_issues_attached_to_url(api_key, url).map(|ids| json!(ids)),
        )?;
        Ok(attached
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect())
    }
}
