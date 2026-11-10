//! The voice panel: a small native child window, first under the work area header's top right,
//! that shows the open conversation's state, live captions and tool lines, the voice (click it to
//! pick another) and the speaking speed, with Mute, Stop talking and Hang up, and says why a
//! conversation stopped.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the conversation keeps going while the user moves between projects and chats, so the panel belongs to the main window, not to a chat or a view: it is an owned pop-up window attached as an AppKit child of the main window (the app toast window's pattern), which moves with the window, never takes focus, and overlaps nothing interactive of its own. A size change of the main window places it again.
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "the call screen should be movable and scrollable during voice mode." The panel's header is its drag handle: dragging it moves the panel's own window (`move_child_window`), kept inside the main window and remembered for the rest of the app's run, and the transcript scrolls with the wheel and trackpad. A window too small for the panel shrinks it, down to a floor.
//! CDXC:VoiceMode 2026-11-08 SEE-ALSO: the panel's edges resize it (size kept on disk), its transcript follows new lines until the reader scrolls up, and the voice list opens on the current voice: the decisions are in `panel_layout.rs`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, CursorStyle, DispatchPhase, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Render, ScrollEvent, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, point, px,
    rgba, size,
};
use gpui_component::{ElementExt as _, h_flex, v_flex};

use super::conversation::{
    VOICE_AGENT_SPEEDS, VoicePhase, VoiceProblem, VoiceRole, VoiceRow, provider_name, voice_name,
};
use super::focus::VoiceFocus;
use super::memory::{VoiceMemory, VoiceMemoryState};
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
#[cfg(not(target_os = "windows"))]
use crate::app::native_chat::child_window::move_child_window;
use crate::app::render::workarea_header::workarea_header_bottom_y;
use crate::app::window::resources_style::{
    RESOURCE_CONTROL_RADIUS, RESOURCE_PANEL_RADIUS, resource_glass_washes, resource_header,
};
use crate::app::window::settings_modal::catalog::settings_catalog;
use crate::app::window::*;
use crate::*;

#[path = "panel_confirm_card.rs"]
mod confirm_card;
#[path = "panel_controls.rs"]
pub(crate) mod controls;
#[path = "panel_copy.rs"]
mod copy;
// The card reads its frame's data as `super::rows`.
use super::rows;
#[path = "panel_focus_strip.rs"]
mod focus_strip;
#[path = "panel_layout.rs"]
pub(crate) mod layout;

use controls::{ControlColors, HoldLook, StripContent};
use layout::ResizeEdge;

/// From the window's trailing edge and below the header.
const PANEL_MARGIN: f32 = 12.0;
/// The closest a dragged panel comes to the window's edges.
const PANEL_EDGE: f32 = 6.0;
/// A press on the header that moves less than this is not a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// How long the main window's size must hold before the panel is placed again.
const REPOSITION_DELAY: std::time::Duration = std::time::Duration::from_millis(160);

const ICON_MIC: &str = "titlebar/microphone.svg";
const ICON_MIC_OFF: &str = "titlebar/microphone-off.svg";
const ICON_HANG_UP: &str = "titlebar/phone-off.svg";
const ICON_TOOL: &str = "titlebar/tool.svg";
const ICON_CHECK: &str = "titlebar/check.svg";
const ICON_ALERT: &str = "titlebar/alert-triangle.svg";
const ICON_CLOSE: &str = "titlebar/x.svg";
const ICON_SETTINGS: &str = "titlebar/settings.svg";
const ICON_HEADSET: &str = "titlebar/headset.svg";
const ICON_CHEVRON: &str = "titlebar/chevron-down.svg";
const ICON_HISTORY: &str = "titlebar/history.svg";
const ICON_TRASH: &str = "titlebar/trash.svg";
const ICON_FRESH: &str = "titlebar/refresh.svg";
/// The remembered conversation's accent (the speaking violet).
const MEMORY_ACCENT: u32 = 0xa78bfaff;

/// The panel's colours for the chrome's appearance.
struct PanelColors {
    background: Hsla,
    /// The panel's outer edge.
    edge: Hsla,
    /// The hairlines under the header and above the footer, and the edges inside the panel.
    border: Hsla,
    ink: Hsla,
    text: Hsla,
    muted: Hsla,
    /// A neutral button at rest and hovered (Mute, the hold button, Start again).
    raised: Hsla,
    raised_hover: Hsla,
}

impl PanelColors {
    fn controls(&self) -> ControlColors {
        ControlColors {
            ink: self.ink,
            text: self.text,
            muted: self.muted,
            border: self.border,
            background: self.background,
            raised: self.raised,
        }
    }

    /// CDXC:VoiceMode 2026-11-02 DECISION: User: "please make the voice agent panel transparent like the notifications menu please ... and also should match it's style", then picked "A. The Notifications glass, exactly" and, for captions while talking, "Same glass everywhere". The panel takes the bell dropdown's own surface, blur, edge, 8px corners, 34px header with full-height cells split by hairlines, footer hairline and washes (`popup_window_panel_surface`, `apply_frosted_menu_blur`, `resource_header`, `resource_glass_washes`), with no extra tint behind the transcript; Hang up stays solid red. Without window glass it is the dropdown's solid sidebar colour, as the bell's is. Supersedes the panel's own near-solid #1e1e21 (light #fafafa) at 97% with 12px corners.
    /// SEE-ALSO: app/window/titlebar_notifications_panel.rs, app/window/resources_style.rs.
    fn resolve() -> Self {
        let ink: Hsla = chrome_ink().into();
        let glass = window_glass_active();
        let washes = resource_glass_washes();
        Self {
            background: popup_window_panel_surface(titlebar_background()),
            edge: titlebar_popup_menu_border_color(),
            border: if glass {
                washes.hairline.into()
            } else {
                ink.opacity(0.12)
            },
            ink,
            text: ink.opacity(0.94),
            muted: ink.opacity(0.58),
            raised: if glass {
                washes.raised.into()
            } else {
                ink.opacity(0.07)
            },
            raised_hover: if glass {
                washes.raised_hover.into()
            } else {
                ink.opacity(0.13)
            },
        }
    }
}

/// The state's colour dot: green while it listens, blue while you talk, amber while it works,
/// violet while it speaks.
fn state_color(state: &str) -> Hsla {
    match state {
        "listening" => rgba(0x4ade80ff),
        "userSpeaking" => rgba(0x38bdf8ff),
        "thinking" | "toolRunning" => rgba(0xfbbf24ff),
        "speaking" => rgba(0xa78bfaff),
        _ => rgba(0x9ca3afff),
    }
    .into()
}

/// What the panel draws, copied out of the app and pushed into the window.
///
/// CDXC:VoiceMode 2026-10-20 WHY: the panel must never read the app entity while rendering: `open_window` renders the new window at once, inside the app's own update (the Voice button's click), and reading the app there panics with "already being updated" and aborts the app.
struct PanelSnapshot {
    phase: VoicePhase,
    state: String,
    tool_label: Option<String>,
    rows: Vec<VoiceRow>,
    muted: bool,
    /// A push-to-talk call, and whether Talk is held.
    push_to_talk: bool,
    talking: bool,
    /// The hold button's look and tooltip (`hold_to_talk.rs`).
    hold_look: HoldLook,
    hold_tooltip: SharedString,
    /// The header's Space keycap: `Some(lit)` while Space would hold Talk (lit while it does).
    space_key: Option<bool>,
    /// What the strip above the buttons shows: Stop talking while the agent speaks, else a hint.
    strip: StripContent,
    problem: Option<VoiceProblem>,
    provider: Option<String>,
    voice: Option<String>,
    next_voice: Option<String>,
    /// The provider's voices as (id, label), for the voice list.
    voice_options: Vec<(String, String)>,
    voice_picker_open: bool,
    speed: f32,
    memory: Option<VoiceMemory>,
    memory_state: VoiceMemoryState,
    memory_details_open: bool,
    /// The session the call is about (`None`: all sessions), and the computer it runs on when it
    /// isn't this one.
    focus: Option<VoiceFocus>,
    machine_name: Option<String>,
}

