//! Native GPUI Install Hooks prompt, the desktop twin of the React
//! `AgentHooksRequiredModal` in packages/core-ui/agent-hooks-required-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:AgentHooks 2026-10-06 DECISION:
//! User: the native modal matches its deleted React twin 1 to 1 (layout, copy, colors, radii, spacing, states, keyboard behaviour and bridge messages in both appearances), with one approved exception: for Pi, OMP and Amp the description says "It installs in seconds and takes effect the next time {agent} starts." instead of "Just approve it when {agent} asks.", because those agents load Ghostex's extension file on their next start and never ask. The user chose this over the 1-to-1 rule. Supersedes the 2026-09-15 rule that the copy stays identical for every agent. The hero tile, benefit chips and top glow take the agent's logo color; white-logo agents fall back to the foreground so the tint follows the theme.
//! CDXC:AgentHooks 2026-09-15 DECISION:
//! User: the missing-hooks prompt is a friendly, short invitation that shows the agent's logo, not a technical notice. The agent's brand color tints the hero tile and benefit icons; copy names the agent and stays to one sentence per idea.
//! SEE-ALSO: packages/core-ui/agent-hooks-required-modal.tsx (deleted 2026-10-01) and packages/core-ui/agent-hook-benefits.tsx (deleted 2026-10-01) (the React twin), the `.agent-hooks-required-*` rules in packages/core-ui/styles/modals.css (deleted 2026-11-06) and modals-light.css, packages/core-ui/agent-logos.ts (deleted 2026-10-01) and packages/shared/sidebar-agents.ts (deleted 2026-10-01) (the agent id, logo file and brand color table mirrored below), apps/desktop/src/app/agent_hooks_required_modal_lifecycle.rs (open, close, sidebar bridge).
use super::native_modal_kit::*;
use gpui::StyledImage as _;
use gpui::{
    AnyElement, App, BoxShadow, Context, FocusHandle, FontWeight, IntoElement, KeyDownEvent,
    ObjectFit, ParentElement as _, Render, Rgba, Styled as _, Window, div, img, linear_color_stop,
    linear_gradient, point, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// The React dialog opens on the Rename Session width (`APP_MODAL_HOST_RENAME_SESSION_WINDOW_WIDTH`).
pub(crate) const AGENT_HOOKS_REQUIRED_MODAL_WIDTH: f32 = 570.0;
/// First-frame height only (`APP_MODAL_HOST_AGENT_HOOKS_REQUIRED_WINDOW_HEIGHT`); the window is resized to the measured layout on the first prepaint.
pub(crate) const AGENT_HOOKS_REQUIRED_MODAL_INITIAL_HEIGHT: f32 = 560.0;

const ICON_CIRCLE_CHECK: &str = "modals/install-hooks/circle-check.svg";
const ICON_PENCIL: &str = "modals/install-hooks/pencil.svg";
const ICON_MESSAGE_CIRCLE: &str = "modals/install-hooks/message-circle.svg";
const ICON_HISTORY: &str = "modals/install-hooks/history.svg";
const ICON_INFO_CIRCLE: &str = "modals/install-hooks/info-circle.svg";
const ICON_PLUG_CONNECTED: &str = "modals/install-hooks/plug-connected.svg";

const NOT_NOW: &str = "Not now";
const INSTALL_HOOKS: &str = "Install hooks";

/// `AGENT_HOOK_BENEFITS` in packages/core-ui/agent-hook-benefits.tsx (deleted 2026-10-01), in order.
const BENEFITS: [(&str, &str, &str); 4] = [
    (
        ICON_CIRCLE_CHECK,
        "Live status and alerts",
        "See agent progress and get notified when you’re needed.",
    ),
    (
        ICON_PENCIL,
        "Automatic session names",
        "Find sessions easily with names based on your first message.",
    ),
    (
        ICON_MESSAGE_CIRCLE,
        "Chat View",
        "Read and reply in chat, with the terminal one click away.",
    ),
    (
        ICON_HISTORY,
        "Resume your work",
        "Pick up the same conversation after sleep or a restart.",
    ),
];

/// The React description is capped at `40ch`: 322px of 13px system UI text in Chrome, which gpui lays out a few pixels wider, so 332px here breaks the same lines.
const DESCRIPTION_MAX_WIDTH: f32 = 332.0;
/// The React surface carries `radial-gradient(120% 70% at 50% -20%, brand 14%, transparent 60%)`.
/// gpui draws linear gradients only, so the glow is a vertical band: 7.3% brand at the top
/// edge (the radial's value there) fading out where the radial reaches transparent (22% of the
/// dialog height, 108px on the fitted window).
const GLOW_HEIGHT: f32 = 108.0;
const GLOW_TOP_ALPHA: f32 = 0.073;

/// One sidebar agent logo: the embedded SVG (served as `agent-icons/<file>.svg`)
/// and its `AGENT_LOGO_COLORS` entry. `neutral` marks the white logos whose
/// brand color is `#ffffff` or `#edecec`; `multicolor` is OMP, drawn as an
/// image instead of a mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentHooksLogo {
    pub(crate) asset: &'static str,
    pub(crate) color: u32,
    pub(crate) neutral: bool,
    pub(crate) multicolor: bool,
}

