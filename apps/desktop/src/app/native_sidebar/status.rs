use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    px, rgb, svg,
};

use super::appearance::SidebarAppearance;
use crate::app::helpers::chrome_palette::chrome_color;
use crate::app::helpers::titlebar_background;

/// CDXC:SessionStatus 2026-10-20 DECISION:
/// User: "the pink question dot is smaller than the other dots this is wrong please make all the dots same size (pink one needs to be bigger a bit)". Every status dot in the sidebar (working, attention, question, shell-running, failed) is this one size, so none of them can drift again; the blue attention dot was 7px and the pink question dot 6px before. The project header's count badges (a 6px legend dot beside a number) are not status dots.
pub(crate) const STATUS_DOT_SIZE: f32 = 8.0;

/// CDXC:SessionStatus 2026-10-23 DECISION:
/// User: "the icons on the right side are not the same horizontal alignment (not aligned to each other), pls fix". Every status indicator at a session row's right edge sits in a slot of this one width with its dot centred in it, so all kinds (working, attention, question, shell-running, failed) share one vertical centre line; the question dot used to sit in a narrower slot and drew 6px right of the others. A question row that is also working puts the orange dot left of the slot, so the pink dot stays on the line.
pub(crate) const STATUS_SLOT_WIDTH: f32 = 19.0;

/// CDXC:RemoteMachines 2026-11-12 DECISION:
/// User: the computer icons must be "always in the same column across rows" (a row with a status dot, one with "4h" and one with "15m" had them at different places). While a row draws its computer's icon (computer_mark.rs), what follows it (the status dot, the last-active age, the question, approval or folded dots) sits in a right-aligned slot at least this wide, which fits the widest usual age ("59m", "23h", "12d") and a status slot, so the icon keeps one distance from the row's right edge. A longer timer still shows whole and pushes the icon left.
pub(crate) const TRAILING_SLOT_WIDTH: f32 = 28.0;

/// CDXC:SessionStatus 2026-09-21 DECISION:
/// User: "I feel we have multiple degrees for the orange color status \"working\" in the sidebar pls unify all of them on this new one you picked", so every working dot, count, and badge in the sidebar uses this one orange (the Spaces badge orange, 20% darker than the old 0xf8ad07 so a white digit stays readable on it).
pub(crate) const WORKING_COLOR: u32 = 0xc68a06;

/// CDXC:SessionStatus 2026-09-19 DECISION:
/// User: "make the working indicator just the orange dot without animation. i dont mind. like the one we have in the SESSIONS header", so a working session shows the same static 8px orange dot the section headers draw.
/// This supersedes the 2026-09-17 decision that brought the animated working spinner back.
/// CDXC:SessionStatus 2026-09-22 DECISION:
/// User: "make shell running just show a #B4B8BF color dot in the sidebar in dark mode and a darker dot in light mode", so a row whose agent still has a background shell or monitor running after its turn draws a grey dot in place of the time; working and attention keep precedence over it. Headers (sections, projects, collections, Spaces, machines) draw the same grey when they have no working session but have a grey-dot session.
pub(crate) fn background_work_color() -> gpui::Rgba {
    chrome_color(0xb4b8bf, 0x6b7078)
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: "we must show a red dot on the session in the sidebar if we fail" to apply the model change picked in chat. The red dot wins over working, attention and the grey shell dot, because the messages behind the change are held until the user picks again; the row's tooltip carries the reason.
/// SEE-ALSO: server/src/session_chat_model_selection_alert.rs.
/// CDXC:SessionStatus 2026-10-14 WHY: a session whose program reported `error` over OSC 7501 (while those reports drive its status) draws the same red dot until the user has seen it; the row's tooltip carries the program's message.
pub(crate) const FAILED_COLOR: u32 = 0xef4444;

/// The blue dot of a session whose finished turn the user has not seen (`attention`).
pub(crate) const ATTENTION_COLOR: u32 = 0x95d7f6;

/// The pink dot of a session with an unanswered question card.
pub(crate) const QUESTION_COLOR: u32 = 0xf472b6;

/// CDXC:SessionStatus 2026-10-27 DECISION:
/// User: "Please for permissions let's show a new red dot so user knows stuff is stuck and waiting on him to approve". Then (Q2) the section, project, collection, Space and machine headers get "a red dot" of their own for those sessions, separate from the blue needs-you count (gx-core `SessionRow::needs_you`). A session waiting for approval (gxserver's `awaitingApproval`) draws this red dot ahead of every other status (question, working, attention, shell running, failed), and an orchestrator above it draws its crew icon and number in it. It is the app's one error red (`FAILED_COLOR`), which reads on the dark and the light sidebar alike.
pub(crate) const APPROVAL_COLOR: u32 = FAILED_COLOR;

/// The red dot of a session waiting for approval, in the same slot as every other indicator.
pub(crate) fn approval_indicator(scale: f32) -> AnyElement {
    div()
        .w(px(STATUS_SLOT_WIDTH * scale))
        .h(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(STATUS_DOT_SIZE * scale))
                .rounded_full()
                .bg(rgb(APPROVAL_COLOR)),
        )
        .into_any_element()
}

