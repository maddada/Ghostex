//! Settings > Autopilot: the switch, which sessions it covers, what it may do, the supervisor, and
//! what it did (autopilot_log.rs). Everything here is gxserver's (`/api/autopilot`,
//! server/src/server/autopilot_http.rs), read when the page opens, every 15 seconds while it is
//! the open page, and after each change; a change shows at once and is saved in the background.
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: "we should have settings area for autopilot specifically", with every hard limit as its own toggle ("add all those as toggles in the page in settings for autopilot please"), the auto-answer toggle with its 5/10/15-minute wait, the approval-cards toggle, the supervisor model ("Opus 5.5 medium" or "sol 6.1 high"), a sweep slider, and "Summary and log: in the autopilot page in settings pls. make that page intuitive please i trust u. it should have log there."
//! SEE-ALSO: server/src/autopilot/config.rs (the choices and their defaults), packages/settings-catalog/src/pages/autopilot.rs (its search rows).
use super::super::super::native_modal_kit::{MODAL_MONO_FONT, looks_like_path, path_link};
use super::super::catalog::SettingOption;
use super::super::fields::notice::{is_service_unreachable, service_unreachable_state};
use super::super::fields::{
    ButtonVariant, FieldStates, ROW_PADDING_X, ROW_PADDING_Y, RowSpec, SettingsPage, SliderBinding,
    SliderSaver, segmented_field, select_field, settings_button, settings_section,
    slider_number_field_with, static_note, toggle_field_with,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, store_gxserver_rpc};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, Styled as _, Task, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the open page reads the log and the summary again.
const REFRESH_EVERY: Duration = Duration::from_secs(15);
/// Log lines read at first; Show more adds this many.
pub(super) const LOG_PAGE: u64 = 50;

pub(crate) fn autopilot_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| AutopilotTab::new(store.clone(), cx)).into()
}

pub(crate) struct AutopilotTab {
    pub(super) store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The last `/api/autopilot` answer, with this page's unsaved changes applied.
    pub(super) view: Option<Value>,
    pub(super) error: Option<String>,
    /// Log lines asked for.
    pub(super) log_limit: u64,
    /// Changes not yet saved; a read that lands meanwhile keeps them.
    saving: usize,
    was_active: bool,
    _refresh: Option<Task<()>>,
}

