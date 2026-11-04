//! Panel 3, Workspace: the view toggles and the mock workspace window whose titlebar tabs follow
//! them (panels/workspace.tsx (deleted 2026-10-01), previews/workspace-window.tsx (deleted 2026-10-01), styles/workspace.css).
use super::GpuiOnboardingWindow;
use super::OnboardingCommand;
use super::agents::window_tab;
use super::interact;
use super::model::{VIEW_KEYS, ViewKey, views_patch};
use super::primitives::*;
use super::stage::*;
use super::welcome::foot_actions;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, div, linear_color_stop, linear_gradient,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WsTab {
    Agents,
    View(ViewKey),
}

pub(crate) struct WorkspaceState {
    pub(super) agent: &'static str,
    tab: WsTab,
    previewed: Option<WsTab>,
    recent: Vec<ViewKey>,
    /// The view shown in the mock window, and when it mounted (its fade and clocks).
    shown: Option<WsTab>,
    pub(super) shown_since: Instant,
    pub(super) browser_clicked_at: Option<Instant>,
    pub(super) drive_since: Instant,
    drive: bool,
    pub(super) docs_saved: bool,
}

impl WorkspaceState {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            agent: "claude",
            tab: WsTab::View(ViewKey::Browser),
            previewed: None,
            recent: vec![ViewKey::Docs, ViewKey::Browser],
            shown: None,
            shown_since: now,
            browser_clicked_at: None,
            drive_since: now,
            drive: false,
            docs_saved: false,
        }
    }
}

const VIEW_ROWS: [(ViewKey, &str, &str, &str, f32); 4] = [
    (
        ViewKey::Docs,
        "list",
        "Files",
        "Markdown, mockups, diagrams and annotations.",
        464.0,
    ),
    (
        ViewKey::Code,
        "code",
        "VS Code",
        "VS Code, built in: source, diffs and review.",
        532.0,
    ),
    (
        ViewKey::Kanban,
        "kanban",
        "Kanban",
        "Split work into cards and hand each one to an agent.",
        600.0,
    ),
    (
        ViewKey::Automate,
        "bolt",
        "Automate",
        "Run agents on a schedule, once or on repeat.",
        668.0,
    ),
];
const RECOMMENDED: [ViewKey; 2] = [ViewKey::Docs, ViewKey::Browser];
const TAB_ORDER: [ViewKey; 5] = [
    ViewKey::Code,
    ViewKey::Browser,
    ViewKey::Kanban,
    ViewKey::Automate,
    ViewKey::Docs,
];
pub(super) const DEMO_AGENTS: [(&str, &str, &str); 4] = [
    ("claude", "Claude Code", "Building feature..."),
    ("codex", "Codex", "Ready"),
    ("cursor", "Cursor Agent", "Ready"),
    ("other", "Other agents", "20+ agents"),
];
fn tab_meta(tab: WsTab) -> (&'static str, &'static str) {
    match tab {
        WsTab::Agents => ("users", "Agents"),
        WsTab::View(ViewKey::Code) => ("code", "VS Code"),
        WsTab::View(ViewKey::Browser) => ("target", "Browser"),
        WsTab::View(ViewKey::Kanban) => ("kanban", "Kanban"),
        WsTab::View(ViewKey::Automate) => ("bolt", "Automate"),
        WsTab::View(ViewKey::Docs) => ("list", "Files"),
    }
}

impl GpuiOnboardingWindow {
    /// The preview binary's way to open the mock window on one view (`None` for Agents).
    pub(super) fn preview_workspace(&mut self, view: Option<ViewKey>) {
        self.ws_show_tab(view.map_or(WsTab::Agents, WsTab::View));
    }

    fn views(&self) -> [bool; 5] {
        VIEW_KEYS.map(|key| self.settings.is_view_on(key))
    }

    fn ws_current(&self) -> WsTab {
        let views = self.views();
        let state = &self.workspace;
        if let Some(previewed) = state.previewed {
            return previewed;
        }
        match state.tab {
            WsTab::Agents => WsTab::Agents,
            WsTab::View(key) if views[key.index()] => state.tab,
            _ => state
                .recent
                .iter()
                .rev()
                .find(|key| views[key.index()])
                .map(|key| WsTab::View(*key))
                .unwrap_or(WsTab::Agents),
        }
    }

