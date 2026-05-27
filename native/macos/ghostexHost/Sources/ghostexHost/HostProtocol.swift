import Foundation

enum HostCommand: Decodable {
  case createTerminal(CreateTerminal)
  case createWebPane(CreateWebPane)
  case openFloatingEditor(OpenFloatingEditor)
  case closeTerminal(SessionCommand)
  case closeWebPane(SessionCommand)
  case focusTerminal(SessionCommand)
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
  case setSessionStatusIndicators(SetSessionStatusIndicators)
  case setPetOverlayState(SetPetOverlayState)
  case showSessionAttentionNotification(ShowSessionAttentionNotification)
  case setTerminalLayout(SetTerminalLayout)
  case setTerminalVisibility(SetTerminalVisibility)
  case pickWorkspaceFolder
  case pickWorkspaceIcon(PickWorkspaceIcon)
  case showMessage(ShowMessage)
  case appendAgentDetectionDebugLog(AppendAgentDetectionDebugLog)
  case appendTerminalFocusDebugLog(AppendTerminalFocusDebugLog)
  case appendRestoreDebugLog(AppendRestoreDebugLog)
  case appendSessionTitleDebugLog(AppendSessionTitleDebugLog)
  case appendSidebarRefreshDebugLog(AppendSidebarRefreshDebugLog)
  case appendWorkspaceDockIndicatorDebugLog(AppendWorkspaceDockIndicatorDebugLog)
  case persistSharedSidebarStorage(PersistSharedSidebarStorage)
  case projectBoardResponse(ProjectBoardResponse)
  case playSound(PlaySound)
  case runProcess(RunProcess)
  case syncGhosttyTerminalSettings(SyncGhosttyTerminalSettings)
  case applyGhosttyConfigSettings(ApplyGhosttyConfigSettings)
  case openGhosttyConfigFile
  case openAccessibilityPreferences
  case requestMacOSNotificationPermission
  case openMacOSNotificationSettings
  case openExternalUrl(OpenExternalUrl)
  case openWorkspaceInFinder(OpenWorkspaceInFinder)
  case openWorkspaceInIde(OpenWorkspaceInIde)
  case openBrowserWindow(OpenBrowserWindow)
  case showBrowserWindow
  case openBrowserDevTools(SessionCommand)
  case injectBrowserReactGrab(SessionCommand)
  case injectBrowserAgentation(SessionCommand)
  case showBrowserProfilePicker(SessionCommand)
  case showBrowserImportSettings(SessionCommand)
  case setSidebarSide(SetSidebarSide)
  case setReactTitlebarHitRegions(SetReactTitlebarHitRegions)
  case openActiveProjectEditorFromTitlebar
  case exitFocusModeFromTitlebar
  case openAgentsModeFromTitlebar
  case openGitHubProjectFromTitlebar
  case showProjectEditorCompanionFromTitlebar
  case openTasksPlaceholderFromTitlebar
  case refreshWorkspaceOpenTargetAvailabilityFromTitlebar
  case rotateActivePaneLayoutClockwiseFromTitlebar
  case togglePetOverlayFromTitlebar
  case toggleCommandsPanelFromTitlebar
  case sleepInactiveSessionsFromTitlebar(SleepInactiveSessionsFromTitlebar)
  case quitResourcesFromTitlebar(QuitResourcesFromTitlebar)
  case runSidebarCommandFromTitlebar(RunSidebarCommandFromTitlebar)
  case runSidebarGitActionFromTitlebar(RunSidebarGitActionFromTitlebar)
  case configureZedOverlay(ConfigureZedOverlay)
  case openZedWorkspace(OpenZedWorkspace)
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
    case setSessionStatusIndicators
    case setPetOverlayState
    case showSessionAttentionNotification
    case setTerminalLayout
    case setTerminalVisibility
    case pickWorkspaceFolder
    case pickWorkspaceIcon
    case showMessage
    case appendAgentDetectionDebugLog
    case appendTerminalFocusDebugLog
    case appendRestoreDebugLog
    case appendSessionTitleDebugLog
    case appendSidebarRefreshDebugLog
    case appendWorkspaceDockIndicatorDebugLog
    case persistSharedSidebarStorage
    case projectBoardResponse
    case playSound
    case runProcess
    case syncGhosttyTerminalSettings
    case applyGhosttyConfigSettings
    case openGhosttyConfigFile
    case openAccessibilityPreferences
    case requestMacOSNotificationPermission
    case openMacOSNotificationSettings
    case openExternalUrl
    case openWorkspaceInFinder
    case openWorkspaceInIde
    case openBrowserWindow
    case showBrowserWindow
    case openBrowserDevTools
    case injectBrowserReactGrab
    case injectBrowserAgentation
    case showBrowserProfilePicker
    case showBrowserImportSettings
    case setSidebarSide
    case setReactTitlebarHitRegions
    case openActiveProjectEditorFromTitlebar
    case exitFocusModeFromTitlebar
    case openAgentsModeFromTitlebar
    case openGitHubProjectFromTitlebar
    case showProjectEditorCompanionFromTitlebar
    case openTasksPlaceholderFromTitlebar
    case refreshWorkspaceOpenTargetAvailabilityFromTitlebar
    case rotateActivePaneLayoutClockwiseFromTitlebar
    case togglePetOverlayFromTitlebar
    case toggleCommandsPanelFromTitlebar
    case sleepInactiveSessionsFromTitlebar
    case quitResourcesFromTitlebar
    case runSidebarCommandFromTitlebar
    case runSidebarGitActionFromTitlebar
    case configureZedOverlay
    case openZedWorkspace
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
    case .appendTerminalFocusDebugLog:
      self = .appendTerminalFocusDebugLog(try AppendTerminalFocusDebugLog(from: decoder))
    case .appendRestoreDebugLog:
      self = .appendRestoreDebugLog(try AppendRestoreDebugLog(from: decoder))
    case .appendSessionTitleDebugLog:
      self = .appendSessionTitleDebugLog(try AppendSessionTitleDebugLog(from: decoder))
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
    case .openExternalUrl:
      self = .openExternalUrl(try OpenExternalUrl(from: decoder))
    case .openWorkspaceInFinder:
      self = .openWorkspaceInFinder(try OpenWorkspaceInFinder(from: decoder))
    case .openWorkspaceInIde:
      self = .openWorkspaceInIde(try OpenWorkspaceInIde(from: decoder))
    case .openBrowserWindow:
      self = .openBrowserWindow(try OpenBrowserWindow(from: decoder))
    case .showBrowserWindow:
      self = .showBrowserWindow
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
    case .setReactTitlebarHitRegions:
      self = .setReactTitlebarHitRegions(try SetReactTitlebarHitRegions(from: decoder))
    case .openActiveProjectEditorFromTitlebar:
      self = .openActiveProjectEditorFromTitlebar
    case .exitFocusModeFromTitlebar:
      self = .exitFocusModeFromTitlebar
    case .openAgentsModeFromTitlebar:
      self = .openAgentsModeFromTitlebar
    case .openGitHubProjectFromTitlebar:
      self = .openGitHubProjectFromTitlebar
    case .showProjectEditorCompanionFromTitlebar:
      self = .showProjectEditorCompanionFromTitlebar
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
    case .sleepInactiveSessionsFromTitlebar:
      self = .sleepInactiveSessionsFromTitlebar(try SleepInactiveSessionsFromTitlebar(from: decoder))
    case .quitResourcesFromTitlebar:
      self = .quitResourcesFromTitlebar(try QuitResourcesFromTitlebar(from: decoder))
    case .runSidebarCommandFromTitlebar:
      self = .runSidebarCommandFromTitlebar(try RunSidebarCommandFromTitlebar(from: decoder))
    case .runSidebarGitActionFromTitlebar:
      self = .runSidebarGitActionFromTitlebar(try RunSidebarGitActionFromTitlebar(from: decoder))
    case .configureZedOverlay:
      self = .configureZedOverlay(try ConfigureZedOverlay(from: decoder))
    case .openZedWorkspace:
      self = .openZedWorkspace(try OpenZedWorkspace(from: decoder))
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
  let sessionId: String
  let sessionPersistenceName: String?
  let sessionPersistenceProvider: String?
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
  let prompt: String?
  let projectId: String?
  let projectPath: String?
  let requestId: String
  let sessionId: String?
  let ticketTitle: String?
}

