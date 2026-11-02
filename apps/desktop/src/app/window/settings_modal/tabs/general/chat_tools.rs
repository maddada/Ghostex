//! Chat and the Tools group of General: Browser, Dev Servers, Editor and
//! File opening.
use super::super::super::catalog::SettingOption;
use super::super::super::catalog::settings_catalog;
use super::super::super::fields::{
    RowSpec, SELECT_WIDTH, SliderBinding, SliderSaver, ignored_ports_field, reset_key,
    segmented_field, setting_row, settings_section, settings_select_with_unavailable,
    slider_number_field_with,
};
use super::super::super::page::PageBlock;
use super::super::super::store::SettingsStore;
use super::super::orchestrator_playbook::{playbook_control, playbook_scope};
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, App, Context, Entity, IntoElement as _, SharedString, Window};
use serde_json::{Map, json};
use std::rc::Rc;

fn chat_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.section_visible("chat") {
        return None;
    }
    let s = "chat";
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(s, "preferredAgentInterface") {
        let spec = RowSpec::new("Default view for compatible agents")
            .description("Chat switches on automatically as soon as Ghostex detects a compatible agent. The terminal stays live in the background, and you can switch back at any time. Settings > Agents can override this for one agent at a time.")
            .modified(g.values.is_modified("preferredAgentInterface"));
        rows.extend(page.segmented(
            g,
            s,
            "preferredAgentInterface",
            spec,
            Some(reset_key::<GeneralTab>("preferredAgentInterface")),
            g.options("PREFERRED_AGENT_INTERFACE_OPTIONS"),
            cx,
        ));
    }
    // CDXC:Sessions 2026-10-09 DECISION: the New-session cleanup is an Advanced setting, off by default, that only runs for Chat; gxserver enforces the Chat condition.
    rows.extend(page.toggle(g, s, "closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs.", false, cx));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q20): Custom transcript width and Transcript
    // width are one slider. It reads Auto (the transcript matches the prompt composer) until it is
    // dragged, which turns the custom width on; its reset goes back to Auto.
    if g.any_visible(
        s,
        &[
            "sessionChatTranscriptWidthPercent",
            "sessionChatCustomTranscriptWidthEnabled",
        ],
    ) {
        let custom = g.values.bool("sessionChatCustomTranscriptWidthEnabled");
        let mut spec = g
            .spec(
                "sessionChatTranscriptWidthPercent",
                "Transcript width (%)",
                "The centered transcript's width on wide panes. Auto matches the prompt composer; drag to set a percentage of the pane instead. The prompt composer keeps its standard width either way.",
            )
            .modified(
                g.values.is_modified("sessionChatTranscriptWidthPercent")
                    || g.values.is_modified("sessionChatCustomTranscriptWidthEnabled"),
            );
        if !custom {
            spec = spec.readout("Auto");
        }
        let reset = Rc::new(
            |page: &mut GeneralTab, _window: &mut Window, cx: &mut Context<GeneralTab>| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.reset_setting("sessionChatTranscriptWidthPercent", cx);
                    store.reset_setting("sessionChatCustomTranscriptWidthEnabled", cx);
                });
            },
        );
        let (min, max) = (
            g.number("MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT"),
            g.number("MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT"),
        );
        let saver: SliderSaver = Rc::new(
            move |store: &Entity<SettingsStore>, value: f64, commit: bool, cx: &mut App| {
                let percent = value.round().clamp(min, max) as i64;
                store.update(cx, |store, cx| {
                    if commit {
                        let mut patch = Map::new();
                        patch.insert(
                            "sessionChatTranscriptWidthPercent".to_string(),
                            json!(percent),
                        );
                        patch.insert(
                            "sessionChatCustomTranscriptWidthEnabled".to_string(),
                            json!(true),
                        );
                        store.apply_patch(patch, "settings:control", cx);
                        return;
                    }
                    if !store
                        .values()
                        .bool("sessionChatCustomTranscriptWidthEnabled")
                    {
                        store.update_setting(
                            "sessionChatCustomTranscriptWidthEnabled",
                            json!(true),
                            cx,
                        );
                    }
                    store.update_setting_debounced(
                        "sessionChatTranscriptWidthPercent",
                        json!(percent),
                        cx,
                    );
                });
            },
        );
        rows.push(slider_number_field_with(
            page,
            &g.p,
            spec,
            Some(reset),
            SliderBinding {
                key: "sessionChatTranscriptWidthPercent",
                min,
                max,
                step: g.number("SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP"),
            },
            g.values.f64("sessionChatTranscriptWidthPercent"),
            saver,
            window,
            cx,
        ));
    }
    rows.extend(page.toggle(g, s, "sessionChatFileEditPreviews", "Show file edit previews", "Show the first seven code lines in each file edit. Turn off to show only the path and change counts.", false, cx));
    rows.extend(page.toggle(g, s, "chatToolCallCards", "Tool call cards (preview)", "Show each run of tool calls as one card: it counts the calls, says what each one is doing and then did, with its result and time, explains a failure, and folds to one line once the agent writes again. Off keeps the current look.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatKeepComposerExpanded", "Keep chat box expanded while scrolling", "Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatConfirmEscapeInterrupt", "Press Escape twice to interrupt", "While the agent is working, the first Escape asks you to press Escape again within 2 seconds before it interrupts. Turn off to interrupt on the first Escape.", false, cx));
    // CDXC:Settings 2026-11-02 SEE-ALSO: the user's words on Default to Detailed view (an Advanced row) are in packages/settings-catalog/src/general/chat.rs.
    rows.extend(page.toggle(g, s, "sessionChatDetailedView", "Default to Detailed view", "New chats open in Detailed view, with the work under each agent reply shown. Off, they open in Normal view. More actions > View in a chat picks Summary, Normal or Detailed for that chat.", false, cx));
    rows.extend(page.toggle(g, s, "chatCatchMeUp", "Catch me up button", "Show a Catch me up button at the top right of the chat box while it is empty. It has your Title Generation Agent (a small, cheap model) read the session's last 30 agent messages and shows where the session stands: what needs you, what is done, in progress or untested.", false, cx));
    rows.extend(page.toggle(g, s, "chatSendOnPick", "Send when you pick an answer", "When a question card asks one question, picking an answer sends it right away, without Send answers. Cards with several questions, multiple-choice questions, Other and text answers still wait for Send.", false, cx));
    rows.extend(page.toggle(g, s, "chatAnswersWaitForIdle", "Wait for turn end before sending form answers", "Send answers from question forms and chat pages only after the agent finishes its turn. Off sends them like a normal chat message, which the agent can read after its current tool call finishes.", false, cx));
    if g.visible(s, "chatPageOpenTarget") {
        let spec = g.spec(
            "chatPageOpenTarget",
            "Open chat pages in",
            "Where a page an agent shows in chat (a mockup, a question form, a comparison) opens when you click its card: a popup over the chat, the Files view, or your default web browser. Answers you send from the page reach the agent the same way in all three. The phone always opens pages in its own viewer.",
        );
        rows.extend(page.segmented(
            g,
            s,
            "chatPageOpenTarget",
            spec,
            Some(reset_key::<GeneralTab>("chatPageOpenTarget")),
            g.options("CHAT_PAGE_OPEN_TARGET_OPTIONS"),
            cx,
        ));
    }
    rows.extend(page.toggle(g, s, "orchestratorAllEfforts", "Every effort level in coordinator pickers", "Also lists the levels above XHigh (Max, Ultra and the like) in the coordinator effort pickers, where the agent has them. Coordinators work best on High or XHigh.", false, cx));
    rows.extend(orchestrator_playbook_row(page, g, s, cx));
    rows.extend(read_aloud_rows(page, g, s, window, cx));
    rows.extend(page.toggle(g, s, "sessionDetailsClaudeTaskTools", "Claude task list", "Turn on Claude's task list tools, so Claude can keep a checklist that the Session details panel shows. Recent Claude models only get them when Ghostex asks. Applies to Claude sessions started or woken after you change it.", false, cx));
    settings_section(&g.p, "Chat", None, None, rows)
        .map(|section| PageBlock::section("chat", section))
}

