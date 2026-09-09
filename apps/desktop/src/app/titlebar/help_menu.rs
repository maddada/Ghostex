// Titlebar Help: the question-mark button, its sample-question popup menu, and
// the flow that turns a picked question into a Ghostex Help quick chat.

use std::time::Duration;

use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui_component::menu::PopupMenu;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::ElementExt as _;

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// CDXC:Onboarding 2026-09-09 DECISION:
/// User: add a question-mark button to the titlebar that opens a dropdown of seven useful sample questions (for example making Claude control Codex, or matching the terminal width to the chat width); picking one starts a Ghostex Help chat.
/// The first row is "Ask anything about Ghostex", and the sample rows show a short summary label while the full question is what gets staged.
/// Picking a row never sends: the chat opens with the question as an editable draft and the user presses Enter (see createGhostexHelpChat in the sidebar runtime).
pub(crate) struct GpuiTitlebarHelpQuestion {
    /// Titlebar icon asset for the row; each question gets its own so the
    /// menu does not repeat one glyph seven times.
    pub(crate) icon_path: &'static str,
    /// The short row label shown in the menu.
    pub(crate) label: &'static str,
    /// The full question sent as the chat's first message.
    pub(crate) question: &'static str,
}

pub(crate) const GPUI_TITLEBAR_HELP_QUESTIONS: &[GpuiTitlebarHelpQuestion] = &[
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/robot.svg",
        label: "Let Claude Code control Codex",
        question: "How can I make Claude Code control Codex and other agents?",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/arrows-diagonal-expand.svg",
        label: "Match terminal width to chat",
        question: "Make the terminal width match the chat width.",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/layout-sidebar-right.svg",
        label: "Sidebar on the right, narrower",
        question: "Move the sidebar to the right side and make it narrower.",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/world.svg",
        label: "Use Ghostex from my phone",
        question: "How do I use Ghostex from my phone or another computer?",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/clock.svg",
        label: "Run an agent on a schedule",
        question: "Set up an automation that runs an agent on a schedule.",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/bell.svg",
        label: "Notify me when an agent finishes",
        question: "Play a sound and notify me when an agent finishes.",
    },
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/layout-board-split.svg",
        label: "Kanban board and starting work",
        question: "What does the Kanban board do, and how do I start work on a card?",
    },
];

/// Menu index of the open-ended row; the sample questions follow it.
pub(crate) const GPUI_TITLEBAR_HELP_ASK_ANYTHING_INDEX: usize = 0;

const GPUI_TITLEBAR_HELP_ASK_ANYTHING_LABEL: &str = "Ask anything about Ghostex";
const GPUI_TITLEBAR_HELP_ASK_ANYTHING_ICON: &str = "titlebar/sparkles.svg";
const GPUI_TITLEBAR_HELP_HEADER_SUMMARY: &str = "Explain Ghostex or change a setting for you.";

/// The bundled skill every Help chat invokes in its first message; the skill
/// itself is installed by `ghostex guide install-skill`.
const GHOSTEX_HELP_SKILL_INVOCATION: &str = "$ghostex-help";

/// The draft staged in the new chat's composer for a Help menu row: the bare
/// skill mention plus a trailing space for the open-ended row (the user types
/// the question after it), otherwise the full sample question after the
/// mention. Nothing is submitted; the user edits and presses Enter.
pub(crate) fn gpui_titlebar_help_question_prompt(question_index: usize) -> Option<String> {
    if question_index == GPUI_TITLEBAR_HELP_ASK_ANYTHING_INDEX {
        return Some(format!("{GHOSTEX_HELP_SKILL_INVOCATION} "));
    }
    let question = GPUI_TITLEBAR_HELP_QUESTIONS
        .get(question_index - 1)?
        .question;
    Some(format!("{GHOSTEX_HELP_SKILL_INVOCATION} {question}"))
}

pub(crate) fn titlebar_help_popup_content_height() -> f32 {
    let section_label_height =
        TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT.max(TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT);
    let mut rows = vec![
        TITLEBAR_POPUP_HELP_HEADER_HEIGHT,
        TITLEBAR_POPUP_MENU_ROW_HEIGHT,
        TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
        section_label_height,
    ];
    rows.extend(std::iter::repeat_n(
        TITLEBAR_POPUP_MENU_ROW_HEIGHT,
        GPUI_TITLEBAR_HELP_QUESTIONS.len(),
    ));
    titlebar_popup_menu_height_for_rows(&rows)
}

const TITLEBAR_POPUP_HELP_HEADER_HEIGHT: f32 = 60.0;

/// Stacked header: a tinted icon badge beside the title, with the one-line
/// summary underneath instead of squeezed onto the title row.
fn titlebar_help_popup_header() -> impl IntoElement {
    h_flex()
        .w_full()
        .min_h(px(TITLEBAR_POPUP_HELP_HEADER_HEIGHT))
        .items_center()
        .gap(px(12.0))
        .py(px(6.0))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .size(px(34.0))
                .items_center()
                .justify_center()
                .rounded(px(9.0))
                .bg(titlebar_active_segment_color())
                .child(titlebar_svg_icon(
                    TITLEBAR_ICON_HELP,
                    20.0,
                    titlebar_icon_hover_color(),
                )),
        )
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(titlebar_active_text_color())
                        .child("Ghostex Help"),
                )
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(titlebar_inactive_text_color())
                        .child(GPUI_TITLEBAR_HELP_HEADER_SUMMARY),
                ),
        )
}

