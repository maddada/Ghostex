use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase, Window};
use std::sync::Arc;
use web_time::Instant;

/// CDXC:Spaces 2026-09-23 DECISION:
/// User: add a fade in/out to the Space switch animation so it looks nicer. The list fades out as it slides away and fades back in as the new Space slides in, each on its own ease-out/ease-in-out curve, long enough to read as a fade rather than a flash.
const EXIT_SECONDS: f32 = 0.12;
const ENTER_SECONDS: f32 = 0.24;

#[derive(Default)]
pub(crate) struct SpaceGesture {
    delta: f32,
    locked: bool,
    native_phases: bool,
    last_event: Option<Instant>,
    transition: Option<SpaceTransition>,
}

struct SpaceTransition {
    started: Instant,
    direction: f32,
    destination: Option<String>,
    phase: TransitionPhase,
    /// CDXC:Spaces 2026-09-18 WHY:
    /// React selects the destination synchronously, so its exit fade runs straight into the enter fade. The native switch round-tripped through the service thread (until QuickJS was deleted on 2026-09-25), and waiting for the exit before asking left a blank list in between.
    /// The switch is requested the moment the gesture locks, while the outgoing Space keeps rendering from this frozen snapshot until its fade ends; the enter fade then starts on whatever the new Space already delivered.
    frozen: Option<Arc<NativeSidebarSnapshot>>,
}

enum TransitionPhase {
    Exit,
    Waiting,
    Enter,
    Boundary,
}

impl SpaceGesture {
    /// The outgoing Space's snapshot while its exit fade is still running.
    pub(crate) fn exiting_snapshot(&self) -> Option<&Arc<NativeSidebarSnapshot>> {
        self.transition
            .as_ref()
            .filter(|transition| matches!(transition.phase, TransitionPhase::Exit))
            .and_then(|transition| transition.frozen.as_ref())
    }

    pub(crate) fn presentation(&self) -> (f32, f32) {
        let Some(transition) = &self.transition else {
            return (0.0, 1.0);
        };
        let elapsed = transition.started.elapsed().as_secs_f32();
        match transition.phase {
            TransitionPhase::Exit => {
                let progress = (elapsed / EXIT_SECONDS).min(1.0);
                let t = bezier(progress, 0.4, 0.0, 1.0, 1.0);
                let fade = bezier(progress, 0.0, 0.0, 0.58, 1.0);
                (-12.0 * transition.direction * t, 1.0 - fade)
            }
            TransitionPhase::Waiting => (0.0, 0.0),
            TransitionPhase::Enter => {
                let progress = (elapsed / ENTER_SECONDS).min(1.0);
                let t = bezier(progress, 0.22, 1.0, 0.36, 1.0);
                let fade = bezier(progress, 0.42, 0.0, 0.58, 1.0);
                (16.0 * transition.direction * (1.0 - t), fade)
            }
            TransitionPhase::Boundary => {
                let t = bezier((elapsed / 0.15).min(1.0), 0.22, 1.0, 0.36, 1.0);
                let distance = if t < 0.45 { t / 0.45 } else { (1.0 - t) / 0.55 };
                (-5.0 * transition.direction * distance, 1.0)
            }
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn handle_native_space_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(snapshot) = self
            .native_sidebar
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.spaces_enabled)
        else {
            return;
        };
        if event.modifiers.control
            || event.modifiers.shift
            || cx.has_active_drag()
            || self.native_sidebar.menu.is_some()
        {
            return;
        }
        let gesture = &mut self.native_sidebar.space_gesture;
        let now = Instant::now();
        if event.touch_phase == TouchPhase::Started {
            gesture.native_phases = true;
            gesture.delta = 0.0;
            gesture.locked = false;
        } else if !gesture.native_phases
            && gesture
                .last_event
                .is_none_or(|last| now.duration_since(last).as_millis() >= 64)
        {
            gesture.delta = 0.0;
            gesture.locked = false;
        }
        gesture.last_event = Some(now);
        let (x, y) = match event.delta {
            ScrollDelta::Pixels(delta) => (-f32::from(delta.x), -f32::from(delta.y)),
            ScrollDelta::Lines(delta) => (-delta.x * 16.0, -delta.y * 16.0),
        };
        if x.abs() < 2.0 || x.abs() <= y.abs() * 1.25 {
            return;
        }
        window.prevent_default();
        cx.stop_propagation();
        if x.abs() < 6.0 || gesture.locked {
            return;
        }
        if x.signum() != gesture.delta.signum() {
            gesture.delta = 0.0;
        }
        gesture.delta += x;
        if gesture.delta.abs() < 44.0 {
            return;
        }
        gesture.locked = true;
        let direction = gesture.delta.signum();
        let snapshot = snapshot.clone();
        let selected = snapshot
            .spaces
            .iter()
            .position(|space| space.selected)
            .unwrap_or(0);
        let destination = if direction > 0.0 {
            snapshot.spaces.get(selected + 1)
        } else {
            selected
                .checked_sub(1)
                .and_then(|index| snapshot.spaces.get(index))
        }
        .map(|space| space.id.clone());
        self.start_native_space_transition(snapshot, direction, destination, cx);
    }

