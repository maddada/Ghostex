//! The in-stage popups: `<Popup>` from primitives.tsx (deleted 2026-10-01) and the "Install another agent" guide
//! (install-guide-popup.tsx (deleted 2026-10-01), `.modal*` and `.install-*` in styles/base.css).
use super::GpuiOnboardingWindow;
use super::OnboardingCommand;
use super::agent_cli::{GuideRowStatus, guide_lists_agent, guide_row_status, strip_ansi};
use super::fonts::{MANROPE, PLEX_MONO};
use super::interact;
use super::model::{INSTALL_GUIDE_URL, PRIMARY_AGENTS, catalog_agent_name, cli_catalog};
use super::primitives::*;
use super::stage::*;
use gpui::{
    AnyElement, ClipboardItem, Context, Div, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, canvas, div, point,
};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};
use gpui_component::tooltip::Tooltip;
use std::time::{Duration, Instant};

/// `<Popup>`: a dimmed backdrop over the stage with a glass card. A press on the backdrop closes it.
/// `close` is the header's close button ([`GpuiOnboardingWindow::popup_close`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn popup(
    s: S,
    title: &str,
    width: f32,
    opened_at: Instant,
    now: Instant,
    close: AnyElement,
    body: Vec<AnyElement>,
    on_close: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let fade = progress(opened_at, now, Duration::from_millis(200), Ease::Ease);
    let close_backdrop = on_close;
    div()
        .id("onboarding-popup-backdrop")
        .absolute()
        .left_0()
        .top_0()
        .w(s.px(STAGE_WIDTH))
        .h(s.px(STAGE_HEIGHT))
        .bg(rgba(3, 4, 8, 0.6))
        .opacity(fade)
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            close_backdrop(window, cx)
        })
        .child(
            glass(s)
                .id("onboarding-popup")
                .w(s.px(width))
                // The same 20pt on every side, so the header's X sits as far from the top as from
                // the side (CDXC:SessionChat 2026-11-11 in visual_page_modal/band_geometry.rs).
                .p(s.px(20.0))
                .border_color(rgba(120, 150, 235, 0.25))
                .bg(hex(0x0e1119))
                .shadow(vec![
                    shadow(black(0.6), 0.0, 30.0, 80.0, 0.0, s),
                    inset_shadow(white(0.045), 0.0, 1.0, 0.0, 0.0, s),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child({
                    let title = tracked_text(
                        s,
                        title.to_string(),
                        MANROPE,
                        700.0,
                        19.0,
                        19.0 * LH_MANROPE,
                        -0.01,
                        hex(0xeef1f7),
                    );
                    let head = div().flex().items_center().mb(s.px(10.0));
                    // The X leads the header on macOS, the title after it (CDXC:AppModal
                    // 2026-11-10 in window/round_close_button.rs).
                    if super::super::round_close_button::close_buttons_on_left() {
                        head.gap(s.px(10.0)).child(close).child(title)
                    } else {
                        head.justify_between().child(title).child(close)
                    }
                })
                .children(body),
        )
        .into_any_element()
}

/// `scrollbar-width: thin` as Chromium draws it on this dark page: a 13-device-pixel track at the
/// page's 1.5x, with arrow buttons at both ends and a square grey thumb.
const SCROLLBAR_WIDTH: f32 = 8.67;
const SCROLLBAR_BUTTON: f32 = 9.33;

/// The install list's always-visible thin scrollbar (`.install-list { scrollbar-width: thin }`).
///
/// CDXC:Onboarding 2026-09-28 WHY:
/// The page's list showed Chromium's classic thin scrollbar at all times (track, arrow buttons and
/// thumb), which tells the reader the list scrolls. GPUI-Kit's scrollbar supplies the thumb, drag
/// and track clicks in its always-visible mode; the track and arrows are drawn around it.
fn thin_scrollbar(s: S, handle: &ScrollHandle) -> AnyElement {
    let arrow = |id: &'static str, up: bool| {
        let handle = handle.clone();
        div()
            .id(id)
            .h(s.px(SCROLLBAR_BUTTON))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let center = bounds.center();
                        let half = bounds.size.width.min(bounds.size.height) * 0.36;
                        let height = half * 0.9;
                        let mut path = gpui::Path::new(if up {
                            point(center.x - half, center.y + height / 2.0)
                        } else {
                            point(center.x - half, center.y - height / 2.0)
                        });
                        if up {
                            path.line_to(point(center.x + half, center.y + height / 2.0));
                            path.line_to(point(center.x, center.y - height / 2.0));
                        } else {
                            path.line_to(point(center.x + half, center.y - height / 2.0));
                            path.line_to(point(center.x, center.y + height / 2.0));
                        }
                        window.paint_path(path, hex(0xa0a0a0));
                    },
                )
                .size_full(),
            )
            .on_click(move |_, window, _| {
                // A click on an arrow scrolls one 40px step, as Chromium's does.
                let offset = handle.offset();
                let max = handle.max_offset();
                let step = s.px(40.0);
                let y = if up {
                    (offset.y + step).min(gpui::px(0.0))
                } else {
                    (offset.y - step).max(-max.y)
                };
                handle.set_offset(point(offset.x, y));
                window.refresh();
            })
    };
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(s.px(SCROLLBAR_WIDTH))
        .bg(hex(0x2d2d2d))
        .flex()
        .flex_col()
        .child(arrow("install-list-up", true))
        .child(
            div().relative().flex_1().child(
                Scrollbar::vertical(handle)
                    .id("install-list-scrollbar")
                    .mode(ScrollbarMode::Always)
                    .viewport_from_layout()
                    .styles(|styles| {
                        let thumb = |style: gpui_component::scroll::ScrollbarThumbStyle,
                                     color: u32| {
                            style
                                .bg(hex(color))
                                .width(s.px(5.0))
                                .inset(s.px((SCROLLBAR_WIDTH - 5.0) / 2.0))
                                .radius(gpui::px(0.0))
                        };
                        styles
                            .track(|track| {
                                track
                                    .bg(gpui::transparent_black())
                                    .width(s.px(SCROLLBAR_WIDTH))
                            })
                            .track_hover(|track| {
                                track
                                    .bg(gpui::transparent_black())
                                    .width(s.px(SCROLLBAR_WIDTH))
                            })
                            .track_active(|track| {
                                track
                                    .bg(gpui::transparent_black())
                                    .width(s.px(SCROLLBAR_WIDTH))
                            })
                            .thumb(|style| thumb(style, 0x9f9f9f))
                            .thumb_hover(|style| thumb(style, 0xbcbcbc))
                            .thumb_active(|style| thumb(style, 0xdadada))
                    }),
            ),
        )
        .child(arrow("install-list-down", false))
        .into_any_element()
}

