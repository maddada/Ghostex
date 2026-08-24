// C1 wave-3 re-cluster: command pane height/width ratio geometry and the sleeping-placeholder/delayed-send badge label paint pipeline, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn command_pane_height_ratio(ratio: f32) -> f32 {
    ratio.clamp(COMMAND_PANE_MIN_HEIGHT_RATIO, COMMAND_PANE_MAX_HEIGHT_RATIO)
}

pub(crate) fn command_pane_width_ratio(ratio: f32) -> f32 {
    ratio.clamp(COMMAND_PANE_MIN_WIDTH_RATIO, COMMAND_PANE_MAX_WIDTH_RATIO)
}

pub(crate) fn command_pane_default_height_px_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> f32 {
    /*
    CDXC:GPUICommandPane 2026-06-25-11:29:
    The GPUI command-pane default-height reader mirrors shared Settings normalization: accept only JSON numbers, round to whole pixels, clamp to 40px-600px, and use the 125px product default for missing or malformed values. This keeps app startup, missing shell-state restore, and resize-rail reset aligned with the macOS command pane without persisting a second setting.
    */
    settings
        .object()
        .get("commandsPanelDefaultHeightPx")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.round() as f32)
        .unwrap_or(COMMAND_PANE_DEFAULT_HEIGHT_PX)
        .clamp(
            COMMAND_PANE_MIN_DEFAULT_HEIGHT_PX,
            COMMAND_PANE_MAX_DEFAULT_HEIGHT_PX,
        )
}

