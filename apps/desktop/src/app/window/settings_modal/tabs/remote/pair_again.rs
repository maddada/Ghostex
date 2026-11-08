//! Settings > Remote's "Pair again" line: while any phone or computer from before 11.0.1 still
//! waits to be paired again (`waiting` of `/api/securityUpgradeNotice`), a line at the top of the
//! page says how many and reopens the security notice, whose QR and Pair buttons do the pairing.
//!
//! CDXC:Security 2026-11-08 DECISION: User (Q1729): Remind me later brings the notice back tomorrow, at most twice more, "with a 'Pair again' line kept in Settings > Remote until done".
//! SEE-ALSO: server/src/security_upgrade.rs, apps/desktop/src/app/security_upgrade_modal_lifecycle.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::store::post_store_message;
use super::RemoteTab;
use super::style::*;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::h_flex;
use serde_json::{Value, json};

const ICON_SHIELD: &str = "modals/settings/shield-check.svg";

/// "1 phone or computer still uses…", "3 phones and computers still use…".
pub(super) fn pair_again_text(waiting: usize) -> String {
    if waiting == 1 {
        "1 phone or computer still uses the old pairing and can't connect until you pair it again."
            .to_string()
    } else {
        format!(
            "{waiting} phones and computers still use the old pairing and can't connect until you pair them again."
        )
    }
}

/// How many rows still wait, from a `/api/securityUpgradeNotice` answer.
pub(super) fn waiting_of(answer: &Value) -> usize {
    answer["waiting"].as_u64().unwrap_or(0) as usize
}

pub(super) fn pair_again_line(
    t: &RemoteTokens,
    waiting: usize,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let p = t.p;
    h_flex()
        .w_full()
        .items_center()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .bg(hsla(p.raised))
        .child(modal_icon(ICON_SHIELD, 18.0, t.foreground))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.0))
                .line_height(px(19.5))
                .text_color(hsla(t.foreground))
                .child(pair_again_text(waiting)),
        )
        .child(compact_button(
            &p,
            "remote-pair-again",
            "Pair again",
            None,
            Look::Filled,
            28.0,
            14.0,
            None,
            false,
            None,
            |tab: &mut RemoteTab, _window, cx| {
                post_store_message(
                    &tab.store,
                    json!({ "type": "openSecurityUpgradeNotice" }),
                    cx,
                );
            },
            cx,
        ))
        .into_any_element()
}
