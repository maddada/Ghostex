//! Claude cloud sessions' claude.ai pages in the Browser: each saved Claude account's own Browser
//! profile, the reader that turns what a page shows into the session's status, the notice Ghostex
//! puts at the top of a page (sign in, wrong account, out of usage), and the per-account sign-in
//! state Settings > Accounts shows (`claude-cloud-signins.json`, read there by
//! window/settings_modal/tabs/accounts/editor.rs).
//!
//! CDXC:ClaudeCloud 2026-10-25 DECISION:
//! User: claude.ai sign-ins live in "one Browser profile per Claude account", kept on disk across restarts and built the same way as workspace profiles (`workspace-claude-<hash>`, so the workspace profile machinery opens it, cef/shell/workspace_browser.rs). Its cloud session pages and the built-in Claude view use it: for the Claude view the user picked "the same sign-in your cloud sessions use" (2026-10-25, CDXC:Extensions in app/claude_web/accounts.rs), superseding the 2026-10-23 "only for its cloud pages". Other Browser tabs keep the workspace's profile.
//!
//! CDXC:ClaudeCloud 2026-10-23 DECISION:
//! User: each account signs in "both" ways: the first time one of its rows needs it (the page shows "Sign in to claude.ai as <email> to see this session" over claude.ai's own login, in that account's profile), or ahead of time from Settings > Accounts; signing in never blocks starting a session. A profile belongs to an account "by email": the email claude.ai reports is matched to the account's email, and signing in as anyone else is a mismatch, answered with "a warning with one Sign in as <account> button"; nothing is signed out without that click.
//!
//! SEE-ALSO: cef/shell/claude_cloud_page.rs (the page reader and the notice script),
//! server/src/claude_cloud/page_status.rs (what a reading changes), packages/gx-core/src/claude_cloud.rs.

use crate::app::gx_store::gx_rpc_with_timeout;
use crate::app::model::*;
use crate::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// How often open claude.ai pages are asked.
const SWEEP_INTERVAL: Duration = Duration::from_secs(2);
const RPC_TIMEOUT: Duration = Duration::from_secs(20);
const SIGNIN_FILE: &str = "claude-cloud-signins.json";

/// What Ghostex last told gxserver and showed on one page.
#[derive(Default)]
struct PageMemory {
    reported: Option<Value>,
    notice: Option<Value>,
}

/// A Browser tab opened for one account's claude.ai sign-in.
#[derive(Clone)]
struct SignInTab {
    profile: String,
    account_id: String,
    name: String,
    email: Option<String>,
}

/// A Browser tab across projects: tab ids are numbered per project, and closing a project's last
/// tab turns that same id into its empty New Tab, which the next link reuses.
type TabKey = (Option<String>, BrowserTabId);

thread_local! {
    /// The Claude account each Browser tab's claude.ai page belongs to, until the tab closes.
    static SIGN_IN_TABS: RefCell<HashMap<TabKey, SignInTab>> = RefCell::new(HashMap::new());
    /// The sign-in Settings just asked for, claimed by the login tab it opens.
    static PENDING_SIGN_IN: RefCell<Option<SignInTab>> = const { RefCell::new(None) };
    static PAGES: RefCell<HashMap<BrowserTabId, PageMemory>> = RefCell::new(HashMap::new());
    static SIGNINS: RefCell<Option<HashMap<String, Value>>> = const { RefCell::new(None) };
    static SWEEP_STARTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The Browser profile of a saved Claude account's claude.ai pages.
pub(crate) fn claude_account_browser_profile(account_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(account_id.trim().as_bytes()));
    format!("workspace-claude-{}", &digest[..24])
}

