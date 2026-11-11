// Ghostex Help: the "Ask Ghostex" sample-question popup menu reached from the
// sidebar menu's Help submenu or the Ghostex Help hotkey, and the flow that turns a picked
// question into a Ghostex Help agent session.

use futures::FutureExt as _;
use gpui::FontWeight;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::px;
use gpui_component::menu::PopupMenu;

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// CDXC:Onboarding 2026-09-21 DECISION:
/// User: Ask Ghostex goes back to opening as an overlay "just like before" the 2026-09-20 revamp made it a view tab; that tab page is gone and must not come back.
/// The overlay is a dropdown of seven useful sample questions (for example making Claude control Codex, or matching the terminal width to the chat width); picking one starts a Ghostex Help chat.
/// The first row is "Ask anything about Ghostex", and the sample rows show a short summary label while the full question is what gets staged.
/// Picking a row never sends: the chat opens with the question as an editable draft and the user presses Enter (see `gx_store_create_ghostex_help_chat` in gx_store/create/os_integration.rs, formerly createGhostexHelpChat in the sidebar runtime).
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
    // CDXC:Onboarding 2026-09-15 DECISION:
    // User: replace the Help sample "Make the sidebar narrower" (the leftover of "Sidebar on the right, narrower") with annotating Browser pages and Markdown files, not "notes".
    GpuiTitlebarHelpQuestion {
        icon_path: "titlebar/pencil.svg",
        label: "Annotate Browser pages and Markdown",
        question: "How do I annotate a Browser page or Markdown file?",
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
        // CDXC:ProjectBoard 2026-11-11 DECISION: User (Q20): the sample is "Beads view and starting work", asked about the Beads view.
        icon_path: "titlebar/route.svg",
        label: "Beads view and starting work",
        question: "What does the Beads view do, and how do I start work on a bead?",
    },
];

/// Menu index of the open-ended row; the sample questions follow it.
pub(crate) const GPUI_TITLEBAR_HELP_ASK_ANYTHING_INDEX: usize = 0;

const GPUI_TITLEBAR_HELP_ASK_ANYTHING_LABEL: &str = "Ask anything about Ghostex";
const GPUI_TITLEBAR_HELP_ASK_ANYTHING_ICON: &str = "titlebar/sparkles.svg";
const GPUI_TITLEBAR_HELP_HEADER_SUMMARY: &str = "Explain Ghostex or change a setting for you.";

