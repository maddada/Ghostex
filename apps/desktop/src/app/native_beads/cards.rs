//! The cards above the tree (a write that failed, a sync that failed, an older computer, a
//! project with no Beads yet) and the Sync panel. Card wording follows the app's rule: a short
//! title, at most two plain sentences, buttons that say what they do, raw output behind Details.

use ghostex_gx_core::beads_view::{parse_bd_time_ms, relative_ago};
use ghostex_gx_protocol::beads_view::{BeadsSyncResult, BeadsWorkspaceState};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::Value;

use super::view::{BeadsNotice, BeadsRequest, BeadsView};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::text::board_notice;
use crate::app::native_kanban::widgets::{KanbanButtonKind, kanban_button};

/// A write bd refused, in plain words; bd's own text goes behind Details.
pub(crate) fn write_failure(title: &str, reply: &Value) -> BeadsNotice {
    let text = |key: &str| {
        reply
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let output = [text("stderr"), text("stdout")]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = output.to_lowercase();
    let message = if lower.contains("already claimed") {
        "Someone else has claimed it. Refresh to see who."
    } else if lower.contains("no issue found") || lower.contains("not found") {
        "It was deleted or renamed elsewhere. Refresh to see the current list."
    } else if reply
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        == Some("timeout")
    {
        "Beads stayed busy. Try again in a moment."
    } else {
        "Beads refused the change. Details has what it said."
    };
    BeadsNotice {
        title: title.to_string(),
        message: message.to_string(),
        details: (!output.is_empty()).then_some(output),
        pull_now: false,
    }
}

/// The card for a sync that did not go through.
fn sync_failure(result: &BeadsSyncResult) -> Option<BeadsNotice> {
    if result.ok {
        return None;
    }
    let push = result.kind == "push";
    let (title, message, pull_now) = match result.code.as_str() {
        "notFastForward" => (
            "Push didn't go through",
            "The remote has changes this computer doesn't have yet. Pull them, then push again.",
            true,
        ),
        "conflict" => (
            if push {
                "Push didn't go through"
            } else {
                "Pull found conflicting changes"
            },
            "The same beads changed here and on another computer. Ask an agent to resolve the conflict with bd, then sync again.",
            false,
        ),
        "noRemote" => (
            "No sync remote set up",
            "This workspace has nowhere to sync to yet. Ask an agent to add one with bd dolt remote add.",
            false,
        ),
        "auth" => (
            if push {
                "Push didn't go through"
            } else {
                "Pull didn't go through"
            },
            "The remote refused this computer's credentials. Check access to it, then try again.",
            false,
        ),
        "network" => (
            if push {
                "Push didn't go through"
            } else {
                "Pull didn't go through"
            },
            "Couldn't reach the remote. Check the connection, then try again.",
            false,
        ),
        "timeout" => (
            if push {
                "Push took too long"
            } else {
                "Pull took too long"
            },
            "Beads stayed busy or the remote was slow. Try again in a moment.",
            false,
        ),
        _ => (
            if push {
                "Push didn't go through"
            } else {
                "Pull didn't go through"
            },
            "Beads stopped with an error. Details has what it said.",
            false,
        ),
    };
    Some(BeadsNotice {
        title: title.to_string(),
        message: message.to_string(),
        details: (!result.output.is_empty()).then(|| result.output.clone()),
        pull_now,
    })
}

impl BeadsView {
    fn sync_result(&self) -> Option<BeadsSyncResult> {
        self.snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.sync.last_result.clone())
    }

    /// "Synced 4 min ago", from the latest successful pull or push.
    pub(crate) fn synced_label(&self) -> (String, bool) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return ("Sync".to_string(), false);
        };
        if let Some(running) = snapshot.sync.running.as_ref() {
            return (
                if running.kind == "push" {
                    "Pushing…"
                } else {
                    "Pulling…"
                }
                .to_string(),
                false,
            );
        }
        let last = [&snapshot.sync.last_push_at, &snapshot.sync.last_pull_at]
            .into_iter()
            .flatten()
            .filter_map(|time| parse_bd_time_ms(time))
            .max();
        let failed = self
            .sync_result()
            .is_some_and(|result| !result.ok && result.code != "noRemote");
        match last {
            Some(time) => (
                format!("Synced {}", relative_ago(time, &self.clock)),
                failed,
            ),
            None => ("Not synced yet".to_string(), failed),
        }
    }

    pub(crate) fn render_cards(
        &mut self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut cards = Vec::new();
        if let Some(notice) = self.notice.clone() {
            cards.push(self.notice_card("beads-notice", &notice, true, p, cx));
        }
        if let Some(notice) = self.sync_result().as_ref().and_then(sync_failure) {
            let dismissed = self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.sync.last_result.as_ref())
                .is_some_and(|result| self.dismissed_sync.as_deref() == Some(result.at.as_str()));
            if !dismissed {
                cards.push(self.notice_card("beads-sync-notice", &notice, false, p, cx));
            }
        }
        if let Some(snapshot) = self.snapshot.as_ref().filter(|_| self.workspace_ready()) {
            if snapshot.readiness == ghostex_gx_protocol::beads_view::BeadsReadiness::Paused {
                cards.push(quiet_line("Ready list paused while Beads is busy.", p));
            } else if snapshot.busy || snapshot.settling {
                cards.push(quiet_line("Beads is busy; showing what it last said.", p));
            }
        }
        cards
    }

    fn notice_card(
        &self,
        id: &'static str,
        notice: &BeadsNotice,
        dismissable: bool,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let details = notice.details.clone();
        let show_details = self.notice_details_open == Some(id);
        div()
            .flex()
            .flex_none()
            .gap(px(12.0))
            .p(px(14.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(p.border)
            .bg(p.lane)
            .child(titlebar_svg_icon(
                "titlebar/alert-triangle.svg",
                18.0,
                p.danger,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(notice.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(p.muted)
                            .child(notice.message.clone()),
                    )
                    .when(show_details, |this| {
                        this.when_some(details.clone(), |this, details| {
                            this.child(
                                div()
                                    .mt(px(6.0))
                                    .p(px(8.0))
                                    .rounded(px(6.0))
                                    .bg(p.control)
                                    .font_family(crate::app::native_chat::fonts::CHAT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(p.muted)
                                    .child(details),
                            )
                        })
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_start()
                    .gap(px(6.0))
                    .when(details.is_some(), |this| {
                        this.child(
                            kanban_button(
                                gpui::SharedString::from(format!("{id}-details")),
                                None,
                                Some(
                                    if show_details {
                                        "Hide details"
                                    } else {
                                        "Details"
                                    }
                                    .into(),
                                ),
                                KanbanButtonKind::Ghost,
                                false,
                                p,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.notice_details_open =
                                        if this.notice_details_open == Some(id) {
                                            None
                                        } else {
                                            Some(id)
                                        };
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .when(notice.pull_now, |this| {
                        this.child(
                            kanban_button(
                                gpui::SharedString::from(format!("{id}-pull")),
                                Some("titlebar/download.svg"),
                                Some("Pull now".into()),
                                KanbanButtonKind::Primary,
                                false,
                                p,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.start_sync("pull", window, cx),
                            )),
                        )
                    })
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("{id}-dismiss")))
                            .cursor_pointer()
                            .p(px(4.0))
                            .child(titlebar_svg_icon("titlebar/x.svg", 13.0, p.muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if dismissable {
                                    this.notice = None;
                                } else {
                                    this.dismissed_sync = this
                                        .snapshot
                                        .as_ref()
                                        .and_then(|snapshot| snapshot.sync.last_result.as_ref())
                                        .map(|result| result.at.clone());
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    /// No workspace, no bd, an error, or a computer whose Ghostex predates the view.
    pub(crate) fn render_empty_card(
        &mut self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let project_path = self
            .target
            .as_ref()
            .map(|target| target.project_path.clone())
            .unwrap_or_default();
        if self.legacy {
            return Some(card(
                "Update Ghostex on that computer",
                vec!["This computer runs an older Ghostex, so the Beads view shows the board only. Switch to Board, or update Ghostex there for the tree.".to_string()],
                None,
                p,
            ));
        }
        let snapshot = self.snapshot.as_ref()?;
        let (message, install) = match snapshot.workspace.state {
            BeadsWorkspaceState::Ready => return None,
            BeadsWorkspaceState::None => ("no beads database found".to_string(), false),
            BeadsWorkspaceState::NoBd => ("bd was not found".to_string(), true),
            BeadsWorkspaceState::Failed => (
                snapshot
                    .workspace
                    .message
                    .clone()
                    .unwrap_or_else(|| "Beads could not be read.".to_string()),
                false,
            ),
        };
        let notice = board_notice(&message, &project_path);
        let prompt = notice.fix_prompt.clone();
        // Names the folder bd was asked about, so a project whose Beads folder setting points
        // somewhere else shows where Ghostex looked (absent from older computers).
        let mut lines = notice.lines.clone();
        if let Some(folder) = snapshot.workspace.checked_folder.as_deref() {
            lines.push(format!("Looked in {}", tilde_home(folder)));
        }
        let mut buttons = div().flex().gap(px(8.0)).child(
            kanban_button(
                "beads-copy-fix",
                Some("titlebar/copy.svg"),
                Some("Copy fix prompt".into()),
                KanbanButtonKind::Secondary,
                false,
                p,
            )
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()))
            })),
        );
        if install {
            buttons = buttons.child(
                kanban_button(
                    "beads-install",
                    Some("titlebar/download.svg"),
                    Some("Install Beads".into()),
                    KanbanButtonKind::Primary,
                    false,
                    p,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    (this.host.clone())(BeadsRequest::InstallBeads, window, cx)
                })),
            );
        }
        Some(card(
            &notice.title,
            lines,
            Some(buttons.into_any_element()),
            p,
        ))
    }
}

/// A path under the home folder written with `~`, as the app shows paths elsewhere.
fn tilde_home(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match path.strip_prefix(home.as_str()) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_string(),
    }
}

fn quiet_line(text: &'static str, p: &KanbanPalette) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .text_color(p.muted)
        .child(titlebar_svg_icon("titlebar/loader2.svg", 12.0, p.muted))
        .child(text)
        .into_any_element()
}

fn card(
    title: &str,
    lines: Vec<String>,
    buttons: Option<AnyElement>,
    p: &KanbanPalette,
) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(p.border)
        .bg(p.lane)
        .child(titlebar_svg_icon(
            "titlebar/alert-triangle.svg",
            18.0,
            gpui::rgb(0xe7b85b).into(),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.to_string()),
                )
                .children(
                    lines
                        .into_iter()
                        .map(|line| div().text_size(px(13.0)).text_color(p.muted).child(line)),
                )
                .children(buttons),
        )
        .into_any_element()
}
