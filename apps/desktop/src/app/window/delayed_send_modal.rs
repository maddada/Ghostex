//! Native GPUI Session Automations dialog, the desktop twin of the React
//! `DelayedSendModal` in packages/core-ui/delayed-send-modal.tsx.
//!
//! CDXC:DelayedSend 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in GPUI one at a time and each native dialog must match its React twin 1 to 1: the same layout, copy, colors, states and behaviour in both appearances. Session Automations keeps one trigger per send, whole hours and minutes for After a delay (remaining deadlines rounded up to the next minute), the awake-agent picker polled every 3 seconds while the dialog is open, and the reserved trigger detail slot so switching triggers never moves the footer.
//! SEE-ALSO: packages/core-ui/delayed-send-modal.tsx and the `.delayed-send-*` rules in packages/core-ui/styles/modals.css and modals-light.css (the React twin), apps/desktop/views/delayed-send-agents.ts (the awake-agent polling mirrored here), apps/desktop/src/app/window/native_modal_kit.rs (shared chrome and controls), apps/desktop/src/app/delayed_send_modal_lifecycle.rs (open, bridge commands, agent replies), apps/desktop/src/bin/native_modal_demo/delayed_send.rs (standalone preview).
use super::native_modal_kit::*;
use chrono::{Local, NaiveTime, TimeZone as _};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Rgba,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb,
};
use gpui_component::date_picker::{DatePicker, DatePickerEvent, DatePickerState};
use gpui_component::input::{Escape, InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;
use std::time::Duration;

/// `APP_MODAL_HOST_DELAYED_SEND_WINDOW_WIDTH`.
pub(crate) const DELAYED_SEND_MODAL_WIDTH: f32 = 470.0;
/// First-frame height only (`APP_MODAL_HOST_DELAYED_SEND_WINDOW_HEIGHT`); the window is resized to the measured layout.
pub(crate) const DELAYED_SEND_MODAL_INITIAL_HEIGHT: f32 = 565.0;

const MAX_DELAY_MS: u64 = 2_147_483_647;
const MINUTE_MS: u64 = 60_000;
const HOUR_MS: u64 = 60 * MINUTE_MS;
/// `useDesktopDelayedSendAgents` re-requests the awake list 3 seconds after each reply.
const AGENTS_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// `.delayed-send-trigger-detail-slot` reserves room for the date/time and agent pickers.
const TRIGGER_DETAIL_SLOT_MIN_HEIGHT: f32 = 112.0;
/// `var(--ghostex-accent, #86d3f8)`: the armed Send Enter status color in the dark appearance.
const DARK_STATUS_ACCENT: u32 = 0x86d3f8;

const TITLE: &str = "Session Automations";
const DESCRIPTION: &str = "Configure automations for this agent session.";
const SESSION_TARGET_FALLBACK: &str = "Current agent session";
const SEND_ENTER_TITLE: &str = "Send Enter";
const STATUS_DISABLED: &str = "No Enter keypress will be scheduled.";
const STATUS_SPECIFIC_AGENT_ACTIVE: &str = "Active when the selected agent finishes working.";
const STATUS_ALL_AGENTS_ACTIVE: &str = "Active when all agents finish working.";
const STATUS_AGENT_ACTIVE: &str = "Active when this agent finishes working.";
const STATUS_IDLE: &str = "Press Enter later using the selected trigger.";
const TRIGGER_LABEL: &str = "Trigger";
const TRIGGER_AFTER_DELAY: &str = "After a delay";
const TRIGGER_SPECIFIC_TIME: &str = "Specific time";
const TRIGGER_AGENT_STOPS: &str = "When this agent finishes";
const TRIGGER_SPECIFIC_AGENT_STOPS: &str = "When a specific agent finishes";
const TRIGGER_ALL_AGENTS_STOP: &str = "When all agents finish";
const HOURS_LABEL: &str = "Hours";
const MINUTES_LABEL: &str = "Minutes";
const AGENT_SESSION_LABEL: &str = "Agent session";
const AGENTS_LOADING: &str = "Loading agent sessions...";
const AGENTS_SELECT: &str = "Select an agent session";
const AGENTS_EMPTY: &str = "No agent sessions available";
const SPECIFIC_AGENT_DESCRIPTION: &str = "Ghostex will send Enter after the selected agent finishes working and remains idle for 10 seconds.";
const AGENT_STOPS_DESCRIPTION: &str = "Ghostex will send Enter automatically after this agent finishes working and remains idle for 10 seconds.";
const ALL_AGENTS_STOP_DESCRIPTION: &str = "Ghostex will send Enter automatically after every agent in this project finishes working and remains idle for 10 seconds.";
const CLOSE_TITLE: &str = "Close session after Done";
const CLOSE_DESCRIPTION: &str = "Closes this terminal 3 minutes after Done.";
const CANCEL: &str = "Cancel";
const SAVE_CHANGES: &str = "Save changes";

/// The mutually exclusive send triggers of the React dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DelayedSendTrigger {
    AfterDelay,
    SpecificTime,
    AgentStops,
    AllAgentsStop,
    SpecificAgentStops,
}