/// Compact muted-amber clock + countdown (`12m`, `3h 12m`, `45s`) for a session on a usage or
/// quota limit (Q4460/Q4461, Q4472). Drawn beside the red dot while `blocked`, and alone while
/// `waiting`.
pub(crate) fn quota_countdown_badge(
    quota_resets_at: Option<&str>,
    quota_wait_state: Option<&str>,
    scale: f32,
) -> Option<AnyElement> {
    if !matches!(quota_wait_state, Some("blocked" | "waiting")) {
        return None;
    }
    let now_ms = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default();
    let label = quota_resets_at
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis() - now_ms)
        .filter(|remaining| *remaining > 0)
        .map(|remaining| {
            let secs = (remaining + 999) / 1000;
            if secs < 60 {
                format!("{secs}s")
            } else if secs < 3600 {
                format!("{}m", (secs + 59) / 60)
            } else {
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                if mins == 0 {
                    format!("{hours}h")
                } else {
                    format!("{hours}h {mins}m")
                }
            }
        });
    // When blocked with no known reset timer, the red dot alone already communicates blocked state.
    if quota_wait_state == Some("blocked") && label.is_none() {
        return None;
    }
    let amber = chrome_color(0xfbbf24, 0xb45309);
    let mut row = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(3.0 * scale))
        .text_size(px(11.5 * scale))
        .text_color(amber)
        .child(
            svg()
                .path("titlebar/clock.svg")
                .size(px(11.5 * scale))
                .flex_shrink_0()
                .text_color(amber),
        );
    if let Some(label) = label {
        row = row.child(label);
    }
    Some(row.into_any_element())
}

pub(crate) fn activity_indicator(
    activity: &str,
    has_background_work: bool,
    model_selection_failed: bool,
    scale: f32,
) -> Option<AnyElement> {
    let indicator = match activity {
        _ if model_selection_failed => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(FAILED_COLOR))
            .into_any_element(),
        "working" => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(WORKING_COLOR))
            .into_any_element(),
        "attention" => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(0x95d7f6))
            .into_any_element(),
        _ if has_background_work => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(background_work_color())
            .into_any_element(),
        _ => return None,
    };
    Some(
        div()
            .w(px(STATUS_SLOT_WIDTH * scale))
            .h(px(16.0 * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(indicator)
            .into_any_element(),
    )
}

/// What a tile's status dot sits on, so each dot's ring matches it: the colour at rest, the colour while the pointer is over the owner, and the owner's GPUI group name (the owner calls `.group(..)` with the same name).
pub(crate) struct DotBackdrop {
    rest: Hsla,
    hover: Hsla,
    pub(crate) group: SharedString,
}

impl DotBackdrop {
    /// `wash` is the owner's own fill over the sidebar (a selected Space's, say), `None` for the bare sidebar.
    pub(crate) fn new(
        appearance: &SidebarAppearance,
        wash: Option<Hsla>,
        group: impl Into<SharedString>,
    ) -> Self {
        let base = titlebar_background();
        Self {
            rest: wash.map_or(base, |wash| base.blend(wash)),
            hover: base.blend(appearance.hover),
            group: group.into(),
        }
    }
}

/// CDXC:Spaces 2026-11-20 DECISION:
/// User picked A of the one-dot page (Q4264): a Space tile and a computer tile show at most one
/// status dot, in the colour of their most urgent state, and hovering the tile lists the counts.
/// "blocker is higher than needs you": red (an approval, or a failed, blocked or stuck session)
/// outranks pink (a question) and blue (an unseen finished turn), which outrank amber (working)
/// and grey (a shell still running). gx-core counts each session once under that order
/// (`TileStatus`), so the hover counts add up. The dot keeps its 1px ring in the colour behind it;
/// its size and place since 2026-11-21 are at [`tile_status_dot`]. This supersedes the 2026-10-20 overlapping stack on these tiles; a
/// folded coordinator row and the chat's pills keep theirs (`folded_status_indicator`,
/// `sized_overlapped_dots`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TileCounts {
    pub(crate) approval: usize,
    pub(crate) failed: usize,
    pub(crate) question: usize,
    pub(crate) attention: usize,
    pub(crate) working: usize,
    pub(crate) background_work: usize,
}

