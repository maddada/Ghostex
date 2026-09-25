//! A finished turn: its "Worked for Xs" heading, the work log behind it, the "N files changed"
//! fold, the artifacts and answered questions it keeps in view, and the final reply.
//!
//! At rest the row is one column with an 8px gap, React's `.ghostex-chat-completed-turn`. While the
//! turn is folding, or its log is opening or closing, `disclosure_motion.rs` reports a frame and
//! the row is built from the same parts with each gap carried inside the part that owns it, so a
//! part whose height eases to nothing takes its gap with it and the first and last frames match
//! the live rows and the settled row exactly.

use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::disclosure_motion::{FoldFrame, FoldMotionKind, motion_clip};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div, px,
    relative,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// Space between the parts of a finished turn, React's `gap: 0.5rem` on the turn.
const PART_GAP: f32 = 8.0;
/// Space between two transcript list items, the `pb(16)` every row wrapper ends with, which the
/// live rows had between them before they folded.
const LIVE_ROW_GAP: f32 = 16.0;

/// A part whose height eases between nothing and its natural height; the fold applies its own
/// opacity to the rows, so the clip carries none.
fn eased_part(height: Rc<Cell<f32>>, factor: f32, content: AnyElement) -> AnyElement {
    motion_clip(
        height,
        FoldFrame {
            kind: FoldMotionKind::Fold,
            heading: 1.0,
            heading_offset: 0.0,
            log: factor,
            rows_opacity: 1.0,
        },
        0.0,
        0.0,
        content,
    )
}

impl NativeChatView {
    pub(super) fn completed_work_row(
        &mut self,
        item: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut work_appearance = p.clone();
        work_appearance.primary = p.muted;
        let id = format!("work:{}", text(item, "id"));
        let expanded = self.is_expanded(&id, p.verbose);
        self.disclosure_motion
            .borrow_mut()
            .arrived(&id, !expanded, cx.reduce_motion());
        let motion = self.disclosure_frame(&id, expanded, cx);
        let heading_opacity = motion.map_or(1.0, |frame| frame.heading);
        let heading = if item["expandable"] == true {
            let disclosure = self.disclosure(
                id.clone(),
                text(item, "label"),
                expanded,
                Some(json!({"type":"loadWork","id":item["id"],"work":item["deferred"]}))
                    .filter(|_| item["deferred"].is_object()),
                &work_appearance,
                cx,
            );
            div()
                .text_color(p.muted)
                .font_weight(FontWeight::MEDIUM)
                .child(disclosure)
                .into_any_element()
        } else {
            div()
                .pl(px(24.0 * s))
                .text_color(p.muted.opacity(0.5))
                .font_weight(FontWeight::MEDIUM)
                .child(text(item, "label"))
                .into_any_element()
        };
        let divider = div()
            .h(px(1.0))
            .mt(px(2.0 * s))
            .mb(px(8.0 * s))
            .w(relative(heading_opacity))
            .bg(p.border)
            .into_any_element();
        // Every row of a finished turn hides its own writes; they belong to the turn's
        // "N files changed" fold below the heading (file_change_card.rs). Rows that are still
        // folding keep theirs until they are gone, as they were a moment ago.
        let folding = motion.is_some_and(|frame| frame.kind == FoldMotionKind::Fold);
        let log = if folding {
            let mut rows: Vec<AnyElement> = Vec::new();
            for message in item["work"].as_array().into_iter().flatten() {
                rows.push(self.message_row(message, p, window, cx));
            }
            (!rows.is_empty()).then(|| rows)
        } else if expanded || motion.is_some() {
            // React hung the whole log off the rail the "Worked for" heading opened
            // (`SessionChatExpansion`), so the turn's work reads as one indented block.
            self.hide_file_changes = true;
            let mut log: Vec<AnyElement> = Vec::new();
            if let Some(notice) = self.deferred_work_notice(item, p, cx) {
                log.push(notice);
            }
            self.in_work_fold = true;
            for message in item["work"].as_array().into_iter().flatten() {
                log.push(self.message_row(message, p, window, cx));
            }
            self.in_work_fold = false;
            (!log.is_empty()).then(|| {
                vec![disclosure_body(
                    p,
                    DisclosureRail::Marker,
                    8.0,
                    id.clone(),
                    "Collapse completed work",
                    log,
                    cx,
                )]
            })
        } else {
            None
        };
        self.hide_file_changes = true;
        let files = self.completed_files_fold(item, p, cx);
        let mut tail: Vec<AnyElement> = Vec::new();
        for message in item["artifacts"].as_array().into_iter().flatten() {
            tail.push(self.message_row(message, p, window, cx));
        }
        // The turn's answered questions, lifted out of the fold so an exchange the reader took
        // part in is never buried by a collapsed "Worked for Xs" section.
        if let Some(cards) = self.question_exchange_cards(&id, &item["questions"], p, cx) {
            tail.push(cards);
        }
        if item["final"].is_object() {
            tail.push(self.message_row(&item["final"], p, window, cx));
        }
        self.hide_file_changes = false;

        let Some(frame) = motion else {
            let mut row = div()
                .flex()
                .flex_col()
                .w_full()
                .gap(px(PART_GAP * s))
                .child(heading)
                .child(divider);
            if let Some(log) = log {
                row = row.children(log);
            }
            return row.children(files).children(tail).into_any_element();
        };
        self.moving_completed_work_row(&id, frame, heading, divider, log, files, tail, s)
    }

    /// The row while it folds, opens or closes. Each part carries the gap above it, so a part
    /// eased to nothing leaves no gap behind.
    #[allow(clippy::too_many_arguments)]
    fn moving_completed_work_row(
        &mut self,
        id: &str,
        frame: FoldFrame,
        heading: AnyElement,
        divider: AnyElement,
        log: Option<Vec<AnyElement>>,
        files: Option<AnyElement>,
        tail: Vec<AnyElement>,
        s: f32,
    ) -> AnyElement {
        let spaced = |part: AnyElement| div().pt(px(PART_GAP * s)).child(part).into_any_element();
        let mut row = div().flex().flex_col().w_full();
        let top = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .top(px(-frame.heading_offset * s))
                    .opacity(frame.heading)
                    .child(heading),
            )
            .child(spaced(divider))
            .into_any_element();
        if frame.kind == FoldMotionKind::Fold {
            // The heading did not exist while the turn was live, so it grows in; the rows it
            // replaces keep the spacing they had as separate transcript rows until they are gone.
            row = row.child(eased_part(
                self.disclosure_motion.borrow_mut().height(id, "heading"),
                frame.heading,
                top,
            ));
            if let Some(rows) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(LIVE_ROW_GAP * s))
                        .pb(px((LIVE_ROW_GAP - PART_GAP) * s))
                        .opacity(frame.rows_opacity)
                        .children(rows)
                        .into_any_element(),
                ));
            }
            if let Some(files) = files {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "files"),
                    frame.heading,
                    spaced(files),
                ));
            }
        } else {
            row = row.child(top);
            if let Some(log) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .pt(px(PART_GAP * s))
                        .opacity(frame.rows_opacity)
                        .children(log)
                        .into_any_element(),
                ));
            }
            row = row.children(files.map(spaced));
        }
        row.children(tail.into_iter().map(spaced))
            .into_any_element()
    }
}