/// What Ghostex last saw of a saved Claude account's claude.ai sign-in: `Some(Some(email))`
/// signed in, `Some(None)` signed out, `None` not seen yet.
pub(crate) fn claude_account_signin_email(account_id: &str) -> Option<Option<String>> {
    with_signins(|map| {
        map.get(account_id).map(|entry| {
            entry
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    })
}

fn signin_file() -> std::path::PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join(SIGNIN_FILE)
}

fn with_signins<T>(read: impl FnOnce(&mut HashMap<String, Value>) -> T) -> T {
    SIGNINS.with(|signins| {
        let mut signins = signins.borrow_mut();
        let map = signins.get_or_insert_with(|| {
            std::fs::read_to_string(signin_file())
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        });
        read(map)
    })
}

pub(crate) fn remember_signin(account_id: &str, email: Option<&str>) {
    let changed = with_signins(|map| {
        let entry = json!({ "email": email, "at": chrono::Utc::now().to_rfc3339() });
        let same = map.get(account_id).and_then(|value| value.get("email")) == entry.get("email");
        map.insert(account_id.to_string(), entry);
        !same
    });
    if changed {
        let text = with_signins(|map| serde_json::to_string(map).unwrap_or_default());
        let path = signin_file();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, text);
    }
}

/// Keeps what a claude.ai page on a saved account's profile shows of its sign-in: signed in as the
/// account's own email, or signed out (signed in as anyone else changes nothing). Returns whether
/// the page is signed in as the account.
pub(crate) fn note_claude_account_signin(
    account_id: &str,
    account_email: Option<&str>,
    seen: Option<&str>,
    signed_out: bool,
) -> bool {
    match (seen, signed_out) {
        (Some(seen), _) if account_email == Some(seen) => {
            remember_signin(account_id, Some(seen));
            true
        }
        (_, true) => {
            remember_signin(account_id, None);
            false
        }
        _ => false,
    }
}

/// The cloud session a page shows: its account and the email it must be signed in as.
struct ExpectedAccount {
    account_id: String,
    name: String,
    email: Option<String>,
    limit_notice: Option<String>,
}

impl GhostexGpuiApp {
    fn claude_tab_key(&self, tab_id: BrowserTabId) -> TabKey {
        (self.browser_tabs_project_id.clone(), tab_id)
    }

    fn claude_tab_account(&self, tab_id: BrowserTabId) -> Option<SignInTab> {
        let key = self.claude_tab_key(tab_id);
        SIGN_IN_TABS.with(|tabs| tabs.borrow().get(&key).cloned())
    }

    fn bind_claude_tab_account(&self, tab_id: BrowserTabId, tab: SignInTab) {
        let key = self.claude_tab_key(tab_id);
        SIGN_IN_TABS.with(|tabs| tabs.borrow_mut().insert(key, tab));
    }

    /// A Browser tab closed (or turned back into the empty New Tab): its id no longer belongs to
    /// the Claude account its page was signed in for.
    ///
    /// CDXC:ClaudeCloud 2026-10-23 WHY: the user signed in one account, closed its tab and clicked Sign in for a second account, and the second account's login opened in the first account's profile, already signed in as the first (seen 2026-10-23). Closing a project's last tab keeps its id for the empty New Tab, the next link loads into it, and the account was still remembered by that id. An account belongs to a tab only until the tab closes, and the id is scoped to its project.
    pub(crate) fn forget_claude_cloud_tab(&self, project_id: Option<&str>, tab_id: BrowserTabId) {
        let key = (project_id.map(str::to_string), tab_id);
        SIGN_IN_TABS.with(|tabs| tabs.borrow_mut().remove(&key));
    }

    /// The CEF profile a Browser tab opens with when it is a claude.ai page of a cloud session (its
    /// account's profile) or a sign-in tab Ghostex opened for an account; `None` for every other
    /// tab, which keeps the workspace's profile.
    pub(crate) fn claude_cloud_tab_profile(
        &self,
        tab_id: BrowserTabId,
        url: &str,
    ) -> Option<String> {
        // The Sign in Settings just asked for wins over anything the tab was before.
        if url.starts_with("https://claude.ai/login")
            && let Some(pending) = PENDING_SIGN_IN.with(|pending| pending.borrow_mut().take())
        {
            let profile = pending.profile.clone();
            self.bind_claude_tab_account(tab_id, pending);
            return Some(profile);
        }
        if let Some(tab) = self.claude_tab_account(tab_id) {
            return Some(tab.profile);
        }
        // A cloud session's page, or claude.ai's login it redirected to: the tab keeps its account
        // from here on, so the sign-in notice still knows it on the login page.
        let expected = self.expected_claude_account(tab_id, url)?;
        let tab = SignInTab {
            profile: claude_account_browser_profile(&expected.account_id),
            account_id: expected.account_id,
            name: expected.name,
            email: expected.email,
        };
        let profile = tab.profile.clone();
        self.bind_claude_tab_account(tab_id, tab);
        Some(profile)
    }