/// A drag of the panel by its header (`edge` is `None`) or a resize by an edge or corner: where
/// the mouse went down, in screen points.
struct PanelDrag {
    mouse_start: Point<Pixels>,
    moved: bool,
    edge: Option<ResizeEdge>,
}

pub(crate) struct VoicePanelWindow {
    app: WeakEntity<GhostexGpuiApp>,
    snapshot: PanelSnapshot,
    scroll: ScrollHandle,
    /// The transcript follows new lines: until the reader scrolls up, and again once they scroll
    /// back to its end (`layout::follow_after_wheel`).
    following: bool,
    /// The voice list's scroll, set when it opens so the current voice shows.
    voice_scroll: ScrollHandle,
    /// The window's background as last set; the panel stays open while window glass is turned on
    /// or off, so its render follows the setting.
    background: WindowBackgroundAppearance,
    drag: Option<PanelDrag>,
    /// The hold button is held down; releasing the mouse anywhere in the panel ends the hold.
    holding_talk: bool,
    /// Where the speed button last painted, for its menu.
    speed_trigger: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Where the gear button last painted, for its menu.
    settings_trigger: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Focused by a press on the transcript, so Cmd+C reaches the panel (`panel_copy.rs`).
    focus: gpui::FocusHandle,
}

/// "1×", "1.2×", … "2×".
fn speed_label(speed: f32) -> String {
    let text = format!("{speed:.1}");
    format!("{}×", text.strip_suffix(".0").unwrap_or(&text))
}

/// Under window glass the panel's window blurs what is behind it, as the bell dropdown's does
/// (`titlebar/popup_lifecycle.rs`).
fn panel_background_appearance() -> WindowBackgroundAppearance {
    if window_glass_active() {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Transparent
    }
}

/// The panel's frame in the main window's content coordinates, for a window of `main` size, at
/// the place the user dragged it to (its gap to the trailing edge, its top) or under the header's
/// top right, at the size the user resized it to (`kept_panel_size`) or the default one.
fn panel_frame(main: gpui::Size<Pixels>, place: Option<Point<Pixels>>) -> Bounds<Pixels> {
    let (main_width, main_height) = (main.width.as_f32(), main.height.as_f32());
    let kept = kept_panel_size();
    // The default size fits under the work area's header; a size the user chose may reach up to
    // the window's edge, where they can drag the panel.
    let room_height = if kept.is_some() {
        main_height - PANEL_EDGE * 2.0
    } else {
        main_height - workarea_header_bottom_y() - PANEL_MARGIN
    };
    let fitted = layout::clamp_panel_size(
        kept.unwrap_or(layout::DEFAULT_SIZE),
        size(px(main_width - PANEL_EDGE * 2.0), px(room_height)),
    );
    let (width, height) = (fitted.width.as_f32(), fitted.height.as_f32());
    let (trailing, top) = match place {
        Some(place) => (place.x.as_f32(), place.y.as_f32()),
        None => (
            PANEL_MARGIN,
            workarea_header_bottom_y() + PANEL_MARGIN / 2.0,
        ),
    };
    let x = (main_width - trailing - width)
        .min(main_width - width - PANEL_EDGE)
        .max(PANEL_EDGE);
    let y = top.min(main_height - height - PANEL_EDGE).max(PANEL_EDGE);
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width), px(height)),
    }
}

/// The size the user last resized the panel to: `None` until read from disk, then the kept size,
/// if any.
static PANEL_SIZE: std::sync::Mutex<Option<Option<gpui::Size<Pixels>>>> =
    std::sync::Mutex::new(None);

fn panel_size_path() -> std::path::PathBuf {
    ghostex_state_root().join("gpui-voice-panel.json")
}

fn read_panel_size() -> Option<gpui::Size<Pixels>> {
    let text = std::fs::read_to_string(panel_size_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let number = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| px(value as f32))
    };
    Some(size(number("width")?, number("height")?))
}

fn kept_panel_size() -> Option<gpui::Size<Pixels>> {
    let mut slot = PANEL_SIZE.lock().ok()?;
    *slot.get_or_insert_with(read_panel_size)
}

fn keep_panel_size(kept: gpui::Size<Pixels>) {
    if let Ok(mut slot) = PANEL_SIZE.lock() {
        *slot = Some(Some(size(
            px(kept.width.as_f32().round()),
            px(kept.height.as_f32().round()),
        )));
    }
}

/// Writes the kept size to disk, once a resize ends.
fn save_panel_size() {
    let Some(kept) = kept_panel_size() else {
        return;
    };
    let _ = std::fs::write(
        panel_size_path(),
        serde_json::json!({
            "width": kept.width.as_f32(),
            "height": kept.height.as_f32(),
        })
        .to_string(),
    );
}

/// The provider's voices with their Settings labels ("Eve"), in the order gxserver listed them.
fn voice_choices(provider: &str, voices: &[String]) -> Vec<(String, String)> {
    let key = match provider {
        "openai" => "voiceAgentOpenaiVoice",
        _ => "voiceAgentXaiVoice",
    };
    let options = settings_catalog()
        .extra_tab("voiceAgent")
        .and_then(|tab| {
            tab.sections
                .iter()
                .flat_map(|section| section.settings.iter())
                .find(|row| row.key == key)
        })
        .map(|row| row.options.clone())
        .unwrap_or_default();
    voices
        .iter()
        .map(|voice| {
            let label = options
                .iter()
                .find(|option| option.value == *voice)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| voice_name(voice));
            (voice.clone(), label)
        })
        .collect()
}

impl GhostexGpuiApp {
    /// Opens, refreshes or closes the panel to match the conversation, and redraws the Voice
    /// button.
    pub(crate) fn sync_voice_agent_panel(&mut self, cx: &mut gpui::Context<Self>) {
        cx.notify();
        // Auto-read stays quiet while a conversation is open (CDXC:VoiceMode 2026-10-21 DECISION
        // in packages/gx-chat-core/src/read_aloud/events.rs).
        crate::app::gx_chat::read_aloud_voice_conversation(self.voice_agent.active());
        if self.voice_agent.phase == VoicePhase::Idle {
            self.voice_agent.panel_reposition = None;
            self.voice_agent.panel_placed_for = None;
            if let Some(panel) = self.voice_agent.panel.take() {
                let _ = panel.update(cx, |_, window, _| window.remove_window());
            }
            return;
        }
        if let Some(panel) = self.voice_agent.panel {
            let snapshot = self.voice_panel_snapshot();
            let updated = panel.update(cx, |panel, _, cx| {
                panel.receive(snapshot);
                cx.notify();
            });
            if updated.is_ok() {
                return;
            }
            self.voice_agent.panel = None;
        }
        self.open_voice_agent_panel_window(cx);
    }