pub(crate) fn build_gpui_titlebar_help_popup_menu(
    menu: PopupMenu,
    width: f32,
    max_height: f32,
    scrollable: bool,
) -> PopupMenu {
    let mut menu = titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable);
    // Disabled header row: the same placeholder action the Git and Tips menus
    // use for rows that only carry a label.
    menu = menu.menu_element_with_disabled(Box::new(CopyGpuiTitlebarGitBranch), true, |_, _| {
        titlebar_help_popup_header()
    });
    menu = menu.menu_element(
        Box::new(RunGpuiTitlebarHelpQuestion {
            question_index: GPUI_TITLEBAR_HELP_ASK_ANYTHING_INDEX as u64,
        }),
        |_, _| {
            titlebar_popup_standard_menu_row(
                GPUI_TITLEBAR_HELP_ASK_ANYTHING_ICON,
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                GPUI_TITLEBAR_HELP_ASK_ANYTHING_LABEL.to_string(),
                false,
            )
        },
    );
    menu = titlebar_popup_git_section(menu.separator(), "Sample questions");
    for (question_index, question) in GPUI_TITLEBAR_HELP_QUESTIONS.iter().enumerate() {
        let label = question.label;
        let icon_path = question.icon_path;
        menu = menu.menu_element(
            Box::new(RunGpuiTitlebarHelpQuestion {
                question_index: (question_index + 1) as u64,
            }),
            move |_, _| {
                titlebar_popup_standard_menu_row(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    label.to_string(),
                    false,
                )
            },
        );
    }
    menu
}

impl GhostexGpuiApp {
    /// Toggles the Help popup anchored to the last painted Help button bounds,
    /// for both the button itself and the `openGhostexHelp` hotkey.
    pub(crate) fn show_gpui_titlebar_help_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Help);
        let trigger_bounds = self.titlebar_help_button_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Help,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// A picked Help row: make sure the bundled `ghostex-help` skill is
    /// installed (a local folder copy, run off the UI thread), then ask the
    /// sidebar runtime to start a Quick agent chat whose first message invokes
    /// the skill with the question. The runtime owns quick-workspace creation,
    /// the default prompt agent, and focusing the new session.
    pub(crate) fn run_gpui_titlebar_help_question(
        &mut self,
        question_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(prompt) = gpui_titlebar_help_question_prompt(question_index) else {
            return;
        };
        // Help chats live in one project rooted at the Ghostex config folder
        // (OS-specific, resolved by ghostex_paths), not in a fresh Quick
        // project per question.
        let project_dir = shared_settings::ghostex_storage_paths().config_dir.clone();
        let project_path = project_dir.to_string_lossy().to_string();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (install_result, trust_warnings) = background
                .spawn(async move {
                    let install_result = gpui_install_bundled_ghostex_skill(
                        &["guide", "install-skill"],
                        "Ghostex Help",
                    );
                    let trust_warnings = gpui_trust_folder_for_agent_clis(&project_dir);
                    (install_result, trust_warnings)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(message) = install_result {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Ghostex Help skill not installed",
                        message.as_str(),
                        cx,
                    );
                }
                if !trust_warnings.is_empty() {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not mark the Ghostex folder as trusted",
                        trust_warnings.join(" ").as_str(),
                        cx,
                    );
                }
                let dispatched = this.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "createGhostexHelpChat",
                        "projectPath": project_path,
                        "question": prompt,
                    }),
                    cx,
                );
                if !dispatched {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Ghostex Help unavailable",
                        "The sidebar is not ready to start a chat yet.",
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn render_titlebar_help_button(
        &self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Help);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let button_bounds = self.titlebar_help_button_bounds.clone();
        let trigger_bounds = button_bounds.get();

        div()
            .id("ghostex-gpui-titlebar-button-help")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(icon_color)
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(move |this| {
                if open {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_icon_hover_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_icon_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::Help,
                        "left",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.show_gpui_titlebar_help_menu(window, cx);
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    |window, cx| titlebar_tooltip(TITLEBAR_HELP_TOOLTIP, window, cx),
                )
            })
            .on_prepaint(move |bounds, window, _cx| {
                let previous = button_bounds.get();
                let first_capture = previous.is_none();
                let moved = previous != Some(bounds);
                button_bounds.set(Some(bounds));
                if first_capture || moved {
                    log_gpui_titlebar_popup_anchor(
                        GpuiTitlebarPopupKind::Help,
                        bounds,
                        first_capture,
                        moved,
                        window,
                    );
                    window.request_animation_frame();
                }
            })
            .child(titlebar_svg_icon(TITLEBAR_ICON_HELP, 16.0, icon_color))
            .into_any_element()
    }
}