    /// A tab a Claude account's claude.ai page opens gets the page's own profile, so what is signed
    /// in there lands in that account's cookies.
    ///
    /// CDXC:ClaudeCloud 2026-10-23 WHY: the user may sign in to claude.ai with Google. A page's sign-in popup is its own window in the page's profile (cef/shell/sign_in_popup.rs), but any other page it opens becomes a Browser tab, which would open on the workspace's profile and keep that sign-in out of the account's profile; the tab inherits the profile of the tab it came from instead.
    pub(crate) fn inherit_claude_cloud_tab_profile(
        &mut self,
        source_tab: BrowserTabId,
        source_url: &str,
        popup_tab: BrowserTabId,
    ) {
        let inherited = self.claude_tab_account(source_tab).or_else(|| {
            let expected = self.expected_claude_account(source_tab, source_url)?;
            Some(SignInTab {
                profile: claude_account_browser_profile(&expected.account_id),
                account_id: expected.account_id,
                name: expected.name,
                email: expected.email,
            })
        });
        if let Some(inherited) = inherited {
            self.bind_claude_tab_account(popup_tab, inherited);
        }
    }

    /// Settings > Accounts' "Sign in to claude.ai": claude.ai's login in the account's own
    /// profile, in a new Browser tab of the active project.
    pub(crate) fn open_claude_account_sign_in(
        &mut self,
        account_id: &str,
        name: &str,
        email: Option<&str>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.ensure_claude_cloud_page_sweep(cx);
        PENDING_SIGN_IN.with(|pending| {
            *pending.borrow_mut() = Some(SignInTab {
                profile: claude_account_browser_profile(account_id),
                account_id: account_id.to_string(),
                name: name.to_string(),
                email: email.map(str::to_lowercase),
            })
        });
        self.open_browser_url_from_renderer_command(
            GpuiSidebarOpenBrowserUrlMessage {
                url: "https://claude.ai/login".to_string(),
                reuse: GpuiBrowserRendererOpenReuse::None,
                from_quick_header: false,
                project_id: None,
            },
            window,
            cx,
        );
        // A login tab whose page is made later (its profile was still being set up) keeps the
        // account until then; nothing else may claim it.
        if let Some(pending) = PENDING_SIGN_IN.with(|pending| pending.borrow_mut().take())
            && let Some(tab) = self.browser_tabs.active_tab()
            && tab.url.starts_with("https://claude.ai/login")
            && !self.browser_surfaces.contains_key(&tab.id)
        {
            self.bind_claude_tab_account(tab.id, pending);
        }
    }

