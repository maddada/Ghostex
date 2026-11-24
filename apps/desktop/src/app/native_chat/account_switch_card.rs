/*!
The account-switch card: the modal-style card centered over the whole chat pane
while a Claude or Codex account switch runs. A port of `AccountSwitchCard`
(packages/core-ui/accounts/account-switch-card.tsx) and its
account-switch-card.css, drawn from the core's `accountSwitchCard` projection
(packages/gx-chat-core/src/menus/native_accounts.rs). The copy, steps and
usage levels come from `menus/accounts_presentation.rs` in the core.

CDXC:AgentProviders 2026-09-25 DECISION:
User: drop the bars under the usage tiles, stop the tile name (Fable) looking the same as its reset time (3d 4h), and show the agent icon once, left of the title, not on both account rows (docs/2026-09-24/account-switch-card-roomy/01-no-bars.html). 2026-09-29: "reduce this one's size when the chat view is large by 30%", then "make it bigger by 15%": in a pane 760px or wider the card is the former 576px card at 70%, then 15% larger, so about 464px with its text scaled to match; narrower panes keep the 430px card. This supersedes the 2026-09-25 growth to 516px / 576px. Each tile shows a semibold name, a smaller muted reset line with a clock, and a large percent; the tile at 100% keeps its stronger outline. This supersedes the 2026-09-23 tile-edge bars and "smaller" card; the rest of that decision (neutral ink, accent for progress, numbered steps with the moving line) stands, as does the 2026-09-16 backdrop that blocks the pointer until the switch finishes.
*/

use super::{
    account_switch_card_view::{
        AccountSwitchCardHooks, AccountSwitchCardLook, render_account_switch_overlay,
    },
    appearance::ChatAppearance,
    new_session_welcome::brand_logo_color,
    state::NativeChatView,
    working_spark::css_ease_in_out,
};
use crate::app::helpers::ThrottledAnimationExt as _;
use gpui::{
    AnyElement, Context, Div, ElementId, IntoElement as _, StatefulInteractiveElement as _,
    Styled as _, relative,
};
use serde_json::{Value, json};
use std::time::Duration;

/// CDXC:AgentProviders 2026-10-01 WHY:
/// The moving line under the steps looked slow and choppy. At the shared thirty-frame indicator rate the bar still jumps about 11px a frame across a step, so it moves on every display frame, as the React card's CSS animation did; the card is up only for the few seconds a switch takes and blocks the chat meanwhile, so the extra frames cost nothing that lasts.
const SWEEP_FRAME_INTERVAL: Duration = Duration::ZERO;

impl NativeChatView {
    /// The overlay React rendered as `.gx-account-switch-overlay` while `accountStatus.visible` holds.
    pub(crate) fn render_account_switch_card(
        &self,
        appearance: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = &self.snapshot["accountSwitchCard"];
        if !card.is_object() {
            return None;
        }
        let provider = card["provider"].as_str().unwrap_or("claude");
        let fills = appearance.neutral_button_fills();
        let look = AccountSwitchCardLook {
            background: appearance.background,
            card_panel: appearance.card_panel,
            border: appearance.border,
            input_border: appearance.input_border,
            foreground: appearance.foreground,
            prose: appearance.prose,
            muted: appearance.muted,
            accent: appearance.accent,
            brand_color: brand_logo_color(provider, appearance),
            button_rest: fills.rest,
            button_hover: fills.hover,
            button_pressed: fills.pressed,
            light: appearance.light,
            window_glass: appearance.window_glass,
            scale: appearance.scale,
            font: appearance.font.clone(),
        };
        let pane_width = f32::from(self.bounds.get().size.width) / appearance.scale;
        let animate_sweep = |bar: Div, id: ElementId| {
            bar.with_throttled_animation(id, Duration::from_millis(1600), |bar, phase| {
                bar.left(relative(-0.4 + 1.4 * css_ease_in_out(phase)))
            })
            .frame_interval(SWEEP_FRAME_INTERVAL)
            .into_any_element()
        };
        let hooks = AccountSwitchCardHooks {
            on_close: Some(Box::new(|close| {
                close.on_click(cx.listener(|this, _, _, cx| {
                    this.invoke(
                        json!({"type":"accounts","request":{"operation":"dismissSwitch"}}),
                        cx,
                    )
                }))
            })),
            on_retry: Some(Box::new(|button, retry: Value| {
                let command = json!({"type":"accounts","request":{"operation":"select","accountId":retry["accountId"]}});
                button.on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
            })),
            animate_sweep: Some(&animate_sweep),
            animate_entrance: true,
        };
        Some(render_account_switch_overlay(card, pane_width, &look, hooks))
    }
}