impl SettingsPage for AutopilotTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl AutopilotTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |page: &mut Self, store, cx| {
            let active = store.read(cx).active_tab() == SettingsTabId::Autopilot;
            if active && !page.was_active {
                page.reload(cx);
            }
            page.was_active = active;
            cx.notify();
        })
        .detach();
        let refresh = cx.spawn(async move |page, cx| {
            loop {
                let _ = page.update(cx, |page, cx| {
                    if page.store.read(cx).active_tab() == SettingsTabId::Autopilot {
                        page.reload(cx);
                    }
                });
                cx.background_executor().timer(REFRESH_EVERY).await;
            }
        });
        Self {
            store,
            fields: FieldStates::default(),
            view: None,
            error: None,
            log_limit: LOG_PAGE,
            saving: 0,
            was_active: true,
            _refresh: Some(refresh),
        }
    }

    fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.call(json!({ "op": "read" }), cx);
    }

    /// Sends one `/api/autopilot` call; its answer is the page's new state.
    pub(super) fn call(&mut self, mut params: Value, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        let read = params["op"] == "read";
        params["limit"] = json!(self.log_limit);
        if !read {
            self.saving += 1;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/autopilot",
            params,
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if !read {
                        page.saving = page.saving.saturating_sub(1);
                    }
                    match result {
                        // A read that lands while a change is saving would undo it on screen.
                        Ok(_) if read && page.saving > 0 => {}
                        Ok(answer) => {
                            page.view = Some(answer);
                            page.error = None;
                        }
                        Err(error) => {
                            page.error = Some(error);
                            if !read {
                                page.reload(cx);
                            }
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Changes some of Autopilot's choices: on screen at once, then saved.
    fn set(&mut self, patch: Value, cx: &mut Context<Self>) {
        if let (Some(config), Some(patch_fields)) = (
            self.view
                .as_mut()
                .and_then(|view| view.get_mut("config"))
                .and_then(Value::as_object_mut),
            patch.as_object(),
        ) {
            for (key, value) in patch_fields {
                if key == "limits" {
                    if let (Some(limits), Some(changes)) = (
                        config.get_mut("limits").and_then(Value::as_object_mut),
                        value.as_object(),
                    ) {
                        for (limit, on) in changes {
                            limits.insert(limit.clone(), on.clone());
                        }
                    }
                } else {
                    config.insert(key.clone(), value.clone());
                }
            }
        }
        cx.notify();
        self.call(json!({ "op": "set", "patch": patch }), cx);
    }

    fn config(&self) -> Value {
        self.view
            .as_ref()
            .and_then(|view| view.get("config"))
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn switch_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let on = config["enabled"].as_bool() == Some(true);
        let view = self.view.clone().unwrap_or(Value::Null);
        let covered = view["watching"]["covered"].as_u64().unwrap_or(0);
        let status = if on {
            format!(
                "On since {}, keeping {} going. Turn it off here, from a coordinator's chat, or with ghostex autopilot off.",
                when(config["enabledAt"].as_str()),
                match covered {
                    1 => "1 session".to_string(),
                    count => format!("{count} sessions"),
                }
            )
        } else {
            "Keeps your sessions going while you are away: it carries on a session that stopped while its background job finished or mid-task, interrupts one stuck on a hung command, reloads one that stays frozen, moves one at a usage limit to an account with room, and restarts one whose terminal ended. It stays on until you turn it off.".to_string()
        };
        let mut rows = vec![toggle_field_with(
            p,
            "autopilotEnabled",
            RowSpec::new("Autopilot").description(status),
            on,
            None,
            |page: &mut Self, checked, _, cx| page.set(json!({ "enabled": checked }), cx),
            cx,
        )];
        // CDXC:Settings 2026-11-14 DECISION (AGENTS.md, Q3107): while Autopilot is off its page is
        // this one switch (and what it did); every choice under it shows once it is on.
        if !on {
            return settings_section(p, "", None, None, rows);
        }
        let scope = config["scope"].as_str().unwrap_or("all").to_string();
        rows.push(segmented_field(
            p,
            "autopilotScope",
            RowSpec::new("Which sessions").description(match scope.as_str() {
                "projects" => "Only the projects switched on below, with their worktrees. A session's Autopilot pill takes it out or puts it back.",
                "sessions" => "Only sessions you put in one by one, with the Autopilot pill in their chat.",
                _ => "Every session, coordinators and threads included, except the projects switched off below (a project's right-click menu, Advanced > Autopilot, does the same). A session's Autopilot pill in its chat takes it out or puts it back.",
            }),
            None,
            &[
                option("Every session", "all"),
                option("Projects I pick", "projects"),
                option("Sessions I pick", "sessions"),
            ],
            Some(scope.as_str()),
            None,
            |page: &mut Self, value, _, cx| page.set(json!({ "scope": value }), cx),
            cx,
        ));
        // The project switches only for "Projects I pick", the session ones only for "Sessions I
        // pick" (in Every session, a project's menu and a session's pill take them out).
        if scope == "projects" {
            let projects = list(&view, "projects");
            let named = |name: &str| {
                projects
                    .iter()
                    .filter(|project| text(project, "name").eq_ignore_ascii_case(name))
                    .count()
            };
            for project in &projects {
                let project_id = text(project, "projectId");
                let name = text(project, "name");
                let inside = project["in"].as_bool() == Some(true);
                let id = row_id(format!("autopilot-project-{project_id}"));
                // Two projects with one name ("Ghostex", "ghostex") show their folders.
                let mut spec = RowSpec::new(name.clone()).dependent();
                if named(&name) > 1 {
                    spec = spec.description(home_relative(&text(project, "path")));
                }
                rows.push(toggle_field_with(
                    p,
                    id,
                    spec,
                    inside,
                    None,
                    move |page: &mut Self, checked, _, cx| {
                        page.set_project(&project_id, checked, cx)
                    },
                    cx,
                ));
            }
        }
        let sessions = if scope == "sessions" {
            list(&view, "sessions")
        } else {
            Vec::new()
        };
        for session in sessions {
            let project_id = text(&session, "projectId");
            let session_id = text(&session, "sessionId");
            let session_on = session["on"].as_bool() == Some(true);
            let id = row_id(format!("autopilot-session-{project_id}-{session_id}"));
            rows.push(toggle_field_with(
                p,
                id,
                RowSpec::new(format!(
                    "{} ({})",
                    text(&session, "title"),
                    text(&session, "project")
                ))
                .description("Set from its chat's Autopilot pill.")
                .dependent(),
                session_on,
                None,
                move |page: &mut Self, checked, _, cx| {
                    page.call(
                        json!({
                            "op": "setSession",
                            "projectId": project_id,
                            "sessionId": session_id,
                            "on": checked,
                        }),
                        cx,
                    )
                },
                cx,
            ));
        }
        settings_section(p, "", None, None, rows)
    }

    fn set_project(&mut self, project_id: &str, on: bool, cx: &mut Context<Self>) {
        if let Some(projects) = self
            .view
            .as_mut()
            .and_then(|view| view.get_mut("projects"))
            .and_then(Value::as_array_mut)
        {
            for project in projects.iter_mut() {
                if project["projectId"] == project_id {
                    project["in"] = json!(on);
                }
            }
        }
        cx.notify();
        self.call(
            json!({ "op": "setProject", "projectId": project_id, "on": on }),
            cx,
        );
    }

    fn allowed_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let tier = config["tier"].as_str().unwrap_or("everything").to_string();
        let mut rows = vec![segmented_field(
            p,
            "autopilotTier",
            RowSpec::new("What it may do on its own").description(match tier.as_str() {
                "heal" => "Only what anyone would do without thinking: carry on, interrupt a hung command, reload, switch account. Questions and approvals wait for you.",
                "decide" => "Heal, plus answer what an agent asks with its recommended choice when you allow it below. Approvals wait for you.",
                _ => "Heal and decide, plus approve any prompt that is not a hard limit when you allow approval cards below.",
            }),
            None,
            &[
                option("Heal", "heal"),
                option("Heal and decide", "decide"),
                option("Everything", "everything"),
            ],
            Some(tier.as_str()),
            None,
            |page: &mut Self, value, _, cx| page.set(json!({ "tier": value }), cx),
            cx,
        )];
        // Heal leaves questions and approvals for you (server/src/server/autopilot_answers.rs), so
        // their switches show only on Heal and decide or Everything (CDXC:Settings 2026-11-14).
        if tier == "heal" {
            return settings_section(p, "What it may do", None, None, rows);
        }
        let answer_questions = config["answerQuestions"].as_bool() == Some(true);
        rows.push(toggle_field_with(
            p,
            "autopilotAnswerQuestions",
            RowSpec::new("Answer questions for me").description(
                "When an agent asks something and you don't answer in time, Autopilot picks its recommended choice for each question, and says so in the chat.",
            ),
            answer_questions,
            None,
            |page: &mut Self, checked, _, cx| page.set(json!({ "answerQuestions": checked }), cx),
            cx,
        ));
        if answer_questions {
            let wait = config["answerAfterMinutes"]
                .as_u64()
                .unwrap_or(10)
                .to_string();
            rows.push(segmented_field(
                p,
                "autopilotAnswerAfter",
                RowSpec::new("Wait for me first").dependent(),
                None,
                &[
                    option("5 min", "5"),
                    option("10 min", "10"),
                    option("15 min", "15"),
                ],
                Some(wait.as_str()),
                None,
                |page: &mut Self, value, _, cx| {
                    let minutes = value.parse::<u64>().unwrap_or(10);
                    page.set(json!({ "answerAfterMinutes": minutes }), cx)
                },
                cx,
            ));
        }
        rows.push(toggle_field_with(
            p,
            "autopilotApprovals",
            RowSpec::new("Answer approval cards").description(
                "Permission prompts, Connectors cards and folder trust, within what it may do. A read-only thread's cards always wait for you; while this is off, only your click answers them.",
            ),
            config["answerApprovals"].as_bool() == Some(true),
            None,
            |page: &mut Self, checked, _, cx| page.set(json!({ "answerApprovals": checked }), cx),
            cx,
        ));
        settings_section(p, "What it may do", None, None, rows)
    }

    fn limits_section(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> Option<Div> {
        let config = self.config();
        let limits = &config["limits"];
        let mut rows: Vec<AnyElement> = Vec::new();
        for (key, label, description) in HARD_LIMITS {
            rows.push(toggle_field_with(
                p,
                key,
                RowSpec::new(*label).description(*description),
                limits[*key].as_bool() == Some(true),
                None,
                move |page: &mut Self, checked, _, cx| {
                    page.set(json!({ "limits": { *key: checked } }), cx)
                },
                cx,
            ));
        }
        rows.push(padded_text(
            p,
            &[(
                "Signing in, 2FA, identity checks and payments are never done: Autopilot leaves them for you with a note, and the agent carries on with the rest.",
                false,
            )],
        ));
        settings_section(p, "Hard limits it may cross", None, None, rows)
    }

    fn supervisor_section(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let config = self.config();
        let supervisor = config["supervisor"]
            .as_str()
            .unwrap_or("opusMedium")
            .to_string();
        let mut rows = vec![select_field(
            self,
            p,
            "autopilotSupervisor",
            RowSpec::new("Supervisor model").description(
                "The agent that wakes only for judgment calls. It starts on whichever of your accounts has room.",
            ),
            None,
            &[
                option("Claude Opus 5.5 · Medium", "opusMedium"),
                option("Codex GPT 6.1 Sol · High", "solHigh"),
            ],
            supervisor.as_str(),
            Some(240.0),
            |page: &mut Self, value, _, cx| page.set(json!({ "supervisor": value }), cx),
            window,
            cx,
        )];
        let minutes = (config["sweepSeconds"].as_f64().unwrap_or(120.0) / 60.0).round();
        let page = cx.weak_entity();
        let saver: SliderSaver = std::rc::Rc::new(move |_store, value, commit, cx| {
            if !commit {
                return;
            }
            let seconds = (value.round().clamp(1.0, 10.0) * 60.0) as u64;
            let _ = page.update(cx, |page, cx| {
                page.set(json!({ "sweepSeconds": seconds }), cx)
            });
        });
        rows.push(slider_number_field_with(
            self,
            p,
            RowSpec::new("Look at every session every (minutes)").description(
                "A session whose turn ends is looked at right away; the sweep catches the rest, like a command that hangs.",
            ),
            None,
            SliderBinding {
                key: "autopilotSweepMinutes",
                min: 1.0,
                max: 10.0,
                step: 1.0,
            },
            minutes,
            saver,
            window,
            cx,
        ));
        let supervisor = &self
            .view
            .as_ref()
            .map(|view| view["supervisor"].clone())
            .unwrap_or(Value::Null);
        let status = match supervisor["agent"].as_str() {
            // User, on the old "It hasn't been needed yet" line: "why has this never been
            // needed??" It wakes only for what the rules can't settle, so the line says which.
            None => format!(
                "Not started yet. It wakes only for what Autopilot's rules can't settle: a session still frozen after an interrupt and a reload, or (with Answer questions on) a question with no recommended choice. Until then the rules handle everything. It starts in an Autopilot project in your sidebar.{}",
                if config["answerQuestions"].as_bool() == Some(true) {
                    ""
                } else {
                    " Answer questions is off, so questions stay with you."
                }
            ),
            Some(agent) => {
                let wakes = supervisor["wakes"].as_u64().unwrap_or(0);
                let waiting = supervisor["waiting"].as_u64().unwrap_or(0);
                format!(
                    "Runs on {} in the Autopilot project. Woken {} time{}{}{}.",
                    if agent == "codex" { "Codex" } else { "Claude" },
                    wakes,
                    if wakes == 1 { "" } else { "s" },
                    supervisor["lastWakeAt"]
                        .as_str()
                        .map(|at| format!(", last at {}", when(Some(at))))
                        .unwrap_or_default(),
                    if waiting > 0 {
                        format!("; {waiting} waiting for it")
                    } else {
                        String::new()
                    },
                )
            }
        };
        rows.push(padded_text(p, &[(status.as_str(), false)]));
        let notes = supervisor["notesDir"]
            .as_str()
            .map(home_relative)
            .unwrap_or_default();
        if !notes.is_empty() {
            rows.push(padded_text(
                p,
                &[
                    ("Each wake reads its notes first: ", false),
                    (notes.as_str(), true),
                    (".", false),
                ],
            ));
        }
        settings_section(p, "Supervisor", None, None, rows)
    }
}

/// The hard limits: their key in `limits`, label and line.
const HARD_LIMITS: &[(&str, &str, &str)] = &[
    (
        "pushBranch",
        "Push a branch",
        "Push work to a branch other than main. Nothing reaches your users.",
    ),
    (
        "finishRelease",
        "Finish a release you asked for",
        "Retry its failed jobs, push and publish it. It never starts one.",
    ),
    (
        "pushMain",
        "Push main",
        "Other sessions' local commits could go with it.",
    ),
    (
        "merge",
        "Merge pull requests",
        "Merge a pull request on GitHub.",
    ),
    (
        "deleteOutsideProject",
        "Delete outside the project",
        "Delete files outside the project and its work folders.",
    ),
    (
        "postPublicly",
        "Post publicly",
        "Post on Discord or X, comment on GitHub, or send email.",
    ),
];

/// A row id for a project's or a session's switch. Rows need `'static` ids, so each one is made
/// once per page process and kept, not once per frame.
fn row_id(id: String) -> &'static str {
    use std::collections::HashSet;
    use std::sync::Mutex;
    static IDS: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut ids = IDS.lock().unwrap_or_else(|error| error.into_inner());
    let ids = ids.get_or_insert_with(HashSet::new);
    if let Some(known) = ids.get(id.as_str()) {
        return known;
    }
    let leaked: &'static str = Box::leak(id.into_boxed_str());
    ids.insert(leaked);
    leaked
}

/// A line of text inside a section's card, with a row's padding; `(text, true)` parts are code.
pub(super) fn padded_text(p: &SettingsPalette, parts: &[(&str, bool)]) -> AnyElement {
    // Plain text alone wraps inside the row (a long one in the flex row below stays on one line,
    // cut off at the right edge).
    if parts.iter().all(|(_, code)| !code) {
        let text: String = parts.iter().map(|(part, _)| *part).collect();
        return div()
            .w_full()
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y + 2.0))
            .text_size(px(13.0))
            .text_color(p.muted)
            .child(text)
            .into_any_element();
    }
    let mut line = h_flex().flex_wrap().items_baseline();
    for (part, code) in parts {
        line = line.child(if *code {
            let chip = div()
                .px(px(4.0))
                .rounded(px(4.0))
                .bg(p.raised_hover)
                .text_color(p.foreground)
                .font_family(MODAL_MONO_FONT)
                .text_size(px(12.5))
                .child(part.to_string());
            // A path is a link: it opens in Files (CDXC:PathLinks 2026-11-08).
            if looks_like_path(part) {
                path_link(format!("padded-text-{part}"), part.to_string(), chip).into_any_element()
            } else {
                chip.into_any_element()
            }
        } else {
            div().child(part.to_string()).into_any_element()
        });
    }
    div()
        .w_full()
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y + 2.0))
        .text_size(px(13.0))
        .text_color(p.muted)
        .child(line)
        .into_any_element()
}

