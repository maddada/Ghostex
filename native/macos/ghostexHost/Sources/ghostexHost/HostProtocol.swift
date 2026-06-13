import Foundation

enum HostCommand: Decodable {
  case createTerminal(CreateTerminal)
  case createWebPane(CreateWebPane)
  case openFloatingEditor(OpenFloatingEditor)
  case closeTerminal(SessionCommand)
  case closeWebPane(SessionCommand)
  case focusTerminal(SessionCommand)
  case focusProjectEditorCompanionSession(SessionCommand)
  case focusWebPane(SessionCommand)
  case reloadWebPane(SessionCommand)
  case startT3CodeRuntime(StartT3CodeRuntime)
  case setT3CodeRuntimeSessionState(SetT3CodeRuntimeSessionState)
  case stopT3CodeRuntime
  case startCodeServerRuntime(StartCodeServerRuntime)
  case stopCodeServerRuntime
  case createProjectEditorPane(CreateProjectEditorPane)
  case focusProjectEditorPane(ProjectEditorCommand)
  case closeProjectEditorPane(ProjectEditorCommand)
  case activateApp
  case writeTerminalText(WriteTerminalText)
  case writeTerminalScript(WriteTerminalScript)
  case sendTerminalEnter(SessionCommand)
  case readTerminalText(ReadTerminalText)
  case checkPersistenceSession(CheckPersistenceSession)
  case setActiveTerminalSet(SetActiveTerminalSet)
  case setSessionPaneChrome(SetSessionPaneChrome)
  case setSessionStatusIndicators(SetSessionStatusIndicators)
  case setPetOverlayState(SetPetOverlayState)
  case showSessionAttentionNotification(ShowSessionAttentionNotification)
  case setTerminalLayout(SetTerminalLayout)
  case setTerminalVisibility(SetTerminalVisibility)
  case pickWorkspaceFolder
  case pickWorkspaceIcon(PickWorkspaceIcon)
  case showMessage(ShowMessage)
  case appendAgentDetectionDebugLog(AppendAgentDetectionDebugLog)
  case appendLayoutLayeringDebugLog(AppendLayoutLayeringDebugLog)
  case appendProjectBoardDebugLog(AppendProjectBoardDebugLog)
  case appendTerminalFocusDebugLog(AppendTerminalFocusDebugLog)
  case appendRestoreDebugLog(AppendRestoreDebugLog)
  case appendSessionTitleDebugLog(AppendSessionTitleDebugLog)
  case appendSidebarCollapseStateDebugLog(AppendSidebarCollapseStateDebugLog)
  case appendSidebarRefreshDebugLog(AppendSidebarRefreshDebugLog)
  case appendWorkspaceDockIndicatorDebugLog(AppendWorkspaceDockIndicatorDebugLog)
  case persistSharedSidebarStorage(PersistSharedSidebarStorage)
  case projectBoardResponse(ProjectBoardResponse)
  case playSound(PlaySound)
  case runProcess(RunProcess)
  case cancelRunProcess(CancelRunProcess)
  case gxserverRequest(GxserverRequest)
  case remoteGxserverConnect(RemoteGxserverConnect)
  case remoteGxserverRequest(RemoteGxserverRequest)
  case remoteGxserverSubscribePresentation(RemoteGxserverPresentationSubscribe)
  case remoteSshPasswordSave(RemoteSshPasswordSave)
  case setKeepAwakeLidSleepPrevention(SetKeepAwakeLidSleepPrevention)
  case syncGhosttyTerminalSettings(SyncGhosttyTerminalSettings)
  case applyGhosttyConfigSettings(ApplyGhosttyConfigSettings)
  case openGhosttyConfigFile
  case openAccessibilityPreferences
  case requestMacOSNotificationPermission
  case openMacOSNotificationSettings
  case setOSIntegrationDefaults(SetOSIntegrationDefaults)
  case requestOSIntegrationStatus
  case openExternalUrl(OpenExternalUrl)
  case openWorkspaceInFinder(OpenWorkspaceInFinder)
  case openWorkspaceInIde(OpenWorkspaceInIde)
  case openBrowserDevTools(SessionCommand)
  case injectBrowserReactGrab(SessionCommand)
  case injectBrowserAgentation(SessionCommand)
  case showBrowserProfilePicker(SessionCommand)
  case showBrowserImportSettings(SessionCommand)
  case setSidebarSide(SetSidebarSide)
  case toggleSidebarCollapsed
  case setReactTitlebarHitRegions(SetReactTitlebarHitRegions)
  case showTitlebarDropdownPanel(ShowTitlebarDropdownPanel)
  case closeTitlebarDropdownPanel
  case resizeTitlebarDropdownPanel(ResizeTitlebarDropdownPanel)
  case titlebarDropdownPanelReady(TitlebarDropdownPanelReady)
  case openActiveProjectEditorFromTitlebar
  case exitFocusModeFromTitlebar
  case openAgentsModeFromTitlebar
  case openGitHubProjectFromTitlebar
  case toggleProjectEditorCompanionFromTitlebar
  case openTasksPlaceholderFromTitlebar
  case refreshWorkspaceOpenTargetAvailabilityFromTitlebar
  case rotateActivePaneLayoutClockwiseFromTitlebar
  case togglePetOverlayFromTitlebar
  case toggleCommandsPanelFromTitlebar
  case showUpdateDialogFromTitlebar
  case startGxserverFromTitlebar
  case stopGxserverFromTitlebar
  case restartGxserverFromTitlebar
  case setGxserverAlwaysStartFromTitlebar(SetGxserverAlwaysStartFromTitlebar)
  case focusResourceSessionFromTitlebar(FocusResourceSessionFromTitlebar)
  case sleepInactiveSessionsFromTitlebar(SleepInactiveSessionsFromTitlebar)
  case quitResourcesFromTitlebar(QuitResourcesFromTitlebar)
  case runSidebarCommandFromTitlebar(RunSidebarCommandFromTitlebar)
  case runSidebarGitActionFromTitlebar(RunSidebarGitActionFromTitlebar)
  case sidebarCliCommand(SidebarCliCommand)
  case sidebarContextMenuOpened
  case sidebarContextMenuClosed

  private enum CodingKeys: String, CodingKey {
    case type
  }