/// Read aloud's Voice and Speed, or on Linux the note that this computer has no voice. The catalog
/// shows them only while Read aloud is on (its `settingKeys`) and leaves Voice out on Linux
/// (`availability.rs`); titles and subtitles are the catalog's rows.
fn read_aloud_rows(
    page: &mut GeneralTab,
    g: &GeneralCx,
    section: &str,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<AnyElement> {
    let catalog_row = |key: &str| {
        settings_catalog()
            .general_section("chat")
            .and_then(|chat| chat.settings.iter().find(|row| row.key == key))
            .map(|row| (row.title.clone(), row.subtitle.clone(), row.options.clone()))
            .unwrap_or_default()
    };
    let mut rows = Vec::new();
    // `RuntimeOptions::SystemVoices`: System voice, then this computer's voices (the host lists
    // them when Settings opens).
    let (title, subtitle, _) = catalog_row("readAloudVoice");
    let listed = page.store.read(cx).request().read_aloud_voices.clone();
    let voices: Vec<SettingOption> = if listed.is_empty() {
        vec![SettingOption {
            label: "System voice".to_string(),
            value: String::new(),
        }]
    } else {
        listed
            .into_iter()
            .map(|(value, label)| SettingOption { label, value })
            .collect()
    };
    rows.extend(page.select(
        g,
        section,
        "readAloudVoice",
        &title,
        &subtitle,
        voices,
        Some(READ_ALOUD_VOICE_WIDTH),
        false,
        window,
        cx,
    ));
    let (title, subtitle, speeds) = catalog_row("readAloudRate");
    rows.extend(page.number_select(
        g,
        section,
        "readAloudRate",
        &title,
        &subtitle,
        speeds,
        false,
        window,
        cx,
    ));
    if g.visible(section, "readAloudUnavailable") {
        let (title, subtitle, _) = catalog_row("readAloudUnavailable");
        rows.push(setting_row(
            &g.p,
            "readAloudUnavailable",
            RowSpec::new(title).description(subtitle),
            None,
            gpui::Empty.into_any_element(),
            cx,
        ));
    }
    rows
}

/// Voice names carry their language ("Samantha · English (United States)"), so the field is wider
/// than the usual select.
const READ_ALOUD_VOICE_WIDTH: f32 = 280.0;

/// The orchestrator playbook for all projects: its state, the switch that turns an edited copy
/// off, and the Edit button that opens its editor (tabs/orchestrator_playbook.rs). The playbook
/// lives in gxserver, not in the settings file.
fn orchestrator_playbook_row(
    page: &mut GeneralTab,
    g: &GeneralCx,
    section: &str,
    cx: &mut Context<GeneralTab>,
) -> Option<AnyElement> {
    const KEY: &str = "orchestratorPlaybook";
    if !g.visible(section, KEY) {
        return None;
    }
    let scope = playbook_scope(page, None, cx);
    let spec = RowSpec::new("Coordinator playbook").description(
        "How every coordinator works: the playbook it follows. Edit it for all projects here, or for one project and its worktrees in Settings > Projects. Coordinators, existing ones too, follow the edited playbook from their next guide read. The switch turns an edited playbook off without losing it.",
    );
    let rpc_available = page.store.read(cx).request().gxserver_rpc_available;
    let control = playbook_control(
        &g.p,
        "orchestrator-playbook",
        None,
        scope.as_ref(),
        rpc_available,
        cx,
    );
    Some(setting_row(&g.p, KEY, spec, None, control, cx))
}

fn browser_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("browser", g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.select(
        g,
        "browser",
        "webLinkOpenTarget",
        "Open links in",
        "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.",
        g.options("WEB_LINK_OPEN_TARGET_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        "browser",
        "sessionLinkOpenTarget",
        "Open session links in",
        "Open a session's pull requests, issues and Linear tickets (the chips after its title and its links list) in the project Browser view or the system default browser. The phone follows this setting too.",
        g.options("SESSION_LINK_OPEN_TARGET_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        "browser",
        "browserPagesInViewRow",
        "Show pages in their own row",
        "Show the Browser as one tab in the side panel's tab bar, with its pages as a row of tabs under it, instead of each page as a tab beside the views.",
        false,
        cx,
    ));
    settings_section(&g.p, "Browser", None, None, rows)
        .map(|section| PageBlock::section("browser", section))
}

fn dev_servers_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("terminalDevServers", g.power_visible)
    {
        return None;
    }
    let s = "terminalDevServers";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        "terminalDevServerDetectionEnabled",
        "Detect running servers in terminals",
        "Detect localhost dev server URLs from terminal output.",
        false,
        cx,
    ));
    if g.visible(s, "terminalDevServerIgnoredPortRules") {
        let spec = g.spec(
            "terminalDevServerIgnoredPortRules",
            "Ignored ports",
            "Servers on these ports are hidden from the server menu. Enter a port or an inclusive range.",
        );
        let value = g.values.value("terminalDevServerIgnoredPortRules");
        rows.push(ignored_ports_field(
            page,
            &g.p,
            spec,
            Some(reset_key::<GeneralTab>("terminalDevServerIgnoredPortRules")),
            &value,
            window,
            cx,
        ));
    }
    settings_section(
        &g.p,
        "Dev Servers",
        Some(SharedString::from(
            "Choose how Ghostex discovers running dev servers and which ports stay hidden. Detected URLs follow Browser \u{2192} Open links in.",
        )),
        None,
        rows,
    )
    .map(|section| PageBlock::section("terminalDevServers", section))
}