/// A folder as the page shows it, the home folder as `~`.
fn home_relative(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

fn option(label: &str, value: &str) -> SettingOption {
    SettingOption {
        label: label.to_string(),
        value: value.to_string(),
    }
}

pub(super) fn list(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A time as the page says it: "14:02" today, "Mon 14:02" this week, else the date.
pub(super) fn when(at: Option<&str>) -> String {
    let Some(at) = at.and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok()) else {
        return "now".to_string();
    };
    let local = at.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else if (now - local).num_days() < 6 {
        local.format("%a %H:%M").to_string()
    } else {
        local.format("%b %-d, %H:%M").to_string()
    }
}

impl Render for AutopilotTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (p, search, matching) = {
            let store = self.store.read(cx);
            let matching: Vec<SettingsTabId> = if store.is_searching() {
                rail_pages(store).into_iter().map(|page| page.tab).collect()
            } else {
                Vec::new()
            };
            (
                store.palette(),
                store.tab_search(SettingsTabId::Autopilot),
                matching,
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::Autopilot,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
            return settings_page(&self.store, SettingsTabId::Autopilot, &p, blocks, cx);
        }
        if self.error.as_deref().is_some_and(is_service_unreachable) {
            blocks.push(PageBlock::plain(service_unreachable_state(
                &p,
                "autopilot-unreachable",
                |page: &mut Self, _window, cx| page.reload(cx),
                cx,
            )));
        }
        if let Some(error) = self
            .error
            .clone()
            .filter(|error| !is_service_unreachable(error))
        {
            let retry = settings_button(
                &p,
                "autopilot-retry",
                "Try again",
                Some("modals/settings/refresh.svg"),
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.reload(cx),
                cx,
            );
            blocks.push(PageBlock::plain(
                h_flex()
                    .w_full()
                    .gap(px(12.0))
                    .items_center()
                    .child(div().flex_1().min_w_0().child(static_note(&p, error, true)))
                    .child(retry),
            ));
        }
        if self.view.is_none() {
            if self.error.is_none() {
                blocks.push(PageBlock::plain(static_note(
                    &p,
                    "Reading Autopilot…",
                    false,
                )));
            }
            return settings_page(&self.store, SettingsTabId::Autopilot, &p, blocks, cx);
        }
        let enabled = self.config()["enabled"].as_bool() == Some(true);
        let shown = |section: &str| {
            let result = search.section(section);
            !result.is_searching || result.has_visible()
        };
        if shown("switch")
            && let Some(section) = self.switch_section(&p, cx)
        {
            let mut block = PageBlock::section("switch", section);
            block.margin_top += 20.0;
            blocks.push(block);
        }
        if shown("activity")
            && let Some(section) = self.summary_section(&p, cx)
        {
            blocks.push(PageBlock::section("summary", section));
        }
        if enabled
            && shown("allowed")
            && let Some(section) = self.allowed_section(&p, cx)
        {
            blocks.push(PageBlock::section("allowed", section));
        }
        if enabled
            && shown("allowed")
            && let Some(section) = self.limits_section(&p, cx)
        {
            blocks.push(PageBlock::section("limits", section));
        }
        if enabled
            && shown("supervisor")
            && let Some(section) = self.supervisor_section(&p, window, cx)
        {
            blocks.push(PageBlock::section("supervisor", section));
        }
        if shown("activity")
            && let Some(section) = self.log_section(&p, cx)
        {
            blocks.push(PageBlock::section("activity", section));
        }
        settings_page(&self.store, SettingsTabId::Autopilot, &p, blocks, cx)
    }
}
