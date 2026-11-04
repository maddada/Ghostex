//! The board notice above the lanes (`ProjectBoardNotice`): why Beads could not answer, with a fix
//! prompt to hand an agent, the Beads guide where one applies, and Install Beads when the CLI is
//! missing (`install_beads.rs`).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::palette::KanbanPalette;
use super::text::board_notice;
use super::widgets::{KanbanButtonKind, kanban_button};
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;

impl GhostexGpuiApp {
    pub(crate) fn render_native_kanban_notice(
        &mut self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let message = self.native_kanban.error.clone()?;
        let project_path = self
            .native_kanban
            .project
            .as_ref()
            .map(|project| project.project_path.clone())
            .unwrap_or_default();
        let notice = board_notice(&message, &project_path);
        let prompt = notice.fix_prompt.clone();
        if notice.offers_install {
            self.native_kanban_fetch_beads_tool(cx);
        }
        let install = &self.native_kanban.beads_install;
        let installing = notice.offers_install && install.running;
        let mut status_lines: Vec<String> = Vec::new();
        if notice.offers_install {
            if installing {
                let progress = install
                    .progress
                    .lock()
                    .ok()
                    .and_then(|progress| progress.clone());
                status_lines.push(format!(
                    "Installing Beads…{}",
                    progress.map(|line| format!(" {line}")).unwrap_or_default()
                ));
            } else if let Some(error) = &install.error {
                status_lines.push(format!("The Beads install did not finish: {error}"));
            }
            if let Some(reason) = &install.blocked_reason {
                status_lines.push(reason.clone());
            }
        }
        let install_button = (notice.offers_install && install.blocked_reason.is_none()).then(|| {
            let plan = install.plan.clone().unwrap_or_else(|| {
                "Downloads Beads from its official GitHub releases, checks it against the published checksum and puts bd in Ghostex's tools folder. No password needed.".to_string()
            });
            kanban_button(
                "kanban-notice-install-beads",
                Some(if installing {
                    "titlebar/loader2.svg"
                } else {
                    "titlebar/download.svg"
                }),
                Some(if installing { "Installing Beads…" } else { "Install Beads" }.into()),
                KanbanButtonKind::Primary,
                installing,
                p,
            )
            .tooltip(move |window, cx| crate::app::helpers::titlebar_tooltip(plan.clone(), window, cx))
            .when(!installing, |this| {
                this.on_click(cx.listener(|this, _, _, cx| this.native_kanban_install_beads(cx)))
            })
        });
        Some(
            div()
                .flex()
                .flex_none()
                .gap(px(10.0))
                .p(px(12.0))
                .rounded(px(10.0))
                .bg(p.panel)
                .border_1()
                .border_color(p.border)
                .child(div().flex_none().pt(px(1.0)).child(titlebar_svg_icon(
                    "titlebar/alert-triangle.svg",
                    16.0,
                    gpui::rgb(0xfbbf24).into(),
                )))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(p.foreground)
                                .child(notice.title.clone()),
                        )
                        .children(notice.lines.iter().chain(status_lines.iter()).map(|line| {
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(p.muted)
                                .child(line.clone())
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .mt(px(6.0))
                                .children(install_button)
                                .child(
                                    kanban_button(
                                        "kanban-notice-copy",
                                        Some("titlebar/copy.svg"),
                                        Some("Copy fix prompt".into()),
                                        KanbanButtonKind::Secondary,
                                        false,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        move |_, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                prompt.clone(),
                                            ));
                                        },
                                    )),
                                )
                                .when_some(notice.link.clone(), |this, (label, url)| {
                                    this.child(
                                        kanban_button(
                                            "kanban-notice-link",
                                            Some("titlebar/external-link.svg"),
                                            Some(label.into()),
                                            KanbanButtonKind::Ghost,
                                            false,
                                            p,
                                        )
                                        .on_click(
                                            move |_, _, cx| {
                                                crate::app::helpers::gpui_open_external_url(
                                                    &url, cx,
                                                )
                                            },
                                        ),
                                    )
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}