    fn open_voice_agent_panel_window(&mut self, cx: &mut gpui::Context<Self>) {
        let main = self.main_window_bounds;
        let frame = panel_frame(main.size, self.voice_agent.panel_place);
        let bounds = Bounds {
            origin: main.origin + frame.origin,
            size: frame.size,
        };
        let app = cx.entity().downgrade();
        let snapshot = self.voice_panel_snapshot();
        let main_window_native_view = self.parent_ns_view;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: self.main_window_popup_owner().display_for(bounds, cx),
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: panel_background_appearance(),
            ..Default::default()
        };
        self.voice_agent.panel = cx
            .open_window(options, move |panel_window, cx| {
                panel_window.set_background_corner_radius(px(RESOURCE_PANEL_RADIUS));
                apply_frosted_menu_blur(panel_window);
                remove_gpui_app_toast_popup_window_chrome(panel_window, main_window_native_view);
                cx.new(|cx| {
                    let scroll = ScrollHandle::new();
                    scroll.scroll_to_bottom();
                    VoicePanelWindow {
                        app,
                        snapshot,
                        scroll,
                        following: true,
                        voice_scroll: ScrollHandle::new(),
                        background: panel_background_appearance(),
                        drag: None,
                        holding_talk: false,
                        speed_trigger: Default::default(),
                        settings_trigger: Default::default(),
                        focus: cx.focus_handle(),
                    }
                })
            })
            .ok();
        self.voice_agent.panel_placed_for = Some(main.size);
    }

    /// Places the panel again after the main window changed size (it follows moves by itself, as
    /// an AppKit child window). Called from the main window's render.
    pub(crate) fn voice_agent_follow_main_window(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(placed_for) = self.voice_agent.panel_placed_for else {
            return;
        };
        if self.voice_agent.panel.is_none()
            || placed_for == self.main_window_bounds.size
            || self.voice_agent.panel_reposition.is_some()
        {
            return;
        }
        self.voice_agent.panel_reposition = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REPOSITION_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                this.voice_agent.panel_reposition = None;
                if this.voice_agent.panel_placed_for == Some(this.main_window_bounds.size) {
                    return;
                }
                this.place_voice_agent_panel(cx);
            });
        }));
    }

    /// Opens (or closes) the speed menu under the panel's speed button, `trigger` in the panel's
    /// coordinates. The menu is hosted over the main window, like every app menu, at the same spot
    /// on screen.
    fn toggle_voice_agent_speed_menu(
        &mut self,
        trigger: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        let frame = panel_frame(self.main_window_bounds.size, self.voice_agent.panel_place);
        let trigger = Bounds::new(frame.origin + trigger.origin, trigger.size);
        let current = self.voice_agent.speed;
        let mut menu = GpuiContextMenu::new();
        for speed in VOICE_AGENT_SPEEDS {
            menu = menu.menu_with_check(
                speed_label(speed),
                (speed - current).abs() < 0.01,
                Box::new(SetVoiceAgentSpeed {
                    hundredths: (speed * 100.0).round() as u16,
                }),
            );
        }
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                menu.toggle_below_for_app(app, trigger, window, cx);
            });
        });
    }

    /// Opens (or closes) the voice settings menu under the panel's gear, `trigger` in the panel's
    /// coordinates: the two billing modes of a muted call, each with its one-line explanation.
    fn toggle_voice_agent_settings_menu(
        &mut self,
        trigger: Bounds<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        let frame = panel_frame(self.main_window_bounds.size, self.voice_agent.panel_place);
        let trigger = Bounds::new(frame.origin + trigger.origin, trigger.size);
        let push_to_talk = self.voice_agent.push_to_talk_setting;
        let mode = |push_to_talk| Box::new(SetVoiceAgentCallMode { push_to_talk });
        let mut menu = GpuiContextMenu::new()
            .menu_with_check("Regular", !push_to_talk, mode(false))
            .menu_with_disabled(
                "Hands-free. Grok bills the whole call, about $0.08/min",
                true,
                mode(false),
            )
            .separator()
            .menu_with_check("Push-to-talk", push_to_talk, mode(true))
            .menu_with_disabled(
                "Hold to talk. Grok bills only audio sent and heard",
                true,
                mode(true),
            )
            .separator();
        if self.voice_agent.provider.as_deref() == Some("openai") {
            menu = menu.menu_with_disabled(
                "GPT Realtime bills only audio in both modes",
                true,
                mode(push_to_talk),
            );
        }
        menu = menu.menu_with_disabled("Applies to your next call", true, mode(push_to_talk));
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                menu.toggle_below_for_app(app, trigger, window, cx);
            });
        });
    }

    /// Moves the open panel to its place for the main window's current size.
    fn place_voice_agent_panel(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(panel) = self.voice_agent.panel else {
            return;
        };
        let size = self.main_window_bounds.size;
        let frame = panel_frame(size, self.voice_agent.panel_place);
        if move_voice_panel_window(
            panel.into(),
            self.parent_ns_view,
            self.main_window_bounds,
            frame,
            cx,
        ) {
            self.voice_agent.panel_placed_for = Some(size);
        } else {
            // This platform cannot move a child window: open it again in place.
            self.voice_agent.panel = None;
            let _ = panel.update(cx, |_, window, _| window.remove_window());
            if self.voice_agent.phase != VoicePhase::Idle {
                self.open_voice_agent_panel_window(cx);
            }
        }
    }

    /// A drag of the panel's header, `delta` from where it started: moves the panel by it, kept
    /// inside the main window, and remembers the place.
    fn drag_voice_agent_panel(
        &mut self,
        delta: Point<Pixels>,
        first: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let main = self.main_window_bounds.size;
        let frame = panel_frame(main, self.voice_agent.panel_place);
        if first || self.voice_agent.panel_drag_from.is_none() {
            self.voice_agent.panel_drag_from = Some(frame.origin);
        }
        let Some(from) = self.voice_agent.panel_drag_from else {
            return;
        };
        let wanted = from + delta;
        let placed = panel_frame(
            main,
            Some(point(main.width - wanted.x - frame.size.width, wanted.y)),
        );
        self.voice_agent.panel_place = Some(point(
            main.width - placed.origin.x - placed.size.width,
            placed.origin.y,
        ));
        self.place_voice_agent_panel(cx);
    }

    /// A resize of the panel by `edge`, `delta` from where it started: the dragged edges follow
    /// the mouse, held to the panel's size limits and inside the main window, and the new size and
    /// place are kept (the size is written to disk when the resize ends).
    fn resize_voice_agent_panel(
        &mut self,
        edge: ResizeEdge,
        delta: Point<Pixels>,
        first: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let main = self.main_window_bounds.size;
        if first || self.voice_agent.panel_resize_from.is_none() {
            self.voice_agent.panel_resize_from =
                Some(panel_frame(main, self.voice_agent.panel_place));
        }
        let Some(from) = self.voice_agent.panel_resize_from else {
            return;
        };
        let area = Bounds::new(
            point(px(PANEL_EDGE), px(PANEL_EDGE)),
            size(
                main.width - px(PANEL_EDGE * 2.0),
                main.height - px(PANEL_EDGE * 2.0),
            ),
        );
        let resized = layout::resize_frame(from, edge, delta, area);
        keep_panel_size(resized.size);
        self.voice_agent.panel_place = Some(point(main.width - resized.right(), resized.origin.y));
        self.place_voice_agent_panel(cx);
    }

    fn voice_panel_snapshot(&self) -> PanelSnapshot {
        let agent = &self.voice_agent;
        PanelSnapshot {
            phase: agent.phase,
            state: agent.state.clone(),
            tool_label: agent.tool_label.clone(),
            rows: agent.rows.clone(),
            muted: agent.muted,
            push_to_talk: agent.push_to_talk,
            talking: agent.talking,
            hold_look: self.voice_agent_hold_look(),
            hold_tooltip: self.voice_agent_hold_tooltip(),
            space_key: self.voice_agent_space_keycap(),
            strip: if agent.phase == VoicePhase::Live && agent.state == "speaking" {
                StripContent::StopTalking
            } else {
                agent
                    .hint
                    .clone()
                    .map_or(StripContent::Empty, |hint| StripContent::Hint(hint.into()))
            },
            problem: agent.problem.clone(),
            provider: agent.provider.clone(),
            voice: agent.voice.clone(),
            next_voice: agent.next_voice.clone(),
            voice_options: voice_choices(
                agent.provider.as_deref().unwrap_or_default(),
                &agent.voice_options,
            ),
            voice_picker_open: agent.voice_picker_open,
            speed: agent.speed,
            memory: agent.memory.clone(),
            memory_state: agent.memory_state,
            memory_details_open: agent.memory_details_open,
            focus: agent.focus.clone(),
            machine_name: agent
                .machine_name
                .clone()
                .filter(|_| !agent.machine.is_local()),
        }
    }
}