struct StartT3CodeRuntime: Decodable {
  let cwd: String
}

struct SetT3CodeRuntimeSessionState: Decodable {
  /**
   CDXC:T3Code 2026-05-10-22:48
   The React sidebar owns the definition of a running T3 session: its card is
   included in the current session-sidebar projection and is not sleeping.
   Native receives only those sidebar ids so the provider heartbeat follows the
   session model instead of workspace-pane visibility.

   CDXC:T3Code 2026-05-14-09:34:
   Native also needs one awake T3 workspace root on the same state update so it
   can restart the managed t3code provider in the background when the server is
   no longer live but T3 session cards remain visible in the sidebar.
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
   */
  let cwd: String
  let linkVscodeUserConfig: Bool?
  let vscodeUserConfigDir: String?
}

struct CreateProjectEditorPane: Decodable {
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
  let activeProjectIsChat: Bool?
  let activeProjectIconDataUrl: String?
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
  let commandsPanelIsVisible: Bool?
  let commandsPanelLayout: NativeTerminalLayout?
  let commandsPanelMode: String?
  let debuggingMode: Bool?
  let focusRequestId: Int?
  let focusedSessionId: String?
  let isFocusModeActive: Bool?
  let sleepingSessionIds: [String]?
  let layoutChanged: Bool?
  let layout: NativeTerminalLayout?
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
  let sessionFaviconDataUrls: [String: String]?
  let sessionTitleBarActions: [String: [TerminalTitleBarAction]]?
  let sessionTitles: [String: String]?
  /**
   CDXC:SessionPersistence 2026-05-23-00:50:
   The sidebar owns the preference for showing provider/session ids. Decode it
   with layout sync because AppKit owns the top-right overlay view and still
   checks per-pane persistence metadata before rendering text.
   */
  let showSessionIdInTerminalPanes: Bool?
  let showProjectEditorDiffFileCount: Bool?
  let sidebarActions: TitlebarSidebarActions?
  let sessionPersistenceProvider: String?
  let titlebarResourceGroups: [TitlebarResourceGroup]?
  let workspaceOpenTargets: TitlebarWorkspaceOpenTargets?
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
  let hasGitHubCli: Bool
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

struct AppendSessionTitleDebugLog: Decodable {
  let details: String?
  let event: String
  let force: Bool?
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
  let key: String
  let payloadJson: String
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

struct SyncGhosttyTerminalSettings: Decodable {
  let adjustCellHeightPercent: Double
  let adjustCellWidth: Double
  let cursorStyle: String
  let fontFamily: String
  let fontSize: Double
  let fontVariationWeight: Int?
  let clipboardPasteProtection: Bool
  let clipboardTrimTrailingSpaces: Bool
  let confirmCloseSurface: String
  let copyOnSelect: String
  let cursorStyleBlink: Bool
  let ghosttyTheme: String
  let mouseHideWhileTyping: Bool
  let mouseScrollMultiplierDiscrete: Double
  let mouseScrollMultiplierPrecision: Double
  let reloadImmediately: Bool?
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
   CDXC:WorkspaceActions 2026-05-04-08:22
   Opening a project in an IDE must carry the Settings-selected target app so
   Swift can reuse the existing native launcher for Zed and VS Code variants.
   */
  let targetApp: ZedOverlayTargetApp
  let workspacePath: String
}

struct OpenBrowserWindow: Decodable {
  let url: String
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

struct RunSidebarCommandFromTitlebar: Decodable {
  let commandId: String
}

struct RunSidebarGitActionFromTitlebar: Decodable {
  let action: String
}

struct SleepInactiveSessionsFromTitlebar: Decodable {
  let sessionIds: [String]
}

struct QuitResourcesFromTitlebar: Decodable {
  let projectIds: [String]
  let sessionIds: [String]
}

enum SidebarSide: String, Decodable {
  case left
  case right
}

struct ConfigureZedOverlay: Decodable {
  let enabled: Bool
  let hideTitlebarButton: Bool?
  let reason: String?
  let targetApp: ZedOverlayTargetApp
  let workspacePath: String?
}

struct OpenZedWorkspace: Decodable {
  let targetApp: ZedOverlayTargetApp
  let workspacePath: String
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
  case nativeHotkey(actionId: String)
  case terminalReady(
    sessionId: String, ttyName: String?, foregroundPid: Int?, sessionPersistenceName: String?,
    persistenceSessionCreated: Bool?)
  case terminalTitleChanged(sessionId: String, title: String, sessionPersistenceName: String?)
  case browserFaviconChanged(sessionId: String, faviconDataUrl: String?)
  case browserUrlChanged(sessionId: String, url: String)
  case terminalTitleBarAction(sessionId: String, action: TerminalTitleBarAction)
  case paneReorderRequested(sourceSessionId: String, targetSessionId: String, placement: PaneDropPlacement?)
  case paneTabSelected(sessionId: String)
  case paneTabFocusRequested(sessionId: String)
  case paneTabReorderRequested(
    sourceSessionId: String, targetSessionId: String, position: PaneTabReorderPosition)
  case paneTabCloseRequested(sessionId: String, scope: PaneTabCloseScope)
  case paneTabSleepRequested(sessionId: String, scope: PaneTabSleepScope)
  case terminalCwdChanged(sessionId: String, cwd: String)
  case terminalExited(sessionId: String, exitCode: Int?)
  case terminalFocused(sessionId: String)
  case terminalBell(sessionId: String)
  case nativeSessionSurfaceMissing(sessionId: String)
  case commandsPanelHeightRatioChanged(heightRatio: Double)
  case terminalError(sessionId: String, message: String)
  case terminalTextResult(requestId: String, sessionId: String, ok: Bool, text: String?, error: String?)
  case persistenceSessionState(
    requestId: String, provider: String, sessionName: String, exists: Bool, error: String?)
  case projectEditorBackRequested(projectId: String)
  case projectEditorCompanionPaneHiddenChanged(projectId: String, hidden: Bool)
  case projectEditorTabSelected(projectId: String, url: String?)
  case projectEditorLoadState(projectId: String, status: String, message: String?)
  case projectBoardRequest(ProjectBoardBridgeRequest)
  case sessionStatusIndicatorClicked(status: NativeSessionStatusIndicatorStatus)
  case petOverlayActivityClicked(projectId: String, sessionId: String)
  case sessionAttentionNotificationClicked(sessionId: String)
  case t3ThreadReady(
    sessionId: String, projectId: String, threadId: String, serverOrigin: String, workspaceRoot: String)
  case t3ThreadChanged(sessionId: String, threadId: String, title: String?)
  case processResult(requestId: String, exitCode: Int32, stdout: String, stderr: String)
  case sidebarCliResult(requestId: String, ok: Bool, payloadJson: String)