  private enum CommandType: String, Decodable {
    case createTerminal
    case createWebPane
    case openFloatingEditor
    case closeTerminal
    case closeWebPane
    case focusTerminal
    case focusProjectEditorCompanionSession
    case focusWebPane
    case reloadWebPane
    case startT3CodeRuntime
    case setT3CodeRuntimeSessionState
    case stopT3CodeRuntime
    case startCodeServerRuntime
    case stopCodeServerRuntime
    case createProjectEditorPane
    case focusProjectEditorPane
    case closeProjectEditorPane
    case activateApp
    case writeTerminalText
    case writeTerminalScript
    case sendTerminalEnter
    case readTerminalText
    case checkPersistenceSession
    case setActiveTerminalSet
    case setSessionPaneChrome
    case setSessionStatusIndicators
    case setPetOverlayState
    case showSessionAttentionNotification
    case setTerminalLayout
    case setTerminalVisibility
    case pickWorkspaceFolder
    case pickWorkspaceIcon
    case showMessage
    case appendAgentDetectionDebugLog
    case appendLayoutLayeringDebugLog
    case appendProjectBoardDebugLog
    case appendTerminalFocusDebugLog
    case appendRestoreDebugLog
    case appendSessionTitleDebugLog
    case appendSidebarCollapseStateDebugLog
    case appendSidebarRefreshDebugLog
    case appendWorkspaceDockIndicatorDebugLog
    case persistSharedSidebarStorage
    case projectBoardResponse
    case playSound
    case runProcess
    case cancelRunProcess
    case gxserverRequest
    case remoteGxserverConnect
    case remoteGxserverRequest
    case remoteGxserverSubscribePresentation
    case remoteSshPasswordSave
    case setKeepAwakeLidSleepPrevention
    case syncGhosttyTerminalSettings
    case applyGhosttyConfigSettings
    case openGhosttyConfigFile
    case openAccessibilityPreferences
    case requestMacOSNotificationPermission
    case openMacOSNotificationSettings
    case setOSIntegrationDefaults
    case requestOSIntegrationStatus
    case openExternalUrl
    case openWorkspaceInFinder
    case openWorkspaceInIde
    case openBrowserDevTools
    case injectBrowserReactGrab
    case injectBrowserAgentation
    case showBrowserProfilePicker
    case showBrowserImportSettings
    case setSidebarSide
    case toggleSidebarCollapsed
    case setReactTitlebarHitRegions
    case showTitlebarDropdownPanel
    case closeTitlebarDropdownPanel
    case resizeTitlebarDropdownPanel
    case titlebarDropdownPanelReady
    case openActiveProjectEditorFromTitlebar
    case exitFocusModeFromTitlebar
    case openAgentsModeFromTitlebar
    case openGitHubProjectFromTitlebar
    case toggleProjectEditorCompanionFromTitlebar
    case openTasksPlaceholderFromTitlebar
    case refreshWorkspaceOpenTargetAvailabilityFromTitlebar
    case rotateActivePaneLayoutClockwiseFromTitlebar
    case togglePetOverlayFromTitlebar
    case toggleCommandsPanelFromTitlebar
    case showUpdateDialogFromTitlebar
    case startGxserverFromTitlebar
    case stopGxserverFromTitlebar
    case restartGxserverFromTitlebar
    case setGxserverAlwaysStartFromTitlebar
    case focusResourceSessionFromTitlebar
    case sleepInactiveSessionsFromTitlebar
    case quitResourcesFromTitlebar
    case runSidebarCommandFromTitlebar
    case runSidebarGitActionFromTitlebar
    case sidebarCliCommand
    case sidebarContextMenuOpened
    case sidebarContextMenuClosed
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    switch try container.decode(CommandType.self, forKey: .type) {
    case .createTerminal:
      self = .createTerminal(try CreateTerminal(from: decoder))
    case .createWebPane:
      self = .createWebPane(try CreateWebPane(from: decoder))
    case .openFloatingEditor:
      self = .openFloatingEditor(try OpenFloatingEditor(from: decoder))
    case .closeTerminal:
      self = .closeTerminal(try SessionCommand(from: decoder))
    case .closeWebPane:
      self = .closeWebPane(try SessionCommand(from: decoder))
    case .focusTerminal:
      self = .focusTerminal(try SessionCommand(from: decoder))
    case .focusProjectEditorCompanionSession:
      self = .focusProjectEditorCompanionSession(try SessionCommand(from: decoder))
    case .focusWebPane:
      self = .focusWebPane(try SessionCommand(from: decoder))
    case .reloadWebPane:
      self = .reloadWebPane(try SessionCommand(from: decoder))
    case .startT3CodeRuntime:
      self = .startT3CodeRuntime(try StartT3CodeRuntime(from: decoder))
    case .setT3CodeRuntimeSessionState:
      self = .setT3CodeRuntimeSessionState(try SetT3CodeRuntimeSessionState(from: decoder))
    case .stopT3CodeRuntime:
      self = .stopT3CodeRuntime
    case .startCodeServerRuntime:
      self = .startCodeServerRuntime(try StartCodeServerRuntime(from: decoder))
    case .stopCodeServerRuntime:
      self = .stopCodeServerRuntime
    case .createProjectEditorPane:
      self = .createProjectEditorPane(try CreateProjectEditorPane(from: decoder))
    case .focusProjectEditorPane:
      self = .focusProjectEditorPane(try ProjectEditorCommand(from: decoder))
    case .closeProjectEditorPane:
      self = .closeProjectEditorPane(try ProjectEditorCommand(from: decoder))
    case .activateApp:
      self = .activateApp
    case .writeTerminalText:
      self = .writeTerminalText(try WriteTerminalText(from: decoder))
    case .writeTerminalScript:
      self = .writeTerminalScript(try WriteTerminalScript(from: decoder))
    case .sendTerminalEnter:
      self = .sendTerminalEnter(try SessionCommand(from: decoder))
    case .readTerminalText:
      self = .readTerminalText(try ReadTerminalText(from: decoder))
    case .checkPersistenceSession:
      self = .checkPersistenceSession(try CheckPersistenceSession(from: decoder))
    case .setActiveTerminalSet:
      self = .setActiveTerminalSet(try SetActiveTerminalSet(from: decoder))
    case .setSessionPaneChrome:
      self = .setSessionPaneChrome(try SetSessionPaneChrome(from: decoder))
    case .setSessionStatusIndicators:
      self = .setSessionStatusIndicators(try SetSessionStatusIndicators(from: decoder))
    case .setPetOverlayState:
      self = .setPetOverlayState(try SetPetOverlayState(from: decoder))
    case .showSessionAttentionNotification:
      self = .showSessionAttentionNotification(try ShowSessionAttentionNotification(from: decoder))
    case .setTerminalLayout:
      self = .setTerminalLayout(try SetTerminalLayout(from: decoder))
    case .setTerminalVisibility:
      self = .setTerminalVisibility(try SetTerminalVisibility(from: decoder))
    case .pickWorkspaceFolder:
      self = .pickWorkspaceFolder
    case .pickWorkspaceIcon:
      self = .pickWorkspaceIcon(try PickWorkspaceIcon(from: decoder))
    case .showMessage:
      self = .showMessage(try ShowMessage(from: decoder))
    case .appendAgentDetectionDebugLog:
      self = .appendAgentDetectionDebugLog(try AppendAgentDetectionDebugLog(from: decoder))
    case .appendLayoutLayeringDebugLog:
      self = .appendLayoutLayeringDebugLog(try AppendLayoutLayeringDebugLog(from: decoder))
    case .appendProjectBoardDebugLog:
      self = .appendProjectBoardDebugLog(try AppendProjectBoardDebugLog(from: decoder))
    case .appendTerminalFocusDebugLog:
      self = .appendTerminalFocusDebugLog(try AppendTerminalFocusDebugLog(from: decoder))
    case .appendRestoreDebugLog:
      self = .appendRestoreDebugLog(try AppendRestoreDebugLog(from: decoder))
    case .appendSessionTitleDebugLog:
      self = .appendSessionTitleDebugLog(try AppendSessionTitleDebugLog(from: decoder))
    case .appendSidebarCollapseStateDebugLog:
      self = .appendSidebarCollapseStateDebugLog(
        try AppendSidebarCollapseStateDebugLog(from: decoder))
    case .appendSidebarRefreshDebugLog:
      self = .appendSidebarRefreshDebugLog(try AppendSidebarRefreshDebugLog(from: decoder))
    case .appendWorkspaceDockIndicatorDebugLog:
      self = .appendWorkspaceDockIndicatorDebugLog(
        try AppendWorkspaceDockIndicatorDebugLog(from: decoder))
    case .persistSharedSidebarStorage:
      self = .persistSharedSidebarStorage(try PersistSharedSidebarStorage(from: decoder))
    case .projectBoardResponse:
      self = .projectBoardResponse(try ProjectBoardResponse(from: decoder))
    case .playSound:
      self = .playSound(try PlaySound(from: decoder))
    case .runProcess:
      self = .runProcess(try RunProcess(from: decoder))
    case .cancelRunProcess:
      self = .cancelRunProcess(try CancelRunProcess(from: decoder))
    case .gxserverRequest:
      self = .gxserverRequest(try GxserverRequest(from: decoder))
    case .remoteGxserverConnect:
      self = .remoteGxserverConnect(try RemoteGxserverConnect(from: decoder))
    case .remoteGxserverRequest:
      self = .remoteGxserverRequest(try RemoteGxserverRequest(from: decoder))
    case .remoteGxserverSubscribePresentation:
      self = .remoteGxserverSubscribePresentation(try RemoteGxserverPresentationSubscribe(from: decoder))
    case .remoteSshPasswordSave:
      self = .remoteSshPasswordSave(try RemoteSshPasswordSave(from: decoder))
    case .setKeepAwakeLidSleepPrevention:
      self = .setKeepAwakeLidSleepPrevention(try SetKeepAwakeLidSleepPrevention(from: decoder))
    case .syncGhosttyTerminalSettings:
      self = .syncGhosttyTerminalSettings(try SyncGhosttyTerminalSettings(from: decoder))
    case .applyGhosttyConfigSettings:
      self = .applyGhosttyConfigSettings(try ApplyGhosttyConfigSettings(from: decoder))
    case .openGhosttyConfigFile:
      self = .openGhosttyConfigFile
    case .openAccessibilityPreferences:
      self = .openAccessibilityPreferences
    case .requestMacOSNotificationPermission:
      self = .requestMacOSNotificationPermission
    case .openMacOSNotificationSettings:
      self = .openMacOSNotificationSettings
    case .setOSIntegrationDefaults:
      self = .setOSIntegrationDefaults(try SetOSIntegrationDefaults(from: decoder))
    case .requestOSIntegrationStatus:
      self = .requestOSIntegrationStatus
    case .openExternalUrl:
      self = .openExternalUrl(try OpenExternalUrl(from: decoder))
    case .openWorkspaceInFinder:
      self = .openWorkspaceInFinder(try OpenWorkspaceInFinder(from: decoder))
    case .openWorkspaceInIde:
      self = .openWorkspaceInIde(try OpenWorkspaceInIde(from: decoder))
    case .openBrowserDevTools:
      self = .openBrowserDevTools(try SessionCommand(from: decoder))
    case .injectBrowserReactGrab:
      self = .injectBrowserReactGrab(try SessionCommand(from: decoder))
    case .injectBrowserAgentation:
      self = .injectBrowserAgentation(try SessionCommand(from: decoder))
    case .showBrowserProfilePicker:
      self = .showBrowserProfilePicker(try SessionCommand(from: decoder))
    case .showBrowserImportSettings:
      self = .showBrowserImportSettings(try SessionCommand(from: decoder))
    case .setSidebarSide:
      self = .setSidebarSide(try SetSidebarSide(from: decoder))
    case .toggleSidebarCollapsed:
      self = .toggleSidebarCollapsed
    case .setReactTitlebarHitRegions:
      self = .setReactTitlebarHitRegions(try SetReactTitlebarHitRegions(from: decoder))
    case .showTitlebarDropdownPanel:
      self = .showTitlebarDropdownPanel(try ShowTitlebarDropdownPanel(from: decoder))
    case .closeTitlebarDropdownPanel:
      self = .closeTitlebarDropdownPanel
    case .resizeTitlebarDropdownPanel:
      self = .resizeTitlebarDropdownPanel(try ResizeTitlebarDropdownPanel(from: decoder))
    case .titlebarDropdownPanelReady:
      self = .titlebarDropdownPanelReady(try TitlebarDropdownPanelReady(from: decoder))
    case .openActiveProjectEditorFromTitlebar:
      self = .openActiveProjectEditorFromTitlebar
    case .exitFocusModeFromTitlebar:
      self = .exitFocusModeFromTitlebar
    case .openAgentsModeFromTitlebar:
      self = .openAgentsModeFromTitlebar
    case .openGitHubProjectFromTitlebar:
      self = .openGitHubProjectFromTitlebar
    case .toggleProjectEditorCompanionFromTitlebar:
      self = .toggleProjectEditorCompanionFromTitlebar
    case .openTasksPlaceholderFromTitlebar:
      self = .openTasksPlaceholderFromTitlebar
    case .refreshWorkspaceOpenTargetAvailabilityFromTitlebar:
      self = .refreshWorkspaceOpenTargetAvailabilityFromTitlebar
    case .rotateActivePaneLayoutClockwiseFromTitlebar:
      self = .rotateActivePaneLayoutClockwiseFromTitlebar
    case .togglePetOverlayFromTitlebar:
      self = .togglePetOverlayFromTitlebar
    case .toggleCommandsPanelFromTitlebar:
      self = .toggleCommandsPanelFromTitlebar
    case .showUpdateDialogFromTitlebar:
      self = .showUpdateDialogFromTitlebar
    case .startGxserverFromTitlebar:
      self = .startGxserverFromTitlebar
    case .stopGxserverFromTitlebar:
      self = .stopGxserverFromTitlebar
    case .restartGxserverFromTitlebar:
      self = .restartGxserverFromTitlebar
    case .setGxserverAlwaysStartFromTitlebar:
      self = .setGxserverAlwaysStartFromTitlebar(try SetGxserverAlwaysStartFromTitlebar(from: decoder))
    case .focusResourceSessionFromTitlebar:
      self = .focusResourceSessionFromTitlebar(try FocusResourceSessionFromTitlebar(from: decoder))
    case .sleepInactiveSessionsFromTitlebar:
      self = .sleepInactiveSessionsFromTitlebar(try SleepInactiveSessionsFromTitlebar(from: decoder))
    case .quitResourcesFromTitlebar:
      self = .quitResourcesFromTitlebar(try QuitResourcesFromTitlebar(from: decoder))
    case .runSidebarCommandFromTitlebar:
      self = .runSidebarCommandFromTitlebar(try RunSidebarCommandFromTitlebar(from: decoder))
    case .runSidebarGitActionFromTitlebar:
      self = .runSidebarGitActionFromTitlebar(try RunSidebarGitActionFromTitlebar(from: decoder))
    case .sidebarCliCommand:
      self = .sidebarCliCommand(try SidebarCliCommand(from: decoder))
    case .sidebarContextMenuOpened:
      self = .sidebarContextMenuOpened
    case .sidebarContextMenuClosed:
      self = .sidebarContextMenuClosed
    }
  }
}