    fn ws_show_tab(&mut self, next: WsTab) {
        self.workspace.previewed = None;
        self.workspace.tab = next;
    }

    fn ws_set_view(&mut self, key: ViewKey, on: bool, cx: &mut Context<Self>) {
        self.update_settings(views_patch(&[(key, on)]), cx);
        if on {
            self.workspace.recent.retain(|item| *item != key);
            self.workspace.recent.push(key);
            self.workspace.tab = WsTab::View(key);
            if self.workspace.previewed != Some(WsTab::View(key)) {
                self.workspace.previewed = None;
            }
        }
    }

    fn ws_apply_recommended(&mut self, cx: &mut Context<Self>) {
        let missing: Vec<ViewKey> = RECOMMENDED
            .iter()
            .copied()
            .filter(|key| !self.settings.is_view_on(*key))
            .collect();
        if missing.is_empty() {
            return;
        }
        let changes: Vec<(ViewKey, bool)> = missing.iter().map(|key| (*key, true)).collect();
        self.update_settings(views_patch(&changes), cx);
        self.workspace.recent.retain(|item| !missing.contains(item));
        self.workspace.recent.extend(missing.iter().copied());
        if let Some(last) = missing.last() {
            self.ws_show_tab(WsTab::View(*last));
        }
    }

    /// CDXC:Onboarding 2026-10-28 WHY:
    /// The switch followed only the installed status, which arrives after the install finishes, so
    /// the first click looked dead, a second click uninstalled what the first had just installed,
    /// and the two app toasts contradicted each other. The switch now moves at once, says it is
    /// adding or removing the skill, ignores clicks until the status confirms it, and reports the
    /// outcome in its own toast.
    fn ws_toggle_browser_skill(&mut self, cx: &mut Context<Self>) {
        if !self.settings.is_view_on(ViewKey::Browser) || self.browser_skill_busy() {
            return;
        }
        let target = !self.browser_skill_installed();
        self.browser_skill_pending = Some(super::BrowserSkillPending {
            target,
            deadline: Instant::now() + std::time::Duration::from_secs(120),
        });
        self.send(
            if target {
                OnboardingCommand::InstallBrowserSkill
            } else {
                OnboardingCommand::UninstallBrowserSkill
            },
            cx,
        );
        self.ws_show_tab(WsTab::View(ViewKey::Browser));
        cx.notify();
    }

