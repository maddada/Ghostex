//! A Claude cloud session's chat pane: the session's claude.ai page, embedded where the chat's
//! transcript would be, once the session exists on claude.ai. Before that the pane is the native
//! chat, whose first message starts the session (server/src/claude_cloud/chat.rs), and while it
//! starts a short placeholder.
//!
//! The page opens in the Browser profile of the session's Claude account (CDXC:ClaudeCloud in
//! app/claude_cloud_page.rs), so a sign-in made there, in Settings > Accounts or in an earlier page
//! of the same account is reused, and it gets the same notice at its top (sign in as the account,
//! signed in as someone else, out of usage).
//!
//! CDXC:ClaudeCloud 2026-10-26 SEE-ALSO: packages/gx-core/src/claude_cloud.rs (the user's decision on `session_claude_cloud_page`), packages/gx-core/src/agentbox.rs (`session_chat_view_unavailable_showing_cloud_pages`), app/session_chat_surfaces.rs (the chat reconcile that shows and hides the pages).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Entity, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::v_flex;
use serde_json::Value;

use crate::app::claude_cloud_page::{
    claude_account_browser_profile, claude_cloud_page_notice, note_claude_account_signin,
};
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// How often the embedded pages are asked whether they are signed in, and as whom.
const SWEEP_INTERVAL: Duration = Duration::from_secs(2);

/// One session's embedded claude.ai page.
struct ChatPage {
    url: String,
    profile: String,
    surface: Entity<CefSurface>,
    /// The notice last put at the top of the page.
    notice: Option<Value>,
}

thread_local! {
    static CHAT_PAGES: RefCell<HashMap<TerminalSessionId, ChatPage>> = RefCell::new(HashMap::new());
    static SWEEP_STARTED: Cell<bool> = const { Cell::new(false) };
}

/// What a cloud session's chat pane needs from its row.
struct CloudChatTarget {
    state: String,
    /// The claude.ai page, once the session exists there (gx-core `session_claude_cloud_page`).
    url: Option<String>,
    account_id: Option<String>,
    account_name: Option<String>,
    account_email: Option<String>,
    limit_notice: Option<String>,
}

impl GhostexGpuiApp {
    fn claude_cloud_chat_target(&self, session_id: TerminalSessionId) -> Option<CloudChatTarget> {
        let key = self.agents_chat_local_key_for_session(session_id)?;
        let key = ghostex_gx_core::SessionKey::local(key.project_id, key.session_id);
        let cloud = self
            .gx_store
            .core
            .presentation()
            .session(&key)?
            .claude_cloud
            .clone()?;
        Some(CloudChatTarget {
            state: cloud.state,
            url: ghostex_gx_core::session_claude_cloud_page(&self.gx_store.core, &key),
            account_id: cloud.account_id,
            account_name: cloud.account_name,
            account_email: cloud.account_email.map(|email| email.to_lowercase()),
            limit_notice: cloud.limit_notice,
        })
    }