struct CreateTerminal: Decodable {
  let activateOnCreate: Bool?
  let cwd: String
  let diagnosticSource: String?
  let env: [String: String]?
  let initialInput: String?
  let persistenceSessionCreated: Bool?
  let sessionId: String
  let sessionPersistenceName: String?
  let sessionPersistenceProvider: String?
  let shellAttachCommand: String?
  let shellCommand: String?
  let title: String?
  let tmuxMode: Bool?
  let tmuxSessionName: String?
}

struct CreateWebPane: Decodable {
  let browserFeedbackTool: String?
  let cwd: String?
  let projectId: String?
  let sessionId: String
  let threadId: String?
  let title: String
  let url: String
}

struct OpenFloatingEditor: Decodable {
  let command: String?
  let cwd: String?
  let editorKind: String?
  let env: [String: String]?
  let filePath: String?
  let language: String?
  let originatingSessionId: String?
  let requestId: String?
  let statusFile: String?
  let title: String?
}

struct SessionCommand: Decodable {
  let preservePersistenceSession: Bool?
  let sessionId: String
}

struct ProjectBoardResponse: Decodable {
  let payloadJson: String
  let projectId: String?
  let requestId: String
}

struct ProjectBoardBridgeRequest: Decodable {
  let action: String
  let agentId: String?
  let beadDisplayId: String?
  let beadId: String?
  let details: String?
  let event: String?
  let projectEditorId: String?
  let prompt: String?
  let projectId: String?
  let projectPath: String?
  let remoteMachineId: String?
  let payloadJson: String?
  let requestId: String
  let sessionId: String?
  let startLocation: String?
  let ticketTitle: String?
}

struct StartT3CodeRuntime: Decodable {
  let cwd: String
}

struct SetT3CodeRuntimeSessionState: Decodable {
  /**
   CDXC:T3Code 2026-06-06-05:13:
   This sidebar-projected state is protocol compatibility only. The managed
   t3code provider lifetime follows live native managed T3 web panes instead,
   because gxserver presentation can omit local T3 cards while the AppKit pane
   registry still owns an open embedded tab.
   */
  let runtimeCwd: String?
  let runningSessionIds: [String]
}

struct StartCodeServerRuntime: Decodable {
  /**
   CDXC:EditorPanes 2026-05-06-14:21
   The sidebar opens project editors by starting one shared code-server runtime,
   then binding project-specific Chromium surfaces to folder URLs. Keep this
   separate from terminal/web-pane commands because editors are not split panes.

   CDXC:EditorPanes 2026-05-06-15:00
   The sidebar sends VS Code settings-link choices with the runtime command so
   the native launcher can pass code-server CLI flags before the process starts.

   CDXC:EditorPanes 2026-06-06-23:50:
   VS Code server startup failures must return to the sidebar immediately as a
   project-scoped error and toast instead of waiting for the generic open timer.
   */
  let cwd: String
  let linkVscodeUserConfig: Bool?
  let projectId: String?
  let vscodeUserConfigDir: String?
}

struct ProjectEditorBrowserTabState: Codable {
  let id: String
  let title: String
  let url: String
}

struct CreateProjectEditorPane: Decodable {
  let activeBrowserTabId: String?
  let browserTabs: [ProjectEditorBrowserTabState]?
  let browserFeedbackTool: String?
  let companionPaneHidden: Bool?
  let mode: String?
  let projectId: String
  let projectTitle: String?
  let showsBrowserToolbar: Bool?
  let showsProjectTabs: Bool?
  let title: String
  let url: String
}

struct ProjectEditorCommand: Decodable {
  let projectId: String
}

struct WriteTerminalText: Decodable {
  let sessionId: String
  let text: String
}

struct WriteTerminalScript: Decodable {
  let sessionId: String
  let text: String
}

struct ReadTerminalText: Decodable {
  let requestId: String
  let sessionId: String
  let source: String?
}

struct CheckPersistenceSession: Decodable {
  let provider: String
  let requestId: String
  let sessionName: String
}