    /// Start a Copy on Another Account: asks which saved Claude account the copy runs on, then
    /// starts it (server/src/claude_cloud/copy.rs) and selects its row.
    pub(crate) fn start_claude_cloud_copy(
        &mut self,
        session: ghostex_gx_core::SessionKey,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let current = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
            .and_then(|row| row.claude_cloud.as_ref()?.account_id.clone());
        let accounts: Vec<(String, String)> = self
            .new_thread_picker_accounts
            .as_ref()
            .map(crate::app::new_thread_picker_lifecycle::new_thread_picker_accounts)
            .unwrap_or_default()
            .into_iter()
            .filter(|account| {
                account.provider == "claude"
                    && account.ready
                    && Some(&account.id) != current.as_ref()
            })
            .take(4)
            .map(|account| (account.id, account.name))
            .collect();
        if accounts.is_empty() {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "No other Claude account to start a copy on",
                "Add another Claude account in Settings > Accounts first.",
                cx,
            );
            return;
        }
        let mut buttons: Vec<&str> = vec!["Cancel"];
        buttons.extend(accounts.iter().map(|(_, name)| name.as_str()));
        let answer = window.prompt(
            gpui::PromptLevel::Info,
            "Start a copy on which account?",
            Some("A new cloud session carries on from this session's branch, with a summary of its task. This session stays where it is."),
            &buttons,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            let Some((account_id, _)) = choice.checked_sub(1).and_then(|index| accounts.get(index))
            else {
                return;
            };
            let params = json!({
                "action": "copy",
                "projectId": session.project_id,
                "sessionId": session.session_id,
                "accountId": account_id,
            });
            let result = gx_rpc_with_timeout(None, "/api/claudeCloud", params, RPC_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(value) => {
                    if let (Some(project_id), Some(session_id)) =
                        (value["projectId"].as_str(), value["sessionId"].as_str())
                    {
                        let row_id = ghostex_gx_core::SessionKey::local(project_id, session_id)
                            .to_sidebar_session_id();
                        this.dispatch_native_sidebar_ui(
                            json!({"type": "selectSession", "sessionId": row_id, "mode": "focus"}),
                            cx,
                        );
                    }
                }
                Err(error) => this.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Could not start the copy",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }

    /// Settings > Accounts' "Sign out": forgets the account's claude.ai cookies.
    pub(crate) fn sign_claude_account_out(&mut self, account_id: &str) -> Result<(), String> {
        let profile = claude_account_browser_profile(account_id);
        sign_profile_out_of_claude(&profile)?;
        remember_signin(account_id, None);
        Ok(())
    }

    /// Starts the page sweep once per app run.
    pub(crate) fn ensure_claude_cloud_page_sweep(&mut self, cx: &mut gpui::Context<Self>) {
        if SWEEP_STARTED.with(|started| started.replace(true)) {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SWEEP_INTERVAL).await;
                if this
                    .update(cx, |this, cx| this.sweep_claude_cloud_pages(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn expected_claude_account(&self, tab_id: BrowserTabId, url: &str) -> Option<ExpectedAccount> {
        if let Some(tab) = self.claude_tab_account(tab_id) {
            return Some(ExpectedAccount {
                account_id: tab.account_id,
                name: tab.name,
                email: tab.email,
                limit_notice: None,
            });
        }
        let link = cloud_session_link_in(url)?;
        let loaded = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)?;
        let cloud = loaded
            .server_sessions()
            .filter_map(|session| session.claude_cloud.as_ref())
            .find(|cloud| {
                cloud
                    .url
                    .as_deref()
                    .and_then(ghostex_gx_core::claude_cloud_session_link)
                    == Some(link.clone())
            })?;
        Some(ExpectedAccount {
            account_id: cloud.account_id.clone()?,
            name: cloud
                .account_name
                .clone()
                .unwrap_or_else(|| "this account".to_string()),
            email: cloud
                .account_email
                .clone()
                .map(|email| email.to_lowercase()),
            limit_notice: cloud.limit_notice.clone(),
        })
    }

    /// Asks every open claude.ai page what it shows, and acts on the answers already in.
    fn sweep_claude_cloud_pages(&mut self, cx: &mut gpui::Context<Self>) {
        // Tabs that are gone or back to an empty New Tab (the projectless tabs go when a project
        // opens) keep no account.
        let live_project = self.browser_tabs_project_id.clone();
        SIGN_IN_TABS.with(|tabs| {
            tabs.borrow_mut().retain(|(project, tab_id), _| {
                if *project != live_project {
                    return project.is_some();
                }
                self.browser_tabs
                    .tab(*tab_id)
                    .is_some_and(|tab| tab.state == BrowserTabState::Loaded)
            })
        });
        let tabs: Vec<(BrowserTabId, String, Entity<CefSurface>)> = self
            .browser_surfaces
            .iter()
            .filter_map(|(tab_id, surface)| {
                let url = self.browser_tabs.tab(*tab_id)?.url.clone();
                url.starts_with("https://claude.ai/")
                    .then(|| (*tab_id, url, surface.clone()))
            })
            .collect();
        PAGES.with(|pages| {
            pages
                .borrow_mut()
                .retain(|tab_id, _| tabs.iter().any(|(id, _, _)| id == tab_id))
        });
        for (tab_id, url, surface) in tabs {
            let answer = surface.read(cx).take_claude_cloud_page_answer();
            surface.read(cx).ask_claude_cloud_page();
            let Some(answer) = answer.and_then(|text| serde_json::from_str::<Value>(&text).ok())
            else {
                continue;
            };
            self.handle_claude_cloud_page_answer(tab_id, &url, &surface, answer, cx);
        }
        // The Claude view's page, on a saved account's profile (app/claude_web/accounts.rs).
        self.sweep_claude_web_page(cx);
    }

    /// A notice's "Sign in as <account>" (and the Claude view's Sign out): signs the page's
    /// profile out of claude.ai and sends the page to claude.ai's login. Returns whether it did.
    pub(crate) fn sign_claude_page_out(
        &mut self,
        profile: &str,
        account_id: Option<&str>,
        surface: &Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if let Err(error) = sign_profile_out_of_claude(profile) {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Could not sign this page out of claude.ai",
                &error,
                cx,
            );
            return false;
        }
        if let Some(account_id) = account_id {
            remember_signin(account_id, None);
        }
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script("location.href = 'https://claude.ai/login';");
        });
        true
    }

    fn handle_claude_cloud_page_answer(
        &mut self,
        tab_id: BrowserTabId,
        tab_url: &str,
        surface: &Entity<CefSurface>,
        answer: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let page_url = answer["url"].as_str().unwrap_or(tab_url).to_string();
        let expected = self.expected_claude_account(tab_id, &page_url);
        let email = answer["email"].as_str().map(str::to_lowercase);
        let signed_out = answer["signedOut"].as_bool() == Some(true);
        if let Some(expected) = &expected
            && note_claude_account_signin(
                &expected.account_id,
                expected.email.as_deref(),
                email.as_deref(),
                signed_out,
            )
        {
            crate::app::claude_cloud_sign_in_return::finish_claude_sign_in_return(
                &expected.account_id,
                cx,
            );
        }
        // The reading, for a cloud session's page (gxserver ignores any other page).
        if ghostex_gx_core::claude_cloud_session_link(&page_url).is_some() {
            let report = json!({
                "action": "pageStatus",
                "url": page_url,
                "status": answer["status"].as_str().unwrap_or("unknown"),
                "branch": answer.get("branch").cloned().unwrap_or(Value::Null),
                "prUrl": answer.get("prUrl").cloned().unwrap_or(Value::Null),
                "limit": answer.get("limit").cloned().unwrap_or(Value::Null),
            });
            let changed = PAGES.with(|pages| {
                let mut pages = pages.borrow_mut();
                let memory = pages.entry(tab_id).or_default();
                let changed = memory.reported.as_ref() != Some(&report);
                memory.reported = Some(report.clone());
                changed
            });
            if changed {
                cx.background_executor()
                    .spawn(async move {
                        let _ = gx_rpc_with_timeout(None, "/api/claudeCloud", report, RPC_TIMEOUT)
                            .await;
                    })
                    .detach();
            }
        }
        let notice = expected.as_ref().and_then(|expected| {
            claude_cloud_page_notice(
                &expected.name,
                expected.email.as_deref(),
                expected.limit_notice.as_deref(),
                email.as_deref(),
                signed_out,
            )
        });
        let show = PAGES.with(|pages| {
            let mut pages = pages.borrow_mut();
            let memory = pages.entry(tab_id).or_default();
            let show = memory.notice != notice;
            memory.notice = notice.clone();
            show
        });
        if show {
            let script = crate::cef::claude_cloud_page_notice_script(notice.as_ref());
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(&script);
            });
        }
        match answer["request"].as_str() {
            Some("signOut") => {
                let profile = self
                    .claude_tab_account(tab_id)
                    .map(|tab| tab.profile)
                    .or_else(|| {
                        expected
                            .as_ref()
                            .map(|expected| claude_account_browser_profile(&expected.account_id))
                    });
                if let Some(profile) = profile
                    && self.sign_claude_page_out(
                        &profile,
                        expected
                            .as_ref()
                            .map(|expected| expected.account_id.as_str()),
                        surface,
                        cx,
                    )
                {
                    PAGES.with(|pages| pages.borrow_mut().remove(&tab_id));
                }
            }
            Some("copy") => {
                if let Some(link) = ghostex_gx_core::claude_cloud_session_link(&page_url) {
                    self.start_claude_cloud_copy_for_link(&link, cx);
                }
            }
            _ => {}
        }
    }

    /// "Start a copy on another account" from a page's notice: the session's row's own action.
    pub(crate) fn start_claude_cloud_copy_for_link(
        &mut self,
        link: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)
            .and_then(|loaded| {
                loaded.server_sessions().find(|session| {
                    session
                        .claude_cloud
                        .as_ref()
                        .and_then(|cloud| cloud.url.as_deref())
                        .and_then(ghostex_gx_core::claude_cloud_session_link)
                        .as_deref()
                        == Some(link)
                })
            })
            .map(|session| {
                ghostex_gx_core::SessionKey::local(
                    session.project_id.clone(),
                    session.session_id.clone(),
                )
            })
        else {
            return;
        };
        self.defer_in_main_window(cx, move |this, window, cx| {
            this.start_claude_cloud_copy(session, window, cx);
        });
    }
}