/// Moves the open panel to `frame`, in the main window's content coordinates (`main` is the main
/// window's bounds). False when this platform cannot move it.
///
/// CDXC:VoiceMode 2026-10-20 WHY: `move_child_window` cannot move a window on Windows, and its callers open the window again instead, which would end a drag of the panel at its first step. On Windows the panel is moved with `SetWindowPos` from a task, the Docs drawer's way (`native_docs/drawer.rs`); moving the main window already carries the panel with it (`workspace_windows/owned_windows.rs`).
#[cfg(target_os = "windows")]
fn move_voice_panel_window(
    handle: gpui::AnyWindowHandle,
    _: *mut std::ffi::c_void,
    main: Bounds<Pixels>,
    frame: Bounds<Pixels>,
    cx: &mut gpui::App,
) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };
    let Ok(Some((hwnd, scale))) = handle.update(cx, |_, window, _| {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some((handle.hwnd.get(), window.scale_factor())),
            _ => None,
        }
    }) else {
        return false;
    };
    let screen = Bounds::new(main.origin + frame.origin, frame.size);
    // `SetWindowPos` delivers `WM_SIZE` at once, and GPUI's resize callback needs the app free.
    cx.spawn(async move |_| {
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        unsafe {
            SetWindowPos(
                hwnd as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                device(screen.origin.x),
                device(screen.origin.y),
                device(screen.size.width),
                device(screen.size.height),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            );
        }
    })
    .detach();
    true
}

#[cfg(not(target_os = "windows"))]
fn move_voice_panel_window(
    handle: gpui::AnyWindowHandle,
    parent: *mut std::ffi::c_void,
    _: Bounds<Pixels>,
    frame: Bounds<Pixels>,
    cx: &mut gpui::App,
) -> bool {
    move_child_window(handle, parent, frame, cx)
}

/// What a panel control asks the app to do.
#[derive(Clone)]
enum PanelAction {
    Mute,
    StopTalking,
    HangUp,
    Close,
    StartAgain,
    OpenSettings,
    ToggleVoicePicker,
    PickVoice(String),
    /// The speed button at these bounds, in the panel's coordinates.
    ToggleSpeedMenu(Bounds<Pixels>),
    /// Push-to-talk's Talk pressed (`true`) or released.
    Talk(bool),
    /// The gear button at these bounds, in the panel's coordinates.
    ToggleSettingsMenu(Bounds<Pixels>),
    DragBy {
        delta: Point<Pixels>,
        first: bool,
    },
    ResizeBy {
        edge: ResizeEdge,
        delta: Point<Pixels>,
        first: bool,
    },
    /// A resize ended: its size is written to disk.
    ResizeEnded,
    ContinueMemory,
    StartFresh,
    ForgetMemory,
    ToggleMemoryDetails,
    /// The focus strip's All sessions: widens the call.
    AllSessions,
    /// The focus strip's title: shows the session.
    ShowFocus,
    /// A confirm card's Stop (`Some(session)`) or Cancel.
    Confirm {
        id: String,
        session: Option<String>,
    },
}

impl VoicePanelWindow {
    /// Takes the app's new snapshot. The transcript keeps following new lines from its own render
    /// (`body`); closing the voice list goes back to its end, and opening it scrolls to the current
    /// voice.
    fn receive(&mut self, snapshot: PanelSnapshot) {
        if self.snapshot.voice_picker_open && !snapshot.voice_picker_open {
            self.following = true;
        }
        if !self.snapshot.voice_picker_open && snapshot.voice_picker_open {
            let current = snapshot.voice.as_deref();
            let index = snapshot
                .voice_options
                .iter()
                .position(|(voice, _)| Some(voice.as_str()) == current);
            self.voice_scroll
                .set_offset(point(px(0.0), -layout::voice_list_scroll_top(index)));
        }
        self.snapshot = snapshot;
    }

    /// How far the transcript's view is above its end, as last laid out.
    fn distance_to_end(&self) -> f32 {
        (self.scroll.max_offset().y + self.scroll.offset().y).as_f32()
    }