impl TileCounts {
    pub(crate) fn has_status(&self) -> bool {
        self.color().is_some()
    }

    /// The dot's colour: the most urgent state with a session in it.
    fn color(&self) -> Option<Hsla> {
        if self.approval + self.failed > 0 {
            Some(rgb(FAILED_COLOR).into())
        } else if self.question > 0 {
            Some(rgb(QUESTION_COLOR).into())
        } else if self.attention > 0 {
            Some(rgb(ATTENTION_COLOR).into())
        } else if self.working > 0 {
            Some(rgb(WORKING_COLOR).into())
        } else if self.background_work > 0 {
            Some(background_work_color().into())
        } else {
            None
        }
    }

    /// The tooltip's lines under the tile's name, the most urgent first, one state a line.
    pub(crate) fn tooltip_lines(&self) -> Vec<String> {
        let line = |count: usize, one: &str, many: &str| {
            (count > 0).then(|| format!("{count} {}", if count == 1 { one } else { many }))
        };
        [
            line(
                self.approval,
                "waiting for approval",
                "waiting for approval",
            ),
            line(self.failed, "blocked or failed", "blocked or failed"),
            line(self.question, "question", "questions"),
            line(self.attention, "finished", "finished"),
            line(self.working, "working", "working"),
            line(self.background_work, "shell running", "shells running"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// `heading`, then [`Self::tooltip_lines`] under it.
    pub(crate) fn tooltip(&self, heading: &str) -> String {
        std::iter::once(heading.to_string())
            .chain(self.tooltip_lines())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// How wide a Space or computer tile's status dot is, and how far it sits in from the tile's top
/// and right edges (outer edges, the 1pt hairline included), before the zoom.
pub(crate) const TILE_DOT_SIZE: f32 = 4.0;
pub(crate) const TILE_DOT_INSET: f32 = 3.0;

/// A Space or computer tile's one status dot ([`TileCounts`]), placed in the tile's top-right
/// corner, with the 1px ring of what it sits on; `None` while nothing in it has a state. The tile
/// must be `relative()`.
///
/// CDXC:Spaces 2026-11-21 DECISION:
/// User, of the 8pt dot hanging over the bottom edge of the Space tiles: "i wanna make these dots
/// smaller and fit inside the cards i think top right?". From the options page they picked A, the
/// top-right corner, at the size of C: a 4pt dot inside the tile, 3pt in from its top and right
/// edges (Q4353). The computer tiles and the folded computers button draw the same dot in the same
/// place, so this computer's tile no longer lifts its icon over it, and another computer's update
/// arrow moves to the top-left corner while the dot is shown. Still one dot, the most urgent
/// colour (Q4264). Supersedes the 2026-11-11 bottom-centre place (machines.rs) and the
/// 2026-09-22 place under a Space's icon (selectors.rs).
pub(crate) fn tile_status_dot(
    counts: TileCounts,
    scale: f32,
    backdrop: DotBackdrop,
) -> Option<gpui::Div> {
    Some(corner_dot(
        counts.color()?,
        scale,
        backdrop.rest,
        Some((backdrop.hover, backdrop.group)),
    ))
}

/// A 4pt dot of `color` in its owner's top-right corner (see [`tile_status_dot`]), with a 1px ring
/// in `ring`, the colour behind it (`hover`: the colour while the pointer is over the owner's
/// group). The owner must be `relative()`.
///
/// CDXC:AgentProviders 2026-11-21 DECISION:
/// User: "change how we indicate that an account has issues: instead of the text name of the
/// account becoming yellow, add a small dot top right of the account's icon (like the one we have
/// on top right of space icon now)". An account's icon or mark (the sidebar usage meters, the chat's
/// model pill, Settings > Accounts' list, the phone's Usage pane) draws this very dot, in the
/// amber its letters used to turn (`needs_sign_in_amber`), so the dot is the Space tile's in size,
/// inset and ring.
pub(crate) fn corner_dot(
    color: Hsla,
    scale: f32,
    ring: Hsla,
    hover: Option<(Hsla, SharedString)>,
) -> gpui::Div {
    // Absolute insets count from the owner's outer edge, and the ring adds 1pt round the dot.
    let offset = (TILE_DOT_INSET - 1.0) * scale;
    div()
        .absolute()
        .top(px(offset))
        .right(px(offset))
        .child(sized_overlapped_dots(
            vec![color],
            TILE_DOT_SIZE,
            scale,
            ring,
            hover,
        ))
}

/// The colours of a folded coordinator's overlapping stack, back to front, least urgent first: grey
/// shell, amber working, blue finished, pink question, and the red blocker (approval, failed or
/// stuck) in front.
///
/// CDXC:SessionStatus 2026-11-20 DECISION:
/// User, of a folded coordinator row whose amber working dot covered its pink one: "make question
/// and blocker higher priority here than the working indicator". Every overlapping stack puts the
/// most urgent dot in front, in the order the Space and computer tiles pick their one dot
/// (`TileCounts::color`): red > pink > blue > amber > grey. Supersedes the 2026-10-23 order with
/// the pink dot at the back.
fn stack_colors(
    working: bool,
    attention: bool,
    background_work: bool,
    question: bool,
    approval: bool,
) -> Vec<Hsla> {
    [
        (!working && background_work, background_work_color().into()),
        (working, rgb(WORKING_COLOR).into()),
        (!working && attention, rgb(ATTENTION_COLOR).into()),
        (question, rgb(QUESTION_COLOR).into()),
        (approval, rgb(APPROVAL_COLOR).into()),
    ]
    .into_iter()
    .filter(|(shown, _)| *shown)
    .map(|(_, color)| color)
    .collect()
}

/// A folded coordinator's overlapping status dots (gx-core `RowNesting::folded_status`), in a session row's status slot; `ring` is the row's fill.
pub(crate) fn folded_status_indicator(
    working: bool,
    attention: bool,
    background_work: bool,
    question: bool,
    approval: bool,
    scale: f32,
    ring: Hsla,
) -> AnyElement {
    div()
        .min_w(px(STATUS_SLOT_WIDTH * scale))
        .h(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(overlapped_dots(
            stack_colors(working, attention, background_work, question, approval),
            scale,
            ring,
            None,
        ))
        .into_any_element()
}

/// The overlapped dots of a status stack, each with a 1px ring in `ring` (what the stack sits on); `hover` is the ring colour while the pointer is over the owner group.
fn overlapped_dots(
    dots: Vec<Hsla>,
    scale: f32,
    ring: Hsla,
    hover: Option<(Hsla, SharedString)>,
) -> gpui::Div {
    sized_overlapped_dots(dots, STATUS_DOT_SIZE, scale, ring, hover)
}

/// [`overlapped_dots`] at another dot size (`dot`, unscaled): each dot overlaps the one before it by
/// half, front to the right, with the 1px ring. The chat's pills draw their corner dots with it
/// (native_chat/chat_pills.rs), so the two looks cannot drift apart.
pub(crate) fn sized_overlapped_dots(
    dots: Vec<Hsla>,
    dot: f32,
    scale: f32,
    ring: Hsla,
    hover: Option<(Hsla, SharedString)>,
) -> gpui::Div {
    let border = 1.0 * scale;
    let size = dot * scale + 2.0 * border;
    let step = dot / 2.0 * scale;
    let width = dots.len().saturating_sub(1) as f32 * step + size;
    div()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .h(px(size))
        .children(dots.into_iter().enumerate().map(|(index, color)| {
            div()
                .absolute()
                .left(px(index as f32 * step))
                .top_0()
                .size(px(size))
                .rounded_full()
                .border(px(border))
                .border_color(ring)
                .when_some(hover.clone(), |dot, (hover, group)| {
                    dot.group_hover(group, move |dot| dot.border_color(hover))
                })
                .bg(color)
        }))
}

/// CDXC:SessionStatus 2026-10-23 DECISION:
/// User: "also 1 more thing if there's question + working or question + done then show those 2 dots overlapping like we show it at the bottom of the space button". A row with a pending question draws the pink dot, and when the session is also working (orange) or done (blue attention) a second dot overlaps it, a 1px ring in `ring` (the row's fill) between them. The pair is centred in the same `STATUS_SLOT_WIDTH` slot as every other indicator. Since 2026-11-20 the pink dot is the front one, the other behind and to the left (`stack_colors`: the most urgent dot in front).
pub(crate) fn question_indicator(activity: &str, scale: f32, ring: Hsla) -> AnyElement {
    let behind: Option<Hsla> = match activity {
        "working" => Some(rgb(WORKING_COLOR).into()),
        "attention" => Some(rgb(ATTENTION_COLOR).into()),
        _ => None,
    };
    let dots = match behind {
        Some(behind) => {
            overlapped_dots(vec![behind, rgb(QUESTION_COLOR).into()], scale, ring, None)
                .into_any_element()
        }
        None => div()
            .size(px(STATUS_DOT_SIZE * scale))
            .rounded_full()
            .bg(rgb(QUESTION_COLOR))
            .into_any_element(),
    };
    div()
        .w(px(STATUS_SLOT_WIDTH * scale))
        .h(px(16.0 * scale))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(dots)
        .into_any_element()
}

/// The ring colour for a row's overlapped dots: the sidebar background under the row's own fill, as it looks at rest or while the pointer is over the row.
pub(crate) fn session_row_ring(
    appearance: &SidebarAppearance,
    highlighted: bool,
    visible: bool,
    question_fill: Option<gpui::Rgba>,
    hovered: bool,
) -> Hsla {
    let base = titlebar_background();
    if let Some(fill) = question_fill {
        return base.blend(fill.into());
    }
    if highlighted {
        return base.blend(appearance.session_selected);
    }
    if hovered {
        return base.blend(appearance.session_hover);
    }
    if visible {
        return base.blend(appearance.visible);
    }
    base
}

pub(crate) fn completion_opacity(elapsed: std::time::Duration) -> f32 {
    let progress = (elapsed.as_secs_f32() / 3.0).min(1.0);
    let stops = [
        (0.0, 1.0),
        (0.08, 0.9),
        (0.16, 0.58),
        (0.24, 1.0),
        (0.36, 0.9),
        (0.44, 0.58),
        (0.52, 1.0),
        (0.64, 0.9),
        (0.72, 0.58),
        (0.80, 1.0),
        (1.0, 1.0),
    ];
    for pair in stops.windows(2) {
        if progress <= pair[1].0 {
            let t = (progress - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + (pair[1].1 - pair[0].1) * t;
        }
    }
    1.0
}

/// CDXC:SessionStatus 2026-09-28 DECISION:
/// User chose the soft fill, only for confirmed human input, as an Advanced setting off by default. Pending questions remain highlighted while working; generic attention is not evidence of a question. Design evidence: branch prototype/sidebar-attention-2026-09-28.
pub(crate) fn pending_question_fill(
    settings: &serde_json::Value,
    pending_question: bool,
) -> Option<gpui::Rgba> {
    (pending_question && settings["highlightPendingQuestions"].as_bool() == Some(true))
        .then(|| rgb(0xf472b6).opacity(0.12))
}