impl DelayedSendTrigger {
    fn label(self) -> &'static str {
        match self {
            Self::AfterDelay => TRIGGER_AFTER_DELAY,
            Self::SpecificTime => TRIGGER_SPECIFIC_TIME,
            Self::AgentStops => TRIGGER_AGENT_STOPS,
            Self::AllAgentsStop => TRIGGER_ALL_AGENTS_STOP,
            Self::SpecificAgentStops => TRIGGER_SPECIFIC_AGENT_STOPS,
        }
    }
}

/// `DelayedSendAgentReference` from packages/shared/delayed-send.ts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DelayedSendAgentReference {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

/// `DelayedSendAgentOption`: an awake agent session the send can wait for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DelayedSendAgentOption {
    pub(crate) reference: DelayedSendAgentReference,
    pub(crate) label: String,
}

/// The React dialog's props, resolved by the host from the sidebar's open message.
pub(crate) struct DelayedSendModalConfig {
    /// The session's agent logo (`agent-icons/<file>.svg`), drawn as a 14px mask beside the session title.
    pub(crate) agent_icon_path: Option<String>,
    pub(crate) close_after_done_active: bool,
    /// `Some` when the open message carried `delayedSendDeadlineAt`; the value is the
    /// remaining time at open, clamped at zero (`getRemainingMs`).
    pub(crate) delayed_send_deadline_remaining_ms: Option<u64>,
    pub(crate) delayed_send_remaining_label: Option<String>,
    pub(crate) send_when_all_project_sessions_stop_active: bool,
    pub(crate) send_when_agent_stops_active: bool,
    pub(crate) send_when_specific_agent_finishes: Option<DelayedSendAgentReference>,
    pub(crate) supports_send_when_agent_stops: bool,
    pub(crate) supports_send_when_all_project_sessions_stop: bool,
    pub(crate) title: Option<String>,
    /// The runtime `--ghostex-accent` when the host knows it; otherwise the CSS default.
    pub(crate) status_accent: Option<Rgba>,
    pub(crate) palette: ModalPalette,
}

/// What Save does to the Send Enter automation.
pub(crate) enum DelayedSendModalSend {
    /// `onConfirm`: `delayMs` is `None` for the status triggers.
    Schedule {
        delay_ms: Option<u64>,
        send_when_agent_stops: bool,
        send_when_all_project_sessions_stop: bool,
        send_when_specific_agent_finishes: Option<DelayedSendAgentReference>,
    },
    /// `onCancelTimer`: the switch was turned off while a send was armed.
    CancelTimer,
    /// Nothing to change on the send automation.
    Keep,
}

/// What the dialog asks its host to do. The dialog removes its own window
/// before sending `Save` or `Cancel`.
pub(crate) enum DelayedSendModalCommand {
    Save {
        /// `onToggleCloseAfterDone`, sent first, only when the switch changed.
        toggle_close_after_done: bool,
        send: DelayedSendModalSend,
    },
    Cancel,
    /// `requestDelayedSendAgents {sessionId, requestId}`; answered through `receive_agents`.
    RequestAgents {
        request_id: String,
    },
}

pub(crate) type DelayedSendModalHost = Rc<dyn Fn(DelayedSendModalCommand, &mut App)>;

/// `useDesktopDelayedSendAgents` state: only present when the all-agents
/// trigger is supported, which is when the React host offers the picker.
struct AwakeAgents {
    sessions: Vec<DelayedSendAgentOption>,
    /// The host's error text, interned once per distinct value because the
    /// select trigger takes a static placeholder.
    error: Option<&'static str>,
    loading: bool,
}

