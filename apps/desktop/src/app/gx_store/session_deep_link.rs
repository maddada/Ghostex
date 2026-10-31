//! `ghostex://session/<Global Ref>`: a link from outside Ghostex brings the app forward and focuses
//! that session, on this computer or a connected machine, or offers to restore it from Previous
//! Sessions. The rules are gx-core's (`session_deep_link.rs`); this file performs them.
//!
//! CDXC:OsIntegration 2026-10-31 WHY:
//! A link can be clicked from any website, and a page can watch its own window lose focus. So the app comes forward first, in the same step for every link, valid or not, before anything is looked up: what the page can see is the same whether the session exists or not, and nothing goes back to it.
//!
//! One prompt at a time, and a link that arrives while one is open is dropped: a page opening links in a loop cannot stack dialogs or answer one by replacing it. A newer link also ends the lookup of an older one.
//!
//! SEE-ALSO: apps/desktop/src/app/sidebar_dispatch/folder_pickers_and_os_integration.rs (the route), apps/desktop/src/session_link_handoff.rs (how a link reaches this process on Windows and Linux), apps/desktop/src/app/gx_store/notifications/commands.rs (the same machine switch, focus and reveal for a notification).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ghostex_gx_core::{
    SessionKey, SessionLinkNotice, SessionLinkPlace, SessionLinkPrompt, SessionLinkStep,
    SidebarUiIntent, previous_session_title, session_link_gone_notice, session_link_invalid_notice,
    session_link_not_connected_notice, session_link_not_known_notice, session_link_restore_prompt,
    session_link_start_prompt, session_link_step, session_link_unanswered_notice,
};
use ghostex_gx_protocol::{GlobalSessionRef, parse_session_link};
use gpui::{AsyncApp, Window};
use serde_json::json;

use super::focus_perform::RowFocusOptions;
use super::gx_rpc_with_timeout;
use crate::GhostexGpuiApp;
use crate::app::model::gpui_click_to_wake_sleeping_sessions_from_shared_settings;
use crate::shared_settings;

/// How long a link waits for a machine that may still answer (a connect over SSH, an app still
/// starting) before it says what it found.
const LOOKUP_LIMIT: Duration = Duration::from_secs(30);
/// Since the app started, how long a machine that reads `disconnected` may simply not have begun
/// connecting yet.
const LAUNCH_WINDOW: Duration = Duration::from_secs(20);
const LOOKUP_POLL: Duration = Duration::from_millis(250);
const PREVIOUS_SESSIONS_TIMEOUT: Duration = Duration::from_secs(15);

/// The newest link's number; a lookup whose link is older stops.
static NEWEST_LINK: AtomicU64 = AtomicU64::new(0);
static PROMPT_OPEN: AtomicBool = AtomicBool::new(false);
static APP_STARTED: OnceLock<Instant> = OnceLock::new();

/// Called once the app has windows, to tell a link opened at launch from one opened later.
pub(crate) fn note_app_started_for_session_links() {
    APP_STARTED.get_or_init(Instant::now);
}

fn app_is_launching() -> bool {
    APP_STARTED
        .get()
        .is_none_or(|started| started.elapsed() < LAUNCH_WINDOW)
}

/// The ref's parts, owned for the lookup.
struct LinkTarget {
    server_id: String,
    project_id: String,
    session_id: String,
}

impl LinkTarget {
    fn as_ref(&self) -> GlobalSessionRef<'_> {
        GlobalSessionRef {
            server_id: &self.server_id,
            project_id: &self.project_id,
            session_id: &self.session_id,
        }
    }
}