const fn logo(asset: &'static str, color: u32) -> AgentHooksLogo {
    AgentHooksLogo {
        asset,
        color,
        neutral: matches!(color, 0xffffff | 0xedecec),
        multicolor: false,
    }
}

/// `getSidebarAgentIconById` (packages/shared/sidebar-agents.ts (deleted 2026-10-01)) joined with
/// `AGENT_LOGO_COLORS` (packages/core-ui/agent-logos.ts (deleted 2026-10-01)): default sidebar agent
/// id, trimmed and lowercased, to its logo file and brand color.
pub(crate) fn agent_hooks_logo_for_agent_id(agent_id: &str) -> Option<AgentHooksLogo> {
    let agent_id = agent_id.trim().to_ascii_lowercase();
    Some(match agent_id.as_str() {
        "codex" => logo("agent-icons/codex.svg", 0xffffff),
        "claude" => logo("agent-icons/claude.svg", 0xd97757),
        "cursor" => logo("agent-icons/cursor-cli.svg", 0xedecec),
        "pi" => logo("agent-icons/pi.svg", 0xc8ff62),
        "opencode" => logo("agent-icons/opencode.svg", 0x6d96c0),
        "gemini" => logo("agent-icons/gemini.svg", 0x8b9aff),
        "copilot" => logo("agent-icons/copilot.svg", 0xffffff),
        "droid" => logo("agent-icons/factory-droid.svg", 0xff7a1a),
        "grok" => logo("agent-icons/grok-build.svg", 0xffffff),
        "antigravity" => logo("agent-icons/antigravity-cli.svg", 0x749bff),
        "amp" => logo("agent-icons/amp-cli.svg", 0xffffff),
        "hermes-agent" => logo("agent-icons/hermes-agent.svg", 0xf3c46b),
        "rovodev" => logo("agent-icons/rovo-dev.svg", 0x4fc3a1),
        "codebuddy" => logo("agent-icons/codebuddy.svg", 0x72d6ff),
        "qoder" => logo("agent-icons/qoder.svg", 0xa991ff),
        "kiro" => logo("agent-icons/kiro.svg", 0xa6e3ff),
        "omp" => AgentHooksLogo {
            multicolor: true,
            ..logo("agent-icons/omp.svg", 0xa663ed)
        },
        "kimi" => logo("agent-icons/kimi.svg", 0x7b6cf6),
        "openclaude" => logo("agent-icons/openclaude.svg", 0xf0a68a),
        "command-code" => logo("agent-icons/command-code.svg", 0x22d3ee),
        "devin" => logo("agent-icons/devin.svg", 0x3ea6ff),
        "empryo" => logo("agent-icons/empryo.svg", 0x1fa31d),
        "mastra" => logo("agent-icons/mastra.svg", 0xffffff),
        "zcode" => logo("agent-icons/zcode.svg", 0xffffff),
        _ => return None,
    })
}

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum AgentHooksRequiredModalCommand {
    /// "Install hooks": `confirmAgentHookLaunch` with `installHooks: true`.
    Install,
    /// "Not now": `confirmAgentHookLaunch` with `installHooks: false`.
    Skip,
    /// Escape: the React `onClose`, which only closes the dialog.
    Close,
}

