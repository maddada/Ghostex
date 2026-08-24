// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the status-pet surface/control colors
// and the pet overlay animation-frame/spritesheet helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use gpui::{Hsla, Image, ImageFormat, rgb};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_status_pet_surface_color() -> Hsla {
    rgb(0x111315).opacity(0.94).into()
}

pub(crate) fn gpui_status_pet_surface_border_color() -> Hsla {
    rgb(0xffffff).opacity(0.16).into()
}

pub(crate) fn gpui_status_pet_surface_text_color() -> Hsla {
    rgb(0xf4f6f8).opacity(0.92).into()
}

pub(crate) fn gpui_status_pet_control_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).opacity(0.18).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).opacity(0.18).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).opacity(0.16).into(),
    }
}

pub(crate) fn gpui_status_pet_control_hover_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).opacity(0.26).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).opacity(0.26).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).opacity(0.24).into(),
    }
}

pub(crate) fn gpui_status_pet_status_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).into(),
    }
}

pub(crate) fn gpui_pet_overlay_activity_hover_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x15242c).opacity(0.96).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0x2a2112).opacity(0.96).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x14251a).opacity(0.96).into(),
    }
}

pub(crate) fn gpui_pet_overlay_secondary_text_color() -> Hsla {
    rgb(0xd7dde3).opacity(0.68).into()
}

pub(crate) fn gpui_pet_overlay_label_background_color() -> Hsla {
    rgb(0x0b0d0f).opacity(0.72).into()
}

pub(crate) const fn gpui_pet_overlay_row_frames<const N: usize>(
    row_index: u8,
    frame_duration_ms: u64,
    final_frame_duration_ms: u64,
) -> [GpuiPetOverlayAnimationFrame; N] {
    let mut frames = [GpuiPetOverlayAnimationFrame {
        row_index,
        column_index: 0,
        duration_ms: frame_duration_ms,
    }; N];
    let mut index = 0;
    while index < N {
        frames[index] = GpuiPetOverlayAnimationFrame {
            row_index,
            column_index: index as u8,
            duration_ms: if index + 1 == N {
                final_frame_duration_ms
            } else {
                frame_duration_ms
            },
        };
        index += 1;
    }
    frames
}

pub(crate) fn gpui_pet_overlay_animation_state_for_surface(
    activities: &[GpuiPetOverlayActivityState],
    avatar_hovered: bool,
) -> GpuiPetOverlayAnimationState {
    if activities
        .iter()
        .any(|activity| activity.state == GpuiStatusIndicatorStatus::Attention)
    {
        return GpuiPetOverlayAnimationState::Review;
    }
    if activities
        .iter()
        .any(|activity| activity.state == GpuiStatusIndicatorStatus::Working)
    {
        return GpuiPetOverlayAnimationState::Running;
    }
    if avatar_hovered {
        return GpuiPetOverlayAnimationState::Jumping;
    }
    GpuiPetOverlayAnimationState::Idle
}

pub(crate) fn gpui_pet_overlay_animation_frame(
    state: GpuiPetOverlayAnimationState,
    started_at: Instant,
    now: Instant,
) -> GpuiPetOverlayAnimationFrame {
    let elapsed_ms = now
        .checked_duration_since(started_at)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    match state {
        GpuiPetOverlayAnimationState::Idle => {
            gpui_pet_overlay_loop_frame(&GPUI_PET_OVERLAY_IDLE_FRAMES, elapsed_ms, true)
        }
        GpuiPetOverlayAnimationState::Jumping => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_JUMPING_FRAMES, elapsed_ms)
        }
        GpuiPetOverlayAnimationState::Review => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_REVIEW_FRAMES, elapsed_ms)
        }
        GpuiPetOverlayAnimationState::Running => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_RUNNING_FRAMES, elapsed_ms)
        }
    }
}

pub(crate) fn gpui_pet_overlay_animation_frame_for_motion_preference(
    state: GpuiPetOverlayAnimationState,
    started_at: Instant,
    now: Instant,
    reduce_motion_enabled: bool,
) -> GpuiPetOverlayAnimationFrame {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    Reduce Motion must freeze the pet on a deterministic frame for the current semantic state while preserving the existing animated frame cadence when the OS setting is off. Keep this pure so ticker gating and frame choice can be verified without launching GPUI.
    */
    if reduce_motion_enabled {
        return gpui_pet_overlay_reduced_motion_frame(state);
    }
    gpui_pet_overlay_animation_frame(state, started_at, now)
}

pub(crate) fn gpui_pet_overlay_reduced_motion_frame(
    state: GpuiPetOverlayAnimationState,
) -> GpuiPetOverlayAnimationFrame {
    match state {
        GpuiPetOverlayAnimationState::Idle => GPUI_PET_OVERLAY_IDLE_FRAMES[0],
        GpuiPetOverlayAnimationState::Jumping => GPUI_PET_OVERLAY_JUMPING_FRAMES[0],
        GpuiPetOverlayAnimationState::Review => GPUI_PET_OVERLAY_REVIEW_FRAMES[0],
        GpuiPetOverlayAnimationState::Running => GPUI_PET_OVERLAY_RUNNING_FRAMES[0],
    }
}

