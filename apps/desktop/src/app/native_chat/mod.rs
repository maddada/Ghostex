pub(crate) mod appearance;
pub(crate) mod composer;
pub(crate) mod diagnostics;
pub(crate) mod render;
pub(crate) mod session_details;
pub(crate) mod state;
pub(crate) mod transcript;

mod account_switch_card;
mod actions;
mod agent_fleet;
mod agent_tasks;
mod answer_anchor;
mod answer_attachments;
mod api_error_card;
mod approval;
mod ask_card;
mod async_questions;
mod attachment_previews;
mod attachments;
mod background_tasks;
mod binding;
mod box_picker;
mod cards;
mod catch_up;
mod chat_hotkeys;
mod chat_pills;
pub(crate) mod child_window;
mod choice_rows;
mod completed_work_row;
mod composer_animation;
mod composer_host;
mod composer_not_ready;
mod composer_references;
mod composer_scroll;
mod coordinator_board;
mod coordinator_intro;
mod coordinator_threads;
pub(crate) mod cursor;
mod deferred_work;
pub(crate) mod dictation;
mod disclosure_body;
mod draft_project_picker;
mod edit_shortcuts;
mod file_actions;
mod file_change_card;
mod files_links;
mod focus;
pub(crate) mod fonts;
mod fork_branches;
mod form_attachments;
pub(crate) mod frosted_overlay_window;
pub(crate) mod image_viewer;
mod images;
mod inline_answers;
mod inter_agent_message;
mod launch;
mod link_words;
mod menu_toggle;
mod minimap;
mod model_hold_card;
mod nested_scroll;
mod new_session_welcome;
mod note;
mod notice;
mod open_questions;
mod pagination;
mod panel_blocks;
mod question;
mod question_exchange;
mod question_form;
mod questions_list;
pub(crate) mod queue;
mod read_pictures;
mod reference_menu;
mod relayed_approval;
mod resource_modal;
mod rewind;
mod row_click;
mod rpc;
mod run_command;
mod run_location;
mod runtime_worker;
mod scroll_bottom;
mod scrollbar;
mod search;
mod secret_request;
mod side_chats;
mod side_question;
mod startup_delivery;
mod status_rows;
mod subagent_view;
mod system_cards;
mod terminal_ready;
mod thinking;
mod thinking_card;
mod tool_card;
mod tool_run;
mod tray_hotkeys;
mod tray_layout;
mod worktree_tray;

mod code_block;
mod keyboard;
pub(crate) mod links_pill;
mod markdown_links;
mod pills_strip;
mod saved_draft_pill;
// Settings' preview uses more of this look than the transcript does.
#[allow(dead_code)]
pub(crate) mod chat_text_look;
pub(crate) mod markdown_style;
// The preview half of the look is used by Settings' own include of this file.
#[allow(dead_code)]
mod marked_look;
pub(crate) mod marked_sections;
mod maximized;
mod mermaid;
mod mermaid_render;
mod message_actions;
mod page_answers;
mod page_card;
mod permission_card;
mod reply_popover;
mod rich_markdown;
mod row_details;
mod send_control;
mod tone_card;
mod toolbar;
mod transcript_host;
mod transcript_layout;
mod transcript_menu;
mod transcript_reveal;
mod visual;
mod visual_popup;

mod terminal_dialog;
mod terminal_picker;

mod option_menu;

mod option_pills;

pub(crate) mod context_meter;

mod context_editor;

mod suggestions;

mod clipboard;

mod disclosure_anchor;
mod disclosure_motion;
mod workflow_card;
mod working_card;
mod working_spark;

pub(crate) mod zoom;

mod save_markdown;

pub(crate) mod table_preview;