pub(crate) type AgentHooksRequiredModalHost = Rc<dyn Fn(AgentHooksRequiredModalCommand, &mut App)>;

pub(crate) struct AgentHooksRequiredModalConfig {
    pub(crate) agent_name: String,
    /// Default sidebar agent id whose hooks are missing; picks the logo and brand tint.
    pub(crate) hook_agent_id: String,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiAgentHooksRequiredModalWindow {
    host: AgentHooksRequiredModalHost,
    palette: ModalPalette,
    agent_name: String,
    /// Pi, OMP and Amp load the hook as an extension on their next start rather than asking.
    loads_on_next_start: bool,
    logo: Option<AgentHooksLogo>,
    /// `--agent-hooks-brand`: the logo color, or the foreground for white logos and unknown agents.
    brand: Rgba,
    /// The mask color of the logo itself (`getBrandAgentLogoStyle`): white logos stay white in dark and take the foreground in light.
    logo_color: Rgba,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiAgentHooksRequiredModalWindow {
    pub(crate) fn new(
        config: AgentHooksRequiredModalConfig,
        host: AgentHooksRequiredModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let p = config.palette;
        let logo = agent_hooks_logo_for_agent_id(&config.hook_agent_id);
        let brand = match logo {
            Some(logo) if !logo.neutral => rgb(logo.color),
            _ => p.foreground,
        };
        let logo_color = match logo {
            Some(logo) if !logo.neutral => rgb(logo.color),
            // modals-light.css (deleted 2026-11-06): `--ghostex-codex-logo`, `--ghostex-light-icon-color` and
            // `--ghostex-zcode-logo` resolve to the foreground inside the light modal.
            Some(_) if p.light => p.foreground,
            Some(logo) => rgb(logo.color),
            None => brand,
        };
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: p,
            agent_name: config.agent_name,
            loads_on_next_start: matches!(config.hook_agent_id.as_str(), "pi" | "omp" | "amp"),
            logo,
            brand,
            logo_color,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: AgentHooksRequiredModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        self.close_window_and_send(AgentHooksRequiredModalCommand::Close, window, cx);
        cx.stop_propagation();
    }

    fn render_logo(&self) -> AnyElement {
        match self.logo {
            Some(logo) if logo.multicolor => img(logo.asset)
                .size(px(34.0))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            Some(logo) => modal_icon(logo.asset, 34.0, self.logo_color).into_any_element(),
            None => modal_icon(ICON_PLUG_CONNECTED, 30.0, self.brand).into_any_element(),
        }
    }

    /// `.agent-hooks-required-header`: the surface glow, the 64px logo tile, the
    /// 18px title and the centered description.
    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let brand = self.brand;
        // modals-light.css (deleted 2026-11-06) widens the ring to 9% brand on light themes.
        let ring_alpha = if p.light { 0.09 } else { 0.07 };
        let glow = div()
            .absolute()
            .top(px(-MODAL_WINDOW_PADDING))
            .left(px(-MODAL_WINDOW_PADDING))
            .w(px(AGENT_HOOKS_REQUIRED_MODAL_WIDTH))
            .h(px(GLOW_HEIGHT))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(hsla(rgba_of(brand, GLOW_TOP_ALPHA)), 0.0),
                linear_color_stop(hsla(rgba_of(brand, 0.0)), 1.0),
            ));
        let tile = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(64.0))
            .mt(px(4.0))
            .mb(px(10.0))
            .rounded(px(18.0))
            .border_1()
            .border_color(hsla(css_mix(brand, 0.28, p.hairline)))
            .bg(hsla(css_mix(brand, 0.12, p.raised)))
            .shadow(vec![BoxShadow {
                color: hsla(rgba_of(brand, ring_alpha)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(6.0),
                inset: false,
            }])
            .child(self.render_logo());
        v_flex()
            .relative()
            .w_full()
            .items_center()
            .gap(px(6.0))
            .text_center()
            .child(glow)
            .child(tile)
            .child(
                div()
                    .text_size(px(18.0))
                    .line_height(px(23.4))
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("Connect Ghostex to {}", self.agent_name)),
            )
            .child(
                div()
                    .max_w(px(DESCRIPTION_MAX_WIDTH))
                    .text_size(px(13.0))
                    .line_height(px(20.15))
                    .text_color(hsla(p.muted))
                    .child(if self.loads_on_next_start {
                        format!(
                            "A small helper called a hook lets Ghostex follow what {0} is doing. It installs in seconds and takes effect the next time {0} starts.",
                            self.agent_name
                        )
                    } else {
                        format!(
                            "A small helper called a hook lets Ghostex follow what {0} is doing. It installs in seconds. Just approve it when {0} asks.",
                            self.agent_name
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_benefit(
        &self,
        icon_path: &'static str,
        title: &'static str,
        text: &'static str,
    ) -> AnyElement {
        let p = self.palette;
        h_flex()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .items_start()
            .gap(px(10.0))
            .pt(px(10.0))
            .pr(px(12.0))
            .pb(px(11.0))
            .pl(px(10.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(28.0))
                    .rounded(px(8.0))
                    .bg(hsla(rgba_of(self.brand, 0.14)))
                    .child(modal_icon(icon_path, 16.0, self.brand)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(16.9))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.4))
                            .text_color(hsla(p.muted))
                            .child(text),
                    ),
            )
            .into_any_element()
    }

    /// `.agent-hooks-required-benefits`: a 2x2 grid, 8px apart, both cells of a row as tall as the taller one.
    fn render_benefits(&self) -> AnyElement {
        let rows = BENEFITS.chunks(2).map(|pair| {
            h_flex().w_full().items_stretch().gap(px(8.0)).children(
                pair.iter()
                    .map(|(icon, title, text)| self.render_benefit(icon, title, text)),
            )
        });
        v_flex()
            .w_full()
            .gap(px(8.0))
            .children(rows)
            .into_any_element()
    }

    /// `.agent-hooks-required-note`: centered muted line with the info glyph, pulled 6px into both neighbours.
    fn render_note(&self) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .mt(px(-6.0))
            .mb(px(-6.0))
            .text_size(px(12.0))
            .line_height(px(17.4))
            .text_color(hsla(p.muted))
            .child(
                div()
                    .flex_shrink_0()
                    .child(modal_icon(ICON_INFO_CIRCLE, 14.0, p.muted)),
            )
            .child(div().min_w_0().child(format!(
                "You can skip for now. These features stay off for {} until its hooks are installed.",
                self.agent_name
            )))
            .into_any_element()
    }

    fn render_body(&self) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(MODAL_SECTION_GAP))
            .child(self.render_benefits())
            .child(self.render_note())
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        modal_footer(vec![
            modal_action_button(
                &p,
                "agent-hooks-required-skip",
                NOT_NOW,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(AgentHooksRequiredModalCommand::Skip, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "agent-hooks-required-install",
                INSTALL_HOOKS,
                None,
                ModalButtonTone::Primary,
                false,
                |this, window, cx| {
                    this.close_window_and_send(AgentHooksRequiredModalCommand::Install, window, cx)
                },
                cx,
            ),
        ])
    }
}

impl Render for GpuiAgentHooksRequiredModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![self.render_header(), self.render_body()];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-agent-hooks-required-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiAgentHooksRequiredModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(AgentHooksRequiredModalCommand::Close, window, cx);
    }
}