fn editor_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("editor", g.power_visible) {
        return None;
    }
    let s = "editor";
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:CodeEditor 2026-06-08-20:12: these two rows have no reset.
    if g.visible(s, "codeServerLinkVscodeUserConfig") {
        let spec = RowSpec::new("Use VS Code settings")
            .description("Use local VS Code settings instead of the bundled editor defaults.");
        rows.push(super::super::super::fields::toggle_field_with(
            &g.p,
            "codeServerLinkVscodeUserConfig",
            spec,
            g.values.bool("codeServerLinkVscodeUserConfig"),
            None,
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "codeServerLinkVscodeUserConfig", json!(checked), cx)
            },
            cx,
        ));
    }
    if g.values.bool("codeServerLinkVscodeUserConfig")
        && g.visible(s, "codeServerUseVscodeInsidersUserConfig")
    {
        let spec = RowSpec::new("Use VS Code Insiders settings")
            .description("Use the VS Code Insiders user settings directory.")
            .dependent();
        rows.push(super::super::super::fields::toggle_field_with(
            &g.p,
            "codeServerUseVscodeInsidersUserConfig",
            spec,
            g.values.bool("codeServerUseVscodeInsidersUserConfig"),
            None,
            |page: &mut GeneralTab, checked, _window, cx| {
                save(
                    page,
                    "codeServerUseVscodeInsidersUserConfig",
                    json!(checked),
                    cx,
                )
            },
            cx,
        ));
    }
    rows.extend(page.toggle(
        g,
        s,
        "codeServerNewWindowsAsTabs",
        "Open VS Code windows as tabs",
        "Show a window VS Code opens (File > New Window, a folder or workspace opened in a new window) as another window tab above the VS Code page instead of blocking it.",
        false,
        cx,
    ));
    settings_section(&g.p, "Editor", None, None, rows)
        .map(|section| PageBlock::section("editor", section))
}