impl GhostexGpuiApp {
    /// A link the operating system handed to the app, exactly as it arrived.
    pub(crate) fn open_session_deep_link(
        &mut self,
        link: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.activate(true);
        window.activate_window();
        let Some(target) = parse_session_link(link) else {
            self.show_session_link_notice(session_link_invalid_notice(), cx);
            return;
        };
        if PROMPT_OPEN.load(Ordering::Acquire) {
            return;
        }
        let target = LinkTarget {
            server_id: target.server_id.to_string(),
            project_id: target.project_id.to_string(),
            session_id: target.session_id.to_string(),
        };
        let number = NEWEST_LINK.fetch_add(1, Ordering::AcqRel) + 1;
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let place = loop {
                if NEWEST_LINK.load(Ordering::Acquire) != number {
                    return;
                }
                let Ok(place) = this.update(cx, |this, _| {
                    ghostex_gx_core::place_session_link(
                        this.gx_store.core.presentation(),
                        this.gx_store.remote.tabs(),
                        &target.as_ref(),
                        app_is_launching(),
                    )
                }) else {
                    return;
                };
                if place != SessionLinkPlace::Waiting {
                    break Some(place);
                }
                if started.elapsed() >= LOOKUP_LIMIT {
                    break None;
                }
                cx.background_executor().timer(LOOKUP_POLL).await;
            };
            let notice = match place {
                Some(SessionLinkPlace::Reachable(session)) => {
                    run_session_link_step(&this, session, cx).await;
                    return;
                }
                Some(SessionLinkPlace::NotConnected { label }) => {
                    session_link_not_connected_notice(&label)
                }
                Some(SessionLinkPlace::NotKnown { unconnected }) => {
                    session_link_not_known_notice(&target.server_id, &unconnected)
                }
                Some(SessionLinkPlace::Waiting) | None => session_link_unanswered_notice(None),
            };
            let _ = this.update(cx, |this, cx| this.show_session_link_notice(notice, cx));
        })
        .detach();
    }

    fn show_session_link_notice(
        &mut self,
        notice: SessionLinkNotice,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast("warning", &notice.title, &notice.description, cx);
    }

    /// The name of the machine a session is on; `None` for this computer.
    fn session_link_machine_label(&self, session: &SessionKey) -> Option<String> {
        let machine_id = session.machine.remote_id()?;
        Some(
            self.gx_store
                .remote
                .tabs()
                .iter()
                .find(|tab| tab.machine_id == machine_id)
                .map_or_else(|| machine_id.to_string(), |tab| tab.label.clone()),
        )
    }

    /// The sidebar goes to the session's computer, then the session is focused and revealed there,
    /// in the window that shows it.
    fn focus_session_link_target(
        &mut self,
        session: SessionKey,
        keep_sleeping: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let row_id = session.to_sidebar_session_id();
        let target_row = row_id.clone();
        self.activate_session_in_its_window(&target_row, window, cx, move |app, cx| {
            app.select_session_link_machine(&session, cx);
            app.gx_store_flush_local_selection(cx);
            let options = RowFocusOptions {
                keep_sleeping,
                ..RowFocusOptions::default()
            };
            app.gx_store_focus_session_row(&row_id, options, cx);
            let request_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_micros() as u64);
            app.gx_store_note_local_sidebar_reveal(&row_id, request_id);
            app.gx_store_note_sidebar_reveal(&row_id, request_id, cx);
            app.gx_store_sidebar_state_changed(cx);
        });
    }

    fn select_session_link_machine(&mut self, session: &SessionKey, cx: &mut gpui::Context<Self>) {
        let machine_id = session
            .machine
            .remote_id()
            .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
            .to_string();
        self.gx_store_apply_sidebar_ui_intent(SidebarUiIntent::SelectMachine { machine_id }, cx);
    }
}