/// The Install button's shape while its install is queued or running: a spinner and the state, not
/// clickable.
fn busy_button(s: S, spinner: AnyElement, label: &str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(7.0))
        .h(s.px(30.0))
        .pl(s.px(9.0))
        .pr(s.px(11.0))
        .rounded(s.px(9.0))
        .border_1()
        .border_color(rgba(110, 150, 255, 0.3))
        .bg(rgba(60, 90, 200, 0.10))
        .font_family(super::fonts::dm_sans())
        .font_weight(gpui::FontWeight(500.0))
        .text_size(s.px(13.0))
        .line_height(s.px(13.0 * LH_DM_SANS))
        .text_color(hex(0xc9d0dc))
        .whitespace_nowrap()
        .cursor_default()
        .child(spinner)
        .child(label.to_string())
}

pub(crate) fn modal_p(s: S) -> Div {
    sans(s, 14.5, 400.0, hex(0xb3bac7)).line_height(s.px(14.5 * 1.5))
}

pub(crate) fn modal_actions(s: S) -> Div {
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(s.px(16.0))
        .mt(s.px(18.0))
}

impl GpuiOnboardingWindow {
    /// The popup header's close button; built before the popup's body, as it comes first in the page.
    pub(super) fn popup_close(&self, s: S, cx: &mut Context<Self>) -> AnyElement {
        self.control(
            s,
            icon_button(s, "onboarding-popup-close", "x", 16.0),
            "onboarding-popup-close",
            interact::Ring::new(7.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| this.close_popup(cx),
        )
        .into_any_element()
    }

    /// The Install guide. On the Agents panel (`queue`) it can be queued for after setup and each
    /// row installs through gxserver; on the finished screen it only copies commands.
    pub(super) fn render_install_guide(
        &mut self,
        s: S,
        now: Instant,
        queue: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connection = queue && self.cli_available;
        let close = self.popup_close(s, cx);
        let installed: std::collections::HashSet<String> = self
            .agents
            .iter()
            .filter(|agent| agent.installed)
            .map(|agent| agent.agent_id.clone())
            .collect();
        let mut entries: Vec<_> = cli_catalog()
            .into_iter()
            .filter(|entry| {
                guide_lists_agent(
                    installed.contains(&entry.agent_id),
                    self.cli_rows
                        .get(&format!("guide:{}", entry.agent_id))
                        .is_some(),
                )
            })
            .collect();
        let rank = |agent_id: &str| {
            PRIMARY_AGENTS
                .iter()
                .position(|(id, _)| *id == agent_id)
                .unwrap_or(PRIMARY_AGENTS.len())
        };
        entries.sort_by_key(|entry| rank(&entry.agent_id));
        if connection {
            for entry in &entries {
                let name = catalog_agent_name(&self.agents, &self.catalog, &entry.agent_id);
                self.ensure_cli_row(
                    &format!("guide:{}", entry.agent_id),
                    &entry.agent_id,
                    &name,
                    true,
                    cx,
                );
            }
        }
        let rows = entries.iter().map(|entry| {
            let name = catalog_agent_name(&self.agents, &self.catalog, &entry.agent_id);
            let key = format!("guide:{}", entry.agent_id);
            let docs = entry.docs_url.clone();
            let docs_tip = entry.docs_url.clone();
            let name_id = SharedString::from(format!("guide-name-{}", entry.agent_id));
            // `.install-row .nm.link:hover`: the host's 120ms button transition.
            let name_color = interact::hover_color(
                &name_id,
                "color",
                hex(0xeef1f7),
                hex(0x8fabff),
                interact::BUTTON_MS,
            );
            let name_link = self.control(
                s,
                nm(s, 14.5, 500.0)
                    .id(name_id.clone())
                    .relative()
                    .w(s.px(96.0))
                    .flex_none()
                    .cursor_pointer()
                    .text_color(name_color)
                    .tooltip(move |window, cx| Tooltip::new(docs_tip.clone()).build(window, cx))
                    .child(name.clone()),
                name_id,
                interact::Ring::new(0.0, 0.0),
                interact::Keys::EnterSpace,
                cx,
                move |this, _, cx| this.send(OnboardingCommand::OpenExternalUrl(docs.clone()), cx),
            );
            let row = self.cli_rows.get(&key);
            let status = guide_row_status(row, installed.contains(&entry.agent_id));
            let error = row
                .filter(|_| status == GuideRowStatus::Failed)
                .and_then(|row| row.error());
            let command = row
                .and_then(|row| row.method())
                .map(|method| method.command.clone())
                .or_else(|| entry.install_command.clone());
            let output = row
                .filter(|_| status == GuideRowStatus::Failed)
                .and_then(|row| row.job_output())
                .map(strip_ansi);
            let show_output = output.is_some() && row.is_some_and(|row| row.show_output);
            let progress = row.and_then(|row| row.progress_line());
            let action: Option<AnyElement> = match status {
                GuideRowStatus::Queued | GuideRowStatus::Installing => Some(
                    busy_button(
                        s,
                        spinner(s, 13.0, 1.5, self.opened_at, now).into_any_element(),
                        if status == GuideRowStatus::Queued {
                            "Queued…"
                        } else {
                            "Installing…"
                        },
                    )
                    .into_any_element(),
                ),
                GuideRowStatus::Installed => Some(
                    detpill(s, "Installed", DetTone::On, (28.0, 10.0, 12.5)).into_any_element(),
                ),
                _ if connection => {
                    let failed = status == GuideRowStatus::Failed;
                    let install_key = key.clone();
                    let title = command.clone().unwrap_or_default();
                    let id = SharedString::from(format!("guide-install-{}", entry.agent_id));
                    let install = self.control(
                        s,
                        install_button(
                            s,
                            id.clone(),
                            if failed { "refresh" } else { "plus" },
                            if failed { "Retry" } else { "Install" },
                            false,
                            (30.0, 9.0, 11.0, 13.0, 14.0),
                        )
                        .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx)),
                        id,
                        interact::Ring::new(9.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| this.cli_install(&install_key, cx),
                    );
                    let details = output.is_some().then(|| {
                        let details_key = key.clone();
                        let id = SharedString::from(format!("guide-details-{}", entry.agent_id));
                        self.control(
                            s,
                            ghost(
                                s,
                                id.clone(),
                                if show_output { "Hide" } else { "Details" },
                                13.0,
                                false,
                            )
                            .h(s.px(30.0))
                            .px(s.px(4.0)),
                            id,
                            interact::Ring::new(7.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                this.cli_rows.toggle_output(&details_key);
                                cx.notify();
                            },
                        )
                    });
                    Some(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(s.px(6.0))
                            .children(details)
                            .child(install)
                            .into_any_element(),
                    )
                }
                _ => command.clone().map(|command| {
                    let id = SharedString::from(format!("guide-copy-{}", entry.agent_id));
                    self.control(
                        s,
                        icon_button(s, id.clone(), "copy", 15.0),
                        id,
                        interact::Ring::new(7.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
                            this.show_toast(format!("Copied {command}"), cx);
                        },
                    )
                    .into_any_element()
                }),
            };
            let middle: AnyElement = match &error {
                Some(error) => {
                    let tip = error.clone();
                    div()
                        .id(SharedString::from(format!(
                            "guide-error-{}",
                            entry.agent_id
                        )))
                        .flex_1()
                        .min_w_0()
                        .font_family(super::fonts::dm_sans())
                        .text_size(s.px(12.0))
                        .text_color(hex(0xff6b62))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .child(error.clone())
                        .into_any_element()
                }
                None => {
                    // While it installs, the row shows the installer's latest line instead of the command.
                    let text = progress
                        .clone()
                        .or_else(|| command.clone())
                        .unwrap_or_else(|| "See the install docs".to_string());
                    let tip = text.clone();
                    div()
                        .id(SharedString::from(format!(
                            "guide-command-{}",
                            entry.agent_id
                        )))
                        .flex_1()
                        .min_w_0()
                        .font_family(PLEX_MONO)
                        .text_size(s.px(12.0))
                        .text_color(hex(0xc9d0dc))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .child(text)
                        .into_any_element()
                }
            };
            let output_panel = output.filter(|_| show_output).map(|output| {
                div()
                    .id(SharedString::from(format!(
                        "guide-output-{}",
                        entry.agent_id
                    )))
                    .mt(s.px(10.0))
                    .max_h(s.px(150.0))
                    .overflow_y_scroll()
                    .p(s.px(10.0))
                    .rounded(s.px(8.0))
                    .bg(rgba(0, 0, 0, 0.35))
                    .border_1()
                    .border_color(white(0.06))
                    .font_family(PLEX_MONO)
                    .text_size(s.px(11.0))
                    .line_height(s.px(11.0 * 1.45))
                    .text_color(hex(0xb3bac7))
                    .child(output.trim().to_string())
            });
            div()
                .flex()
                .flex_col()
                .flex_none()
                .py(s.px(10.0))
                .px(s.px(12.0))
                .rounded(s.px(10.0))
                .bg(white(0.03))
                .border_1()
                .border_color(if error.is_some() {
                    rgba(255, 107, 98, 0.22)
                } else {
                    white(0.07)
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(s.px(12.0))
                        .child(agent_logo(s, &self.catalog, &entry.agent_id, 18.0))
                        .child(name_link)
                        .child(middle)
                        .children(action),
                )
                .children(output_panel)
                .into_any_element()
        });
        let lead = if connection {
            "Ghostex runs any agent CLI installed on your computer. Install one here with its official command; it shows up in the list once the scan finds it.".to_string()
        } else {
            format!(
                "Ghostex runs any agent CLI already installed on your computer. Install one, then {}.",
                if queue {
                    "press Rescan"
                } else {
                    "rescan in Settings → Agents"
                }
            )
        };
        let mut body = vec![
            modal_p(s).child(lead).into_any_element(),
            div()
                .relative()
                .my(s.px(14.0))
                .child(
                    div()
                        .id("install-list")
                        .flex()
                        .flex_col()
                        .gap(s.px(8.0))
                        .max_h(s.px(430.0))
                        .overflow_y_scroll()
                        .track_scroll(&self.guide_scroll)
                        .pr(s.px(4.0 + SCROLLBAR_WIDTH))
                        .children(rows),
                )
                .child(thin_scrollbar(s, &self.guide_scroll))
                .into_any_element(),
        ];
        // `.ghost.guide-link`: `transition: color 0.2s`.
        let guide_color =
            interact::hover_color("guide-full", "color", hex(0xd6dbe5), gpui::white(), 200);
        let guide_link = self.control(
            s,
            div()
                .id("guide-full")
                .relative()
                .mr_auto()
                .flex()
                .items_center()
                .gap(s.px(8.0))
                .h(s.px(44.0))
                .px(s.px(4.0))
                .font_family(super::fonts::dm_sans())
                .text_size(s.px(15.5))
                .text_color(guide_color)
                .cursor_pointer()
                .child(icon(s, "external", 14.0, 1.6, guide_color))
                .child("Full install guide"),
            "guide-full",
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| {
                this.send(
                    OnboardingCommand::OpenExternalUrl(INSTALL_GUIDE_URL.to_string()),
                    cx,
                );
                if !queue {
                    this.finished.follow_up = None;
                    this.flow.install_queued = false;
                    cx.notify();
                }
            },
        );
        // The link's auto right margin takes the free space; Taffy would also apply
        // `justify-content: flex-end` on top of it and push the row past the card.
        let actions = if queue {
            modal_actions(s)
                .justify_start()
                .child(guide_link)
                .child(self.control(
                    s,
                    ghost(s, "guide-close", "Close", 15.5, false),
                    "guide-close",
                    interact::Ring::new(0.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| {
                        this.agents_panel.guide_open = false;
                        this.cli_rows.clear_lazy();
                        cx.notify();
                    },
                ))
                .child(self.control(
                    s,
                    cta(
                        s,
                        "guide-later",
                        "Open after onboarding",
                        true,
                        false,
                        false,
                        CtaSize::Small,
                    ),
                    "guide-later",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| {
                        this.flow.install_queued = true;
                        this.agents_panel.guide_open = false;
                        this.cli_rows.clear_lazy();
                        this.show_toast("Install guide will open after setup", cx);
                    },
                ))
        } else {
            modal_actions(s)
                .justify_start()
                .child(guide_link)
                .child(self.control(
                    s,
                    cta(s, "guide-done", "Done", true, false, false, CtaSize::Small),
                    "guide-done",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| {
                        this.finished.follow_up = None;
                        this.flow.install_queued = false;
                        cx.notify();
                    },
                ))
        };
        body.push(actions.into_any_element());
        let this = cx.entity().downgrade();
        let opened_at = if queue {
            self.agents_panel.guide_opened_at
        } else {
            self.finished.follow_up_at
        };
        popup(
            s,
            "Install another agent",
            620.0,
            opened_at,
            now,
            close,
            body,
            move |_, cx| {
                let _ = this.update(cx, |this, cx| this.close_popup(cx));
            },
        )
    }
}
