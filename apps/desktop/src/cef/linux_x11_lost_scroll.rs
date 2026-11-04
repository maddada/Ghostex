//! Finds the scroll deltas Chromium drops in an X11-embedded browser, so the X11 adapter can send
//! them to the page itself (`linux_x11_pointer_focus.rs`).

use std::collections::{HashMap, HashSet};

/// CDXC:PlatformSupport 2026-10-27 DECISION:
/// User (Q685): "Keep pixel-precise touchpads" — deliver the wheel notches Chromium lost to the page the pointer was over, through CEF's own SendMouseWheelEvent, instead of turning off Chromium's high-precision scrolling. That switch (f64ff2dac) made the wheel work but made touchpads scroll in wheel-sized steps.
/// Why Chromium loses them (GitHub issue #338): Chromium measures an XI2 scroll motion against the last scroll-valuator value it saw from that device, and forgets every device's value on any XI_Enter that is not Inferior or PassiveUngrab (and one device's on a SlaveSwitch), so the first scroll motion after that scrolls by 0. Window managers keep passive button grabs on their client windows (xfwm4 on every window, Mutter and KWin on unfocused ones), and each wheel notch's emulated button 4/5 press activates that grab on GPUI's top-level window; because Chromium's input window is inside it, the grab and its replay send Chromium an Enter(Ungrab), so under xfwm4 every notch scrolled by 0 and touchpads lost about 17% of their travel.
/// This tracker mirrors Chromium's rules from the same XI2 events: when Chromium is about to measure a scroll motion with no previous value, the motion's own delta (from the raw event the server sends just before it) is the scroll Chromium dropped. Raw events whose device event never reached the watched windows count as seen by Chromium, so a doubt costs at most one lost delta, never a double scroll.
#[derive(Default)]
pub(crate) struct LostScrollTracker {
    /// Scroll classes per device id, as XIQueryDevice reports them.
    axes: HashMap<u16, Vec<ScrollAxis>>,
    /// Devices whose XI1 type is TOUCHPAD; Chromium turns their scrolls into precise scroll events.
    touchpads: HashSet<u16>,
    /// (source device, valuator) pairs Chromium has a previous value for.
    seen: HashSet<(u16, u16)>,
    /// The last raw motion's scroll deltas, waiting for its device event.
    pending: Option<RawScroll>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollAxis {
    pub(crate) number: u16,
    pub(crate) horizontal: bool,
    pub(crate) increment: f64,
}

/// Wheel offsets Chromium dropped, in Chromium's units: 120 per scroll increment, negative for
/// down and right, the same values its X11 event translation would have produced. `precise` is set
/// for touchpads, whose scrolls Chromium applies pixel by pixel instead of animating them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LostScroll {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) precise: bool,
}

struct RawScroll {
    source: u16,
    time: u32,
    deltas: Vec<(u16, f64)>,
}

/// `kWheelScrollAmount` in Chromium's ui/events/x/events_x_utils.cc.
const CHROMIUM_WHEEL_SCROLL_AMOUNT: f64 = 120.0;

impl LostScrollTracker {
    /// Chromium rebuilds its device list (and forgets every value) on XI_HierarchyChanged and on
    /// XI_DeviceChanged with reason DeviceChange.
    pub(crate) fn set_devices(
        &mut self,
        axes: HashMap<u16, Vec<ScrollAxis>>,
        touchpads: HashSet<u16>,
    ) {
        self.axes = axes;
        self.touchpads = touchpads;
        self.seen.clear();
        self.pending = None;
    }

    /// An XI_Enter on one of Chromium's input windows.
    pub(crate) fn enter(&mut self, inferior: bool, passive_ungrab: bool) {
        self.settle_pending();
        if !inferior && !passive_ungrab {
            self.seen.clear();
        }
    }

    /// An XI_DeviceChanged with reason SlaveSwitch.
    pub(crate) fn slave_switch(&mut self, source: u16) {
        self.settle_pending();
        self.seen.retain(|(device, _)| *device != source);
    }

    /// An XI_RawMotion; `valuators` are (valuator number, value) pairs.
    pub(crate) fn raw_motion(&mut self, source: u16, time: u32, valuators: &[(u16, f64)]) {
        self.settle_pending();
        let Some(axes) = self.axes.get(&source) else {
            return;
        };
        let deltas: Vec<(u16, f64)> = valuators
            .iter()
            .copied()
            .filter(|(number, _)| axes.iter().any(|axis| axis.number == *number))
            .collect();
        if !deltas.is_empty() {
            self.pending = Some(RawScroll {
                source,
                time,
                deltas,
            });
        }
    }

