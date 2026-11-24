use crate::json::J;

pub const DEFAULT_GHOSTEX_SETTINGS: &[(&str, J)] = &[
    // CDXC:Notifications 2026-05-29-12:00:
    // Action-completion feedback should use the plain shamisen sound by default;
    // shamisen reverb remains available from Settings for users who prefer it.
    ("actionCompletionSound", J::Str("shamisen")),
    ("gpuiTitlebarActionCommandByProject", J::Obj(&[])),
    ("gpuiTitlebarOpenTargetByProject", J::Obj(&[])),
    // CDXC:GhostexCapture 2026-09-30 WHY:
    // The floating button sits over every app, so it is opt-in: a new install or an update must not
    // put a window on top of someone's screen they never asked for.
    ("ghostexCaptureEnabled", J::Bool(false)),
    // CDXC:GhostexCapture 2026-09-30 DECISION:
    // User: "when we send a prompt to the app we have to switch the app to that session", with a
    // toggle under Floating Capture to turn it off. On by default; Ghostex shows the session without
    // coming in front of the app the user is in.
    ("ghostexCaptureSwitchToSession", J::Bool(true)),
    ("agentboxDefaultLocation", J::Str("local")),
    // CDXC:AgentProviders 2026-09-04 DECISION:
    // User: new installs must start with Agent approvals set to Keep default. Running supported agents without approval is an explicit opt-in.
    ("agentAcceptAllEnabled", J::Bool(false)),
    // Per-agent session mode map (`{ "<family>": "non-cli" | "agent-cli" }`). Absent defaults to
    // `"agent-cli"` (interactive terminal); `"non-cli"` runs supported agents through a structured
    // host inside `zmx`.
    ("agentSessionMode", J::Obj(&[])),
    // Per-agent explicit default model map (`{ "<family>": "<modelValue>" }`). Overrides the last
    // model picked in chat for new sessions and threads of that agent.
    ("agentDefaultModels", J::Obj(&[])),
    // Per-agent dismissals of the chat strip's "Switch to Native" offer (Q4770), comma-separated
    // canonical families (e.g. `"claude,codex"`).
    ("nativeSwitchOfferDismissedAgents", J::Str("")),
    // CDXC:AgentProviders 2026-11-14 DECISION: off by default, and on only with its confirmation
    // beside it (folder_trust.rs).
    ("trustProjectFoldersAutomatically", J::Bool(false)),
    ("trustProjectFoldersConfirmedAt", J::Str("")),
    ("agentHooksAutoInstall", J::Bool(false)),
    ("useAgentHooks", J::Bool(false)),
    // CDXC:Updates 2026-11-19 SEE-ALSO: Settings > Updates > Checking for updates, every hour by default (update_checks.rs, where the user's decision is).
    ("updatesCheckGhostex", J::Str("1h")),
    ("updatesCheckAgents", J::Str("1h")),
    ("updatesCheckTools", J::Str("1h")),
    ("updatesCheckExtensions", J::Str("1h")),
    ("agentsTidyUpOfferDismissed", J::Bool(false)),
    ("agentManagerZoomPercent", J::Num(100.0)),
    ("defaultPromptAgentId", J::Str("claude")),
    ("sessionTitleGenerationAgent", J::Str("claude")),
    ("customSessionTitleGenerationCommand", J::Str("")),
    // CDXC:Navigation 2026-07-02-13:05:
    // In-app link routing is the default so cmd-clicked web links land in the
    // project Browser view unless the user opts back into the system browser in
    // Settings.
    //
    // CDXC:SessionChat 2026-08-18:
    // Session chat web links share this default for the same reason.
    //
    // CDXC:Navigation 2026-08-19:
    // Detected dev-server rows now share it as well, so a fresh install answers
    // every web link the same way instead of splitting chat and terminal links
    // from dev-server links.
    ("webLinkOpenTarget", J::Str("internal-browser")),
    // CDXC:SessionLinks 2026-10-24 DECISION: User: "user can pick to open in embedded or external browser in settings (default to embedded)".
    ("sessionLinkOpenTarget", J::Str("embedded")),
    ("browserPagesInViewRow", J::Bool(false)),
    // CDXC:Browser 2026-11-16 WHY: reloading the open project's Browser tabs on launch came with the file:// tab restore (PR #624); it is an Advanced toggle, off by default, because web panes sleep until clicked so launch never loads a page nobody asked for (model/project_editor_shell.rs).
    ("reloadBrowserTabsOnStartup", J::Bool(false)),
    // CDXC:Extensions 2026-11-09 DECISION: User: "that url is mine but it should open grok.com usually please". The Grok page opens grok.com unless the user puts their own page (a Grok bot, say) here; no one's own page ships as a default.
    ("grokWebStartPage", J::Str("https://grok.com/")),
    // The Grok logins, each `{ "id", "name" }` with its own browser profile (apps/desktop/src/app/claude_web/grok.rs). Empty is one login named Grok.
    ("grokWebLogins", J::Arr(&[])),
    // The sidebar ⋯ menu's links, each `{ "id", "name", "url" }` (packages/gx-core/src/custom_links.rs). In no Settings > Sync category: a localhost link points at the computer it was added on.
    ("customLinks", J::Arr(&[])),
    // CDXC:SessionLinks 2026-10-27 SEE-ALSO: `SessionLinksDisplay` in packages/gx-core/src/sidebar_view/links.rs (what each value shows). The default is Badge (2026-10-27: "and also please make the sidebar show as badge by default"), which replaces the 2026-10-24 Always default.
    ("sidebarSessionLinks", J::Str("badge")),
    // CDXC:SessionLinks 2026-10-24 SEE-ALSO: native_sidebar/links_list.rs; the links list's folded groups, kept app-wide (Merged or closed and Removed folded until opened).
    (
        "sessionLinksFoldedGroups",
        J::Arr(&[J::Str("finished"), J::Str("removed")]),
    ),
    // CDXC:WorkMode 2026-10-24 DECISION: User: Issues Workflow has "No switch; Linear parts appear once you add a Linear key, GitHub parts once gh is signed in. Branch titles and the Clean up offer become their own settings." Both are global, one value for every project. Asked to keep branch titles off (most users never saw them, and turning them on renames every session on a branch) and the Clean up offer on (it changes nothing until answered), the user answered "Yes, keep those defaults".
    // SEE-ALSO: server/src/work_mode/settings.rs (gxserver reads both), server/src/work_mode/presentation.rs (`work_display_title`, `offerCleanup`).
    ("titleSessionsByBranch", J::Bool(false)),
    ("offerCleanupWhenPrMerges", J::Bool(true)),
    // CDXC:WorkMode 2026-11-20 SEE-ALSO: server/src/server/work_cleanup_threads.rs (the user decision, Q4286, and the rules a thread must pass); off by default, as the page the user picked from said ("a setting, Ask by default"): the banner asks until it is turned on.
    ("cleanUpMergedThreads", J::Bool(false)),
    // CDXC:Navigation 2026-10-17 SEE-ALSO: these replace markdownFileOpenView, htmlFileOpenView and imageFileOpenTarget, which still decide until documentFileOpenTarget is saved (file_opening.rs).
    ("documentFileOpenTarget", J::Str("files")),
    ("fileChangesOpenTarget", J::Str("changes")),
    ("changesHideTestFiles", J::Bool(false)),
    ("beadsViewLayout", J::Str("board")),
    ("otherFileOpenTarget", J::Str("files")),
    ("videoFileOpenTarget", J::Str("files")),
    ("audioFileOpenTarget", J::Str("files")),
    ("docsOpenFilesAsTabs", J::Bool(true)),
    // CDXC:Settings 2026-06-29-17:54:
    // New installs start at General Settings. Once the user closes Settings, the
    // modal saves only navigation chrome state here, not search text or private
    // setting values beyond the already persisted preferences.
    //
    // CDXC:Settings 2026-06-30-04:47:
    // Navigation writes can happen before close so native-window teardown does not
    // lose the last selected Settings page.
    (
        "settingsModalNavigation",
        J::Obj(&[
            ("activeTab", J::Str("settings")),
            ("extensionsGroupOpen", J::Bool(false)),
            ("scrollTopByTab", J::Obj(&[])),
            ("version", J::Num(1.0)),
        ]),
    ),
    // CDXC:Settings 2026-06-28-07:41:
    // New installs and missing persisted settings should keep experimental
    // surfaces hidden until the user enables Enable Experimental Features from
    // Advanced Settings.
    //
    // CDXC:Automations 2026-09-17:
    ("fasterRendering", J::Bool(false)),
    ("chatToolCallLines", J::Bool(false)),
    ("securityLogsEnabled", J::Bool(false)),
    ("approvalsRequirePresence", J::Bool(false)),
    // CDXC:Extensions 2026-11-24 WHY:
    // PostHog audit (Q6 slim-and-discover): VS Code (`code`), Beads (`kanban`) and Automate
    // (`automate`) are the top actively disabled built-ins, so brand-new installs start with them
    // opt-in (hidden by default) while Settings > Extensions can turn any of them on. Existing
    // users with a completed setup keep them enabled through `keep_old_heavy_views_once`
    // (apps/desktop/src/shared_settings/heavy_views_default.rs).
    ("codeViewTabHidden", J::Bool(true)),
    ("browserViewTabHidden", J::Bool(false)),
    ("kanbanViewTabHidden", J::Bool(true)),
    ("automateViewTabHidden", J::Bool(true)),
    ("docsViewTabHidden", J::Bool(false)),
    ("changesViewTabHidden", J::Bool(false)),
    ("terminalViewTabHidden", J::Bool(false)),
    ("storybookViewTabHidden", J::Bool(false)),
    ("linearViewTabHidden", J::Bool(false)),
    ("jiraViewTabHidden", J::Bool(false)),
    ("githubViewTabHidden", J::Bool(false)),
    ("sentryViewTabHidden", J::Bool(true)),
    ("figmaViewTabHidden", J::Bool(true)),
    ("vercelViewTabHidden", J::Bool(true)),
    ("supabaseViewTabHidden", J::Bool(true)),
    ("githubActionsViewTabHidden", J::Bool(true)),
    ("posthogViewTabHidden", J::Bool(true)),
    ("customWebsiteViewTabHidden", J::Bool(true)),
    ("projectWebsiteViews", J::Obj(&[])),
    // CDXC:Bots 2026-09-26 DECISION:
    // User: Bots is off by default, so Ghostex looks exactly as it did until the user turns it on.
    ("botsHidden", J::Bool(true)),
    // CDXC:Bots 2026-09-26 DECISION:
    // User: Bot automations is its own switch, off by default, so Bots can be on without the feed.
    ("botAutomationsHidden", J::Bool(true)),
    // CDXC:GBots 2026-11-08 DECISION: GBots is off by default (official_extensions.rs).
    ("gbotsEnabled", J::Bool(false)),
    // CDXC:GBots 2026-11-09 DECISION: User (Q2103 A): Chief of Staff adds GBots on its own, on by default; off, each add asks you first (server/src/server/security_gates.rs).
    ("gbotsChiefAddsOnItsOwn", J::Bool(true)),
    ("notificationsTitlebarButtonHidden", J::Bool(false)),
    ("helpTitlebarButtonHidden", J::Bool(false)),
    ("resourcesTitlebarButtonHidden", J::Bool(false)),
    ("devServersTitlebarButtonHidden", J::Bool(false)),
    ("extensionsTitlebarButtonHidden", J::Bool(false)),
    ("gitActionsTitlebarButtonHidden", J::Bool(false)),
    // CDXC:Extensions 2026-10-01 DECISION: Actions is off by default (built_in_extensions.rs).
    ("actionsHidden", J::Bool(true)),
    // CDXC:CloudBoxes 2026-11-02 DECISION: Cloud Boxes is on by default everywhere (D11, official_extensions.rs); `cloudBoxesEnabled` replaces `cloudBoxesHidden`, whose saved `true` default would have kept it off.
    ("cloudBoxesEnabled", J::Bool(true)),
    // Cloud Boxes (packages/gx-protocol/src/cloud_box/settings.rs `setting_keys`): 0 = no monthly cap.
    ("cloudBoxesMonthlyCapUsd", J::Num(0.0)),
    ("cloudBoxesIdleStopMinutes", J::Num(15.0)),
    ("cloudBoxesDeleteStoppedAfterDays", J::Num(7.0)),
    ("cloudBoxesPhoneStartEnabled", J::Bool(false)),
    // CDXC:Workspaces 2026-10-09 DECISION: Workspaces is off by default (official_extensions.rs).
    ("workspacesHidden", J::Bool(true)),
    // CDXC:VoiceMode 2026-10-21 DECISION: Read aloud is off by default (official_extensions.rs).
    ("readAloudHidden", J::Bool(true)),
    // CDXC:SessionDetails 2026-10-25 DECISION: the Session details panel is a beta built-in extension, on by default (official_extensions.rs).
    ("sessionDetailsPanelHidden", J::Bool(false)),
    // CDXC:SessionNotes 2026-10-30 DECISION: Session notes is off by default (official_extensions.rs).
    ("sessionNotesHidden", J::Bool(true)),
    // CDXC:SessionChat 2026-11-22 DECISION: the Chat Status Line is off by default, for existing users too (official_extensions.rs).
    ("chatStatusLineEnabled", J::Bool(false)),
    // CDXC:GhostexApps 2026-11-25 DECISION: Ghostex Apps is off by default, a Beta built-in extension (official_extensions.rs).
    ("ghostexAppsEnabled", J::Bool(false)),
    // CDXC:Extensions 2026-10-25 DECISION: Claude is off by default (official_extensions.rs).
    ("claudeWebHidden", J::Bool(true)),
    // CDXC:Extensions 2026-11-09 DECISION: Grok is off by default, like Claude (official_extensions.rs).
    ("grokWebHidden", J::Bool(true)),
    // CDXC:GrokBot 2026-11-19 DECISION: Grok Bot Integration is a built-in extension, on by default (official_extensions.rs).
    ("grokBotHidden", J::Bool(false)),
    // CDXC:GrokBot 2026-11-26 DECISION: Grok Bot gets asks and blockers by default, never done reports (server/src/grokbot/filter.rs).
    ("grokBotSends", J::Str("asksAndBlockers")),
    // CDXC:AgentProviders 2026-11-18 DECISION: CLIProxyAPI is off by default (official_extensions.rs).
    ("cliProxyApiEnabled", J::Bool(false)),
    // CDXC:Extensions 2026-10-19 DECISION: Connectors, OS Integration and Debugging are on by default; their old gates still apply (official_extensions.rs).
    // Off by default since Experimental features became one switch each (2026-11-09).
    ("connectorsHidden", J::Bool(true)),
    ("osIntegrationHidden", J::Bool(true)),
    ("keepAwakeEnabled", J::Bool(false)),
    ("allAutomationsEnabled", J::Bool(false)),
    ("debuggingHidden", J::Bool(false)),
    // CDXC:VoiceMode 2026-10-25 DECISION: the voice agent is a built-in extension, on by default (official_extensions.rs).
    ("voiceAgentHidden", J::Bool(false)),
    // CDXC:Chief 2026-11-24 DECISION: User: "disable turning off main hub please make it always on thingy ppl can just keep it collapsed". Main Hub is always on and cannot be turned off; `mainHubHidden` is retired and ignored (supersedes the 2026-11-08 and 2026-11-21 Main Hub switch decisions).
    // CDXC:Chief 2026-11-08 DECISION: User (Q1910): New Chat asks first, with a Don't ask again box that turns this off.
    ("chiefConfirmNewChat", J::Bool(true)),
    // CDXC:Chief 2026-11-08 DECISION: User: the home row "should remain top of the space that it's a part of ... not show on top of all spaces like we have now (make this a setting, default off, 'Show on all spaces')".
    ("chiefShowOnAllSpaces", J::Bool(false)),
    // CDXC:Chief 2026-11-08 DECISION: User (Q1915): agents Ghostex starts in the home folder are kept out of secret folders (server/src/agents/home_secret_folders.rs). Paths start with ~/ or are absolute; a missing one costs nothing.
    (
        "homeSecretFolders",
        J::Arr(&[
            J::Str("~/.ssh"),
            J::Str("~/.aws"),
            J::Str("~/.gnupg"),
            J::Str("~/.config/gh"),
            J::Str("~/.netrc"),
            J::Str("~/.git-credentials"),
            J::Str("~/.password-store"),
            J::Str("~/.config/gcloud"),
            J::Str("~/.azure"),
            J::Str("~/.codex/auth.json"),
            J::Str("~/.claude/.credentials.json"),
            J::Str("~/.claude-profiles/ghostex-*/.ghostex-hub-key"),
            J::Str("~/.codex-profiles/ghostex-*/config.toml"),
            J::Str("~/Library/Keychains"),
            J::Str("~/Library/Cookies"),
            J::Str("~/Library/Safari"),
            J::Str("~/Library/Application Support/Google/Chrome"),
            J::Str("~/Library/Application Support/Firefox"),
            J::Str("~/Library/Application Support/BraveSoftware"),
            J::Str("~/Library/Application Support/Arc"),
            J::Str("~/.mozilla"),
            J::Str("~/.config/google-chrome"),
            J::Str("~/.config/chromium"),
            J::Str("~/.local/share/keyrings"),
            J::Str("~/AppData/Local/Google/Chrome/User Data"),
            J::Str("~/AppData/Roaming/Mozilla/Firefox"),
            J::Str("~/AppData/Roaming/Microsoft/Credentials"),
        ]),
    ),
    ("openInTitlebarButtonHidden", J::Bool(false)),
    // CDXC:CodeEditor 2026-05-06-15:00
    // Embedded code-server editor panes can reuse the user's local VS Code
    // user settings. A separate Insiders toggle switches the linked source
    // directory without disabling the shared project editor runtime.
    //
    // CDXC:CodeEditor 2026-06-08-20:12:
    // New installs should use Ghostex-owned bundled editor settings by default
    // so the embedded VS Code surface starts on Dark 2026. Users can still opt
    // into local VS Code settings explicitly from Settings.
    ("codeServerLinkVscodeUserConfig", J::Bool(true)),
    ("codeServerUseVscodeInsidersUserConfig", J::Bool(false)),
    ("codeServerNewWindowsAsTabs", J::Bool(false)),
    // Legacy external-IDE preference keys remain normalized so existing settings
    // files and generic Open in IDE actions stay readable. They are no longer
    // exposed in Settings or used by Agents Hub, which opens files in Source.
    ("customDefaultEditorCommand", J::Str("")),
    // CDXC:Icons 2026-06-25-21:50: New installs use the default bundled app icon (empty source id).
    ("appIconSourceId", J::Str("")),
    ("defaultEditorCommand", J::Str("code")),
    // CDXC:Git 2026-05-16-08:46:
    // Users can hide the project-header +added/-removed git summary completely
    // when they want project names to stay visually quiet. This is independent
    // from the existing changed-file count preference.
    //
    // CDXC:Settings 2026-06-13-01:06:
    // Recommended is the default sidebar preset, so new settings show project-header
    // git stats while keeping the changed-file count off unless the user enables it.
    ("hideProjectHeaderDiffStats", J::Bool(false)),
    // CDXC:Docs 2026-06-30-19:47:
    // Additional Docs scan folders remain opt-in beyond the built-in ./docs,
    // ./artifacts, ./ai, and ./tmp roots (and the same folder names one level
    // down) plus root Markdown, HTML, and Excalidraw files.
    // A configured Docs directory adds its own tree on top of whatever this
    // lists (CDXC:Docs).
    ("manageAdditionalDocsFolders", J::Str("")),
    // CDXC:Projects 2026-08-02:
    // New installs ship every Global Default empty so project resolution stays
    // byte-for-byte identical to the pre-feature behavior until a user fills one in.
    ("globalWorktreeCommand", J::Str("")),
    ("globalBeadsDisplayKey", J::Str("")),
    ("globalBeadsDirectory", J::Str("")),
    ("globalDocsDirectory", J::Str("")),
    // Empty keeps Add Project opening at the home folder, as before the setting existed.
    ("defaultProjectsFolder", J::Str("")),
    // CDXC:Git 2026-05-15-14:33:
    // Project-header git stats should hide the changed-file count by default and
    // show only added/removed line counts. Users can opt back into the file
    // number from Settings when they want the full diff summary.
    ("showProjectEditorDiffFileCount", J::Bool(false)),
    // CDXC:Git 2026-05-27-09:25:
    // Match Starship-style tracked line counts by default. Users can opt in to
    // show untracked line totals only when tracked `git diff --numstat HEAD` is
    // +0 -0.
    (
        "showUntrackedProjectDiffWhenNoTrackedChanges",
        J::Bool(false),
    ),
    ("completionSound", J::Str("two-tone-1")),
    // CDXC:Clipboard 2026-09-15 DECISION:
    // User: play the copy sound everywhere in the app when copying, but "make it disabled by default for now". This supersedes the earlier enabled default; this switch is the only gate the copy sound has.
    ("copySound", J::Bool(false)),
    // CDXC:Notifications 2026-07-01-01:13:
    // Plain terminal BEL events include ordinary shell feedback such as zsh
    // completion misses. Keep terminal-bell attention notifications opt-in so
    // Monaco prompt editing and agent completion alerts remain independent from
    // noisy terminal-emulator bells.
    ("showNotificationOnTerminalBell", J::Bool(false)),
    ("createSessionOnSidebarDoubleClick", J::Bool(false)),
    ("sidebarSessionCycleSkipsSleeping", J::Bool(false)),
    // CDXC:Sessions 2026-09-10 DECISION:
    // User: enable parking by default, but keep sleep on park disabled by default.
    ("enableSessionParking", J::Bool(true)),
    ("sleepSessionWhenParking", J::Bool(true)),
    // CDXC:Sessions 2026-09-12 DECISION:
    // User: "Park & Snooze with tags" is on by default, so Park and Snooze open the tag menu unless the user turns it off. Supersedes the 2026-09-11 off-by-default choice for the park-only version of this setting.
    ("showTagMenuWhenParking", J::Bool(false)),
    // CDXC:Sessions 2026-09-11 DECISION:
    // User: "Unpark after sending a message" defaults to on.
    ("unparkAfterSendingMessage", J::Bool(true)),
    // CDXC:Telemetry 2026-11-24:
    // Usage analytics are on by default and opt-out (held during first-run
    // onboarding until it finishes or is skipped). Events carry only counts and
    // fixed-list values tied to a random per-install UUIDv4. Nothing personal:
    // no prompts, no paths, no project names, and no third-party account ids.
    ("analyticsEnabled", J::Bool(true)),
    ("debuggingMode", J::Bool(false)),
    (
        "diagnosticLogging",
        J::Obj(&[("scenarios", J::Obj(&[])), ("version", J::Num(1.0))]),
    ),
    ("renameSessionOnDoubleClick", J::Bool(false)),
    ("showProjectIcons", J::Bool(true)),
    // CDXC:Sessions 2026-05-16-08:46:
    // Agent identity remains configurable in Settings through an explicit
    // hover-only mode for quieter session lists.
    //
    // CDXC:Settings 2026-06-13-01:06:
    // Superseded by CDXC:Settings 2026-06-30-22:29.
    //
    // CDXC:Settings 2026-06-30-22:29:
    // Recommended is the first-run preset and keeps session agent icons visible
    // while showing detailed sidebar status chrome.
    ("hideSessionAgentIconUntilHover", J::Bool(false)),
    // CDXC:Browser 2026-05-28-07:38:
    // Browser page favicons are page identity, not agent chrome. Keep them
    // visible in the default Codex and Detailed presets even when agent icons are
    // hover-only, while Minimal can hide favicons until hover for a quieter list.
    ("hideBrowserFaviconUntilHover", J::Bool(false)),
    // CDXC:Sessions 2026-05-09-17:00
    // Session-card close controls should be available out of the box. Users can
    // still turn the hover chrome off from Settings when they want quieter cards.
    (
        "sessionCardHoverButtons",
        super::session_card_hover_actions::DEFAULT_SESSION_CARD_HOVER_BUTTONS,
    ),
    // CDXC:Sessions 2026-10-01 DECISION:
    // User: the enabled hover buttons also appear in the session context menu by default, in the menu's own ChatGPT-style order (Rename, Pin, Snooze, Park, Sleep, Note, Tag As); this supersedes the 2026-09-15 rule that they led the menu in the card's right-to-left order. Close is the exception and never appears in the menu while it is on the card. Turning this off restores the 2026-09-12 rule where every enabled hover button leaves the context menu.
    ("showSessionCardHoverButtonsInContextMenu", J::Bool(true)),
    // CDXC:Tooltips 2026-10-29 DECISION: User: "please let me disable the tooltip for sessions in the sidebar with a toggle (advanced setting)". Session tooltips stay on by default, as before; turning this off removes only the hover tooltip on a sidebar session row's title.
    ("showSidebarSessionTooltips", J::Bool(true)),
    // CDXC:Coordinators 2026-11-15 DECISION (issue #618, Q3275): "no bold" (the user dislikes bold text). A coordinator and its threads stand apart by the crown, colour and dimmer thread text; Bold coordinator title (off by default) bolds the coordinator's title for people who want it. Same key as the phone's own setting (apps/mobile/app/src/settings/store.ts).
    ("sessionsBoldCoordinatorTitle", J::Bool(false)),
    // CDXC:Sessions 2026-06-13-15:42
    // Recommended is the default sidebar style and hides session-card Last Active
    // timestamps by default. Settings still owns an explicit toggle for users who
    // want the timestamp back, and the setting must not affect project-header git
    // diff stats.
    ("hideLastActiveTimeOnSessionCards", J::Bool(false)),
    ("highlightPendingQuestions", J::Bool(false)),
    ("hideAccountEmails", J::Bool(false)),
    // CDXC:ScreenshotMode 2026-11-20 DECISION in apps/desktop/src/app/screenshot_mode.rs: off by
    // default, and in no Settings > Sync category, so it stays on this computer.
    ("screenshotMode", J::Bool(false)),
    // CDXC:AgentProviders 2026-10-06 DECISION:
    // User: Claude and Codex each get a setting to auto-redeem banked resets that are going to expire anyway, used 5 minutes before expiry ("no no shouldn't be 60 minutes before that's big waste. Let's do 5 minutes instead."); using one at a usage limit in its last 24 hours happens only "if user enables that toggle", the separate AtLimit switch. All off by default because a redeemed reset cannot be given back; gxserver owns the rule (server/src/accounts/reset_watch.rs). Supersedes the 2026-10-05 single switch that also covered the limit case, and the 60-minute last call.
    // CDXC:AgentProviders 2026-11-13 DECISION:
    // User: "we also need a new toggle that continues on any open Opus when Fable runs out on all accounts, in Settings for Claude accounts, since that's the most common case." On by default for that reason; gxserver owns the rule (server/src/accounts/model_fallback.rs).
    ("claudeFableFallbackToOpus", J::Bool(true)),
    ("claudeAutoRedeemExpiringResets", J::Bool(false)),
    ("codexAutoRedeemExpiringResets", J::Bool(false)),
    ("claudeAutoRedeemResetsAtLimit", J::Bool(false)),
    ("codexAutoRedeemResetsAtLimit", J::Bool(false)),
    // CDXC:Sessions 2026-06-13-17:50:
    // First-run sidebar tag filter settings should show the default triage tags,
    // the No tag filter, and the default separators. Users opt out by hiding or
    // disabling individual rows from the collapsed Sidebar Tags settings area.
    // CDXC:Sidebar 2026-11-14 WHY: the value is `DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS` itself (data/session_tags.rs), not a second copy, so the six status entries it starts with and the Settings editor's Reset to Default can never disagree with this default.
    (
        "sidebarSessionTagListItems",
        super::session_tags::DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
    ),
    // CDXC:SessionSleep 2026-09-13 DECISION:
    // User: keep Agent Auto Sleep off by default because remote Claude requires its terminal to remain running.
    //
    // CDXC:SessionSleep 2026-05-28-08:06:
    // Background VS Code, Project, and Git panes originally auto-slept after
    // fifteen minutes of idle time by default. Agent terminal auto-sleep starts
    // opt-in because it closes live user-created conversation surfaces.
    //
    // CDXC:SessionSleep 2026-06-15-18:31:
    // Heavy editor, Project, Git/Browser, and browser-session surfaces should
    // retire quickly by default because many awake webviews and code-server
    // processes make sidebar switching laggy. Use ten-minute idle windows for
    // browser and VS Code panes, retain five minutes for Git and Project panes,
    // and enable browser-session Auto Sleep while keeping agent terminals opt-in.
    //
    // CDXC:SessionSleep 2026-06-07-00:53:
    // Agent auto-sleep keeps its opt-in policy, but the default idle threshold is
    // now fifteen minutes so enabled agent sessions retire on the same window as
    // editor surfaces.
    //
    // CDXC:SessionSleep 2026-06-07-00:56:
    // Focused agent sessions must never auto-sleep and no longer have a Settings
    // override because sleeping the active conversation is not a supported UX.
    ("autoSleepAgentIdleMinutes", J::Num(0.0)),
    ("autoSleepBrowserIdleMinutes", J::Num(10.0)),
    ("autoSleepCodeEditorIdleMinutes", J::Num(10.0)),
    ("autoSleepGitEditorIdleMinutes", J::Num(5.0)),
    ("autoSleepProjectEditorIdleMinutes", J::Num(5.0)),
    ("keepViewsLoaded", J::Bool(false)),
    ("autoSleepRequireAgentResumeCommand", J::Bool(true)),
    ("autoSleepFavoriteAgentSessions", J::Bool(false)),
    ("startServerAtLogin", J::Bool(false)),
    ("openAppAtLogin", J::Bool(false)),
    ("resumeSessionsAfterRestart", J::Bool(false)),
    ("memoryGuardEnabled", J::Bool(true)),
    ("memoryGuardNotify", J::Bool(true)),
    ("keepAwakeActivateOnExternalDisplay", J::Bool(false)),
    ("keepAwakeActivateOnLaunch", J::Bool(false)),
    ("keepAwakeAllowDisplaySleep", J::Bool(false)),
    ("keepAwakeBatteryThresholdPercent", J::Num(0.0)),
    ("keepAwakeDeactivateOnLowPowerMode", J::Bool(false)),
    ("keepAwakeDeactivateOnUserSwitch", J::Bool(false)),
    ("keepAwakeDefaultDurationMinutes", J::Num(0.0)),
    ("keepAwakeWhileWorkingSessions", J::Bool(false)),
    // CDXC:KeepAwake 2026-05-28-19:28:
    // Closing a MacBook lid is not covered by the standard caffeinate idle-sleep assertion.
    // Keep lid-close sleep prevention as an explicit opt-in because it changes the system-wide `pmset disablesleep` policy with administrator approval.
    ("keepAwakePreventLidSleep", J::Bool(false)),
    // CDXC:KeepAwake 2026-05-27-07:32:
    // The titlebar keep-awake affordance is optional chrome. Keep the per-control
    // hide preference off by default, but persist a Power setting that can remove
    // the titlebar control completely for users who do not use Mac sleep
    // management from Ghostex.
    //
    // CDXC:Settings 2026-06-28-07:41:
    // Keep Awake is an experimental macOS feature. Its own switch (`keepAwakeEnabled`, Settings >
    // Extensions > Experimental, since 2026-11-09) must be on before the titlebar button or
    // runtime automation is available; this preference only hides the button again while it is on.
    ("hideKeepAwakeTitlebarControl", J::Bool(false)),
    // CDXC:AgentLauncher 2026-08-01:
    // Every built-in tab strip button stays visible until the user hides it, so
    // adding Global Actions never silently removes a control someone relies on.
    // CDXC:Notifications 2026-05-10-16:46
    // macOS attention notifications are enabled by default so a background
    // session that transitions into attention can surface itself without relying
    // on persistent status badges or completion sounds.
    //
    // CDXC:Notifications 2026-05-11-01:14
    // Keep this default-on even after adding macOS permission prompts and test
    // controls; users should opt out explicitly when they do not want banners.
    ("showMacOSAttentionNotifications", J::Bool(true)),
    // CDXC:Notifications 2026-11-07 DECISION: All stays the default; Needs you only (approved with PR #457) is the rule in packages/gx-core/src/attention.rs.
    ("attentionAlertsFor", J::Str("all")),
    ("workerNotifications", J::Str("off")),
    // CDXC:Notifications 2026-10-05 DECISION:
    // User: a banked reset that expires within 3 days, and again within 24 hours, also gets a system notification, which "can be switched off for the system" while the red bell row stays.
    ("resetExpirySystemNotifications", J::Bool(true)),
    // CDXC:Notifications 2026-11-01 DECISION:
    // User (Q929) on a switch and a colour for the light-blue border a session's pane gets when it finishes work: "Yes, add both", "make it disabled by default for new users too pls. keep the color and the switch in same row in settings. only show color picker if toggled on."
    ("showFinishedWorkBorder", J::Bool(false)),
    ("finishedWorkBorderColor", J::Str("#95d7f6")),
    // CDXC:Workarea 2026-05-23-00:50:
    // The session-id pane overlay preference is configurable, and the
    // native label itself must still render only for terminal panes that carry
    // zmx/tmux/zellij persistence metadata.
    //
    // CDXC:Workarea 2026-06-06-05:47:
    // Provider session ids in terminal panes are opt-in chrome. Keep the setting
    // disabled for default settings so new users do not see top-right provider
    // identifiers unless they explicitly enable the pane overlay.
    ("showQuickModelPickerInTerminal", J::Bool(true)),
    ("showSessionIdInTerminalPanes", J::Bool(false)),
    // CDXC:SessionChat 2026-11-24 DECISION:
    // User: "pls make native chat default for all new useres and dont even offerthem anything about terminal based chat which we're taking out soon". Native Chat is the default for all new users and onboarding never offers terminal-based chat; existing users' saved `"terminal"` preferences stay respected until terminal-based chat is removed.
    ("preferredAgentInterface", J::Str("chat")),
    // No per-agent overrides: every agent follows the global Default Agent View
    // until the user picks a different view for that agent in Settings > Agents.
    ("preferredAgentInterfaceOverrides", J::Obj(&[])),
    // CDXC:AgentLauncher 2026-10-30 DECISION: User: remember the agent picked on a project's agent button per project by default; per Space and everywhere (one choice for every project) are the other two.
    ("newSessionAgentScope", J::Str("project")),
    // CDXC:Sessions 2026-10-09 DECISION: User: "setting to enable this just for preferred chat view user (advanced). let's set disabled by default". Off until the user turns it on.
    ("closeEmptySessionsOnNew", J::Bool(false)),
    ("sidebarCollapseAnimationDurationMs", J::Num(400.0)),
    ("panelAnimationSpeed", J::Str("normal")),
    ("closeSidePanelWithLastTab", J::Bool(true)),
    ("sidebarTooltipDelayMs", J::Num(600.0)),
    // CDXC:Sidebar 2026-06-05-04:40:
    // First-run reset target is 275px, but users can change this Settings
    // value for explicit sidebar-handle double-click resets without changing the
    // last-width restore path used at app restart.
    ("sidebarDefaultWidthPx", J::Num(275.0)),
    ("groupWorkingSessions", J::Bool(false)),
    ("sidebarSpacesEnabled", J::Bool(true)),
    ("sidebarSpaceSwitchBehavior", J::Str("restore")),
    ("sidebarView", J::Str("project")),
    ("sidebarLayout", J::Str("rail")),
    // CDXC:Sidebar 2026-11-20 SEE-ALSO: gx-core sidebar_rail.rs (`RAIL_ITEMS`, the keys; `RAIL_DEFAULT_PINS`, which this matches: no Notifications, whose bell is in the sidebar's top bar with the rail on; Settings, the gear at the rail's very bottom since 2026-11-25); the rail's pinned icons, top to bottom. No Settings row: the rail's ⋯ menu and dragging are how it is set.
    (
        "sidebarRailPins",
        J::Arr(&[
            J::Str("search"),
            J::Str("mainHub"),
            J::Str("agentsHub"),
            J::Str("allAutomations"),
            J::Str("hermes"),
            J::Str("claude"),
            J::Str("grok"),
            J::Str("settings"),
        ]),
    ),
    // "pending" while an existing user's one-time "Meet the rail" intro has not been answered; set by `switch_existing_settings_to_rail`, "done" once it has opened. No Settings row.
    ("sidebarRailIntro", J::Str("")),
    // One-time desktop first-launch modal announcing Native mode (`""` | `"pending"` | `"done"`).
    ("nativeModeIntro", J::Str("")),
    ("unifiedWorkingSection", J::Bool(true)),
    ("unifiedGroupByProject", J::Bool(false)),
    ("unifiedSessionOrder", J::Str("recent")),
    // Off by default, so people who use tags as they are see no change (gx-core unified_sidebar/status_groups.rs).
    ("groupSessionsByTags", J::Bool(false)),
    // CDXC:Sidebar 2026-10-31 SEE-ALSO: gx-core unified_sidebar/grouping.rs; repository keys in the order the user dragged the project groups into. No Settings row: the drag is how it is set.
    ("unifiedProjectOrder", J::Arr(&[])),
    (
        "unifiedSessionCardHoverButtons",
        super::session_card_hover_actions::DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS,
    ),
    ("sidebarHideProjectsWithoutSessions", J::Bool(false)),
    ("sidebarSeparateByMachine", J::Bool(false)),
    // CDXC:RemoteMachines 2026-11-09 DECISION in gx-core sidebar_view/merged.rs: on by default.
    ("sidebarMergeRepositoryCopies", J::Bool(true)),
    // CDXC:RemoteMachines 2026-11-15 DECISION in gx-core sidebar_view/merged.rs: off by default.
    ("sidebarHideDisconnectedMachines", J::Bool(false)),
    // CDXC:RemoteMachines 2026-11-22 DECISION in apps/desktop native_sidebar/computer_mark.rs: on by default.
    ("hideComputerIconUntilHover", J::Bool(true)),
    // CDXC:RemoteMachines 2026-11-20 DECISION in apps/desktop native_sidebar/computer_mark.rs: on by default.
    ("markComputerYouAreOn", J::Bool(false)),
    // CDXC:Worktrees 2026-11-06 DECISION in gx-core sidebar_view/worktree_fold.rs.
    ("worktreeSessionsInProject", J::Bool(true)),
    // CDXC:Sidebar 2026-11-07 DECISION in gx-core sidebar_view/inputs.rs (Pinned order): Manual by default.
    ("pinnedSessionOrder", J::Str("manual")),
    // Settings > Sync, one switch per thing that syncs (gx-core settings_sync `SYNC_CATEGORIES`).
    ("syncSpaces", J::Bool(true)),
    ("syncAppearance", J::Bool(false)),
    ("syncAgentDefaults", J::Bool(false)),
    ("syncAgents", J::Bool(false)),
    ("syncSkills", J::Bool(false)),
    ("syncAccounts", J::Bool(false)),
    ("syncSoundsAndNotifications", J::Bool(false)),
    ("syncKeybindings", J::Bool(false)),
    ("syncCoordinator", J::Bool(false)),
    ("projectSwitchKeepAliveMinutes", J::Num(10.0)),
    ("sidebarSpaceFollowActiveSession", J::Bool(true)),
    // CDXC:Spaces 2026-11-12 DECISION in packages/settings-catalog/src/spaces.rs: Active projects is on by default.
    ("sidebarActiveProjects", J::Bool(true)),
    ("sidebarVisibilityMemory", J::Str("shared")),
    ("expandCollapsedProjectsOnJump", J::Bool(true)),
    // CDXC:Theming 2026-09-14 DECISION: User: default App theme to System, with Chat and Terminal on Follow app. Existing saved theme selections remain unchanged.
    // CDXC:Theming 2026-11-22 DECISION: User: default Ghostex's built-in theme to their current look (Dark mode, Polar Night artwork and colours, Gruvbox Material Light artwork and Everforest Light colours in light mode; see `darkThemePreset`).
    ("sidebarTheme", J::Str("dark-2")),
    ("sessionChatTheme", J::Str("app")),
    ("sessionChatFontFamily", J::Str("")),
    // CDXC:SessionChat 2026-10-31 DECISION: User: "add a slider to pick the color of the text how strong it is (color of text in the chat now is not full white, i have user wanting it to be full white". 0% is today's colour, so nobody's look changes until they move it; 100% is full white in dark mode and full black in light mode.
    ("sessionChatTextStrengthPercent", J::Num(0.0)),
    // CDXC:Settings 2026-11-05 DECISION: User (issue #425): one Interface scale for the sidebar, chat, Files and terminal font, with Scale areas separately off by default. A file that never saved the switch reads it as on when an area was already set away from its default (`interface_scale::scale_areas_separately`).
    ("interfaceScalePercent", J::Num(100.0)),
    ("interfaceScaleAreasSeparately", J::Bool(false)),
    // CDXC:PlatformSupport 2026-11-08 DECISION: User (issue #503): Linux takes its Display scale from the desktop by default; Settings can fix one instead (`interface_scale::LINUX_DISPLAY_SCALE_SETTING`).
    ("linuxDisplayScale", J::Str("auto")),
    ("sessionChatZoomPercent", J::Num(100.0)),
    ("filesZoomPercent", J::Num(100.0)),
    ("sessionChatCustomTranscriptWidthEnabled", J::Bool(false)),
    ("sessionChatTranscriptWidthPercent", J::Num(75.0)),
    // CDXC:SessionChat 2026-11-02 DECISION: User: "in settings i think we have setting for verbose, make that about defaulting to detailed view but make sure it's in advanced". Off, new chats open Normal (supersedes 2026-09-27's Simple mode by default, which this inherits as the off state); a saved `sessionChatVerboseMode` true or `sessionChatSimpleMode` false reads as on (`migrate_chat_view_default`).
    ("sessionChatDetailedView", J::Bool(false)),
    ("sessionChatFileEditPreviews", J::Bool(false)),
    // CDXC:Experience 2026-11-15 SEE-ALSO: the chat box switches and Chill's Quiet View that Settings > Experience sets (packages/settings-catalog/src/experience.rs, where the user's decisions are). On by default except the calm Quiet View, so nothing moves on update (Q3287).
    ("chatComposerModePicker", J::Bool(true)),
    ("chatComposerTerminalButton", J::Bool(true)),
    ("chatComposerContextRing", J::Bool(true)),
    ("chatComposerMaximize", J::Bool(true)),
    ("chatQuietViewCalm", J::Bool(false)),
    ("chatVariedWorkingWords", J::Bool(false)),
    // CDXC:SessionChat 2026-10-16 DECISION: User, on a tester's request to draw the agent's thinking grey and its output white as terminals do: "Sure please do it as a toggle (disabled by default) so we can check how it looks." Off keeps the unified look; on, thinking takes the theme's muted text colour and replies its full foreground, in the desktop, web and phone chats.
    ("sessionChatMutedThinking", J::Bool(true)),
    ("sessionChatKeepComposerExpanded", J::Bool(false)),
    ("sessionChatConfirmEscapeInterrupt", J::Bool(true)),
    // CDXC:CatchMeUp 2026-10-11 DECISION: User: "disable this feature by default. add a toggle to enable it under chat". The composer's Catch me up button shows only while this is on.
    ("chatCatchMeUp", J::Bool(false)),
    // CDXC:Onboarding 2026-11-11 SEE-ALSO: the tips card under a new chat's headline (packages/gx-chat-core/src/extras/new_chat_tips.rs, where the user's decision is); on by default, off from the card's Hide tips.
    ("newChatTips", J::Bool(true)),
    ("sessionDetailsClaudeTaskTools", J::Bool(true)),
    // CDXC:AgentProviders 2026-11-14 SEE-ALSO: Claude Code's own idle compaction and prompt cache length, set per launch (server/src/agents/claude_idle_compaction.rs, where the user's decision is). Claude's defaults: on, 200K, its automatic cache length.
    ("claudeIdleCompaction", J::Bool(true)),
    ("claudeIdleCompactAbove", J::Str("200000")),
    ("claudeOneHourPromptCache", J::Bool(false)),
    // CDXC:SessionChat 2026-11-22 DECISION: User (Q4396, ask on enter): when pressing Enter on a long chat (>= Compact chats above) whose prompt cache has expired, offer to compact first to save usage, with Don't ask again for This chat or Always (which turns this off). On by default.
    ("offerCompactBeforeSending", J::Bool(true)),
    ("compactWorkersBeforeReturn", J::Bool(true)),
    // CDXC:AgentProviders 2026-11-20 SEE-ALSO: a message sent to a busy OMP or Pi steers it, as Enter does in its own terminal (server/src/session_chat_queue_runtime/pi_channel_send.rs, where the user's decision is).
    ("piMessagesWhileWorking", J::Str("steer")),
    // CDXC:VoiceMode 2026-10-20 WHY:
    // xAI is the default provider: Grok Voice is billed per minute at a known price, and Grok dictation already needs the xAI key. Each provider keeps its own voice; the API keys are not settings (gxserver seals them, see server/src/voice_agent/keys.rs).
    ("voiceAgentProvider", J::Str("xai")),
    ("voiceAgentXaiVoice", J::Str("lux")),
    ("voiceAgentOpenaiVoice", J::Str("marin")),
    ("voiceAgentSpeed", J::Str("1")),
    ("voiceAgentCallMode", J::Str("regular")),
    // On: a Regular call answers once the user says "reply", "over" or "go ahead" (server/src/voice_agent/keys.rs).
    ("voiceAgentWaitForReply", J::Bool(true)),
    // CDXC:VoiceMode 2026-11-05 DECISION: User (Q1261): the Dictate hotkey is Auto, a tap toggles and a hold of 300 ms or more records until release. These three are this app's own Dictate settings (the engine, model and clean-up are gxserver's, per computer); an empty microphone is the system default input.
    ("dictateHotkeyMode", J::Str("auto")),
    ("dictateMicrophone", J::Str("")),
    ("dictateInsertSpacing", J::Bool(true)),
    // Read aloud's Voice (empty is the system voice) and Speed, on Settings > Chat. Auto-read has no
    // saved value: CDXC:VoiceMode 2026-10-21 DECISION in packages/gx-chat-core/src/read_aloud/events.rs.
    ("readAloudVoice", J::Str("")),
    ("readAloudRate", J::Num(1.0)),
    // CDXC:SessionChat 2026-10-16 DECISION: User: "please add a setting to enable the behavior ... but keep it disabled by default" and "make it an advanced setting". Picking an answer sends a one-question card only while this is on.
    ("chatSendOnPick", J::Bool(false)),
    ("chatAnswersWaitForIdle", J::Bool(false)),
    ("chatPageOpenTarget", J::Str("popup")),
    // CDXC:SessionChat 2026-11-26 SEE-ALSO: the Inbox reply arrow opens the reply form unless set to Reply in the chat; the user decision is on `open_row_reply` in apps/desktop/src/app/native_chat/row_reply.rs.
    ("chatInboxReplyArrow", J::Str("form")),
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: the chat look settings (Message layout, Your messages, Status line in the chat) are opt-in; Agent work and Tool call cards are retired (`migrate_retired_chat_looks` in lib.rs), all Default; the user decisions are in packages/gx-chat-core/src/transcript/chat_look.rs.
    ("chatMessageLayout", J::Str("default")),
    ("chatUserMessageStyle", J::Str("default")),
    ("chatStatusInTranscript", J::Bool(false)),
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: Chat box matches transcript width is opt-in, off; the user decision is on the chat box in apps/desktop/src/app/native_chat/composer.rs.
    ("chatComposerMatchesTranscriptWidth", J::Bool(false)),
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: Message spacing's user decision is in packages/gx-chat-core/src/transcript/chat_look.rs.
    ("chatMessageSpacing", J::Str("default")),
    // CDXC:SessionChat 2026-11-13 SEE-ALSO: Quiet View's user decisions are in packages/gx-chat-core/src/transcript/quiet_view.rs: coordinators on (2026-11-11), and since the chat modes page (Q2771, Q2776) every other chat and the expanded last turn on too, for new users and for anyone who never saved these.
    ("chatQuietViewCoordinators", J::Bool(true)),
    ("chatQuietViewChats", J::Bool(true)),
    ("chatQuietViewExpandLast", J::Bool(true)),
    // CDXC:Coordinators 2026-11-05 DECISION: the Board pill opens the side list until the user picks Columns (extras/coordinator_board.rs in gx-chat-core).
    ("coordinatorBoardView", J::Str("list")),
    // CDXC:Coordinators 2026-11-10 DECISION: the board's columns view collapses any column to its strip; Finished today starts collapsed (apps/desktop/src/app/native_chat/coordinator_board.rs `board_columns_body`).
    ("coordinatorBoardCollapsedColumns", J::Str("done")),
    // CDXC:AgentGuide 2026-11-02 DECISION: User (Q1066): the short Ghostex note goes to every agent session Ghostex starts, "on by default (with a setting to turn it off)".
    ("agentGhostexNote", J::Bool(true)),
    // CDXC:AgentProviders 2026-11-20 SEE-ALSO: Lean Codex workers, off by default (server/src/agents/codex_lean.rs).
    ("leanCodexWorkers", J::Bool(false)),
    ("readAgentLiveStatus", J::Bool(true)),
    // CDXC:AgentGuide 2026-11-02 DECISION: User (#263): "if user asks for browser we ask which the user prefers (internal ghostex browser, or external browser)", and (Q1069) the answer is kept in this setting, which the note and `ghostex agent-guide` then state so agents stop asking. Ask until the user picks.
    ("agentPreviewBrowser", J::Str("ask")),
    // CDXC:SwitchRequests 2026-11-26 DECISION: User (Q5021-Q5022, #762): off by default, so an agent's page open asks with a toast first; the toast's Always or Settings > Browser turns it on, and only the user can (help/mod.rs `AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING`).
    (
        crate::help::AGENT_PAGES_OPEN_WITHOUT_ASKING_SETTING,
        J::Bool(false),
    ),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: "force the marked handovers for coordinators/orchestrator threads and their children by default and make it disabled by default for non-orchestrator/child threads". This switch covers every other session and stays off by default; threads mark regardless. Since 2026-11-11 coordinators follow this switch too, unless `orchestratorPlainReplies` is on (CDXC:Coordinators 2026-11-11 DECISION in server/src/coordinators/role.rs).
    // CDXC:MarkedHandovers 2026-11-13 DECISION: User (Q2963 "A"), after a user lost their coordinators' marked sections in 11.4.0 without changing anything: Marked handovers is one setting with three values, Off, Coordinators only and Every session (`MARKED_HANDOVERS_OPTIONS`), "Coordinators only" by default, so coordinators mark their replies again out of the box while other sessions stay as they were. Supersedes the boolean above and `orchestratorPlainReplies` (Coordinators answer without markers), which is gone: a saved true here reads as Every session, false or nothing as Coordinators only, and plain coordinator replies with this off as Off (`migrate_marked_handovers` in lib.rs). Threads mark their reports under every value.
    ("markedHandovers", J::Str("coordinators")),
    // CDXC:Coordinators 2026-10-20 DECISION: User: "By default, we just show high and X high, but if he enables this, he can see all of the effort levels ... make the settings advanced." Off by default, so orchestrator effort pickers list only Low, Medium, High and XHigh since 2026-10-31 ("please allow setting low as the orchestrator effort level"; Medium joined on 2026-10-29 with "allow picking medium here but default to high"; `orchestrator_efforts` in packages/gx-chat-core/src/menus/orchestrator_effort.rs).
    ("orchestratorAllEfforts", J::Bool(false)),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: coloured labels (style D) by default, their first pick of the three.
    ("markedHandoversStyle", J::Str("labels")),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: "add a toggle for emojis in the colorful handover please. and set it to off by default."
    ("markedHandoversEmoji", J::Bool(false)),
    // CDXC:MarkedHandovers 2026-10-20 DECISION: User: "i want an advanced setting to be able to set how strong the colors of these headings in the handover are" and "make current colors the max". 100% (the default) draws today's palette; lower blends each heading colour toward the normal text colour, 0% leaving plain bold text. Nothing goes more vivid than today.
    // CDXC:MarkedHandovers 2026-10-22 DECISION: User: "make handover color strength 60 by default". Supersedes the 100 default above; saved values keep what the user chose, and 100% is still the full palette.
    ("markedHandoversHeadingStrength", J::Num(30.0)),
    // CDXC:Theming 2026-06-15-11:24:
    // Custom sidebar/titlebar colors are scoped to the sidebar and titlebar.
    // The default background matches Dark Gray chrome without changing modal or
    // dropdown color tokens.
    //
    // CDXC:Theming 2026-06-15-13:22:
    // Foreground is derived from background luminance, so the default foreground
    // remains light for Dark Gray and flips to the dark foreground on light
    // custom backgrounds.
    //
    // CDXC:Theming 2026-06-15-13:45:
    // The custom background contrast slider defaults near Dark Gray and is
    // restricted to dark applied values to avoid arbitrary bright color blends
    // in sidebar rows.
    //
    // CDXC:Theming 2026-06-15-15:01:
    // Clamp the slider to 85-100 per visual review; lighter values made the
    // sidebar feel too gray.
    //
    // CDXC:Theming 2026-06-15-15:15:
    // Keep this persisted field named darkness for compatibility while Settings
    // labels the same control Background Contrast.
    //
    // CDXC:Theming 2026-06-15-15:28:
    // The tint picker originally defaulted to neutral #808080. The tint
    // algorithm now maps picker colors to very dark chrome backgrounds, so
    // neutral same-channel tints do not change Dark Gray chrome.
    //
    // CDXC:Theming 2026-06-16-14:28:
    // The custom chrome default is now 95 contrast with white #FFFFFF tint.
    // Store the computed default background with those controls so Settings,
    // native startup, and protocol snapshots agree.
    //
    // CDXC:Theming 2026-07-22:
    // Default app chrome to neutral #808080 at 93 Background Contrast,
    // resolving to #141414.
    //
    // CDXC:Theming 2026-09-08 SEE-ALSO:
    // titlebar-color.ts owns the default matching the user's saved neutral #808080 tint at 96 contrast (#0b0b0b).
    //
    // Background Contrast and Background Tint are always-active Theming controls.
    ("customSidebarTitlebarForegroundColor", J::Str("#d8d8d8")),
    // Since 2026-11-22 the default dark look's colour (see `darkThemePreset`); Graphite keeps #808080.
    (
        "customSidebarTitlebarBackgroundTintColor",
        J::Str("#404e78"),
    ),
    (
        "customSidebarTitlebarBackgroundDarknessPercent",
        J::Num(96.0),
    ),
    ("customSidebarTitlebarBackgroundColor", J::Str("#0b0b0b")),
    // CDXC:Theming 2026-11-20 DECISION:
    // User: "make my current theme values the default for Ghostex in dark mode, with transparency and all that." Out of the box dark mode is their look: the Black colour square, glass tinted 88 in the sidebar and the work area (Strength 20), a 4 point blur, the Ghostex Kanagawa Dragon terminal palette, faint borders (`#3e3e3e6b`) and dark message bubbles (`#0000006b`), and Ink as the Live background. Light mode keeps its own defaults (its light tints move to 93 only so the one Strength slider reads 20; light mode is solid out of the box, so they change nothing there). Their App font is not shipped with Ghostex, so the App font stays App default; Terminal background stays Black / white (the CDXC:Theming 2026-09-28 decision in terminal_gpui_engine.rs), which in dark mode with Black looks the same. Supersedes Gray (2026-07-22), Strength 10 (2026-10-23) and the GitHub Dark dark palette (2026-10-14) as defaults. Then (Q4290) long-time users keep their current look: `keep_old_look_for_existing_settings` (lib.rs) writes the old defaults into an existing settings file once. Superseded on 2026-11-21 by their Windows look (above).
    // CDXC:Theming 2026-11-21 DECISION:
    // User: "can u please make the current theme i have on windows now the default dark theme for people when they first install ghostex? i dont like the fully black one we have." Out of the box dark mode is their Windows look (the agent-made Inkstone look without its fonts): the Custom colour #46506a at depth 96, the Inkstone theme colours (paper-white text #ece4d6, grey text #9b968d, a cinnabar accent and focus ring #d8664f, borders 8% and dividers 6%, cards, controls, menus and the chat box in near-neutral blue-blacks, selection 9%, hover 5%, muted success, warning and danger), the cinnabar pane outline colour, and Live glass behind the window: Ink at speed 0.3 and brightness 25. Live draws the same on macOS, Windows and Linux (no system blur), so the look matches across platforms; Blur stays 4 because Live is not blurred by it on macOS, and Blur on Windows stays Acrylic for Desktop and windows. Kept as they were: transparency off by default on Windows (the 2026-10-04 decision in availability.rs) and one tint for the sidebar and the work area (88, the 2026-10-23 Strength decision; their Windows sidebar is 82). Fonts are not part of a look (their Windows fonts are the defaults anyway). Supersedes the 2026-11-20 Black look below. Long-time users keep their current look (`DARK_LOOK_2026_11_21_OLD_DEFAULTS` in lib.rs). Superseded on 2026-11-22 by their Polar Night art look below.
    // CDXC:Theming 2026-11-22 DECISION:
    // User: "I want my current theme to be the default one for Ghostex (same art, same everything)." Out of the box Ghostex uses Dark appearance (`dark-2`), Custom dark tint `#404e78` at depth 96, Custom light tint `#fcf3da` at depth 97, Polar Night artwork in dark mode (`polar-night`) and Gruvbox Material Light artwork in light mode (`gm-light`) with dimming 45 and blur 0, Transparency Always (`frosted`; Windows keeps `opaque` in availability.rs while Art still paints in an opaque window), Same for light and dark off, Picture position desktop, Animated styles Aurora (dark) and Ink (light) (keeping the raised 1.0 speed and 80 brightness defaults), glass tints 61/67 (dark) and 64/64 (light) (Strength 45), Polar Night dark theme colours with `#5581a9` accent/focus ring and Everforest Light light theme colours, terminal palettes Ghostex Nord (dark) and Ghostex Everforest Light (light) with Follow theme terminal background, and `#7cbf9f` active pane outline. Fonts stay unchanged (not part of a theme). Changes only the catalog defaults so saved user settings are untouched.
    // CDXC:Theming 2026-11-24 DECISION:
    // User: in Ghostex's default theme, the notification bell's unread badge blue (`#5581a9`, from `themeRoles.dark.accent`) is too dark; change it for everyone using the default theme to `#95d7f6` (dark mode) and a matching high-contrast sky blue `#0c74a6` in light mode (~5.0:1 on `#fdfbf4`, ~5.2:1 against white badge ink). Existing saved default-theme files migrate once via `migrate_default_theme_accent` (lib.rs).
    ("darkThemePreset", J::Str("custom")),
    ("lightThemePreset", J::Str("custom")),
    ("themeSidebarContrast", J::Num(0.0)),
    ("themeWorkAreaContrast", J::Num(0.0)),
    (
        "themeRoles",
        J::Obj(&[
            (
                "dark",
                J::Obj(&[
                    ("accent", J::Str("#95d7f6")),
                    ("focusRing", J::Str("#5581a9")),
                    ("border", J::Str("8%")),
                    ("divider", J::Str("6%")),
                    ("text", J::Str("#e6e4de")),
                    ("mutedText", J::Str("#8c95a3")),
                    ("userMessage", J::Str("base+12%")),
                    ("chatBox", J::Str("base+6%/80%")),
                    ("surface", J::Str("base+8%/80%")),
                    ("control", J::Str("base+10%/80%")),
                    ("menu", J::Str("base+6%/95%")),
                    ("selection", J::Str("9%")),
                    ("hover", J::Str("5%")),
                    ("success", J::Str("#a3c48e")),
                    ("warning", J::Str("#dcbd7a")),
                    ("danger", J::Str("#d9827c")),
                ]),
            ),
            (
                "light",
                J::Obj(&[
                    ("text", J::Str("#5c6a72")),
                    ("mutedText", J::Str("#939f91")),
                    ("accent", J::Str("#0c74a6")),
                    ("focusRing", J::Str("#8da101")),
                    ("link", J::Str("#3a94c5")),
                    ("success", J::Str("#8da101")),
                    ("warning", J::Str("#dfa000")),
                    ("danger", J::Str("#f85552")),
                    ("userMessage", J::Str("#efebd4")),
                    ("codeBlock", J::Str("#f4f0d9")),
                    ("selection", J::Str("#eaedc8")),
                ]),
            ),
        ]),
    ),
    (
        "customSidebarTitlebarLightBackgroundTintColor",
        J::Str("#fcf3da"),
    ),
    (
        "customSidebarTitlebarLightBackgroundLightnessPercent",
        J::Num(97.0),
    ),
    (
        "customSidebarTitlebarLightBackgroundColor",
        J::Str("#f4f4f5"),
    ),
    // CDXC:Terminal 2026-05-22-12:29:
    // New Ghostex terminals should default to the requested terminal profile
    // (its dark palette is Ghostex Kanagawa Dragon since 2026-11-20, see `darkThemePreset`): JetBrains Mono 13pt, bar cursor with blink, wght=300, 20% cell
    // height expansion, 15 MB scrollback, no copy-on-select, and one-to-one
    // precision/discrete mouse scrolling.
    ("terminalCursorStyle", J::Str("bar")),
    ("terminalCursorStyleBlink", J::Bool(true)),
    // macOS Metal terminal effect; ordinary rendering remains the default.
    ("terminalShadersEnabled", J::Bool(false)),
    // CDXC:PlatformSupport 2026-09-14 DECISION: User: make PowerShell the default Windows environment.
    ("windowsTerminalBackend", J::Str("powershell")),
    ("windowsWslDistribution", J::Str("")),
    ("terminalFontFamily", J::Str("JetBrains Mono")),
    ("terminalFontSize", J::Num(13.0)),
    ("terminalFontWeight", J::Num(300.0)),
    ("terminalBoldUsesHeavierFont", J::Bool(true)),
    ("terminalColorScheme", J::Str("app")),
    (
        "terminalGhosttyLightTheme",
        J::Str("Ghostex Everforest Light"),
    ),
    ("terminalGhosttyTheme", J::Str("Ghostex Nord")),
    ("terminalBackgroundOpacity", J::Num(100.0)),
    ("terminalBackgroundImage", J::Str("")),
    ("terminalBackgroundImageOpacity", J::Num(1.0)),
    ("terminalLetterSpacing", J::Num(0.0)),
    ("terminalLineHeight", J::Num(1.2)),
    ("terminalViewWidthMode", J::Str("full")),
    ("terminalViewWidthPercent", J::Num(75.0)),
    ("terminalWidthApplyToCommandPaneTerminals", J::Bool(false)),
    ("terminalPaneHorizontalPaddingPx", J::Num(0.0)),
    ("terminalPaneVerticalPaddingPx", J::Num(0.0)),
    ("terminalMouseScrollMultiplierDiscrete", J::Num(1.0)),
    ("terminalMouseScrollMultiplierPrecision", J::Num(1.0)),
    ("terminalScrollToBottomWhenTyping", J::Bool(true)),
    ("terminalScrollbackLimitMb", J::Num(15.0)),
    ("terminalCopyOnSelect", J::Str("false")),
    ("terminalConfirmCloseSurface", J::Str("true")),
    ("terminalClipboardTrimTrailingSpaces", J::Bool(true)),
    ("terminalClipboardPasteProtection", J::Bool(true)),
    ("terminalPastePreviewableImages", J::Bool(true)),
    ("terminalMouseHideWhileTyping", J::Bool(false)),
    ("terminalScrollbar", J::Str("system")),
    // CDXC:Resources 2026-06-23-19:22:
    // New installs should discover local dev servers from terminal output and start with no ignored ports.
    //
    // CDXC:Navigation 2026-08-19:
    // Where a detected URL opens is no longer a Dev Servers choice; it follows webLinkOpenTarget with every other web link.
    ("terminalDevServerDetectionEnabled", J::Bool(true)),
    ("terminalDevServerIgnoredPortRules", J::Arr(&[])),
    // CDXC:PromptEditor 2026-05-13-15:58
    // Ctrl+G rich prompt editing originally defaulted to the floating Monaco editor.
    //
    // CDXC:PromptEditor 2026-05-25-11:31:
    // Monaco is the out-of-the-box Ctrl+G prompt editor again. New installs should open the floating Monaco editor for local app terminals.
    //
    // CDXC:PromptEditor 2026-06-30-00:08:
    // Settings must expose only Monaco and the user's machine default editor. Removed gte and custom selections migrate to inherit so Ctrl+G stops injecting a Ghostex-owned editor command when users choose the machine default path.
    ("promptEditorBackend", J::Str("monaco")),
    (
        "hotkeys",
        J::Obj(&[
            ("scrollChatToBottom", J::Str("ctrl+shift+down")),
            ("focusChatComposer", J::Str("")),
            ("copyLastChatCodeBlock", J::Str("cmd+shift+;")),
            ("copyLastChatReply", J::Str("cmd+shift+c")),
            ("toggleChatSummaryMode", J::Str("ctrl+alt+s")),
            ("toggleChatComposerMaximized", J::Str("ctrl+alt+m")),
            ("toggleChatFilesLinks", J::Str("ctrl+alt+l")),
            ("newChatPickProject", J::Str("alt+o")),
            ("newChatPickComputer", J::Str("alt+m")),
            ("newChatPickWhere", J::Str("alt+w")),
            ("newChatPickBranch", J::Str("alt+b")),
            ("newChatPickAgent", J::Str("alt+a")),
            ("toggleVoiceAgent", J::Str("ctrl+alt+v")),
            ("voiceCallAboutSession", J::Str("")),
            ("holdToTalk", J::Str("")),
            ("toggleDictation", J::Str("ctrl+alt+space")),
            ("createAgentSession", J::Str("cmd+n")),
            ("createSession", J::Str("cmd+shift+t")),
            ("openCommandPalette", J::Str("cmd+shift+p")),
            ("openSessionSearchPalette", J::Str("cmd+p")),
            ("openProjectSearchPalette", J::Str("cmd+alt+shift+o")),
            ("openQuickAccessFiles", J::Str("cmd+o")),
            ("openCommandsPanel", J::Str("cmd+j")),
            ("openCommandsPanelSecondKey", J::Str("shift+escape")),
            ("openSettings", J::Str("cmd+,")),
            ("openExtensions", J::Str("")),
            ("openGhostexHelp", J::Str("")),
            ("toggleScreenshotMode", J::Str("")),
            ("openFileInFiles", J::Str("")),
            ("openHotkeys", J::Str("cmd+/")),
            ("toggleSidebarCollapsed", J::Str("cmd+b")),
            ("toggleViewPanel", J::Str("cmd+alt+b")),
            ("expandViewPanel", J::Str("cmd+ctrl+e")),
            ("expandViewPanelFully", J::Str("cmd+ctrl+shift+e")),
            ("renameActiveSession", J::Str("cmd+r")),
            ("openBrowserPane", J::Str("cmd+t")),
            ("switchAgentsView", J::Str("")),
            ("switchSourceView", J::Str("")),
            ("switchGitHubView", J::Str("")),
            ("switchKanbanView", J::Str("")),
            ("switchManageView", J::Str("")),
            ("switchAutomateView", J::Str("")),
            ("switchChangesView", J::Str("")),
            ("switchTerminalView", J::Str("")),
            ("switchTitlebarView1", J::Str("alt+1")),
            ("switchTitlebarView2", J::Str("alt+2")),
            ("switchTitlebarView3", J::Str("alt+3")),
            ("switchTitlebarView4", J::Str("alt+4")),
            ("switchTitlebarView5", J::Str("alt+5")),
            ("switchTitlebarView6", J::Str("alt+6")),
            ("switchTitlebarView7", J::Str("alt+7")),
            ("switchTitlebarView8", J::Str("alt+8")),
            ("switchTitlebarView9", J::Str("alt+9")),
            ("rotatePanesClockwise", J::Str("ctrl+shift+l")),
            ("mergeAllTabs", J::Str("ctrl+shift+m")),
            ("delayedSend", J::Str("ctrl+shift+s")),
            ("closeAfterDone", J::Str("")),
            ("openModelPicker", J::Str("alt+p")),
            ("promptEditor", J::Str("ctrl+g")),
            ("attachFileOrFolder", J::Str("cmd+alt+p")),
            ("sessionNote", J::Str("cmd+alt+n")),
            ("stashPrompt", J::Str("alt+s")),
            ("stashedPrompts", J::Str("cmd+alt+s")),
            ("exportTranscript", J::Str("")),
            ("toggleAgentActions", J::Str("cmd+alt+a")),
            ("toggleChatView", J::Str("alt+g")),
            ("openFindPrompts", J::Str("cmd+shift+f")),
            ("searchProjectFiles", J::Str("cmd+shift+f")),
            ("navigateFilesBack", J::Str("cmd+[")),
            ("navigateFilesForward", J::Str("cmd+]")),
            ("scrollTerminalToTop", J::Str("")),
            ("scrollTerminalToBottom", J::Str("")),
            ("splitSessionRight", J::Str("alt+shift+d")),
            ("forkSession", J::Str("cmd+ctrl+shift+f")),
            ("reloadSession", J::Str("")),
            ("sleepFocusedSession", J::Str("cmd+shift+a")),
            ("wakeFocusedSession", J::Str("")),
            ("closeFocusedSession", J::Str("cmd+shift+backspace")),
            ("popOutPane", J::Str("ctrl+shift+o")),
            ("generateSessionTitle", J::Str("")),
            ("focusPreviousGroup", J::Str("")),
            ("focusNextGroup", J::Str("")),
            ("navigateHistoryBack", J::Str("cmd+[")),
            ("navigateHistoryForward", J::Str("cmd+]")),
            ("openNotifications", J::Str("cmd+i")),
            ("jumpToLatestUnreadNotification", J::Str("cmd+shift+u")),
            ("deferNotificationAndJumpNext", J::Str("cmd+ctrl+u")),
            ("focusPreviousSession", J::Str("ctrl+shift+tab")),
            ("focusNextSession", J::Str("ctrl+tab")),
            ("focusPreviousPaneTab", J::Str("cmd+alt+[")),
            ("focusNextPaneTab", J::Str("cmd+alt+]")),
            ("focusUp", J::Str("cmd+alt+up")),
            ("focusRight", J::Str("cmd+alt+right")),
            ("focusDown", J::Str("cmd+alt+down")),
            ("focusLeft", J::Str("cmd+alt+left")),
            ("jumpToProject1", J::Str("cmd+ctrl+1")),
            ("jumpToProject2", J::Str("cmd+ctrl+2")),
            ("jumpToProject3", J::Str("cmd+ctrl+3")),
            ("jumpToProject4", J::Str("cmd+ctrl+4")),
            ("jumpToProject5", J::Str("cmd+ctrl+5")),
            ("jumpToProject6", J::Str("cmd+ctrl+6")),
            ("jumpToProject7", J::Str("cmd+ctrl+7")),
            ("jumpToProject8", J::Str("cmd+ctrl+8")),
            ("jumpToProject9", J::Str("cmd+ctrl+9")),
            ("goToSpace1", J::Str("cmd+alt+shift+1")),
            ("goToSpace2", J::Str("cmd+alt+shift+2")),
            ("goToSpace3", J::Str("cmd+alt+shift+3")),
            ("goToSpace4", J::Str("cmd+alt+shift+4")),
            ("goToSpace5", J::Str("cmd+alt+shift+5")),
            ("goToSpace6", J::Str("cmd+alt+shift+6")),
            ("goToSpace7", J::Str("cmd+alt+shift+7")),
            ("goToSpace8", J::Str("cmd+alt+shift+8")),
            ("goToSpace9", J::Str("cmd+alt+shift+9")),
            ("goToWorkspace1", J::Str("ctrl+alt+1")),
            ("goToWorkspace2", J::Str("ctrl+alt+2")),
            ("goToWorkspace3", J::Str("ctrl+alt+3")),
            ("goToWorkspace4", J::Str("ctrl+alt+4")),
            ("goToWorkspace5", J::Str("ctrl+alt+5")),
            ("goToWorkspace6", J::Str("ctrl+alt+6")),
            ("goToWorkspace7", J::Str("ctrl+alt+7")),
            ("goToWorkspace8", J::Str("ctrl+alt+8")),
            ("goToWorkspace9", J::Str("ctrl+alt+9")),
            ("focusSessionSlot1", J::Str("cmd+1")),
            ("focusSessionSlot2", J::Str("cmd+2")),
            ("focusSessionSlot3", J::Str("cmd+3")),
            ("focusSessionSlot4", J::Str("cmd+4")),
            ("focusSessionSlot5", J::Str("cmd+5")),
            ("focusSessionSlot6", J::Str("cmd+6")),
            ("focusSessionSlot7", J::Str("cmd+7")),
            ("focusSessionSlot8", J::Str("cmd+8")),
            ("focusSessionSlot9", J::Str("cmd+9")),
            ("runActionSlot1", J::Str("ctrl+shift+1")),
            ("runActionSlot2", J::Str("ctrl+shift+2")),
            ("runActionSlot3", J::Str("ctrl+shift+3")),
            ("runActionSlot4", J::Str("ctrl+shift+4")),
            ("runActionSlot5", J::Str("ctrl+shift+5")),
            ("splitMore", J::Str("cmd+d")),
            ("splitMoreDown", J::Str("cmd+shift+d")),
        ]),
    ),
    ("showActivePaneOutline", J::Bool(false)),
    // The default dark look's pane outline since 2026-11-22 (see `darkThemePreset`); #d8664f before.
    ("workspaceActivePaneBorderColor", J::Str("#5581a9")),
    // CDXC:Settings 2026-11-08 DECISION: User: "to show traffic lights on macos when sidebar is collapsed (default to yes)". On: the window's buttons stay with the sidebar collapsed, as they did before the 2026-11-06 floating-sidebar change (GitHub #432), which off restores.
    ("showWindowButtonsWhenSidebarCollapsed", J::Bool(false)),
    // CDXC:Workarea 2026-11-25 DECISION in apps/desktop/src/app/helpers/work_area_frame.rs: the work area stays the framed card by default; Floating sidebar is a choice.
    ("windowLayout", J::Str("framedWorkArea")),
    ("closeButtonsOnRight", J::Bool(false)),
    // CDXC:Theming 2026-11-21 DECISION:
    // User (through the coordinator): six dark art themes (Moonwater, Glasshouse, Red Earth, Deep Current, Night Garden, Nocturne Canyon) are Ghostex's main themes. Each paints its own artwork behind the sidebar and the work area of an opaque window: no desktop shows through. `windowArt` names the artwork (Polar Night by default since 2026-11-22, `gm-light` in light mode; None keeps the glass as it is), `windowArtDim` darkens it (percent black over it) and `windowArtBlur` softens it (points). The surfaces over it take the glass tints (`windowGlassSidebarOpacityDark`, `windowGlassWorkAreaTintDark`).
    // SEE-ALSO: apps/desktop/src/app/helpers/window_art.rs, data/theme_gallery.rs.
    ("windowArt", J::Str("polar-night")),
    ("windowArtLight", J::Str("gm-light")),
    ("windowArtDim", J::Num(45.0)),
    ("windowArtBlur", J::Num(0.0)),
    // CDXC:Theming 2026-11-25 (`stylise_artwork` in apps/desktop/src/app/helpers/window_art.rs): the artwork's style (Picture, the default, or ASCII or Dither) and whether it is drawn in the theme's text colour or greys (Black and white) or its own colours.
    ("windowArtMonochrome", J::Bool(false)),
    ("windowArtStyle", J::Str("picture")),
    // CDXC:Theming 2026-11-22 (`YOUR_IMAGE_ART_ID` in data/themes.rs): the user's own picture
    // behind the window, per computer; `savedThemes` are the looks saved under Your themes
    // (apps/desktop/src/app/window/settings_modal/tabs/theme/user_themes.rs).
    ("windowArtImage", J::Str("")),
    ("windowArtImageLight", J::Str("")),
    ("savedThemes", J::Arr(&[])),
    ("windowGlass", J::Str("frosted")),
    // Animated (stored as "live") since 2026-11-21, the default dark look's glass (see `darkThemePreset`); Desktop and windows before.
    ("windowGlassSource", J::Str("live")),
    ("windowGlassImagePlacement", J::Str("desktop")),
    ("windowGlassSameForLightAndDark", J::Bool(false)),
    ("windowGlassImageDark", J::Str("")),
    ("windowGlassImageLight", J::Str("")),
    ("windowGlassVideoDark", J::Str("")),
    ("windowGlassVideoLight", J::Str("")),
    ("windowGlassVideoOnlyOnPower", J::Bool(true)),
    ("windowGlassLiveStyleDark", J::Str("aurora")),
    ("windowGlassLiveStyleLight", J::Str("ink")),
    // CDXC:Theming 2026-11-22 DECISION: User: "Make the effect for Animated more noticeable by default: it's super not apparent now by default." Raised from 0.3 and 25 (2026-11-21) to 1.0 and 80 so motion is clearly visible on first pick while keeping text readable.
    ("windowGlassLiveSpeed", J::Num(1.0)),
    ("windowGlassLiveBrightness", J::Num(50.0)),
    ("windowGlassBlurRadius", J::Num(4.0)),
    ("windowGlassWindowsBlur", J::Str("acrylic")),
    // CDXC:Theming 2026-11-20 DECISION: User (Q4288): "yes, make pane gap 6 and menu blur 17 defaults too", from their own look (see `darkThemePreset`). Menu blur applies in light mode too. Supersedes 20.
    ("windowGlassMenuBlurRadius", J::Num(17.0)),
    // CDXC:Theming 2026-11-20 DECISION:
    // User: "pls make the sidebar and main area default to having the same strength for the transparency by default" (2026-10-23), then "make my current theme values the default for Ghostex in dark mode, with transparency and all that" (2026-11-20, see `darkThemePreset`), then "I want my current theme to be the default one for Ghostex (same art, same everything)" (2026-11-22: 61/67 dark, 64/64 light, Strength 45).
    // SEE-ALSO: apps/desktop/src/app/helpers/window_glass.rs, apps/desktop/src/app/window/onboarding/model.rs, apps/desktop/src/app/window/settings_modal/tabs/theme/colours.rs.
    ("windowGlassSidebarOpacityDark", J::Num(61.0)),
    ("windowGlassWorkAreaTintDark", J::Num(67.0)),
    ("windowGlassSidebarOpacityLight", J::Num(64.0)),
    ("windowGlassWorkAreaTintLight", J::Num(64.0)),
    ("terminalBackgroundMode", J::Str("theme")),
    ("workspaceBackgroundColor", J::Str("")),
    ("clickToWakeSleepingSessions", J::Bool(true)),
    ("dimSleepingSessions", J::Bool(false)),
    ("dimInactiveProjects", J::Bool(false)),
    ("wakeSleepingSessionsOnSelect", J::Bool(true)),
    ("customViews", J::Arr(&[])),
    ("customViewTemplates", J::Arr(&[])),
    ("viewScopes", J::Obj(&[])),
    ("titlebarViewOrder", J::Arr(&[])),
    // CDXC:Titlebar 2026-05-11-00:22
    // The titlebar Open In menu is configurable: built-in editor targets can be
    // hidden and user-defined command targets can be appended without changing
    // the default editor catalog.
    ("customWorkspaceOpenTargets", J::Arr(&[])),
    // CDXC:Titlebar 2026-05-11-02:03
    // First launch starts with only ghostex/Open Folder until the native sidebar performs
    // its one startup installed-target scan and persists the detected IDE list.
    //
    // CDXC:Titlebar 2026-06-04-13:39:
    // The default folder target should be described with OS-agnostic Open File/Folder Location copy even though the persisted target id remains finder for compatibility.
    (
        "workspaceOpenTargetAvailability",
        J::Obj(&[
            ("availableTargetIds", J::Arr(&[J::Str("finder")])),
            ("checkedAtMs", J::Num(0.0)),
            ("resolvedAppNames", J::Obj(&[])),
            ("resolvedCommands", J::Obj(&[])),
        ]),
    ),
    ("workspaceOpenTargetHiddenIds", J::Arr(&[])),
    // CDXC:Workarea 2026-05-30-07:24:
    // The macOS app no longer exposes Pane Gap as a user setting. Keep the
    // persisted field for settings compatibility; native panes render without
    // configurable spacing.
    // CDXC:Theming 2026-11-20 DECISION: User (Q4288): "yes, make pane gap 6 and menu blur 17 defaults too". The default is 6, as the user's own settings and the gallery themes have it; the desktop app does not read this key today, so it changes no spacing.
    ("workspacePaneGap", J::Num(6.0)),
    ("remoteMachines", J::Arr(&[])),
    ("remoteTailscaleEnabled", J::Bool(true)),
    ("commandsPanelDefaultHeightPx", J::Num(200.0)),
    ("commandsPanelSide", J::Str("bottom")),
    ("commandsPanelAutoMinimize", J::Bool(true)),
    ("commandsPanelAutoMinimizeDelaySeconds", J::Num(60.0)),
];

#[cfg(test)]
mod group_sessions_by_tags_tests {
    /// Off until someone turns it on, so people who use tags as they are see no change.
    #[test]
    fn group_sessions_by_tags_is_off_by_default() {
        let default = crate::default_value("groupSessionsByTags")
            .map(|value| crate::ToJson::to_json(value).to_value());
        assert_eq!(default, Some(serde_json::json!(false)));
    }
}