/// What the link does to a session on a machine that can answer. Every path that starts anything
/// goes through [`confirmed`].
async fn run_session_link_step(
    this: &gpui::WeakEntity<GhostexGpuiApp>,
    session: SessionKey,
    cx: &mut AsyncApp,
) {
    let Ok((step, machine)) = this.update(cx, |this, _| {
        let click_to_wake = gpui_click_to_wake_sleeping_sessions_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        );
        (
            session_link_step(this.gx_store.core.presentation(), &session, click_to_wake),
            this.session_link_machine_label(&session),
        )
    }) else {
        return;
    };
    match step {
        SessionLinkStep::Focus | SessionLinkStep::FocusKeepingAsleep => {
            let keep_sleeping = step == SessionLinkStep::FocusKeepingAsleep;
            let _ = this.update_in(cx, |this, window, cx| {
                this.focus_session_link_target(session, keep_sleeping, window, cx);
            });
        }
        SessionLinkStep::AskToStart { title } => {
            let prompt = session_link_start_prompt(&title, machine.as_deref());
            if confirmed(this, prompt, cx).await {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.focus_session_link_target(session, false, window, cx);
                });
            }
        }
        SessionLinkStep::LookInPreviousSessions => {
            offer_restore(this, session, machine, cx).await;
        }
    }
}

/// A session no machine lists as open: Previous Sessions has it, and the user is asked, or nothing
/// has it, and the link says so.
async fn offer_restore(
    this: &gpui::WeakEntity<GhostexGpuiApp>,
    session: SessionKey,
    machine: Option<String>,
    cx: &mut AsyncApp,
) {
    let target = match session.machine.remote_id() {
        Some(machine_id) => {
            let Ok(target) = this.update(cx, |this, _| {
                this.gpui_remote_gxserver_request_target(machine_id)
            }) else {
                return;
            };
            let Some(target) = target else {
                let label = machine.as_deref().unwrap_or(machine_id);
                let notice = session_link_not_connected_notice(label);
                let _ = this.update(cx, |this, cx| this.show_session_link_notice(notice, cx));
                return;
            };
            Some(target)
        }
        None => None,
    };
    // The same question the restore itself asks (`restore_previous_session` in gxserver).
    let previous = gx_rpc_with_timeout(
        target,
        "/api/listPreviousSessions",
        json!({
            "includeActive": false,
            "includePrevious": true,
            "limit": 20,
            "projectId": session.project_id,
            "query": session.session_id,
        }),
        PREVIOUS_SESSIONS_TIMEOUT,
    )
    .await;
    let previous = match previous {
        Ok(previous) => previous,
        Err(_) => {
            let notice = session_link_unanswered_notice(machine.as_deref());
            let _ = this.update(cx, |this, cx| this.show_session_link_notice(notice, cx));
            return;
        }
    };
    let Some(title) = previous_session_title(&previous, &session.project_id, &session.session_id)
    else {
        let notice = session_link_gone_notice(machine.as_deref());
        let _ = this.update(cx, |this, cx| this.show_session_link_notice(notice, cx));
        return;
    };
    let prompt = session_link_restore_prompt(&title, machine.as_deref());
    if !confirmed(this, prompt, cx).await {
        return;
    }
    let history_id = match session.machine.remote_id() {
        Some(machine_id) => format!(
            "remote-gxserver:{machine_id}:{}:{}",
            session.project_id, session.session_id
        ),
        None => format!("gxserver:{}:{}", session.project_id, session.session_id),
    };
    let _ = this.update(cx, |this, cx| {
        this.select_session_link_machine(&session, cx);
        this.restore_previous_session_from_history(history_id, cx);
    });
}

/// Asks, and answers whether the user pressed the confirming button.
async fn confirmed(
    this: &gpui::WeakEntity<GhostexGpuiApp>,
    prompt: SessionLinkPrompt,
    cx: &mut AsyncApp,
) -> bool {
    if PROMPT_OPEN.swap(true, Ordering::AcqRel) {
        return false;
    }
    let receiver = this.update_in(cx, |_, window, cx| {
        window.prompt(
            gpui::PromptLevel::Info,
            &prompt.title,
            Some(&prompt.detail),
            &[
                gpui::PromptButton::Ok(prompt.confirm.clone().into()),
                gpui::PromptButton::Cancel("Cancel".into()),
            ],
            cx,
        )
    });
    let answer = match receiver {
        Ok(receiver) => receiver.await.ok(),
        Err(_) => None,
    };
    PROMPT_OPEN.store(false, Ordering::Release);
    answer == Some(0)
}
