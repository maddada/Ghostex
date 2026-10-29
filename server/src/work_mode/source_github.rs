//! GitHub issues as a ticket source: tickets named by number in the project's repo, matched by a
//! branch whose last segment starts with the number, listed from each repo's open issues through
//! `gh` (the same `gh` pass that lists the repo's PRs, which stay the code host's).

use std::borrow::Cow;

use serde_json::Value;

use super::*;

pub(crate) struct GithubSource;

fn number_of(id: &str) -> Result<u64, String> {
    id.parse()
        .map_err(|_| format!("\"{id}\" is not a GitHub issue number."))
}

impl WorkSource for GithubSource {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("GitHub")
    }

    fn link_kind(&self) -> &'static str {
        "issue"
    }

    fn key(&self, id: &str, repo: &str) -> String {
        format!("issue:{repo}#{id}")
    }

    fn label(&self, id: &str) -> String {
        format!("#{id}")
    }

    fn untitled(&self, id: &str) -> String {
        format!("Issue #{id}")
    }

    fn keys_name_a_repo(&self) -> bool {
        true
    }

    fn invalid_ref(&self, _text: &str) -> String {
        "ticketId must be a GitHub issue number.".to_string()
    }

    fn project_kind(&self) -> Option<TicketProjectKind> {
        Some(TicketProjectKind::Github)
    }

    fn team_of(&self, _id: &str) -> Option<String> {
        None
    }

    fn parse_ref(&self, text: &str) -> Option<String> {
        text.trim()
            .trim_start_matches('#')
            .parse::<u64>()
            .ok()
            .filter(|number| *number > 0)
            .map(|number| number.to_string())
    }

    fn can_read(&self, scope: &TicketScope) -> bool {
        scope.cwd.is_some() && crate::session_git_status::gh_cli_is_available()
    }

    fn tickets_in_branch(&self, _scope: &TicketScope, branch: &str) -> Vec<String> {
        github_issue_in_branch(branch)
            .map(|number| number.to_string())
            .into_iter()
            .collect()
    }

    fn branch_names(&self, branch: &str, id: &str) -> bool {
        github_issue_in_branch(branch).map(|number| number.to_string()) == Some(id.to_string())
    }

    /// The issue the head branch starts with, then the ones the description closes.
    fn tickets_in_pull_request(
        &self,
        _known_keys: &[String],
        pull_request: &PullRequestText,
    ) -> Vec<String> {
        github_issue_in_branch(pull_request.head)
            .into_iter()
            .chain(
                pull_request
                    .body
                    .map(closing_issue_numbers)
                    .unwrap_or_default(),
            )
            .map(|number| number.to_string())
            .collect()
    }

    /// The `gh` feed lists every open issue of the repo.
    fn list_holds_every_open_ticket(&self) -> bool {
        true
    }

    fn status_follows_pull_request(&self) -> bool {
        true
    }

    fn cached_list(&self, projects: &[WorkProjectInput], plan: &WorkFeedPlan) -> SourceList {
        let viewer_login = cached_github_viewer_login();
        let is_me = |login: &str| {
            viewer_login
                .as_deref()
                .is_some_and(|viewer| viewer.eq_ignore_ascii_case(login))
        };
        let mut list = SourceList::default();
        for (cwd, project_id) in &plan.github {
            let Some(input) = projects
                .iter()
                .find(|input| input.project_id == *project_id)
                .filter(|input| input.tracker == WorkSourceId::Github)
            else {
                continue;
            };
            // A failed `gh` list is reported once, by the PR pass that shares it (items.rs).
            let Some(Ok(feed)) = cached_github_feed(cwd) else {
                continue;
            };
            let repo = feed
                .repo
                .clone()
                .or_else(|| input.repo.clone())
                .unwrap_or_else(|| input.project_id.clone());
            for issue in feed.issues {
                list.tickets.push(ListedTicket {
                    ticket: Ticket {
                        ticket: TicketRef::new(WorkSourceId::Github, issue.number.to_string()),
                        title: Some(issue.title),
                        url: issue.url,
                        updated_at: issue.updated_at,
                        state: Some("open".to_string()),
                        status: github_issue_status(issue.project.as_ref()),
                        assigned_to_me: issue.assignees.iter().any(|login| is_me(login)),
                        assignee: issue.assignees.first().map(|login| WorkItemPerson {
                            name: login.clone(),
                            avatar_url: None,
                            is_me: is_me(login),
                        }),
                        labels: issue.labels,
                        cycle: None,
                        branch_name: None,
                        project: issue.project.map(|project| WorkItemLink {
                            name: project.title,
                            url: project.url,
                        }),
                        pull_request_urls: Vec::new(),
                        team: None,
                    },
                    project_id: Some(input.project_id.clone()),
                    repo: repo.clone(),
                });
            }
        }
        list
    }

    fn cached_ticket(&self, scope: &TicketScope, id: &str) -> TicketRead {
        let Some(info) = scope
            .cwd
            .zip(id.parse().ok())
            .and_then(|(cwd, number)| cached_work_github_issue(cwd, number))
        else {
            return TicketRead::Unread;
        };
        let closed = info.state.as_deref() == Some("closed");
        TicketRead::Found(Ticket {
            ticket: TicketRef::new(WorkSourceId::Github, id),
            title: info.title,
            url: info.url,
            updated_at: None,
            state: info.state,
            status: WorkItemStatus {
                group: if closed { "closed" } else { "todo" },
                name: if closed { "Closed" } else { "Open" }.to_string(),
            },
            assignee: None,
            assigned_to_me: false,
            labels: Vec::new(),
            cycle: None,
            branch_name: None,
            project: None,
            pull_request_urls: Vec::new(),
            team: None,
        })
    }

    fn read_detail(&self, scope: &TicketScope, id: &str, force: bool) -> Result<Value, String> {
        let number = number_of(id)?;
        let cwd = scope.cwd.ok_or("This project has no folder.")?;
        cached_detail(format!("issue:{cwd}#{number}"), force, || {
            fetch_github_issue_detail(cwd, number)
        })
    }

    fn create(&self, scope: &TicketScope, draft: &TicketDraft) -> Result<CreatedTicket, String> {
        let cwd = scope.cwd.ok_or("This project has no folder.")?;
        let created = create_github_issue(
            cwd,
            &NewGithubIssue {
                title: draft.title,
                body: draft.description,
                assign_to_me: draft.assign_to_me,
                repo: draft.extra.get("repo").and_then(Value::as_str),
            },
        )?;
        Ok(CreatedTicket {
            ticket: TicketRef::new(WorkSourceId::Github, created.number.to_string()),
            url: Some(created.url),
            branch_name: None,
            repo: Some(created.repo),
        })
    }

    fn branch_for(&self, scope: &TicketScope, id: &str) -> Result<String, String> {
        let number = number_of(id)?;
        github_issue_branch(scope.cwd.unwrap_or_default(), number).map_err(|error| error.message)
    }

    /// GitHub ties issues to PRs through the PR itself (its branch and description).
    fn tickets_attached_to(
        &self,
        _scope: &TicketScope,
        _url: &str,
        _force: bool,
    ) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}
