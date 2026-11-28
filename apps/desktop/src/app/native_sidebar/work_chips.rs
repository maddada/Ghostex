//! A session row's Clean up / Keep offer for its merged PR: the merge badge on the row's title line
//! and the small popup it opens beside the sidebar, level with the row.
//!
//! CDXC:WorkMode 2026-11-25 DECISION:
//! User picked option D on the sidebar-rows page: "On a row whose PR is merged and that gxserver
//! offers to clean up, the PR badge turns into a merge icon with #108. That is the only mark on the
//! row; the separate second line with #108 + Clean up + Keep chips goes away." Hovering the badge
//! says "PR #108 merged. Click to clean up or keep."; clicking it opens a popup beside the sidebar,
//! level with the row like the links card, saying what Clean up does, with Open PR, Keep and Clean
//! up in the app's neutral button style (never the accent). The row's right-click menu carries the
//! same two answers (gx-core sidebar_menu/layout.rs), and the phone opens the popup as a sheet. The
//! answers are unchanged (`answerWorkCleanup`, gx_store/work_links.rs), and a coordinator's threads
//! still answer in its banner (cleanup_banner.rs). This supersedes the 2026-10-09 chips on the
//! card's second line.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/work.rs (what the badge reads),
//! server/src/work_mode/cleanup.rs (the offer), window/merged_pr_popup.rs (the desktop's popup
//! window), apps/gpui-web/src/app/web_host/merged_pr_popup.rs (the web build's),
//! apps/mobile/app/src/components/sessions/SessionRow.tsx (the phone's sheet).

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use web_time::{Duration, Instant};

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::tooltips::SidebarTooltipSpan;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use ghostex_gx_core::SessionLinkTone;

/// The popup's width, as the mockup draws it.
pub(crate) const MERGED_PR_POPUP_WIDTH: f32 = 270.0;
/// The gap between the sidebar's edge and the popup.
pub(crate) const MERGED_PR_POPUP_GAP: f32 = 6.0;
/// A click on the badge right after a click outside closed the popup is that same click.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// A merged PR the row offers to clean up, as its badge and popup show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MergedOffer {
    pub(crate) session_id: String,
    pub(crate) number: u64,
    pub(crate) url: Option<String>,
    pub(crate) branch: Option<String>,
}

impl MergedOffer {
    fn terms(&self) -> ghostex_gx_protocol::git_provider::GitHostTerms {
        self.url
            .as_deref()
            .and_then(ghostex_gx_protocol::git_provider::detect_provider_from_pr_url)
            .unwrap_or_default()
            .terms()
    }
}

/// The open popup: whose offer, and the top of its row in the window when it opened.
#[derive(Clone, Debug)]
pub(crate) struct MergedPopupOpen {
    pub(crate) offer: MergedOffer,
    pub(crate) top: f32,
}

/// What a popup button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergedOfferAnswer {
    OpenPr,
    Keep,
    CleanUp,
}