pub(crate) fn gpui_pet_overlay_animation_ticker_should_run(
    pet_enabled: bool,
    reduce_motion_enabled: bool,
) -> bool {
    pet_enabled && !reduce_motion_enabled
}

pub(crate) fn gpui_pet_overlay_active_then_idle_frame(
    frames: &[GpuiPetOverlayAnimationFrame],
    elapsed_ms: u64,
) -> GpuiPetOverlayAnimationFrame {
    let active_duration = gpui_pet_overlay_animation_duration_ms(frames, false) * 3;
    if elapsed_ms < active_duration {
        return gpui_pet_overlay_loop_frame(frames, elapsed_ms, false);
    }
    gpui_pet_overlay_loop_frame(
        &GPUI_PET_OVERLAY_IDLE_FRAMES,
        elapsed_ms - active_duration,
        true,
    )
}

pub(crate) fn gpui_pet_overlay_loop_frame(
    frames: &[GpuiPetOverlayAnimationFrame],
    elapsed_ms: u64,
    slow_idle: bool,
) -> GpuiPetOverlayAnimationFrame {
    let Some(first_frame) = frames.first().copied() else {
        return GPUI_PET_OVERLAY_IDLE_FRAMES[0];
    };
    let duration = gpui_pet_overlay_animation_duration_ms(frames, slow_idle);
    if duration == 0 {
        return first_frame;
    }
    let mut remaining_ms = elapsed_ms % duration;
    for frame in frames {
        let frame_duration = gpui_pet_overlay_frame_duration_ms(*frame, slow_idle);
        if remaining_ms < frame_duration {
            return *frame;
        }
        remaining_ms = remaining_ms.saturating_sub(frame_duration);
    }
    first_frame
}

pub(crate) fn gpui_pet_overlay_animation_duration_ms(
    frames: &[GpuiPetOverlayAnimationFrame],
    slow_idle: bool,
) -> u64 {
    frames
        .iter()
        .map(|frame| gpui_pet_overlay_frame_duration_ms(*frame, slow_idle))
        .sum()
}

pub(crate) fn gpui_pet_overlay_frame_duration_ms(
    frame: GpuiPetOverlayAnimationFrame,
    slow_idle: bool,
) -> u64 {
    if slow_idle {
        frame
            .duration_ms
            .saturating_mul(GPUI_PET_OVERLAY_IDLE_SPEED_MULTIPLIER)
    } else {
        frame.duration_ms
    }
}

pub(crate) fn gpui_pet_overlay_pet_id_known(pet_id: &str) -> bool {
    gpui_pet_overlay_pet_display_name(pet_id).is_some()
}

pub(crate) fn gpui_pet_overlay_pet_display_name(pet_id: &str) -> Option<&'static str> {
    match pet_id {
        "boo" => Some("Boo"),
        "bsod" => Some("BSOD"),
        "codex" => Some("Codex"),
        "dewey" => Some("Dewey"),
        "fireball" => Some("Fireball"),
        "null-signal" => Some("Null Signal"),
        "rocky" => Some("Rocky"),
        "seedy" => Some("Seedy"),
        "stacky" => Some("Stacky"),
        _ => None,
    }
}

pub(crate) fn gpui_pet_overlay_spritesheet_image(pet_id: &str) -> Option<Arc<Image>> {
    match pet_id {
        "boo" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_BOO_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/boo-spritesheet-codexpethub-8a8161fb.webp"
            ),
        )),
        "bsod" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_BSOD_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/bsod-spritesheet-v4-BRrRVy1T.webp"
            ),
        )),
        "codex" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_CODEX_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/codex-spritesheet-v4-Bl6P89d_.webp"
            ),
        )),
        "dewey" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_DEWEY_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/dewey-spritesheet-v4-gAYk_M9g.webp"
            ),
        )),
        "fireball" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_FIREBALL_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/fireball-spritesheet-v4-BtU8R9Qp.webp"
            ),
        )),
        "null-signal" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_NULL_SIGNAL_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/null-signal-spritesheet-v4-CCoTR-8t.webp"
            ),
        )),
        "rocky" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_ROCKY_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/rocky-spritesheet-v4-3RlTi26B.webp"
            ),
        )),
        "seedy" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_SEEDY_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/seedy-spritesheet-v4-CdlE_fn9.webp"
            ),
        )),
        "stacky" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_STACKY_IMAGE,
            include_bytes!(
                "../../../../../../packages/core-ui/assets/pets/stacky-spritesheet-v4-CaUJd4fY.webp"
            ),
        )),
        _ => None,
    }
}

pub(crate) fn gpui_pet_overlay_cached_spritesheet(
    cache: &'static OnceLock<Arc<Image>>,
    bytes: &'static [u8],
) -> Arc<Image> {
    cache
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Webp, bytes.to_vec())))
        .clone()
}