struct SetActiveTerminalSet: Decodable {
  let activeProjectEditorId: String?
  let activeProjectDiffStats: TitlebarProjectDiffStats?
  let activeProjectGitState: TitlebarGitState?
  let activeProjectMode: String?
  let activeProjectEditorCompanionPaneHidden: Bool?
  let activeProjectEditorIsOpen: Bool?
  let activeProjectEditorIsSleeping: Bool?
  let activeProjectEditorStatus: String?
  let activeProjectId: String?
  let activeProjectIconDataUrl: String?
  let activeProjectIsQuick: Bool?
  let activeProjectName: String?
  let activeProjectPath: String?
  let activeSessionIds: [String]
  /**
   CDXC:NativeWindowChrome 2026-05-10-14:19
   The sidebar owns active project/chat state, so it sends the native app title
   with layout sync. AppKit uses this for the outer window title bar while pane
   title bars continue to read per-session titles.
   */
  let appTitle: String?
  let attentionSessionIds: [String]?
  let backgroundColor: String?
  let commandsPanelActiveSessionIds: [String]?
  let commandsPanelFocusedSessionId: String?
  let commandsPanelHeightRatio: Double?
  let commandsPanelDefaultHeightPx: Double?
  let commandsPanelIsVisible: Bool?
  let commandsPanelLayout: NativeTerminalLayout?
  let commandsPanelMode: String?
  let clickToWakeSleepingSessions: Bool?
  let debuggingMode: Bool?
  let focusRequestId: Int?
  let focusedSessionId: String?
  let isFocusModeActive: Bool?
  /**
   CDXC:SessionFocusMode 2026-05-28-12:52:
   Native tab context menus need explicit Focus availability from the sidebar layout model because a single pane can contain multiple tabs without having any split pane to zoom.

   CDXC:SessionFocusMode 2026-05-28-15:35:
   Availability follows rendered awake pane owners, so a persisted split whose other pane is sleeping does not leave Focus visible while AppKit shows one pane.
   */
  let sessionFocusModeAvailableSessionIds: [String]?
  let sleepingSessionIds: [String]?
  let layoutChanged: Bool?
  let paneOwnerSelectionChanged: Bool?
  let layout: NativeTerminalLayout?
  let keepAwake: TitlebarKeepAwakeSettings?
  let gxserverDaemon: TitlebarGxserverDaemon?
  let paneGap: Double?
  let petOverlayEnabled: Bool?
  /**
   CDXC:PanePopOut 2026-05-11-09:35
   The sidebar keeps popped-out sessions in the active layout and sends this
   set so AppKit can move the live pane into a ghostex-owned window while the
   original split/tab slot renders an in-app reattach placeholder.
   */
  let poppedOutSessionIds: [String]?
  let sessionAgentIconColors: [String: String]?
  let sessionAgentIconDataUrls: [String: String]?
  let sessionActivities: [String: NativeTerminalActivity]?
  /**
   CDXC:DelayedSend 2026-05-17-03:14
   AppKit tab strips and terminal-pane overlays need the same Delayed Send
   countdown labels as the React sidebar because native Ghostty surfaces sit
   outside the webview tree.
   */
  let sessionDelayedSendRemainingLabels: [String: String]?
  /**
   CDXC:SessionTitleSync 2026-05-30-05:44:
   First-prompt title generation blocks terminal input and shows an AppKit
   overlay because native Ghostty panes are outside the React DOM.
   */
  let sessionFirstPromptTitleGenerationSessionIds: [String]?
  let sessionFaviconDataUrls: [String: String]?
  let sessionTitleBarActions: [String: [TerminalTitleBarAction]]?
  let sessionTitles: [String: String]?
  /**
   CDXC:PaneTabs 2026-06-04-20:36:
   The native pane-tab moon marks an inactive zmx provider session, not a
   missing AppKit terminal renderer.
   */
  let sessionZmxInactiveIds: [String]?
  /**
   CDXC:SessionPersistence 2026-05-23-00:50:
   The sidebar owns the preference for showing provider/session ids. Decode it
   with layout sync because AppKit owns the top-right overlay view and still
   checks per-pane persistence metadata before rendering text.
   */
  let showSessionIdInTerminalPanes: Bool?
  let showProjectEditorDiffFileCount: Bool?
  let sidebarActions: TitlebarSidebarActions?
  let agentHookStatus: TitlebarAgentHookStatus?
  let ghostexCliStatus: TitlebarGhostexCliStatus?
  let sessionPersistenceProvider: String?
  let titlebarResourceGroups: [TitlebarResourceGroup]?
  let workspaceOpenTargets: TitlebarWorkspaceOpenTargets?
}

struct SetSessionPaneChrome: Decodable {
  /**
   CDXC:SessionAttentionFocus 2026-05-29-19:14:
   Attention and working transitions are status-only chrome updates. Keep pane
   metadata outside setActiveTerminalSet so a session entering attention never
   enters the broad layout/focus sync path.
   */
  let attentionSessionIds: [String]?
  let sessionAgentIconColors: [String: String]?
  let sessionAgentIconDataUrls: [String: String]?
  let sessionActivities: [String: NativeTerminalActivity]?
  let sessionDelayedSendRemainingLabels: [String: String]?
  let sessionFaviconDataUrls: [String: String]?
  let sessionFirstPromptTitleGenerationSessionIds: [String]?
  let sessionTitleBarActions: [String: [TerminalTitleBarAction]]?
  let sessionTitles: [String: String]?
  /**
   CDXC:PaneTabs 2026-06-04-20:36:
   ZMX liveness is pane chrome, so provider inactivity can repaint tab moons
   without rebuilding the native split/tab layout.
   */
  let sessionZmxInactiveIds: [String]?
  let showSessionIdInTerminalPanes: Bool?
}

struct TitlebarResourceGroup: Decodable {
  let groupId: String
  let isActive: Bool
  let projectId: String?
  let projectName: String
  let projectPath: String
  let sessions: [TitlebarResourceSession]
  let title: String
}

struct TitlebarAgentHookStatusItem: Decodable {
  let agentId: String
  let cliCommand: String
  let cliInstalled: Bool
  let detail: String
  let hookInstalled: Bool
  let paths: [String]
  let status: String
}

struct TitlebarAgentHookStatus: Decodable {
  let agents: [TitlebarAgentHookStatusItem]
  let errorMessage: String?
  let generatedAt: String
  let hookStateDirectory: String
  let notifyHookPath: String
  let type: String
}

struct TitlebarGhostexCliStatus: Decodable {
  let generatedAt: String
  let gxUsable: Bool
  let installed: Bool
  let type: String
}

struct TitlebarKeepAwakeSettings: Decodable {
  let activateOnExternalDisplay: Bool
  let activateOnLaunch: Bool
  let allowDisplaySleep: Bool
  let batteryThresholdPercent: Double
  let deactivateBelowBatteryThreshold: Bool
  let deactivateOnLowPowerMode: Bool
  let deactivateOnUserSwitch: Bool
  let defaultDurationMinutes: Int
  let preventLidSleep: Bool
}

struct TitlebarGxserverDaemon: Decodable {
  let alwaysStart: Bool?
  let message: String?
  let nodePath: String?
  let nodeVersion: String?
  let ok: Bool?
  let pid: Int?
  let startedAt: String?
  let state: String
  let version: String?
}

struct TitlebarResourceSession: Decodable {
  let activity: String
  let agentIcon: String?
  let delayedSendDeadlineAt: String?
  let delayedSendRemainingLabel: String?
  let delayedSendRemainingMs: Double?
  let isRunning: Bool
  let isSleeping: Bool?
  let lastInteractionAt: String?
  let projectId: String?
  let sessionId: String
  let sessionKind: String?
  let sessionPersistenceName: String?
  let sessionPersistenceProvider: String?
  let terminalTitle: String?
  let title: String
}

struct TitlebarProjectDiffStats: Decodable {
  let additions: Int
  let deletions: Int
  let files: Int
  let isLoading: Bool
  let isRepo: Bool
}

struct TitlebarGitChangedFile: Decodable {
  let additions: Int
  let deletions: Int
  let path: String
}

struct TitlebarGitPullRequest: Decodable {
  let number: Int?
  let state: String
  let title: String
  let url: String
}

struct TitlebarGitState: Decodable {
  let additions: Int
  let aheadCount: Int
  let behindCount: Int
  let branch: String?
  let confirmSuggestedCommit: Bool
  let deletions: Int
  let files: [TitlebarGitChangedFile]
  let generateCommitBody: Bool
  let hasCheckedGitHubRemote: Bool
  let hasGitHubCli: Bool
  let hasGitHubRemote: Bool
  let hasOriginRemote: Bool
  let hasUpstream: Bool
  let hasWorkingTreeChanges: Bool
  let isBusy: Bool
  let isRepo: Bool
  let isWorktree: Bool
  let pr: TitlebarGitPullRequest?
  let primaryAction: String
  let worktreeName: String?
}

struct TitlebarWorkspaceOpenTargets: Decodable {
  let availability: TitlebarWorkspaceOpenTargetAvailability?
  let customTargets: [TitlebarCustomWorkspaceOpenTarget]?
  let hiddenTargetIds: [String]?
}

struct TitlebarWorkspaceOpenTargetAvailability: Decodable {
  let availableTargetIds: [String]?
  let checkedAtMs: Double?
  let resolvedAppNames: [String: String]?
  let resolvedCommands: [String: String]?
}

struct TitlebarSidebarActions: Decodable {
  let commands: [TitlebarSidebarCommand]?
}

struct TitlebarSidebarCommand: Decodable {
  let actionType: String
  let closeTerminalOnExit: Bool?
  let command: String?
  let commandId: String
  let icon: String?
  let iconColor: String?
  let isDefault: Bool?
  let name: String
  let playCompletionSound: Bool?
  let url: String?
}

struct TitlebarCustomWorkspaceOpenTarget: Decodable {
  let args: [String]?
  let command: String
  let id: String
  let label: String
}