/// The row's merged-PR offer is answered in its coordinator's banner (gx-core
/// `RowNesting::cleanup_in_banner`, sidebar_snapshot.rs `cleanupInBanner`).
pub(crate) fn offer_in_banner(session: &NativeSidebarSession) -> bool {
    session
        .details
        .get("cleanupInBanner")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// The offer the row answers itself: gxserver offers to clean up its merged PR and no banner of
/// its coordinator answers for it.
pub(crate) fn row_merged_offer(session: &NativeSidebarSession) -> Option<MergedOffer> {
    let work = session
        .work
        .as_ref()
        .filter(|work| work.offer_cleanup && !offer_in_banner(session))?;
    let pr = work.pull_request.as_ref()?;
    let branch = work
        .branch
        .clone()
        .or_else(|| super::threads::worktree_branch_label(session))
        .filter(|branch| !branch.trim().is_empty());
    Some(MergedOffer {
        session_id: session.session_id.clone(),
        number: pr.number,
        url: pr.url.clone(),
        branch,
    })
}

fn merged_color() -> gpui::Hsla {
    super::link_chips::session_link_badge_color(SessionLinkTone::Merged)
}

impl GhostexGpuiApp {
    /// The merge badge in place of the row's links badge: the merge icon and the PR's number.
    pub(super) fn render_merged_pr_badge(
        &self,
        offer: &MergedOffer,
        tooltip_span: SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let color = merged_color();
        let terms = offer.terms();
        let tooltip = format!(
            "{} {}{} merged. Click to clean up or keep.",
            terms.short, terms.number_prefix, offer.number
        );
        let open = self.merged_pr_popup_open_for(&offer.session_id);
        let show_tooltip = !open
            && self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let toggle_offer = offer.clone();
        div()
            .id(format!("native-session-merged-pr-{}", offer.session_id))
            .role(gpui::Role::Button)
            .aria_label(tooltip.clone())
            .aria_expanded(open)
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0 * scale))
            .text_size(px(super::threads::BADGE_TEXT_SIZE * scale))
            .text_color(color)
            .whitespace_nowrap()
            .cursor_pointer()
            .child(titlebar_svg_icon(
                "titlebar/git-merge.svg",
                12.0 * scale,
                color,
            ))
            .child(format!("{}{}", terms.number_prefix, offer.number))
            // Its own click: the row's selection and drag never start from the badge.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_merged_pr_popup(toggle_offer.clone(), cx);
            }))
            .when(show_tooltip, |badge| {
                badge.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    appearance.tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_tooltip(
                            tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .into_any_element()
    }

    pub(crate) fn merged_pr_popup_open_for(&self, session_id: &str) -> bool {
        self.native_sidebar
            .merged_popup
            .as_ref()
            .is_some_and(|open| open.offer.session_id == session_id)
    }

    /// Opens the popup for the row's offer, or closes it when it is the one open.
    fn toggle_merged_pr_popup(&mut self, offer: MergedOffer, cx: &mut gpui::Context<Self>) {
        let session_id = offer.session_id.clone();
        if self.merged_pr_popup_open_for(&session_id) {
            self.close_merged_pr_popup(cx);
            return;
        }
        let just_closed = self
            .native_sidebar
            .merged_popup_closed
            .as_ref()
            .is_some_and(|(closed, at)| *closed == session_id && at.elapsed() < REOPEN_GUARD);
        if just_closed {
            return;
        }
        let Some(row) = self.native_sidebar.session_card_bounds.get(&session_id) else {
            return;
        };
        let top = row.top().as_f32();
        self.close_session_links_card(cx);
        self.open_merged_pr_popup(MergedPopupOpen { offer, top }, cx);
    }

    /// Closes the popup and remembers which one, so the press that closed it doesn't reopen it.
    pub(crate) fn close_merged_pr_popup(&mut self, cx: &mut gpui::Context<Self>) {
        self.close_merged_pr_popup_window(cx);
        self.forget_merged_pr_popup(cx);
    }

    /// Forgets the open popup without touching its window: what the desktop's popup window calls
    /// before it removes itself (a window can't be closed from inside its own update).
    pub(crate) fn forget_merged_pr_popup(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(open) = self.native_sidebar.merged_popup.take() {
            self.native_sidebar.merged_popup_closed = Some((open.offer.session_id, Instant::now()));
        }
        self.native_sidebar.merged_popup_window = None;
        cx.notify();
    }

    /// A popup button: Open PR opens it where Settings says (gx_store/session_links.rs), Keep and
    /// Clean up answer the offer on the session's computer (gx_store/work_links.rs).
    pub(crate) fn answer_merged_pr_popup(
        &mut self,
        offer: &MergedOffer,
        answer: MergedOfferAnswer,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_merged_pr_popup(cx);
        let command = match answer {
            MergedOfferAnswer::OpenPr => match &offer.url {
                Some(url) => {
                    json!({ "type": "openSessionLink", "url": url, "sessionId": offer.session_id })
                }
                None => return,
            },
            MergedOfferAnswer::Keep | MergedOfferAnswer::CleanUp => json!({
                "type": "answerWorkCleanup",
                "sessionId": offer.session_id,
                "answer": if answer == MergedOfferAnswer::Keep { "keep" } else { "cleanUp" },
            }),
        };
        self.dispatch_native_sidebar_ui(command, cx);
    }
}

/// What a popup button calls: the desktop's popup window and the web build's page both route it to
/// `answer_merged_pr_popup`.
pub(crate) type MergedAnswerHandler = Rc<dyn Fn(MergedOfferAnswer, &mut Window, &mut App)>;

/// The popup's card: the merged PR, its branch, what Clean up and Keep do, and the three buttons.
/// `frosted` draws it on the frosted host's fill (the desktop's blurred window), otherwise on the
/// in-window bubble (the web build's page), like the Main Hub card (tooltips.rs `main_hub_card`).
pub(crate) fn merged_pr_popup_card(
    offer: &MergedOffer,
    frosted: bool,
    glass: bool,
    on_answer: MergedAnswerHandler,
) -> gpui::Stateful<gpui::Div> {
    let menu = titlebar_popup_menu_background();
    let bubble = super::tooltips::in_window_bubble(glass);
    let fill = if frosted {
        frosted_tooltip_fill(menu)
    } else {
        bubble.fill
    };
    let foreground = titlebar_popup_menu_foreground();
    let muted = bubble.secondary;
    let light = chrome_uses_light_appearance();
    let fills = crate::app::window::native_modal_kit::neutral_button_fills(light);
    let button = |id: &'static str,
                  label: &'static str,
                  icon: Option<&'static str>,
                  answer: MergedOfferAnswer| {
        let on_answer = on_answer.clone();
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_shrink_0()
            .h(px(26.0))
            .px(px(9.0))
            .gap(px(5.0))
            .items_center()
            .rounded(px(6.0))
            .bg(gpui::Hsla::from(fills.rest))
            .hover(move |style| style.bg(gpui::Hsla::from(fills.hover)))
            .active(move |style| style.bg(gpui::Hsla::from(fills.pressed)))
            .text_size(px(12.5))
            .text_color(foreground)
            .cursor_pointer()
            .children(icon.map(|icon| titlebar_svg_icon(icon, 13.0, foreground)))
            .child(label)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_answer(answer, window, cx);
            })
    };
    let terms = offer.terms();
    let open_label = format!("Open {}", terms.short);
    let open_pr = offer.url.is_some().then(|| {
        let on_answer = on_answer.clone();
        h_flex()
            .id("merged-pr-popup-open")
            .role(gpui::Role::Link)
            .aria_label(open_label.clone())
            .flex_shrink_0()
            .h(px(26.0))
            .gap(px(5.0))
            .items_center()
            .text_size(px(12.5))
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.text_color(foreground))
            .child(titlebar_svg_icon("titlebar/external-link.svg", 13.0, muted))
            .child(open_label)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_answer(MergedOfferAnswer::OpenPr, window, cx);
            })
    });
    v_flex()
        .id("merged-pr-popup")
        .w(px(MERGED_PR_POPUP_WIDTH))
        .p(px(12.0))
        .gap(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(bubble.border)
        .bg(fill)
        .text_color(foreground)
        .cursor_default()
        .child(
            v_flex()
                .gap(px(3.0))
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .items_center()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(titlebar_svg_icon(
                            "titlebar/git-merge.svg",
                            14.0,
                            merged_color(),
                        ))
                        .child(format!(
                            "{} {}{} is merged",
                            terms.short, terms.number_prefix, offer.number
                        )),
                )
                .children(offer.branch.clone().map(|branch| {
                    h_flex()
                        .min_w_0()
                        .gap(px(5.0))
                        .items_center()
                        .text_size(px(11.5))
                        .text_color(muted)
                        .child(titlebar_svg_icon("titlebar/git-branch.svg", 12.0, muted))
                        .child(div().min_w_0().truncate().child(branch))
                })),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(muted)
                .child(format!(
                    "Clean up removes this session's worktree and parks the session. Keep leaves \
                     both and won't ask again for this {}.",
                    terms.short
                )),
        )
        .child(
            h_flex()
                .pt(px(2.0))
                .gap(px(6.0))
                .items_center()
                .children(open_pr)
                .child(div().flex_1())
                .child(button(
                    "merged-pr-popup-keep",
                    "Keep",
                    None,
                    MergedOfferAnswer::Keep,
                ))
                .child(button(
                    "merged-pr-popup-clean-up",
                    "Clean up",
                    Some("titlebar/archive.svg"),
                    MergedOfferAnswer::CleanUp,
                )),
        )
}

impl super::model::NativeSidebarGroup {
    /// The project id of a group whose project has Work mode on, on this computer; `None` for
    /// every other group (the snapshot sets `workMode` on work-mode projects only,
    /// gx_store/sidebar_snapshot.rs).
    pub(crate) fn work_mode_project_id(&self) -> Option<&str> {
        if self.remote_machine_context.is_some() {
            return None;
        }
        let context = self.project_context.as_ref()?;
        if context.get("workMode").and_then(serde_json::Value::as_bool) != Some(true) {
            return None;
        }
        context
            .pointer("/editor/projectId")
            .and_then(serde_json::Value::as_str)
            .filter(|project_id| !project_id.is_empty())
    }
}

impl super::model::NativeSidebarSnapshot {
    /// The work-mode projects the sidebar shows, once each: what the Work view lists and whether
    /// the briefcase shows (app/work_view/).
    pub(crate) fn work_mode_project_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for group in &self.groups {
            if let Some(project_id) = group.work_mode_project_id()
                && !ids.iter().any(|id| id == project_id)
            {
                ids.push(project_id.to_string());
            }
        }
        ids
    }
}