/// The longest a picked Help row waits for the ghostex-help skill install before its chat opens
/// (`run_gpui_titlebar_help_question`).
const HELP_SKILL_INSTALL_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// The question a Help menu row asks: empty for the open-ended row (the user
/// types their own), otherwise the full sample question. The store puts the
/// ghostex-help skill mention in front of it, spelled for the Help chat's
/// agent (`gx_store_create_ghostex_help_chat`), since only it knows the agent.
pub(crate) fn gpui_titlebar_help_question_prompt(question_index: usize) -> Option<String> {
    if question_index == GPUI_TITLEBAR_HELP_ASK_ANYTHING_INDEX {
        return Some(String::new());
    }
    let question = GPUI_TITLEBAR_HELP_QUESTIONS
        .get(question_index - 1)?
        .question;
    Some(question.to_string())
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
    /// Toggles the Help popup, hung from the sidebar's menu button, for the
    /// `openGhostexHelp` hotkey.
    pub(crate) fn show_gpui_titlebar_help_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Help);
        // Ask Ghostex is a row of the sidebar's menu (Help), so the hotkey hangs its panel where
        // that row does (titlebar/menu_panels.rs).
        let trigger_bounds = self.sidebar_menu_panel_trigger_bounds();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Help,
            open,
            Some(trigger_bounds),
            window,
            cx,
        );
    }

    /// A picked Help row: refresh the bundled `ghostex-help` skill (a local folder copy through the
    /// `ghostex` CLI, run off the UI thread) and ask the Rust store (gx_store/create/os_integration.rs)
    /// to start an agent session in the Ghostex config folder's project whose draft invokes the skill
    /// with the question. The store owns the project lookup, the default prompt agent, and focusing
    /// the new session.
    ///
    /// CDXC:Onboarding 2026-11-07 WHY:
    /// User: "clicking here takes ages to open a thread with /ghostex-help in it". The chat used to
    /// open only once the `ghostex guide install-skill` child process had exited, and that wait was
    /// bounded only by the install's two-minute limit, so a slow or stuck CLI held the click with
    /// nothing on screen. The install now runs beside the open: the chat opens when the install is
    /// done or after `HELP_SKILL_INSTALL_WAIT`, whichever comes first (measured on 2026-11-07: the
    /// install takes 0.1 to 0.4 s, the create, focus and chat draft about 0.1 s more). The skill is
    /// normally already installed from an earlier run, and an install still running when the agent
    /// starts finishes in the background; a failure is still reported with its toast.
    pub(crate) fn run_gpui_titlebar_help_question(
        &mut self,
        question_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(prompt) = gpui_titlebar_help_question_prompt(question_index) else {
            return;
        };
        if self.ask_chief_help_question(&prompt, cx) {
            return;
        }
        self.help_chat_timing_start(question_index);
        // Help chats live in one project rooted at the Ghostex config folder
        // (OS-specific, resolved by ghostex_paths), not in a fresh Quick
        // project per question.
        let project_dir = shared_settings::ghostex_storage_paths().config_dir.clone();
        let project_path = project_dir.to_string_lossy().to_string();
        let background = cx.background_executor().clone();
        let mut install = background
            .spawn(async move {
                gpui_install_bundled_ghostex_skill(&["guide", "install-skill"], "Ghostex Help")
            })
            .fuse();
        let mut wait = background.timer(HELP_SKILL_INSTALL_WAIT).fuse();
        cx.spawn(async move |this, cx| {
            let finished = futures::select! {
                result = install => Some(result),
                _ = wait => None,
            };
            let _ = this.update(cx, |this, cx| {
                this.help_chat_timing_step(
                    match &finished {
                        Some(_) => "skillInstallDone",
                        None => "skillInstallWaitEnded",
                    },
                    serde_json::json!({ "installed": matches!(finished, Some(Ok(_))) }),
                );
                if let Some(Err(message)) = &finished {
                    this.report_help_skill_install_failure(message, cx);
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
            // The open did not wait for a slow install; its failure still gets its toast.
            if finished.is_none() {
                let result = install.await;
                let _ = this.update(cx, |this, cx| {
                    this.help_chat_timing_step(
                        "skillInstallDoneLate",
                        serde_json::json!({ "installed": result.is_ok() }),
                    );
                    if let Err(message) = &result {
                        this.report_help_skill_install_failure(message, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// CDXC:Chief 2026-11-08 DECISION:
    /// User (Q1739): "Ask anything" goes to the head coordinator. While Chief is on and has been opened, a Help row opens Chief's chat instead of a new Help chat, with the sample question staged as an editable draft (the 2026-09-09 rule that a Help row never sends still holds); "Ask anything" opens it with nothing staged. Chief answers from the ghostex-help guide (its playbook says so). With Chief off, or before its first open, the Help chat below is unchanged.
    /// Returns whether Chief took the question.
    fn ask_chief_help_question(&mut self, question: &str, cx: &mut gpui::Context<Self>) -> bool {
        if !crate::app::native_sidebar::chief::chief_on() {
            return false;
        }
        let Some(chief) = self
            .native_sidebar
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.chiefs.iter().find(|chief| chief.is_local).cloned())
        else {
            return false;
        };
        let staged = (!question.trim().is_empty()).then(|| {
            serde_json::json!({
                "projectId": chief.project_id,
                "sessionId": chief.session_id,
                "content": question,
                "clientId": "ghostex-help-menu",
            })
        });
        let sidebar_session_id = chief.sidebar_session_id.clone();
        /*
        CDXC:Chief 2026-11-10 DECISION:
        User (a Discord report): with Chief already selected the Ask Ghostex row seemed to do nothing, so it looked broken. The row now says that Chief of Staff took the question in a notice, and puts the keyboard in Chief's chat box.
        */
        let (title, description) = if staged.is_some() {
            (
                "Ask Chief of Staff",
                "It answers Ghostex questions here. Your question is in its chat box: press Enter to send it.",
            )
        } else {
            (
                "Ask Chief of Staff",
                "It answers Ghostex questions here. Type your question in its chat box.",
            )
        };
        self.dispatch_gpui_app_modal_toast("info", title, description, cx);
        cx.spawn(async move |this, cx| {
            if let Some(params) = staged {
                let _ = crate::app::gx_store::gx_rpc(None, "/api/setSessionChatDraft", params).await;
            }
            let _ = this.update(cx, |this, cx| {
                this.dispatch_native_sidebar_ui(
                    serde_json::json!({ "type": "selectSession", "sessionId": sidebar_session_id, "mode": "focus" }),
                    cx,
                );
                let _ = this.react_to_native_sidebar_session_click(&sidebar_session_id, cx);
                // With Chief already selected the click moves nothing, so ask for the keyboard
                // explicitly; the handoff waits for the chat box to be on screen.
                this.request_keyboard_handoff_for_shell_focus(cx);
            });
        })
        .detach();
        true
    }

    fn report_help_skill_install_failure(&mut self, message: &str, cx: &mut gpui::Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Ghostex Help skill not installed",
            message,
            cx,
        );
    }
}