struct SetSessionStatusIndicators: Decodable {
  let attentionCount: Int
  let workingCount: Int
  let availableCount: Int
  let hideFloatingIndicators: Bool
  let hideMenuBarIndicators: Bool
  let size: NativeSessionStatusIndicatorSize
}

struct SetPetOverlayState: Codable {
  let activities: [PetOverlayActivity]
  let enabled: Bool
  let selectedPetId: String
  let statusItems: [PetOverlayStatusItem]
}

struct PetOverlayActivity: Codable {
  let id: String
  let projectId: String
  let state: PetOverlayActivityState
  let title: String
}

enum PetOverlayActivityState: String, Codable {
  case attention
  case available
  case working
}

struct PetOverlayStatusItem: Codable {
  let count: Int
  let status: NativeSessionStatusIndicatorStatus
}

struct ShowSessionAttentionNotification: Decodable {
  /**
   CDXC:SessionAttentionNotifications 2026-05-10-16:46
   The sidebar posts already-rate-limited attention notification requests with
   the native pane id. The native host must preserve that id in userInfo so a
   click can route back to the same session and activate it for typing.
  */
  let body: String?
  let iconDataUrl: String?
  let sessionId: String
  let title: String
}

enum NativeSessionStatusIndicatorStatus: String, Codable {
  case attention
  case working
  case available
}

enum NativeSessionStatusIndicatorSize: String, Decodable {
  case small
  case medium
  case large
  case xLarge = "x-large"
}

struct SetTerminalLayout: Decodable {
  let layout: NativeTerminalLayout
}

enum NativeTerminalActivity: String, Decodable {
  case attention
  case sleeping
  case working
}

struct SetTerminalVisibility: Decodable {
  let sessionId: String
  let visible: Bool
}

struct PickWorkspaceIcon: Decodable {
  let projectId: String
}

struct ShowMessage: Decodable {
  let level: MessageLevel
  let message: String
}

struct AppendAgentDetectionDebugLog: Decodable {
  let details: String?
  let event: String
}

struct AppendTerminalFocusDebugLog: Decodable {
  let details: String?
  let event: String
  let force: Bool?
}

struct AppendLayoutLayeringDebugLog: Decodable {
  let details: String?
  let event: String
  let force: Bool?
}

struct AppendProjectBoardDebugLog: Decodable {
  let details: String?
  let event: String
}

struct AppendSessionTitleDebugLog: Decodable {
  let details: String?
  let event: String
  let force: Bool?
}

struct AppendSidebarCollapseStateDebugLog: Decodable {
  let details: String?
  let event: String
}

struct AppendRestoreDebugLog: Decodable {
  let details: String?
  let event: String
}

struct AppendSidebarRefreshDebugLog: Decodable {
  let details: String?
  let event: String
}

struct AppendWorkspaceDockIndicatorDebugLog: Decodable {
  let details: String?
  let event: String
}

struct PersistSharedSidebarStorage: Decodable {
  let key: SharedSidebarStorageKey
  let payloadJson: String
}

enum SharedSidebarStorageKey: String, Decodable {
  /**
   CDXC:ProjectSidebarOwnership 2026-06-02-15:04:
   After the gxserver/native ownership cutoff, Swift accepts shared-sidebar
   persistence only for settings. Projects, worktrees, sessions, and previous
   sessions are gxserver-owned and must not decode as valid native shared
   storage commands.
   */
  case settings
}

struct PlaySound: Decodable {
  let fileName: String
  let volume: Double?
}

enum MessageLevel: String, Decodable {
  case info
  case warning
  case error
}

struct RunProcess: Decodable {
  let args: [String]
  let cwd: String?
  let env: [String: String]?
  let executable: String
  let requestId: String
}

struct CancelRunProcess: Decodable {
  let requestId: String
}

struct GxserverRequest: Decodable {
  let method: String
  let paramsJson: String?
  let path: String
  let requestId: String
}

struct RemoteGxserverConnect: Decodable {
  let identityFile: String?
  let installApproved: Bool?
  let remoteMachineId: String
  let remoteMachineName: String
  let requestId: String
  let sshHost: String
  let sshPort: Int?
  let sshUser: String?
}

struct RemoteSshPasswordSave: Decodable {
  let password: String
  let remoteMachineId: String
  let requestId: String
}

struct RemoteGxserverRequest: Decodable {
  let method: String
  let paramsJson: String?
  let path: String
  let remoteMachineId: String
  let requestId: String
}

struct RemoteGxserverPresentationSubscribe: Decodable {
  let clientId: String?
  let lastRevision: Int?
  let remoteMachineId: String
  let requestId: String
}

struct SetKeepAwakeLidSleepPrevention: Decodable {
  let enabled: Bool
  let installIfNeeded: Bool?
  let requestId: String
}

struct SyncGhosttyTerminalSettings: Decodable {
  let adjustCellHeightPercent: Double
  let adjustCellWidth: Double
  let cursorStyle: String
  let fontFamily: String
  let fontSize: Double
  let fontVariationWeight: Int?
  let clipboardPasteProtection: Bool
  let clipboardTrimTrailingSpaces: Bool
  let pastePreviewableImages: Bool?
  let confirmCloseSurface: String
  let copyOnSelect: String
  let cursorStyleBlink: Bool
  let ghosttyTheme: String
  let mouseHideWhileTyping: Bool
  let mouseScrollMultiplierDiscrete: Double
  let mouseScrollMultiplierPrecision: Double
  let reloadImmediately: Bool?
  let runtimeOnly: Bool?
  let scrollbackLimitBytes: Int
  let scrollbar: String
}

struct ApplyGhosttyConfigSettings: Decodable {
  let lines: [String]
  let managedKeys: [String]
  let reloadImmediately: Bool?
}

struct OpenExternalUrl: Decodable {
  let url: String
}

struct OpenWorkspaceInFinder: Decodable {
  /**
   CDXC:WorkspaceActions 2026-05-04-08:22
   Project context-menu open commands cross the WKWebView/AppKit bridge with
   only the trusted native-sidebar workspace path needed by Finder.
   */
  let workspacePath: String
}

struct OpenWorkspaceInIde: Decodable {
  /**
   CDXC:WorkspaceActions 2026-05-27-07:24
   Opening a project in an IDE carries the explicit target app from the command.
   It no longer depends on the removed IDE attachment settings or overlay controller.
   */
  let targetApp: WorkspaceIdeTargetApp
  let workspacePath: String
}

struct SetSidebarSide: Decodable {
  let side: SidebarSide
}

struct ReactTitlebarHitRegion: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

struct SetReactTitlebarHitRegions: Decodable {
  /**
   CDXC:ReactTitlebar 2026-05-09-17:11
   React titlebar chrome must expose only its real interactive bounds to
   AppKit. Native hit-testing uses these regions to keep blank titlebar space
   draggable and workspace content clickable while React buttons/dropdowns own
   their own events.

   CDXC:ReactTitlebar 2026-05-25-10:09:
   Workspace shielding follows React's explicit dropdown/menu open state, not
   stale hit-region geometry. Regions still route visible titlebar overlay
   clicks, but they are not the source of truth for blocking terminals.
   */
  let overlayOpen: Bool
  let regions: [ReactTitlebarHitRegion]

  private enum CodingKeys: String, CodingKey {
    case overlayOpen
    case regions
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    overlayOpen = try container.decodeIfPresent(Bool.self, forKey: .overlayOpen) ?? false
    regions = try container.decode([ReactTitlebarHitRegion].self, forKey: .regions)
  }
}

struct TitlebarDropdownAnchorRect: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

struct TitlebarDropdownPreferredSize: Decodable {
  /*
   CDXC:ReactTitlebar 2026-06-12-02:50:
   React owns the rendered option count for titlebar dropdowns, so the native
   bridge accepts a preferred child-window size before AppKit creates the panel.
   */
  let width: Double
  let height: Double
}

struct ShowTitlebarDropdownPanel: Decodable {
  /*
   CDXC:ReactTitlebar 2026-06-11-13:22:
   Titlebar dropdowns must render in native child windows instead of portaled
   content inside the full-window titlebar WKWebView, so AppKit never places a
   titlebar-owned view over the editor workspace during CEF/WKWebView drags.
   */
  let kind: String
  let anchorRect: TitlebarDropdownAnchorRect
  let preferredSize: TitlebarDropdownPreferredSize?
}

struct ResizeTitlebarDropdownPanel: Decodable {
  let kind: String
  let width: Double
  let height: Double
}

struct TitlebarDropdownPanelReady: Decodable {
  /*
   CDXC:TitlebarResources 2026-06-11-18:13:
   The Resources dropdown loads in a native child WKWebView, but it should not be
   ordered onscreen until its first resource snapshot has rendered. React sends
   this readiness signal after committing non-loading content.
   */
  let kind: String
}