/// The "As before" choice of HTML, Markdown and images: shown only while an older per-kind choice
/// still decides (`documentFileOpenTarget` unsaved and one of the old keys not at its default).
const DOCUMENT_FILE_OPEN_AS_BEFORE: &str = "asBefore";

/// What the old Markdown, HTML and Images settings still choose, in words, or `None` when they
/// are all at their defaults (everything in Files).
fn document_file_open_as_before(g: &GeneralCx) -> Option<String> {
    use ghostex_settings_catalog::file_opening::{
        DocumentFileKind, legacy_document_file_open_target,
    };
    if g.values.map().contains_key("documentFileOpenTarget") {
        return None;
    }
    let place = |kind| match legacy_document_file_open_target(g.values.map(), kind) {
        "code" => "VS Code",
        "system-app" => "the system app",
        _ => "Files",
    };
    let (markdown, html, images) = (
        place(DocumentFileKind::Markdown),
        place(DocumentFileKind::Html),
        place(DocumentFileKind::Image),
    );
    if [markdown, html, images]
        .iter()
        .all(|place| *place == "Files")
    {
        return None;
    }
    Some(format!(
        "As before: Markdown in {markdown}, HTML in {html}, images in {images}"
    ))
}

fn file_opening_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("fileOpening", g.power_visible) {
        return None;
    }
    let s = "fileOpening";
    // The editors this computer found (`workspaceOpenTargetAvailability`, the Open In page reads
    // the same list).
    let installed: Vec<String> = g
        .values
        .value("workspaceOpenTargetAvailability")
        .get("availableTargetIds")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut targets: Vec<SettingOption> =
        ghostex_settings_catalog::file_opening::file_open_target_options()
            .into_iter()
            .map(|option| SettingOption {
                label: option.label,
                value: option.value,
            })
            .collect();
    let unavailable: Vec<String> = targets
        .iter()
        .map(|option| option.value.clone())
        .filter(|value| {
            !matches!(value.as_str(), "files" | "code" | "system-app") && !installed.contains(value)
        })
        .collect();
    for option in &mut targets {
        if unavailable.contains(&option.value) {
            option.label = format!("{} (not installed)", option.label);
        }
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    for (key, label, subtitle) in [
        (
            "documentFileOpenTarget",
            "HTML, Markdown and images",
            ".html, .md and pictures such as .png, .jpg and .svg, opened from Quick Access, agent chat, the terminal or `ghostex open`. System App opens each in your computer's default app for its type.",
        ),
        (
            "otherFileOpenTarget",
            "Other files",
            "Code and text files, opened from Quick Access, agent chat, the terminal or `ghostex open`. System App opens each in your computer's default app for its type.",
        ),
    ] {
        if !g.visible(s, key) {
            continue;
        }
        let mut options = targets.clone();
        let as_before = (key == "documentFileOpenTarget")
            .then(|| document_file_open_as_before(g))
            .flatten();
        let value = match &as_before {
            Some(label) => {
                options.insert(
                    0,
                    SettingOption {
                        label: label.clone(),
                        value: DOCUMENT_FILE_OPEN_AS_BEFORE.to_string(),
                    },
                );
                DOCUMENT_FILE_OPEN_AS_BEFORE.to_string()
            }
            None => {
                let allowed: Vec<String> =
                    options.iter().map(|option| option.value.clone()).collect();
                g.values.choice(key, &allowed)
            }
        };
        let control = settings_select_with_unavailable(
            page,
            &g.p,
            key,
            &options,
            &unavailable,
            &value,
            Some(SELECT_WIDTH),
            false,
            None,
            move |page: &mut GeneralTab, next, _window, cx| {
                if next != DOCUMENT_FILE_OPEN_AS_BEFORE {
                    save(page, key, json!(next), cx);
                }
            },
            window,
            cx,
        );
        rows.push(setting_row(
            &g.p,
            key,
            RowSpec::new(label).subtitle(subtitle),
            None,
            control,
            cx,
        ));
    }
    let media_options = g.options("MEDIA_FILE_OPEN_TARGET_OPTIONS");
    rows.extend(
        [
            ("videoFileOpenTarget", "Videos", ".webm and .ogv play in Files. Formats Files cannot play, such as .mp4 and .mov, always open in the system app."),
            ("audioFileOpenTarget", "Audio", ".mp3, .wav, .ogg, .flac and .opus play in Files. Formats Files cannot play, such as .m4a, always open in the system app."),
        ]
        .into_iter()
        .filter(|(key, _, _)| g.visible(s, key))
        .map(|(key, label, subtitle)| {
            let allowed: Vec<String> = media_options
                .iter()
                .map(|option| option.value.clone())
                .collect();
            let value = g.values.choice(key, &allowed);
            // `MediaFileOpenTargetSetting`: a subtitle, no reset.
            segmented_field(
                &g.p,
                key,
                RowSpec::new(label).subtitle(subtitle),
                None,
                &media_options,
                Some(&value),
                None,
                move |page: &mut GeneralTab, next, _window, cx| save(page, key, json!(next), cx),
                cx,
            )
        }),
    );
    let changes_options = g.options("FILE_CHANGES_OPEN_TARGET_OPTIONS");
    let key = "fileChangesOpenTarget";
    if g.visible(s, key) {
        let allowed: Vec<String> = changes_options
            .iter()
            .map(|option| option.value.clone())
            .collect();
        let value = g.values.choice(key, &allowed);
        rows.push(segmented_field(
            &g.p,
            key,
            RowSpec::new("Open file changes in").subtitle(
                "Review all on an agent's file changes and Session details' Changes card open here. If this view is turned off, the other one opens.",
            ),
            None,
            &changes_options,
            Some(&value),
            None,
            move |page: &mut GeneralTab, next, _window, cx| save(page, key, json!(next), cx),
            cx,
        ));
    }
    rows.extend(page.toggle(
        g,
        s,
        "docsOpenFilesAsTabs",
        "Show open files as tabs",
        "Show the files open in Files as a row of tabs above the document instead of the Open Files list in the files sidebar.",
        false,
        cx,
    ));
    settings_section(
        &g.p,
        "File opening",
        Some(SharedString::from(
            "Choose where files open from Quick Access, agent chat links and terminal links. An editor app shows here once it is installed on this computer. If a view is unavailable, Ghostex uses the other one.",
        )),
        None,
        rows,
    )
    .map(|section| PageBlock::section("fileOpening", section))
}

