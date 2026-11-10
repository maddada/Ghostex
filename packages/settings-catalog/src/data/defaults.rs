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
    ("trustProjectFoldersAutomatically", J::Bool(true)),
    ("agentHooksAutoInstall", J::Bool(false)),
    ("agentsTidyUpOfferDismissed", J::Bool(false)),
    ("agentManagerZoomPercent", J::Num(100.0)),
    ("defaultPromptAgentId", J::Str("codex")),
    ("sessionTitleGenerationAgent", J::Str("codex")),
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
    // CDXC:Extensions 2026-11-09 DECISION: User: "that url is mine but it should open grok.com usually please". The Grok page opens grok.com unless the user puts their own page (a Grok bot, say) here; no one's own page ships as a default.
    ("grokWebStartPage", J::Str("https://grok.com/")),
    // The Grok logins, each `{ "id", "name" }` with its own browser profile (apps/desktop/src/app/claude_web/grok.rs). Empty is one login named Grok.
    ("grokWebLogins", J::Arr(&[])),
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
    // CDXC:Navigation 2026-10-17 SEE-ALSO: these replace markdownFileOpenView, htmlFileOpenView and imageFileOpenTarget, which still decide until documentFileOpenTarget is saved (file_opening.rs).
    ("documentFileOpenTarget", J::Str("files")),
    ("fileChangesOpenTarget", J::Str("changes")),
    ("changesHideTestFiles", J::Bool(false)),
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
    ("codeViewTabHidden", J::Bool(false)),
    ("browserViewTabHidden", J::Bool(false)),
    ("kanbanViewTabHidden", J::Bool(false)),
    ("automateViewTabHidden", J::Bool(false)),
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
    ("tipsAndTricksTitlebarButtonHidden", J::Bool(false)),
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
    // CDXC:Extensions 2026-10-25 DECISION: Claude is off by default (official_extensions.rs).
    ("claudeWebHidden", J::Bool(true)),
    // CDXC:Extensions 2026-11-09 DECISION: Grok is off by default, like Claude (official_extensions.rs).
    ("grokWebHidden", J::Bool(true)),
    // CDXC:Extensions 2026-10-19 DECISION: Connectors, OS Integration and Debugging are on by default; their old gates still apply (official_extensions.rs).
    // Off by default since Experimental features became one switch each (2026-11-09).
    ("connectorsHidden", J::Bool(true)),
    ("osIntegrationHidden", J::Bool(true)),
    ("keepAwakeEnabled", J::Bool(false)),
    ("allAutomationsEnabled", J::Bool(false)),
    ("debuggingHidden", J::Bool(false)),
    // CDXC:VoiceMode 2026-10-25 DECISION: the voice agent is a built-in extension, on by default (official_extensions.rs).
    ("voiceAgentHidden", J::Bool(false)),
    // CDXC:Chief 2026-11-08 DECISION: Chief is a built-in extension, on by default (official_extensions.rs).
    // CDXC:Chief 2026-11-08 DECISION: User: "please disable chief of staff for now and make it an extension so ppl aren't confused by it until i finish working on it (call it beta in settings pls)", "i mean the Main Hub thingy". Main Hub (with Chief of Staff) is off by default under a new key: the settings file stores every default, so a new key is what turns it off once for every install that never chose, while anyone who turns it on after this release keeps it. Supersedes `chiefHidden` (on by default), which nothing reads any more.
    ("mainHubHidden", J::Bool(true)),
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
    ("codeServerLinkVscodeUserConfig", J::Bool(false)),
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
    ("completionSound", J::Str("arcade")),
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
    ("sleepSessionWhenParking", J::Bool(false)),
    // CDXC:Sessions 2026-09-12 DECISION:
    // User: "Park & Snooze with tags" is on by default, so Park and Snooze open the tag menu unless the user turns it off. Supersedes the 2026-09-11 off-by-default choice for the park-only version of this setting.
    ("showTagMenuWhenParking", J::Bool(true)),
    // CDXC:Sessions 2026-09-11 DECISION:
    // User: "Unpark after sending a message" defaults to on.
    ("unparkAfterSendingMessage", J::Bool(true)),
    // CDXC:Telemetry 2026-08-26:
    // Usage analytics are on by default and opt-out. Events carry only counts and
    // fixed-list values, tied to a one-way salted hash so one person's machines
    // group together. Nothing personal: no prompts, no paths, no project names,
    // and never the raw account id the hash is derived from.
    ("analyticsEnabled", J::Bool(true)),
    ("debuggingMode", J::Bool(false)),
    ("claudeHookFreeActivity", J::Bool(false)),
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
        J::Arr(&[
            J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("rename"))]),
            J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("pin"))]),
            J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("note"))]),
            J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("snooze"))]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("closeAfterDone")),
            ]),
            J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("tag"))]),
            J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("park"))]),
            J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("sleep"))]),
            J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("chevron"))]),
            J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("close"))]),
        ]),
    ),
    // CDXC:Sessions 2026-10-01 DECISION:
    // User: the enabled hover buttons also appear in the session context menu by default, in the menu's own ChatGPT-style order (Rename, Pin, Snooze, Park, Sleep, Note, Tag As); this supersedes the 2026-09-15 rule that they led the menu in the card's right-to-left order. Close is the exception and never appears in the menu while it is on the card. Turning this off restores the 2026-09-12 rule where every enabled hover button leaves the context menu.
    ("showSessionCardHoverButtonsInContextMenu", J::Bool(true)),
    // CDXC:Tooltips 2026-10-29 DECISION: User: "please let me disable the tooltip for sessions in the sidebar with a toggle (advanced setting)". Session tooltips stay on by default, as before; turning this off removes only the hover tooltip on a sidebar session row's title.
    ("showSidebarSessionTooltips", J::Bool(true)),
    // CDXC:Sessions 2026-06-13-15:42
    // Recommended is the default sidebar style and hides session-card Last Active
    // timestamps by default. Settings still owns an explicit toggle for users who
    // want the timestamp back, and the setting must not affect project-header git
    // diff stats.
    ("hideLastActiveTimeOnSessionCards", J::Bool(true)),
    ("highlightPendingQuestions", J::Bool(false)),
    ("hideAccountEmails", J::Bool(false)),
    // CDXC:AgentProviders 2026-10-06 DECISION:
    // User: Claude and Codex each get a setting to auto-redeem banked resets that are going to expire anyway, used 5 minutes before expiry ("no no shouldn't be 60 minutes before that's big waste. Let's do 5 minutes instead."); using one at a usage limit in its last 24 hours happens only "if user enables that toggle", the separate AtLimit switch. All off by default because a redeemed reset cannot be given back; gxserver owns the rule (server/src/accounts/reset_watch.rs). Supersedes the 2026-10-05 single switch that also covered the limit case, and the 60-minute last call.
    ("claudeAutoRedeemExpiringResets", J::Bool(false)),
    ("codexAutoRedeemExpiringResets", J::Bool(false)),
    ("claudeAutoRedeemResetsAtLimit", J::Bool(false)),
    ("codexAutoRedeemResetsAtLimit", J::Bool(false)),
    // CDXC:Sessions 2026-06-13-17:50:
    // First-run sidebar tag filter settings should show the default triage tags,
    // the No tag filter, and the default separators. Users opt out by hiding or
    // disabling individual rows from the collapsed Sidebar Tags settings area.
    (
        "sidebarSessionTagListItems",
        J::Arr(&[
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("favorite")),
                ("tag", J::Str("favorite")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("high-priority")),
                ("tag", J::Str("high-priority")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(false)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("low-priority")),
                ("tag", J::Str("low-priority")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(false)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("separator-priority-progress")),
                ("type", J::Str("separator")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("todo")),
                ("tag", J::Str("todo")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(false)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("in-progress")),
                ("tag", J::Str("in-progress")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("testing")),
                ("tag", J::Str("testing")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("blocked")),
                ("tag", J::Str("blocked")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("on-hold")),
                ("tag", J::Str("on-hold")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("done")),
                ("tag", J::Str("done")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("separator-progress-type")),
                ("type", J::Str("separator")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("research")),
                ("tag", J::Str("research")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("bug")),
                ("tag", J::Str("bug")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(false)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(false)),
                ("id", J::Str("feature")),
                ("tag", J::Str("feature")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(false)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("design")),
                ("tag", J::Str("design")),
                ("type", J::Str("tag")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("separator-type-untagged")),
                ("type", J::Str("separator")),
                ("visible", J::Bool(true)),
            ]),
            J::Obj(&[
                ("enabled", J::Bool(true)),
                ("id", J::Str("untagged")),
                ("type", J::Str("untagged")),
                ("visible", J::Bool(true)),
            ]),
        ]),
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
    ("sidebarSpacesEnabled", J::Bool(false)),
    ("sidebarSpaceSwitchBehavior", J::Str("restore")),
    ("sidebarView", J::Str("project")),
    ("unifiedWorkingSection", J::Bool(true)),
    ("unifiedGroupByProject", J::Bool(false)),
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
    ("sidebarSpaceFollowActiveSession", J::Bool(false)),
    ("sidebarVisibilityMemory", J::Str("shared")),
    ("expandCollapsedProjectsOnJump", J::Bool(true)),
    // CDXC:Theming 2026-09-14 DECISION: User: default App theme to System, with Chat and Terminal on Follow app. Existing saved theme selections remain unchanged.
    ("sidebarTheme", J::Str("system")),
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
    // CDXC:SessionChat 2026-10-16 DECISION: User, on a tester's request to draw the agent's thinking grey and its output white as terminals do: "Sure please do it as a toggle (disabled by default) so we can check how it looks." Off keeps the unified look; on, thinking takes the theme's muted text colour and replies its full foreground, in the desktop, web and phone chats.
    ("sessionChatMutedThinking", J::Bool(false)),
    // CDXC:SessionChat 2026-10-28 DECISION: User: "Let's please do this under feature flag if possible (toggle in settings) so i can test it. Keep current look as it is." Off by default; on, the chat draws tool calls as cards (packages/gx-chat-core/src/transcript/tool_card.rs).
    ("chatToolCallCards", J::Bool(false)),
    ("sessionChatKeepComposerExpanded", J::Bool(false)),
    ("sessionChatConfirmEscapeInterrupt", J::Bool(true)),
    // CDXC:CatchMeUp 2026-10-11 DECISION: User: "disable this feature by default. add a toggle to enable it under chat". The composer's Catch me up button shows only while this is on.
    ("chatCatchMeUp", J::Bool(false)),
    ("sessionDetailsClaudeTaskTools", J::Bool(true)),
    // CDXC:VoiceMode 2026-10-20 WHY:
    // xAI is the default provider: Grok Voice is billed per minute at a known price, and Grok dictation already needs the xAI key. Each provider keeps its own voice; the API keys are not settings (gxserver seals them, see server/src/voice_agent/keys.rs).
    ("voiceAgentProvider", J::Str("xai")),
    ("voiceAgentXaiVoice", J::Str("eve")),
    ("voiceAgentOpenaiVoice", J::Str("marin")),
    ("voiceAgentSpeed", J::Str("1")),
    ("voiceAgentCallMode", J::Str("regular")),
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
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: the chat look settings (Message layout, Your messages, Agent work, Status line in the chat) are opt-in, all Default; the user decisions are in packages/gx-chat-core/src/transcript/chat_look.rs.
    ("chatMessageLayout", J::Str("default")),
    ("chatUserMessageStyle", J::Str("default")),
    ("chatAgentWorkStyle", J::Str("default")),
    ("chatStatusInTranscript", J::Bool(false)),
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: Chat box matches transcript width is opt-in, off; the user decision is on the chat box in apps/desktop/src/app/native_chat/composer.rs.
    ("chatComposerMatchesTranscriptWidth", J::Bool(false)),
    // CDXC:SessionChat 2026-11-07 SEE-ALSO: Message spacing's user decision is in packages/gx-chat-core/src/transcript/chat_look.rs.
    ("chatMessageSpacing", J::Str("default")),
    // CDXC:Coordinators 2026-11-05 DECISION: the Board pill opens the side list until the user picks Columns (extras/coordinator_board.rs in gx-chat-core).
    ("coordinatorBoardView", J::Str("list")),
    // CDXC:Coordinators 2026-11-10 DECISION: the board's columns view collapses any column to its strip; Finished today starts collapsed (apps/desktop/src/app/native_chat/coordinator_board.rs `board_columns_body`).
    ("coordinatorBoardCollapsedColumns", J::Str("done")),
    // CDXC:AgentGuide 2026-11-02 DECISION: User (Q1066): the short Ghostex note goes to every agent session Ghostex starts, "on by default (with a setting to turn it off)".
    ("agentGhostexNote", J::Bool(true)),
    // CDXC:AgentGuide 2026-11-02 DECISION: User (#263): "if user asks for browser we ask which the user prefers (internal ghostex browser, or external browser)", and (Q1069) the answer is kept in this setting, which the note and `ghostex agent-guide` then state so agents stop asking. Ask until the user picks.
    ("agentPreviewBrowser", J::Str("ask")),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: "force the marked handovers for coordinators/orchestrator threads and their children by default and make it disabled by default for non-orchestrator/child threads". This switch covers every other session and stays off by default; threads mark regardless, orchestrators unless `orchestratorPlainReplies` is on.
    ("markedHandovers", J::Bool(false)),
    // CDXC:MarkedHandovers 2026-10-17 DECISION: User: "please allow launching an orchestrator without the reply style so the orchestrator answers normally. but put "Not recommended" and make it an advanced setting". Off by default, so orchestrators keep marking their handovers; their threads mark their reports either way.
    ("orchestratorPlainReplies", J::Bool(false)),
    // CDXC:Coordinators 2026-10-20 DECISION: User: "By default, we just show high and X high, but if he enables this, he can see all of the effort levels ... make the settings advanced." Off by default, so orchestrator effort pickers list only Low, Medium, High and XHigh since 2026-10-31 ("please allow setting low as the orchestrator effort level"; Medium joined on 2026-10-29 with "allow picking medium here but default to high"; `orchestrator_efforts` in packages/gx-chat-core/src/menus/orchestrator_effort.rs).
    ("orchestratorAllEfforts", J::Bool(false)),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: coloured labels (style D) by default, their first pick of the three.
    ("markedHandoversStyle", J::Str("labels")),
    // CDXC:MarkedHandovers 2026-10-11 DECISION: User: "add a toggle for emojis in the colorful handover please. and set it to off by default."
    ("markedHandoversEmoji", J::Bool(false)),
    // CDXC:MarkedHandovers 2026-10-20 DECISION: User: "i want an advanced setting to be able to set how strong the colors of these headings in the handover are" and "make current colors the max". 100% (the default) draws today's palette; lower blends each heading colour toward the normal text colour, 0% leaving plain bold text. Nothing goes more vivid than today.
    // CDXC:MarkedHandovers 2026-10-22 DECISION: User: "make handover color strength 60 by default". Supersedes the 100 default above; saved values keep what the user chose, and 100% is still the full palette.
    ("markedHandoversHeadingStrength", J::Num(60.0)),
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
    (
        "customSidebarTitlebarBackgroundTintColor",
        J::Str("#808080"),
    ),
    (
        "customSidebarTitlebarBackgroundDarknessPercent",
        J::Num(96.0),
    ),
    ("customSidebarTitlebarBackgroundColor", J::Str("#0b0b0b")),
    ("darkThemePreset", J::Str("gray")),
    ("lightThemePreset", J::Str("gray")),
    ("themeSidebarContrast", J::Num(0.0)),
    ("themeWorkAreaContrast", J::Num(0.0)),
    (
        "customSidebarTitlebarLightBackgroundTintColor",
        J::Str("#808080"),
    ),
    (
        "customSidebarTitlebarLightBackgroundLightnessPercent",
        J::Num(96.0),
    ),
    (
        "customSidebarTitlebarLightBackgroundColor",
        J::Str("#f4f4f5"),
    ),
    // CDXC:Terminal 2026-05-22-12:29:
    // New Ghostex terminals should default to the requested GitHub Dark terminal
    // profile: JetBrains Mono 13pt, bar cursor with blink, wght=300, 20% cell
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
    ("terminalGhosttyLightTheme", J::Str("GitHub Light Default")),
    ("terminalGhosttyTheme", J::Str("GitHub Dark")),
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
            ("toggleVoiceAgent", J::Str("ctrl+alt+v")),
            ("voiceCallAboutSession", J::Str("")),
            ("holdToTalk", J::Str("")),
            ("toggleDictation", J::Str("ctrl+alt+space")),
            ("createAgentSession", J::Str("cmd+shift+o")),
            ("createSession", J::Str("cmd+shift+t")),
            ("openCommandPalette", J::Str("cmd+shift+p")),
            ("openSessionSearchPalette", J::Str("cmd+p")),
            ("openProjectSearchPalette", J::Str("cmd+alt+shift+o")),
            ("openQuickAccessFiles", J::Str("cmd+o")),
            ("openNewThreadPalette", J::Str("cmd+n")),
            ("openCommandsPanel", J::Str("cmd+j")),
            ("openCommandsPanelSecondKey", J::Str("shift+escape")),
            ("openSettings", J::Str("cmd+,")),
            ("openExtensions", J::Str("")),
            ("openGhostexHelp", J::Str("")),
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
            ("scrollTerminalToTop", J::Str("")),
            ("scrollTerminalToBottom", J::Str("")),
            ("splitSessionRight", J::Str("alt+shift+d")),
            ("forkSession", J::Str("cmd+ctrl+shift+f")),
            ("reloadSession", J::Str("")),
            ("sleepFocusedSession", J::Str("cmd+shift+a")),
            ("wakeFocusedSession", J::Str("")),
            ("closeFocusedSession", J::Str("cmd+shift+backspace")),
            ("popOutPane", J::Str("ctrl+shift+o")),
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
    ("workspaceActivePaneBorderColor", J::Str("#3b82f6")),
    // CDXC:Workarea 2026-11-08 DECISION: User: "make frame around work area toggle enabled by default for new users please". On for a new install; a settings file that already exists keeps the look it has (off) because `keep_work_area_frame_off_for_existing_settings` writes `workAreaFrame` false into it once, at the first start of the version that flips this default. Supersedes 2026-11-05's off default (GitHub issue #407, helpers/work_area_frame.rs).
    ("workAreaFrame", J::Bool(true)),
    // CDXC:Settings 2026-11-08 DECISION: User: "to show traffic lights on macos when sidebar is collapsed (default to yes)". On: the window's buttons stay with the sidebar collapsed, as they did before the 2026-11-06 floating-sidebar change (GitHub #432), which off restores.
    ("showWindowButtonsWhenSidebarCollapsed", J::Bool(true)),
    ("closeButtonsOnRight", J::Bool(false)),
    ("windowGlass", J::Str("auto")),
    ("windowGlassSource", J::Str("desktopAndWindows")),
    ("windowGlassImagePlacement", J::Str("static")),
    ("windowGlassSameForLightAndDark", J::Bool(true)),
    ("windowGlassImageDark", J::Str("")),
    ("windowGlassImageLight", J::Str("")),
    ("windowGlassVideoDark", J::Str("")),
    ("windowGlassVideoLight", J::Str("")),
    ("windowGlassVideoOnlyOnPower", J::Bool(true)),
    ("windowGlassLiveStyleDark", J::Str("aurora")),
    ("windowGlassLiveStyleLight", J::Str("drift")),
    ("windowGlassLiveSpeed", J::Num(1.0)),
    ("windowGlassLiveBrightness", J::Num(60.0)),
    ("windowGlassBlurRadius", J::Num(60.0)),
    ("windowGlassWindowsBlur", J::Str("acrylic")),
    ("windowGlassMenuBlurRadius", J::Num(20.0)),
    // CDXC:Theming 2026-10-23 DECISION:
    // User: "please make the app's transparency level 10 by default (the slider in settings)" (2026-10-04), then "pls make the sidebar and main area default to having the same strength for the transparency by default". The four tints are the ones Settings' Strength slider writes at 10 (`transparencyStrengthPatch`), so the slider opens on 10, and the sidebar and the work area share one tint per appearance; they replace 94/91 (dark) and 97/93 (light), which kept the work area more see-through.
    // SEE-ALSO: apps/desktop/src/app/helpers/window_glass.rs, apps/desktop/src/app/window/onboarding/model.rs, apps/desktop/src/app/window/settings_modal/tabs/theme/colours.rs.
    ("windowGlassSidebarOpacityDark", J::Num(94.0)),
    ("windowGlassWorkAreaTintDark", J::Num(94.0)),
    ("windowGlassSidebarOpacityLight", J::Num(97.0)),
    ("windowGlassWorkAreaTintLight", J::Num(97.0)),
    ("terminalBackgroundMode", J::Str("pure")),
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
    // persisted field for settings compatibility, but normalize it to zero so
    // native panes always render without configurable spacing.
    ("workspacePaneGap", J::Num(0.0)),
    ("remoteMachines", J::Arr(&[])),
    // This computer's pill in the sidebar; empty uses one made from its name
    // (CDXC:RemoteMachines 2026-11-06 in gx-core machine_short_names.rs).
    ("localMachineShortName", J::Str("")),
    ("remoteTailscaleEnabled", J::Bool(true)),
    ("commandsPanelDefaultHeightPx", J::Num(125.0)),
    ("commandsPanelSide", J::Str("bottom")),
    ("commandsPanelAutoMinimize", J::Bool(true)),
    ("commandsPanelAutoMinimizeDelaySeconds", J::Num(60.0)),
];
