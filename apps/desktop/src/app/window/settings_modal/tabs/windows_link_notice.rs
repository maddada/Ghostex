//! The Windows Developer Mode notice at the top of Settings > Agents and Settings > Integrations,
//! shown only while the machine those pages install onto runs Windows (the picked remote machine,
//! or this computer when Ghostex runs on Windows). It folds to its warning icon in the same spot
//! and opens again from it; folded or open is one remembered state for both pages
//! (`windowsLinkNoticeCollapsed` in `settingsModalNavigation`, SettingsStore).
//!
//! CDXC:RemoteMachines 2026-10-27 DECISION:
//! User: "please add a notice about this for windows that can be collapsed to an icon on the integrations/agents page (when the machine we're installing integrations on is windows)". "This" is a Windows tester's report that their hooks stopped linking to their agents until they turned on Developer Mode: without it Windows lets only administrators create symbolic links, and Ghostex links hooks and skills into each agent with them (packages/agent-sync/src/fsx.rs, server/src/agent_skills.rs). This computer's notice also opens `ms-settings:developers`; a remote machine's can only say where the switch is. Since the user's "Fall back to a junction, restore on failure" (Q614, CDXC:AgentSync 2026-10-27 in fsx.rs) links work without it too, so the notice recommends Developer Mode instead of saying links fail.
use super::super::super::native_modal_kit::*;
use super::super::fields::{
    ButtonVariant, settings_button, settings_icon, settings_icon_button, tooltip_text,
};
use super::super::palette::SettingsPalette;
use super::super::store::{SettingsStore, post_store_message};
use gpui::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

const ICON_ALERT: &str = "modals/settings/alert-triangle.svg";
const ICON_FOLD: &str = "modals/settings/x.svg";
const AMBER_500: u32 = 0xf59e0b;
const AMBER_TEXT_DARK: u32 = 0xfbbf24;
const AMBER_TEXT_LIGHT: u32 = 0x92400e;

/// The notice's two lines: a title, then why and where the switch is. `remote_name` names a remote
/// Windows machine; `None` is this computer.
pub(crate) fn notice_lines(remote_name: Option<&str>) -> (String, String) {
    let title = match remote_name {
        Some(name) => format!("Windows Developer Mode is recommended on {name}"),
        None => "Windows Developer Mode is recommended".to_string(),
    };
    let place = if remote_name.is_some() { "there " } else { "" };
    (
        title,
        format!(
            "Without it Windows blocks symbolic links, so Ghostex links hooks and skills with junctions and file copies instead. Turn it on {place}in Windows Settings > System > For developers > Developer Mode."
        ),
    )
}

/// Whether a gxserver's `platform` (`windows`, `macos`, `linux`) is Windows.
pub(crate) fn is_windows(platform: Option<&str>) -> bool {
    platform.is_some_and(|platform| platform.eq_ignore_ascii_case("windows"))
}

/// The notice, open or folded to its icon. Callers show it only for a Windows machine.
pub(crate) fn render_windows_link_notice<V: 'static>(
    p: &SettingsPalette,
    store: &Entity<SettingsStore>,
    remote_name: Option<&str>,
    cx: &mut Context<V>,
) -> AnyElement {
    let amber = rgb(AMBER_500);
    let amber_text = rgb(if p.light {
        AMBER_TEXT_LIGHT
    } else {
        AMBER_TEXT_DARK
    });
    let (title, body) = notice_lines(remote_name);
    let toggle_store = store.clone();
    let toggle = move |_: &mut V, _: &mut gpui::Window, cx: &mut Context<V>| {
        toggle_store.update(cx, |store, cx| store.toggle_windows_link_notice(cx));
    };
    if store.read(cx).windows_link_notice_collapsed() {
        // Folded: the notice's own amber icon in the same spot, which opens it again.
        let open_store = store.clone();
        let hover = p.raised_hover;
        return h_flex()
            .w_full()
            .child(
                div()
                    .id("windows-link-notice-open")
                    .role(gpui::Role::Button)
                    .aria_label(title.clone())
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .tooltip(tooltip_text(format!("{title}. Click to read why.")))
                    .on_click(cx.listener(move |_, _, _window, cx| {
                        open_store.update(cx, |store, cx| store.toggle_windows_link_notice(cx));
                    }))
                    .child(settings_icon(ICON_ALERT, 16.0, amber_text)),
            )
            .into_any_element();
    }
    let mut actions = h_flex().flex_shrink_0().items_center().gap(px(6.0));
    // Only Ghostex on this Windows computer can open its Settings app.
    if remote_name.is_none() && cfg!(target_os = "windows") {
        let open_store = store.clone();
        actions = actions.child(settings_button(
            p,
            "windows-link-notice-settings",
            "Open Windows settings",
            None,
            ButtonVariant::Outline,
            false,
            None,
            move |_: &mut V, _window, cx| {
                post_store_message(
                    &open_store,
                    json!({ "type": "openWindowsDeveloperSettings" }),
                    cx,
                );
            },
            cx,
        ));
    }
    actions = actions.child(settings_icon_button(
        p,
        "windows-link-notice-fold",
        ICON_FOLD,
        14.0,
        28.0,
        ButtonVariant::Ghost,
        Some("Fold to an icon".into()),
        false,
        toggle,
        cx,
    ));
    h_flex()
        .id("windows-link-notice")
        .w_full()
        .items_start()
        .gap(px(10.0))
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(css_fade(amber, 0.4)))
        .bg(hsla(css_fade(amber, 0.08)))
        .child(
            div()
                .mt(px(2.0))
                .flex_shrink_0()
                .child(settings_icon(ICON_ALERT, 16.0, amber_text)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child(title),
                )
                .child(div().text_color(hsla(p.muted)).child(body)),
        )
        .child(actions)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_says_why_and_where_the_switch_is() {
        let (title, body) = notice_lines(None);
        assert_eq!(title, "Windows Developer Mode is recommended");
        assert_eq!(
            body,
            "Without it Windows blocks symbolic links, so Ghostex links hooks and skills with junctions and file copies instead. Turn it on in Windows Settings > System > For developers > Developer Mode."
        );
        let (title, body) = notice_lines(Some("Windows"));
        assert_eq!(title, "Windows Developer Mode is recommended on Windows");
        assert!(body.contains("Turn it on there in Windows Settings > System > For developers"));
    }

    #[test]
    fn only_a_windows_platform_shows_it() {
        assert!(is_windows(Some("windows")));
        assert!(!is_windows(Some("macos")));
        assert!(!is_windows(Some("linux")));
        assert!(!is_windows(None));
    }
}