pub(crate) struct GpuiDelayedSendModalWindow {
    host: DelayedSendModalHost,
    palette: ModalPalette,
    status_accent: Rgba,
    agent_icon_path: Option<String>,
    close_after_done_active: bool,
    deadline_remaining_ms: Option<u64>,
    remaining_label: Option<String>,
    send_when_all_project_sessions_stop_active: bool,
    send_when_agent_stops_active: bool,
    /// The open message's `sendWhenSpecificAgentFinishes`.
    configured_specific_agent: Option<DelayedSendAgentReference>,
    /// `delayedSendAgents.active ?? sendWhenSpecificAgentFinishes`: the armed specific-agent send.
    active_specific_agent: Option<DelayedSendAgentReference>,
    supports_send_when_agent_stops: bool,
    supports_send_when_all_project_sessions_stop: bool,
    title: Option<String>,
    awake: Option<AwakeAgents>,
    pending_request_id: Option<String>,
    request_counter: u64,
    hours: gpui::Entity<InputState>,
    minutes: gpui::Entity<InputState>,
    specific_date: gpui::Entity<DatePickerState>,
    specific_time: gpui::Entity<InputState>,
    send_enter_enabled: bool,
    trigger: DelayedSendTrigger,
    specific_agent: Option<DelayedSendAgentReference>,
    close_after_done_enabled: bool,
    trigger_select: ModalSelect,
    agent_select: ModalSelect,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl GpuiDelayedSendModalWindow {
    pub(crate) fn new(
        config: DelayedSendModalConfig,
        host: DelayedSendModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let hours = cx.new(|cx| InputState::new(window, cx));
        let minutes = cx.new(|cx| InputState::new(window, cx));
        let specific_date = cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));
        let specific_time = cx.new(|cx| InputState::new(window, cx).placeholder("HH:MM"));
        let mut subscriptions = Vec::new();
        subscriptions
            .push(cx.subscribe(&specific_date, |_, _, _: &DatePickerEvent, cx| cx.notify()));
        for state in [&hours, &minutes, &specific_time] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                |this: &mut Self, _input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit_from_duration_input(window, cx),
                    _ => {}
                },
            ));
        }
        // The React minutes input selects its whole value whenever it gains focus.
        subscriptions.push(cx.subscribe_in(
            &minutes,
            window,
            |_this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Focus) {
                    input.update(cx, |input, cx| {
                        let len = input.value().len();
                        input.set_selected_range(0..len, cx);
                    });
                }
            },
        ));
        let palette = config.palette;
        let mut this = Self {
            host,
            palette,
            status_accent: config.status_accent.unwrap_or(if palette.light {
                palette.foreground
            } else {
                rgb(DARK_STATUS_ACCENT)
            }),
            agent_icon_path: config.agent_icon_path,
            close_after_done_active: config.close_after_done_active,
            deadline_remaining_ms: config.delayed_send_deadline_remaining_ms,
            remaining_label: config
                .delayed_send_remaining_label
                .filter(|label| !label.is_empty()),
            send_when_all_project_sessions_stop_active: config
                .send_when_all_project_sessions_stop_active,
            send_when_agent_stops_active: config.send_when_agent_stops_active,
            configured_specific_agent: config.send_when_specific_agent_finishes.clone(),
            active_specific_agent: config.send_when_specific_agent_finishes,
            supports_send_when_agent_stops: config.supports_send_when_agent_stops,
            supports_send_when_all_project_sessions_stop: config
                .supports_send_when_all_project_sessions_stop,
            title: config.title,
            awake: config
                .supports_send_when_all_project_sessions_stop
                .then(|| AwakeAgents {
                    sessions: Vec::new(),
                    error: None,
                    loading: true,
                }),
            pending_request_id: None,
            request_counter: 0,
            hours,
            minutes,
            specific_date,
            specific_time,
            send_enter_enabled: true,
            trigger: DelayedSendTrigger::AfterDelay,
            specific_agent: None,
            close_after_done_enabled: config.close_after_done_active,
            trigger_select: ModalSelect::new(),
            agent_select: ModalSelect::new(),
            fit: ModalFit::new(),
            focus_handle,
            _subscriptions: subscriptions,
        };
        this.reset_from_open_state(window, cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.send_enter_enabled
                            && this.trigger == DelayedSendTrigger::SpecificTime
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if this.awake.is_some() {
            this.request_agents(cx);
        }
        this
    }

    /// The React open effect: prefill the duration from the remaining deadline,
    /// resolve the initial trigger from the armed automations, re-enable the
    /// send switch, and focus the minutes field for the delay trigger.
    fn reset_from_open_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let remaining_ms = self.deadline_remaining_ms.unwrap_or(0);
        let (hours, minutes) = if remaining_ms > 0 {
            duration_parts_from_ms(remaining_ms)
        } else {
            (0, 5)
        };
        self.hours.update(cx, |input, cx| {
            input.set_value(hours.to_string(), window, cx);
        });
        self.minutes.update(cx, |input, cx| {
            input.set_value(minutes.to_string(), window, cx);
        });
        let initial_ms = Local::now().timestamp_millis()
            + if remaining_ms > 0 {
                remaining_ms as i64
            } else {
                5 * MINUTE_MS as i64
            };
        let initial = Local
            .timestamp_millis_opt(
                ((initial_ms + MINUTE_MS as i64 - 1) / MINUTE_MS as i64) * MINUTE_MS as i64,
            )
            .single()
            .unwrap();
        self.specific_date.update(cx, |picker, cx| {
            picker.set_date(initial.date_naive(), window, cx);
        });
        self.specific_time.update(cx, |input, cx| {
            input.set_value(initial.format("%H:%M").to_string(), window, cx);
        });
        let initial_trigger = self.initial_trigger();
        self.send_enter_enabled = true;
        self.trigger = initial_trigger;
        self.specific_agent = self.active_specific_agent.clone();
        self.close_after_done_enabled = self.close_after_done_active;
        self.trigger_select.close();
        self.agent_select.close();
        if initial_trigger == DelayedSendTrigger::AfterDelay {
            self.focus_minutes(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    /// CDXC:DelayedSend 2026-09-19 DECISION:
    /// User: the default Delayed Send trigger is When all agents finish.
    /// An already armed send still reopens on its own trigger; After a delay remains the fallback when the all-agents option is unavailable.
    /// SEE-ALSO: packages/core-ui/delayed-send-modal.tsx, apps/mobile/app/src/components/sessions/DelayedSendDialog.tsx
    fn initial_trigger(&self) -> DelayedSendTrigger {
        let should_send_when_all_project_sessions_stop = self
            .supports_send_when_all_project_sessions_stop
            && self.send_when_all_project_sessions_stop_active;
        let should_send_when_agent_stops = !should_send_when_all_project_sessions_stop
            && self.supports_send_when_agent_stops
            && self.send_when_agent_stops_active;
        let has_armed_timer =
            self.deadline_remaining_ms.is_some() || self.remaining_label.is_some();
        if self.active_specific_agent.is_some() {
            DelayedSendTrigger::SpecificAgentStops
        } else if should_send_when_all_project_sessions_stop {
            DelayedSendTrigger::AllAgentsStop
        } else if should_send_when_agent_stops {
            DelayedSendTrigger::AgentStops
        } else if self.supports_send_when_all_project_sessions_stop && !has_armed_timer {
            DelayedSendTrigger::AllAgentsStop
        } else {
            DelayedSendTrigger::AfterDelay
        }
    }

    fn focus_minutes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.minutes.update(cx, |input, cx| {
            input.focus(window, cx);
            let len = input.value().len();
            input.set_selected_range(0..len, cx);
        });
    }

    /// Posts one `requestDelayedSendAgents` round trip; the reply lands in `receive_agents`.
    fn request_agents(&mut self, cx: &mut Context<Self>) {
        self.request_counter = self.request_counter.wrapping_add(1);
        let nanos = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let request_id = format!("gpui-delayed-send-{nanos:x}-{}", self.request_counter);
        self.pending_request_id = Some(request_id.clone());
        (self.host)(DelayedSendModalCommand::RequestAgents { request_id }, cx);
    }

    /// The host's `delayedSendAgents` reply. Replies for a request other than
    /// the pending one are ignored, like the React hook does; a matching reply
    /// updates the picker and schedules the next poll 3 seconds later.
    pub(crate) fn receive_agents(
        &mut self,
        request_id: &str,
        sessions: Vec<DelayedSendAgentOption>,
        active: Option<DelayedSendAgentReference>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_request_id.as_deref() != Some(request_id) {
            return;
        }
        self.pending_request_id = None;
        if let Some(awake) = self.awake.as_mut() {
            awake.sessions = sessions;
            let error = error.filter(|error| !error.is_empty());
            if awake.error.map(str::to_string) != error {
                awake.error = error.map(|error| &*Box::leak(error.into_boxed_str()));
            }
            awake.loading = false;
            if awake.sessions.is_empty() {
                self.agent_select.close();
            }
        }
        // `delayedSendAgents.active ?? sendWhenSpecificAgentFinishes`; a changed
        // armed reference re-runs the open effect, as the React dependency list does.
        let next_active = active.or_else(|| self.configured_specific_agent.clone());
        if next_active != self.active_specific_agent {
            self.active_specific_agent = next_active;
            self.reset_from_open_state(window, cx);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AGENTS_POLL_INTERVAL).await;
            let _ = this.update(cx, |this, cx| this.request_agents(cx));
        })
        .detach();
    }

    fn trigger_options(&self) -> Vec<DelayedSendTrigger> {
        let mut options = vec![
            DelayedSendTrigger::AfterDelay,
            DelayedSendTrigger::SpecificTime,
        ];
        if self.supports_send_when_agent_stops {
            options.push(DelayedSendTrigger::AgentStops);
        }
        if self.awake.is_some() {
            options.push(DelayedSendTrigger::SpecificAgentStops);
        }
        if self.supports_send_when_all_project_sessions_stop {
            options.push(DelayedSendTrigger::AllAgentsStop);
        }
        options
    }

    fn trigger_index(&self) -> Option<usize> {
        self.trigger_options()
            .iter()
            .position(|option| *option == self.trigger)
    }

    fn awake_sessions(&self) -> &[DelayedSendAgentOption] {
        self.awake
            .as_ref()
            .map(|awake| awake.sessions.as_slice())
            .unwrap_or(&[])
    }

    fn selected_agent(&self) -> Option<&DelayedSendAgentOption> {
        let key = self.specific_agent.as_ref()?;
        self.awake_sessions()
            .iter()
            .find(|session| session.reference == *key)
    }

    fn selected_agent_index(&self) -> Option<usize> {
        let key = self.specific_agent.as_ref()?;
        self.awake_sessions()
            .iter()
            .position(|session| session.reference == *key)
    }

    /// CDXC:DelayedSend 2026-09-16 DECISION:
    /// User: Specific time belongs in the GPUI, React, and React Native Session Automations dialogs and must calculate the wait to reuse After a delay.
    /// Resolve local date/time on Save as well as render, so time spent editing does not move the deadline.
    fn delay_ms(&self, cx: &App) -> Option<u64> {
        if self.trigger == DelayedSendTrigger::SpecificTime {
            let date = self.specific_date.read(cx).date().start()?;
            let time =
                NaiveTime::parse_from_str(self.specific_time.read(cx).value().trim(), "%H:%M")
                    .ok()?;
            let deadline = Local.from_local_datetime(&date.and_time(time)).earliest()?;
            return (deadline - Local::now()).num_milliseconds().try_into().ok();
        }
        get_delay_ms(
            self.hours.read(cx).value().as_ref(),
            self.minutes.read(cx).value().as_ref(),
        )
    }

    fn is_valid_delay(&self, cx: &App) -> bool {
        let minimum = if self.trigger == DelayedSendTrigger::SpecificTime {
            1
        } else {
            MINUTE_MS
        };
        self.delay_ms(cx)
            .is_some_and(|delay_ms| (minimum..=MAX_DELAY_MS).contains(&delay_ms))
    }

    fn has_status_trigger(&self) -> bool {
        !matches!(
            self.trigger,
            DelayedSendTrigger::AfterDelay | DelayedSendTrigger::SpecificTime
        )
    }

    fn is_valid_schedule(&self, cx: &App) -> bool {
        if self.trigger == DelayedSendTrigger::SpecificAgentStops {
            self.selected_agent().is_some()
        } else {
            self.has_status_trigger() || self.is_valid_delay(cx)
        }
    }

    fn has_active_send(&self) -> bool {
        self.active_specific_agent.is_some()
            || self.deadline_remaining_ms.is_some()
            || self.remaining_label.is_some()
            || self.send_when_agent_stops_active
            || self.send_when_all_project_sessions_stop_active
    }

    fn close_after_done_changed(&self) -> bool {
        self.close_after_done_enabled != self.close_after_done_active
    }

    /// `canSave`: the desktop host always supplies `onCancelTimer`, so an armed
    /// send can always be disabled.
    fn can_save(&self, cx: &App) -> bool {
        if self.send_enter_enabled {
            self.is_valid_schedule(cx)
        } else {
            self.has_active_send() || self.close_after_done_changed()
        }
    }

    fn send_automation_description(&self) -> String {
        if !self.send_enter_enabled {
            STATUS_DISABLED.to_string()
        } else if self.active_specific_agent.is_some() {
            STATUS_SPECIFIC_AGENT_ACTIVE.to_string()
        } else if self.send_when_all_project_sessions_stop_active {
            STATUS_ALL_AGENTS_ACTIVE.to_string()
        } else if self.send_when_agent_stops_active {
            STATUS_AGENT_ACTIVE.to_string()
        } else if let Some(label) = &self.remaining_label {
            format!("Active. Enter sends in {label}.")
        } else {
            STATUS_IDLE.to_string()
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let delay_ms = self.delay_ms(cx);
        if self.send_enter_enabled
            && !self.has_status_trigger()
            && !delay_ms.is_some_and(|delay_ms| delay_ms > 0 && delay_ms <= MAX_DELAY_MS)
        {
            return;
        }
        let send = if self.send_enter_enabled {
            DelayedSendModalSend::Schedule {
                delay_ms: if self.has_status_trigger() {
                    None
                } else {
                    delay_ms
                },
                send_when_agent_stops: self.trigger == DelayedSendTrigger::AgentStops,
                send_when_all_project_sessions_stop: self.trigger
                    == DelayedSendTrigger::AllAgentsStop,
                send_when_specific_agent_finishes: if self.trigger
                    == DelayedSendTrigger::SpecificAgentStops
                {
                    self.selected_agent().map(|agent| agent.reference.clone())
                } else {
                    None
                },
            }
        } else if self.has_active_send() {
            DelayedSendModalSend::CancelTimer
        } else {
            DelayedSendModalSend::Keep
        };
        let command = DelayedSendModalCommand::Save {
            toggle_close_after_done: self.close_after_done_changed(),
            send,
        };
        self.close_window_and_send(command, window, cx);
    }

    /// Enter inside a duration or time input submits the form when the delay is valid.
    fn submit_from_duration_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.send_enter_enabled || self.has_status_trigger() || !self.is_valid_delay(cx) {
            return;
        }
        self.submit(window, cx);
    }

    fn close_window_and_send(
        &mut self,
        command: DelayedSendModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(DelayedSendModalCommand::Cancel, window, cx);
    }

    fn set_send_enter_enabled(
        &mut self,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_enter_enabled = checked;
        self.trigger_select.close();
        self.agent_select.close();
        if checked && self.trigger == DelayedSendTrigger::AfterDelay {
            self.focus_minutes(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    fn toggle_close_after_done(&mut self, cx: &mut Context<Self>) {
        self.close_after_done_enabled = !self.close_after_done_enabled;
        cx.notify();
    }

    fn toggle_trigger_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.trigger_options().len() == 1 {
            return;
        }
        self.agent_select.close();
        let selected = self.trigger_index();
        self.trigger_select.toggle(selected);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn choose_trigger(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_select.close();
        if let Some(trigger) = self.trigger_options().get(index).copied() {
            self.trigger = trigger;
            if trigger == DelayedSendTrigger::AfterDelay {
                self.focus_minutes(window, cx);
            } else {
                self.focus_handle.focus(window, cx);
            }
        }
        cx.notify();
    }

    fn toggle_agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.awake_sessions().is_empty() {
            return;
        }
        self.trigger_select.close();
        let selected = self.selected_agent_index();
        self.agent_select.toggle(selected);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        self.agent_select.close();
        if let Some(session) = self.awake_sessions().get(index) {
            self.specific_agent = Some(session.reference.clone());
        }
        cx.notify();
    }

    fn on_input_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match self
            .trigger_select
            .handle_key(key, self.trigger_options().len())
        {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_trigger(index, window, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match self
            .agent_select
            .handle_key(key, self.awake_sessions().len())
        {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        if key == "escape" {
            self.cancel(window, cx);
            cx.stop_propagation();
        }
    }

    /// `.delayed-send-dialog-description`: the sentence and the session target row.
    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let title = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(SESSION_TARGET_FALLBACK)
            .to_string();
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(div().text_size(px(16.0)).line_height(px(20.8)).child(TITLE))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(20.15))
                            .text_color(hsla(p.muted))
                            .child(DESCRIPTION),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap(px(7.0))
                            .text_size(px(13.0))
                            .line_height(px(16.25))
                            .text_color(hsla(p.foreground))
                            .children(self.agent_icon_path.clone().map(|path| {
                                div().flex_shrink_0().size(px(14.0)).opacity(0.78).child(
                                    gpui::svg()
                                        .path(path)
                                        .size(px(14.0))
                                        .text_color(hsla(p.foreground)),
                                )
                            }))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(title),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// A size-sm shadcn Card in the `.gx-app-modal` skin: 16px padding, the
    /// title/description column with the switch in the top-right corner.
    fn render_card(
        &self,
        id: &'static str,
        title: &'static str,
        description: String,
        description_color: Rgba,
        checked: bool,
        on_toggle: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        content: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        modal_panel(&p)
            .py(px(16.0))
            .gap(px(16.0))
            .child(
                h_flex()
                    .w_full()
                    .px(px(16.0))
                    .gap(px(8.0))
                    .items_start()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(20.15))
                                    .text_color(hsla(description_color))
                                    .whitespace_nowrap()
                                    .child(description),
                            ),
                    )
                    .child(
                        div()
                            .id(id)
                            .flex_shrink_0()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                on_toggle(this, window, cx);
                            }))
                            .child(modal_switch(&p, checked, false)),
                    ),
            )
            .children(content.map(|content| div().w_full().px(px(16.0)).child(content)))
            .into_any_element()
    }

    fn render_field_label(&self, text: &'static str) -> AnyElement {
        let p = self.palette;
        div()
            .text_size(px(12.0))
            .line_height(px(16.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(p.muted))
            .child(text)
            .into_any_element()
    }

    /// `.delayed-send-trigger-description`: 13px muted, line-height 1.45.
    fn render_trigger_description(&self, text: impl Into<SharedString>) -> AnyElement {
        let p = self.palette;
        div()
            .w_full()
            .text_size(px(13.0))
            .line_height(px(18.85))
            .text_color(hsla(p.muted))
            .child(text.into())
            .into_any_element()
    }

    fn render_duration_grid(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_start()
            .gap(px(10.0))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(12.0))
                    .child(self.render_field_label(HOURS_LABEL))
                    .child(modal_text_input(&p, &self.hours, false, window, cx)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(12.0))
                    .child(self.render_field_label(MINUTES_LABEL))
                    .child(modal_text_input(&p, &self.minutes, false, window, cx)),
            )
            .into_any_element()
    }

    fn render_specific_time(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(10.0))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(12.0))
                            .child(self.render_field_label("Date"))
                            .child(DatePicker::new(&self.specific_date)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(12.0))
                            .child(self.render_field_label("Time (24-hour)"))
                            .child(modal_text_input(&p, &self.specific_time, false, window, cx)),
                    ),
            )
            .child(self.render_trigger_description(if self.is_valid_delay(cx) {
                "Uses your local time."
            } else {
                "Choose a future date and time within 24 days."
            }))
            .into_any_element()
    }

    fn render_agent_field(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (placeholder, disabled): (&'static str, bool) = match &self.awake {
            Some(awake) => (
                awake.error.unwrap_or(if awake.loading {
                    AGENTS_LOADING
                } else if !awake.sessions.is_empty() {
                    AGENTS_SELECT
                } else {
                    AGENTS_EMPTY
                }),
                awake.sessions.is_empty(),
            ),
            None => (AGENTS_EMPTY, true),
        };
        let value = self.selected_agent().map(|agent| agent.label.clone());
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(self.render_field_label(AGENT_SESSION_LABEL))
            .child(
                h_flex()
                    .w_full()
                    .on_children_prepainted(capture_child_bounds(
                        self.agent_select.trigger_bounds.clone(),
                        0,
                    ))
                    .child(modal_select_trigger(
                        &p,
                        &self.agent_select,
                        "delayed-send-agent-select",
                        value,
                        placeholder,
                        disabled,
                        |this, window, cx| this.toggle_agent_menu(window, cx),
                        cx,
                    )),
            )
            .child(self.render_trigger_description(SPECIFIC_AGENT_DESCRIPTION))
            .into_any_element()
    }

    fn render_trigger_detail(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let detail = match self.trigger {
            DelayedSendTrigger::AfterDelay => self.render_duration_grid(window, cx),
            DelayedSendTrigger::SpecificTime => self.render_specific_time(window, cx),
            DelayedSendTrigger::SpecificAgentStops => self.render_agent_field(cx),
            DelayedSendTrigger::AgentStops => {
                self.render_trigger_description(AGENT_STOPS_DESCRIPTION)
            }
            DelayedSendTrigger::AllAgentsStop => {
                self.render_trigger_description(ALL_AGENTS_STOP_DESCRIPTION)
            }
        };
        v_flex()
            .w_full()
            .min_h(px(TRIGGER_DETAIL_SLOT_MIN_HEIGHT))
            .justify_center()
            .child(detail)
            .into_any_element()
    }

    fn render_send_enter_content(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let options = self.trigger_options();
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(12.0))
                    .child(self.render_field_label(TRIGGER_LABEL))
                    .child(
                        h_flex()
                            .w_full()
                            .on_children_prepainted(capture_child_bounds(
                                self.trigger_select.trigger_bounds.clone(),
                                0,
                            ))
                            .child(modal_select_trigger(
                                &p,
                                &self.trigger_select,
                                "delayed-send-trigger-select",
                                Some(self.trigger.label().to_string()),
                                "",
                                options.len() == 1,
                                |this, window, cx| this.toggle_trigger_menu(window, cx),
                                cx,
                            )),
                    ),
            )
            .child(self.render_trigger_detail(window, cx))
            .into_any_element()
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let send_active = self.send_enter_enabled && self.has_active_send();
        let send_content = self
            .send_enter_enabled
            .then(|| self.render_send_enter_content(window, cx));
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(self.render_card(
                "delayed-send-send-enter-switch",
                SEND_ENTER_TITLE,
                self.send_automation_description(),
                if send_active {
                    self.status_accent
                } else {
                    p.muted
                },
                self.send_enter_enabled,
                |this, window, cx| {
                    let next = !this.send_enter_enabled;
                    this.set_send_enter_enabled(next, window, cx);
                },
                send_content,
                cx,
            ))
            .child(self.render_card(
                "delayed-send-close-after-done-switch",
                CLOSE_TITLE,
                CLOSE_DESCRIPTION.to_string(),
                p.muted,
                self.close_after_done_enabled,
                |this, _window, cx| this.toggle_close_after_done(cx),
                None,
                cx,
            ))
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        modal_footer(vec![
            modal_action_button(
                &p,
                "delayed-send-cancel",
                CANCEL,
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.cancel(window, cx),
                cx,
            ),
            modal_action_button(
                &p,
                "delayed-send-save",
                SAVE_CHANGES,
                None,
                ModalButtonTone::Neutral,
                !self.can_save(cx),
                |this, window, cx| this.submit(window, cx),
                cx,
            ),
        ])
    }
}