    /// The embedded page a cloud session's chat pane shows, when it has one.
    pub(crate) fn claude_cloud_chat_page_surface(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<Entity<CefSurface>> {
        let url = self.claude_cloud_chat_target(session_id)?.url?;
        CHAT_PAGES.with(|pages| {
            pages
                .borrow()
                .get(&session_id)
                .filter(|page| page.url == url)
                .map(|page| page.surface.clone())
        })
    }

    /// The chat pane of a Claude cloud session that has its claude.ai page or is starting; `None`
    /// for every other session and for a cloud session waiting for its task, whose pane is the
    /// native chat.
    pub(crate) fn render_claude_cloud_chat_page(
        &self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.claude_cloud_chat_target(session_id)?;
        let Some(url) = target.url else {
            return (target.state == "starting").then(|| {
                cloud_chat_placeholder(
                    session_id,
                    "Starting in Claude's cloud…",
                    "The session's claude.ai page opens here in a moment.",
                )
            });
        };
        let surface = CHAT_PAGES.with(|pages| {
            pages
                .borrow()
                .get(&session_id)
                .filter(|page| page.url == url)
                .map(|page| page.surface.clone())
        });
        if let Some(surface) = surface {
            return Some(
                div()
                    .id(format!("claude-cloud-chat-page-{}", session_id.0))
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(surface)
                    .into_any_element(),
            );
        }
        // The page is made by the chat reconcile, which also decides whether it shows.
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| app.reconcile_agents_pane_surfaces(cx));
        });
        Some(cloud_chat_placeholder(
            session_id,
            "Opening the session on claude.ai…",
            "",
        ))
    }

    /// Makes, shows and hides the embedded pages: a page shows exactly when its session's chat
    /// pane is on screen (`visible`, from the chat reconcile), and goes when its session leaves the
    /// workspace or no longer has the page.
    pub(crate) fn reconcile_claude_cloud_chat_pages(
        &mut self,
        visible: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        let live: HashSet<TerminalSessionId> = self
            .agents_workspace
            .terminal_session_ids()
            .into_iter()
            .collect();
        let pages: Vec<(TerminalSessionId, String)> = CHAT_PAGES.with(|pages| {
            pages
                .borrow()
                .iter()
                .map(|(id, page)| (*id, page.url.clone()))
                .collect()
        });
        for (session_id, url) in pages {
            let current = self
                .claude_cloud_chat_target(session_id)
                .and_then(|target| target.url);
            if live.contains(&session_id) && current.as_deref() == Some(url.as_str()) {
                continue;
            }
            if let Some(page) = CHAT_PAGES.with(|pages| pages.borrow_mut().remove(&session_id)) {
                page.surface
                    .update(cx, |surface, _| surface.set_visible(false));
            }
        }
        for session_id in visible {
            let Some(target) = self.claude_cloud_chat_target(*session_id) else {
                continue;
            };
            let Some(url) = target.url else {
                continue;
            };
            if CHAT_PAGES.with(|pages| pages.borrow().contains_key(session_id)) {
                continue;
            }
            let profile = match target.account_id.as_deref() {
                Some(account_id) => claude_account_browser_profile(account_id),
                // A session on the current CLI login: the window's own Browser profile.
                None => self.browser_tab_cef_profile(BrowserProfileId::default_profile()),
            };
            if !self.prepare_workspace_browser_context(&profile, cx) {
                // Made by a later reconcile, once the profile is ready (it notifies the app).
                continue;
            }
            let popup_open_handler = self.browser_popup_open_handler(None, None, cx);
            let surface = match CefSurface::try_new(
                format!("claude-cloud-chat-page-{}", session_id.0),
                self.parent_ns_view,
                url.clone(),
                profile.clone(),
                pane_prepaint_background_color(),
                true,
                chrome_color(0x0d0d0d, 0xffffff).into(),
                None,
                false,
                Some(popup_open_handler),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                cx,
            ) {
                Ok(surface) => surface,
                Err(error) => {
                    // Made again by the next reconcile (CDXC:CefRuntime 2026-07-11).
                    support_logs::append(
                        support_logs::GpuiSupportLog::CrashReports,
                        "gpui.cefSurface.createFailed",
                        serde_json::json!({ "surface": "claudeCloudChatPage", "error": error }),
                    );
                    continue;
                }
            };
            CHAT_PAGES.with(|pages| {
                pages.borrow_mut().insert(
                    *session_id,
                    ChatPage {
                        url,
                        profile,
                        surface,
                        notice: None,
                    },
                )
            });
        }
        let surfaces: Vec<(TerminalSessionId, Entity<CefSurface>)> = CHAT_PAGES.with(|pages| {
            pages
                .borrow()
                .iter()
                .map(|(id, page)| (*id, page.surface.clone()))
                .collect()
        });
        for (session_id, surface) in &surfaces {
            let shown = visible.contains(session_id);
            surface.update(cx, |surface, _| surface.set_visible(shown));
        }
        if !surfaces.is_empty() {
            self.ensure_claude_cloud_chat_page_sweep(cx);
            cx.notify();
        }
    }

    fn ensure_claude_cloud_chat_page_sweep(&mut self, cx: &mut gpui::Context<Self>) {
        if SWEEP_STARTED.with(|started| started.replace(true)) {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SWEEP_INTERVAL).await;
                if this
                    .update(cx, |this, cx| this.sweep_claude_cloud_chat_pages(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Asks each embedded page whether it is signed in and as whom, and puts the account's notice
    /// at its top, the way a cloud session's page in the Browser gets it.
    fn sweep_claude_cloud_chat_pages(&mut self, cx: &mut gpui::Context<Self>) {
        let pages: Vec<(TerminalSessionId, String, Entity<CefSurface>)> =
            CHAT_PAGES.with(|pages| {
                pages
                    .borrow()
                    .iter()
                    .map(|(id, page)| (*id, page.profile.clone(), page.surface.clone()))
                    .collect()
            });
        for (session_id, profile, surface) in pages {
            let answer = surface.read(cx).take_claude_cloud_page_answer();
            surface.read(cx).ask_claude_cloud_page();
            let Some(answer) = answer.and_then(|text| serde_json::from_str::<Value>(&text).ok())
            else {
                continue;
            };
            let Some(target) = self.claude_cloud_chat_target(session_id) else {
                continue;
            };
            let seen = answer["email"].as_str().map(str::to_lowercase);
            let signed_out = answer["signedOut"].as_bool() == Some(true);
            if let Some(account_id) = target.account_id.as_deref() {
                note_claude_account_signin(
                    account_id,
                    target.account_email.as_deref(),
                    seen.as_deref(),
                    signed_out,
                );
            }
            // A session on the current CLI login has no account to sign in as.
            let notice = target.account_id.as_ref().and_then(|_| {
                claude_cloud_page_notice(
                    target.account_name.as_deref().unwrap_or("this account"),
                    target.account_email.as_deref(),
                    target.limit_notice.as_deref(),
                    seen.as_deref(),
                    signed_out,
                )
            });
            let changed = CHAT_PAGES.with(|pages| {
                let mut pages = pages.borrow_mut();
                let Some(page) = pages.get_mut(&session_id) else {
                    return false;
                };
                let changed = page.notice != notice;
                page.notice = notice.clone();
                changed
            });
            if changed {
                let script = crate::cef::claude_cloud_page_notice_script(notice.as_ref());
                surface.update(cx, |surface, _| surface.execute_app_owned_script(&script));
            }
            match answer["request"].as_str() {
                Some("signOut") => {
                    if self.sign_claude_page_out(
                        &profile,
                        target.account_id.as_deref(),
                        &surface,
                        cx,
                    ) {
                        CHAT_PAGES.with(|pages| {
                            if let Some(page) = pages.borrow_mut().get_mut(&session_id) {
                                page.notice = None;
                            }
                        });
                    }
                }
                Some("copy") => {
                    if let Some(link) = target
                        .url
                        .as_deref()
                        .and_then(ghostex_gx_core::claude_cloud_session_link)
                    {
                        self.start_claude_cloud_copy_for_link(&link, cx);
                    }
                }
                _ => {}
            }
        }
    }
}

/// What a cloud session's chat pane shows while its page is not there yet.
fn cloud_chat_placeholder(session_id: TerminalSessionId, title: &str, message: &str) -> AnyElement {
    v_flex()
        .id(format!("claude-cloud-chat-placeholder-{}", session_id.0))
        .size_full()
        .min_w_0()
        .min_h_0()
        .items_center()
        .justify_center()
        .bg(glass_clear(gpui_session_chat_background_color()))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(workspace_terminal_placeholder_title_color())
                .child(title.to_string()),
        )
        .when(!message.is_empty(), |this| {
            this.child(
                div()
                    .mt(px(5.0))
                    .max_w(px(390.0))
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(workspace_terminal_placeholder_message_color())
                    .child(message.to_string()),
            )
        })
        .into_any_element()
}