    /// An XI2 device event on one of Chromium's input windows that Chromium measures scroll
    /// valuators on: a motion, or a wheel button (4-7) press. `valuators` are the valuator
    /// numbers set in the event. Returns what Chromium dropped from this motion.
    pub(crate) fn device_event(
        &mut self,
        source: u16,
        time: u32,
        motion: bool,
        valuators: &[u16],
    ) -> Option<LostScroll> {
        let raw = match self.pending.take() {
            Some(raw) if raw.source == source && raw.time == time => Some(raw),
            other => {
                self.pending = other;
                self.settle_pending();
                None
            }
        };
        let axes = self.axes.get(&source)?;
        let mut lost = LostScroll {
            x: 0.0,
            y: 0.0,
            precise: self.touchpads.contains(&source),
        };
        for axis in axes.iter().filter(|axis| valuators.contains(&axis.number)) {
            let first_sighting = self.seen.insert((source, axis.number));
            if !(motion && first_sighting) || axis.increment == 0.0 {
                continue;
            }
            let Some(delta) = raw.as_ref().and_then(|raw| {
                raw.deltas
                    .iter()
                    .find_map(|(number, delta)| (*number == axis.number).then_some(*delta))
            }) else {
                continue;
            };
            let offset = -delta / axis.increment * CHROMIUM_WHEEL_SCROLL_AMOUNT;
            if axis.horizontal {
                lost.x += offset;
            } else {
                lost.y += offset;
            }
        }
        (lost.x != 0.0 || lost.y != 0.0).then_some(lost)
    }

    /// A raw scroll whose device event did not reach the watched windows went to Chromium alone
    /// (its implicit grab while a button is held, a popup) or elsewhere; count it as seen.
    fn settle_pending(&mut self) {
        if let Some(raw) = self.pending.take() {
            for (number, _) in raw.deltas {
                self.seen.insert((raw.source, number));
            }
        }
    }
}