impl Render for GpuiDelayedSendModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let trigger_items: Vec<String> = self
            .trigger_options()
            .iter()
            .map(|option| option.label().to_string())
            .collect();
        let trigger_menu = modal_select_menu(
            &p,
            &self.trigger_select,
            "delayed-send-trigger-menu",
            &trigger_items,
            self.trigger_index(),
            |this, index, window, cx| this.choose_trigger(index, window, cx),
            |this, _window, cx| {
                this.trigger_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let agent_items: Vec<String> = self
            .awake_sessions()
            .iter()
            .map(|session| session.label.clone())
            .collect();
        let agent_menu = modal_select_menu(
            &p,
            &self.agent_select,
            "delayed-send-agent-menu",
            &agent_items,
            self.selected_agent_index(),
            |this, index, _window, cx| this.choose_agent(index, cx),
            |this, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let content = vec![self.render_header(), self.render_body(window, cx)];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-delayed-send-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            trigger_menu.or(agent_menu),
            cx,
        )
        .capture_action(cx.listener(Self::on_input_escape))
    }
}

/// `parseDurationPart`: JavaScript `Number(value)` for a whole, non-negative amount.
fn parse_duration_part(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.' | 'e' | 'E'))
    {
        return None;
    }
    let parsed: f64 = trimmed.parse().ok()?;
    if !parsed.is_finite() || parsed < 0.0 || parsed.fract() != 0.0 {
        return None;
    }
    Some(parsed as u64)
}

/// `getDelayMs`: `None` stands for the React `NaN`.
fn get_delay_ms(hours: &str, minutes: &str) -> Option<u64> {
    let hours = parse_duration_part(hours)?;
    let minutes = parse_duration_part(minutes)?;
    hours
        .checked_mul(HOUR_MS)?
        .checked_add(minutes.checked_mul(MINUTE_MS)?)
}

/// `durationPartsFromMs`: whole minutes, rounded up, never below one.
fn duration_parts_from_ms(delay_ms: u64) -> (u64, u64) {
    let total_minutes = delay_ms.div_ceil(MINUTE_MS).max(1);
    (total_minutes / 60, total_minutes % 60)
}