/// The Chat page.
pub(super) fn chat_sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        chat_section(page, g, window, cx),
        marked_handovers_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// CDXC:Settings 2026-10-29 DECISION (wording Q25): the five marked-handover rows sat loose in the
/// Chat list; they are their own Marked handovers section (packages/settings-catalog
/// general/chat.rs `marked_handovers`). The switch is an everyday row, the other four advanced.
fn marked_handovers_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("markedHandovers", g.power_visible)
    {
        return None;
    }
    let s = "markedHandovers";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(g, s, "markedHandovers", "Marked handovers in every session", "Coordinators and their threads start each part of the message they hand back with a marker such as ❓ Needs you, ✅ Done, ⚠️ Read this, 🧪 Untested or 🛑 Blocked, so a quick skim shows what needs you. Turn this on to have every other agent session do the same. Applies to sessions started or resumed after you change it.", false, cx));
    rows.extend(page.marked_sections_style_row(g, s, window, cx));
    rows.extend(page.toggle(g, s, "orchestratorPlainReplies", "Coordinators answer without markers", "Coordinators answer normally, without marking the sections of their handovers. Their threads still mark the reports they send to the coordinator. Applies to coordinators started or resumed after you change it.", false, cx));
    settings_section(&g.p, "Marked handovers", None, None, rows)
        .map(|section| PageBlock::section("markedHandovers", section))
}

/// The Files & Links page (the old Tools group: CDXC:Settings 2026-10-29 in model.rs): where
/// files and links open, the editor rows, then dev servers.
pub(super) fn files_links_sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        file_opening_section(page, g, window, cx),
        browser_section(page, g, window, cx),
        editor_section(page, g, cx),
        dev_servers_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}
