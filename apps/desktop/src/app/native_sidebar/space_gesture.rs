use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::fold_motion::bezier;
use gpui::{
    Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollDelta, ScrollEvent,
    TouchPhase, Window,
};
use std::sync::Arc;
use web_time::Instant;

/// CDXC:Spaces 2026-09-23 DECISION:
/// User: add a fade in/out to the Space switch animation so it looks nicer. The list fades out as it slides away and fades back in as the new Space slides in, each on its own ease-out/ease-in-out curve, long enough to read as a fade rather than a flash.
const EXIT_SECONDS: f32 = 0.12;
const ENTER_SECONDS: f32 = 0.24;
/// How far a sideways trackpad scroll or mouse drag travels before it switches Space.
const SWITCH_DISTANCE: f32 = 44.0;
/// How much more sideways than vertical a scroll or drag must be to count as a Space switch.
const SIDEWAYS_RATIO: f32 = 1.25;

#[derive(Default)]
pub(crate) struct SpaceGesture {
    delta: f32,
    locked: bool,
    native_phases: bool,
    last_event: Option<Instant>,
    transition: Option<SpaceTransition>,
    mouse_drag: Option<SpaceMouseDrag>,
    /// Last frame's bounds of every block in the list (projects, collections, the automations row,
    /// notices, the empty state), so a press outside all of them is a press on empty list space.
    list_blocks: Vec<Bounds<Pixels>>,
}

/// A left-button press on empty list space, which switches Space once it travels far enough sideways.
struct SpaceMouseDrag {
    origin: Point<Pixels>,
    switched: bool,
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

    pub(crate) fn set_list_blocks(&mut self, blocks: &[Bounds<Pixels>]) {
        self.list_blocks.clear();
        self.list_blocks.extend_from_slice(blocks);
    }

    pub(crate) fn is_mouse_dragging(&self) -> bool {
        self.mouse_drag.is_some()
    }

    /// Whether a click ending at `up` after a press at `down` was a sideways Space drag rather than a click.
    pub(crate) fn is_drag_click(down: Point<Pixels>, up: Point<Pixels>) -> bool {
        f32::from(up.x - down.x).abs() >= SWITCH_DISTANCE
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
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Bots mode draws no Space, so there is nothing to swipe between.
        let Some(snapshot) = self
            .native_sidebar
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.spaces_enabled && !snapshot.bots_mode)
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
        if x.abs() < 2.0 || x.abs() <= y.abs() * SIDEWAYS_RATIO {
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
        if gesture.delta.abs() < SWITCH_DISTANCE {
            return;
        }
        gesture.locked = true;
        let direction = gesture.delta.signum();
        let snapshot = snapshot.clone();
        self.step_native_space(snapshot, direction, cx);
    }

    /// CDXC:Spaces 2026-10-06 DECISION:
    /// User: "I want grabbing on an empty area with the mouse in the scroll area of the sidebar in the GPUI app and moving the mouse left/right to do the same action as scrolling sideways with the trackpad (switch 1 time to the next space per drag and move)". A left press on list space outside every project, collection and row arms the drag; once it has travelled the trackpad's distance, mostly sideways, it switches Space once like a swipe (dragging left goes where swiping left goes) and does nothing more until the button is released.
    pub(crate) fn begin_native_space_mouse_drag(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let spaces_shown = self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.spaces_enabled && !snapshot.bots_mode);
        let gesture = &mut self.native_sidebar.space_gesture;
        if !spaces_shown
            || event.modifiers.modified()
            || cx.has_active_drag()
            || self.native_sidebar.menu.is_some()
            || gesture
                .list_blocks
                .iter()
                .any(|bounds| bounds.contains(&event.position))
        {
            return;
        }
        gesture.mouse_drag = Some(SpaceMouseDrag {
            origin: event.position,
            switched: false,
        });
        // The window's text selection layer would otherwise start a selection from this press and sweep it along the drag.
        gpui_component::GlobalState::suppress_text_selection(cx);
        cx.notify();
    }

    pub(crate) fn move_native_space_mouse_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.native_sidebar.space_gesture.mouse_drag.as_mut() else {
            return;
        };
        // A move without the button means its release never reached the window.
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_native_space_mouse_drag(cx);
            return;
        }
        let x = f32::from(event.position.x - drag.origin.x);
        let y = f32::from(event.position.y - drag.origin.y);
        if drag.switched || x.abs() < SWITCH_DISTANCE || x.abs() <= y.abs() * SIDEWAYS_RATIO {
            return;
        }
        drag.switched = true;
        let Some(snapshot) = self.native_sidebar.snapshot.clone() else {
            return;
        };
        // Pulling the list left, like two fingers swiping left with natural scrolling, goes to the next Space.
        self.step_native_space(snapshot, -x.signum(), cx);
    }

    pub(crate) fn end_native_space_mouse_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self
            .native_sidebar
            .space_gesture
            .mouse_drag
            .take()
            .is_some()
        {
            cx.notify();
        }
    }

    /// Switches to the Space after (`direction` > 0) or before the selected one, or bounces at the end of the row.
    fn step_native_space(
        &mut self,
        snapshot: Arc<NativeSidebarSnapshot>,
        direction: f32,
        cx: &mut gpui::Context<Self>,
    ) {
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

    /// Go to Space `position` (1-based, in the sidebar's current order) with the swipe's slide-and-fade.
    /// Nothing happens while Spaces is off, in Bots mode, when no Space has that position, or when it is already selected.
    pub(crate) fn go_to_native_space(&mut self, position: usize, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self
            .native_sidebar
            .snapshot
            .clone()
            .filter(|snapshot| snapshot.spaces_enabled && !snapshot.bots_mode)
        else {
            return;
        };
        let Some(target) = position
            .checked_sub(1)
            .filter(|index| *index < snapshot.spaces.len())
        else {
            return;
        };
        let selected = snapshot.spaces.iter().position(|space| space.selected);
        if selected == Some(target) {
            return;
        }
        let direction = if selected.is_none_or(|selected| target > selected) {
            1.0
        } else {
            -1.0
        };
        let destination = Some(snapshot.spaces[target].id.clone());
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
        if self.gpui_reduce_motion_enabled {
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