/// The notice Ghostex puts at the top of a cloud session's claude.ai page for the account it runs
/// on (`name`, `email`): sign in, signed in as someone else, or out of usage. `seen` is the email
/// the page is signed in as.
pub(crate) fn claude_cloud_page_notice(
    name: &str,
    email: Option<&str>,
    limit_notice: Option<&str>,
    seen: Option<&str>,
    signed_out: bool,
) -> Option<Value> {
    let wanted = email.unwrap_or(name);
    if signed_out {
        return Some(json!({
            "text": format!(
                "Sign in to claude.ai as {wanted} to see this session. You only do this once for {name}."
            ),
        }));
    }
    if let (Some(seen), Some(want)) = (seen, email)
        && seen != want
    {
        return Some(json!({
            "text": format!("This page is signed in as {seen}, but the session runs on {name} ({want})."),
            "button": format!("Sign in as {name}"),
            "action": "signOut",
        }));
    }
    limit_notice.map(|limit| {
        json!({
            "text": limit,
            "button": "Start a copy on another account",
            "action": "copy",
        })
    })
}

/// The cloud session link a claude.ai page belongs to: the page itself, or the session claude.ai's
/// login page returns to (`/login?returnTo=%2Fcode%2Fsession_…`).
fn cloud_session_link_in(url: &str) -> Option<String> {
    ghostex_gx_core::claude_cloud_session_link(url).or_else(|| {
        let query = url
            .strip_prefix("https://claude.ai/login")?
            .split_once('?')?
            .1;
        let return_to = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("returnTo="))?;
        let decoded = return_to
            .replace("%2F", "/")
            .replace("%2f", "/")
            .replace("%3F", "?")
            .replace("%3f", "?");
        ghostex_gx_core::claude_cloud_session_link(&format!("https://claude.ai{decoded}"))
    })
}

/// Signs one profile out of claude.ai: its claude.ai cookies are deleted, nothing else.
pub(crate) fn sign_profile_out_of_claude(profile: &str) -> Result<(), String> {
    crate::cef::clear_workspace_browser_site_cookies(
        profile,
        &["https://claude.ai", "https://anthropic.com"],
    )
    .map_err(|_| "Open one of this account's claude.ai pages first.".to_string())
}