    /// CDXC:Spaces 2026-09-23 DECISION:
    /// User: clicking a Space in the Spaces row plays the same slide-and-fade the trackpad swipe plays. It slides the way a swipe to that Space would: forward for a Space to the right of the selected one, back for one to the left.
    pub(crate) fn select_native_space(&mut self, space_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self
            .native_sidebar
            .snapshot
            .clone()
            .filter(|snapshot| snapshot.spaces_enabled)
        else {
            self.dispatch_native_sidebar_ui(
                serde_json::json!({"type": "selectSpace", "spaceId": space_id}),
                cx,
            );
            return;
        };
        let selected = snapshot.spaces.iter().position(|space| space.selected);
        let target = snapshot
            .spaces
            .iter()
            .position(|space| space.id == space_id);
        match (selected, target) {
            (Some(selected), Some(target)) if selected != target => {
                let direction = if target > selected { 1.0 } else { -1.0 };
                self.start_native_space_transition(
                    snapshot,
                    direction,
                    Some(space_id.to_owned()),
                    cx,
                );
            }
            _ => self.dispatch_native_sidebar_ui(
                serde_json::json!({"type": "selectSpace", "spaceId": space_id}),
                cx,
            ),
        }
    }

    /// Selects `destination` behind the slide-and-fade, or plays the edge bounce when there is no Space that way.
    fn start_native_space_transition(
        &mut self,
        snapshot: Arc<NativeSidebarSnapshot>,
        direction: f32,
        destination: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gpui_pet_overlay_reduce_motion_enabled {
            if let Some(space_id) = destination {
                self.dispatch_native_sidebar_ui(
                    serde_json::json!({"type": "selectSpace", "spaceId": space_id}),
                    cx,
                );
            }
            return;
        }
        let phase = if destination.is_some() {
            TransitionPhase::Exit
        } else {
            TransitionPhase::Boundary
        };
        let frozen = destination.is_some().then_some(snapshot);
        self.native_sidebar.space_gesture.transition = Some(SpaceTransition {
            started: Instant::now(),
            direction,
            destination: destination.clone(),
            phase,
            frozen,
        });
        if let Some(space_id) = destination {
            self.dispatch_native_sidebar_ui(
                serde_json::json!({"type": "selectSpace", "spaceId": space_id}),
                cx,
            );
        }
        // CDXC:Spaces 2026-09-17 WHY:
        // Wheel callbacks have no current render view, so request_animation_frame panics there.
        // Notify starts the redraw; update_native_space_transition schedules subsequent frames during prepaint.
        cx.notify();
    }

    pub(crate) fn update_native_space_transition(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(transition) = self.native_sidebar.space_gesture.transition.as_mut() else {
            return;
        };
        let elapsed = transition.started.elapsed().as_secs_f32();
        let mut complete = false;
        let destination_selected = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.spaces.iter().any(|space| {
                    space.selected && Some(&space.id) == transition.destination.as_ref()
                })
            });
        match transition.phase {
            TransitionPhase::Exit if elapsed >= EXIT_SECONDS => {
                transition.frozen = None;
                if destination_selected {
                    transition.phase = TransitionPhase::Enter;
                    transition.started = Instant::now();
                } else {
                    transition.phase = TransitionPhase::Waiting;
                }
            }
            TransitionPhase::Waiting => {
                if self
                    .native_sidebar
                    .snapshot
                    .as_ref()
                    .is_none_or(|snapshot| {
                        !snapshot.spaces_enabled
                            || !snapshot
                                .spaces
                                .iter()
                                .any(|space| Some(&space.id) == transition.destination.as_ref())
                    })
                {
                    complete = true;
                }

                if destination_selected {
                    transition.phase = TransitionPhase::Enter;
                    transition.started = Instant::now();
                }
            }
            TransitionPhase::Enter if elapsed >= ENTER_SECONDS => complete = true,
            TransitionPhase::Boundary if elapsed >= 0.15 => complete = true,
            _ => {}
        }
        if complete {
            self.native_sidebar.space_gesture.transition = None;
        }
        window.request_animation_frame();
        cx.notify();
    }
}

pub(super) fn bezier(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let sample = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let mid = (low + high) / 2.0;
        if sample(mid, x1, x2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample((low + high) / 2.0, y1, y2)
}
