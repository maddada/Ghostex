use crate::json::J;

pub const GHOSTEX_OFFICIAL_EXTENSIONS: J = J::Arr(&[
    J::Obj(&[
        ("id", J::Str("linear")),
        ("title", J::Str("Linear")),
        ("description", J::Str("Open your team's issues and projects. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("linearViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("jira")),
        ("title", J::Str("Jira")),
        ("description", J::Str("Keep your team's board beside your work. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("jiraViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("github")),
        ("title", J::Str("GitHub")),
        ("description", J::Str("Open this project's GitHub repository automatically from its origin remote.")),
        ("settingsKey", J::Str("githubViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("sentry")),
        ("title", J::Str("Sentry")),
        ("description", J::Str("Investigate errors and stack traces beside your code. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("sentryViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("figma")),
        ("title", J::Str("Figma")),
        ("description", J::Str("Keep designs and component specs beside your implementation. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("figmaViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("vercel")),
        ("title", J::Str("Vercel")),
        ("description", J::Str("Check deployments and preview your changes. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("vercelViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("supabase")),
        ("title", J::Str("Supabase")),
        ("description", J::Str("Browse your database, authentication, and project logs. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("supabaseViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("github-actions")),
        ("title", J::Str("GitHub Actions")),
        ("description", J::Str("Follow workflow runs, build results, and job logs. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("githubActionsViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("posthog")),
        ("title", J::Str("PostHog")),
        ("description", J::Str("Explore product analytics and session replays. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("posthogViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("id", J::Str("custom-website")),
        ("title", J::Str("Custom Website")),
        ("description", J::Str("Keep any website beside your work. Choose a home URL for each project or worktree.")),
        ("settingsKey", J::Str("customWebsiteViewTabHidden")),
        ("placement", J::Str("view")),
        ("category", J::Str("project-websites")),
    ]),
    J::Obj(&[
        ("description", J::Str("Build, browse, and annotate your project’s components without a persistent development server. Appears only in projects with Storybook.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("storybook")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("storybookViewTabHidden")),
        ("title", J::Str("Storybook")),
    ]),
    J::Obj(&[
        ("description", J::Str("Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("code")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("codeViewTabHidden")),
        ("title", J::Str("VS Code")),
    ]),
    J::Obj(&[
        ("description", J::Str("Open websites alongside your project and keep useful pages organized without leaving Ghostex.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("browser")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("browserViewTabHidden")),
        ("title", J::Str("Browser")),
    ]),
    J::Obj(&[
        ("description", J::Str("Plan upcoming work and track task progress at a glance.")),
        ("category", J::Str("planning")),
        ("id", J::Str("kanban")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("kanbanViewTabHidden")),
        ("title", J::Str("Beads")),
    ]),
    J::Obj(&[
        ("description", J::Str("Turn repeatable project routines into simple workflows you can run whenever you need them.")),
        ("category", J::Str("planning")),
        ("id", J::Str("automate")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("automateViewTabHidden")),
        ("title", J::Str("Automate")),
    ]),
    J::Obj(&[
        ("appWide", J::Bool(true)),
        ("description", J::Str("Swap the sidebar to your Hermes agents: one row each, with a new session one click away. Needs the Hermes CLI.")),
        ("category", J::Str("planning")),
        ("id", J::Str("bots")),
        ("placement", J::Str("sidebar")),
        ("requiresAgentCli", J::Str("hermes-agent")),
        ("settingsKey", J::Str("botsHidden")),
        ("title", J::Str("Bots")),
    ]),
    J::Obj(&[
        ("appWide", J::Bool(true)),
        ("description", J::Str("A feed of every Hermes cron run, one channel per job, opened from the Bots sidebar. Needs Bots.")),
        ("category", J::Str("planning")),
        ("id", J::Str("botAutomations")),
        ("placement", J::Str("view")),
        ("requiresAgentCli", J::Str("hermes-agent")),
        ("requiresExtension", J::Str("bots")),
        ("settingsKey", J::Str("botAutomationsHidden")),
        ("title", J::Str("Bot automations")),
    ]),
    // CDXC:GBots 2026-11-08 DECISION:
    // User: native GBots show "under Chief of Staff in Main Hub"; the GBots switch stays and turns them on or off, and needs Main Hub on (`requiresExtension: chief`). Off by default under a key that enables (`gbotsEnabled`). Supersedes the 2026-11-06 entry, which put the previous GBots in the Bots space.
    J::Obj(&[
        ("appWide", J::Bool(true)),
        ("description", J::Str("Your own team of bots under Chief of Staff in Main Hub, each with its own instructions and memory, running on your Claude Code or Codex. Needs Main Hub.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("gbots")),
        ("placement", J::Str("feature")),
        ("requiresExtension", J::Str("chief")),
        ("settingsKey", J::Str("gbotsEnabled")),
        ("settingsKeyEnables", J::Bool(true)),
        ("title", J::Str("GBots")),
        // CDXC:GBots 2026-11-09 DECISION: User: "pls fully hide gbots if disabled in experimental under extensions". Its one row on another page leaves Settings and its search while GBots are off; Main Hub's entry lists it too, since GBots need Main Hub.
        ("settingKeys", J::Arr(&[J::Str("gbotsChiefAddsOnItsOwn")])),
    ]),
    J::Obj(&[
        ("description", J::Str("Browse your project’s notes, plans, and reference files together in one focused reading space.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("docs")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("docsViewTabHidden")),
        ("title", J::Str("Files")),
    ]),
    J::Obj(&[
        ("description", J::Str("Review what changed, comment on lines for your agent, revert a hunk, and commit, all beside your sessions.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("changes")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("changesViewTabHidden")),
        ("title", J::Str("Changes")),
    ]),
    J::Obj(&[
        ("description", J::Str("A command terminal beside your sessions, with its own tabs and splits, that works like the Commands pane but lives in the view panel.")),
        ("category", J::Str("code-and-files")),
        ("id", J::Str("terminal")),
        ("placement", J::Str("view")),
        ("settingsKey", J::Str("terminalViewTabHidden")),
        ("title", J::Str("Terminal")),
    ]),
    J::Obj(&[
        ("description", J::Str("A bell in the sidebar's top row that lists what your agents finished or need from you.")),
        ("category", J::Str("header-buttons")),
        ("id", J::Str("notifications")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("notificationsTitlebarButtonHidden")),
        ("title", J::Str("Notifications")),
    ]),
    J::Obj(&[
        ("appWide", J::Bool(true)),
        ("description", J::Str("An entry under Help in the sidebar's menu, with sample questions that start a Ghostex Help chat: an agent explains the app or changes settings for you.")),
        ("category", J::Str("menus-and-panels")),
        ("id", J::Str("help")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("helpTitlebarButtonHidden")),
        ("title", J::Str("Ghostex Help")),
    ]),
    J::Obj(&[
        ("description", J::Str("A panel listing development servers running on this computer, opened from Tools in the sidebar's menu.")),
        ("category", J::Str("menus-and-panels")),
        ("id", J::Str("devServers")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("devServersTitlebarButtonHidden")),
        ("title", J::Str("Dev servers")),
    ]),
    J::Obj(&[
        ("appWide", J::Bool(true)),
        ("description", J::Str("A panel listing what Ghostex is running right now, with the CPU and memory each part is using, opened from Tools in the sidebar's menu.")),
        ("category", J::Str("menus-and-panels")),
        ("id", J::Str("resources")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("resourcesTitlebarButtonHidden")),
        ("title", J::Str("Resources")),
    ]),
    J::Obj(&[
        ("description", J::Str("A work area header button for commit, branch, and worktree helpers on the active project.")),
        ("category", J::Str("header-buttons")),
        ("id", J::Str("gitActions")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("gitActionsTitlebarButtonHidden")),
        ("title", J::Str("Git actions")),
    ]),
    // CDXC:Extensions 2026-10-01 DECISION:
    // User: "I want Actions to be an extension, and if I disable it then it should hide it from the settings and hide the button at the top of the screen (disables this whole feature), default to off." The switch is a new key (`actionsHidden`, default on = hidden) rather than the old `quickActionsTitlebarButtonHidden`, because the settings file stores every default, so the old key's saved `false` cannot tell a user who turned Actions on from one who never touched it; everyone starts with Actions off and their saved actions are kept.
    J::Obj(&[
        ("description", J::Str("Saved terminal commands and web pages you start in one click from the Start button, a hotkey, Quick Access or a project row.")),
        ("category", J::Str("features")),
        ("id", J::Str("quickActions")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("actionsHidden")),
        ("title", J::Str("Actions")),
        ("settingsPages", J::Arr(&[J::Str("actions")])),
        ("hotkeys", J::Arr(&[J::Str("runActionSlot1"), J::Str("runActionSlot2"), J::Str("runActionSlot3"), J::Str("runActionSlot4"), J::Str("runActionSlot5")])),
    ]),
    // CDXC:Extensions 2026-10-01 DECISION:
    // User: "Same for 'Open in': when turned off we need to turn it off in settings, default to on." It keeps its old `openInTitlebarButtonHidden` key, so a user who had hidden the Open button keeps Open In off.
    J::Obj(&[
        ("description", J::Str("Open the active project or a session's folder in your editor, terminal or file manager from the Open button and the Open In menus.")),
        ("category", J::Str("features")),
        ("id", J::Str("openIn")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("openInTitlebarButtonHidden")),
        ("title", J::Str("Open In")),
        ("settingsPages", J::Arr(&[J::Str("openTargets")])),
    ]),
    // CDXC:Spaces 2026-11-10 DECISION: Spaces is no longer a built-in extension; its switch is a normal setting on Settings > Sidebar & Sessions > Spaces (spaces.rs).
    // CDXC:CloudBoxes 2026-11-02 DECISION:
    // User (D11 "on everywhere"): Cloud Boxes is on by default on every desktop platform, Windows included, because boxes run Linux. This supersedes the 2026-10-06 AgentBox decision ("keep it off by default and only show it on linux/macos"); the switch stays, so it can still be turned off. The settings file stores every default, so the saved `cloudBoxesHidden: true` would have kept it off: the switch moved to a new key, `cloudBoxesEnabled` (default on, `settingsKeyEnables`), and the old key is no longer read. The agentbox part (Settings > Cloud Boxes > Advanced) still needs macOS or Linux and says so on Windows. Saved boxes, provider logins and `agentboxDefaultLocation` are kept while it is off.
    J::Obj(&[
        ("description", J::Str("Run agent sessions on a Cloud Box: a Linux machine on your Instabox or Boat account, started from a new chat, with more providers through agentbox.")),
        ("category", J::Str("features")),
        ("id", J::Str("cloudBoxes")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("cloudBoxesEnabled")),
        ("settingsKeyEnables", J::Bool(true)),
        ("title", J::Str("Cloud Boxes")),
        ("settingsPages", J::Arr(&[J::Str("cloudBoxes")])),
    ]),
    // CDXC:Workspaces 2026-10-09 DECISION:
    // User: "Please make the workspaces feature as a built in extension in the app: when enabled we show the workspaces page in settings and we show the switcher and we allow enabling work mode etc." A whole-feature entry like Cloud Boxes, off by default, with a new key (`workspacesHidden`, default on = hidden) because the settings file stores every default. Off hides the workspace tile, Move to workspace and the Workspaces page; gxserver publishes no workspaces, runs no team sync and applies no workspace Claude account, so every window shows every project as before workspaces. Saved workspaces, Linear keys and team connections are kept, so turning it on restores everything. Its "we allow enabling work mode" part is superseded by the user's "Yes, no Workspaces needed" (CDXC:WorkMode 2026-10-24 below).
    // CDXC:WorkMode 2026-10-24 DECISION: User: Issues Workflow (formerly Work mode) is "On everywhere once set up", and asked whether it should still need the Workspaces extension: "Yes, no Workspaces needed". This replaces the 2026-10-09 "we allow enabling work mode" above: Link to, Create ticket, the Issues view and branch titles work with Workspaces off. Workspaces keeps what is per company: its Linear key and primary tracker (a project outside one uses the Personal workspace's), the team backend and Slack.
    // SEE-ALSO: server/src/workspaces/feature.rs (gxserver's gate), packages/gx-core/src/sidebar_view/inputs.rs (`workspaces_enabled`).
    J::Obj(&[
        ("description", J::Str("Keep each company's projects, Spaces, browser sign-ins, Linear key and Claude account in its own workspace, pick each one's primary tracker, and share a team backend for your team's Slack flow.")),
        ("category", J::Str("features")),
        ("id", J::Str("workspaces")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("workspacesHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Workspaces")),
        ("settingsPages", J::Arr(&[J::Str("workspaces")])),
    ]),
    // CDXC:VoiceMode 2026-10-25 DECISION:
    // User: "voice agent should be an extension so ppl can disable it's button and not see it top of the app if they want". This supersedes the 2026-10-20 decision that the voice agent is built in and not an extension (pages/voice_agent.rs). A whole-feature entry like Read aloud, on by default with a new key (`voiceAgentHidden`, default false, like `connectorsHidden`) and not behind Enable Experimental Features, so nothing changes for anyone who has not touched it. Off hides the header's headset and its arrow menu, the voice call rows and the three hotkeys, ends a call that is running, and gxserver refuses new calls (server/src/voice_agent/feature.rs); the phone follows through `builtInFeaturesOff`. Saved voice settings and the remembered conversation are kept.
    // CDXC:VoiceMode 2026-11-08 DECISION: User: Voice becomes two pages, "Dictation" and "Voice Agent" (CDXC:VoiceMode 2026-11-08 in pages/dictation.rs). The 2026-10-25 option A ("It stays in Settings with only the xAI key and Dictation history") and Q795 ("show the OpenAI key too") are kept by the Dictation page, which shows the key its engine needs and the Dictation history whether this is on or off, so the Voice Agent page is this feature's `settingsPages` and leaves the rail while it is off, with its key rows. Supersedes the 2026-10-30 version, where the shared page stayed for the keys.
    // SEE-ALSO: server/src/voice_agent/feature.rs (gxserver's gate), apps/desktop/src/app/voice_agent/conversation.rs (`voice_agent_on`), apps/mobile/app/src/voice/featureSwitch.ts, docs/2026-10-20/voice-mode/voice-agent-client-contract.md.
    J::Obj(&[
        ("description", J::Str("Talk with a voice agent about your agent sessions: the headset at the top of the window starts a call, and its arrow starts one about a single session, here and in the phone app. Dictate in the chat box is separate and keeps working when this is off.")),
        ("category", J::Str("features")),
        ("id", J::Str("voiceAgent")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("voiceAgentHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Voice agent")),
        ("settingKeys", J::Arr(&[J::Str("voiceAgentProvider"), J::Str("voiceAgentXaiKey"), J::Str("voiceAgentOpenaiKey"), J::Str("voiceAgentXaiVoice"), J::Str("voiceAgentOpenaiVoice"), J::Str("voiceAgentSpeed"), J::Str("voiceAgentCallMode"), J::Str("voiceAgentWaitForReply"), J::Str("voiceAgentMemory")])),
        ("settingsPages", J::Arr(&[J::Str("voiceAgent")])),
        ("hotkeys", J::Arr(&[J::Str("toggleVoiceAgent"), J::Str("voiceCallAboutSession"), J::Str("holdToTalk")])),
    ]),
    // CDXC:Chief 2026-11-08 DECISION:
    // User: "please disable chief of staff for now and make it an extension so ppl aren't confused by it until i finish working on it (call it beta in settings pls)", then "i mean the Main Hub thingy". One whole-feature entry, Main Hub, with a Beta badge, off by default (`mainHubHidden`, default true, user-only so no agent can switch it): off, the home project is an ordinary project (gxserver publishes no `isHome` or `isChief`, and hides Chief of Staff's chats), Chief of Staff's agent sleeps and loses its head role, the home folder's secret-folder list is not applied, and nothing says Main Hub or Chief of Staff; on, all of it comes back with its chat and memory. Supersedes Chief's own entry (Q1734-Q1750, on by default).
    // SEE-ALSO: server/src/chief/ (the session, the head role, `chief_on`), packages/settings-catalog/src/data/defaults.rs (`mainHubHidden`), packages/settings-catalog/src/help/supplemental.rs.
    J::Obj(&[
        ("description", J::Str("The home for chats that don't belong to a project, with Chief of Staff, an agent that can see and steer all your other agents, pinned at the top.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("chief")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("mainHubHidden")),
        ("appWide", J::Bool(true)),
        ("badge", J::Str("Beta")),
        ("title", J::Str("Main Hub")),
        ("settingKeys", J::Arr(&[J::Str("chiefShowOnAllSpaces"), J::Str("chiefConfirmNewChat"), J::Str("homeSecretFolders"), J::Str("gbotsChiefAddsOnItsOwn")])),
    ]),
    // CDXC:VoiceMode 2026-10-21 DECISION:
    // User: Read aloud ships as a Ghostex extension, disabled by default (2026-10-20), and (Q23) as a built-in extension, not a Store extension: a Store extension cannot reach the chat composer, a message's actions or the phone. A whole-feature entry like Workspaces, with a new key (`readAloudHidden`, default on = hidden) because the settings file stores every default; off hides the reply action, the ⋯ menu rows, the Voice and Speed rows and the two hotkeys, and keeps their saved values. No `platforms` list: the switch also governs the phone and Ghostex in a browser that talk to this computer, so it exists on Linux too, where the desktop draws no controls (Q25; `readAloudUnavailable` says why).
    // SEE-ALSO: packages/gx-chat-core/src/read_aloud/ (the rules), server/src/ghostex_cli/sessions/mobile_summary.rs (`builtInFeatures`, how the phone learns the switch), docs/2026-10-20/voice-mode/read-aloud-client-contract.md.
    J::Obj(&[
        ("description", J::Str("Hear the agent's replies spoken aloud: Read aloud on a reply, and Auto-read in the chat's ⋯ menu, which speaks each finished reply of the chat you're looking at. Uses this device's own voices.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("readAloud")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("readAloudHidden")),
        // CDXC:VoiceMode 2026-10-22 DECISION: User: "it's ok to bring read aloud work into this release just make it experimental for now if not finished yet (show experimental features makes it appear in the extensions where if enabled then it actually work)". Gated by Enable Experimental Features like Connectors: its card, its Chat rows and its hotkeys show only while that is on, and the phone hears it is on (`builtInFeatures`) only then.
        ("appWide", J::Bool(true)),
        ("title", J::Str("Read aloud")),
        ("settingKeys", J::Arr(&[J::Str("readAloudVoice"), J::Str("readAloudRate"), J::Str("readAloudUnavailable")])),
        ("hotkeys", J::Arr(&[J::Str("readAloudLastReply"), J::Str("readAloudStop")])),
    ]),
    // CDXC:SessionDetails 2026-10-25 DECISION:
    // User: "please make the Session detail panel extension enabled by default". This supersedes Q18's "so i enable it and test it but others dont get it": the panel stays a whole-feature entry like Read aloud with its own key (`sessionDetailsPanelHidden`, now default false), still marked beta in its description as Q18 asked, and on for everyone who has not turned it off. Off hides the header's context ring, the panel and the Claude task list row, and gives Claude sessions no task-list variable. Not behind Enable Experimental Features.
    // SEE-ALSO: server/src/session_details/ (the data and the gate), packages/gx-chat-core/src/extras/session_details/, apps/desktop/src/app/native_chat/session_details/.
    J::Obj(&[
        ("description", J::Str("Beta. A panel beside each Claude chat, opened from the context ring at the top right of the window: where the session runs, how full its context is with Claude's own breakdown, its connectors, skills and memory files, its changes, background commands and task list.")),
        ("category", J::Str("features")),
        ("id", J::Str("sessionDetailsPanel")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("sessionDetailsPanelHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Session details panel")),
        ("settingKeys", J::Arr(&[J::Str("sessionDetailsClaudeTaskTools")])),
    ]),
    // CDXC:SessionNotes 2026-10-30 DECISION:
    // User: "please make session notes a built in extension in the extensions page so i can toggle it on and off and if it's off then hide it from the sessions context menu and from the more actions menu in the chat composer and everywhere (should be disabled by default)". A whole-feature entry with a new key (`sessionNotesHidden`, default on = hidden), so it starts off for new and existing users. Off removes the Note row of a session's menu and its hover button, the note dot and the note line in the card's tooltip, the Session note row of the chat's and the terminal bar's More actions, the note dialog, the Session Note hotkey and its Quick Access row, and `ghostex session-note`; the phone follows through `builtInFeaturesOff`. Saved notes are kept and come back when it is turned on.
    // SEE-ALSO: packages/gx-core/src/sidebar_view/inputs.rs (`session_notes_enabled`), apps/desktop/src/app/session_notes.rs, apps/gpui-web/src/app/session_notes.rs, apps/mobile/app/src/sessions/sessionNotesSwitch.ts.
    J::Obj(&[
        ("description", J::Str("Attach a note to an agent conversation, such as what to do next when you come back to it: Note in a session's menu, Session note in the chat's More actions, and a dot on the session's card while it has one. Notes you wrote are kept while this is off.")),
        ("category", J::Str("features")),
        ("id", J::Str("sessionNotes")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("sessionNotesHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Session notes")),
        ("hotkeys", J::Arr(&[J::Str("sessionNote")])),
    ]),
    // CDXC:Extensions 2026-10-25 DECISION:
    // User: claude.ai inside Ghostex "has to be built in extension", not an external Ghostex extension from the Store. Enabled, it adds a sidebar button that opens a global page, with "different claude profiles for each account" to switch between. The 2026-10-28 decision in apps/desktop/src/app/claude_web/takeover.rs supersedes "switches to a global page that just keeps just my sidebar shown": turning Claude on now hides the sidebar too. A whole-feature entry with a new key (`claudeWebHidden`, default on = hidden, so it starts off like Actions) because the settings file stores every default. Off removes the sidebar button and the Claude view; each account's claude.ai sign-in is kept.
    // SEE-ALSO: apps/desktop/src/app/claude_web/ (the view, the button and the account bar).
    J::Obj(&[
        ("description", J::Str("Use claude.ai inside Ghostex from a button in the header, with one sign-in per saved Claude account and a bar above the page to switch between them.")),
        ("category", J::Str("features")),
        ("id", J::Str("claudeWeb")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("claudeWebHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Claude")),
    ]),
    // CDXC:Extensions 2026-11-09 DECISION:
    // User: "let's please add an extension to the app like the claude one that opens different claude accounts but this one is to open different grok bot logins". Built like Claude: its own sidebar button beside Claude's, the page in the sessions column's place with the same takeover (apps/desktop/src/app/claude_web/takeover.rs), and a bar above the page to switch logins. Ghostex has no saved Grok accounts, so the logins are its own (`grokWebLogins`), each a named browser profile with its own cookies; the bar adds one, Settings > Files & Links > Browser renames and removes them. The start page is one setting for every login (`grokWebStartPage`, grok.com by default; the coordinator, for the user: "that url is mine but it should open grok.com usually"), as Claude has no per-account settings. A whole-feature entry with a new key (`grokWebHidden`, default on = hidden, so it starts off like Claude). Off removes the button, the page and the two rows; every login's sign-in is kept.
    // SEE-ALSO: apps/desktop/src/app/claude_web/grok.rs (the logins and the bar), apps/desktop/src/app/claude_web/place.rs (Claude or Grok).
    J::Obj(&[
        ("description", J::Str("Use grok.com inside Ghostex from a button in the header, with several Grok logins kept apart, each with its own sign-in, and a bar above the page to switch between them or add another.")),
        ("category", J::Str("features")),
        ("id", J::Str("grokWeb")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("grokWebHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Grok Bot")),
        ("settingKeys", J::Arr(&[J::Str("grokWebStartPage"), J::Str("grokWebLogins")])),
    ]),
    // CDXC:Extensions 2026-10-19 DECISION:
    // User (Q306): Connectors, OS Integration and Debugging join the Settings rail's Extensions group, and "for the ones that aren't in extensions already add them to extensions and allow disabling/enabling them". Q313: "Switch on top of the gate": each is on by default and its page shows only while its switch is on and its gate (`gatedBy`) is open: Enable Experimental Features for Connectors and OS Integration. Debugging has no gate (CDXC:Settings 2026-10-29 in rail.rs: Show Advanced never hides a page), only its switch. Its Store card is left out while the gate is closed. Turning Connectors off also turns off `ghostex connectors` and its gxserver routes (server/src/connectors/gate.rs, whose CDXC:Connectors 2026-10-15 decision still holds). New `…Hidden` keys default to false, so nobody's Settings changes.
    J::Obj(&[
        ("description", J::Str("Connect Gmail, Slack, Linear and more apps to a workspace so its agents can use them, from the Connectors page and the ghostex connectors commands.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("connectors")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("connectorsHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Connectors")),
        ("settingsPages", J::Arr(&[J::Str("connectors")])),
    ]),
    J::Obj(&[
        ("description", J::Str("Make Ghostex your default editor, terminal-link handler and script runner, and check how this computer opens its files and links.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("osIntegration")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("osIntegrationHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("OS Integration")),
        ("settingsPages", J::Arr(&[J::Str("osIntegration")])),
    ]),
    J::Obj(&[
        ("description", J::Str("The Debugging page in Settings: debug UI controls and the diagnostic logs you turn on while reproducing an issue.")),
        ("category", J::Str("features")),
        ("id", J::Str("debugging")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("debuggingHidden")),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Debugging")),
        ("settingsPages", J::Arr(&[J::Str("debugging")])),
    ]),
    J::Obj(&[
        ("description", J::Str("An entry under Tools in the sidebar's menu that lists your installed extensions.")),
        ("category", J::Str("menus-and-panels")),
        ("id", J::Str("extensionsButton")),
        ("placement", J::Str("titlebar-button")),
        ("settingsKey", J::Str("extensionsTitlebarButtonHidden")),
        ("title", J::Str("Extensions")),
    ]),
    // CDXC:Extensions 2026-11-09 DECISION:
    // User, on the Extensions tabs page (Q2043): "remove the 'Experimental features' toggle itself
    // and move toggling each of those individually here instead". Enable Experimental Features
    // (`showBetaFeatures`) is gone; everything it turned on is its own switch in the Experimental
    // tab, off by default. Keep Awake (the ⋯ menu entry, the top bar control and Settings >
    // General > Power), All Automations (projectless automations, a coming-soon overlay while
    // off) and Terminal shaders (macOS) are new entries; Read aloud, Connectors and OS
    // Integration lost their gate. Someone who had the old switch on keeps what it showed:
    // `migrate_experimental_features_switch` in lib.rs.
    J::Obj(&[
        ("description", J::Str("Keep this computer awake for a while from the sidebar's menu or the button at the top of the window, with its own settings under Settings > General > Power.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("keepAwake")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("keepAwakeEnabled")),
        ("settingsKeyEnables", J::Bool(true)),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Keep Awake")),
        ("platforms", J::Arr(&[J::Str("macos")])),
        ("settingKeys", J::Arr(&[J::Str("hideKeepAwakeTitlebarControl"), J::Str("keepAwakeDefaultDurationMinutes"), J::Str("keepAwakeActivation"), J::Str("keepAwakeDeactivation"), J::Str("keepAwakeAllowDisplaySleep"), J::Str("keepAwakePreventLidSleep"), J::Str("keepAwakeActivateOnLaunch"), J::Str("keepAwakeActivateOnExternalDisplay"), J::Str("keepAwakeWhileWorkingSessions"), J::Str("keepAwakeBatteryThresholdPercent"), J::Str("keepAwakeDeactivateOnLowPowerMode")])),
    ]),
    J::Obj(&[
        ("description", J::Str("Automations that belong to no project, in Automate's All Automations board. While this is off the board shows that it is coming soon.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("allAutomations")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("allAutomationsEnabled")),
        ("settingsKeyEnables", J::Bool(true)),
        ("appWide", J::Bool(true)),
        ("title", J::Str("All Automations")),
    ]),
    J::Obj(&[
        ("description", J::Str("Apply the shaders from Ghostex's terminal config in order. Metal only; tested on Apple Silicon, not yet on Intel Macs. Turn off to restore ordinary rendering in the same sessions.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("terminalShaders")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("terminalShadersEnabled")),
        ("settingsKeyEnables", J::Bool(true)),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Terminal shaders")),
        ("platforms", J::Arr(&[J::Str("macos")])),
    ]),
    // CDXC:Extensions 2026-11-09 DECISION: User (Q2000 C on the Faster rendering page): "keep the
    // toggle but move this to the extensions -> experimental (since we're taking away the
    // experimental toggle in about page)". Its key, value and default (off) are unchanged; the
    // About page's Advanced section, which held only it, is gone.
    J::Obj(&[
        ("description", J::Str("Redraw only the parts of a window that changed, so Ghostex uses less CPU while agents stream and you scroll. Turn it off if part of a window stops updating.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("fasterRendering")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("fasterRendering")),
        ("settingsKeyEnables", J::Bool(true)),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Faster rendering")),
    ]),
    // CDXC:SessionChat 2026-11-09 DECISION: User, after picking design C for the tool calls ("one
    // quiet line per run, rows inside"): "let's keep the current way how things work as is ... still
    // keep the new behavior under a toggle please". Off by default; the tool rows stay the look
    // (gx-chat-core `transcript/tool_card.rs`).
    J::Obj(&[
        ("description", J::Str("Draw each run of an agent's tool calls as one quiet line that says what it is doing or did, with the calls as rows under it. Detailed view opens the lines, Simple keeps them shut. Off keeps the tool rows. Settings > Chat > Agent work set to Reduced or Quiet replaces these lines.")),
        ("category", J::Str("experimental")),
        ("id", J::Str("toolCallLines")),
        ("placement", J::Str("feature")),
        ("settingsKey", J::Str("chatToolCallLines")),
        ("settingsKeyEnables", J::Bool(true)),
        ("appWide", J::Bool(true)),
        ("title", J::Str("Tool call lines")),
    ]),
]);

pub const GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES: J = J::Arr(&[
    J::Obj(&[
        // CDXC:Extensions 2026-11-09 DECISION: User (Q2041): "features is bad category, too
        // general, wtf is features". These are the parts of Ghostex itself you can switch off;
        // with the page in tabs, Built-in sits beside Views, Panels & menus, Installed and
        // Community, which say what they are or where they come from.
        ("id", J::Str("features")),
        ("label", J::Str("Built-in")),
        ("typeLabel", J::Str("Feature")),
    ]),
    J::Obj(&[
        ("id", J::Str("project-websites")),
        ("label", J::Str("Project websites")),
        ("typeLabel", J::Str("View")),
    ]),
    J::Obj(&[
        ("id", J::Str("code-and-files")),
        ("label", J::Str("Code and files")),
        ("typeLabel", J::Str("View")),
    ]),
    J::Obj(&[
        ("id", J::Str("planning")),
        ("label", J::Str("Planning and automation")),
        ("typeLabel", J::Str("View")),
    ]),
    J::Obj(&[
        ("id", J::Str("header-buttons")),
        ("label", J::Str("Header buttons")),
        ("typeLabel", J::Str("Header button")),
    ]),
    J::Obj(&[
        ("id", J::Str("menus-and-panels")),
        ("label", J::Str("Menus and panels")),
        ("typeLabel", J::Str("Menu item")),
    ]),
    // Last, at the right of the tabs (User, Q2041: "Experimental should be the last one on the
    // right"). CDXC:Extensions 2026-11-09 DECISION: User (Q2038 on the Features list page): the features
    // shown only while Experimental features is on get their own group, "move gbots there too
    // please and main hub for now".
    J::Obj(&[
        ("id", J::Str("experimental")),
        ("label", J::Str("Experimental")),
        ("typeLabel", J::Str("Feature")),
    ]),
]);
