//! What Autopilot does with a prompt that waits on the user: an agent's question (answered with its
//! recommended choice after the user's wait, or handed to the supervisor when it has none), a
//! Ghostex question form (the same), and approval cards: a permission prompt (stored by the hooks
//! or read off the screen), a Connectors card, or folder trust. Approvals are answered by Ghostex
//! itself, never by an agent, only while "When it asks permission" is Safe steps or All but limits,
//! after the user's minute, and never across a hard limit (which counts only on All but limits);
//! one that would cross it, or that only a human can do (sign in, 2FA, identity, payment), is
//! declined and the agent is told to carry on without it.
//!
//! SEE-ALSO: server/src/coordinators/relay.rs (CDXC:Autopilot 2026-11-07 DECISION on the approval
//! rule), server/src/session_chat_send/answer_http.rs (the answer path), server/src/server/autopilot_runtime.rs.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::autopilot::{AutopilotConfig, HardLimits, Tier};
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
};

/// The tool name a Connectors approval card's question carries (connectors/agent/cards.rs).
const CONNECTOR_CARD_TOOL: &str = "ghostex_connector_approval";

/// What one prompt gets.
pub(super) enum PromptAct {
    /// Not yet (the user's wait), or nothing Autopilot may do: leave it.
    Wait,
    /// Send this answer (`/api/answerSessionChatPrompt` params, without the session's ids).
    Answer { params: Value, did: String },
    /// Decline it with this answer, then tell the agent `message`; logged as left for the user.
    Decline {
        params: Value,
        message: String,
        note: String,
    },
    /// Leave it for the user, logged once with this note.
    Leave { note: String },
    /// An approval card in the user's minute: nothing yet, look again in this many milliseconds.
    Hold { after_ms: i64 },
    /// Answer the session's open Ghostex forms with their recommended choices.
    Forms,
    /// Hand it to the supervisor.
    Judge { why: String, detail: String },
}

pub(super) struct PromptDecision {
    pub rule: &'static str,
    pub saw: String,
    pub act: PromptAct,
}

/// What Autopilot remembers about one session's prompts.
#[derive(Default)]
pub(super) struct PromptMemory {
    /// The prompt on screen and when Autopilot first saw it.
    seen: Option<(String, i64)>,
    /// Prompts already acted on or left, so each gets one decision.
    done: HashSet<String>,
}

impl PromptMemory {
    pub(super) fn mark_done(&mut self, fingerprint: &str) {
        if self.done.len() > 64 {
            self.done.clear();
        }
        self.done.insert(fingerprint.to_string());
    }
}