    pub(super) fn render_workspace(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let views = self.views();
        let browser_on = views[ViewKey::Browser.index()];
        let drive = browser_on && self.browser_skill_shown_on();
        let skill_busy = self.browser_skill_busy();
        if drive != self.workspace.drive {
            self.workspace.drive = drive;
            self.workspace.drive_since = now;
        }
        let recommended_on = RECOMMENDED.iter().all(|key| views[key.index()]);
        let current = self.ws_current();
        if self.workspace.shown != Some(current) {
            self.workspace.shown = Some(current);
            self.workspace.shown_since = now;
            self.workspace.docs_saved = false;
            self.workspace.browser_clicked_at = None;
        }
        let mut out = Vec::new();
        out.push(eyebrow(s, 60.0, 118.0, None, "Workspace").into_any_element());
        out.push(
            heading(
                s,
                60.0,
                144.0,
                640.0,
                46.0,
                "Choose what lives next",
                Some("to your agents."),
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 60.0, 250.0, 632.0, 16.0, false)
                .child("Start lean; anything you hide stays available in Settings.")
                .into_any_element(),
        );
        // `.rec-chip { transition: border-color 0.2s }`; `:hover` is declared after `.on` and wins.
        let chip_border = interact::tween_color(
            "ws-rec-chip",
            "border",
            if interact::hovered("ws-rec-chip") {
                white(0.26)
            } else if recommended_on {
                rgba(90, 130, 255, 0.4)
            } else {
                white(0.1)
            },
            200,
        );
        let chip = abs(s, 60.0, 290.0, None, Some(38.0))
            .id("ws-rec-chip")
            .flex()
            .items_center()
            .gap(s.px(12.0))
            .px(s.px(20.0))
            .whitespace_nowrap()
            .rounded(s.px(20.0))
            .border_1()
            .border_color(chip_border)
            .bg(rgba(12, 14, 20, 0.6))
            .cursor_pointer()
            .child(icon(
                s,
                "checkCircle",
                18.0,
                1.6,
                if recommended_on {
                    hex(0x6d93ff)
                } else {
                    hex(0x2f4fae)
                },
            ))
            .child(sans(s, 15.0, 400.0, hex(0xe8ecf4)).child("Recommended · Browser + Files"));
        out.push(
            self.control(
                s,
                chip,
                "ws-rec-chip",
                interact::Ring::new(20.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.ws_apply_recommended(cx),
            )
            .into_any_element(),
        );
        let browser_thumb = self.thumb("ws-browser", browser_on, now);
        let drive_thumb = self.thumb("ws-drive", drive, now);
        let browser_toggle = self.control(
            s,
            toggle(
                s,
                "ws-toggle-browser",
                browser_on,
                browser_thumb,
                ToggleSize::Lg,
                false,
            ),
            "ws-toggle-browser",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| {
                cx.stop_propagation();
                this.ws_set_view(ViewKey::Browser, !browser_on, cx);
            },
        );
        let browser_row = self.control(
            s,
            view_row_inner(s, "ws-row-browser", 55.0)
                .child(icon(s, "target", 22.0, 1.6, hex(0xd6dbe5)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 17.0, 500.0).child("Browser"))
                        .child(
                            ss(s, 12.5)
                                .child("Preview and inspect the app your agent is building."),
                        ),
                )
                .child(browser_toggle),
            "ws-row-browser",
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                this.workspace.previewed = Some(WsTab::View(ViewKey::Browser));
                cx.notify();
            },
        );
        let skill_toggle = toggle(
            s,
            "ws-toggle-skill",
            drive,
            drive_thumb,
            ToggleSize::Lg,
            !browser_on,
        );
        // A disabled switch is out of the tab order.
        let skill_toggle = if browser_on {
            self.control(
                s,
                skill_toggle,
                "ws-toggle-skill",
                interact::Ring::new(17.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| {
                    cx.stop_propagation();
                    this.ws_toggle_browser_skill(cx);
                },
            )
        } else {
            skill_toggle
        };
        // `.vsub.off { opacity: 0.35 }` with `transition: opacity 0.25s`.
        let skill_opacity = interact::tween_value(
            "ws-row-skill",
            "opacity",
            if browser_on { 1.0 } else { 0.35 },
            250,
        );
        let skill_row = self.control(
            s,
            div()
                .id("ws-row-skill")
                .relative()
                .flex()
                .items_center()
                .gap(s.px(14.0))
                .h(s.px(56.0))
                .pl(s.px(34.0))
                .pr(s.px(20.0))
                .border_t_1()
                .border_color(white(0.05))
                .opacity(skill_opacity)
                .when(browser_on, |this| this.cursor_pointer())
                .child(div().ml(s.px(-1.0)).mr(s.px(16.0)).child(icon(
                    s,
                    "wrench",
                    16.0,
                    1.6,
                    hex(0xaab2c1),
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(s.px(3.0))
                        .child(
                            sans(s, 14.0, 500.0, hex(0xe2e6ee))
                                .child("Give agents the browser skill"),
                        )
                        .child(sans(s, 12.0, 400.0, hex(0x9ea7b6)).child(
                            match (skill_busy, drive) {
                                (true, true) => "Adding the browser skill for your agents…",
                                (true, false) => "Removing the browser skill…",
                                _ => "Agents can open, click, type and screenshot pages in this browser.",
                            },
                        )),
                )
                .child(skill_toggle),
            "ws-row-skill",
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                this.workspace.previewed = Some(WsTab::View(ViewKey::Browser));
                cx.notify();
            },
        );
        out.push(
            glass(s)
                .absolute()
                .left(s.px(60.0))
                .top(s.px(340.0))
                .w(s.px(632.0))
                .h(s.px(112.0))
                .overflow_hidden()
                .child(browser_row)
                .child(skill_row)
                .into_any_element(),
        );
        for (key, icon_name, title, detail, top) in VIEW_ROWS {
            let on = views[key.index()];
            let thumb = self.thumb(&format!("ws-{title}"), on, now);
            let row_key = format!("ws-row-{title}");
            let toggle_key = format!("ws-toggle-{title}");
            let row_toggle = self.control(
                s,
                toggle(s, toggle_key.clone(), on, thumb, ToggleSize::Lg, false),
                toggle_key,
                interact::Ring::new(17.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| {
                    cx.stop_propagation();
                    this.ws_set_view(key, !on, cx);
                },
            );
            // `.vrow { transition: border-color 0.25s }` and its `:hover` border.
            let border = interact::hover_color(
                &row_key,
                "border",
                rgba(150, 165, 205, 0.12),
                rgba(180, 195, 235, 0.22),
                250,
            );
            let row = glass(s)
                .absolute()
                .left(s.px(60.0))
                .top(s.px(top))
                .w(s.px(632.0))
                .h(s.px(58.0))
                .flex()
                .items_center()
                .gap(s.px(26.0))
                .pl(s.px(30.0))
                .pr(s.px(20.0))
                .id(SharedString::from(row_key.clone()))
                .border_color(border)
                .cursor_pointer()
                .child(icon(s, icon_name, 22.0, 1.6, hex(0xd6dbe5)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 17.0, 500.0).child(title))
                        .child(ss(s, 12.5).child(detail)),
                )
                .child(row_toggle);
            out.push(
                self.control(
                    s,
                    row,
                    row_key,
                    interact::Ring::new(14.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| {
                        this.workspace.previewed = Some(WsTab::View(key));
                        cx.notify();
                    },
                )
                .into_any_element(),
            );
        }
        out.push(
            foot_actions(s, 3)
                .child(self.control(
                    s,
                    cta(
                        s,
                        "workspace-next",
                        "Next",
                        true,
                        true,
                        false,
                        CtaSize::Foot,
                    ),
                    "workspace-next",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.go(4, cx),
                ))
                .into_any_element(),
        );
        out.extend(self.render_workspace_window(s, now, views, drive, current, cx));
        out
    }

    fn render_workspace_window(
        &self,
        s: S,
        now: Instant,
        views: [bool; 5],
        drive: bool,
        current: WsTab,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        out.push(
            glass(s)
                .absolute()
                .left(s.px(758.0))
                .top(s.px(116.0))
                .w(s.px(875.0))
                .h(s.px(720.0))
                .border_color(rgba(90, 125, 235, 0.25))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(rgba(10, 13, 24, 0.88), 0.0),
                    linear_color_stop(rgba(6, 8, 16, 0.9), 1.0),
                ))
                .into_any_element(),
        );
        let mut tabs = vec![WsTab::Agents];
        tabs.extend(
            TAB_ORDER
                .iter()
                .filter(|key| views[key.index()] || current == WsTab::View(**key))
                .map(|key| WsTab::View(*key)),
        );
        let tab_elements = tabs.into_iter().map(|tab| {
            let preview_only = matches!(tab, WsTab::View(key) if !views[key.index()]);
            let (icon_name, label) = tab_meta(tab);
            let on = current == tab && !preview_only;
            let id = SharedString::from(format!("ws-tab-{label}"));
            let element = if preview_only {
                div()
                    .id(id.clone())
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(s.px(7.0))
                    .h(s.px(32.0))
                    .px(s.px(10.0))
                    .rounded(s.px(8.0))
                    .border_1()
                    .border_dashed()
                    .border_color(rgba(110, 150, 255, 0.6))
                    .bg(rgba(60, 90, 200, 0.08))
                    .font_family(super::fonts::dm_sans())
                    .text_size(s.px(12.5))
                    .text_color(hex(0xaec2ff))
                    .whitespace_nowrap()
                    .child(icon(s, icon_name, 14.0, 1.6, hex(0xaec2ff)))
                    .child(label)
                    .child(
                        div()
                            .absolute()
                            .top(s.px(32.0 + 3.0 - 1.0))
                            .left(s.px(-40.0))
                            .right(s.px(-40.0))
                            .flex()
                            .justify_center()
                            .child(
                                sans(s, 11.0, 400.0, hex(0x8fabff))
                                    .line_height(s.px(14.0))
                                    .whitespace_nowrap()
                                    .child("↑ adds this tab"),
                            ),
                    )
            } else {
                window_tab(s, id.clone(), icon_name, label, on, 32.0, 12.5).when(
                    tab == WsTab::View(ViewKey::Browser) && drive,
                    |this| {
                        this.child(
                            div()
                                .h(s.px(18.0))
                                .px(s.px(6.0))
                                .flex()
                                .items_center()
                                .rounded(s.px(5.0))
                                .border_1()
                                .border_color(rgba(90, 130, 255, 0.35))
                                .bg(rgba(60, 90, 200, 0.14))
                                .text_size(s.px(10.0))
                                .text_color(hex(0x8fabff))
                                .child("skill on"),
                        )
                    },
                )
            };
            self.control(
                s,
                element,
                id,
                interact::Ring::new(8.0, if preview_only { 1.0 } else { 0.0 }),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| {
                    if this.ws_current() != tab {
                        this.ws_show_tab(tab);
                    }
                    cx.notify();
                },
            )
            .into_any_element()
        });
        let tab_elements: Vec<AnyElement> = tab_elements.collect();
        let menu_open = self.menu_open;
        // `button.more:hover` over the host's 120ms button transition.
        let more_bg = interact::hover_color(
            "ws-more",
            "bg",
            white(0.0),
            white(0.07),
            interact::BUTTON_MS,
        );
        let more = self.control(
            s,
            div()
                .id("ws-more")
                .relative()
                .flex()
                .items_center()
                .gap(s.px(3.0))
                .py(s.px(8.0))
                .px(s.px(6.0))
                .rounded(s.px(6.0))
                .bg(more_bg)
                .cursor_pointer()
                .children((0..3).map(|_| div().size(s.px(4.0)).rounded_full().bg(hex(0xb3bac7)))),
            "ws-more",
            interact::Ring::new(6.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                let same_press = this
                    .workspace_menu_dismissed
                    .take()
                    .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(400));
                if !this.menu_open && same_press {
                    return;
                }
                this.menu_open = !this.menu_open;
                this.workspace_menu_opened = Instant::now();
                cx.notify();
            },
        );
        // The menu follows the button in the page's DOM, so it is built (and tab-ordered) here.
        let menu = menu_open.then(|| self.render_workspace_menu(s, now, cx));
        out.push(
            abs(s, 758.0, 116.0, Some(875.0), Some(54.0))
                .flex()
                .items_center()
                .gap(s.px(12.0))
                .pl(s.px(24.0))
                .pr(s.px(16.0))
                .child(lights(s, false).mr(s.px(14.0)))
                .child(ghostex_logo(s, 28.0, false))
                .child(
                    sans(s, 14.0, 600.0, hex(0xeef1f7))
                        .ml(s.px(2.0))
                        .mr(s.px(14.0))
                        .child("my-app"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(s.px(4.0))
                        .children(tab_elements),
                )
                .child(more)
                .into_any_element(),
        );
        out.push(
            abs(s, 760.0, 182.0, Some(200.0), Some(640.0))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .pt(s.px(2.0))
                        .pr(s.px(22.0))
                        .pb(s.px(18.0))
                        .pl(s.px(20.0))
                        .child(sans(s, 16.0, 500.0, hex(0xeef1f7)).child("Agents"))
                        .child(self.control(
                            s,
                            icon_button(s, "ws-new-agent", "plus", 16.0),
                            "ws-new-agent",
                            interact::Ring::new(7.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| this.show_toast("New agent session", cx),
                        )),
                )
                .children(DEMO_AGENTS.iter().map(|(id, name, status)| {
                    let on = self.workspace.agent == *id && current == WsTab::Agents;
                    let agent: &'static str = id;
                    let key = format!("ws-agent-{id}");
                    // `.wsa { transition: background 0.2s }`; `.wsa.on`'s gradient is an image.
                    let flat = interact::tween_color(
                        &key,
                        "bg",
                        if !on && interact::hovered(&key) {
                            white(0.03)
                        } else {
                            white(0.0)
                        },
                        200,
                    );
                    let row = div()
                        .id(SharedString::from(key.clone()))
                        .relative()
                        .flex()
                        .items_center()
                        .w_full()
                        .pt(s.px(10.0))
                        .pb(s.px(10.0))
                        .pr(s.px(20.0))
                        .pl(s.px(18.0))
                        .mb(s.px(10.0))
                        .border_l_2()
                        .cursor_pointer()
                        .map(|this| {
                            if on {
                                this.border_color(hex(0x4b73ff)).bg(linear_gradient(
                                    90.0,
                                    linear_color_stop(rgba(60, 90, 200, 0.16), 0.0),
                                    linear_color_stop(rgba(60, 90, 200, 0.0), 1.0),
                                ))
                            } else {
                                this.border_color(white(0.0)).bg(flat)
                            }
                        })
                        .child(abox(s, 40.0, 10.0, 12.0).child(agent_logo(
                            s,
                            &self.catalog,
                            id,
                            22.0,
                        )))
                        .child(
                            div()
                                .child(sans(s, 13.0, 500.0, hex(0xeef1f7)).child(name.to_string()))
                                .child(
                                    sans(s, 11.5, 400.0, hex(0xaab2c1))
                                        .mt(s.px(3.0))
                                        .flex()
                                        .items_center()
                                        .gap(s.px(6.0))
                                        .whitespace_nowrap()
                                        .child(
                                            div().size(s.px(6.0)).rounded_full().bg(hex(0x3be3a2)),
                                        )
                                        .child(status.to_string()),
                                ),
                        );
                    self.control(
                        s,
                        row,
                        key,
                        interact::Ring::new(0.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            this.workspace.agent = agent;
                            this.ws_show_tab(WsTab::Agents);
                            cx.notify();
                        },
                    )
                }))
                .into_any_element(),
        );
        let fade = progress(
            self.workspace.shown_since,
            now,
            Duration::from_millis(300),
            Ease::Ease,
        );
        let body: AnyElement = match current {
            WsTab::Agents => self.ws_session_view(s, now),
            WsTab::View(ViewKey::Browser) => self.ws_browser_view(s, now, drive, cx),
            WsTab::View(ViewKey::Docs) => self.ws_docs_view(s, cx),
            WsTab::View(ViewKey::Code) => self.ws_code_view(s, now),
            WsTab::View(ViewKey::Kanban) => self.ws_kanban_view(s, now),
            WsTab::View(ViewKey::Automate) => self.ws_automate_view(s, cx),
        };
        out.push(
            abs(s, 972.0, 182.0, Some(650.0), Some(640.0))
                .rounded(s.px(12.0))
                .border_1()
                .border_color(white(0.07))
                .bg(rgba(10, 12, 20, 0.82))
                .overflow_hidden()
                .opacity(fade)
                .child(body)
                .into_any_element(),
        );
        out.extend(menu);
        out
    }