    /// A wheel or trackpad scroll over the transcript: up stops following, back to the end
    /// resumes it.
    fn transcript_wheel(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(layout::WHEEL_LINE)).y.as_f32();
        let scrollable = self.scroll.max_offset().y > px(0.5);
        let following =
            layout::follow_after_wheel(self.following, self.distance_to_end(), scrollable, delta);
        if following != self.following {
            self.following = following;
            cx.notify();
        }
    }

    fn run(&mut self, action: PanelAction, cx: &mut Context<Self>) {
        let app = self.app.clone();
        // Deferred: the app's answer updates (or removes) this very window.
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| match action {
                PanelAction::ToggleVoicePicker => app.toggle_voice_agent_voice_picker(cx),
                PanelAction::PickVoice(voice) => app.pick_voice_agent_voice(voice, cx),
                PanelAction::Talk(down) => app.voice_agent_button_hold(down, cx),
                PanelAction::ToggleSettingsMenu(trigger) => {
                    app.toggle_voice_agent_settings_menu(trigger, cx)
                }
                PanelAction::ToggleSpeedMenu(trigger) => {
                    app.toggle_voice_agent_speed_menu(trigger, cx)
                }
                PanelAction::DragBy { delta, first } => {
                    app.drag_voice_agent_panel(delta, first, cx)
                }
                PanelAction::ResizeBy { edge, delta, first } => {
                    app.resize_voice_agent_panel(edge, delta, first, cx)
                }
                PanelAction::ResizeEnded => {
                    app.voice_agent.panel_resize_from = None;
                    save_panel_size();
                }
                PanelAction::ContinueMemory => app.continue_voice_agent_memory(cx),
                PanelAction::StartFresh => app.start_voice_agent_fresh(cx),
                PanelAction::ForgetMemory => app.forget_voice_agent_memory(cx),
                PanelAction::ToggleMemoryDetails => app.toggle_voice_agent_memory_details(cx),
                PanelAction::Mute => app.toggle_voice_agent_mute(cx),
                PanelAction::StopTalking => app.interrupt_voice_agent(cx),
                PanelAction::HangUp => app.hang_up_voice_agent(cx),
                PanelAction::Close => app.close_voice_agent_panel(cx),
                PanelAction::StartAgain => {
                    let about = app.voice_agent.requested.clone();
                    let gbot = app.voice_agent.requested_gbot.clone();
                    app.close_voice_agent_panel(cx);
                    app.start_voice_call(about, gbot, cx);
                }
                PanelAction::AllSessions => app.voice_call_all_sessions(cx),
                PanelAction::Confirm { id, session } => {
                    app.answer_voice_agent_confirm(&id, session.as_deref(), cx)
                }
                PanelAction::ShowFocus => {
                    if let Some(key) = app
                        .voice_agent
                        .focus
                        .as_ref()
                        .map(|focus| focus.key.clone())
                    {
                        app.show_voice_call_session(&key, cx);
                    }
                }
                PanelAction::OpenSettings => {
                    app.close_voice_agent_panel(cx);
                    app.open_gpui_settings_voice_agent_page(cx);
                }
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: Option<&'static str>,
        action: PanelAction,
        colors: &PanelColors,
        danger: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (fill, hover, foreground): (Hsla, Hsla, Hsla) = if danger {
            (
                rgba(0xdc2626ff).into(),
                rgba(0xef4444ff).into(),
                gpui::white(),
            )
        } else {
            (colors.raised, colors.raised_hover, colors.text)
        };
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(SharedString::new_static(label.unwrap_or(id)))
            .h(px(28.0))
            .px(px(if label.is_some() { 10.0 } else { 7.0 }))
            .gap(px(6.0))
            .items_center()
            .rounded(px(RESOURCE_CONTROL_RADIUS))
            .when(!danger, |this| this.border_1().border_color(colors.border))
            .bg(fill)
            .text_color(foreground)
            .text_size(px(12.5))
            .cursor_default()
            .hover(move |this| this.bg(hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener({
                    let action = action.clone();
                    move |this, _: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.run(action.clone(), cx);
                    }
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let panel = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| this.run(action.clone(), cx));
                }
            })
            .child(titlebar_svg_icon(icon, 14.0, foreground))
            .when_some(label, |this, label| this.child(label))
            .into_any_element()
    }

    fn header(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, busy): (String, bool) = match snapshot.phase {
            VoicePhase::Starting => ("Connecting…".to_string(), true),
            VoicePhase::Ending => ("Hanging up…".to_string(), true),
            VoicePhase::Ended => ("Voice agent stopped".to_string(), false),
            VoicePhase::Live if snapshot.talking => ("Hearing you".to_string(), false),
            VoicePhase::Idle | VoicePhase::Live => match snapshot.state.as_str() {
                "listening" if snapshot.muted => ("Muted".to_string(), false),
                "listening" if snapshot.push_to_talk => ("Hold to talk".to_string(), false),
                "listening" => ("Listening".to_string(), false),
                "userSpeaking" => ("Hearing you".to_string(), false),
                "thinking" => ("Thinking…".to_string(), true),
                "toolRunning" => (
                    snapshot
                        .tool_label
                        .clone()
                        .unwrap_or_else(|| "Working…".to_string()),
                    true,
                ),
                "speaking" => ("Speaking".to_string(), false),
                _ => ("Connecting…".to_string(), true),
            },
        };
        let indicator: AnyElement = if snapshot.phase == VoicePhase::Ended {
            titlebar_svg_icon(ICON_ALERT, 14.0, rgba(0xfbbf24ff).into()).into_any_element()
        } else if busy {
            canvas(
                |_, _, _| {},
                |bounds, _, window, _| {
                    window.request_animation_frame();
                    paint_agent_gui_loading_spinner(bounds, window);
                },
            )
            .size(px(12.0))
            .into_any_element()
        } else {
            let color = if snapshot.muted {
                colors.muted
            } else {
                state_color(&snapshot.state)
            };
            div()
                .size(px(9.0))
                .rounded_full()
                .bg(color)
                .into_any_element()
        };
        let pickers = matches!(snapshot.phase, VoicePhase::Starting | VoicePhase::Live);
        // The bell dropdown's header: 34px with a hairline under it, the title on the left and
        // full-height cells split by hairlines on the right.
        resource_header()
            .id("voice-panel-header")
            .cursor(if self.drag.as_ref().is_some_and(|drag| drag.moved) {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    this.drag = Some(PanelDrag {
                        mouse_start: window.bounds().origin + event.position,
                        moved: false,
                        edge: None,
                    });
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .size(px(14.0))
                            .items_center()
                            .justify_center()
                            .child(indicator),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(colors.ink.opacity(0.96))
                            .child(label),
                    )
                    .children(
                        snapshot
                            .space_key
                            .map(|lit| controls::keycap("Space", lit, colors.controls())),
                    ),
            )
            .child(self.voice_button(snapshot, colors, !pickers, cx))
            .when(pickers, |this| {
                this.child(self.speed_button(snapshot, colors, cx))
                    .child(self.settings_button(colors, cx))
            })
            .into_any_element()
    }

    /// One of the header's full-height cells, as the bell dropdown draws its actions
    /// (`render_notification_header_action`): a hairline on its left, 12px text at 78% ink and a
    /// 14% wash under the pointer. The last cell follows the panel's top-right corner.
    fn header_cell(
        id: &'static str,
        colors: &PanelColors,
        last: bool,
        active: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let ink = colors.ink;
        h_flex()
            .id(id)
            .h_full()
            .flex_shrink_0()
            .px(px(8.0))
            .gap(px(4.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(colors.border)
            .when(last, |this| {
                this.rounded_tr(px(RESOURCE_PANEL_RADIUS - 1.0))
            })
            .whitespace_nowrap()
            .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
            .text_color(ink.opacity(if active { 0.94 } else { 0.78 }))
            .when(active, |this| this.bg(ink.opacity(0.14)))
            .cursor_default()
    }

    /// The header cells' icon colour, the bell dropdown's 78% ink.
    fn header_icon_color(colors: &PanelColors) -> Hsla {
        colors.ink.opacity(0.78)
    }

    /// The gear right of the speed button: opens the voice settings menu (what muting bills).
    fn settings_button(&self, colors: &PanelColors, cx: &mut Context<Self>) -> AnyElement {
        let ink = colors.ink;
        let trigger = self.settings_trigger.clone();
        let open_menu = move |this: &mut Self, cx: &mut Context<Self>| {
            if let Some(bounds) = this.settings_trigger.get() {
                this.run(PanelAction::ToggleSettingsMenu(bounds), cx);
            }
        };
        Self::header_cell("voice-panel-settings", colors, true, false)
            .role(gpui::Role::Button)
            .aria_label(SharedString::new_static("Voice settings"))
            .w(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
            .px_0()
            .hover(move |this| this.bg(ink.opacity(0.14)))
            .on_prepaint(move |bounds, _, _| trigger.set(Some(bounds)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    open_menu(this, cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let panel = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| open_menu(this, cx));
                }
            })
            .child(titlebar_svg_icon(
                ICON_SETTINGS,
                14.0,
                Self::header_icon_color(colors),
            ))
            .into_any_element()
    }

    /// The current speed ("1.2×"), right of the voice picker: opens the speed menu.
    ///
    /// CDXC:VoiceMode 2026-10-20 DECISION: User: "Move the voice speed control from the bottom bar into the top-right picker. Add a smaller button to its right that opens a speed menu when clicked, so users can pick a different speed without the bottom bar." The menu is the app's shared dropdown (`GpuiContextMenu`, keyboard and Escape included) listing 1× to 2× with the current one ticked. Supersedes the row of speed buttons above the panel's bottom bar (the phone keeps its speed buttons for now).
    fn speed_button(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = colors.ink;
        let trigger = self.speed_trigger.clone();
        let open_menu = move |this: &mut Self, cx: &mut Context<Self>| {
            if let Some(bounds) = this.speed_trigger.get() {
                this.run(PanelAction::ToggleSpeedMenu(bounds), cx);
            }
        };
        Self::header_cell("voice-panel-speed", colors, false, false)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!(
                "Speaking speed {}. Change the speed",
                speed_label(snapshot.speed)
            )))
            .hover(move |this| this.bg(ink.opacity(0.14)).text_color(ink.opacity(0.94)))
            .on_prepaint(move |bounds, _, _| trigger.set(Some(bounds)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    open_menu(this, cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let panel = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| open_menu(this, cx));
                }
            })
            .child(speed_label(snapshot.speed))
            .child(titlebar_svg_icon(
                ICON_CHEVRON,
                11.0,
                Self::header_icon_color(colors),
            ))
            .into_any_element()
    }

    /// "Grok · Eve ⌄": opens the voice list once the provider named its voices.
    fn voice_button(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = match (&snapshot.provider, &snapshot.voice) {
            (Some(provider), Some(voice)) => {
                format!("{} · {}", provider_name(provider), voice_name(voice))
            }
            _ => "Voice agent".to_string(),
        };
        let pickable =
            !snapshot.voice_options.is_empty() && matches!(snapshot.phase, VoicePhase::Live);
        let open = snapshot.voice_picker_open;
        let ink = colors.ink;
        let icon = Self::header_icon_color(colors);
        Self::header_cell("voice-panel-voice", colors, last, open)
            .child(titlebar_svg_icon(ICON_HEADSET, 14.0, icon))
            .child(label)
            .when(pickable, |this| {
                this.role(gpui::Role::Button)
                    .aria_label(SharedString::new_static("Change the voice"))
                    .hover(move |this| this.bg(ink.opacity(0.14)).text_color(ink.opacity(0.94)))
                    .child(titlebar_svg_icon(ICON_CHEVRON, 11.0, icon))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.run(PanelAction::ToggleVoicePicker, cx);
                        }),
                    )
                    .on_a11y_action(gpui::AccessibleAction::Click, {
                        let panel = cx.entity().downgrade();
                        move |_, _, cx| {
                            let _ = panel.update(cx, |this, cx| {
                                this.run(PanelAction::ToggleVoicePicker, cx)
                            });
                        }
                    })
            })
            .into_any_element()
    }

    /// The voice list that replaces the transcript while it is open.
    fn voice_picker(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = snapshot.voice.clone().unwrap_or_default();
        let hover = colors.ink.opacity(0.08);
        let selected_fill = colors.ink.opacity(0.10);
        let rows = snapshot.voice_options.iter().map(|(voice, label)| {
            let selected = *voice == current;
            let next = snapshot.next_voice.as_deref() == Some(voice.as_str());
            let pick = PanelAction::PickVoice(voice.clone());
            h_flex()
                .id(SharedString::from(format!("voice-panel-voice-{voice}")))
                .role(gpui::Role::MenuItemRadio)
                .aria_label(SharedString::from(label.clone()))
                .aria_toggled(if selected {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .h(px(layout::VOICE_ROW_HEIGHT))
                .flex_shrink_0()
                .px(px(10.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(6.0))
                .text_size(px(12.5))
                .text_color(colors.text)
                .cursor_default()
                .when(selected, |this| this.bg(selected_fill))
                .hover(move |this| this.bg(hover))
                .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                .when(next, |this| {
                    this.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted)
                            .child("Next conversation"),
                    )
                })
                .when(selected, |this| {
                    this.child(titlebar_svg_icon(ICON_CHECK, 12.0, colors.text))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener({
                        let pick = pick.clone();
                        move |this, _: &MouseUpEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.run(pick.clone(), cx);
                        }
                    }),
                )
                .on_a11y_action(gpui::AccessibleAction::Click, {
                    let panel = cx.entity().downgrade();
                    move |_, _, cx| {
                        let _ = panel.update(cx, |this, cx| this.run(pick.clone(), cx));
                    }
                })
        });
        v_flex()
            .id("voice-panel-voices")
            .role(gpui::Role::Menu)
            .aria_label(SharedString::new_static("Voices"))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.voice_scroll)
            .p(px(layout::VOICE_LIST_PADDING))
            .gap(px(layout::VOICE_ROW_GAP))
            .children(rows)
            .into_any_element()
    }

    /// Follows a header drag with the panel's window, through moves outside the panel too (the
    /// press that started it keeps the mouse).
    fn drag_listener(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = cx.entity().downgrade();
        // The drag's cursor stays wherever the mouse goes over the panel.
        let cursor = match self.drag.as_ref() {
            Some(PanelDrag {
                edge: Some(edge), ..
            }) => edge.cursor(),
            Some(PanelDrag { moved: true, .. }) => CursorStyle::ClosedHand,
            _ => CursorStyle::OpenHand,
        };
        let listener = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event({
                    let panel = panel.clone();
                    move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let mouse = window.bounds().origin + event.position;
                        let pressed = event.pressed_button == Some(MouseButton::Left);
                        let _ = panel.update(cx, |this, cx| this.drag_moved(mouse, pressed, cx));
                    }
                });
                window.on_mouse_event({
                    let panel = panel.clone();
                    move |_: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let _ = panel.update(cx, |this, cx| this.end_drag(cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();
        div()
            .absolute()
            .size_full()
            .cursor(cursor)
            .child(listener)
            .into_any_element()
    }

    /// The icon-only hold button beside Mute (`panel_controls.rs`); pressing it while the agent
    /// talks cuts the reply in a push-to-talk call.
    fn hold_button(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = cx.entity().downgrade();
        controls::hold_button(
            snapshot.hold_look,
            snapshot.hold_tooltip.clone(),
            colors.controls(),
            move |_, cx| {
                let _ = panel.update(cx, |this, cx| {
                    this.holding_talk = true;
                    this.run(PanelAction::Talk(true), cx);
                    cx.notify();
                });
            },
        )
    }

    /// The strip between the transcript and the buttons: Stop talking while the agent speaks, a
    /// hint after a tap or a refused press.
    fn status_strip(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = cx.entity().downgrade();
        controls::status_strip(snapshot.strip.clone(), colors.controls(), move |_, cx| {
            let _ = panel.update(cx, |this, cx| this.run(PanelAction::StopTalking, cx));
        })
    }

    /// Ends a held Talk when the mouse is released anywhere, the button or not.
    fn talk_release_listener(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event({
                    let panel = panel.clone();
                    move |_: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let _ = panel.update(cx, |this, cx| {
                            if this.holding_talk {
                                this.holding_talk = false;
                                this.run(PanelAction::Talk(false), cx);
                                cx.notify();
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn drag_moved(&mut self, mouse: Point<Pixels>, pressed: bool, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if !pressed {
            self.end_drag(cx);
            return;
        }
        let delta = mouse - drag.mouse_start;
        if !drag.moved && delta.x.abs().max(delta.y.abs()) < px(DRAG_THRESHOLD) {
            return;
        }
        let first = !drag.moved;
        if first {
            drag.moved = true;
            cx.notify();
        }
        let action = match drag.edge {
            Some(edge) => PanelAction::ResizeBy { edge, delta, first },
            None => PanelAction::DragBy { delta, first },
        };
        self.run(action, cx);
    }

    /// The mouse let go of a move or a resize; a resize that changed the size keeps it on disk.
    fn end_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if drag.moved && drag.edge.is_some() {
            self.run(PanelAction::ResizeEnded, cx);
        }
        cx.notify();
    }

    /// The panel's resize edges and corners (`layout::resize_edges`): a press starts a resize that
    /// the drag listener follows.
    fn resize_edges(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let panel = cx.entity().downgrade();
        layout::resize_edges(move |edge, event, window, cx| {
            let mouse_start = window.bounds().origin + event.position;
            let _ = panel.update(cx, |this, cx| {
                this.drag = Some(PanelDrag {
                    mouse_start,
                    moved: false,
                    edge: Some(edge),
                });
                cx.notify();
            });
        })
    }

    fn row(at: usize, row: &VoiceRow, colors: &PanelColors) -> AnyElement {
        match row {
            VoiceRow::Caption {
                role,
                text,
                paragraphs,
                cut,
                ..
            } => {
                let you = *role == VoiceRole::User;
                let parts = if paragraphs.len() > 1 {
                    paragraphs.clone()
                } else {
                    vec![text.clone()]
                };
                let last = parts.len() - 1;
                v_flex()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colors.muted)
                            .child(if you { "You" } else { "Agent" }),
                    )
                    .child(
                        v_flex()
                            .gap(px(7.0))
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(if you {
                                colors.ink.opacity(0.78)
                            } else {
                                colors.text
                            })
                            .children(parts.into_iter().enumerate().map(|(part_at, part)| {
                                div().child(Self::selectable_part(
                                    at,
                                    part_at,
                                    if *cut && part_at == last {
                                        format!("{part}…")
                                    } else {
                                        part
                                    },
                                ))
                            })),
                    )
                    .into_any_element()
            }
            VoiceRow::Tool { label, ok, .. } => h_flex()
                .gap(px(6.0))
                .items_center()
                .text_size(px(11.5))
                .text_color(colors.muted)
                .child(titlebar_svg_icon(ICON_TOOL, 12.0, colors.muted))
                .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                .when(*ok == Some(true), |this| {
                    this.child(titlebar_svg_icon(ICON_CHECK, 12.0, rgba(0x4ade80ff).into()))
                })
                .when(*ok == Some(false), |this| {
                    this.child(titlebar_svg_icon(ICON_CLOSE, 12.0, rgba(0xf87171ff).into()))
                })
                .into_any_element(),
            VoiceRow::Announcement { text } => div()
                .text_size(px(11.5))
                .italic()
                .text_color(colors.muted)
                .child(text.clone())
                .into_any_element(),
            // Drawn by `confirm_row`, which needs the panel for its buttons.
            VoiceRow::Confirm(_) => div().into_any_element(),
        }
    }

    fn confirm_row(
        &self,
        card: &rows::ConfirmCard,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = cx.entity().downgrade();
        let id = card.id.clone();
        confirm_card::confirm_card(card, colors.controls(), move |session, _, cx| {
            let action = PanelAction::Confirm {
                id: id.clone(),
                session,
            };
            let _ = panel.update(cx, |this, cx| this.run(action, cx));
        })
    }

    fn body(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if snapshot.voice_picker_open {
            return self.voice_picker(snapshot, colors, cx);
        }
        // Asked from the panel's own render, so its own prepaint takes it in this frame (the WHY
        // in `panel_layout.rs`).
        if self.following {
            self.scroll.scroll_to_bottom();
        }
        let rows = v_flex()
            .id("voice-panel-rows")
            .role(gpui::Role::Log)
            .aria_label(SharedString::new_static("Conversation"))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(
                cx.listener(|this, event: &ScrollEvent, _, cx| this.transcript_wheel(event, cx)),
            )
            // CDXC:VoiceMode 2026-10-24 WHY: more room between rows so a reply, its tool row and an announcement read as separate lines; the user's decision is in gxserver's `paragraphs.rs`.
            .gap(px(14.0))
            .px(px(12.0))
            .py(px(10.0))
            .on_mouse_down(MouseButton::Left, {
                let focus = self.focus.clone();
                move |_, window, cx| Self::take_keyboard(&focus, window, cx)
            })
            .children(snapshot.rows.iter().enumerate().map(|(at, row)| match row {
                VoiceRow::Confirm(card) => self.confirm_row(card, colors, cx),
                VoiceRow::Caption { text, .. } => {
                    self.copyable_line(Self::row(at, row, colors), at, text.clone(), cx)
                }
                row => Self::row(at, row, colors),
            }));
        let empty = snapshot.rows.is_empty() && snapshot.problem.is_none();
        let memory = self.memory_block(snapshot, colors, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .when_some(memory, |this, memory| this.child(memory))
            // Laid out like the bell dropdown's "No notifications yet" (`render_notifications_empty_state`).
            .when(empty, |this| {
                this.child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap(px(6.0))
                        .px(px(24.0))
                        .text_center()
                        .child(titlebar_svg_icon(
                            ICON_MIC,
                            32.0,
                            colors.ink.opacity(0.35),
                        ))
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(13.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(colors.ink.opacity(0.86))
                                .child(match snapshot.phase {
                                    VoicePhase::Live => "Go ahead, I'm listening.",
                                    _ => "Starting the voice agent…",
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .line_height(px(16.0))
                                .text_color(colors.ink.opacity(0.50))
                                .child("Ask what your agents are doing, send one a prompt, answer a question, or start a new agent."),
                        ),
                )
            })
            .when(!empty, |this| {
                let away = layout::shows_scroll_to_bottom(self.following, self.distance_to_end());
                let panel = cx.entity().downgrade();
                this.child(
                    v_flex()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(rows)
                        .when(away, |this| {
                            this.child(layout::scroll_to_bottom_pill(
                                colors.controls(),
                                move |_, cx| {
                                    let _ = panel.update(cx, |this, cx| {
                                        this.following = true;
                                        cx.notify();
                                    });
                                },
                            ))
                        }),
                )
            })
            .when_some(snapshot.problem.clone(), |this, problem| {
                this.child(
                    h_flex()
                        .mx(px(12.0))
                        .mb(px(8.0))
                        .p(px(10.0))
                        .gap(px(8.0))
                        .items_start()
                        .rounded(px(8.0))
                        .bg(rgba(0xfbbf2418))
                        .border_1()
                        .border_color(rgba(0xfbbf2440))
                        .child(
                            div()
                                .pt(px(1.0))
                                .child(titlebar_svg_icon(ICON_ALERT, 14.0, rgba(0xfbbf24ff).into())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(colors.text)
                                .child(problem.message),
                        ),
                )
            })
            .into_any_element()
    }

    /// The remembered last conversation, above the transcript while the call is live: the
    /// Continue / Start fresh card, the "Remembering" line (click it for the whole summary), or
    /// "Forgot the last conversation."
    fn memory_block(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if snapshot.phase != VoicePhase::Live || snapshot.voice_picker_open {
            return None;
        }
        let accent: Hsla = rgba(MEMORY_ACCENT).into();
        match (snapshot.memory_state, &snapshot.memory) {
            (VoiceMemoryState::Offered, Some(memory)) => {
                let ago = memory.ago();
                let gist = if memory.gist.is_empty() {
                    memory.summary.clone()
                } else {
                    memory.gist.clone()
                };
                Some(
                    v_flex()
                        .id("voice-panel-memory-card")
                        .role(gpui::Role::Group)
                        .aria_label(SharedString::new_static("Last conversation"))
                        .flex_shrink_0()
                        .mx(px(12.0))
                        .mt(px(10.0))
                        .p(px(10.0))
                        .gap(px(7.0))
                        .rounded(px(10.0))
                        .bg(colors.ink.opacity(0.05))
                        .border_1()
                        .border_color(colors.border)
                        .child(
                            h_flex()
                                .gap(px(6.0))
                                .items_center()
                                .text_size(px(11.5))
                                .text_color(colors.muted)
                                .child(titlebar_svg_icon(ICON_HISTORY, 12.0, accent))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(colors.text)
                                        .child("Last conversation"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .when(!ago.is_empty(), |this| {
                                            this.child(format!("· {ago}"))
                                        }),
                                )
                                .child(self.button(
                                    "voice-panel-memory-forget",
                                    ICON_TRASH,
                                    None,
                                    PanelAction::ForgetMemory,
                                    colors,
                                    false,
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.5))
                                .text_color(colors.text.opacity(0.85))
                                .child(gist),
                        )
                        .child(
                            h_flex()
                                .gap(px(6.0))
                                .child(self.button(
                                    "voice-panel-memory-continue",
                                    ICON_HISTORY,
                                    Some("Continue"),
                                    PanelAction::ContinueMemory,
                                    colors,
                                    false,
                                    cx,
                                ))
                                .child(self.button(
                                    "voice-panel-memory-fresh",
                                    ICON_FRESH,
                                    Some("Start fresh"),
                                    PanelAction::StartFresh,
                                    colors,
                                    false,
                                    cx,
                                )),
                        )
                        .into_any_element(),
                )
            }
            (VoiceMemoryState::Continued, Some(memory)) => {
                let ago = memory.ago();
                let open = snapshot.memory_details_open;
                let hover = colors.ink.opacity(0.06);
                Some(
                    v_flex()
                        .flex_shrink_0()
                        .mx(px(12.0))
                        .mt(px(10.0))
                        .p(px(6.0))
                        .gap(px(6.0))
                        .rounded(px(8.0))
                        .bg(accent.opacity(0.08))
                        .border_1()
                        .border_color(accent.opacity(0.22))
                        .child(
                            h_flex()
                                .gap(px(6.0))
                                .items_center()
                                .child(
                                    h_flex()
                                        .id("voice-panel-memory-remembering")
                                        .role(gpui::Role::Button)
                                        .aria_label(SharedString::new_static(
                                            "Show what the agent remembers",
                                        ))
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(24.0))
                                        .px(px(4.0))
                                        .gap(px(6.0))
                                        .items_center()
                                        .rounded(px(6.0))
                                        .text_size(px(11.5))
                                        .text_color(colors.muted)
                                        .cursor_default()
                                        .hover(move |this| this.bg(hover))
                                        .child(titlebar_svg_icon(ICON_HISTORY, 12.0, accent))
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(colors.text)
                                                .child("Remembering"),
                                        )
                                        .child(div().flex_1().min_w_0().truncate().child(
                                            if ago.is_empty() {
                                                "last conversation".to_string()
                                            } else {
                                                format!("last conversation ({ago})")
                                            },
                                        ))
                                        .child(titlebar_svg_icon(ICON_CHEVRON, 11.0, colors.muted))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|_, _: &MouseDownEvent, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                                this.run(PanelAction::ToggleMemoryDetails, cx);
                                            }),
                                        )
                                        .on_a11y_action(gpui::AccessibleAction::Click, {
                                            let panel = cx.entity().downgrade();
                                            move |_, _, cx| {
                                                let _ = panel.update(cx, |this, cx| {
                                                    this.run(PanelAction::ToggleMemoryDetails, cx)
                                                });
                                            }
                                        }),
                                )
                                .child(self.button(
                                    "voice-panel-memory-forget",
                                    ICON_TRASH,
                                    Some("Forget"),
                                    PanelAction::ForgetMemory,
                                    colors,
                                    false,
                                    cx,
                                )),
                        )
                        .when(open, |this| {
                            this.child(
                                div()
                                    .px(px(4.0))
                                    .pb(px(2.0))
                                    .text_size(px(12.0))
                                    .line_height(px(16.5))
                                    .text_color(colors.text.opacity(0.85))
                                    .child(memory.summary.clone()),
                            )
                        })
                        .into_any_element(),
                )
            }
            (VoiceMemoryState::Forgotten, _) => Some(
                h_flex()
                    .flex_shrink_0()
                    .mx(px(12.0))
                    .mt(px(10.0))
                    .gap(px(6.0))
                    .items_center()
                    .text_size(px(11.5))
                    .text_color(colors.muted)
                    .child(titlebar_svg_icon(ICON_TRASH, 12.0, colors.muted))
                    .child("Forgot the last conversation.")
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    fn footer(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut left: Vec<AnyElement> = Vec::new();
        let mut right: Vec<AnyElement> = Vec::new();
        match snapshot.phase {
            VoicePhase::Starting | VoicePhase::Live => {
                left.push(self.button(
                    "voice-panel-mute",
                    if snapshot.muted {
                        ICON_MIC_OFF
                    } else {
                        ICON_MIC
                    },
                    Some(if snapshot.muted { "Unmute" } else { "Mute" }),
                    PanelAction::Mute,
                    colors,
                    false,
                    cx,
                ));
                left.push(self.hold_button(snapshot, colors, cx));
                right.push(self.button(
                    "voice-panel-hang-up",
                    ICON_HANG_UP,
                    Some("Hang up"),
                    PanelAction::HangUp,
                    colors,
                    true,
                    cx,
                ));
            }
            VoicePhase::Ending => {}
            VoicePhase::Ended | VoicePhase::Idle => {
                if snapshot
                    .problem
                    .as_ref()
                    .is_some_and(VoiceProblem::wants_settings)
                {
                    left.push(self.button(
                        "voice-panel-open-settings",
                        ICON_SETTINGS,
                        Some("Open Settings"),
                        PanelAction::OpenSettings,
                        colors,
                        false,
                        cx,
                    ));
                } else if snapshot
                    .problem
                    .as_ref()
                    .is_none_or(VoiceProblem::can_start_again)
                {
                    left.push(self.button(
                        "voice-panel-start-again",
                        ICON_MIC,
                        Some("Start again"),
                        PanelAction::StartAgain,
                        colors,
                        false,
                        cx,
                    ));
                }
                right.push(self.button(
                    "voice-panel-close",
                    ICON_CLOSE,
                    Some("Close"),
                    PanelAction::Close,
                    colors,
                    false,
                    cx,
                ));
            }
        }
        h_flex()
            .h(px(44.0))
            .flex_shrink_0()
            .px(px(10.0))
            .gap(px(6.0))
            .items_center()
            .border_t_1()
            .border_color(colors.border)
            .children(left)
            .child(div().flex_1())
            .children(right)
            .into_any_element()
    }
}

impl Render for VoicePanelWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = panel_background_appearance();
        if background != self.background {
            window.set_background_appearance(background);
            self.background = background;
        }
        // The panel stays open, so it follows the Menu blur slider here rather than at open
        // (CDXC:Theming 2026-10-15 on `apply_frosted_menu_blur`).
        #[cfg(target_os = "macos")]
        apply_frosted_menu_blur(window);
        let snapshot = &self.snapshot;
        let colors = PanelColors::resolve();
        v_flex()
            .id("voice-panel")
            .role(gpui::Role::Group)
            .aria_label(SharedString::new_static("Voice agent"))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, window, cx| {
                Self::copy_key(event, window, cx)
            }))
            .size_full()
            .overflow_hidden()
            .rounded(px(RESOURCE_PANEL_RADIUS))
            .border_1()
            .border_color(colors.edge)
            .bg(colors.background)
            .font_family(crate::ui_fonts::UI_FONT)
            .child(gpui_base::TextSelectionLayer)
            .child(self.header(snapshot, &colors, cx))
            .children(self.focus_strip(snapshot, &colors, cx))
            .child(self.body(snapshot, &colors, cx))
            .when(
                matches!(snapshot.phase, VoicePhase::Starting | VoicePhase::Live),
                |this| this.child(self.status_strip(snapshot, &colors, cx)),
            )
            .child(self.footer(snapshot, &colors, cx))
            .when(self.holding_talk, |this| {
                this.child(self.talk_release_listener(cx))
            })
            .children(self.resize_edges(cx))
            .when(self.drag.is_some(), |this| {
                this.child(self.drag_listener(cx))
            })
            .into_any_element()
    }
}
