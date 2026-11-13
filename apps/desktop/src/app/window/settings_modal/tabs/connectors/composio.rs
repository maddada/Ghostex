//! The Composio section of a workspace: its sign-in, and the apps connected or suggested there.
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_list_item, settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::ConnectorsTab;
use super::data::{busy_key, flag, list, text};
use super::widgets::{detail, note_row};
use gpui::{AnyElement, Context, Div, IntoElement, ParentElement as _, Styled as _, px};
use gpui_component::h_flex;
use serde_json::Value;

impl ConnectorsTab {
    pub(super) fn composio_section(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        title: String,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let composio = workspace.get("composio")?;
        let id = text(workspace, "workspaceId");
        let signed_in = flag(composio, "signedIn");
        let offline = !self.rpc_available(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        if signed_in {
            for app in list(composio, "apps") {
                let slug = text(&app, "slug");
                let connected = flag(&app, "connected");
                let busy = self.is_busy(&id, &format!("composio-connect-{slug}"));
                let control = (!connected).then(|| {
                    let (workspace, slug) = (id.clone(), slug.clone());
                    settings_button(
                        p,
                        format!("connectors-{id}-composio-connect-{slug}"),
                        if busy { "Opening…" } else { "Connect" },
                        None,
                        ButtonVariant::Outline,
                        offline || busy,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.composio_connect(&workspace, &slug, cx)
                        },
                        cx,
                    )
                });
                rows.push(settings_list_item(
                    p,
                    Some(if connected {
                        ListItemStatus::Success
                    } else {
                        ListItemStatus::Neutral
                    }),
                    None,
                    text(&app, "name"),
                    Some(detail(if connected {
                        "Connected"
                    } else {
                        "Not connected"
                    })),
                    control,
                ));
            }
        } else {
            let busy = self.is_busy(&id, "composio-sign-in");
            let workspace = id.clone();
            // A saved sign-in that can't be read anymore (server/src/connectors/store.rs, ReadError).
            let unreadable = flag(composio, "unreadable");
            rows.push(settings_list_item(
                p,
                Some(if unreadable {
                    ListItemStatus::Warning
                } else {
                    ListItemStatus::Neutral
                }),
                None,
                if unreadable {
                    "Needs a new sign-in"
                } else {
                    "Not signed in"
                },
                Some(detail(if unreadable {
                    "This sign-in can't be read anymore. Sign in again."
                } else {
                    "Sign this workspace in to Composio once in your browser, then connect its apps."
                })),
                Some(settings_button(
                    p,
                    format!("connectors-{id}-composio-sign-in"),
                    match (busy, unreadable) {
                        (true, _) => "Opening…",
                        (false, true) => "Sign in again",
                        (false, false) => "Sign in",
                    },
                    None,
                    ButtonVariant::Outline,
                    offline || busy,
                    None,
                    move |page: &mut Self, _window, cx| page.composio_sign_in(&workspace, cx),
                    cx,
                )),
            ));
        }
        if let Some(note) = self.notes.get(&busy_key(&id, "composio")) {
            rows.push(note_row(p, note));
        }
        rows.extend(self.sign_in_panel(p, &id, "composio", cx));
        let actions = signed_in.then(|| {
            let refreshing = self.is_busy(&id, "composio-refresh");
            let signing_out = self.is_busy(&id, "composio-sign-out");
            let (refresh_workspace, sign_out_workspace) = (id.clone(), id.clone());
            h_flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_button(
                    p,
                    format!("connectors-{id}-composio-refresh"),
                    if refreshing {
                        "Checking…"
                    } else {
                        "Check apps"
                    },
                    Some("modals/settings/refresh.svg"),
                    ButtonVariant::Outline,
                    offline || refreshing,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.composio_refresh(&refresh_workspace, cx)
                    },
                    cx,
                ))
                .child(settings_button(
                    p,
                    format!("connectors-{id}-composio-sign-out"),
                    "Sign out",
                    None,
                    ButtonVariant::Outline,
                    offline || signing_out,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.composio_sign_out(&sign_out_workspace, cx)
                    },
                    cx,
                ))
                .into_any_element()
        });
        settings_section(p, title, None, actions, rows)
    }
}