struct RunSidebarCommandFromTitlebar: Decodable {
  let commandId: String
}

struct RunSidebarGitActionFromTitlebar: Decodable {
  let action: String
}

struct FocusResourceSessionFromTitlebar: Decodable {
  let sessionId: String
}

struct SleepInactiveSessionsFromTitlebar: Decodable {
  let sessionIds: [String]
}

struct SetGxserverAlwaysStartFromTitlebar: Decodable {
  let enabled: Bool
}

struct QuitResourcesFromTitlebar: Decodable {
  let projectIds: [String]
  let sessionIds: [String]
}

enum SidebarSide: String, Decodable {
  case left
  case right
}

enum WorkspaceIdeTargetApp: String, Decodable {
  case zed
  case zedPreview = "zed-preview"
  case vscode
  case vscodeInsiders = "vscode-insiders"
}

struct SetOSIntegrationDefaults: Decodable {
  let target: String
}

struct SidebarCliCommand: Decodable {
  let action: String
  let payloadJson: String?
  let requestId: String
}

enum NativeTerminalLayout: Decodable {
  case leaf(sessionId: String)
  case split(direction: SplitDirection, ratio: Double?, children: [NativeTerminalLayout])
  case tabs(activeSessionId: String?, sessionIds: [String])

  enum SplitDirection: String, Decodable {
    case horizontal
    case vertical
  }

  private enum CodingKeys: String, CodingKey {
    case children
    case direction
    case kind
    case ratio
    case activeSessionId
    case sessionId
    case sessionIds
  }

  private enum Kind: String, Decodable {
    case leaf
    case split
    case tabs
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    switch try container.decode(Kind.self, forKey: .kind) {
    case .leaf:
      self = .leaf(sessionId: try container.decode(String.self, forKey: .sessionId))
    case .split:
      self = .split(
        direction: try container.decode(SplitDirection.self, forKey: .direction),
        ratio: try container.decodeIfPresent(Double.self, forKey: .ratio),
        children: try container.decode([NativeTerminalLayout].self, forKey: .children)
      )
    case .tabs:
      self = .tabs(
        activeSessionId: try container.decodeIfPresent(String.self, forKey: .activeSessionId),
        sessionIds: try container.decode([String].self, forKey: .sessionIds)
      )
    }
  }
}

enum HostEvent: Encodable {
  case hostReady
  case appShotCaptured(
    appName: String?, bundleIdentifier: String?, imagePath: String, text: String?, title: String?,
    trigger: String)
  case appShotCaptureFailed(message: String)
  case nativeHotkey(actionId: String)
  case terminalReady(
    sessionId: String, ttyName: String?, foregroundPid: Int?, sessionPersistenceName: String?,
    persistenceSessionCreated: Bool?)
  case terminalTitleChanged(sessionId: String, title: String, sessionPersistenceName: String?)
  case browserFaviconChanged(sessionId: String, faviconDataUrl: String?)
  case browserUrlChanged(sessionId: String, url: String)
  case browserOpenInNewTabRequested(sourceSessionId: String, url: String)
  case terminalTitleBarAction(sessionId: String, action: TerminalTitleBarAction)
  case paneReorderRequested(sourceSessionId: String, targetSessionId: String, placement: PaneDropPlacement?)
  case paneTabSelected(sessionId: String)
  case sleepingPaneWakeRequested(sessionId: String)
  case paneTabFocusRequested(sessionId: String)
  case paneTabReorderRequested(
    sourceSessionId: String, targetSessionId: String, position: PaneTabReorderPosition)
  case paneTabCloseRequested(sessionId: String, scope: PaneTabCloseScope)
  case paneTabSleepRequested(sessionId: String, scope: PaneTabSleepScope)
  case terminalCwdChanged(sessionId: String, cwd: String)
  case terminalExited(sessionId: String, exitCode: Int?, text: String?)
  case terminalFocused(sessionId: String)
  case terminalEscapePressed(sessionId: String)
  case terminalBell(sessionId: String)
  case firstPromptAutoRenameCancelled(sessionId: String)
  case nativeSessionSurfaceMissing(sessionId: String)
  case terminalRestoreBlocked(sessionId: String, reason: String, cwd: String)
  case commandsPanelHeightRatioChanged(heightRatio: Double)
  case terminalError(sessionId: String, message: String)
  case terminalTextResult(requestId: String, sessionId: String, ok: Bool, text: String?, error: String?)
  case persistenceSessionState(
    requestId: String, provider: String, sessionName: String, exists: Bool, error: String?)
  case projectEditorBackRequested(projectId: String)
  case projectEditorCompanionPaneHiddenChanged(projectId: String, hidden: Bool)
  case projectEditorTabSelected(
    projectId: String, url: String?, activeTabId: String?, tabs: [ProjectEditorBrowserTabState]?)
  case projectEditorLoadState(projectId: String, status: String, message: String?)
  case codeServerRuntimeStartFailed(projectId: String?, message: String)
  case projectBoardRequest(ProjectBoardBridgeRequest)
  case osIntegrationStatus(payloadJson: String)
  case sessionStatusIndicatorClicked(status: NativeSessionStatusIndicatorStatus)
  case petOverlayActivityClicked(projectId: String, sessionId: String)
  case sessionAttentionNotificationClicked(sessionId: String)
  case t3RuntimeStartFailed(sessionId: String?, message: String)
  case t3ThreadReady(
    sessionId: String, projectId: String, threadId: String, serverOrigin: String, workspaceRoot: String)
  case t3ThreadChanged(sessionId: String, threadId: String, title: String?)
  case processResult(requestId: String, exitCode: Int32, stdout: String, stderr: String)
  case gxserverResponse(
    requestId: String, path: String, ok: Bool, statusCode: Int?, bodyJson: String?, error: String?)
  case remoteGxserverStatus(remoteMachineId: String, payloadJson: String)
  case remoteGxserverResponse(
    remoteMachineId: String, requestId: String, path: String, ok: Bool, statusCode: Int?, bodyJson: String?, error: String?)
  case remoteGxserverPresentationEvent(remoteMachineId: String, payloadJson: String)
  case remoteSshPasswordSaveResult(
    remoteMachineId: String, requestId: String, ok: Bool, hasPassword: Bool, error: String?)
  case sidebarCliResult(requestId: String, ok: Bool, payloadJson: String)
  case gxserverStatus(payloadJson: String)