/// What an approval asks, sorted by what the user allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// On the allow list: reads, builds, tests, edits and local commits inside the project.
    Safe,
    /// Anything else that crosses no limit.
    Plain,
    Limit(Limit),
    /// What only a human can do.
    Never(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Limit {
    PushBranch,
    FinishRelease,
    PushMain,
    Merge,
    DeleteOutside,
    PostPublicly,
}

impl Limit {
    fn allowed(self, limits: &HardLimits) -> bool {
        match self {
            Self::PushBranch => limits.push_branch,
            Self::FinishRelease => limits.finish_release,
            Self::PushMain => limits.push_main,
            Self::Merge => limits.merge,
            Self::DeleteOutside => limits.delete_outside_project,
            Self::PostPublicly => limits.post_publicly,
        }
    }

    fn words(self) -> &'static str {
        match self {
            Self::PushBranch => "pushing a branch",
            Self::FinishRelease => "a release step (publish, release workflow)",
            Self::PushMain => "pushing to main",
            Self::Merge => "merging a pull request",
            Self::DeleteOutside => "deleting outside the project",
            Self::PostPublicly => "posting publicly",
        }
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

/// Sorts what an approval asks by its tool and command (or a Connectors call's text).
///
/// CDXC:Autopilot 2026-11-07 DECISION:
/// User: the hard limits Autopilot may cross are each a toggle, off by default ("push a branch, finish a release, push main, merge, delete outside the project, post publicly"), and "Sign-ins, 2FA, identity checks and payments are never done". The allow list of the Heal and decide tier follows the design page: builds, tests, reads, edits and deletes inside the project, local commits.
fn classify(tool: &str, text: &str, project_path: Option<&str>) -> Class {
    let tool = tool.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    let all = format!("{tool} {text}");
    if contains_any(
        &all,
        &[
            " login",
            "auth login",
            "/login",
            "sign in",
            "signin",
            "2fa",
            "otp ",
            "one-time code",
            "passkey",
            "verify your identity",
            "identity verification",
            "payment",
            "checkout",
            "purchase",
            "billing",
            "credit card",
            "subscribe",
        ],
    ) {
        return Class::Never("a sign-in, identity check or payment");
    }
    if contains_any(
        &all,
        &[
            "gh pr merge",
            "git merge --ff",
            "merge_pull_request",
            "mergepullrequest",
        ],
    ) {
        return Class::Limit(Limit::Merge);
    }
    if contains_any(
        &all,
        &[
            "gh release",
            "release:",
            "npm publish",
            "cargo publish",
            "bun publish",
            "pnpm publish",
            "gh workflow run",
            "release_workflow",
            "eas update",
            "release:ota",
        ],
    ) {
        return Class::Limit(Limit::FinishRelease);
    }
    if all.contains("git push") {
        let to_main = contains_any(
            &all,
            &[
                " main",
                " master",
                ":main",
                ":master",
                "head:main",
                "origin main",
                "origin master",
            ],
        );
        return Class::Limit(if to_main {
            Limit::PushMain
        } else {
            Limit::PushBranch
        });
    }
    if contains_any(
        &all,
        &[
            "gh pr comment",
            "gh issue comment",
            "gh pr review",
            "gh issue create",
            "gh pr create",
            "discord",
            "twitter",
            "x.com/",
            "tweet",
            "slack",
            "sendmail",
            "send_email",
            "gmail_send",
            "send email",
            "post_message",
            "create_post",
            "postmessage",
            "webhook",
        ],
    ) {
        return Class::Limit(Limit::PostPublicly);
    }
    if deletes_outside(&text, project_path) {
        return Class::Limit(Limit::DeleteOutside);
    }
    let safe_tools = [
        "read",
        "glob",
        "grep",
        "ls",
        "notebookread",
        "webfetch",
        "websearch",
        "todowrite",
        "edit",
        "write",
        "multiedit",
        "notebookedit",
    ];
    if safe_tools.contains(&tool.as_str()) {
        return Class::Safe;
    }
    let safe_commands = [
        "ls",
        "cat",
        "head",
        "tail",
        "rg",
        "grep",
        "find",
        "wc",
        "pwd",
        "echo",
        "git status",
        "git diff",
        "git log",
        "git show",
        "git add",
        "git commit",
        "git branch",
        "git switch",
        "git checkout",
        "git stash",
        "cargo build",
        "cargo check",
        "cargo test",
        "cargo clippy",
        "cargo fmt",
        "cargo xtask",
        "bun test",
        "bun run",
        "bun install",
        "npm test",
        "npm run",
        "npm install",
        "pnpm",
        "yarn",
        "make",
        "pytest",
        "go test",
        "go build",
        "tsc",
        "node ",
        "python",
        "mkdir",
        "touch",
        "cp ",
        "mv ",
        "rm ",
        "sleep",
    ];
    let command = text.trim_start();
    if (tool.is_empty() || tool == "bash" || tool == "shell" || tool == "exec")
        && safe_commands.iter().any(|safe| command.starts_with(safe))
    {
        return Class::Safe;
    }
    // A Connectors call that only reads.
    if contains_any(
        &all,
        &[
            "_get", "_list", "_search", "_read", "_fetch", " get ", " list ",
        ],
    ) && !contains_any(&all, &["_create", "_update", "_delete", "_send", "_post"])
    {
        return Class::Safe;
    }
    Class::Plain
}

/// An `rm` (or similar) that names an absolute path outside the project, the home folder, or a
/// parent folder.
fn deletes_outside(text: &str, project_path: Option<&str>) -> bool {
    let deletes = contains_any(
        text,
        &[
            "rm ",
            "rm -",
            "rmdir ",
            "trash ",
            "unlink ",
            "shred ",
            // PowerShell and cmd, where the same delete has other names.
            "remove-item",
            " del ",
            " erase ",
            " rd ",
        ],
    );
    if !deletes {
        return false;
    }
    let project = project_path.map(|path| path.to_ascii_lowercase().replace('\\', "/"));
    text.split_whitespace().any(|word| {
        let word = word.trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ';' || c == ')');
        let word = word.replace('\\', "/");
        if word.starts_with("~")
            || word.starts_with("$home")
            || word.starts_with("$env:userprofile")
            || word.starts_with("..")
        {
            return true;
        }
        // A cmd switch such as `/s` or `/q` is not a path.
        let absolute = (word.starts_with('/') && word.len() > 2)
            || word.as_bytes().get(1) == Some(&b':') && word.as_bytes().get(2) == Some(&b'/');
        absolute
            && !word.starts_with("/tmp")
            && !word.starts_with("/private/tmp")
            && project
                .as_deref()
                .is_none_or(|project| !word.starts_with(project))
    })
}

enum Verdict {
    Approve,
    /// The note for the user, and why, in the agent's terms.
    Decline(String, String),
    Leave(String),
    Wait,
    /// What it would do, held for the user's minute: look again in this many milliseconds.
    Hold(i64),
}

/// How long an approval card waits for the user before Autopilot answers it.
///
/// CDXC:Autopilot 2026-11-14 DECISION:
/// User, on the Autopilot design page (Q3034, "1 minute"): approval cards were answered at Autopilot's next look, so a card for `rm -rf build/` could be approved while the user sat reading it. Every card (approve or decline) now waits a minute from when Autopilot first sees it, so a user at the desk answers it themselves; away, it is answered a minute later.
const APPROVAL_WAIT_MS: i64 = 60_000;

/// Holds a verdict that would act until the card has waited its minute.
fn after_users_minute(verdict: Verdict, waited: i64) -> Verdict {
    let wait = crate::coordinators::test_scaled(APPROVAL_WAIT_MS);
    match verdict {
        Verdict::Wait => Verdict::Wait,
        _ if waited < wait => Verdict::Hold(wait - waited),
        verdict => verdict,
    }
}

/// What the user's choices make of one approval. `read_only` is a read-only thread's card (not
/// its folder trust): Autopilot may decline it, never approve it.
///
/// CDXC:Autopilot 2026-11-13 DECISION:
/// The coordinator, for the user, when "Answer approval cards" became on by default: "Autopilot must never approve leaving read-only/Plan mode for a read-only thread" (`start-thread --read-only`, CDXC:Coordinators 2026-11-13 in coordinators/read_only.rs). Every approval such a thread asks for is a step out of its mode (Claude's "Ready to code?" plan exit, an edit or command in Plan mode, a Codex sandbox escalation), and its plan text would sort as an ordinary prompt that Everything approves, so the whole card waits for the user and Autopilot's log says why.
fn verdict(config: &AutopilotConfig, class: Class, read_only: bool) -> Verdict {
    if !config.answer_approvals || config.tier == Tier::Heal {
        return Verdict::Wait;
    }
    match class {
        Class::Never(what) => Verdict::Decline(
            format!("It asked for {what}, which only you can do."),
            format!("it asks for {what}, which only the user can do"),
        ),
        // CDXC:Autopilot 2026-11-14 DECISION: user (Q3033, "only full"): the hard-limit switches show and count only on "All but limits"; on "Safe steps" every hard limit is declined.
        Class::Limit(limit)
            if config.tier != Tier::Everything || !limit.allowed(&config.limits) =>
        {
            Verdict::Decline(
            format!(
                "It asked for {}, a hard limit you haven't allowed.",
                limit.words()
            ),
            format!("{} is a hard limit the user hasn't allowed", limit.words()),
            )
        }
        _ if read_only => Verdict::Leave(READ_ONLY_NOTE.to_string()),
        Class::Limit(_) | Class::Safe => Verdict::Approve,
        Class::Plain if config.tier == Tier::Everything => Verdict::Approve,
        Class::Plain => Verdict::Leave(
            "It isn't one of the safe steps Autopilot may approve, so it waits for you.".to_string(),
        ),
    }
}

fn short(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn fingerprint(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// A question option's recommended mark: the agent's own index, or "(Recommended)" in its label.
fn recommended_index(
    question: &crate::session_chat_interactive::SessionChatQuestion,
) -> Option<usize> {
    question
        .recommended
        .filter(|index| *index < question.options.len())
        .or_else(|| {
            question
                .options
                .iter()
                .position(|option| option.label.to_ascii_lowercase().contains("recommended"))
        })
}

const READ_ONLY_NOTE: &str =
    "It's a read-only thread, so its approvals wait for you: approving one would take it out of read-only.";

const DECLINE_MESSAGE: &str = "Ghostex: [Autopilot] I declined that request: {why}. It's noted for them in Autopilot's log. Carry on with the rest of your task without it, and say at the end what's left for them.";

/// What to do about the session's waiting prompt, if any: `None` when nothing waits (the other
/// rules then look at it), `Some(decision)` when something does.
pub(super) fn decide_prompt(
    state: &super::AppState,
    session: &Value,
    key: &(String, String),
    memory: &mut PromptMemory,
    config: &AutopilotConfig,
    project_path: Option<&str>,
    now: i64,
) -> Option<PromptDecision> {
    let stored = crate::agents::session_chat_prompt_setting(session);
    let read_only = crate::coordinators::is_read_only_thread(
        session.get("runtimeSettings").and_then(Value::as_object),
    );
    let notice =
        crate::session_chat_options::cached_session_chat_terminal_notice(state, &key.0, &key.1)
            .filter(|notice| {
                matches!(notice.kind.as_str(), "permissionPrompt" | "trustPrompt")
                    && !notice.auto_trust
            });
    let open_forms = session
        .pointer("/runtimeSettings/sessionChatOpenQuestions/count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fingerprint = match (&stored, &notice) {
        (Some(stored), _) => fingerprint(stored),
        (None, Some(notice)) => fingerprint(&format!("{}{}", notice.kind, notice.detected_at)),
        (None, None) if open_forms > 0 => {
            fingerprint(&format!("forms:{open_forms}:{}", session["lastActiveAt"]))
        }
        (None, None) => {
            memory.seen = None;
            return None;
        }
    };
    if memory.done.contains(&fingerprint) {
        return Some(PromptDecision {
            rule: "",
            saw: String::new(),
            act: PromptAct::Wait,
        });
    }
    let first_seen = match &memory.seen {
        Some((seen, at)) if *seen == fingerprint => *at,
        _ => {
            memory.seen = Some((fingerprint.clone(), now));
            now
        }
    };
    let waited = now - first_seen;
    let question_wait =
        crate::coordinators::test_scaled(config.answer_after_minutes as i64 * 60_000);
    let decision =
        |rule: &'static str, saw: String, act: PromptAct| Some(PromptDecision { rule, saw, act });
    let wait = || {
        Some(PromptDecision {
            rule: "",
            saw: String::new(),
            act: PromptAct::Wait,
        })
    };
    let hold = |after_ms: i64| {
        Some(PromptDecision {
            rule: "",
            saw: String::new(),
            act: PromptAct::Hold { after_ms },
        })
    };

    // A stored prompt: an agent's question, a Connectors card, or a hook permission card.
    if let Some(prompt) = stored.as_deref().and_then(parse_stored_session_chat_prompt) {
        match prompt {
            SessionChatInteractivePrompt::Question {
                questions,
                tool_use_id,
            } => {
                let connector = questions
                    .iter()
                    .any(|question| question.tool_name.as_deref() == Some(CONNECTOR_CARD_TOOL));
                if connector {
                    let text = questions
                        .first()
                        .map(|question| question.question.clone())
                        .unwrap_or_default();
                    let saw = format!(
                        "A Connectors call waits for approval: {}",
                        short(&text, 160)
                    );
                    let body = |index: usize| {
                        json!({
                            "kind": "question",
                            "toolUseId": tool_use_id,
                            "selections": [{ "indices": [index], "other": null }],
                        })
                    };
                    return match after_users_minute(
                        verdict(
                            config,
                            classify("connector", &text, project_path),
                            read_only,
                        ),
                        waited,
                    ) {
                        Verdict::Wait => wait(),
                        Verdict::Hold(after_ms) => hold(after_ms),
                        Verdict::Approve => decision(
                            "approval",
                            saw,
                            PromptAct::Answer {
                                params: body(0),
                                did: "Approved it (Approve).".to_string(),
                            },
                        ),
                        Verdict::Decline(note, why) => decision(
                            "hard-limit",
                            saw,
                            PromptAct::Decline {
                                params: body(2),
                                message: DECLINE_MESSAGE.replace("{why}", &why),
                                note,
                            },
                        ),
                        Verdict::Leave(note) => {
                            decision("approval", saw, PromptAct::Leave { note })
                        }
                    };
                }
                if !config.answer_questions || config.tier == Tier::Heal {
                    return wait();
                }
                if waited < question_wait {
                    return wait();
                }
                let picks: Vec<Option<usize>> = questions.iter().map(recommended_index).collect();
                let asked = questions
                    .iter()
                    .map(|question| {
                        format!(
                            "- {} (choices: {})",
                            question.question,
                            question
                                .options
                                .iter()
                                .map(|option| option.label.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let saw = format!(
                    "It asked {} and you didn't answer for {} minutes.",
                    if questions.len() == 1 {
                        "a question".to_string()
                    } else {
                        format!("{} questions", questions.len())
                    },
                    config.answer_after_minutes
                );
                if picks.iter().all(Option::is_some) && !questions.is_empty() {
                    let labels: Vec<String> = questions
                        .iter()
                        .zip(&picks)
                        .map(|(question, pick)| question.options[pick.unwrap_or(0)].label.clone())
                        .collect();
                    return decision(
                        "question",
                        saw,
                        PromptAct::Answer {
                            params: json!({
                                "kind": "question",
                                "toolUseId": tool_use_id,
                                "selections": picks
                                    .iter()
                                    .map(|pick| json!({ "indices": [pick.unwrap_or(0)], "other": null }))
                                    .collect::<Vec<_>>(),
                            }),
                            did: format!(
                                "Answered with its recommended choice: {}.",
                                labels.join("; ")
                            ),
                        },
                    );
                }
                return decision(
                    "question",
                    saw,
                    PromptAct::Judge {
                        why: format!(
                            "It asked a question with no recommended choice, and the user didn't answer for {} minutes.",
                            config.answer_after_minutes
                        ),
                        detail: format!("Its question:\n{asked}"),
                    },
                );
            }
            SessionChatInteractivePrompt::Approval {
                tool,
                summary,
                tool_use_id,
            } => {
                let text = summary.clone().unwrap_or_default();
                let saw = format!("It asked to run {tool}: {}", short(&text, 160));
                return match after_users_minute(
                    verdict(config, classify(&tool, &text, project_path), read_only),
                    waited,
                ) {
                    Verdict::Wait => wait(),
                    Verdict::Hold(after_ms) => hold(after_ms),
                    Verdict::Approve => decision(
                        "approval",
                        saw,
                        PromptAct::Answer {
                            params: json!({
                                "kind": "approval",
                                "approvalSend": "1",
                                "promptTool": tool,
                                "promptSummary": summary,
                                "toolUseId": tool_use_id,
                            }),
                            did: "Approved it.".to_string(),
                        },
                    ),
                    Verdict::Decline(note, why) => decision(
                        "hard-limit",
                        saw,
                        PromptAct::Decline {
                            params: json!({ "kind": "approval", "approvalSend": "" }),
                            message: DECLINE_MESSAGE.replace("{why}", &why),
                            note,
                        },
                    ),
                    Verdict::Leave(note) => decision("approval", saw, PromptAct::Leave { note }),
                };
            }
        }
    }

    // A prompt only on screen: a permission dialog or folder trust.
    if let Some(notice) = notice {
        let trust = notice.kind == "trustPrompt";
        let text = notice
            .approval_command
            .clone()
            .or_else(|| {
                notice
                    .permission_request
                    .as_ref()
                    .and_then(|request| request.content.clone())
            })
            .or_else(|| {
                notice
                    .permission_request
                    .as_ref()
                    .and_then(|request| request.subject.clone())
            })
            .unwrap_or_else(|| notice.title.clone());
        let tool = notice
            .permission_request
            .as_ref()
            .map(|request| {
                request
                    .source
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .unwrap_or_default();
        let saw = if trust {
            "It asked to trust its project folder.".to_string()
        } else {
            format!("It asked on screen to run: {}", short(&text, 160))
        };
        let class = if trust {
            Class::Safe
        } else {
            classify(&tool, &text, project_path)
        };
        let labels: Vec<(usize, String)> = match &notice.dialog {
            Some(dialog) => dialog
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| (index, row.label.to_ascii_lowercase()))
                .collect(),
            None => notice
                .choices
                .iter()
                .map(|choice| (choice.index, choice.label.to_ascii_lowercase()))
                .collect(),
        };
        let yes = labels
            .iter()
            .find(|(_, label)| {
                (label.starts_with("yes")
                    || label.starts_with("trust")
                    || label.starts_with("allow")
                    || label.starts_with("approve"))
                    && !contains_any(label, &["don't ask", "dont ask", "always", "remember"])
            })
            .or_else(|| labels.iter().find(|(_, label)| label.starts_with("yes")))
            .map(|(index, _)| *index);
        let no = labels
            .iter()
            .find(|(_, label)| {
                label.starts_with("no") || label.starts_with("deny") || label.starts_with("reject")
            })
            .map(|(index, _)| *index);
        let body = |index: usize| match &notice.dialog {
            Some(dialog) => {
                json!({ "kind": "terminalDialog", "choiceIndex": index, "dialogId": dialog.id })
            }
            None => {
                json!({ "kind": "terminalChoice", "choiceIndex": index, "noticeDetectedAt": notice.detected_at })
            }
        };
        // Trusting the project folder lets the agent start; it is not a step out of read-only.
        return match after_users_minute(verdict(config, class, read_only && !trust), waited) {
            Verdict::Wait => wait(),
            Verdict::Hold(after_ms) => hold(after_ms),
            Verdict::Approve => match yes {
                Some(index) => decision(
                    if trust { "trust" } else { "approval" },
                    saw,
                    PromptAct::Answer {
                        params: body(index),
                        did: "Approved it.".to_string(),
                    },
                ),
                None => decision(
                    "approval",
                    saw,
                    PromptAct::Leave {
                        note: "Autopilot couldn't tell which choice approves it.".to_string(),
                    },
                ),
            },
            Verdict::Decline(note, why) => match no {
                Some(index) => decision(
                    "hard-limit",
                    saw,
                    PromptAct::Decline {
                        params: body(index),
                        message: DECLINE_MESSAGE.replace("{why}", &why),
                        note,
                    },
                ),
                None => decision("hard-limit", saw, PromptAct::Leave { note }),
            },
            Verdict::Leave(note) => decision("approval", saw, PromptAct::Leave { note }),
        };
    }

    // Ghostex question forms the agent printed in its chat.
    if open_forms > 0 {
        if !config.answer_questions || config.tier == Tier::Heal || waited < question_wait {
            return wait();
        }
        return decision(
            "question",
            format!(
                "Its chat asks {} and you didn't answer for {} minutes.",
                if open_forms == 1 {
                    "a question".to_string()
                } else {
                    format!("{open_forms} questions")
                },
                config.answer_after_minutes
            ),
            PromptAct::Forms,
        );
    }
    None
}

/// The fingerprint of the prompt Autopilot just decided, to mark it done.
pub(super) fn current_fingerprint(memory: &PromptMemory) -> Option<String> {
    memory
        .seen
        .as_ref()
        .map(|(fingerprint, _)| fingerprint.clone())
}
