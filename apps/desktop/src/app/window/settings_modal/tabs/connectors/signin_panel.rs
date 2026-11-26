//! The panel a browser sign-in leaves on the page until it is finished: what to do, the one-time
//! code in large type with Copy (Ghostex copies it before it opens the page), and buttons to open
//! the page again or copy its link. Every sign-in and connect on the page uses it: Composio, its
//! apps, the Executor CLI and the gh and gws sign-ins.
//!
//! CDXC:Connectors 2026-10-17 DECISION:
//! User, on the first gh sign-in: "the ux for this is VERY bad", "we need to make the button that opens copy the code", "this tiny text is NOT GOOD", and "this ux needs to improve for all of them that have the same issue please, not just gh cli". Sign in copies the code before it opens the page, and every browser sign-in shows this panel in normal-size text, the code large, until the page sees it finished or the user closes it.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{ButtonVariant, ROW_PADDING_X, ROW_PADDING_Y, settings_button};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_copy_to_clipboard;
use super::ConnectorsTab;
use super::data::{self, flag, text};
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;

/// When a waiting sign-in counts as finished, read from `/api/readConnectors`.
#[derive(Clone)]
pub(super) enum DoneWhen {
    ComposioSignedIn,
    ComposioApp(String),
    /// The computer's Executor CLI is signed in (`executor` in the read, not a workspace's).
    ExecutorCli,
    Cli(String),
}

/// A browser sign-in waiting to be finished.
#[derive(Clone)]
pub(super) struct PendingSignIn {
    pub(super) title: String,
    pub(super) url: String,
    pub(super) code: Option<String>,
    pub(super) done_when: DoneWhen,
}

impl PendingSignIn {
    fn finished(&self, workspace: &Value) -> bool {
        let app_done = |part: &str, reference: &str, key: &str| {
            data::list(workspace.get(part).unwrap_or(&Value::Null), "apps")
                .iter()
                .any(|app| {
                    (text(app, "slug") == reference || text(app, "id") == reference)
                        && flag(app, key)
                })
        };
        match &self.done_when {
            DoneWhen::ComposioSignedIn => workspace
                .get("composio")
                .is_some_and(|composio| flag(composio, "signedIn")),
            DoneWhen::ComposioApp(slug) => app_done("composio", slug, "connected"),
            DoneWhen::ExecutorCli => false,
            DoneWhen::Cli(tool) => workspace
                .get("cli")
                .and_then(|cli| cli.get(tool))
                .is_some_and(|state| flag(state, "signedIn")),
        }
    }
}

impl ConnectorsTab {
    /// Starts showing a sign-in: copies its code first, then opens its page.
    pub(super) fn begin_sign_in(
        &mut self,
        workspace: &str,
        section: &str,
        pending: PendingSignIn,
        cx: &mut Context<Self>,
    ) {
        if let Some(code) = pending.code.clone() {
            store_copy_to_clipboard(&self.store, code, cx);
        }
        self.open_url(&pending.url, cx);
        self.pending
            .insert(data::busy_key(workspace, section), pending);
        cx.notify();
    }

    /// Drops the sign-ins the last read shows finished.
    pub(super) fn drop_finished_sign_ins(&mut self) {
        let workspaces = data::workspaces(self.connectors.as_ref());
        let executor_signed_in = self
            .connectors
            .as_ref()
            .and_then(|connectors| connectors.get("executor"))
            .is_some_and(|executor| flag(executor, "signedIn"));
        self.pending.retain(|key, pending| {
            if matches!(pending.done_when, DoneWhen::ExecutorCli) {
                return !executor_signed_in;
            }
            let workspace_id = key.split(':').next().unwrap_or_default();
            !workspaces
                .iter()
                .find(|workspace| text(workspace, "workspaceId") == workspace_id)
                .is_some_and(|workspace| pending.finished(workspace))
        });
    }

    /// The panel for a section's waiting sign-in, if it has one.
    pub(super) fn sign_in_panel(
        &self,
        p: &SettingsPalette,
        workspace: &str,
        section: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = data::busy_key(workspace, section);
        let pending = self.pending.get(&key)?.clone();
        let element_id = |name: &str| format!("connectors-{key}-signin-{name}");
        let hint = match &pending.code {
            Some(_) => {
                "The code is copied. Paste it on the page that opened in your browser, then approve. This page updates when you're done."
            }
            None => {
                "Finish in the page that opened in your browser. This page updates when you're done."
            }
        };
        let mut buttons = h_flex().gap(px(8.0)).flex_wrap();
        if let Some(code) = pending.code.clone() {
            let store = self.store.clone();
            buttons = buttons.child(settings_button(
                p,
                element_id("copy-code"),
                "Copy code",
                Some("modals/settings/copy.svg"),
                ButtonVariant::Outline,
                false,
                None,
                move |_page: &mut Self, _window, cx| {
                    store_copy_to_clipboard(&store, code.clone(), cx)
                },
                cx,
            ));
        }
        let reopen = pending.clone();
        let reopen_section = section.to_string();
        let reopen_workspace = workspace.to_string();
        buttons = buttons.child(settings_button(
            p,
            element_id("open"),
            "Open page again",
            None,
            ButtonVariant::Outline,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.begin_sign_in(&reopen_workspace, &reopen_section, reopen.clone(), cx)
            },
            cx,
        ));
        let link = pending.url.clone();
        let store = self.store.clone();
        buttons = buttons.child(settings_button(
            p,
            element_id("copy-link"),
            "Copy link",
            Some("modals/settings/copy.svg"),
            ButtonVariant::Outline,
            false,
            None,
            move |_page: &mut Self, _window, cx| store_copy_to_clipboard(&store, link.clone(), cx),
            cx,
        ));
        let close_key = key.clone();
        buttons = buttons.child(settings_button(
            p,
            element_id("close"),
            "Close",
            None,
            ButtonVariant::Ghost,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.pending.remove(&close_key);
                cx.notify();
            },
            cx,
        ));
        let mut panel = v_flex()
            .w_full()
            .gap(px(10.0))
            .px(px(ROW_PADDING_X))
            .py(px(ROW_PADDING_Y + 4.0))
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(p.foreground))
                    .child(pending.title.clone()),
            );
        if let Some(code) = pending.code.as_ref() {
            panel = panel.child(
                div()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(28.0))
                    .line_height(px(34.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(p.foreground))
                    .child(code.clone()),
            );
        }
        panel = panel
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .child(hint),
            )
            .child(buttons);
        Some(panel.into_any_element())
    }
}