pub(crate) fn gpui_click_to_wake_sleeping_sessions_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> bool {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
    GPUI Agents and command tab selection must honor the shared macOS `clickToWakeSleepingSessions` setting. Missing or malformed settings default to true, where tab selection keeps a sleeping placeholder cold; strict false makes selecting a sleeping tab wake it immediately.
    */
    settings
        .object()
        .get("clickToWakeSleepingSessions")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

pub(crate) fn command_pane_click_to_wake_sleeping_sessions_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> bool {
    gpui_click_to_wake_sleeping_sessions_from_shared_settings(settings)
}

pub(crate) fn command_pane_sleeping_tab_selection_wake_target(
    command_pane: &CommandPaneModel,
    group_id: CommandPaneGroupId,
    session_id: CommandSessionId,
    click_to_wake_enabled: bool,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandTabWake 2026-06-27-04:25:
    Command-tab selection has exactly one eager wake case: strict `clickToWakeSleepingSessions: false` on a live sleeping tab in the clicked command group. Default click-to-wake keeps the selected sleeping tab parked so the visible body placeholder owns the later wake affordance; stale group/session ids must not fall back to focused command state.
    */
    if click_to_wake_enabled {
        return None;
    }
    let leaf = command_pane.find_leaf(group_id)?;
    if !leaf.tab_group.has_session(session_id) {
        return None;
    }
    command_pane
        .session(session_id)
        .is_some_and(|session| session.is_sleeping)
        .then_some((group_id, session_id))
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label(
    active_session_is_sleeping: bool,
    click_to_wake_enabled: bool,
) -> Option<&'static str> {
    (active_session_is_sleeping && click_to_wake_enabled)
        .then_some(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL)
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_is_private_data_safe(
    label: &str,
) -> bool {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    The paint path may render only the fixed native wake affordance. Keep this guard at the writer boundary so future canvas callers cannot paint command text, session titles, paths, URLs, tokens, or terminal content into the sleeping command body.
    */
    label == COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelFrame {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelLine {
    pub(crate) text: String,
    pub(crate) measured_width: f32,
}

pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelPaintState {
    pub(crate) frame: CommandPaneSleepingPlaceholderWakeLabelFrame,
    pub(crate) label_lines: Vec<gpui::ShapedLine>,
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_max_size(
    body_width: f32,
    body_height: f32,
) -> Option<(f32, f32)> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Match native body-bound visibility exactly: the sleeping wake label has no geometry when the body cannot provide positive `body.width - 8` and `body.height - 16` label limits.
    */
    if !body_width.is_finite() || !body_height.is_finite() {
        return None;
    }

    let max_label_width =
        body_width - (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_HORIZONTAL_PADDING * 2.0);
    let max_label_height =
        body_height - (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_VERTICAL_PADDING * 2.0);
    (max_label_width > 0.0 && max_label_height > 0.0).then_some((max_label_width, max_label_height))
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_frame(
    body_width: f32,
    body_height: f32,
    measured_width: f32,
    measured_height: f32,
) -> Option<CommandPaneSleepingPlaceholderWakeLabelFrame> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Native centers a measured label frame inside the command body after applying the exact AppKit clamps: width is ceil(measured width)+8 with a 1px floor, height is ceil(measured height) with an 18px floor, and both are clamped to the body-derived max label size.
    */
    if !measured_width.is_finite()
        || !measured_height.is_finite()
        || measured_width < 0.0
        || measured_height < 0.0
    {
        return None;
    }

    let (max_label_width, max_label_height) =
        command_pane_sleeping_placeholder_wake_label_max_size(body_width, body_height)?;
    let label_width = (measured_width.ceil()
        + (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_HORIZONTAL_PADDING * 2.0))
        .max(1.0)
        .min(max_label_width);
    let label_height = measured_height
        .ceil()
        .max(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT)
        .min(max_label_height);

    Some(CommandPaneSleepingPlaceholderWakeLabelFrame {
        x: (body_width - label_width) / 2.0,
        y: (body_height - label_height) / 2.0,
        width: label_width,
        height: label_height,
    })
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_char_wrap_lines(
    label: &str,
    max_label_width: f32,
    mut measure: impl FnMut(&str) -> Option<f32>,
) -> Option<Vec<CommandPaneSleepingPlaceholderWakeLabelLine>> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    AppKit uses character wrapping for the wake label, not word wrapping. Split the fixed label by measured character fit so narrow command bodies break within words exactly because the body bounds require it.
    */
    if !command_pane_sleeping_placeholder_wake_label_is_private_data_safe(label)
        || !max_label_width.is_finite()
        || max_label_width <= 0.0
    {
        return None;
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0.0;

    for character in label.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        let candidate_width = measure(&candidate)?;
        if !candidate_width.is_finite() || candidate_width < 0.0 {
            return None;
        }

        if !current.is_empty() && candidate_width > max_label_width {
            lines.push(CommandPaneSleepingPlaceholderWakeLabelLine {
                text: current,
                measured_width: current_width,
            });
            current = character.to_string();
            current_width = measure(&current)?;
            if !current_width.is_finite() || current_width < 0.0 {
                return None;
            }
        } else {
            current = candidate;
            current_width = candidate_width;
        }
    }

    if !current.is_empty() {
        lines.push(CommandPaneSleepingPlaceholderWakeLabelLine {
            text: current,
            measured_width: current_width,
        });
    }

    (!lines.is_empty()).then_some(lines)
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_text_run(len: usize) -> gpui::TextRun {
    let mut font = gpui::font(".SystemUIFont");
    font.weight = FontWeight::MEDIUM;
    gpui::TextRun {
        len,
        font,
        color: command_pane_sleeping_placeholder_wake_label_color(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_shape_line(
    label: &str,
    window: &mut Window,
) -> gpui::ShapedLine {
    window.text_system().shape_line(
        gpui::SharedString::from(label.to_string()),
        px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_FONT_SIZE),
        &[command_pane_sleeping_placeholder_wake_label_text_run(
            label.len(),
        )],
        None,
    )
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_prepaint(
    bounds: Bounds<Pixels>,
    label: &'static str,
    window: &mut Window,
) -> Option<CommandPaneSleepingPlaceholderWakeLabelPaintState> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Runtime wake-label layout must come from this paint pass's exact command-body bounds. Measure the fixed label with 13px medium system text, character-wrap within `body.width - 8`, clamp the centered frame to native limits, and produce no paint state when native would hide it.
    */
    if !command_pane_sleeping_placeholder_wake_label_is_private_data_safe(label) {
        return None;
    }

    let body_width = bounds.size.width.as_f32();
    let body_height = bounds.size.height.as_f32();
    let (max_label_width, _) =
        command_pane_sleeping_placeholder_wake_label_max_size(body_width, body_height)?;
    let wrapped_lines = command_pane_sleeping_placeholder_wake_label_char_wrap_lines(
        label,
        max_label_width,
        |line| {
            Some(
                command_pane_sleeping_placeholder_wake_label_shape_line(line, window)
                    .width()
                    .as_f32(),
            )
        },
    )?;
    let measured_width = wrapped_lines
        .iter()
        .map(|line| line.measured_width)
        .fold(0.0, f32::max);
    let measured_height =
        wrapped_lines.len() as f32 * COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT;
    let frame = command_pane_sleeping_placeholder_wake_label_frame(
        body_width,
        body_height,
        measured_width,
        measured_height,
    )?;
    let label_lines = wrapped_lines
        .iter()
        .map(|line| command_pane_sleeping_placeholder_wake_label_shape_line(&line.text, window))
        .collect();

    Some(CommandPaneSleepingPlaceholderWakeLabelPaintState { frame, label_lines })
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_paint(
    body_bounds: Bounds<Pixels>,
    paint_state: CommandPaneSleepingPlaceholderWakeLabelPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let frame = paint_state.frame;
    let label_bounds = Bounds::new(
        gpui::point(
            body_bounds.left() + px(frame.x),
            body_bounds.top() + px(frame.y),
        ),
        size(px(frame.width), px(frame.height)),
    );
    let measured_height = paint_state.label_lines.len() as f32
        * COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT;
    let mut line_origin = gpui::point(
        label_bounds.left(),
        label_bounds.top() + px((frame.height - measured_height).max(0.0) / 2.0),
    );

    window.with_content_mask(
        Some(ContentMask {
            bounds: label_bounds,
        }),
        |window| {
            for line in paint_state.label_lines {
                let _ = line.paint(
                    line_origin,
                    px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT),
                    gpui::TextAlign::Center,
                    Some(px(frame.width)),
                    window,
                    cx,
                );
                line_origin.y += px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT);
            }
        },
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneDelayedSendBadgeFrame {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

pub(crate) struct CommandPaneDelayedSendBadgePaintState {
    pub(crate) frame: CommandPaneDelayedSendBadgeFrame,
    pub(crate) label_line: gpui::ShapedLine,
}

pub(crate) fn command_pane_delayed_send_badge_label_is_private_data_safe(label: &str) -> bool {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-27-00:07:
    The body badge label is generated from a runtime countdown only. Keep the canvas writer bounded to the countdown grammar so future callers cannot accidentally paint command text, titles, paths, URLs, or terminal content into the terminal body.
    */
    let mut parts = label.split(':');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(second) = parts.next() else {
        return false;
    };
    let third = parts.next();
    if parts.next().is_some() {
        return false;
    }

    let two_digit_component =
        |component: &str| component.len() == 2 && component.bytes().all(|b| b.is_ascii_digit());
    match third {
        Some(seconds) => {
            (2..=3).contains(&first.len())
                && first.bytes().all(|b| b.is_ascii_digit())
                && two_digit_component(second)
                && two_digit_component(seconds)
        }
        None => two_digit_component(first) && two_digit_component(second),
    }
}

pub(crate) fn command_pane_delayed_send_badge_fitting_size(
    label_text_width: f32,
) -> Option<(f32, f32)> {
    if !label_text_width.is_finite() || label_text_width <= 0.0 {
        return None;
    }

    Some((
        label_text_width.ceil() + COMMAND_PANE_DELAYED_SEND_BADGE_TOTAL_HORIZONTAL_PADDING,
        COMMAND_PANE_DELAYED_SEND_BADGE_MIN_HEIGHT,
    ))
}

pub(crate) fn command_pane_delayed_send_badge_frame(
    body_width: f32,
    body_height: f32,
    label_text_width: f32,
) -> Option<CommandPaneDelayedSendBadgeFrame> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-19:13:
    Keep GPUI's Delayed Send body-badge geometry tied to native's exact terminal body contract: no badge for tiny bodies, 60px total horizontal fitting padding, centered placement, and width/height clamps based only on the current body rectangle. Do not substitute command-group bounds or retained layout maps when same-pass body bounds are unavailable.
    */
    if !body_width.is_finite()
        || !body_height.is_finite()
        || body_width <= COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_WIDTH
        || body_height <= COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_HEIGHT
    {
        return None;
    }

    let (fitting_width, fitting_height) =
        command_pane_delayed_send_badge_fitting_size(label_text_width)?;
    let badge_width = fitting_width
        .min((body_width - COMMAND_PANE_DELAYED_SEND_BADGE_BODY_WIDTH_CLAMP_INSET).max(0.0));
    let badge_height = fitting_height
        .min((body_height - COMMAND_PANE_DELAYED_SEND_BADGE_BODY_HEIGHT_CLAMP_INSET).max(0.0));
    let x = (body_width / 2.0 - badge_width / 2.0)
        .max(0.0)
        .min(body_width - badge_width);
    let y = (body_height / 2.0 - badge_height / 2.0)
        .max(0.0)
        .min(body_height - badge_height);

    Some(CommandPaneDelayedSendBadgeFrame {
        x,
        y,
        width: badge_width,
        height: badge_height,
    })
}

pub(crate) fn command_pane_delayed_send_badge_prepaint(
    bounds: Bounds<Pixels>,
    label: String,
    window: &mut Window,
) -> Option<CommandPaneDelayedSendBadgePaintState> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-27-00:07:
    Runtime badge layout must come from this paint pass's command-body bounds. Shape the private countdown first, derive the native badge frame from the shaped width and exact body size, and return no paint state when native would hide the badge.
    */
    if !command_pane_delayed_send_badge_label_is_private_data_safe(&label) {
        return None;
    }

    let label_len = label.len();
    let label_line = window.text_system().shape_line(
        gpui::SharedString::from(label),
        px(COMMAND_PANE_DELAYED_SEND_BADGE_FONT_SIZE),
        &[gpui::TextRun {
            len: label_len,
            font: gpui::font(COMMAND_PANE_DELAYED_SEND_BADGE_FONT_FAMILY).bold(),
            color: command_pane_delayed_send_badge_text_color(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let frame = command_pane_delayed_send_badge_frame(
        bounds.size.width.as_f32(),
        bounds.size.height.as_f32(),
        label_line.width().as_f32(),
    )?;

    Some(CommandPaneDelayedSendBadgePaintState { frame, label_line })
}

pub(crate) fn command_pane_delayed_send_badge_paint(
    body_bounds: Bounds<Pixels>,
    paint_state: CommandPaneDelayedSendBadgePaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let frame = paint_state.frame;
    let badge_bounds = Bounds::new(
        gpui::point(
            body_bounds.left() + px(frame.x),
            body_bounds.top() + px(frame.y),
        ),
        size(px(frame.width), px(frame.height)),
    );
    window.paint_quad(gpui::quad(
        badge_bounds,
        px(COMMAND_PANE_DELAYED_SEND_BADGE_CORNER_RADIUS),
        command_pane_delayed_send_badge_background_color(),
        px(1.0),
        command_pane_delayed_send_badge_border_color(),
        gpui::BorderStyle::Solid,
    ));

    let label_origin = gpui::point(
        badge_bounds.left(),
        badge_bounds.top() + px((frame.height - COMMAND_PANE_DELAYED_SEND_BADGE_LINE_HEIGHT) / 2.0),
    );
    window.with_content_mask(
        Some(ContentMask {
            bounds: badge_bounds,
        }),
        |window| {
            let _ = paint_state.label_line.paint(
                label_origin,
                px(COMMAND_PANE_DELAYED_SEND_BADGE_LINE_HEIGHT),
                gpui::TextAlign::Center,
                Some(px(frame.width)),
                window,
                cx,
            );
        },
    );
}

pub(crate) fn command_pane_default_height_ratio_for_default_height_px(
    default_height_px: f32,
    content_height: f32,
) -> f32 {
    command_pane_height_ratio(default_height_px / content_height.max(1.0))
}

pub(crate) fn command_pane_content_height(window: &Window) -> f32 {
    (window.bounds().size.height.as_f32() - TITLEBAR_HEIGHT).max(1.0)
}

pub(crate) fn command_pane_workspace_width(
    window: &Window,
    sidebar_width: f32,
    sidebar_collapsed: bool,
) -> f32 {
    /*
    CDXC:GPUICommandPaneSide 2026-08-16:
    Collapsing the sidebar removes both the sidebar and its divider from the
    body row without mutating the saved width, so the workspace really does
    own the whole window width there. The right dock sizes its column from
    this number, so it has to follow that collapse instead of subtracting a
    sidebar that is not on screen.
    */
    let sidebar_chrome_width = if gpui_sidebar_chrome_visible(sidebar_collapsed) {
        sidebar_width + SIDEBAR_DIVIDER_WIDTH
    } else {
        0.0
    };
    (window.bounds().size.width.as_f32() - sidebar_chrome_width).max(0.0)
}

pub(crate) fn command_pane_panel_chrome_width(workspace_width: f32, floating: bool) -> f32 {
    if floating {
        (workspace_width - COMMAND_PANE_FLOATING_MARGIN * 2.0).max(0.0)
    } else {
        workspace_width.max(0.0)
    }
}

pub(crate) fn command_pane_owner_content_width(panel_chrome_width: f32) -> f32 {
    (panel_chrome_width - COMMAND_PANE_OUTER_CONTENT_RIGHT_INSET).max(0.0)
}

pub(crate) fn command_pane_height_for_ratio(ratio: f32, content_height: f32) -> f32 {
    command_pane_height_ratio(ratio) * content_height.max(1.0)
}

pub(crate) fn command_pane_width_for_ratio(ratio: f32, content_width: f32) -> f32 {
    command_pane_width_ratio(ratio) * content_width.max(1.0)
}

pub(crate) fn command_pane_resize_drag_height_ratio(
    drag: CommandPaneResizeDragState,
    current_y: f32,
    content_height: f32,
) -> f32 {
    /*
    CDXC:GPUICommandPaneResize 2026-06-25-19:13:
    Native `beginCommandsPanelResize` stores the command panel's absolute start height and start Y, then `continueCommandsPanelResize` applies one signed pointer delta and clamps the resulting ratio to 5%-90%.
    GPUI pointer Y is top-origin, so visual upward motion is `start_y - current_y`; keep that conversion in one helper so drag handling cannot regress the AppKit sign/clamp contract.
    */
    let content_height = content_height.max(1.0);
    let upward_delta = drag.start_position - current_y;
    command_pane_height_ratio((drag.start_extent + upward_delta) / content_height)
}

pub(crate) fn command_pane_resize_drag_width_ratio(
    drag: CommandPaneResizeDragState,
    current_x: f32,
    content_width: f32,
) -> f32 {
    // The right-docked pane grows as the divider moves left, so leftward
    // pointer motion is `start_position - current_x`; the same one-helper rule
    // as the bottom rail keeps the sign and clamp contract in one place.
    let content_width = content_width.max(1.0);
    let leftward_delta = drag.start_position - current_x;
    command_pane_width_ratio((drag.start_extent + leftward_delta) / content_width)
}

pub(crate) fn command_pane_floating_height_for_ratio(ratio: f32, content_height: f32) -> f32 {
    /*
    CDXC:GPUICommandPaneFloating 2026-06-25-18:07:
    Native resolves floating command-panel height by first applying the normal command-panel ratio clamp, then capping the frame to workspace height minus the reserved collapsed strip and two floating margins. Match that cap so the floating panel keeps visible top/bottom breathing room instead of clipping like a pinned panel.
    */
    let requested_height = command_pane_height_for_ratio(ratio, content_height);
    let available_height =
        (content_height.max(1.0) - COMMAND_PANE_STRIP_HEIGHT - COMMAND_PANE_FLOATING_MARGIN * 2.0)
            .max(0.0);
    requested_height.min(available_height)
}