  private enum CodingKeys: String, CodingKey {
    case exitCode
    case cwd
    case foregroundPid
    case hidden
    case heightRatio
    case hasPassword
    case message
    case protocolVersion
    case action
    case agentId
    case appName
    case beadDisplayId
    case beadId
    case details
    case event
    case bundleIdentifier
    case imagePath
    case placement
    case actionId
    case sessionId
    case stderr
    case stdout
    case title
    case trigger
    case faviconDataUrl
    case url
    case activeTabId
    case tabs
    case projectEditorId
    case projectId
    case serverOrigin
    case threadId
    case ttyName
    case type
    case workspaceRoot
    case requestId
    case ok
    case bodyJson
    case payloadJson
    case path
    case projectPath
    case prompt
    case error
    case exists
    case persistenceSessionCreated
    case provider
    case reason
    case text
    case sessionName
    case sessionPersistenceName
    case scope
    case status
    case statusCode
    case startLocation
    case ticketTitle
    case position
    case sourceSessionId
    case targetSessionId
    case tmuxSessionName
    case remoteMachineId
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    switch self {
    case .hostReady:
      try container.encode("hostReady", forKey: .type)
      try container.encode(1, forKey: .protocolVersion)
    case .appShotCaptured(
      let appName, let bundleIdentifier, let imagePath, let text, let title, let trigger):
      try container.encode("appShotCaptured", forKey: .type)
      try container.encodeIfPresent(appName, forKey: .appName)
      try container.encodeIfPresent(bundleIdentifier, forKey: .bundleIdentifier)
      try container.encode(imagePath, forKey: .imagePath)
      try container.encodeIfPresent(text, forKey: .text)
      try container.encodeIfPresent(title, forKey: .title)
      try container.encode(trigger, forKey: .trigger)
    case .appShotCaptureFailed(let message):
      try container.encode("appShotCaptureFailed", forKey: .type)
      try container.encode(message, forKey: .message)
    case .nativeHotkey(let actionId):
      /**
       CDXC:Hotkeys 2026-04-28-06:15
       AppKit-matched hotkeys must travel over the typed native host event bus
       instead of an optional JavaScript global. This makes the native-to-sidebar
       boundary observable and avoids silently dropping shortcuts before the
       sidebar action executor can run.
      */
      try container.encode("nativeHotkey", forKey: .type)
      try container.encode(actionId, forKey: .actionId)
    case .terminalReady(
      let sessionId, let ttyName, let foregroundPid, let sessionPersistenceName,
      let persistenceSessionCreated):
      try container.encode("terminalReady", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encodeIfPresent(ttyName, forKey: .ttyName)
      try container.encodeIfPresent(foregroundPid, forKey: .foregroundPid)
      try container.encodeIfPresent(sessionPersistenceName, forKey: .sessionPersistenceName)
      try container.encodeIfPresent(persistenceSessionCreated, forKey: .persistenceSessionCreated)
    case .terminalTitleChanged(let sessionId, let title, let sessionPersistenceName):
      try container.encode("terminalTitleChanged", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(title, forKey: .title)
      try container.encodeIfPresent(sessionPersistenceName, forKey: .sessionPersistenceName)
    case .browserFaviconChanged(let sessionId, let faviconDataUrl):
      try container.encode("browserFaviconChanged", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encodeIfPresent(faviconDataUrl, forKey: .faviconDataUrl)
    case .browserUrlChanged(let sessionId, let url):
      try container.encode("browserUrlChanged", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(url, forKey: .url)
    case .browserOpenInNewTabRequested(let sourceSessionId, let url):
      /**
       CDXC:BrowserTabs 2026-06-13-00:00:
       Normal CEF browser panes need target-blank, middle-click, and context-menu new-window intents to create a sibling Agents/workspace browser tab.
       Send only the source native session id and destination URL so the sidebar can preserve tab-group placement without native duplicating React workspace state.
       */
      try container.encode("browserOpenInNewTabRequested", forKey: .type)
      try container.encode(sourceSessionId, forKey: .sourceSessionId)
      try container.encode(url, forKey: .url)
    case .terminalTitleBarAction(let sessionId, let action):
      try container.encode("terminalTitleBarAction", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(action, forKey: .action)
    case .paneReorderRequested(let sourceSessionId, let targetSessionId, let placement):
      try container.encode("paneReorderRequested", forKey: .type)
      try container.encode(sourceSessionId, forKey: .sourceSessionId)
      try container.encode(targetSessionId, forKey: .targetSessionId)
      try container.encodeIfPresent(placement, forKey: .placement)
    case .paneTabSelected(let sessionId):
      try container.encode("paneTabSelected", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .sleepingPaneWakeRequested(let sessionId):
      /**
       CDXC:SleepingPanePlaceholders 2026-06-13-01:44:
       Native placeholders send a separate wake event so selecting a sleeping
       tab can preserve split geometry without starting Ghostty until the user
       clicks the black pane body.
       */
      try container.encode("sleepingPaneWakeRequested", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .paneTabFocusRequested(let sessionId):
      /**
       CDXC:SessionFocusMode 2026-05-23-09:28:
       Native pane-tab double-clicks and tab context-menu Focus need a distinct
       event from normal selection because the sidebar must enter reversible
       focus mode and may switch the workarea from Code/Git/Project to Agents.
       */
      try container.encode("paneTabFocusRequested", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .paneTabReorderRequested(let sourceSessionId, let targetSessionId, let position):
      /**
       CDXC:PaneTabs 2026-05-11-01:43
       Tab-bar reorder gestures use a dedicated host event instead of
       paneReorderRequested so the sidebar can mutate tab order without creating
       a split, pane grouping, or visibleSessionIds swap.
       */
      try container.encode("paneTabReorderRequested", forKey: .type)
      try container.encode(sourceSessionId, forKey: .sourceSessionId)
      try container.encode(targetSessionId, forKey: .targetSessionId)
      try container.encode(position, forKey: .position)
    case .paneTabCloseRequested(let sessionId, let scope):
      try container.encode("paneTabCloseRequested", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(scope, forKey: .scope)
    case .paneTabSleepRequested(let sessionId, let scope):
      try container.encode("paneTabSleepRequested", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(scope, forKey: .scope)
    case .terminalCwdChanged(let sessionId, let cwd):
      try container.encode("terminalCwdChanged", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(cwd, forKey: .cwd)
    case .terminalExited(let sessionId, let exitCode, let text):
      try container.encode("terminalExited", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encodeIfPresent(exitCode, forKey: .exitCode)
      try container.encodeIfPresent(text, forKey: .text)
    case .terminalFocused(let sessionId):
      try container.encode("terminalFocused", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .terminalEscapePressed(let sessionId):
      /**
       CDXC:SessionStatus 2026-06-11-08:46:
       Escape suppression must be tied to terminal input, not sidebar/modal
       Escape handling. Report this event only after Ghostty's surface keyDown
       decides the Escape key is going to the terminal process.
       */
      try container.encode("terminalEscapePressed", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .terminalBell(let sessionId):
      try container.encode("terminalBell", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .firstPromptAutoRenameCancelled(let sessionId):
      /**
       CDXC:SessionTitleSync 2026-05-30-05:44:
       Escape in the native "Generating title..." overlay must cancel the
       sidebar's in-flight first-prompt title generation without forwarding
       Escape to the terminal process.
       */
      try container.encode("firstPromptAutoRenameCancelled", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .nativeSessionSurfaceMissing(let sessionId):
      /**
       CDXC:SessionSurfaceRecovery 2026-05-23-09:05:
       If layout sync asks AppKit to focus an active session id but no terminal
       or web pane surface exists for that id, the sidebar must treat the row as
       stale runtime state and perform the same full reload a user would choose
       manually. Report the missing surface explicitly instead of leaving the
       tab selected but impossible to focus.
       */
      try container.encode("nativeSessionSurfaceMissing", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .terminalRestoreBlocked(let sessionId, let reason, let cwd):
      /**
       CDXC:SessionRestore 2026-05-28-16:13:
       Deleted project/chat folders are a user-action restore failure, not a
       terminal process failure. Report the missing cwd to the sidebar before
       launching Ghostty so the user can confirm removing the dead session
       instead of watching an empty pane appear and close immediately.
       */
      try container.encode("terminalRestoreBlocked", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(reason, forKey: .reason)
      try container.encode(cwd, forKey: .cwd)
    case .commandsPanelHeightRatioChanged(let heightRatio):
      try container.encode("commandsPanelHeightRatioChanged", forKey: .type)
      try container.encode(heightRatio, forKey: .heightRatio)
    case .terminalError(let sessionId, let message):
      try container.encode("terminalError", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(message, forKey: .message)
    case .terminalTextResult(let requestId, let sessionId, let ok, let text, let error):
      /**
       CDXC:CliTerminalReadback 2026-05-23-13:18:
       Agent-to-agent automation needs to inspect another visible Ghostex
       terminal by session id or title. Return explicit CLI readback results
       over the host-event bus so read operations do not create hidden sessions
       or depend on screenshots.
       */
      try container.encode("terminalTextResult", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(ok, forKey: .ok)
      try container.encodeIfPresent(text, forKey: .text)
      try container.encodeIfPresent(error, forKey: .error)
    case .persistenceSessionState(let requestId, let provider, let sessionName, let exists, let error):
      try container.encode("persistenceSessionState", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(provider, forKey: .provider)
      try container.encode(sessionName, forKey: .sessionName)
      try container.encode(exists, forKey: .exists)
      try container.encodeIfPresent(error, forKey: .error)
    case .projectEditorBackRequested(let projectId):
      /**
       CDXC:ProjectEditorCompanion 2026-05-14-09:19:
       The native editor companion Back button returns to the agents workarea
       immediately in AppKit, then notifies the sidebar so its project-editor
       open state stops reactivating VS Code on the next layout sync.
      */
      try container.encode("projectEditorBackRequested", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
    case .projectEditorCompanionPaneHiddenChanged(let projectId, let hidden):
      /**
       CDXC:ProjectEditorCompanion 2026-05-16-14:42:
       Closing the agent side pane is a project preference, not a transient
       AppKit layout toggle. Report native close clicks to the sidebar so Code,
       Git, and Project editor modes share the hidden state across restarts.
       */
      try container.encode("projectEditorCompanionPaneHiddenChanged", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
      try container.encode(hidden, forKey: .hidden)
    case .projectEditorTabSelected(let projectId, let url, let activeTabId, let tabs):
      /**
       CDXC:ProjectBrowserTabs 2026-06-13-00:12:
       Browser project tabs and browser toolbar controls live in native AppKit
       chrome, but the sidebar remains the owner of active project and mode
       state. Send the selected project-editor id and active tab URL back so
       React's next layout sync keeps the Browser CEF pane visible instead of
       restoring the same project's Code CEF pane.

       CDXC:ProjectBrowserTabs 2026-06-13-00:12:
       Browser mode reports every project Browser tab back to React whenever
       native selection or navigation changes. React persists those tabs locally
       so restart restores them as sleeping metadata until Browser is surfaced.
       */
      try container.encode("projectEditorTabSelected", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
      try container.encodeIfPresent(url, forKey: .url)
      try container.encodeIfPresent(activeTabId, forKey: .activeTabId)
      try container.encodeIfPresent(tabs, forKey: .tabs)
    case .projectEditorLoadState(let projectId, let status, let message):
      /**
       CDXC:EditorPanes 2026-05-09-17:24
       Project editor load state is not terminal state. Report it through a
       project-scoped native event so the sidebar can keep the VS Code row
       visible while loading and show startup timeout/error diagnostics.
       */
      try container.encode("projectEditorLoadState", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
      try container.encode(status, forKey: .status)
      try container.encodeIfPresent(message, forKey: .message)
    case .codeServerRuntimeStartFailed(let projectId, let message):
      /**
       CDXC:EditorPanes 2026-06-06-23:50:
       A code-server process can fail before CEF navigation begins. Report that
       native runtime failure separately so React can show an error toast and
       avoid leaving users with only the delayed VS Code timeout row.
       */
      try container.encode("codeServerRuntimeStartFailed", forKey: .type)
      try container.encodeIfPresent(projectId, forKey: .projectId)
      try container.encode(message, forKey: .message)
    case .projectBoardRequest(let request):
      /**
       CDXC:ProjectBoard 2026-05-26-10:16:
       The Project WKWebView cannot own Ghostex session state. Forward its
       conversation-link actions over the typed host-event bus so the sidebar
       remains the single owner of project/session persistence and focusing.

       CDXC:ProjectBoardRouting 2026-06-04-23:51:
       Forward both the raw Project board project id and the native editor id. The raw id selects gxserver data, while projectEditorId routes the sidebar response back to the WKWebView that sent the request.
      */
      try container.encode("projectBoardRequest", forKey: .type)
      try container.encode(request.action, forKey: .action)
      try container.encodeIfPresent(request.agentId, forKey: .agentId)
      try container.encodeIfPresent(request.beadDisplayId, forKey: .beadDisplayId)
      try container.encodeIfPresent(request.beadId, forKey: .beadId)
      try container.encodeIfPresent(request.details, forKey: .details)
      try container.encodeIfPresent(request.event, forKey: .event)
      try container.encodeIfPresent(request.projectEditorId, forKey: .projectEditorId)
      try container.encodeIfPresent(request.prompt, forKey: .prompt)
      try container.encodeIfPresent(request.projectId, forKey: .projectId)
      try container.encodeIfPresent(request.projectPath, forKey: .projectPath)
      try container.encodeIfPresent(request.remoteMachineId, forKey: .remoteMachineId)
      try container.encode(request.requestId, forKey: .requestId)
      try container.encodeIfPresent(request.sessionId, forKey: .sessionId)
      try container.encodeIfPresent(request.startLocation, forKey: .startLocation)
      try container.encodeIfPresent(request.ticketTitle, forKey: .ticketTitle)
    case .osIntegrationStatus(let payloadJson):
      try container.encode("osIntegrationStatus", forKey: .type)
      try container.encode(payloadJson, forKey: .payloadJson)
    case .sessionStatusIndicatorClicked(let status):
      /**
       CDXC:SessionStatusIndicators 2026-06-02-15:27:
       Floating AppKit status circles report only the clicked aggregate status back to the sidebar adapter. The adapter chooses the current-window focus target from gxserver presentation plus local-only panes instead of letting AppKit own shared session inventory.
      */
      try container.encode("sessionStatusIndicatorClicked", forKey: .type)
      try container.encode(status, forKey: .status)
    case .petOverlayActivityClicked(let projectId, let sessionId):
      /**
       CDXC:PetOverlay 2026-05-14-10:23:
       Pet messages name one exact session. Carry the project id with the
       session id so sidebar routing can activate that specific card instead
       of cycling through an aggregate attention or working bucket.
       */
      try container.encode("petOverlayActivityClicked", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
      try container.encode(sessionId, forKey: .sessionId)
    case .sessionAttentionNotificationClicked(let sessionId):
      /**
       CDXC:SessionAttentionNotifications 2026-05-10-16:46
       Notification clicks carry the concrete native session id instead of a
       status bucket so the sidebar can focus the exact completed session.
       */
      try container.encode("sessionAttentionNotificationClicked", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .t3RuntimeStartFailed(let sessionId, let message):
      /**
       CDXC:T3CodeStartup 2026-06-09-07:07:
       T3 runtime launch/auth failures need their own native event so React can
       show a short error toast while the WKWebView switches to a stable failure
       page instead of continuing the liveness loading loop.
       */
      try container.encode("t3RuntimeStartFailed", forKey: .type)
      try container.encodeIfPresent(sessionId, forKey: .sessionId)
      try container.encode(message, forKey: .message)
    case .t3ThreadReady(let sessionId, let projectId, let threadId, let serverOrigin, let workspaceRoot):
      try container.encode("t3ThreadReady", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(projectId, forKey: .projectId)
      try container.encode(threadId, forKey: .threadId)
      try container.encode(serverOrigin, forKey: .serverOrigin)
      try container.encode(workspaceRoot, forKey: .workspaceRoot)
    case .t3ThreadChanged(let sessionId, let threadId, let title):
      try container.encode("t3ThreadChanged", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encode(threadId, forKey: .threadId)
      try container.encodeIfPresent(title, forKey: .title)
    case .processResult(let requestId, let exitCode, let stdout, let stderr):
      try container.encode("processResult", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(exitCode, forKey: .exitCode)
      try container.encode(stdout, forKey: .stdout)
      try container.encode(stderr, forKey: .stderr)
    case .gxserverResponse(let requestId, let path, let ok, let statusCode, let bodyJson, let error):
      try container.encode("gxserverResponse", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(path, forKey: .path)
      try container.encode(ok, forKey: .ok)
      try container.encodeIfPresent(statusCode, forKey: .statusCode)
      try container.encodeIfPresent(bodyJson, forKey: .bodyJson)
      try container.encodeIfPresent(error, forKey: .error)
    case .remoteGxserverStatus(let remoteMachineId, let payloadJson):
      /**
       CDXC:RemoteMachines 2026-06-03-00:18:
       Remote gxserver bootstrap status travels over a native-only event so
       React can render connection state while Swift keeps SSH, tunnel process,
       and Keychain token ownership out of the webview.
       */
      try container.encode("remoteGxserverStatus", forKey: .type)
      try container.encode(remoteMachineId, forKey: .remoteMachineId)
      try container.encode(payloadJson, forKey: .payloadJson)
    case .remoteGxserverResponse(let remoteMachineId, let requestId, let path, let ok, let statusCode, let bodyJson, let error):
      try container.encode("remoteGxserverResponse", forKey: .type)
      try container.encode(remoteMachineId, forKey: .remoteMachineId)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(path, forKey: .path)
      try container.encode(ok, forKey: .ok)
      try container.encodeIfPresent(statusCode, forKey: .statusCode)
      try container.encodeIfPresent(bodyJson, forKey: .bodyJson)
      try container.encodeIfPresent(error, forKey: .error)
    case .remoteGxserverPresentationEvent(let remoteMachineId, let payloadJson):
      try container.encode("remoteGxserverPresentationEvent", forKey: .type)
      try container.encode(remoteMachineId, forKey: .remoteMachineId)
      try container.encode(payloadJson, forKey: .payloadJson)
    case .remoteSshPasswordSaveResult(let remoteMachineId, let requestId, let ok, let hasPassword, let error):
      try container.encode("remoteSshPasswordSaveResult", forKey: .type)
      try container.encode(remoteMachineId, forKey: .remoteMachineId)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(ok, forKey: .ok)
      try container.encode(hasPassword, forKey: .hasPassword)
      try container.encodeIfPresent(error, forKey: .error)
    case .sidebarCliResult(let requestId, let ok, let payloadJson):
      try container.encode("sidebarCliResult", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(ok, forKey: .ok)
      try container.encode(payloadJson, forKey: .payloadJson)
    case .gxserverStatus(let payloadJson):
      /**
       CDXC:GxserverBootstrap 2026-05-30-15:39:
       Native gxserver bootstrap status crosses the existing typed host-event bus so the trusted React sidebar can update its gxserver client wrapper without owning daemon launch or reading local token files itself.
       */
      try container.encode("gxserverStatus", forKey: .type)
      try container.encode(payloadJson, forKey: .payloadJson)
    }
  }
}

enum TerminalTitleBarAction: String, Codable, Hashable {
  case close
  case closeCommandsPanel
  case delayedSend
  case expandCommandsPanel
  case fork
  case mergeAllTabs
  case newTerminal
  case openBrowser
  case pinCommandsPanel
  case popOut
  case reload
  case rename
  case restorePopOut
  case rotatePanesClockwise
  case sleep
  case splitHorizontal
  case splitVertical
  case unpinCommandsPanel
}

enum PaneDropPlacement: String, Codable {
  case bottom
  case center
  case left
  case right
  case top
}

enum PaneTabReorderPosition: String, Codable {
  case after
  case before
}

enum PaneTabCloseScope: String, Codable {
  case close
  case closeLeft
  case closeOthers
  case closeRight
}

enum PaneTabSleepScope: String, Codable {
  case sleep
  case sleepLeft
  case sleepOthers
  case sleepRight
}
