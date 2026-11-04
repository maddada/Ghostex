//! The Cloud Boxes page. Its top part is Cloud Boxes proper (Instabox, Boat, and the debug-only
//! Local provider) over `/api/cloudBoxes`: providers, spending and the cap, stopping and deleting,
//! every box, and agent logins for boxes (the `cloud_*` files). Under Advanced sits "More providers
//! (agentbox)": running agent sessions in an agentbox sandbox on this computer (Docker) or in the
//! cloud. That part reads `/api/agentbox` (`status`, `list`, `openTarget`, `stop`, `destroy`) from
//! gxserver, and every setup step runs in a command-pane terminal: the page posts
//! `runAgentboxTerminalCommand` with the step's name and gxserver answers with the command text,
//! so a page can never make the app run text of its choosing. "Set it up for me" posts
//! `setUpAgentboxWithAgent`, which starts an agent session that does the whole setup.
//!
//! CDXC:AgentBox 2026-10-01 DECISION:
//! User: Cloud Boxes "has a new page in settings", and "we need explanation in the app on how to configure it (or a prompt to an agent that configures it with computer use would be even better)". The page explains boxes in plain words, shows what is set up, runs each setup step in a terminal, and offers "Set it up for me", which hands the whole setup to an agent.
//! SEE-ALSO: docs/2026-10-01/agentbox/PLAN.md (wire contract 3), server/src (the `/api/agentbox` endpoint), apps/desktop/src/app/os_integration/agentbox_settings.rs (the terminal commands and the setup chat), packages/core-ui/settings-modal/tabs/cloud-boxes.tsx (deleted 2026-10-01) (the React twin that feeds search, Help and Storybook).
use super::super::fields::{
    ButtonVariant, FieldStates, ListItemStatus, SettingsPage, advanced_fold_header, card_inset,
    description_info_button, settings_button, settings_icon, settings_list_item, settings_section,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::palette::SettingsPalette;
use super::super::rail::{rail_pages, render_no_matches};
use super::super::store::{SettingsStore, post_store_message, store_gxserver_rpc};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::time::{Duration, Instant};

mod boxes;
mod cloud_box_list;
mod cloud_logins;
mod cloud_providers;
mod cloud_spend;
mod cloud_state;
mod model;
mod preferences;
mod providers;

use model::{BoxRow, StatusSummary, status_summary};

const ICON_BOX: &str = "modals/settings/box.svg";
const ICON_REFRESH: &str = "modals/settings/refresh.svg";
const ICON_SPARKLES: &str = "modals/settings/sparkles.svg";
const ICON_DOWNLOAD: &str = "modals/settings/download.svg";
const ICON_STETHOSCOPE: &str = "modals/settings/microscope.svg";
const ICON_EXTERNAL: &str = "modals/settings/external-link.svg";
const ICON_INFO: &str = "modals/settings/info-circle.svg";

/// `agentbox doctor` behind `status` can take a while on a cold Docker or a slow cloud API.
const STATUS_TIMEOUT: Duration = Duration::from_secs(45);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// While a setup step started here may still be running in its terminal, the page re-reads the
/// status this often, for at most `WATCH_WINDOW`.
const WATCH_INTERVAL: Duration = Duration::from_secs(10);
const WATCH_WINDOW: Duration = Duration::from_secs(5 * 60);

const AGENTBOX_REPO_URL: &str = "https://github.com/madarco/agentbox";

pub(crate) fn cloud_boxes_tab_view(store: &Entity<SettingsStore>, cx: &mut App) -> AnyView {
    cx.new(|cx| CloudBoxesTab::new(store.clone(), cx)).into()
}

pub(crate) struct CloudBoxesTab {
    store: Entity<SettingsStore>,
    fields: FieldStates,
    /// The last `/api/agentbox status` answer.
    status: Option<Value>,
    /// Why the last status read failed (the endpoint is missing, gxserver is down, ...).
    status_error: Option<String>,
    status_loading: bool,
    boxes: Option<Vec<BoxRow>>,
    boxes_error: Option<String>,
    boxes_loading: bool,
    /// Re-reads the status while a setup step started here may still be running.
    watch: Option<Task<()>>,
    watch_until: Option<Instant>,
    /// The Add Server form under "Your own server (SSH)".
    adding_server: bool,
    server_alias: String,
    server_ssh: String,
    /// The box whose Destroy is waiting for its confirmation.
    confirm_destroy: Option<String>,
    /// Boxes with a stop, destroy or open in flight.
    busy_boxes: HashSet<String>,
    /// The Cloud Boxes part (`/api/cloudBoxes`).
    cloud: cloud_state::CloudState,
}

impl CloudBoxesTab {
    fn new(store: Entity<SettingsStore>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        // The preview binary's state with the Add Server form open.
        let adding_server =
            store.read(cx).request().preview_state.as_deref() == Some("cloud-boxes-add-server");
        // Read after the window being built reaches its host, as Integrations does.
        cx.spawn(async move |page, cx| {
            let _ = page.update(cx, |page, cx| {
                page.load_cloud_all(cx);
                page.load_status(false, cx);
                page.load_boxes(cx);
            });
        })
        .detach();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            status: None,
            status_error: None,
            status_loading: false,
            boxes: None,
            boxes_error: None,
            boxes_loading: false,
            watch: None,
            watch_until: None,
            adding_server,
            server_alias: String::new(),
            server_ssh: String::new(),
            confirm_destroy: None,
            busy_boxes: HashSet::new(),
            cloud: cloud_state::CloudState::default(),
        };
        // The Add Server form the last visit left open (settings_modal/restore.rs).
        if let Some(form) = page.store.update(cx, |store, _| {
            store.open_forms_mut().take(SettingsTabId::CloudBoxes)
        }) {
            let text = |key: &str| {
                form.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            page.adding_server = true;
            page.server_alias = text("alias");
            page.server_ssh = text("ssh");
        }
        page
    }

    fn rpc_available(&self, cx: &App) -> bool {
        self.store.read(cx).request().gxserver_rpc_available
    }

    fn toast(&self, level: &str, title: &str, description: &str, cx: &mut App) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    /// `/api/agentbox status`; `refresh` re-runs `agentbox doctor` instead of the ~30s cache.
    fn load_status(&mut self, refresh: bool, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) || self.status_loading {
            return;
        }
        self.status_loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/agentbox",
            json!({ "action": "status", "refresh": refresh }),
            STATUS_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.status_loading = false;
                    match result {
                        Ok(status) => {
                            page.status = Some(status);
                            page.status_error = None;
                        }
                        Err(error) => {
                            page.status_error = Some(if error.trim().is_empty() {
                                "gxserver did not answer.".to_string()
                            } else {
                                error
                            });
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// `/api/agentbox list`.
    fn load_boxes(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) || self.boxes_loading {
            return;
        }
        self.boxes_loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/agentbox",
            json!({ "action": "list" }),
            LIST_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.boxes_loading = false;
                    match result {
                        Ok(list) => {
                            page.boxes = Some(model::box_rows(&list));
                            page.boxes_error = None;
                        }
                        Err(error) => {
                            page.boxes_error = Some(if error.trim().is_empty() {
                                "gxserver did not answer.".to_string()
                            } else {
                                error
                            });
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        self.load_cloud_all(cx);
        self.load_status(true, cx);
        self.load_boxes(cx);
    }

    /// Runs one setup step in a command-pane terminal. Only the step's name and its validated
    /// arguments leave the page; the app asks gxserver for the command text itself.
    pub(super) fn run_terminal_command(
        &mut self,
        command: &str,
        arguments: &[(&str, &str)],
        cx: &mut Context<Self>,
    ) {
        let mut message = Map::new();
        message.insert("type".into(), json!("runAgentboxTerminalCommand"));
        message.insert("command".into(), json!(command));
        for (key, value) in arguments {
            message.insert((*key).to_string(), json!(value));
        }
        post_store_message(&self.store, Value::Object(message), cx);
        self.watch_status(cx);
    }

    /// Re-reads the status every `WATCH_INTERVAL` for `WATCH_WINDOW` after a setup step started,
    /// so the page shows its result without a Refresh. Another step extends the window.
    fn watch_status(&mut self, cx: &mut Context<Self>) {
        self.watch_until = Some(Instant::now() + WATCH_WINDOW);
        if self.watch.is_some() {
            return;
        }
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WATCH_INTERVAL).await;
                let keep_going = this
                    .update(cx, |page, cx| {
                        let open = page.watch_until.is_some_and(|until| Instant::now() < until);
                        if open {
                            page.load_status(true, cx);
                        } else {
                            page.watch = None;
                            page.watch_until = None;
                        }
                        open
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// "Set it up for me": an agent session in the Ghostex folder project does the whole setup.
    /// Settings closes so the new session is in view.
    fn set_up_with_agent(&mut self, cx: &mut Context<Self>) {
        post_store_message(&self.store, json!({ "type": "setUpAgentboxWithAgent" }), cx);
        self.store.update(cx, |store, cx| store.close(cx));
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        post_store_message(
            &self.store,
            json!({ "type": "openExternalUrl", "url": url }),
            cx,
        );
    }
}

impl super::HoldsUnsavedInput for CloudBoxesTab {
    /// The Add Server form, a Connect form, or a Destroy or Delete confirmation is open.
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        self.adding_server
            || self.confirm_destroy.is_some()
            || self.cloud.connect.is_some()
            || self.cloud.confirm_delete.is_some()
    }
}

impl super::open_forms::RemembersOpenForm for CloudBoxesTab {
    /// The Add Server form: the server's name and its SSH target (`user@host`, `host:port` or an
    /// ~/.ssh/config name), which hold no secret.
    fn open_form(&self, _cx: &gpui::App) -> Option<Value> {
        self.adding_server.then(
            || json!({ "kind": "server", "alias": self.server_alias, "ssh": self.server_ssh }),
        )
    }
}

impl SettingsPage for CloudBoxesTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

/// A Cloud Boxes list row: the label with the hover-revealed info icon whose tooltip explains the
/// row, an optional one-line live state under it (the item's own data, like the Tools rows'
/// versions), and the controls.
///
/// CDXC:Settings 2026-10-01 SEE-ALSO: rows show no subtitle text (the 2026-09-09 decision in packages/core-ui/settings-modal/fields/primitives.tsx (deleted 2026-10-01)); every explanation on this page sits behind the info icon, and a row's detail line carries only its short state.
#[allow(clippy::too_many_arguments)]
pub(super) fn info_row(
    p: &SettingsPalette,
    id: &str,
    status: Option<ListItemStatus>,
    icon: &'static str,
    title: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    state: Option<String>,
    controls: Vec<AnyElement>,
) -> AnyElement {
    let group: SharedString = format!("cloud-boxes-row-{id}").into();
    let title_line = h_flex()
        .min_w_0()
        .items_center()
        .gap(px(4.0))
        .child(div().min_w_0().child(title.into()))
        .child(description_info_button(
            p,
            ElementId::Name(format!("cloud-boxes-row-{id}-info").into()),
            group.clone(),
            tooltip,
        ));
    div()
        .id(ElementId::Name(group.clone()))
        .group(group)
        .w_full()
        .child(settings_list_item(
            p,
            status,
            Some(settings_icon(icon, 17.0, p.muted).into_any_element()),
            title_line,
            state
                .filter(|state| !state.is_empty())
                .map(|state| div().child(state).into_any_element()),
            (!controls.is_empty()).then(|| {
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(controls)
                    .into_any_element()
            }),
        ))
        .into_any_element()
}

impl CloudBoxesTab {
    /// The agentbox row: installed or not, its version, Docker, and the status read's failure.
    fn status_row(
        &mut self,
        p: &SettingsPalette,
        summary: &StatusSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = self.rpc_available(cx);
        let checking = connected && self.status.is_none() && self.status_error.is_none();
        // (dot, the row's short state, the full sentence for its tooltip)
        let (dot, state, full) = if !connected {
            (
                ListItemStatus::Warning,
                "Server not reachable".to_string(),
                "Ghostex could not reach its server on this computer.".to_string(),
            )
        } else if let Some(error) = &self.status_error {
            (
                ListItemStatus::Warning,
                format!("Ghostex could not read agentbox status: {error}"),
                format!("Ghostex could not read agentbox status: {error}"),
            )
        } else if checking {
            (
                ListItemStatus::Neutral,
                "Checking…".to_string(),
                String::new(),
            )
        } else if !summary.supported {
            (
                ListItemStatus::Warning,
                "Needs macOS or Linux".to_string(),
                "Boxes run on macOS and Linux. On Windows, use Ghostex inside WSL.".to_string(),
            )
        } else if summary.installed == Some(false) {
            (
                ListItemStatus::Warning,
                "Not installed".to_string(),
                "Install adds it with npm.".to_string(),
            )
        } else {
            let mut state = vec![match &summary.version {
                Some(version) => format!("Version {version}"),
                None => "Installed".to_string(),
            }];
            let mut full = Vec::new();
            match summary.docker_ready {
                Some(true) => state.push("Docker running".to_string()),
                Some(false) => {
                    state.push("Docker not running".to_string());
                    full.push(
                        "Docker isn't running, so boxes on this computer can't start.".to_string(),
                    );
                }
                None => {}
            }
            if summary.portless_installed == Some(true) {
                full.push("Box web apps open at https://<box>.localhost.".to_string());
            }
            if let Some(error) = &summary.error {
                state.push("Last check failed".to_string());
                full.push(format!("The last check failed: {error}"));
            }
            let dot = if summary.error.is_some() || summary.docker_ready == Some(false) {
                ListItemStatus::Warning
            } else {
                ListItemStatus::Success
            };
            (dot, state.join(" · "), full.join(" "))
        };
        let installed = summary.installed == Some(true);
        let mut controls: Vec<AnyElement> = Vec::new();
        let unsupported = !summary.supported;
        if connected && summary.installed == Some(false) && !unsupported {
            controls.push(settings_button(
                p,
                "cloud-boxes-install",
                "Install agentbox",
                Some(ICON_DOWNLOAD),
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.run_terminal_command("install", &[], cx),
                cx,
            ));
        }
        if connected && installed && !unsupported {
            controls.push(settings_button(
                p,
                "cloud-boxes-doctor",
                "Run Check",
                Some(ICON_STETHOSCOPE),
                ButtonVariant::Outline,
                false,
                None,
                |page: &mut Self, _window, cx| page.run_terminal_command("doctor", &[], cx),
                cx,
            ));
        }
        if connected {
            controls.push(settings_button(
                p,
                "cloud-boxes-refresh",
                "Refresh",
                Some(ICON_REFRESH),
                ButtonVariant::Ghost,
                self.status_loading,
                Some("Checking agentbox…".into()),
                |page: &mut Self, _window, cx| page.refresh_all(cx),
                cx,
            ));
        }
        let tooltip = format!(
            "agentbox is the free, open-source command line tool Ghostex uses to run boxes. Run Check shows its own health check. {full}"
        );
        info_row(
            p,
            "agentbox",
            Some(dot),
            ICON_BOX,
            "agentbox",
            tooltip.trim().to_string(),
            Some(state),
            controls,
        )
    }

    fn set_up_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let button = settings_button(
            p,
            "cloud-boxes-set-up-for-me",
            "Set It Up for Me",
            Some(ICON_SPARKLES),
            ButtonVariant::Outline,
            false,
            None,
            |page: &mut Self, _window, cx| page.set_up_with_agent(cx),
            cx,
        );
        info_row(
            p,
            "set-up-for-me",
            None,
            ICON_SPARKLES,
            "Set it up for me",
            "An agent installs agentbox, asks which clouds you want, creates the API tokens in your browser, and signs Claude and Codex in for boxes. It asks before anything that costs money.",
            None,
            vec![button],
        )
    }

    fn learn_more_row(&mut self, p: &SettingsPalette, cx: &mut Context<Self>) -> AnyElement {
        let button = settings_button(
            p,
            "cloud-boxes-learn-more",
            "agentbox on GitHub",
            Some(ICON_EXTERNAL),
            ButtonVariant::Ghost,
            false,
            None,
            |page: &mut Self, _window, cx| page.open_url(AGENTBOX_REPO_URL, cx),
            cx,
        );
        info_row(
            p,
            "what-is-a-box",
            None,
            ICON_INFO,
            "What is a box?",
            "An isolated copy of your project where an agent works without touching this computer. Your agent's settings, skills and Codex sign-in go with it, and the box's web app opens here on your computer.",
            None,
            vec![button],
        )
    }

    fn overview_section(
        &mut self,
        p: &SettingsPalette,
        summary: &StatusSummary,
        show: impl Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut rows = Vec::new();
        if show("agentboxStatus") {
            rows.push(self.status_row(p, summary, cx));
        }
        if show("agentboxSetUpForMe") && summary.supported {
            rows.push(self.set_up_row(p, cx));
        }
        if show("agentboxWhatIsABox") {
            rows.push(self.learn_more_row(p, cx));
        }
        settings_section(
            p,
            "More providers (agentbox)",
            Some(
                "Run an agent session in an isolated box on this computer (Docker), on Hetzner, Vercel, Daytona, E2B or DigitalOcean, or on your own server. Ghostex uses agentbox, a free open-source command line tool."
                    .into(),
            ),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    /// "How it works": three short sentences as one text block, not rows.
    fn how_to_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
    ) -> Option<AnyElement> {
        let mut lines: Vec<&str> = Vec::new();
        if show("agentboxStartThread") {
            lines.push("Start a box: in New Thread, pick a place under Run on.");
        }
        if show("agentboxWebApp") {
            lines.push(
                "Open its web app: right-click the session and choose Open Box Web App. An agentbox.yaml with services.web.expose.port starts your dev server.",
            );
        }
        if show("agentboxBilling") {
            lines.push(
                "Cloud boxes bill until you stop or destroy them. Deleting a session stops its box.",
            );
        }
        if lines.is_empty() {
            return None;
        }
        let block = v_flex()
            .w_full()
            .gap(px(6.0))
            .text_size(px(13.0))
            .line_height(px(18.85))
            .text_color(gpui::Hsla::from(p.muted))
            .children(
                lines
                    .into_iter()
                    .map(|line| div().whitespace_normal().child(line)),
            );
        settings_section(p, "How it works", None, None, vec![card_inset(block)])
            .map(IntoElement::into_any_element)
    }
}

/// The Cloud Boxes sections, in page order.
enum CloudSection {
    Providers,
    Spending,
    Lifetime,
    Boxes,
    Logins,
}

impl Render for CloudBoxesTab {
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
                store.tab_search(SettingsTabId::CloudBoxes),
                matching,
            )
        };
        let summary = status_summary(self.status.as_ref());
        let ready_to_use = summary.supported && summary.installed != Some(false);
        let mut blocks: Vec<PageBlock> = Vec::new();
        if search.tab.is_searching && !search.tab.has_visible() {
            let store = self.store.clone();
            blocks.push(PageBlock::plain(render_no_matches(
                &p,
                SettingsTabId::CloudBoxes,
                &matching,
                move |tab, _window, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
            )));
        }
        let visible = |section: &'static str| {
            let search = search.clone();
            move |key: &str| search.row_visible(section, key)
        };
        let cloud_sections: [(&'static str, CloudSection); 5] = [
            ("cloudProviders", CloudSection::Providers),
            ("cloudSpending", CloudSection::Spending),
            ("cloudLifetime", CloudSection::Lifetime),
            ("cloudBoxList", CloudSection::Boxes),
            ("cloudAgentLogins", CloudSection::Logins),
        ];
        for (id, kind) in cloud_sections {
            if !search.section(id).has_visible() {
                continue;
            }
            let show = visible(id);
            let element = match kind {
                CloudSection::Providers => self.cloud_providers_section(&p, show, window, cx),
                CloudSection::Spending => self.cloud_spending_section(&p, show, window, cx),
                CloudSection::Lifetime => self.cloud_lifetime_section(&p, show, window, cx),
                CloudSection::Boxes => self.cloud_box_list_section(&p, show, cx),
                CloudSection::Logins => self.cloud_logins_section(&p, show, cx),
            };
            blocks.extend(element.map(|element| PageBlock::section(id, element)));
        }
        // CDXC:CloudBoxes 2026-11-02 DECISION: the user put today's agentbox providers (Docker, your SSH server, Hetzner, Vercel, Daytona, E2B, DigitalOcean) under Advanced (Q997 "advanced"): they keep working unchanged in the fold, and a search still shows their matching rows in place.
        let advanced_open = search.tab.is_searching
            || self
                .store
                .read(cx)
                .advanced_fold_open(SettingsTabId::CloudBoxes);
        if !search.tab.is_searching {
            blocks.push(PageBlock::section(
                "advancedFold",
                advanced_fold_header(
                    &p,
                    advanced_open,
                    |page: &mut Self, _window, cx| {
                        let store = page.store.clone();
                        store.update(cx, |store, cx| {
                            store.toggle_advanced_fold(SettingsTabId::CloudBoxes, cx)
                        });
                    },
                    cx,
                ),
            ));
        }
        if !advanced_open {
            return settings_page(&self.store, SettingsTabId::CloudBoxes, &p, blocks, cx);
        }
        if search.section("overview").has_visible() {
            blocks.extend(
                self.overview_section(&p, &summary, visible("overview"), cx)
                    .map(|element| PageBlock::section("overview", element)),
            );
        }
        if summary.supported && search.section("providers").has_visible() {
            blocks.extend(
                self.providers_section(&p, &summary, visible("providers"), window, cx)
                    .map(|element| PageBlock::section("providers", element)),
            );
        }
        if summary.supported && search.section("agentSignIn").has_visible() {
            blocks.extend(
                self.sign_in_section(&p, &summary, visible("agentSignIn"), cx)
                    .map(|element| PageBlock::section("agentSignIn", element)),
            );
        }
        if summary.supported && search.section("newThreads").has_visible() {
            blocks.extend(
                self.default_location_section(&p, visible("newThreads"), window, cx)
                    .map(|element| PageBlock::section("newThreads", element)),
            );
        }
        if ready_to_use && search.section("boxes").has_visible() {
            blocks.extend(
                self.boxes_section(&p, visible("boxes"), cx)
                    .map(|element| PageBlock::section("boxes", element)),
            );
        }
        if search.section("howTo").has_visible() {
            blocks.extend(
                self.how_to_section(&p, visible("howTo"))
                    .map(|element| PageBlock::section("howTo", element)),
            );
        }
        settings_page(&self.store, SettingsTabId::CloudBoxes, &p, blocks, cx)
    }
}