  private enum CodingKeys: String, CodingKey {
    case exitCode
    case cwd
    case foregroundPid
    case hidden
    case heightRatio
    case message
    case protocolVersion
    case action
    case agentId
    case beadDisplayId
    case beadId
    case placement
    case actionId
    case sessionId
    case stderr
    case stdout
    case title
    case faviconDataUrl
    case url
    case projectId
    case serverOrigin
    case threadId
    case ttyName
    case type
    case workspaceRoot
    case requestId
    case ok
    case payloadJson
    case projectPath
    case prompt
    case error
    case exists
    case persistenceSessionCreated
    case provider
    case text
    case sessionName
    case sessionPersistenceName
    case scope
    case status
    case ticketTitle
    case position
    case sourceSessionId
    case targetSessionId
    case tmuxSessionName
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    switch self {
    case .hostReady:
      try container.encode("hostReady", forKey: .type)
      try container.encode(1, forKey: .protocolVersion)
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
    case .terminalExited(let sessionId, let exitCode):
      try container.encode("terminalExited", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
      try container.encodeIfPresent(exitCode, forKey: .exitCode)
    case .terminalFocused(let sessionId):
      try container.encode("terminalFocused", forKey: .type)
      try container.encode(sessionId, forKey: .sessionId)
    case .terminalBell(let sessionId):
      try container.encode("terminalBell", forKey: .type)
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
    case .projectEditorTabSelected(let projectId, let url):
      /**
       CDXC:GitProjectTabs 2026-05-16-09:50:
       Git project tabs and browser toolbar controls live in native AppKit
       chrome, but the sidebar remains the owner of active project and mode
       state. Send the selected project-editor id and active tab URL back so
       React's next layout sync keeps the Git CEF pane visible instead of
       restoring the same project's Code CEF pane.
       */
      try container.encode("projectEditorTabSelected", forKey: .type)
      try container.encode(projectId, forKey: .projectId)
      try container.encodeIfPresent(url, forKey: .url)
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
    case .projectBoardRequest(let request):
      /**
       CDXC:ProjectBoard 2026-05-26-10:16:
       The Project WKWebView cannot own Ghostex session state. Forward its
       conversation-link actions over the typed host-event bus so the sidebar
       remains the single owner of project/session persistence and focusing.
      */
      try container.encode("projectBoardRequest", forKey: .type)
      try container.encode(request.action, forKey: .action)
      try container.encodeIfPresent(request.agentId, forKey: .agentId)
      try container.encodeIfPresent(request.beadDisplayId, forKey: .beadDisplayId)
      try container.encodeIfPresent(request.beadId, forKey: .beadId)
      try container.encodeIfPresent(request.prompt, forKey: .prompt)
      try container.encodeIfPresent(request.projectId, forKey: .projectId)
      try container.encodeIfPresent(request.projectPath, forKey: .projectPath)
      try container.encode(request.requestId, forKey: .requestId)
      try container.encodeIfPresent(request.sessionId, forKey: .sessionId)
      try container.encodeIfPresent(request.ticketTitle, forKey: .ticketTitle)
    case .sessionStatusIndicatorClicked(let status):
      /**
       CDXC:SessionStatusIndicators 2026-05-05-19:47
       Floating AppKit status circles report only the clicked aggregate status
       back to the sidebar. The sidebar owns the live session graph, so it
       selects and focuses the correct matching session at click time.
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
    case .sidebarCliResult(let requestId, let ok, let payloadJson):
      try container.encode("sidebarCliResult", forKey: .type)
      try container.encode(requestId, forKey: .requestId)
      try container.encode(ok, forKey: .ok)
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