    /// The `MoreMenu` popover under the window's `⋯`.
    fn render_workspace_menu(&self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        {
            let t = entrance(self.workspace_menu_opened, now, 140);
            abs(
                s,
                1617.0 - 196.0,
                159.0 - 4.0 * (1.0 - t),
                Some(196.0),
                None,
            )
            .id("ws-menu")
            .opacity(t)
            .p(s.px(5.0))
            .rounded(s.px(10.0))
            .bg(hex(0x11141c))
            .border_1()
            .border_color(rgba(140, 160, 220, 0.2))
            .shadow(vec![shadow(black(0.5), 0.0, 16.0, 40.0, 0.0, s)])
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu_open = false;
                this.workspace_menu_dismissed = Some(Instant::now());
                cx.notify();
            }))
            .children(
                ["New session", "Split right", "Settings"]
                    .into_iter()
                    .map(|item| {
                        let key = format!("ws-menu-{item}");
                        let bg = interact::hover_color(
                            &key,
                            "bg",
                            rgba(90, 120, 230, 0.0),
                            rgba(90, 120, 230, 0.16),
                            interact::BUTTON_MS,
                        );
                        self.control(
                            s,
                            sans(s, 13.0, 400.0, hex(0xdfe4ee))
                                .id(SharedString::from(key.clone()))
                                .relative()
                                .py(s.px(8.0))
                                .px(s.px(10.0))
                                .rounded(s.px(7.0))
                                .bg(bg)
                                .whitespace_nowrap()
                                .cursor_pointer()
                                .child(item),
                            key,
                            interact::Ring::new(7.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                this.menu_open = false;
                                this.show_toast(item, cx);
                            },
                        )
                    }),
            )
            .into_any_element()
        }
    }
}

fn view_row_inner(s: S, id: &'static str, height: f32) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(s.px(26.0))
        .h(s.px(height))
        .pl(s.px(30.0))
        .pr(s.px(20.0))
        .cursor_pointer()
}