/// CEF event flags for an XI2 event's effective modifiers and held buttons (button N is bit N of
/// the first buttons-mask word).
pub(crate) fn cef_event_flags(effective_mods: u32, buttons: u32) -> u32 {
    const X_SHIFT: u32 = 1;
    const X_LOCK: u32 = 1 << 1;
    const X_CONTROL: u32 = 1 << 2;
    const X_MOD1_ALT: u32 = 1 << 3;
    const X_MOD2_NUM_LOCK: u32 = 1 << 4;
    const X_MOD4_SUPER: u32 = 1 << 6;
    let pairs = [
        (effective_mods & X_LOCK, 1 << 0),       // EVENTFLAG_CAPS_LOCK_ON
        (effective_mods & X_SHIFT, 1 << 1),      // EVENTFLAG_SHIFT_DOWN
        (effective_mods & X_CONTROL, 1 << 2),    // EVENTFLAG_CONTROL_DOWN
        (effective_mods & X_MOD1_ALT, 1 << 3),   // EVENTFLAG_ALT_DOWN
        (buttons & (1 << 1), 1 << 4),            // EVENTFLAG_LEFT_MOUSE_BUTTON
        (buttons & (1 << 2), 1 << 5),            // EVENTFLAG_MIDDLE_MOUSE_BUTTON
        (buttons & (1 << 3), 1 << 6),            // EVENTFLAG_RIGHT_MOUSE_BUTTON
        (effective_mods & X_MOD4_SUPER, 1 << 7), // EVENTFLAG_COMMAND_DOWN
        (effective_mods & X_MOD2_NUM_LOCK, 1 << 8), // EVENTFLAG_NUM_LOCK_ON
    ];
    pairs
        .into_iter()
        .filter(|(set, _)| *set != 0)
        .fold(0, |flags, (_, flag)| flags | flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHEEL: u16 = 9;
    const TOUCHPAD: u16 = 10;
    const VERTICAL: u16 = 3;
    const HORIZONTAL: u16 = 2;

    fn tracker() -> LostScrollTracker {
        let mut tracker = LostScrollTracker::default();
        let axes = |increment| {
            vec![
                ScrollAxis {
                    number: HORIZONTAL,
                    horizontal: true,
                    increment,
                },
                ScrollAxis {
                    number: VERTICAL,
                    horizontal: false,
                    increment,
                },
            ]
        };
        tracker.set_devices(
            HashMap::from([(WHEEL, axes(120.0)), (TOUCHPAD, axes(15.0))]),
            HashSet::from([TOUCHPAD]),
        );
        tracker
    }

    /// One scroll motion as the server sends it: the raw event, then the device event.
    fn scroll(
        tracker: &mut LostScrollTracker,
        source: u16,
        time: u32,
        axis: u16,
        delta: f64,
    ) -> Option<LostScroll> {
        tracker.raw_motion(source, time, &[(axis, delta)]);
        tracker.device_event(source, time, true, &[axis])
    }

    /// A wheel notch under a window manager that grabs buttons: motion, emulated press, and the
    /// grab's Enter(Ungrab) on Chromium's window.
    fn grabbed_notch(tracker: &mut LostScrollTracker, time: u32) -> Option<LostScroll> {
        let lost = scroll(tracker, WHEEL, time, VERTICAL, 120.0);
        tracker.device_event(WHEEL, time, false, &[]);
        tracker.enter(false, false);
        lost
    }

    #[test]
    fn every_notch_after_a_grab_crossing_is_delivered_once() {
        let mut tracker = tracker();
        tracker.slave_switch(WHEEL);
        for time in 1..=5 {
            assert_eq!(
                grabbed_notch(&mut tracker, time),
                Some(LostScroll {
                    x: 0.0,
                    y: -120.0,
                    precise: false
                })
            );
        }
    }

    #[test]
    fn notches_chromium_measures_itself_are_not_sent_again() {
        let mut tracker = tracker();
        tracker.slave_switch(WHEEL);
        assert_eq!(
            scroll(&mut tracker, WHEEL, 1, VERTICAL, 120.0),
            Some(LostScroll {
                x: 0.0,
                y: -120.0,
                precise: false
            })
        );
        for time in 2..=5 {
            assert_eq!(scroll(&mut tracker, WHEEL, time, VERTICAL, 120.0), None);
        }
    }

    #[test]
    fn inferior_and_passive_ungrab_crossings_keep_the_previous_value() {
        let mut tracker = tracker();
        scroll(&mut tracker, WHEEL, 1, VERTICAL, 120.0);
        tracker.enter(true, false);
        assert_eq!(scroll(&mut tracker, WHEEL, 2, VERTICAL, 120.0), None);
        tracker.enter(false, true);
        assert_eq!(scroll(&mut tracker, WHEEL, 3, VERTICAL, 120.0), None);
        tracker.enter(false, false);
        assert!(scroll(&mut tracker, WHEEL, 4, VERTICAL, 120.0).is_some());
    }

    #[test]
    fn slave_switch_forgets_only_that_device() {
        let mut tracker = tracker();
        scroll(&mut tracker, WHEEL, 1, VERTICAL, 120.0);
        scroll(&mut tracker, TOUCHPAD, 2, VERTICAL, 3.0);
        tracker.slave_switch(TOUCHPAD);
        assert_eq!(scroll(&mut tracker, WHEEL, 3, VERTICAL, 120.0), None);
        assert_eq!(
            scroll(&mut tracker, TOUCHPAD, 4, VERTICAL, 3.0),
            Some(LostScroll {
                x: 0.0,
                y: -24.0,
                precise: true
            })
        );
    }

    #[test]
    fn touchpad_loses_only_the_motion_after_each_crossing() {
        let mut tracker = tracker();
        tracker.slave_switch(TOUCHPAD);
        let mut sent = 0.0;
        for time in 1..=20 {
            if let Some(lost) = scroll(&mut tracker, TOUCHPAD, time, VERTICAL, -3.0) {
                sent += lost.y;
            }
            if time % 5 == 0 {
                tracker.enter(false, false);
            }
        }
        // Motions 1, 6, 11 and 16 follow a reset: 4 x 3 / 15 x 120.
        assert_eq!(sent, 96.0);
    }

    #[test]
    fn horizontal_axis_goes_to_x() {
        let mut tracker = tracker();
        assert_eq!(
            scroll(&mut tracker, WHEEL, 1, HORIZONTAL, -120.0),
            Some(LostScroll {
                x: 120.0,
                y: 0.0,
                precise: false
            })
        );
    }

    #[test]
    fn a_raw_scroll_that_never_reached_chromiums_windows_counts_as_seen() {
        let mut tracker = tracker();
        // Scrolled during Chromium's own implicit grab: only the raw event reaches us.
        tracker.raw_motion(WHEEL, 1, &[(VERTICAL, 120.0)]);
        tracker.raw_motion(WHEEL, 2, &[(VERTICAL, 120.0)]);
        assert_eq!(tracker.device_event(WHEEL, 2, true, &[VERTICAL]), None);
    }

    #[test]
    fn a_motion_without_its_raw_event_sends_nothing() {
        let mut tracker = tracker();
        tracker.raw_motion(WHEEL, 1, &[(VERTICAL, 120.0)]);
        assert_eq!(tracker.device_event(WHEEL, 7, true, &[VERTICAL]), None);
        assert_eq!(scroll(&mut tracker, WHEEL, 8, VERTICAL, 120.0), None);
    }

    #[test]
    fn a_wheel_button_press_with_the_valuator_counts_as_seen() {
        let mut tracker = tracker();
        tracker.device_event(WHEEL, 1, false, &[VERTICAL]);
        assert_eq!(scroll(&mut tracker, WHEEL, 2, VERTICAL, 120.0), None);
    }

    #[test]
    fn device_list_changes_forget_everything() {
        let mut tracker = tracker();
        scroll(&mut tracker, WHEEL, 1, VERTICAL, 120.0);
        let axes = tracker.axes.clone();
        let touchpads = tracker.touchpads.clone();
        tracker.set_devices(axes, touchpads);
        assert!(scroll(&mut tracker, WHEEL, 2, VERTICAL, 120.0).is_some());
    }

    #[test]
    fn modifier_and_button_flags() {
        assert_eq!(cef_event_flags(1 | 4, 0), 2 | 4);
        assert_eq!(cef_event_flags(8 | 64, 1 << 1), 8 | 128 | 16);
        assert_eq!(cef_event_flags(2 | 16, 1 << 3), 1 | 256 | 64);
    }
}
