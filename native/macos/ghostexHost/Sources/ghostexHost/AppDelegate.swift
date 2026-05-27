import AppKit
import ApplicationServices
import CoreImage
import GhosttyKit
import OSLog
import Security
import Sparkle
import UniformTypeIdentifiers
import UserNotifications
import WebKit

/**
 CDXC:SidebarReference 2026-05-08-02:40
 The reference sidebar and standalone macOS title bar must share the same
 requested background color (#0e0e0e). Keep this in native code because AppKit,
 not the webview CSS, owns the NSWindow titlebar surface.
 */
private let ghostexReferenceSidebarChromeBackgroundColor = NSColor(
  srgbRed: 14.0 / 255.0,
  green: 14.0 / 255.0,
  blue: 14.0 / 255.0,
  alpha: 1.0)
/**
 CDXC:NativeWindowChrome 2026-05-25-07:16:
 The app titlebar should be only 5px taller than the original compact 30px
 strip. Keep this shared 35px height as the source for Swift layout reservation
 and native traffic-light centering so AppKit and React chrome do not drift.
 */
private let ghostexAppTitlebarHeight: CGFloat = 35
/**
 CDXC:NativeWindowChrome 2026-05-25-07:22:
 The traffic-light buttons should sit 5px below exact vertical center in the
 35px app titlebar. Keep this as a named visual-down offset so flipped and
 non-flipped AppKit titlebar coordinate systems apply the same requirement.
 */
private let ghostexTrafficLightVisualDownOffset: CGFloat = 2

private func normalizedNativeProcessEnvironment(overrides: [String: String]?) -> [String: String] {
  /**
   CDXC:NativeCommandBridge 2026-05-10-12:08
   macOS GUI launches do not reliably inherit the user's shell PATH. Native
   background commands must still find common developer tools installed through
   Homebrew, mise, asdf, or ~/.local/bin, because features such as session title
   generation run Codex through this process bridge instead of inside a terminal.
   */
  var environment = ProcessInfo.processInfo.environment
  environment["PATH"] = normalizedNativeProcessPath(environment["PATH"])
  if let overrides {
    environment.merge(overrides) { _, newValue in newValue }
    environment["PATH"] = normalizedNativeProcessPath(environment["PATH"])
  }
  return environment
}

private func normalizedNativeProcessPath(_ path: String?) -> String {
  let homeDirectory = NSHomeDirectory()
  let defaultEntries = [
    "\(homeDirectory)/.local/share/mise/shims",
    "\(homeDirectory)/.local/bin",
    "\(homeDirectory)/.asdf/shims",
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
  ]
  let existingEntries = (path ?? "")
    .split(separator: ":")
    .map(String.init)
  var seen = Set<String>()
  return (defaultEntries + existingEntries)
    .filter { entry in
      let normalizedEntry = entry.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !normalizedEntry.isEmpty, !seen.contains(normalizedEntry) else {
        return false
      }
      seen.insert(normalizedEntry)
      return true
    }
    .joined(separator: ":")
}

private final class SessionAttentionNotificationController: NSObject, UNUserNotificationCenterDelegate {
  private let center = UNUserNotificationCenter.current()
  private let onSessionClicked: (String) -> Void

  init(onSessionClicked: @escaping (String) -> Void) {
    self.onSessionClicked = onSessionClicked
    super.init()
    center.delegate = self
  }

  func show(_ command: ShowSessionAttentionNotification) {
    /**
     CDXC:SessionAttentionNotifications 2026-05-10-16:46
     The sidebar decides when attention notifications are allowed. Native code
     requests macOS alert permission only on first use, then posts a banner for
     the exact session id so click handling can focus the right pane.

     CDXC:SessionAttentionNotifications 2026-05-11-01:14
     Attention notifications must not add their own macOS notification sound.
     Request only alert permission and leave notification content sound unset;
     the existing completion-bell setting remains the only audio path.
     */
    center.getNotificationSettings { [weak self] settings in
      guard let self else { return }
      switch settings.authorizationStatus {
      case .authorized, .provisional:
        self.deliver(command)
      case .notDetermined:
        self.center.requestAuthorization(options: [.alert]) { granted, _ in
          if granted {
            self.deliver(command)
          }
        }
      case .denied:
        break
      @unknown default:
        break
      }
    }
  }

  func requestPermissionFromSettings() {
    center.getNotificationSettings { [weak self] settings in
      guard let self else { return }
      switch settings.authorizationStatus {
      case .authorized, .provisional:
        self.presentNotificationAlreadyEnabledDialog()
      case .notDetermined:
        self.presentNotificationPermissionExplanation()
      case .denied:
        self.presentNotificationSettingsDialog()
      @unknown default:
        self.presentNotificationSettingsDialog()
      }
    }
  }

  private func deliver(_ command: ShowSessionAttentionNotification) {
    let identifier = "ghostex.session.attention.\(command.sessionId).\(UUID().uuidString)"
    let content = UNMutableNotificationContent()
    let title = command.title.trimmingCharacters(in: .whitespacesAndNewlines)
    content.title = title.isEmpty ? "Session needs attention" : title
    content.body = command.body ?? "A Ghostex session needs attention."
    content.categoryIdentifier = "ghostex.session.attention"
    content.threadIdentifier = "ghostex.session.attention.\(command.sessionId)"
    content.targetContentIdentifier = command.sessionId
    content.userInfo = ["sessionId": command.sessionId]
    content.sound = nil
    let attachmentUrl = applyProjectIconAttachment(
      to: content,
      command: command,
      identifier: identifier
    )
    center.add(UNNotificationRequest(identifier: identifier, content: content, trigger: nil)) {
      [weak self] error in
      guard error == nil else { return }
      self?.removeDeliveredNotificationLater(identifier, attachmentUrl: attachmentUrl)
    }
  }

  private func applyProjectIconAttachment(
    to content: UNMutableNotificationContent,
    command: ShowSessionAttentionNotification,
    identifier: String
  ) -> URL? {
    /**
     CDXC:ProjectIcons 2026-05-11-01:50
     Attention notifications should show the same project image selected in the
     sidebar/React project model. Convert the shared data URL into a bounded
     temporary PNG attachment because macOS notification attachments require a
     file URL and may not render SVG data directly.
     */
    guard let attachmentUrl = Self.writeNotificationProjectIcon(
      command.iconDataUrl,
      notificationIdentifier: identifier
    ) else {
      return nil
    }
    do {
      content.attachments = [
        try UNNotificationAttachment(
          identifier: "projectIcon",
          url: attachmentUrl,
          options: [UNNotificationAttachmentOptionsTypeHintKey: UTType.png.identifier]
        )
      ]
      return attachmentUrl
    } catch {
      try? FileManager.default.removeItem(at: attachmentUrl)
      return nil
    }
  }

  private static func writeNotificationProjectIcon(
    _ dataUrl: String?,
    notificationIdentifier: String
  ) -> URL? {
    guard let dataUrl = dataUrl?.trimmingCharacters(in: .whitespacesAndNewlines),
      dataUrl.count <= 700_000,
      let commaIndex = dataUrl.firstIndex(of: ",")
    else {
      return nil
    }
    let header = dataUrl[..<commaIndex].lowercased()
    guard header.hasPrefix("data:image/"), header.contains(";base64") else {
      return nil
    }
    let payload = String(dataUrl[dataUrl.index(after: commaIndex)...])
    guard let rawData = Data(base64Encoded: payload), rawData.count <= 512_000 else {
      return nil
    }
    guard let image = NSImage(data: rawData), let pngData = pngDataForNotificationIcon(image) else {
      return nil
    }
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-notification-icons", isDirectory: true)
    do {
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      let fileName = notificationIdentifier.replacingOccurrences(of: "/", with: "_") + ".png"
      let fileUrl = directory.appendingPathComponent(fileName, isDirectory: false)
      try pngData.write(to: fileUrl, options: .atomic)
      return fileUrl
    } catch {
      return nil
    }
  }

  private static func pngDataForNotificationIcon(_ image: NSImage) -> Data? {
    let targetSize = NSSize(width: 128, height: 128)
    let sourceSize = image.size.width > 0 && image.size.height > 0 ? image.size : targetSize
    let scale = min(targetSize.width / sourceSize.width, targetSize.height / sourceSize.height)
    let drawSize = NSSize(width: sourceSize.width * scale, height: sourceSize.height * scale)
    let drawRect = NSRect(
      x: (targetSize.width - drawSize.width) / 2.0,
      y: (targetSize.height - drawSize.height) / 2.0,
      width: drawSize.width,
      height: drawSize.height
    )
    let output = NSImage(size: targetSize)
    output.lockFocus()
    NSColor.clear.setFill()
    NSRect(origin: .zero, size: targetSize).fill()
    image.draw(in: drawRect, from: .zero, operation: .sourceOver, fraction: 1.0)
    output.unlockFocus()
    guard let tiffData = output.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiffData)
    else {
      return nil
    }
    return bitmap.representation(using: .png, properties: [:])
  }

  private func removeDeliveredNotificationLater(_ identifier: String, attachmentUrl: URL?) {
    /**
     CDXC:SessionAttentionNotifications 2026-05-10-16:46
     Attention notifications should behave like temporary banners by default:
     if the user ignores or swipes one away, remove the delivered notification
     shortly afterward so it does not accumulate in Notification Center.
     */
    DispatchQueue.main.asyncAfter(deadline: .now() + 12.0) { [weak self] in
      self?.center.removeDeliveredNotifications(withIdentifiers: [identifier])
      if let attachmentUrl {
        try? FileManager.default.removeItem(at: attachmentUrl)
      }
    }
  }

  private func presentNotificationPermissionExplanation() {
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      let alert = NSAlert()
      alert.messageText = "Enable Ghostex Notifications"
      alert.informativeText =
        "Ghostex can show a temporary macOS banner when an agent task needs attention. Completion sounds remain controlled by Ghostex Settings."
      alert.alertStyle = .informational
      alert.addButton(withTitle: "Enable Notifications")
      alert.addButton(withTitle: "Cancel")
      if let primaryButton = alert.buttons.first {
        primaryButton.keyEquivalent = "\r"
        primaryButton.bezelColor = .controlAccentColor
      }
      if alert.buttons.count > 1 {
        alert.buttons[1].keyEquivalent = "\u{1b}"
      }
      guard alert.runModal() == .alertFirstButtonReturn else {
        return
      }
      self.center.requestAuthorization(options: [.alert]) { [weak self] granted, _ in
        if !granted {
          self?.presentNotificationSettingsDialog()
        }
      }
    }
  }

  private func presentNotificationAlreadyEnabledDialog() {
    DispatchQueue.main.async {
      let alert = NSAlert()
      alert.messageText = "Ghostex Notifications Are Enabled"
      alert.informativeText =
        "macOS already allows Ghostex to show notification banners. Use Test agent task completion to verify your current Ghostex sound and notification settings."
      alert.alertStyle = .informational
      alert.addButton(withTitle: "OK")
      alert.runModal()
    }
  }

  private func presentNotificationSettingsDialog() {
    DispatchQueue.main.async {
      let alert = NSAlert()
      alert.messageText = "Enable Notifications in macOS Settings"
      alert.informativeText =
        "macOS is not allowing Ghostex notification banners. Open Notification Settings and allow notifications for Ghostex."
      alert.alertStyle = .warning
      alert.addButton(withTitle: "Open Settings")
      alert.addButton(withTitle: "Cancel")
      if let primaryButton = alert.buttons.first {
        primaryButton.keyEquivalent = "\r"
        primaryButton.bezelColor = .controlAccentColor
      }
      if alert.buttons.count > 1 {
        alert.buttons[1].keyEquivalent = "\u{1b}"
      }
      guard alert.runModal() == .alertFirstButtonReturn else {
        return
      }
      Self.openMacOSNotificationSettings()
    }
  }

  static func openMacOSNotificationSettings() {
    /**
     CDXC:SessionAttentionNotifications 2026-05-11-01:14
     Settings exposes a direct path to macOS Notifications so users can repair
     denied banner permission without hunting through System Settings.
     */
    guard
      let url = URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension")
    else {
      return
    }
    NSWorkspace.shared.open(url)
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.banner])
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    guard
      response.notification.request.content.categoryIdentifier == "ghostex.session.attention",
      let sessionId = response.notification.request.content.userInfo["sessionId"] as? String
    else {
      completionHandler()
      return
    }
    DispatchQueue.main.async { [onSessionClicked] in
      onSessionClicked(sessionId)
      completionHandler()
    }
  }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
  static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "app")
  private static let standardWindowButtonTypes: [NSWindow.ButtonType] = [
    .closeButton, .miniaturizeButton, .zoomButton,
  ]
  private static let logDateFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS ZZZZ"
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = .current
    return formatter
  }()
  private static var createdLogDirectories = Set<String>()
  nonisolated(unsafe) let ghostty: GhostexGhosttyApp
  let undoManager = UndoManager()
  private let ghosttyConfigSelection: GhosttyConfigSelection

  private var bridge: NativeHostBridge?
  private var tickTimer: Timer?
  private var window: NSWindow?
  private var workspacePath =
    ProcessInfo.processInfo.environment["ghostex_WORKSPACE_PATH"]
    ?? FileManager.default.currentDirectoryPath
  private weak var workspaceView: TerminalWorkspaceView?
  private var zedOverlayController: ZedOverlayController?
  private var browserOverlayController: BrowserOverlayController?
  private var sessionStatusIndicatorController: SessionStatusIndicatorController?
  private var petOverlayController: PetOverlayController?
  private var hasPresentedAccessibilityPermissionDialog = false
  private var pendingZedOverlayConfiguration: ConfigureZedOverlay?
  private var hasUserDetachedZedOverlay = false
  private var isMainWindowHiddenByIdeAttachment = false
  private var lastVisibleMainWindowFrameForPersistence: NSRect?
  private var pendingGhosttyConfigReloadTimer: Timer?
  private var isFlushingCEFBeforeTerminate = false
  private var didFlushCEFBeforeTerminate = false
  private var workspaceActivationObserver: NSObjectProtocol?
  private var appHotkeyEventMonitor: Any?
  private var lastNativeActivationRequest: NativeActivationRequest?
  private var lastNativeInputEventPayload: [String: Any]?
  private var lastNativeInputEventRecordedAt: Date?
  private weak var attachToIdeTitlebarButton: NSButton?
  private weak var appTitlebarLabel: NSTextField?
  private let nativeSettingsStore = NativeSettingsStore()
  private let updaterController: SPUStandardUpdaterController
  private var t3CodeRuntimeProcess: Process?
  private var t3RuntimeVisibleSessionCwd: String?
  private var t3RuntimeLivenessTimer: Timer?
  private var codeServerRuntimeProcess: Process?
  private var codeServerRuntimeStartedAt: Date?
  private lazy var sessionAttentionNotificationController =
    SessionAttentionNotificationController { [weak self] sessionId in
      Task { @MainActor in
        self?.handleSessionAttentionNotificationClick(sessionId)
      }
    }

  private struct NativeActivationRequest {
    let reason: String
    let sessionId: String?
    let timestamp: Date
  }

  override init() {
    let configSelection = Self.preferredGhosttyConfig()
    /**
     CDXC:NativeTerminals 2026-04-26-06:50
     Embedded Ghostty terminals should use the same user configuration as
     Ghostty itself. Honor GHOSTTY_CONFIG_PATH when provided; otherwise let
     Ghostty load its normal default config files from the user's machine.
     */
    ghosttyConfigSelection = configSelection
    ghostty = GhostexGhosttyApp(configPath: configSelection.path)
    /**
     CDXC:AutoUpdate 2026-05-02-06:51
     The native app should use Sparkle for the macOS appcast update flow, like
     DockDoor. Start the updater controller during app initialization so the
     menu item and background checks share Sparkle's standard state machine.
     */
    updaterController = SPUStandardUpdaterController(
      startingUpdater: true,
      updaterDelegate: nil,
      userDriverDelegate: nil)
    super.init()
    logGhosttyConfigStartup()
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.regular)
    installWorkspaceActivationObserver()
    Self.appendNativeHostLifecycleLog(
      "applicationDidFinishLaunching pid=\(ProcessInfo.processInfo.processIdentifier) workspacePath=\(workspacePath)"
    )
    MainActor.assumeIsolated {
      installMainMenu()
      /**
       CDXC:NativeTerminals 2026-04-28-12:06
       Persistent helper mode was removed by request. Native terminals now
       always use the in-process embedded Ghostty SurfaceView backend from
       startup, so no restart-survival helper client is created.
       */
      makeWindow()
      installAppHotkeyEventMonitor()
      startBridge()
      startSparkleBackgroundUpdateCheck()
    }
    tickTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
      self?.ghostty.appTick()
    }
  }

  func applicationWillTerminate(_ notification: Notification) {
    if let workspaceActivationObserver {
      NSWorkspace.shared.notificationCenter.removeObserver(workspaceActivationObserver)
    }
    if let appHotkeyEventMonitor {
      NSEvent.removeMonitor(appHotkeyEventMonitor)
      self.appHotkeyEventMonitor = nil
    }
    persistMainWindowChrome()
    (window?.contentView as? ghostexRootView)?.persistNativeChromeForAppLifecycle()
    Self.appendNativeHostLifecycleLog(
      "applicationWillTerminate pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false)"
    )
    stopCodeServerRuntime(logPrefix: "nativeHost.applicationWillTerminate")
    (window?.contentView as? ghostexRootView)?.stopCodeServerRuntimeForAppTermination()
  }

  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    if didFlushCEFBeforeTerminate {
      return .terminateNow
    }
    if isFlushingCEFBeforeTerminate {
      return .terminateLater
    }
    isFlushingCEFBeforeTerminate = true
    /**
     CDXC:ChromiumBrowserPanes 2026-05-06-01:12
     Chrome embed panes must preserve authenticated browser sessions across
     ghostex restarts. Delay app termination long enough for CEF to flush cookie
     stores before the CEF message loop exits and CefShutdown runs.
     */
    GhostexCEFFlushBrowserState { [weak self, weak sender] in
      guard let self else {
        sender?.reply(toApplicationShouldTerminate: true)
        return
      }
      self.didFlushCEFBeforeTerminate = true
      self.isFlushingCEFBeforeTerminate = false
      sender?.reply(toApplicationShouldTerminate: true)
    }
    return .terminateLater
  }

  func applicationWillBecomeActive(_ notification: Notification) {
    /**
     CDXC:IDEAttachment 2026-04-29-03:08
     Dock-click surfacing needs native activation breadcrumbs outside the
     overlay controller so a repro can distinguish "macOS never activated
     ghostex" from "the overlay activation branch made the wrong ordering call."
     */
    Self.appendNativeHostLifecycleLog(
      "applicationWillBecomeActive pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.activation.willBecomeActive")
    BrowserOverlayRestoreReproLog.append(
      "appDelegate.applicationWillBecomeActive",
      [
        "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName as Any,
        "keyWindow": window?.isKeyWindow as Any,
        "windowVisible": window?.isVisible as Any,
      ])
  }

  func applicationDidBecomeActive(_ notification: Notification) {
    Self.appendNativeHostLifecycleLog(
      "applicationDidBecomeActive pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.activation.didBecomeActive")
    BrowserOverlayRestoreReproLog.append(
      "appDelegate.applicationDidBecomeActive",
      [
        "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName as Any,
        "keyWindow": window?.isKeyWindow as Any,
        "windowFrame": window.map {
          "x=\($0.frame.minX),y=\($0.frame.minY),w=\($0.frame.width),h=\($0.frame.height)"
        } as Any,
        "windowLevel": window?.level.rawValue as Any,
        "windowVisible": window?.isVisible as Any,
      ])
  }

  func applicationDidResignActive(_ notification: Notification) {
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-20:09:
     Focus-steal reports need both sides of the activation boundary. Log when Ghostex resigns active so the next self-activation can be compared against the exact workspace, responder, and recent input that existed before macOS brought another app or Ghostex forward.
     */
    Self.appendNativeHostLifecycleLog(
      "applicationDidResignActive pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.activation.didResignActive")
  }

  @MainActor
  private func installWorkspaceActivationObserver() {
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-10:54:
     Focus-steal reports can happen after a session already exists, so creation logs are insufficient.
     Record system-wide app activation transitions and the latest internal ghostex activation request so a later repro can separate explicit ghostex activation from an external macOS/frontmost-app transition.
     */
    workspaceActivationObserver = NSWorkspace.shared.notificationCenter.addObserver(
      forName: NSWorkspace.didActivateApplicationNotification,
      object: nil,
      queue: .main
    ) { [weak self] notification in
      guard let self else {
        return
      }
      let application =
        notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
      let isSelf = application?.processIdentifier == ProcessInfo.processInfo.processIdentifier
      let details: [String: Any] = [
        "activatedBundleIdentifier": application?.bundleIdentifier ?? NSNull(),
        "activatedName": application?.localizedName ?? NSNull(),
        "activatedPid": application.map { Int($0.processIdentifier) } ?? NSNull(),
        "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName ?? NSNull(),
        "isSelf": isSelf,
        "lastActivationRequest": self.lastNativeActivationRequestPayload(),
        "recentInput": self.recentNativeInputEventPayload(),
        "workspace": self.workspaceView?.activationDebugSnapshot() ?? NSNull(),
      ]
      TerminalFocusDebugLog.append(event: "nativeHost.workspaceApplicationActivated", details: details)
      Self.appendNativeHostLifecycleLog(
        "workspaceApplicationActivated app=\(application?.localizedName ?? "<missing>") pid=\(application.map { String($0.processIdentifier) } ?? "<missing>") isSelf=\(isSelf) lastActivationRequest=\(self.describeLastNativeActivationRequest()) recentInput=\(self.describeRecentNativeInputEvent()) workspace=\(self.describeWorkspaceActivationSnapshot())"
      )
    }
  }

  @MainActor
  private func recordNativeActivationRequest(reason: String, sessionId: String? = nil) {
    lastNativeActivationRequest = NativeActivationRequest(
      reason: reason,
      sessionId: sessionId,
      timestamp: Date()
    )
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-10:54:
     Any code path that intentionally raises Ghostex should leave an activation breadcrumb before calling NSApp.activate or makeKeyAndOrderFront.
     The next activation notification can then prove whether ghostex stole focus by request or was activated by something outside the native host.
     */
    TerminalFocusDebugLog.append(
      event: "nativeHost.activation.request",
      details: [
        "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName ?? NSNull(),
        "reason": reason,
        "sessionId": sessionId ?? NSNull(),
        "windowIsKey": window?.isKeyWindow ?? false,
        "windowIsVisible": window?.isVisible ?? false,
        "recentInput": recentNativeInputEventPayload(),
        "workspace": workspaceView?.activationDebugSnapshot() ?? NSNull(),
      ])
    Self.appendNativeHostLifecycleLog(
      "activationRequest reason=\(reason) sessionId=\(sessionId ?? "<none>") windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
  }

  @MainActor
  private func logNativeActivationLifecycleEvent(_ event: String) {
    TerminalFocusDebugLog.append(
      event: event,
      details: [
        "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName ?? NSNull(),
        "lastActivationRequest": lastNativeActivationRequestPayload(),
        "recentInput": recentNativeInputEventPayload(),
        "windowIsKey": window?.isKeyWindow ?? false,
        "windowIsVisible": window?.isVisible ?? false,
        "workspace": workspaceView?.activationDebugSnapshot() ?? NSNull(),
      ])
  }

  @MainActor
  private func logNativeActivationBoundaryInputEvent(_ event: NSEvent, phase: String) {
    let now = Date()
    let eventType = Self.nativeEventTypeName(event.type)
    let eventAgeMs = Int((ProcessInfo.processInfo.systemUptime - event.timestamp) * 1000)
    let eventWindow = event.window ?? window
    let hitView = Self.hitViewDescription(for: event, in: eventWindow)
    let payload: [String: Any] = [
      "appIsActive": NSApp.isActive,
      "buttonNumber": event.buttonNumber,
      "clickCount": event.clickCount,
      "eventAgeMs": eventAgeMs,
      "eventNumber": event.eventNumber,
      "eventTimestamp": event.timestamp,
      "eventType": eventType,
      "frontmostApplication": NSWorkspace.shared.frontmostApplication?.localizedName ?? NSNull(),
      "hitView": hitView,
      "isSyntheticLikely": event.eventNumber == 0,
      "lastActivationRequest": lastNativeActivationRequestPayload(),
      "locationInWindowX": Double(event.locationInWindow.x),
      "locationInWindowY": Double(event.locationInWindow.y),
      "modifierFlags": Self.nativeEventModifierNames(event.modifierFlags),
      "phase": phase,
      "workspace": workspaceView?.activationDebugSnapshot() ?? NSNull(),
      "windowIsKey": eventWindow?.isKeyWindow ?? false,
      "windowIsMain": eventWindow?.isMainWindow ?? false,
      "windowIsVisible": eventWindow?.isVisible ?? false,
      "windowNumber": eventWindow?.windowNumber ?? 0,
    ]
    lastNativeInputEventPayload = [
      "eventAgeMs": eventAgeMs,
      "eventNumber": event.eventNumber,
      "eventType": eventType,
      "hitView": hitView,
      "isSyntheticLikely": event.eventNumber == 0,
      "phase": phase,
      "windowIsKey": eventWindow?.isKeyWindow ?? false,
      "windowNumber": eventWindow?.windowNumber ?? 0,
    ]
    lastNativeInputEventRecordedAt = now
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-20:09:
     App activation logs showed Ghostex becoming frontmost without a fresh internal activation request. Persist the AppKit input event at the window boundary, including synthetic-event detection and hit view, so the next repro can distinguish a real click into Ghostex from a delayed synthetic companion click or an external macOS/window-ordering activation.
     */
    TerminalFocusDebugLog.append(
      event: "nativeHost.activationBoundary.inputEvent",
      details: payload,
      force: true)
    Self.appendNativeHostLifecycleLog(
      "activationBoundaryInput phase=\(phase) type=\(eventType) eventNumber=\(event.eventNumber) syntheticLikely=\(event.eventNumber == 0) appActive=\(NSApp.isActive) keyWindow=\(eventWindow?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") eventAgeMs=\(eventAgeMs) hitView=\(hitView)"
    )
  }

  @MainActor
  private func describeLastNativeActivationRequest() -> String {
    guard let lastNativeActivationRequest else {
      return "<none>"
    }
    let ageMs = Int(Date().timeIntervalSince(lastNativeActivationRequest.timestamp) * 1000)
    let sessionText = lastNativeActivationRequest.sessionId ?? "<none>"
    return "\(lastNativeActivationRequest.reason) sessionId=\(sessionText) ageMs=\(ageMs)"
  }

  @MainActor
  private func lastNativeActivationRequestPayload() -> Any {
    guard let lastNativeActivationRequest else {
      return NSNull()
    }
    return [
      "ageMs": Int(Date().timeIntervalSince(lastNativeActivationRequest.timestamp) * 1000),
      "reason": lastNativeActivationRequest.reason,
      "sessionId": lastNativeActivationRequest.sessionId ?? NSNull(),
    ]
  }

  @MainActor
  private func describeWorkspaceActivationSnapshot() -> String {
    guard let snapshot = workspaceView?.activationDebugSnapshot() else {
      return "<missing>"
    }
    let focused = snapshot["focusedSessionId"] as? String ?? "<none>"
    let responder = snapshot["responderSessionId"] as? String ?? "<none>"
    let projectEditor = snapshot["activeProjectEditorId"] as? String ?? "<none>"
    return "focused=\(focused) responder=\(responder) activeProjectEditor=\(projectEditor)"
  }

  @MainActor
  private func recentNativeInputEventPayload() -> Any {
    guard var payload = lastNativeInputEventPayload else {
      return NSNull()
    }
    if let lastNativeInputEventRecordedAt {
      payload["recordedAgeMs"] = Int(Date().timeIntervalSince(lastNativeInputEventRecordedAt) * 1000)
    }
    return payload
  }

  @MainActor
  private func describeRecentNativeInputEvent() -> String {
    guard let payload = recentNativeInputEventPayload() as? [String: Any] else {
      return "<none>"
    }
    let type = payload["eventType"] as? String ?? "<unknown>"
    let phase = payload["phase"] as? String ?? "<unknown>"
    let eventNumber = payload["eventNumber"].map { "\($0)" } ?? "<unknown>"
    let recordedAgeMs = payload["recordedAgeMs"].map { "\($0)" } ?? "<unknown>"
    let hitView = payload["hitView"] as? String ?? "<unknown>"
    return "\(type) phase=\(phase) eventNumber=\(eventNumber) ageMs=\(recordedAgeMs) hitView=\(hitView)"
  }

  private static func hitViewDescription(for event: NSEvent, in window: NSWindow?) -> String {
    guard let contentView = window?.contentView else {
      return "<none>"
    }
    let contentPoint = contentView.convert(event.locationInWindow, from: nil)
    guard let hitView = contentView.hitTest(contentPoint) else {
      return "<none>"
    }
    return String(describing: type(of: hitView))
  }

  private static func nativeEventTypeName(_ eventType: NSEvent.EventType) -> String {
    switch eventType {
    case .leftMouseDown: return "leftMouseDown"
    case .leftMouseUp: return "leftMouseUp"
    case .rightMouseDown: return "rightMouseDown"
    case .rightMouseUp: return "rightMouseUp"
    case .otherMouseDown: return "otherMouseDown"
    case .otherMouseUp: return "otherMouseUp"
    case .keyDown: return "keyDown"
    default: return "\(eventType.rawValue)"
    }
  }

  private static func nativeEventModifierNames(_ flags: NSEvent.ModifierFlags) -> [String] {
    let normalizedFlags = flags.intersection(.deviceIndependentFlagsMask)
    var names: [String] = []
    if normalizedFlags.contains(.capsLock) { names.append("capsLock") }
    if normalizedFlags.contains(.shift) { names.append("shift") }
    if normalizedFlags.contains(.control) { names.append("control") }
    if normalizedFlags.contains(.option) { names.append("option") }
    if normalizedFlags.contains(.command) { names.append("command") }
    if normalizedFlags.contains(.numericPad) { names.append("numericPad") }
    if normalizedFlags.contains(.help) { names.append("help") }
    if normalizedFlags.contains(.function) { names.append("function") }
    return names
  }

  private struct GhosttyConfigSelection {
    let path: String?
    let source: String
  }

  private static func preferredGhosttyConfig() -> GhosttyConfigSelection {
    let value = ProcessInfo.processInfo.environment["GHOSTTY_CONFIG_PATH"]?.trimmingCharacters(
      in: .whitespacesAndNewlines)
    if value?.isEmpty == false {
      return GhosttyConfigSelection(path: value, source: "GHOSTTY_CONFIG_PATH")
    }

    let appSupportURL = FileManager.default.urls(
      for: .applicationSupportDirectory, in: .userDomainMask
    ).first
    let macOSConfigPaths = [
      appSupportURL?.appendingPathComponent("com.mitchellh.ghostty/config").path,
      appSupportURL?.appendingPathComponent("com.ghostty.org/config").path,
      appSupportURL?.appendingPathComponent("Ghostty/config").path,
    ].compactMap { $0 }
    /**
     CDXC:NativeTerminals 2026-04-26-06:53
     Installed Ghostty for macOS stores user settings in Application Support
     on this machine. Prefer that real app config before falling back to
     Ghostty's default loader so embedded terminals match the user's app.
     */
    if let path = macOSConfigPaths.first(where: { FileManager.default.fileExists(atPath: $0) }) {
      return GhosttyConfigSelection(path: path, source: "macOS Application Support")
    }

    return GhosttyConfigSelection(path: nil, source: "Ghostty default loader")
  }

  private func logGhosttyConfigStartup() {
    /**
     CDXC:NativeTerminals 2026-04-26-07:12
     User Ghostty configuration must be diagnosable without noisy runtime
     traces. Log one startup snapshot with the selected config path,
     resource availability, representative loaded values, and diagnostics.
     */
    let resourcePath = Bundle.main.resourceURL?.appendingPathComponent("ghostty").path
    let themesPath = Bundle.main.resourceURL?.appendingPathComponent("ghostty/themes").path
    let fileManager = FileManager.default
    let configPath = ghosttyConfigSelection.path ?? "<default>"
    let configExists =
      ghosttyConfigSelection.path.map { fileManager.fileExists(atPath: $0) } ?? false
    let resourceExists = resourcePath.map { fileManager.fileExists(atPath: $0) } ?? false
    let themesExists = themesPath.map { fileManager.fileExists(atPath: $0) } ?? false
    let fontSize = ghosttyConfigFloat("font-size").map { String($0) } ?? "<unreadable>"
    let cursorStyle = ghosttyConfigString("cursor-style") ?? "<unreadable>"
    let background = ghosttyConfigColorHex("background") ?? "<unreadable>"
    let diagnostics =
      ghostty.config.errors.isEmpty ? "none" : ghostty.config.errors.joined(separator: " | ")
    let logFields = [
      "source=\(ghosttyConfigSelection.source)",
      "configPath=\(configPath)",
      "configExists=\(configExists)",
      "resourcePath=\(resourcePath ?? "<missing>")",
      "resourceExists=\(resourceExists)",
      "themesExists=\(themesExists)",
      "font-size=\(fontSize)",
      "cursor-style=\(cursorStyle)",
      "background=\(background)",
      "diagnostics=\(diagnostics)",
    ]
    Self.appendGhosttyConfigLog(logFields.joined(separator: " "))
  }

  private func ghosttyConfigString(_ key: String) -> String? {
    guard let config = ghostty.config.config else {
      return nil
    }
    var value: UnsafePointer<Int8>?
    guard ghostty_config_get(config, &value, key, UInt(key.lengthOfBytes(using: .utf8))),
      let value
    else {
      return nil
    }
    return String(cString: value)
  }

  private func ghosttyConfigFloat(_ key: String) -> Float32? {
    guard let config = ghostty.config.config else {
      return nil
    }
    var value: Float32 = 0
    guard ghostty_config_get(config, &value, key, UInt(key.lengthOfBytes(using: .utf8))) else {
      return nil
    }
    return value
  }

  private func ghosttyConfigColorHex(_ key: String) -> String? {
    guard let config = ghostty.config.config else {
      return nil
    }
    var color = ghostty_config_color_s()
    guard ghostty_config_get(config, &color, key, UInt(key.lengthOfBytes(using: .utf8))) else {
      return nil
    }
    return String(format: "#%02X%02X%02X", color.r, color.g, color.b)
  }

  private static func appendGhosttyConfigLog(_ message: String) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("native-ghostty-config.log")
    appendLogLine(
      message, to: logURL, logsDirectory: logsDirectory, label: "Ghostty config startup")
  }

  fileprivate static func appendSessionTitleDebugLog(
    event: String, details: String?, force: Bool = false
  ) {
    /**
     CDXC:SessionTitleDiagnostics 2026-04-26-08:03
     The native packaged app must write session-title diagnostics into the
     same app storage logs location as the Bun controller so missing Codex
     auto-renames can be correlated with native Ghostty title events.

     CDXC:SessionTitleSync 2026-05-08-09:09
     Forced session-title entries record Codex title-generation failures even
     when debugging mode is disabled. Those failures must persist to the
     session-title log instead of interrupting the user with a native alert.
     */
    guard force || NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("session-title-sync-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(message, to: logURL, logsDirectory: logsDirectory, label: "session title debug")
  }

  fileprivate static func appendAgentDetectionDebugLog(event: String, details: String?) {
    /**
     CDXC:AgentDetection 2026-04-26-11:14
     Agent-icon debugging needs a dedicated app storage logs file so native
     title events, detector output, and sidebar projection can be correlated
     without mixing them with session rename diagnostics.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("agent-detection-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(message, to: logURL, logsDirectory: logsDirectory, label: "agent detection debug")
  }

  fileprivate static func appendTerminalFocusDebugLog(
    event: String, details: String?, force: Bool = false
  ) {
    TerminalFocusDebugLog.append(
      event: event,
      details: [
        "details": nullableLogString(details),
        "source": "native-sidebar",
      ],
      force: force)
  }

  fileprivate static func appendRestoreDebugLog(event: String, details: String?) {
    /**
     CDXC:WorkspaceRestore 2026-04-26-10:00
     The packaged native sidebar owns workspace/session persistence. Write
     restore diagnostics into a dedicated app storage logs file so project load,
     localStorage persistence, and native terminal recreation can be traced
     independently from session-title logs.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("workspace-restore-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(
      message, to: logURL, logsDirectory: logsDirectory, label: "workspace restore debug")
  }

  fileprivate static func appendSidebarRefreshDebugLog(event: String, details: String?) {
    SidebarRefreshDebugLog.append(event: event, details: details)
  }

  fileprivate static func appendWorkspaceDockIndicatorDebugLog(event: String, details: String?) {
    /**
     CDXC:WorkspaceDock 2026-04-27-04:23
     Native workspace rail indicator repros need a dedicated log file under
     app storage logs because this UI is rendered from the native sidebar webview,
     not the older Electrobun mainview dock.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("workspace-dock-indicator-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(
      message, to: logURL, logsDirectory: logsDirectory, label: "workspace dock indicator debug")
  }

  fileprivate static func appendAppModalErrorLog(area: String, message: String, stack: String?) {
    /**
     CDXC:AppModals 2026-04-27-14:25
     Full-window modal failures must be persisted outside React debug mode.
     Every modal host exception writes an area-tagged timestamped line under
     app storage logs so missing bridge, render, and command routing failures can
     be diagnosed after the UI has already failed.
     */
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("app-modal-errors.log")
    let stackText = stack.map { " stack=\($0)" } ?? ""
    appendLogLine(
      "[\(area)] \(message)\(stackText)", to: logURL, logsDirectory: logsDirectory,
      label: "app modal error")
  }

  fileprivate static func appendNativeHostLifecycleLog(_ message: String) {
    /**
     CDXC:CrashDiagnostics 2026-04-27-17:38
     When the app disappears from the Dock, native lifecycle breadcrumbs must
     survive outside WebKit and JS logs so close-button, last-window, and
     termination paths can be separated from renderer crashes.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("native-host-lifecycle.log")
    appendLogLine(message, to: logURL, logsDirectory: logsDirectory, label: "native host lifecycle")
  }

  fileprivate static func persistSharedSidebarStorage(_ command: PersistSharedSidebarStorage) {
    do {
      try GhostexAppStorage.persistSharedSidebarStorage(
        key: command.key, payloadJson: command.payloadJson)
    } catch {
      appendRestoreDebugLog(
        event: "nativeSidebar.sharedStorage.persistFailed",
        details: jsonObjectString([
          "error": error.localizedDescription,
          "key": command.key,
        ]))
    }
  }

  private static func appendLogLine(
    _ message: String,
    to logURL: URL,
    logsDirectory: URL,
    label: String
  ) {
    /**
     CDXC:Diagnostics 2026-04-29-09:16
     Native logging can be called from title/focus paths. Reuse timestamp
     formatting and avoid recreating the logs directory on every append so
     enabled diagnostics do not become the app's hot path.
     */
    let line = "[\(logDateFormatter.string(from: Date()))] \(message)\n"

    do {
      if !createdLogDirectories.contains(logsDirectory.path) {
        try FileManager.default.createDirectory(at: logsDirectory, withIntermediateDirectories: true)
        createdLogDirectories.insert(logsDirectory.path)
      }
      if FileManager.default.fileExists(atPath: logURL.path) {
        let handle = try FileHandle(forWritingTo: logURL)
        try handle.seekToEnd()
        if let data = line.data(using: .utf8) {
          try handle.write(contentsOf: data)
        }
        try handle.close()
      } else {
        try line.write(to: logURL, atomically: true, encoding: .utf8)
      }
    } catch {
      logger.warning("failed to write \(label) log: \(error.localizedDescription)")
    }
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    /**
     CDXC:CrashDiagnostics 2026-04-27-18:31
     The native host should terminate after its last window closes, and this
     delegate decision must be an explicit Bool return so Swift compilation
     cannot depend on expression-style behavior that methods do not support.
     */
    Self.appendNativeHostLifecycleLog("applicationShouldTerminateAfterLastWindowClosed result=true")
    return true
  }

  func windowWillClose(_ notification: Notification) {
    persistMainWindowChrome()
    (window?.contentView as? ghostexRootView)?.persistNativeChromeForAppLifecycle()
    Self.appendNativeHostLifecycleLog(
      "windowWillClose title=\(window?.title ?? "<missing>") visibleBeforeClose=\(window?.isVisible ?? false)"
    )
  }

  func windowDidResize(_ notification: Notification) {
    persistMainWindowChrome()
    if let window {
      scheduleMainWindowTrafficLightPositioning(on: window)
    }
    /**
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     Main-window resize changes the frame of every surfaced terminal pane without using TerminalWorkspaceView's split resize handlers.
     Ask the workspace to run its trailing surfaced-only zmx viewport refresh after AppKit resize settles.
     */
    workspaceView?.scheduleZmxPersistenceRefreshForSurfacedTerminalsAfterResize(reason: "mainWindowResize")
  }

  func windowDidMove(_ notification: Notification) {
    persistMainWindowChrome()
  }

  func windowDidBecomeKey(_ notification: Notification) {
    if let window {
      scheduleMainWindowTrafficLightPositioning(on: window)
    }
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-20:09:
     App active and key-window transitions can differ during focus-steal repros. Log key/main window changes independently from application activation so the next incident shows whether Ghostex became frontmost before, after, or without the main terminal window becoming key.
     */
    Self.appendNativeHostLifecycleLog(
      "windowDidBecomeKey windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) mainWindow=\(window?.isMainWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.window.didBecomeKey")
  }

  func windowDidResignKey(_ notification: Notification) {
    Self.appendNativeHostLifecycleLog(
      "windowDidResignKey windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) mainWindow=\(window?.isMainWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.window.didResignKey")
  }

  func windowDidBecomeMain(_ notification: Notification) {
    if let window {
      scheduleMainWindowTrafficLightPositioning(on: window)
    }
    Self.appendNativeHostLifecycleLog(
      "windowDidBecomeMain windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) mainWindow=\(window?.isMainWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.window.didBecomeMain")
  }

  func windowDidResignMain(_ notification: Notification) {
    Self.appendNativeHostLifecycleLog(
      "windowDidResignMain windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) mainWindow=\(window?.isMainWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.window.didResignMain")
  }

  func performGhosttyBindingMenuKeyEquivalent(with event: NSEvent) -> Bool {
    NSApp.mainMenu?.performKeyEquivalent(with: event) ?? false
  }

  @MainActor
  private func installAppHotkeyEventMonitor() {
    if let appHotkeyEventMonitor {
      NSEvent.removeMonitor(appHotkeyEventMonitor)
    }
    /**
     CDXC:Hotkeys 2026-05-15-11:24:
     Next Tab and Previous Tab must keep working after the first navigation
     moves focus from the sidebar into a native terminal or embedded browser.
     Match configured ghostex hotkeys at the app event boundary before focused
     terminal/CEF surfaces can consume Cmd+Tab or Cmd+Shift+Tab, while
     handleHotkeyEquivalent still lets sidebar/settings web chrome own recorder
     and editable-field shortcuts.

     CDXC:Hotkeys 2026-05-15-11:48:
     AppDelegate installs the app-wide monitor, but the hotkey matcher belongs
     to ghostexRootView because it owns sidebar/modal first-responder checks and
     host-event dispatch. Resolve the live root view instead of duplicating that
     behavior on the delegate.
     */
    appHotkeyEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
      [weak self] event in
      guard let root = self?.window?.contentView as? ghostexRootView else {
        return event
      }
      return root.handleHotkeyEquivalent(event) ? nil : event
    }
  }

  @MainActor
  private func installMainMenu() {
    /**
     CDXC:MacMenuBar 2026-05-02-06:36
     The native ghostex app should expose a standard macOS application menu like
     other desktop apps: About, Check for Updates, Settings, Services, Hide,
     Hide Others, and Quit. Build the menu explicitly because this AppKit host
     runs without a storyboard or nib-provided main menu.
     */
    let appName = Self.appMenuName()
    let mainMenu = NSMenu(title: "Main Menu")

    let appMenuItem = NSMenuItem()
    let appMenu = NSMenu(title: appName)
    appMenu.addItem(
      withTitle: "About \(appName)",
      action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
      keyEquivalent: "")
    appMenu.addItem(
      withTitle: "Check for Updates",
      action: #selector(checkForUpdates(_:)),
      keyEquivalent: "")
    appMenu.addItem(NSMenuItem.separator())
    appMenu.addItem(
      withTitle: "Settings...",
      action: #selector(openSettingsFromMainMenu(_:)),
      keyEquivalent: ",")
    appMenu.addItem(NSMenuItem.separator())

    let servicesItem = NSMenuItem(title: "Services", action: nil, keyEquivalent: "")
    let servicesMenu = NSMenu(title: "Services")
    servicesItem.submenu = servicesMenu
    appMenu.addItem(servicesItem)
    NSApp.servicesMenu = servicesMenu

    appMenu.addItem(NSMenuItem.separator())
    appMenu.addItem(
      withTitle: "Hide \(appName)",
      action: #selector(NSApplication.hide(_:)),
      keyEquivalent: "h")
    let hideOthersItem = appMenu.addItem(
      withTitle: "Hide Others",
      action: #selector(NSApplication.hideOtherApplications(_:)),
      keyEquivalent: "h")
    hideOthersItem.keyEquivalentModifierMask = [.command, .option]
    appMenu.addItem(NSMenuItem.separator())
    appMenu.addItem(
      withTitle: "Quit \(appName)",
      action: #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q")
    appMenuItem.submenu = appMenu
    mainMenu.addItem(appMenuItem)

    mainMenu.addItem(makeFileMenu())
    mainMenu.addItem(Self.makeEditMenu())
    mainMenu.addItem(Self.makeViewMenu())
    mainMenu.addItem(Self.makeWindowMenu())
    mainMenu.addItem(Self.makeHelpMenu())
    NSApp.mainMenu = mainMenu
  }

  @MainActor
  private static func appMenuName() -> String {
    let bundle = Bundle.main
    let name =
      bundle.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
      ?? bundle.object(forInfoDictionaryKey: "CFBundleName") as? String
      ?? ProcessInfo.processInfo.processName
    return name.isEmpty ? "Ghostex" : name
  }

  @MainActor
  private func makeFileMenu() -> NSMenuItem {
    let menuItem = NSMenuItem()
    let menu = NSMenu(title: "File")
    /**
     CDXC:MacMenuBar 2026-05-10-11:56
     Cmd-W is a pane/session close shortcut in ghostex, matching browser-tab and
     Ghostty-pane expectations. Route the File menu item to the focused workspace
     surface so the top-level app window is not closed by a normal close hotkey.
     */
    let closePaneItem = menu.addItem(
      withTitle: "Close Pane",
      action: #selector(closeFocusedSessionFromMainMenu(_:)),
      keyEquivalent: "w")
    closePaneItem.target = self
    menuItem.submenu = menu
    return menuItem
  }

  @MainActor
  private static func makeEditMenu() -> NSMenuItem {
    let menuItem = NSMenuItem()
    let menu = NSMenu(title: "Edit")
    menu.addItem(
      withTitle: "Undo",
      action: Selector(("undo:")),
      keyEquivalent: "z")
    menu.addItem(
      withTitle: "Redo",
      action: Selector(("redo:")),
      keyEquivalent: "Z")
    menu.addItem(NSMenuItem.separator())
    menu.addItem(
      withTitle: "Cut",
      action: #selector(NSText.cut(_:)),
      keyEquivalent: "x")
    menu.addItem(
      withTitle: "Copy",
      action: #selector(NSText.copy(_:)),
      keyEquivalent: "c")
    menu.addItem(
      withTitle: "Paste",
      action: #selector(NSText.paste(_:)),
      keyEquivalent: "v")
    menu.addItem(
      withTitle: "Select All",
      action: #selector(NSText.selectAll(_:)),
      keyEquivalent: "a")
    menuItem.submenu = menu
    return menuItem
  }

  @MainActor
  private static func makeViewMenu() -> NSMenuItem {
    let menuItem = NSMenuItem()
    menuItem.submenu = NSMenu(title: "View")
    return menuItem
  }

  @MainActor
  private static func makeWindowMenu() -> NSMenuItem {
    let menuItem = NSMenuItem()
    let menu = NSMenu(title: "Window")
    menu.addItem(
      withTitle: "Minimize",
      action: #selector(NSWindow.performMiniaturize(_:)),
      keyEquivalent: "m")
    menu.addItem(
      withTitle: "Zoom",
      action: #selector(NSWindow.performZoom(_:)),
      keyEquivalent: "")
    menu.addItem(NSMenuItem.separator())
    menu.addItem(
      withTitle: "Bring All to Front",
      action: #selector(NSApplication.arrangeInFront(_:)),
      keyEquivalent: "")
    NSApp.windowsMenu = menu
    menuItem.submenu = menu
    return menuItem
  }

  @MainActor
  private static func makeHelpMenu() -> NSMenuItem {
    let menuItem = NSMenuItem()
    menuItem.submenu = NSMenu(title: "Help")
    return menuItem
  }

  @objc @MainActor private func openSettingsFromMainMenu(_ sender: Any?) {
    guard let root = window?.contentView as? ghostexRootView else {
      return
    }
    recordNativeActivationRequest(reason: "mainMenu.openSettings")
    NSApp.activate(ignoringOtherApps: true)
    /**
     CDXC:MacMenuBar 2026-05-02-06:36
     The Settings menu item must open the existing React settings modal rather
     than maintaining a separate AppKit settings surface. Dispatch the typed
     native hotkey event so menu selection, configured shortcuts, and sidebar
     actions share one implementation path.
     */
    root.postHostEvent(.nativeHotkey(actionId: "openSettings"))
  }

  @objc @MainActor private func closeFocusedSessionFromMainMenu(_ sender: Any?) {
    guard workspaceView?.closeFocusedSession(reason: "mainMenuClosePane") == true else {
      NSSound.beep()
      return
    }
  }

  @MainActor
  private func startSparkleBackgroundUpdateCheck() {
    /**
     CDXC:AutoUpdate 2026-05-02-06:51
     Automatic update checks should honor Sparkle's persisted user preference.
     When enabled, ask Sparkle to check in the background at launch instead of
     implementing a parallel polling path in ghostex.
     */
    if updaterController.updater.automaticallyChecksForUpdates {
      updaterController.updater.checkForUpdatesInBackground()
    }
  }

  @IBAction func checkForUpdates(_ sender: Any?) {
    updaterController.updater.checkForUpdates()
  }

  @IBAction nonisolated func closeAllWindows(_ sender: Any?) {}

  @IBAction nonisolated func toggleQuickTerminal(_ sender: Any?) {}

  nonisolated func toggleVisibility(_ sender: Any?) {}

  nonisolated func syncFloatOnTopMenu(_ window: NSWindow) {}

  nonisolated func setSecureInput(_ mode: Ghostty.SetSecureInput) {}

  @MainActor
  private func makeWindow() {
    let sessionStatusIndicatorController = SessionStatusIndicatorController(
      onActivationRequest: { [weak self] reason in
        self?.recordNativeActivationRequest(reason: reason)
      },
      onClick: { [weak self] status in
        self?.handleSessionStatusIndicatorClick(status)
      })
    self.sessionStatusIndicatorController = sessionStatusIndicatorController
    let petOverlayController = PetOverlayController(
      onActivityClick: { [weak self] projectId, sessionId in
        Task { @MainActor in
          self?.handlePetOverlayActivityClick(projectId: projectId, sessionId: sessionId)
        }
      },
      onGoToGhostex: { [weak self] in
        Task { @MainActor in
          self?.handlePetOverlayGoToGhostex()
        }
      },
      onStatusClick: { [weak self] status in
        Task { @MainActor in
          /**
           CDXC:PetOverlay 2026-05-21-02:19:
           Collapsed pet status badges must behave exactly like the floating
           status indicator badges: raise Ghostex, record the native activation,
           and let the sidebar choose the matching aggregate session target.
           */
          self?.recordNativeActivationRequest(
            reason: "petOverlayStatusIndicatorClick.\(status.rawValue)")
          NSApp.activate(ignoringOtherApps: true)
          self?.handleSessionStatusIndicatorClick(status)
        }
      },
      onSleepPet: { [weak self] in
        Task { @MainActor in
          (self?.window?.contentView as? ghostexRootView)?.sleepPetOverlayFromPet()
        }
      })
    self.petOverlayController = petOverlayController
    petOverlayController.load(webAssets: ghostexRootView.resolveWebAssets())
    let root = ghostexRootView(
      ghostty: ghostty,
      sendEvent: { [weak self] event in
        self?.bridge?.send(event)
        (self?.window?.contentView as? ghostexRootView)?.postHostEvent(event)
      },
      configureZedOverlay: { [weak self] command in
        self?.handle(.configureZedOverlay(command))
      },
      syncGhosttyTerminalSettings: { [weak self] command in
        self?.handle(.syncGhosttyTerminalSettings(command))
      },
      applyGhosttyConfigSettings: { [weak self] command in
        self?.handle(.applyGhosttyConfigSettings(command))
      },
      openGhosttyConfigFile: { [weak self] in
        self?.handle(.openGhosttyConfigFile)
      },
      openAccessibilityPreferences: { [weak self] in
        self?.handle(.openAccessibilityPreferences)
      },
      openBrowserWindow: { [weak self] command in
        self?.handle(.openBrowserWindow(command))
      },
      openZedWorkspace: { [weak self] command in
        self?.handle(.openZedWorkspace(command))
      },
      openWorkspaceInFinder: { [weak self] command in
        self?.handle(.openWorkspaceInFinder(command))
      },
      openWorkspaceInIde: { [weak self] command in
        self?.handle(.openWorkspaceInIde(command))
      },
      showBrowserWindow: { [weak self] in
        self?.handle(.showBrowserWindow)
      },
      setAppTitlebarTitle: { [weak self] title in
        self?.updateAppTitlebarTitle(title)
      },
      setSessionStatusIndicators: { [weak sessionStatusIndicatorController] command in
        sessionStatusIndicatorController?.apply(command)
      },
      setPetOverlayState: { [weak petOverlayController] command in
        petOverlayController?.apply(command)
      }
    )
    workspaceView = root.workspaceView

    let initialWindowFrame = restoredInitialWindowFrame()
    let windowStyleMask: NSWindow.StyleMask = [
      .closable, .fullSizeContentView, .miniaturizable, .resizable, .titled,
    ]
    /**
     CDXC:NativeWindowChrome 2026-05-07-08:17
     Persisted placement stores the outer NSWindow frame because that is the
     size and position users see. NSWindow initializers take a content rect, so
     convert here instead of treating the saved frame as content dimensions.
     */
    let initialContentRect = NSWindow.contentRect(
      forFrameRect: initialWindowFrame,
      styleMask: windowStyleMask)
    let window = ghostexFocusReportingWindow(
      contentRect: initialContentRect,
      styleMask: windowStyleMask,
      backing: .buffered,
      defer: false
    )
    window.onFirstResponderChanged = { [weak root] responder in
      root?.workspaceView.windowFirstResponderChanged(responder, reason: "windowMakeFirstResponder")
    }
    window.onKeyDownDispatch = { [weak root] event in
      root?.workspaceView.windowKeyDownDispatch(event)
    }
    window.onKeyEquivalent = { [weak root] event in
      root?.handleHotkeyEquivalent(event) ?? false
    }
    window.onActivationBoundaryEvent = { [weak self, weak root] event, phase in
      if phase == "windowSendEvent.beforeSuper" {
        root?.handleWindowMouseDownBeforeDispatch(event)
      }
      self?.logNativeActivationBoundaryInputEvent(event, phase: phase)
    }
    window.title = "Ghostex"
    window.titleVisibility = .hidden
    window.titlebarAppearsTransparent = true
    window.isMovableByWindowBackground = false
    window.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor
    window.contentView = root
    window.delegate = self
    self.window = window
    scheduleMainWindowTrafficLightPositioning(on: window)
    window.makeKeyAndOrderFront(nil)
    scheduleMainWindowTrafficLightPositioning(on: window)
    let zedOverlayController = ZedOverlayController(
      window: window,
      initialWindowSize: initialWindowFrame.size,
      willHideAttachment: { [weak self] visibleFrame in
        self?.isMainWindowHiddenByIdeAttachment = true
        self?.lastVisibleMainWindowFrameForPersistence = visibleFrame
        self?.persistMainWindowChrome()
      },
      didActivateAttachment: { [weak self] in
        self?.browserOverlayController?.markBrowserNoLongerShownInAttachment(
          reason: "ghostexActivated"
        )
      },
      didHideAttachment: { [weak self] in
        self?.browserOverlayController?.logAttachmentEvent(
          "appDelegate.didHideAttachment.beforeMoveBrowser")
        self?.browserOverlayController?.moveBrowserOffscreen()
        self?.browserOverlayController?.logAttachmentEvent(
          "appDelegate.didHideAttachment.afterMoveBrowser")
      },
      didShowAttachment: { [weak self] in
        self?.isMainWindowHiddenByIdeAttachment = false
        self?.persistMainWindowChrome()
        self?.browserOverlayController?.logAttachmentEvent(
          "appDelegate.didShowAttachment.beforeRestoreBrowser")
        self?.browserOverlayController?.restoreBrowserIfNeeded()
        self?.browserOverlayController?.logAttachmentEvent(
          "appDelegate.didShowAttachment.afterRestoreBrowser")
      },
      didRequestDetach: { [weak self] targetApp in
        self?.detachZedOverlayFromNativeButton(targetApp: targetApp)
      }
    )
    self.zedOverlayController = zedOverlayController
    self.browserOverlayController = BrowserOverlayController(
      window: window,
      workareaFrameProvider: { [weak root] in
        root?.workspaceScreenFrame()
      },
      setCompanionBrowserActive: { [weak zedOverlayController] active in
        zedOverlayController?.setCompanionApplicationBundleIdentifiers(
          active ? [BrowserOverlayController.chromeCanaryBundleIdentifier] : []
        )
      }
    )
    if let pendingZedOverlayConfiguration {
      zedOverlayController.configure(pendingZedOverlayConfiguration)
      updateAttachToIdeTitlebarButton(
        enabled: pendingZedOverlayConfiguration.enabled,
        hideTitlebarButton: pendingZedOverlayConfiguration.hideTitlebarButton ?? false,
        targetApp: pendingZedOverlayConfiguration.targetApp
      )
      self.pendingZedOverlayConfiguration = nil
    } else if let initialZedOverlayConfiguration = initialZedOverlayConfiguration() {
      zedOverlayController.configure(initialZedOverlayConfiguration)
      updateAttachToIdeTitlebarButton(
        enabled: initialZedOverlayConfiguration.enabled,
        hideTitlebarButton: initialZedOverlayConfiguration.hideTitlebarButton ?? false,
        targetApp: initialZedOverlayConfiguration.targetApp
      )
    }
    recordNativeActivationRequest(reason: "startup.makeWindow")
    NSApp.activate(ignoringOtherApps: true)
    scheduleMainWindowTrafficLightPositioning(on: window)
  }

  @MainActor
  private func handleSessionStatusIndicatorClick(_ status: NativeSessionStatusIndicatorStatus) {
    /**
     CDXC:SessionStatusIndicators 2026-05-05-19:47
     Clicking a status indicator badge should raise ghostex and ask the sidebar to
     choose the live matching session. Keep click routing on the typed native
     host event bus so AppKit chrome and webview/sidebar state stay decoupled.
     CDXC:SessionStatusIndicators 2026-05-09-17:30
     Floating and menu bar badges intentionally share this event so visibility
     settings do not fork green attention or orange working navigation behavior.
     */
    let event = HostEvent.sessionStatusIndicatorClicked(status: status)
    window?.makeKeyAndOrderFront(nil)
    bridge?.send(event)
    (window?.contentView as? ghostexRootView)?.postHostEvent(event)
  }

  @MainActor
  private func handlePetOverlayActivityClick(projectId: String, sessionId: String) {
    /**
     CDXC:PetOverlay 2026-05-14-10:23:
     Clicking the message above the pet should open ghostex and focus the exact
     shown session. The overlay supplies project/session ids, then AppKit raises
     the main window before the sidebar applies the usual focus mutation.
     */
    let event = HostEvent.petOverlayActivityClicked(projectId: projectId, sessionId: sessionId)
    recordNativeActivationRequest(reason: "petOverlayActivityClick", sessionId: sessionId)
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
    bridge?.send(event)
    (window?.contentView as? ghostexRootView)?.postHostEvent(event)
  }

  @MainActor
  private func handlePetOverlayGoToGhostex() {
    /**
     CDXC:PetOverlay 2026-05-21-14:59:
     The pet context menu's Go to Ghostex item should reverse both macOS hide and
     minimize states before raising the main window. This is a pure app activation
     action, not a session-selection event, so it does not send a sidebar host
     event after bringing Ghostex forward.
     */
    recordNativeActivationRequest(reason: "petOverlayContextMenu.goToGhostex")
    NSApp.unhide(nil)
    if window?.isMiniaturized == true {
      window?.deminiaturize(nil)
    }
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
    window?.orderFrontRegardless()
  }

  @MainActor
  private func handleSessionAttentionNotificationClick(_ sessionId: String) {
    /**
     CDXC:SessionAttentionNotifications 2026-05-10-16:46
     A clicked notification should raise ghostex before the sidebar focuses the
     target session, otherwise AppKit may select the pane without making it the
     first responder for immediate typing.
     */
    let event = HostEvent.sessionAttentionNotificationClicked(sessionId: sessionId)
    recordNativeActivationRequest(reason: "sessionAttentionNotificationClick", sessionId: sessionId)
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
    bridge?.send(event)
    (window?.contentView as? ghostexRootView)?.postHostEvent(event)
  }

  private func restoredInitialWindowFrame() -> NSRect {
    /**
     CDXC:NativeWindowChrome 2026-05-07-08:17
     Startup must restore the exact main-window size, position, and display
     from the previous close. Use the saved screen identifier plus the saved
     screen-relative origin so a re-ordered multi-monitor layout still opens
     ghostex on the same physical display instead of the primary display.
     */
    let stored = nativeSettingsStore.readMainWindowChrome()
    if let restoredFrame = Self.restoredMainWindowFrame(from: stored) {
      return restoredFrame
    }

    let size = CGSize(width: max(stored.width ?? 1440, 320), height: max(stored.height ?? 900, 240))
    return Self.defaultInitialWindowFrame(size: size)
  }

  private static func restoredMainWindowFrame(from stored: NativeMainWindowChromeSettings)
    -> NSRect?
  {
    guard let storedFrame = stored.frame else {
      return nil
    }
    let size = CGSize(width: max(storedFrame.width, 320), height: max(storedFrame.height, 240))
    if let screen = screen(matchingIdentifier: stored.screenID),
      let storedScreenFrame = stored.screenFrame
    {
      return NSRect(
        x: screen.frame.minX + (storedFrame.minX - storedScreenFrame.minX),
        y: screen.frame.minY + (storedFrame.minY - storedScreenFrame.minY),
        width: size.width,
        height: size.height)
    }
    if screen(containingLargestVisibleAreaOf: storedFrame) != nil {
      return NSRect(origin: storedFrame.origin, size: size)
    }
    return nil
  }

  private static func defaultInitialWindowFrame(size: CGSize) -> NSRect {
    let screenFrame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
    let width = size.width
    let height = size.height
    let x = screenFrame.minX + min(100, max(0, screenFrame.width - width))
    let y = screenFrame.minY + min(80, max(0, screenFrame.height - height))
    return NSRect(x: x, y: y, width: width, height: height)
  }

  private func persistMainWindowChrome() {
    guard let window else {
      return
    }
    let frame = window.frame
    /**
     CDXC:NativeWindowChrome 2026-05-07-08:17
     IDE attachment can tuck the real NSWindow offscreen while the user still
     thinks of the prior visible window frame as ghostex's location. Persist that
     last visible frame during hidden attachment states so launch restore never
     reopens at an intentional offscreen helper coordinate.
     */
    let frameForPersistence =
      isMainWindowHiddenByIdeAttachment
      ? lastVisibleMainWindowFrameForPersistence ?? frame
      : frame
    guard let screen = Self.screen(containingLargestVisibleAreaOf: frameForPersistence) else {
      return
    }
    lastVisibleMainWindowFrameForPersistence = frameForPersistence
    nativeSettingsStore.persistMainWindowChrome(frame: frameForPersistence, screen: screen)
  }

  private func positionMainWindowTrafficLightButtons(on window: NSWindow) {
    /**
     CDXC:NativeWindowChrome 2026-05-25-07:22:
     The macOS traffic-light buttons should be positioned from the custom 35px titlebar center, then pushed 5px visually lower. Compute the absolute frame on every AppKit relayout so close/minimize/zoom do not snap back to AppKit's default 30px placement.
     */
    for buttonType in Self.standardWindowButtonTypes {
      guard let button = window.standardWindowButton(buttonType), let titlebarView = button.superview else {
        continue
      }
      var frame = button.frame
      if titlebarView.isFlipped {
        frame.origin.y =
          (ghostexAppTitlebarHeight - frame.height) / 2
          + ghostexTrafficLightVisualDownOffset
      } else {
        frame.origin.y =
          titlebarView.bounds.height - ((ghostexAppTitlebarHeight + frame.height) / 2)
          - ghostexTrafficLightVisualDownOffset
      }
      button.frame = frame
    }
  }

  private func scheduleMainWindowTrafficLightPositioning(on window: NSWindow) {
    /**
     CDXC:NativeWindowChrome 2026-05-25-07:22:
     AppKit can restore standard window-button frames during activation and resize layout passes after the custom titlebar is already visible. Reapply the 35px-titlebar positioning plus 5px visual-down offset at the end of those passes so the final on-screen traffic lights remain in the requested spot.
     */
    positionMainWindowTrafficLightButtons(on: window)
    DispatchQueue.main.async { [weak self, weak window] in
      guard let self, let window, self.window === window else {
        return
      }
      self.positionMainWindowTrafficLightButtons(on: window)
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self, weak window] in
      guard let self, let window, self.window === window else {
        return
      }
      self.positionMainWindowTrafficLightButtons(on: window)
    }
  }

  private static func screen(matchingIdentifier identifier: UInt32?) -> NSScreen? {
    guard let identifier else {
      return nil
    }
    return NSScreen.screens.first { screenIdentifier($0) == identifier }
  }

  private static func screen(containingLargestVisibleAreaOf frame: NSRect) -> NSScreen? {
    let candidates = NSScreen.screens
      .map { screen -> (screen: NSScreen, area: CGFloat) in
        let intersection = screen.frame.intersection(frame)
        return (screen, max(0, intersection.width) * max(0, intersection.height))
      }
      .filter { $0.area > 0 }
      .sorted { lhs, rhs in lhs.area > rhs.area }
    return candidates.first?.screen
  }

  fileprivate static func screenIdentifier(_ screen: NSScreen?) -> UInt32? {
    guard
      let number = screen?.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
    else {
      return nil
    }
    return number.uint32Value
  }

  @MainActor private func installAppTitlebarLabel(on window: NSWindow) {
    /**
     CDXC:NativeWindowChrome 2026-05-10-14:19
     Users need the outer macOS title bar to show the active code project. Keep
     this as a custom left title item because the centered native title slot is
     already used by the Attach/Detach IDE control.
     */
    guard let titlebarView = window.standardWindowButton(.closeButton)?.superview else {
      return
    }
    let label = NSTextField(labelWithString: window.title)
    label.font = .systemFont(ofSize: 12, weight: .semibold)
    label.textColor = NSColor(calibratedWhite: 0.88, alpha: 1)
    label.lineBreakMode = .byTruncatingTail
    label.toolTip = window.title
    label.translatesAutoresizingMaskIntoConstraints = false
    titlebarView.addSubview(label)

    let centerYAnchor =
      window.standardWindowButton(.closeButton)?.centerYAnchor ?? titlebarView.centerYAnchor
    let leadingAnchor = window.standardWindowButton(.zoomButton)?.trailingAnchor
      ?? window.standardWindowButton(.miniaturizeButton)?.trailingAnchor
      ?? window.standardWindowButton(.closeButton)?.trailingAnchor
      ?? titlebarView.leadingAnchor
    NSLayoutConstraint.activate([
      label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
      label.centerYAnchor.constraint(equalTo: centerYAnchor),
      label.widthAnchor.constraint(lessThanOrEqualToConstant: 260),
    ])
    appTitlebarLabel = label
  }

  @MainActor private func updateAppTitlebarTitle(_ title: String?) {
    let normalizedTitle = normalizedAppTitlebarTitle(title)
    window?.title = normalizedTitle
    appTitlebarLabel?.stringValue = normalizedTitle
    appTitlebarLabel?.toolTip = normalizedTitle
  }

  private func normalizedAppTitlebarTitle(_ title: String?) -> String {
    let normalizedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return normalizedTitle.isEmpty ? "Ghostex" : normalizedTitle
  }

  @MainActor private func installAttachToIdeTitlebarButton(on window: NSWindow) {
    /**
     CDXC:IDEAttachment 2026-04-27-00:54
     The attach action belongs at the center of the native title bar and
     should read as a text button, matching the rounded AppKit style of the
     floating Show Ghostex/Show IDE buttons instead of using a blue link icon.
     Its label names the currently selected IDE in the shortest requested
     form and switches between Attach/Detach from the persisted
     attach-enabled state.
     */
    let stored = nativeSettingsStore.readZedOverlay()
    let targetApp = stored.targetApp ?? .zedPreview
    let button = NSButton(
      title: attachToIdeTitlebarButtonTitle(enabled: stored.enabled ?? false, targetApp: targetApp),
      target: self,
      action: #selector(handleAttachToIdeTitlebarButton)
    )
    button.isHidden = stored.hideTitlebarButton ?? false
    button.bezelStyle = .rounded
    button.controlSize = .small
    button.font = .systemFont(ofSize: 12, weight: .semibold)
    button.toolTip = "Attach to IDE"
    button.setButtonType(.momentaryPushIn)
    button.translatesAutoresizingMaskIntoConstraints = false

    guard let titlebarView = window.standardWindowButton(.closeButton)?.superview else {
      return
    }
    titlebarView.addSubview(button)
    let centerYAnchor =
      window.standardWindowButton(.closeButton)?.centerYAnchor ?? titlebarView.centerYAnchor
    NSLayoutConstraint.activate([
      button.centerXAnchor.constraint(equalTo: titlebarView.centerXAnchor),
      button.centerYAnchor.constraint(equalTo: centerYAnchor),
      button.heightAnchor.constraint(equalToConstant: 24),
      button.widthAnchor.constraint(greaterThanOrEqualToConstant: 132),
    ])
    if let appTitlebarLabel {
      appTitlebarLabel.trailingAnchor.constraint(lessThanOrEqualTo: button.leadingAnchor, constant: -12)
        .isActive = true
    }
    attachToIdeTitlebarButton = button
  }

  @objc @MainActor private func handleAttachToIdeTitlebarButton() {
    let stored = nativeSettingsStore.readZedOverlay()
    let targetApp = stored.targetApp ?? .zedPreview
    let nextEnabled = !(stored.enabled ?? false)
    let command = ConfigureZedOverlay(
      enabled: nextEnabled,
      hideTitlebarButton: stored.hideTitlebarButton,
      reason: nil,
      targetApp: targetApp,
      workspacePath: workspacePath
    )
    handle(.configureZedOverlay(command))
    if nextEnabled {
      (window?.contentView as? ghostexRootView)?.applyNativeZedOverlayAttached(targetApp: targetApp)
    } else {
      (window?.contentView as? ghostexRootView)?.applyNativeZedOverlayDetached(targetApp: targetApp)
    }
  }

  @MainActor private func updateAttachToIdeTitlebarButton(
    enabled: Bool,
    hideTitlebarButton: Bool,
    targetApp: ZedOverlayTargetApp
  ) {
    attachToIdeTitlebarButton?.title = attachToIdeTitlebarButtonTitle(
      enabled: enabled,
      targetApp: targetApp
    )
    (window?.contentView as? ghostexRootView)?.applyNativeTitlebarZedOverlay(
      enabled: enabled,
      hideTitlebarButton: hideTitlebarButton,
      targetApp: targetApp
    )
    /**
     CDXC:IDEAttachment 2026-05-01-13:52
     Settings can hide the native title-bar Attach/Detach IDE button without
     changing whether ghostex is attached. Keep the button object installed so the
     setting can show it again immediately without rebuilding title-bar chrome.
     */
    attachToIdeTitlebarButton?.isHidden = hideTitlebarButton
  }

  private func attachToIdeTitlebarButtonTitle(
    enabled: Bool,
    targetApp: ZedOverlayTargetApp
  ) -> String {
    let action = enabled ? "Detach" : "Attach"
    switch targetApp {
    case .zed:
      return "\(action) Zed"
    case .zedPreview:
      return "\(action) Zed"
    case .vscode:
      return "\(action) VS Code"
    case .vscodeInsiders:
      return "\(action) VS Code"
    }
  }

  @MainActor
  private func startBridge() {
    do {
      /**
       CDXC:ChromiumBrowserPanes 2026-05-04-17:06
       CEF browser-pane verification runs the ghostex-dev app beside the installed
       ghostex app. Give the dev bundle a separate CLI bridge port so browser-pane
       creation can be tested without stopping the user's normal ghostex process.
       */
      let bridgePort: UInt16 = Bundle.main.bundleIdentifier == "com.madda.ghostex-dev.host"
        ? 58744
        : 58743
      let bridgeAuthToken = try Self.prepareBridgeAuthToken()
      let bridge = try NativeHostBridge(port: bridgePort, authToken: bridgeAuthToken) { [weak self] command in
        self?.handle(command)
      }
      self.bridge = bridge
      bridge.start()
      Self.appendNativeHostLifecycleLog("nativeHostBridge.started port=\(bridgePort)")
    } catch {
      /**
       CDXC:CliBridgeTransport 2026-05-15-20:03:
       Ctrl+G prompt editing depends on the native CLI bridge. Persist bridge
       startup failures in lifecycle logs instead of only writing into the
       hidden bridge-error terminal so a missing listener can be diagnosed from
       logs after the prompt editor fails to appear.
       */
      Self.appendNativeHostLifecycleLog("nativeHostBridge.failed error=\(error.localizedDescription)")
      workspaceView?.createTerminal(
        CreateTerminal(
          activateOnCreate: true,
          cwd: FileManager.default.currentDirectoryPath,
          diagnosticSource: nil,
	          env: nil,
	          initialInput: "printf 'Failed to start Ghostex bridge: \(error.localizedDescription)\\n'\r",
	          sessionId: "bridge-error",
          sessionPersistenceName: nil,
          sessionPersistenceProvider: nil,
          /**
           CDXC:CliBridgeTransport 2026-05-21-00:56:
           The bridge-error terminal must remain a normal shell session that receives diagnostic initial input, so the explicit shellCommand contract is nil here.
           */
          shellCommand: nil,
          title: "Bridge error",
          tmuxMode: nil,
          tmuxSessionName: nil
        ))
    }
  }

  private static func prepareBridgeAuthToken() throws -> String {
    /**
     CDXC:CliBridgeSecurity 2026-05-15-18:25
     CLI automation still needs a localhost bridge, but browser pages can also
     attempt loopback WebSocket connections. Rotate a per-launch token into the
     app's private CLI directory so trusted local CLI commands can authenticate
     without exposing privileged HostCommand execution to arbitrary web content.
     */
    let token = try makeBridgeAuthToken()
    let fileManager = FileManager.default
    try fileManager.createDirectory(
      at: GhostexAppStorage.cliDirectory,
      withIntermediateDirectories: true,
      attributes: [.posixPermissions: 0o700]
    )
    try? fileManager.setAttributes(
      [.posixPermissions: 0o700],
      ofItemAtPath: GhostexAppStorage.cliDirectory.path
    )
    if fileManager.fileExists(atPath: GhostexAppStorage.cliBridgeTokenURL.path) {
      try fileManager.removeItem(at: GhostexAppStorage.cliBridgeTokenURL)
    }
    guard
      fileManager.createFile(
        atPath: GhostexAppStorage.cliBridgeTokenURL.path,
        contents: Data("\(token)\n".utf8),
        attributes: [.posixPermissions: 0o600]
      )
    else {
      throw NSError(
        domain: "GhostexBridgeAuth",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "Failed to write bridge token."])
    }
    return token
  }

  private static func makeBridgeAuthToken() throws -> String {
    var bytes = [UInt8](repeating: 0, count: 32)
    let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
    guard status == errSecSuccess else {
      throw NSError(
        domain: "GhostexBridgeAuth",
        code: Int(status),
        userInfo: [NSLocalizedDescriptionKey: "Failed to create bridge token."])
    }
    return Data(bytes).base64EncodedString()
  }

  @MainActor
  private func handle(_ command: HostCommand) {
    switch command {
    case .createTerminal(let command):
      workspaceView?.createTerminal(command)
    case .createWebPane(let command):
      workspaceView?.createWebPane(command)
    case .openFloatingEditor(let command):
      if let root = window?.contentView as? ghostexRootView {
        root.openFloatingEditor(command)
      } else {
        workspaceView?.openFloatingEditor(command)
      }
    case .closeTerminal(let command):
      if let root = window?.contentView as? ghostexRootView {
        root.closeTerminal(
          sessionId: command.sessionId,
          preservePersistenceSession: command.preservePersistenceSession == true)
      } else {
        workspaceView?.closeTerminal(
          sessionId: command.sessionId,
          preservePersistenceSession: command.preservePersistenceSession == true)
      }
    case .closeWebPane(let command):
      workspaceView?.closeWebPane(sessionId: command.sessionId)
    case .focusTerminal(let command):
      workspaceView?.focusTerminal(sessionId: command.sessionId)
    case .focusWebPane(let command):
      workspaceView?.focusWebPane(sessionId: command.sessionId)
    case .reloadWebPane(let command):
      workspaceView?.reloadWebPane(sessionId: command.sessionId)
    case .startT3CodeRuntime(let command):
      startT3CodeRuntime(command)
    case .setT3CodeRuntimeSessionState(let command):
      setT3CodeRuntimeSessionState(command, reason: "nativeHost")
    case .stopT3CodeRuntime:
      stopT3CodeRuntime(logPrefix: "nativeHost")
    case .startCodeServerRuntime(let command):
      startCodeServerRuntime(command)
    case .stopCodeServerRuntime:
      stopCodeServerRuntime(logPrefix: "nativeHost")
    case .createProjectEditorPane(let command):
      workspaceView?.createProjectEditorPane(command)
    case .focusProjectEditorPane(let command):
      workspaceView?.focusProjectEditorPane(projectId: command.projectId)
    case .closeProjectEditorPane(let command):
      workspaceView?.closeProjectEditorPane(projectId: command.projectId)
    case .activateApp:
      activateAppWindow()
    case .writeTerminalText(let command):
      workspaceView?.writeTerminalText(sessionId: command.sessionId, text: command.text)
    case .writeTerminalScript(let command):
      workspaceView?.writeTerminalScript(sessionId: command.sessionId, text: command.text)
    case .sendTerminalEnter(let command):
      workspaceView?.sendTerminalEnter(sessionId: command.sessionId)
    case .readTerminalText(let command):
      if let workspaceView {
        workspaceView.readTerminalText(command)
      } else {
        (window?.contentView as? ghostexRootView)?.postHostEvent(
          .terminalTextResult(
            requestId: command.requestId,
            sessionId: command.sessionId,
            ok: false,
            text: nil,
            error: "workspace-view-missing"
          ))
      }
    case .checkPersistenceSession(let command):
      if let workspaceView {
        workspaceView.checkPersistenceSession(command)
      } else {
        (window?.contentView as? ghostexRootView)?.postHostEvent(
          .persistenceSessionState(
            requestId: command.requestId,
            provider: command.provider,
            sessionName: command.sessionName,
            exists: false,
            error: "workspace-view-missing"
          ))
      }
    case .setActiveTerminalSet(let command):
      updateAppTitlebarTitle(command.appTitle)
      (window?.contentView as? ghostexRootView)?.applyReactTitlebarProjectState(command)
      workspaceView?.setActiveTerminalSet(command)
    case .setSessionStatusIndicators(let command):
      sessionStatusIndicatorController?.apply(command)
    case .setPetOverlayState(let command):
      petOverlayController?.apply(command)
    case .showSessionAttentionNotification(let command):
      sessionAttentionNotificationController.show(command)
    case .setTerminalLayout(let command):
      workspaceView?.setTerminalLayout(command.layout)
    case .setTerminalVisibility(let command):
      workspaceView?.setTerminalVisibility(sessionId: command.sessionId, visible: command.visible)
    case .pickWorkspaceFolder:
      break
    case .pickWorkspaceIcon:
      break
    case .showMessage(let command):
      showMessage(command)
    case .appendAgentDetectionDebugLog(let command):
      Self.appendAgentDetectionDebugLog(event: command.event, details: command.details)
    case .appendTerminalFocusDebugLog(let command):
      Self.appendTerminalFocusDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendRestoreDebugLog(let command):
      Self.appendRestoreDebugLog(event: command.event, details: command.details)
    case .appendSessionTitleDebugLog(let command):
      Self.appendSessionTitleDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendSidebarRefreshDebugLog(let command):
      Self.appendSidebarRefreshDebugLog(event: command.event, details: command.details)
    case .appendWorkspaceDockIndicatorDebugLog(let command):
      Self.appendWorkspaceDockIndicatorDebugLog(event: command.event, details: command.details)
    case .persistSharedSidebarStorage(let command):
      Self.persistSharedSidebarStorage(command)
    case .projectBoardResponse(let command):
      workspaceView?.dispatchProjectBoardBridgeResponse(command)
    case .playSound(let command):
      NativeSoundPlayer.shared.play(command)
    case .runProcess(let command):
      runProcess(command) { [weak self] event in
        self?.bridge?.send(event)
      }
    case .syncGhosttyTerminalSettings(let command):
      syncGhosttyTerminalSettings(command)
    case .applyGhosttyConfigSettings(let command):
      applyGhosttyConfigSettings(command)
    case .openGhosttyConfigFile:
      openGhosttyConfigFile()
    case .openAccessibilityPreferences:
      openAccessibilityPreferences()
    case .requestMacOSNotificationPermission:
      sessionAttentionNotificationController.requestPermissionFromSettings()
    case .openMacOSNotificationSettings:
      SessionAttentionNotificationController.openMacOSNotificationSettings()
    case .openExternalUrl(let command):
      openExternalUrl(command)
    case .openWorkspaceInFinder(let command):
      openWorkspaceInFinder(command)
    case .openWorkspaceInIde(let command):
      openWorkspaceInIde(command)
    case .openBrowserWindow(let command):
      browserOverlayController?.open(command)
    case .showBrowserWindow:
      browserOverlayController?.showRunningChromeCanary()
    case .openBrowserDevTools(let command):
      workspaceView?.openBrowserDevTools(sessionId: command.sessionId)
    case .injectBrowserReactGrab(let command):
      workspaceView?.injectBrowserReactGrab(sessionId: command.sessionId)
    case .injectBrowserAgentation(let command):
      workspaceView?.injectBrowserAgentation(sessionId: command.sessionId)
    case .showBrowserProfilePicker(let command):
      workspaceView?.showBrowserProfilePicker(sessionId: command.sessionId)
    case .showBrowserImportSettings(let command):
      workspaceView?.showBrowserImportSettings(sessionId: command.sessionId)
    case .setSidebarSide(let command):
      (window?.contentView as? ghostexRootView)?.setSidebarSide(command.side)
    case .setReactTitlebarHitRegions(let command):
      (window?.contentView as? ghostexRootView)?.setReactTitlebarHitRegions(
        command.regions, overlayOpen: command.overlayOpen)
    case .openActiveProjectEditorFromTitlebar:
      break
    case .openAgentsModeFromTitlebar:
      break
    case .openGitHubProjectFromTitlebar:
      break
    case .showProjectEditorCompanionFromTitlebar:
      break
    case .openTasksPlaceholderFromTitlebar:
      break
    case .refreshWorkspaceOpenTargetAvailabilityFromTitlebar:
      break
    case .rotateActivePaneLayoutClockwiseFromTitlebar:
      break
    case .exitFocusModeFromTitlebar:
      break
    case .togglePetOverlayFromTitlebar:
      break
    case .toggleCommandsPanelFromTitlebar:
      break
    case .sleepInactiveSessionsFromTitlebar:
      break
    case .quitResourcesFromTitlebar:
      break
    case .runSidebarCommandFromTitlebar:
      break
    case .runSidebarGitActionFromTitlebar:
      break
    case .configureZedOverlay(let command):
      if let workspacePath = command.workspacePath {
        self.workspacePath = workspacePath
      }
      if command.enabled, hasUserDetachedZedOverlay, command.reason == "workspace-focus" {
        /**
         CDXC:IDEAttachment 2026-05-01-13:32
         Workspace selection must not undo an explicit IDE detach. The sidebar
         posts configureZedOverlay during focus changes to sync the selected
         workspace path; if its in-memory settings are stale, reject only that
         workspace-focus reattach while preserving explicit Settings/titlebar
         attach commands.
         */
        BrowserOverlayRestoreReproLog.append(
          "appDelegate.configureZedOverlay.skippedWorkspaceReattach",
          [
            "reason": command.reason as Any,
            "targetApp": command.targetApp.rawValue,
            "workspacePath": command.workspacePath as Any,
          ])
        return
      }
      if command.enabled {
        hasUserDetachedZedOverlay = false
      } else {
        hasUserDetachedZedOverlay = true
      }
      if command.enabled, command.reason == "settings-enable" {
        /**
         CDXC:AccessibilityPermissions 2026-05-08-13:08
         Settings is the consent point for IDE attachment. When attachment is
         switched on from Settings, ask for Accessibility immediately; other
         settings saves, startup syncs, and workspace focus messages must not
         create a permission prompt.
         */
        presentAccessibilityPermissionDialogIfNeeded()
      }
      updateAttachToIdeTitlebarButton(
        enabled: command.enabled,
        hideTitlebarButton: command.hideTitlebarButton ?? false,
        targetApp: command.targetApp
      )
      nativeSettingsStore.persistZedOverlay(command)
      guard let zedOverlayController else {
        /**
         CDXC:ZedOverlay 2026-04-26-03:29
         The sidebar webview can send saved Zed overlay settings while
         the AppKit window is still being assembled. Preserve that
         command and apply it once the native overlay controller exists.
         */
        pendingZedOverlayConfiguration = command
        return
      }
      zedOverlayController.configure(command)
    case .openZedWorkspace(let command):
      guard !hasUserDetachedZedOverlay else {
        BrowserOverlayRestoreReproLog.append(
          "appDelegate.openZedWorkspace.skippedDetached",
          [
            "targetApp": command.targetApp.rawValue,
            "workspacePath": command.workspacePath,
          ])
        return
      }
      self.workspacePath = command.workspacePath
      zedOverlayController?.openWorkspace(
        targetApp: command.targetApp, workspacePath: command.workspacePath)
    case .sidebarCliCommand(let command):
      runSidebarCliCommand(command)
    case .sidebarContextMenuOpened:
      /**
       CDXC:SidebarContextMenu 2026-05-21-04:27:
       HostCommand is shared by the sidebar WKWebView and the localhost CLI bridge,
       so app-level dispatch must keep the sidebar context-menu lifecycle exhaustive
       and forward it to the root view that owns native outside-click monitoring.
       */
      (window?.contentView as? ghostexRootView)?.noteSidebarContextMenuOpenedFromHost()
    case .sidebarContextMenuClosed:
      (window?.contentView as? ghostexRootView)?.noteSidebarContextMenuClosedFromHost()
    }
  }

  /**
   CDXC:T3Code 2026-05-14-09:34:
   While the sidebar still shows awake T3 sessions, native must treat the
   managed t3code provider as required background infrastructure. Refresh the
   heartbeat and actively repair missing or unresponsive localhost runtime
   state so restored T3 cards do not strand users in a manual retry flow.
   */
  @MainActor
  private func setT3CodeRuntimeSessionState(_ command: SetT3CodeRuntimeSessionState, reason: String) {
    NativeT3RuntimeLauncher.setRunningSessionHeartbeat(
      runningSessionIds: command.runningSessionIds,
      reason: reason)
    let runtimeCwd = command.runtimeCwd?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !command.runningSessionIds.isEmpty, let runtimeCwd, !runtimeCwd.isEmpty else {
      t3RuntimeVisibleSessionCwd = nil
      t3RuntimeLivenessTimer?.invalidate()
      t3RuntimeLivenessTimer = nil
      return
    }

    t3RuntimeVisibleSessionCwd = runtimeCwd
    ensureT3CodeRuntimeForRunningSessions(reason: reason)
    if t3RuntimeLivenessTimer == nil {
      let timer = Timer(timeInterval: 10.0, repeats: true) { [weak self] _ in
        MainActor.assumeIsolated {
          self?.ensureT3CodeRuntimeForRunningSessions(reason: "livenessTimer")
        }
      }
      t3RuntimeLivenessTimer = timer
      RunLoop.main.add(timer, forMode: .common)
    }
  }

  @MainActor
  private func ensureT3CodeRuntimeForRunningSessions(reason: String) {
    guard let runtimeCwd = t3RuntimeVisibleSessionCwd else {
      return
    }
    guard !NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
      return
    }
    NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.runningSessions.autoStart", [
      "cwd": runtimeCwd,
      "reason": reason,
    ])
    startT3CodeRuntime(StartT3CodeRuntime(cwd: runtimeCwd))
  }

  /**
   CDXC:T3Code 2026-04-30-02:38
   Native T3 Code launches must use desktop/no-browser mode before the WKWebView
   pane loads localhost. Running the plain CLI would open an external browser,
   which is the behavior this integration replaces.
   */
  @MainActor
  private func startT3CodeRuntime(_ command: StartT3CodeRuntime) {
    /**
     CDXC:T3Code 2026-05-10-22:07
     Runtime start/reuse commands must not refresh the managed T3 keepalive:
     sidebar restore loops can request a provider before a T3 card is actually
     running. setT3CodeRuntimeSessionState owns the session heartbeat, and
     createLaunch grants only the startup grace needed for a new provider.
     */
    if let process = t3CodeRuntimeProcess, process.isRunning {
      /**
       CDXC:T3Code 2026-05-02-00:48
       A retained Process handle does not prove the T3 server is usable. A Bun
       runtime can keep running at high CPU while `/api/auth/session` and bearer
       bootstrap requests time out, leaving the pane as a white WKWebView. Reuse
       the handle only after the same health probe used for listener adoption.
       */
      guard NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
        if NativeT3RuntimeLauncher.shouldRetainUnresponsiveManagedRuntime(
          pid: Int(process.processIdentifier))
        {
          /**
           CDXC:T3Code 2026-05-08-13:11
           A tracked T3 runtime can briefly fail auth and environment probes
           while its desktop server is still booting. Retain only that startup
           case; an older unresponsive process is wedged and must be replaced so
           T3 Code does not stay on "Preparing the embedded workspace".
           */
          NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.runningUnhealthyRetained", [
            "cwd": command.cwd,
            "pid": process.processIdentifier,
          ])
          workspaceView?.reloadManagedT3WebPanes(reason: "runtimeRetainedDuringStartup")
          return
        }
        NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        t3CodeRuntimeProcess = nil
        NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeHost")
        return startT3CodeRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
      ])
      return
    }

    /**
     CDXC:T3Code 2026-04-30-09:35
     App restarts lose the Process handle for a still-running managed T3
     provider. Adopt that listener instead of killing it as stale, because T3
     pane restore may already be creating a thread route against the provider.
     */
    if NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() {
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.adoptedExisting", [
        "cwd": command.cwd,
        "port": NativeT3RuntimeLauncher.port,
      ])
      return
    }

    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeHost")
    if NativeT3RuntimeLauncher.hasManagedRuntimeListener() {
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.retainedExistingUnresponsive", [
        "cwd": command.cwd,
        "port": NativeT3RuntimeLauncher.port,
      ])
      workspaceView?.reloadManagedT3WebPanes(reason: "runtimeRetainedDuringStartup")
      return
    }
    NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.spawn", [
      "cwd": command.cwd,
      "mode": "desktop-bootstrap",
    ])
    do {
      let launch = try NativeT3RuntimeLauncher.createLaunch(cwd: command.cwd)
      let process = launch.process
      try process.run()
      t3CodeRuntimeProcess = process
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
      ])
      workspaceView?.reloadManagedT3WebPanes(reason: "runtimeSpawned")
      process.terminationHandler = { [outputCapture = launch.outputCapture] terminatedProcess in
        var details = outputCapture.finish()
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.exit", details)
      }
    } catch {
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      Self.logger.error("Failed to start T3 Code runtime: \(error.localizedDescription)")
    }
  }

  /**
   CDXC:T3Code 2026-04-30-09:23
   The Running modal owns native T3 lifecycle controls. Stop the tracked
   desktop/no-browser provider and clear any managed listener on port 3774 so
   users can recover a blank or stale pane without shelling out manually.
   */
  @MainActor
  private func stopT3CodeRuntime(logPrefix: String) {
    if let process = t3CodeRuntimeProcess {
      NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.stop.tracked", [
        "isRunning": process.isRunning,
        "pid": process.processIdentifier,
      ])
      if process.isRunning {
        process.terminate()
      }
      t3CodeRuntimeProcess = nil
    }
    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(
      logPrefix: "\(logPrefix).stop",
      forceOwnedRuntimeStop: true)
  }

  /**
   CDXC:EditorPanes 2026-05-06-14:21
   Embedded project editors use one shared code-server process. The native host
   verifies the localhost listener before reusing a tracked process so editor
   panes attach to a live VS Code runtime instead of a stale port or dead child.
   */
  @MainActor
  private func startCodeServerRuntime(_ command: StartCodeServerRuntime) {
    if let process = codeServerRuntimeProcess, process.isRunning {
      guard NativeCodeServerRuntimeLauncher.hasResponsiveRuntimeListener() else {
        if let startedAt = codeServerRuntimeStartedAt,
          Date().timeIntervalSince(startedAt)
            < NativeCodeServerRuntimeLauncher.startupGraceInterval
        {
          NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.booting", [
            "cwd": command.cwd,
            "pid": process.processIdentifier,
            "startedAt": startedAt.timeIntervalSince1970,
          ])
          return
        }
        NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        codeServerRuntimeProcess = nil
        codeServerRuntimeStartedAt = nil
        return startCodeServerRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
        "startedAt": codeServerRuntimeStartedAt?.timeIntervalSince1970 ?? NSNull(),
      ])
      return
    }

    if NativeCodeServerRuntimeLauncher.hasResponsiveRuntimeListener() {
      /**
       CDXC:EditorPanes 2026-05-06-15:00
       code-server settings-link options are process launch arguments. Do not
       adopt an untracked listener on the editor port because it may have been
       started without the selected VS Code config flags.
       */
      NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.portBusy", [
        "cwd": command.cwd,
        "origin": NativeCodeServerRuntimeLauncher.origin,
      ])
      _ = NativeCodeServerRuntimeLauncher.waitUntilNotResponsive(timeout: 2.0)
    }

    do {
      let launch = try NativeCodeServerRuntimeLauncher.createLaunch(
        cwd: command.cwd,
        linkVscodeUserConfig: command.linkVscodeUserConfig ?? true,
        vscodeUserConfigDir: command.vscodeUserConfigDir)
      let process = launch.process
      try process.run()
      codeServerRuntimeProcess = process
      let startedAt = Date()
      codeServerRuntimeStartedAt = startedAt
      NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
      ])
      process.terminationHandler = { [outputCapture = launch.outputCapture, startedAt] terminatedProcess in
        var details = outputCapture.finish()
        details["cwd"] = command.cwd
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        details["uptimeSeconds"] = Date().timeIntervalSince(startedAt)
        NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.exit", details)
      }
    } catch {
      NativeT3CodePaneReproLog.append("nativeHost.codeServerRuntime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      Self.logger.error("Failed to start code-server runtime: \(error.localizedDescription)")
    }
  }

  @MainActor
  private func stopCodeServerRuntime(logPrefix: String) {
    if let process = codeServerRuntimeProcess {
      NativeT3CodePaneReproLog.append("\(logPrefix).codeServerRuntime.stop.tracked", [
        "isRunning": process.isRunning,
        "pid": process.processIdentifier,
      ])
      if process.isRunning {
        process.terminate()
      }
      codeServerRuntimeProcess = nil
      codeServerRuntimeStartedAt = nil
    }
  }

  @MainActor private func activateAppWindow() {
    /**
     CDXC:AgentManagerXBridge 2026-04-27-20:34
     Agent Manager focus commands for Ghostex sessions should bring the native
     workarea forward before selecting the requested Ghostty surface.
     */
    recordNativeActivationRequest(reason: "agentManager.activateAppWindow")
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
  }

  @MainActor private func presentAccessibilityPermissionDialogIfNeeded() {
    guard !hasPresentedAccessibilityPermissionDialog, !AXIsProcessTrusted() else {
      return
    }
    hasPresentedAccessibilityPermissionDialog = true
    recordNativeActivationRequest(reason: "accessibilityPermissionDialog")
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)

    /**
     CDXC:AccessibilityPermissions 2026-05-08-13:08
     Accessibility permission must be requested only after the user explicitly
     enables IDE attachment in Settings. Startup should not ask because default
     Ghostex sessions do not need Accessibility until attachment is active.
     CDXC:Branding 2026-05-12-07:35
     Public permission prompts use Ghostex while implementation identifiers
     keep the ghostex storage and bundle naming used by existing installs.
     */
    let alert = NSAlert()
    alert.messageText = "Accessibility Permissions Required"
    alert.informativeText =
      "Ghostex uses Accessibility to attach to Zed, VS Code, or other supported IDE windows. Click OK to open System Settings and enable Accessibility for Ghostex. A restart may be required after granting permission."
    alert.alertStyle = .warning
    alert.addButton(withTitle: "OK")
    alert.addButton(withTitle: "Cancel")

    if let primaryButton = alert.buttons.first {
      primaryButton.keyEquivalent = "\r"
      primaryButton.bezelColor = .controlAccentColor
    }
    if alert.buttons.count > 1 {
      alert.buttons[1].keyEquivalent = "\u{1b}"
    }

    let result = alert.runModal()
    guard result == .alertFirstButtonReturn else {
      return
    }
    openAccessibilityPreferences()
  }

  private func openAccessibilityPreferences() {
    guard
      let url = URL(
        string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
    else {
      return
    }
    NSWorkspace.shared.open(url)
  }

  @MainActor private func runSidebarCliCommand(_ command: SidebarCliCommand) {
    /**
     CDXC:DebugCli 2026-04-27-07:18
     The CLI must exercise the same sidebar/runtime code paths as a user
     click. Forward debug commands into the sidebar webview and return the
     JSON result through the existing bridge instead of creating orphan
     native terminals behind the sidebar's state.
     */
    guard let sidebarView = (window?.contentView as? ghostexRootView)?.sidebarWebView else {
      bridge?.send(
        .sidebarCliResult(
          requestId: command.requestId,
          ok: false,
          payloadJson: #"{"error":"sidebar-webview-missing"}"#
        ))
      return
    }
    guard
      let actionJson = Self.javascriptStringLiteral(command.action),
      let payloadJson = Self.javascriptStringLiteral(command.payloadJson ?? "{}")
    else {
      bridge?.send(
        .sidebarCliResult(
          requestId: command.requestId,
          ok: false,
          payloadJson: #"{"error":"sidebar-cli-command-encoding-failed"}"#
        ))
      return
    }
    /**
     CDXC:BrowserPanes 2026-05-02-11:18
     Browser-pane verification uses the real ghostex app and sidebar CLI. WebKit's
     evaluateJavaScript cannot serialize a Promise result, so CLI commands must
     run through callAsyncJavaScript before returning JSON to the bridge.
     */
    let script = """
      const handler = window.__ghostex_NATIVE_CLI__;
      if (!handler || typeof handler.handleCommand !== 'function') {
        return JSON.stringify({ ok: false, error: 'sidebar-cli-handler-missing' });
      }
      return JSON.stringify(await handler.handleCommand(action, JSON.parse(payloadJson)));
      """
    let handleResult: (Any?, Error?) -> Void = { [weak self] result, error in
      let payloadJson: String
      let ok: Bool
      if let error {
        ok = false
        payloadJson = Self.jsonObjectString(["error": error.localizedDescription])
      } else if let result = result as? String {
        ok = !result.contains(#""ok":false"#)
        payloadJson = result
      } else {
        ok = false
        payloadJson = #"{"error":"sidebar-cli-result-missing"}"#
      }
      self?.bridge?.send(
        .sidebarCliResult(
          requestId: command.requestId,
          ok: ok,
          payloadJson: payloadJson
        ))
    }
    if #available(macOS 11.0, *) {
      sidebarView.callAsyncJavaScript(
        script,
        arguments: [
          "action": command.action,
          "payloadJson": command.payloadJson ?? "{}",
        ],
        in: nil,
        in: .page
      ) { result in
        switch result {
        case .success(let value):
          handleResult(value, nil)
        case .failure(let error):
          handleResult(nil, error)
        }
      }
      return
    }
    let fallbackScript = """
      (async () => {
        const action = \(actionJson);
        const payloadJson = \(payloadJson);
        \(script)
      })()
      """
    sidebarView.evaluateJavaScript(fallbackScript) { result, error in
      handleResult(result, error)
    }
  }

  private static func javascriptStringLiteral(_ value: String) -> String? {
    guard let data = try? JSONEncoder().encode(value) else {
      return nil
    }
    return String(data: data, encoding: .utf8)
  }

  fileprivate static func jsonObjectString(_ value: [String: String]) -> String {
    guard let data = try? JSONEncoder().encode(value),
      let text = String(data: data, encoding: .utf8)
    else {
      return #"{"error":"json-encoding-failed"}"#
    }
    return text
  }

  @MainActor private func detachZedOverlayFromNativeButton(targetApp: ZedOverlayTargetApp) {
    /**
     CDXC:ZedOverlay 2026-04-26-10:54
     The native Detach button must behave like turning off the sidebar
     attach checkbox: persist the disabled attach setting, apply standalone
     window behavior immediately, and update the sidebar settings UI.
     */
    let command = ConfigureZedOverlay(
      enabled: false,
      hideTitlebarButton: nativeSettingsStore.readZedOverlay().hideTitlebarButton,
      reason: nil,
      targetApp: targetApp,
      workspacePath: workspacePath
    )
    handle(.configureZedOverlay(command))
    (window?.contentView as? ghostexRootView)?.applyNativeZedOverlayDetached(targetApp: targetApp)
  }

  private func initialZedOverlayConfiguration() -> ConfigureZedOverlay? {
    let environment = ProcessInfo.processInfo.environment
    let stored = nativeSettingsStore.readZedOverlay()
    let enabledValue =
      environment["ghostex_ZED_OVERLAY_ENABLED"].map { value in
        value == "1" || value.lowercased() == "true"
      } ?? stored.enabled
    guard let enabledValue else {
      return nil
    }
    let targetApp =
      environment["ghostex_ZED_OVERLAY_TARGET_APP"]
      .flatMap(ZedOverlayTargetApp.init(rawValue:))
      ?? stored.targetApp
      ?? .zedPreview
    return ConfigureZedOverlay(
      enabled: enabledValue,
      hideTitlebarButton: stored.hideTitlebarButton,
      reason: nil,
      targetApp: targetApp,
      workspacePath: workspacePath
    )
  }

  private func syncGhosttyTerminalSettings(_ command: SyncGhosttyTerminalSettings) {
    /**
     CDXC:TerminalSettings 2026-04-26-19:02
     ghostex settings run in the native sidebar webview and must write the
     same Ghostty config file selected for embedded terminals. Keep the
     merge narrow so themes, keybinds, and unrelated Ghostty settings stay
     user-owned.
     */
    do {
      let configURL =
        ghosttyConfigSelection.path.map { URL(fileURLWithPath: $0) }
        ?? Self.defaultWritableGhosttyConfigURL()
      let existingConfig = (try? String(contentsOf: configURL, encoding: .utf8)) ?? ""
      let mergedConfig = Self.mergeGhosttyTerminalSettings(existingConfig, command)
      try FileManager.default.createDirectory(
        at: configURL.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try mergedConfig.write(to: configURL, atomically: true, encoding: .utf8)
      scheduleGhosttyConfigReload(immediate: command.reloadImmediately == true)
    } catch {
      Self.logger.error("Failed to sync Ghostty terminal settings: \(error.localizedDescription)")
    }
  }

  private func applyGhosttyConfigSettings(_ command: ApplyGhosttyConfigSettings) {
    /**
     CDXC:GhosttySettings 2026-04-30-01:48
     Ghostty config action buttons must edit the real selected config file,
     not only ghostex sidebar state. Merge only managed keys so reset restores
     Ghostty defaults without discarding unrelated user configuration.
     */
    do {
      let configURL =
        ghosttyConfigSelection.path.map { URL(fileURLWithPath: $0) }
        ?? Self.defaultWritableGhosttyConfigURL()
      let existingConfig = (try? String(contentsOf: configURL, encoding: .utf8)) ?? ""
      let mergedConfig = Self.mergeGhosttyConfigSettings(
        existingConfig,
        lines: command.lines,
        managedKeys: Set(command.managedKeys)
      )
      try FileManager.default.createDirectory(
        at: configURL.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try mergedConfig.write(to: configURL, atomically: true, encoding: .utf8)
      scheduleGhosttyConfigReload(immediate: command.reloadImmediately == true)
    } catch {
      Self.logger.error("Failed to apply Ghostty config settings: \(error.localizedDescription)")
    }
  }

  private func scheduleGhosttyConfigReload(immediate: Bool = false) {
    /**
     CDXC:TerminalSettings 2026-04-26-20:21
     Slider drags can emit many terminal-setting writes. Reload embedded
     Ghostty automatically only after the user stops changing values for
     three seconds, matching Ghostty's reloadConfig API without causing
     repeated font/metric rebuilds during a continuous drag.

     CDXC:TerminalScrollSettings 2026-04-29-08:56
     Mouse scroll multiplier changes do not rebuild font metrics and need
     immediate feedback, so scroll-only changes bypass the delayed reload.
     */
    pendingGhosttyConfigReloadTimer?.invalidate()
    if immediate {
      pendingGhosttyConfigReloadTimer = nil
      ghostty.reloadConfig()
      return
    }
    pendingGhosttyConfigReloadTimer = Timer.scheduledTimer(withTimeInterval: 3, repeats: false) {
      [weak self] _ in
      MainActor.assumeIsolated {
        guard let self else {
          return
        }
        self.pendingGhosttyConfigReloadTimer = nil
        self.ghostty.reloadConfig()
      }
    }
  }

  private static func defaultWritableGhosttyConfigURL() -> URL {
    let appSupport = FileManager.default.urls(
      for: .applicationSupportDirectory, in: .userDomainMask)[0]
    return appSupport.appendingPathComponent("com.mitchellh.ghostty/config")
  }

  private static func mergeGhosttyTerminalSettings(
    _ config: String,
    _ command: SyncGhosttyTerminalSettings
  ) -> String {
    var retainedLines =
      config
      .components(separatedBy: .newlines)
      .filter { shouldRetainGhosttyConfigLine($0, command: command) }
    while retainedLines.last?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == true {
      retainedLines.removeLast()
    }
    var managedSettingLines = [
        "font-size = \(formatGhosttyNumber(command.fontSize))",
        "adjust-cell-height = \(formatGhosttyPercent(command.adjustCellHeightPercent))",
        "adjust-cell-width = \(formatGhosttyNumber(command.adjustCellWidth))",
        /**
         CDXC:GhosttyDefaults 2026-05-22-12:29:
         Ghostex-owned defaults should generate the requested black GitHub Dark
         profile even when the user has no prior Ghostty config: white text,
         cyan ANSI palette slot 6, blue selection, white bar cursor, opaque
         splits, gray dividers, shift mouse capture, Cmd+E command palette,
         Option-as-Alt, and SSH shell integration features.
         */
        "background = #000000",
        "foreground = #ffffff",
        "palette = 6=#39c5cf",
        "selection-background = #07284f",
        "cursor-style = \(command.cursorStyle)",
        "cursor-color = #FFFFFF",
        "unfocused-split-opacity = 1",
        "split-divider-color = #8f8f8f",
        "mouse-shift-capture = always",
        "keybind = super+e=toggle_command_palette",
        "macos-option-as-alt = true",
        "shell-integration-features = ssh-env,ssh-terminfo",
        /**
         CDXC:TerminalBehaviorSettings 2026-04-29-09:32
         Common ghostex settings map directly to Ghostty config keys so the
         embedded terminal and external Ghostty windows share scrollback,
         cursor blink, copy-on-select, and close confirmation behavior.
         */
        "scrollback-limit = \(max(1, command.scrollbackLimitBytes))",
        "cursor-style-blink = \(command.cursorStyleBlink ? "true" : "false")",
        "clipboard-trim-trailing-spaces = \(command.clipboardTrimTrailingSpaces ? "true" : "false")",
        "clipboard-paste-protection = \(command.clipboardPasteProtection ? "true" : "false")",
        "copy-on-select = \(command.copyOnSelect)",
        "confirm-close-surface = \(command.confirmCloseSurface)",
        "mouse-hide-while-typing = \(command.mouseHideWhileTyping ? "true" : "false")",
        "scrollbar = \(command.scrollbar)",
        /**
         CDXC:TerminalScrollSettings 2026-04-29-08:56
         ghostex manages Ghostty scroll speed through the documented prefixed
         mouse-scroll-multiplier values so precision devices and discrete
         mouse wheels keep separate settings in the shared Ghostty config.
         */
        "mouse-scroll-multiplier = precision:\(formatGhosttyNumber(command.mouseScrollMultiplierPrecision)),discrete:\(formatGhosttyNumber(command.mouseScrollMultiplierDiscrete))",
      ]
    let fontFamily = command.fontFamily.trimmingCharacters(in: .whitespacesAndNewlines)
    if !fontFamily.isEmpty {
      /**
       CDXC:TerminalTypographySettings 2026-04-29-09:32
       Empty font-family means ghostex leaves the user's existing Ghostty font
       family or platform default untouched. Non-empty values are written as
       raw Ghostty font-family strings from the settings modal text field.
      */
      managedSettingLines.insert("font-family = \(formatGhosttyString(fontFamily))", at: 0)
    }
    if let fontVariationWeight = command.fontVariationWeight {
      /**
       CDXC:TerminalTypographySettings 2026-04-29-09:32
       Ghostty has no font-weight key. The weight slider writes the documented
       variable-font axis setting, and the config merge removes older ghostex
       wght entries before adding the selected value.
       */
      managedSettingLines.append("font-variation = wght=\(fontVariationWeight)")
    }
    let lines = retainedLines + managedSettingLines
    let themeName = command.ghosttyTheme.trimmingCharacters(in: .whitespacesAndNewlines)
    let finalLines =
      themeName.isEmpty ? lines : lines + ["theme = \(formatGhosttyString(themeName))"]
    return finalLines.joined(separator: "\n") + "\n"
  }

  private static func mergeGhosttyConfigSettings(
    _ config: String,
    lines: [String],
    managedKeys: Set<String>
  ) -> String {
    var retainedLines =
      config
      .components(separatedBy: .newlines)
      .filter { !managedKeys.contains(readGhosttyConfigKey($0)) }
    while retainedLines.last?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == true {
      retainedLines.removeLast()
    }
    var nextLines = retainedLines + lines
    while nextLines.last?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == true {
      nextLines.removeLast()
    }
    return nextLines.isEmpty ? "" : nextLines.joined(separator: "\n") + "\n"
  }

  private static func shouldRetainGhosttyConfigLine(
    _ line: String,
    command: SyncGhosttyTerminalSettings
  ) -> Bool {
    let managedKeys: Set<String> = [
      "adjust-cell-height",
      "adjust-cell-width",
      "background",
      "clipboard-paste-protection",
      "clipboard-trim-trailing-spaces",
      "confirm-close-surface",
      "copy-on-select",
      "cursor-color",
      "cursor-style",
      "cursor-style-blink",
      "font-size",
      "font-thicken",
      "font-thicken-strength",
      "foreground",
      "macos-option-as-alt",
      "mouse-hide-while-typing",
      "mouse-scroll-multiplier",
      "mouse-shift-capture",
      "scrollbar",
      "scrollback-limit",
      "selection-background",
      "shell-integration-features",
      "split-divider-color",
      "unfocused-split-opacity",
    ]
    let key = readGhosttyConfigKey(line)
    if managedKeys.contains(key) {
      return false
    }
    if key == "font-family" {
      return command.fontFamily.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
    if key == "theme" {
      return command.ghosttyTheme.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
    if key == "keybind" {
      return !readGhosttyConfigValue(line)
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
        .hasPrefix("super+e=")
    }
    if key == "palette" {
      return !readGhosttyConfigValue(line)
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
        .hasPrefix("6=")
    }
    if key != "font-variation" {
      return true
    }
    if command.fontVariationWeight == nil {
      return true
    }
    return !readGhosttyConfigValue(line)
      .split(separator: ",")
      .contains {
        $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased().hasPrefix("wght=")
      }
  }

  private static func readGhosttyConfigKey(_ line: String) -> String {
    let trimmedLine = line.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmedLine.isEmpty || trimmedLine.hasPrefix("#") {
      return ""
    }
    return trimmedLine.split(separator: "=", maxSplits: 1).first.map {
      String($0).trimmingCharacters(in: .whitespacesAndNewlines)
    } ?? ""
  }

  private static func readGhosttyConfigValue(_ line: String) -> String {
    guard let equalsIndex = line.firstIndex(of: "=") else {
      return ""
    }
    return String(line[line.index(after: equalsIndex)...]).trimmingCharacters(
      in: .whitespacesAndNewlines)
  }

  private static func formatGhosttyString(_ value: String) -> String {
    "\"\(value.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\""))\""
  }

  private static func formatGhosttyNumber(_ value: Double) -> String {
    if value.rounded() == value {
      return String(Int(value))
    }
    return String(format: "%.2f", value)
      .replacingOccurrences(of: #"0+$"#, with: "", options: .regularExpression)
      .replacingOccurrences(of: #"\.$"#, with: "", options: .regularExpression)
  }

  private static func formatGhosttyPercent(_ value: Double) -> String {
    "\(formatGhosttyNumber(value * 100))%"
  }

  private func showMessage(_ command: ShowMessage) {
    let alert = NSAlert()
    switch command.level {
    case .info:
      alert.alertStyle = .informational
    case .warning:
      alert.alertStyle = .warning
    case .error:
      alert.alertStyle = .critical
    }
    alert.messageText = "Ghostex"
    alert.informativeText = command.message
    alert.addButton(withTitle: "OK")
    if let window {
      alert.beginSheetModal(for: window)
    } else {
      alert.runModal()
    }
  }

  private func openExternalUrl(_ command: OpenExternalUrl) {
    guard let url = URL(string: command.url) else {
      return
    }
    NSWorkspace.shared.open(url)
  }

  @MainActor private func openWorkspaceInFinder(_ command: OpenWorkspaceInFinder) {
    let path = command.workspacePath.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !path.isEmpty, FileManager.default.fileExists(atPath: path) else {
      showMessage(.init(level: .warning, message: "Workspace folder does not exist."))
      return
    }

    /**
     CDXC:WorkspaceActions 2026-05-04-08:22
     Project right-click "Open in Finder" should reveal the actual stored
     workspace folder through Finder instead of routing through a URL opener or
     creating a fallback path when the project record is wrong.
     */
    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path, isDirectory: true)])
  }

  @MainActor private func openWorkspaceInIde(_ command: OpenWorkspaceInIde) {
    let path = command.workspacePath.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !path.isEmpty else {
      return
    }

    /**
     CDXC:WorkspaceActions 2026-05-04-08:22
     Project right-click "Open in IDE" is an explicit command and must use the
     IDE selected in Settings even when IDE attachment or sync-open is disabled.
     Reuse the native IDE launcher so Zed, Zed Preview, VS Code, and Insiders
     keep their existing command-line workspace behavior.
     */
    zedOverlayController?.openWorkspace(targetApp: command.targetApp, workspacePath: path)
  }

  private func openGhosttyConfigFile() {
    /**
     CDXC:GhosttySettings 2026-04-30-01:48
     The settings modal's config-file button should open the selected Ghostty
     config path directly. Create an empty file when missing so the editor has
     a concrete target instead of opening only the parent directory.
     */
    do {
      let configURL =
        ghosttyConfigSelection.path.map { URL(fileURLWithPath: $0) }
        ?? Self.defaultWritableGhosttyConfigURL()
      try FileManager.default.createDirectory(
        at: configURL.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      if !FileManager.default.fileExists(atPath: configURL.path) {
        try "".write(to: configURL, atomically: true, encoding: .utf8)
      }
      NSWorkspace.shared.open(configURL)
    } catch {
      Self.logger.error("Failed to open Ghostty config file: \(error.localizedDescription)")
    }
  }

  private func runProcess(_ command: RunProcess, sendEvent: @escaping (HostEvent) -> Void) {
    Task.detached {
      let process = Process()
      process.executableURL = URL(fileURLWithPath: command.executable)
      process.arguments = command.args
      if let cwd = command.cwd {
        process.currentDirectoryURL = URL(fileURLWithPath: cwd, isDirectory: true)
      }
      process.environment = normalizedNativeProcessEnvironment(overrides: command.env)
      let stdoutPipe = Pipe()
      let stderrPipe = Pipe()
      process.standardInput = FileHandle.nullDevice
      process.standardOutput = stdoutPipe
      process.standardError = stderrPipe
      let outputLock = NSLock()
      var stdoutData = Data()
      var stderrData = Data()
      let stdoutHandle = stdoutPipe.fileHandleForReading
      let stderrHandle = stderrPipe.fileHandleForReading
      /**
       CDXC:AgentsHub 2026-05-14-08:43
       Agents Hub catalog discovery can return megabytes of real profile, skill,
       hook, and config metadata. Drain process output while the command is
       running so large stdout/stderr payloads cannot fill the pipe and block
       the scanner before native posts processResult back to the webview.
       */
      stdoutHandle.readabilityHandler = { handle in
        let data = handle.availableData
        if data.isEmpty {
          return
        }
        outputLock.lock()
        stdoutData.append(data)
        outputLock.unlock()
      }
      stderrHandle.readabilityHandler = { handle in
        let data = handle.availableData
        if data.isEmpty {
          return
        }
        outputLock.lock()
        stderrData.append(data)
        outputLock.unlock()
      }

      do {
        try process.run()
        process.waitUntilExit()
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil
        let remainingStdoutData = stdoutHandle.readDataToEndOfFile()
        let remainingStderrData = stderrHandle.readDataToEndOfFile()
        outputLock.lock()
        stdoutData.append(remainingStdoutData)
        stderrData.append(remainingStderrData)
        let stdout = String(data: stdoutData, encoding: .utf8) ?? ""
        let stderr = String(data: stderrData, encoding: .utf8) ?? ""
        outputLock.unlock()
        await MainActor.run {
          sendEvent(
            .processResult(
              requestId: command.requestId,
              exitCode: process.terminationStatus,
              stdout: stdout,
              stderr: stderr
            ))
        }
      } catch {
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil
        await MainActor.run {
          sendEvent(
            .processResult(
              requestId: command.requestId,
              exitCode: 127,
              stdout: "",
              stderr: error.localizedDescription
            ))
        }
      }
    }
  }
}

private struct NativeZedOverlaySettings {
  let enabled: Bool?
  let hideTitlebarButton: Bool?
  let targetApp: ZedOverlayTargetApp?
}

private struct NativeSidebarChromeSettings {
  let width: CGFloat?
  let projectEditorCompanionWidthRatio: CGFloat?
}

private struct NativeMainWindowChromeSettings {
  let frame: NSRect?
  let screenID: UInt32?
  let screenFrame: NSRect?
  let width: CGFloat?
  let height: CGFloat?
}

private final class NativeSettingsStore {
  private static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "settings")
  private static let defaultHotkeys: [String: String] = [
    /**
     CDXC:Hotkeys 2026-05-11-09:26
     Default hotkeys prefer plain Cmd chords for common navigation and reserve
     heavier modifiers only where plain Cmd is already used by session slots or
     split-direction conventions.
     CDXC:Hotkeys 2026-05-15-13:31:
     Plain Cmd+Arrow belongs to terminal and prompt text editing.
     Directional pane focus uses Cmd+Alt+Arrow so AppKit no longer intercepts common text navigation shortcuts.
    */
    "createSession": "cmd+n",
    /**
     CDXC:CommandPalette 2026-05-17-01:32:
     Pane context-menu actions also need configurable defaults at the AppKit
     boundary so terminal-focused shortcuts can dispatch the same focused-pane
     commands shown in the command palette.
     */
    "delayedSend": "ctrl+shift+s",
    "focusDown": "cmd+alt+down",
    "focusGroup1": "cmd+ctrl+1",
    "focusGroup2": "cmd+ctrl+2",
    "focusGroup3": "cmd+ctrl+3",
    "focusGroup4": "cmd+ctrl+4",
    "focusGroup5": "cmd+ctrl+5",
    "focusLeft": "cmd+alt+left",
    "focusNextGroup": "cmd+]",
    "focusNextSession": "cmd+tab",
    "focusPreviousGroup": "cmd+[",
    "focusPreviousSession": "cmd+shift+tab",
    "focusRight": "cmd+alt+right",
    "focusSessionSlot1": "cmd+1",
    "focusSessionSlot2": "cmd+2",
    "focusSessionSlot3": "cmd+3",
    "focusSessionSlot4": "cmd+4",
    "focusSessionSlot5": "cmd+5",
    "focusSessionSlot6": "cmd+6",
    "focusSessionSlot7": "cmd+7",
    "focusSessionSlot8": "cmd+8",
    "focusSessionSlot9": "cmd+9",
    "focusUp": "cmd+alt+up",
    "forkSession": "ctrl+shift+f",
    "mergeAllTabs": "ctrl+shift+m",
    "moveSidebar": "cmd+b",
    /**
     CDXC:CommandPalette 2026-05-15-20:38:
     Cmd+K opens the shadcn command palette even while terminal panes own first
     responder, so AppKit must match the same shared hotkey id as the sidebar.
     */
    "openCommandPalette": "cmd+k",
    /**
     CDXC:Hotkeys 2026-05-14-08:09:
     F12 is the default Commands panel shortcut in shared sidebar settings, and terminal focus reaches AppKit before the sidebar DOM can observe that bare function key.
     Keep the native defaults in sync so AppKit matches and dispatches openCommandsPanel instead of filtering the action out of persisted hotkeys.
     */
    "openCommandsPanel": "f12",
    "openBrowserPane": "ctrl+shift+b",
    "openSettings": "cmd+,",
    "popOutPane": "ctrl+shift+o",
    /**
     CDXC:CommandPalette 2026-05-17-01:34:
     Rotate and Reload defaults are intentionally swapped so Ctrl+Shift+L
     rotates the layout while Ctrl+Shift+R keeps the common reload mnemonic.
     */
    "reloadSession": "ctrl+shift+r",
    "renameActiveSession": "cmd+r",
    "rotatePanesClockwise": "ctrl+shift+l",
    /**
     CDXC:ActionsHotkeys 2026-05-17-01:18:
     Action hotkeys launch the first five Actions by their current list order,
     so native AppKit defaults must match the shared sidebar settings while
     terminal panes own first responder.
     */
    "runActionSlot1": "ctrl+shift+1",
    "runActionSlot2": "ctrl+shift+2",
    "runActionSlot3": "ctrl+shift+3",
    "runActionSlot4": "ctrl+shift+4",
    "runActionSlot5": "ctrl+shift+5",
    /**
     CDXC:NativeSplits 2026-05-10-18:30
     Cmd+D and Cmd+Shift+D now create real terminal panes in the sidebar state
     rather than stepping a preset count.
     */
    "splitMore": "cmd+d",
    "splitMoreDown": "cmd+shift+d",
  ]
  fileprivate static let defaultHotkeyAliases: [String: [String]] = [
    "focusNextSession": ["cmd+shift+]"],
    "focusPreviousSession": ["cmd+shift+["],
  ]
  private static let retiredDefaultHotkeys: [String: [String]] = [
    "focusDown": ["cmd+down"],
    "focusLeft": ["cmd+left"],
    "focusNextGroup": ["cmd+shift+]"],
    "focusNextSession": ["cmd+]"],
    "focusPreviousGroup": ["cmd+shift+["],
    "focusPreviousSession": ["cmd+["],
    "focusRight": ["cmd+right"],
    "focusUp": ["cmd+up"],
  ]

  /**
   CDXC:ZedOverlay 2026-04-26-04:14
   The all-native host must keep the Zed overlay setting in native app state,
   not only WKWebView localStorage. Reading and writing the same settings file
   used by the packaged app keeps the overlay button enabled after restarts.
   */
  func readZedOverlay() -> NativeZedOverlaySettings {
    guard let settings = readSettingsDictionary() else {
      return NativeZedOverlaySettings(enabled: nil, hideTitlebarButton: nil, targetApp: nil)
    }
    return NativeZedOverlaySettings(
      enabled: settings["zedOverlayEnabled"] as? Bool,
      hideTitlebarButton: settings["zedOverlayHideTitlebarButton"] as? Bool,
      targetApp: (settings["zedOverlayTargetApp"] as? String).flatMap(
        ZedOverlayTargetApp.init(rawValue:))
    )
  }

  func persistZedOverlay(_ command: ConfigureZedOverlay) {
    do {
      let url = settingsURL()
      var settings = readSettingsDictionary() ?? [:]
      settings["zedOverlayEnabled"] = command.enabled
      if let hideTitlebarButton = command.hideTitlebarButton {
        settings["zedOverlayHideTitlebarButton"] = hideTitlebarButton
      }
      settings["zedOverlayTargetApp"] = command.targetApp.rawValue
      let data = try JSONSerialization.data(
        withJSONObject: settings, options: [.prettyPrinted, .sortedKeys])
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try data.write(to: url, options: [.atomic])
    } catch {
      Self.logger.error("Failed to persist Zed overlay settings: \(error.localizedDescription)")
    }
  }

  /**
   CDXC:NativeSidebarChrome 2026-04-26-07:16
   The native sidebar width is user-resized AppKit chrome, so it must be
   stored in the shared native settings file and restored before the first
   layout after an app restart.
   CDXC:ProjectEditorCompanion 2026-05-16-06:55:
   The project-editor companion pane width is the same kind of native chrome preference: it should follow the user across projects and app restarts, not reset with the active project's workspace snapshot.
   */
  func readSidebarChrome() -> NativeSidebarChromeSettings {
    guard let settings = readSettingsDictionary() else {
      return NativeSidebarChromeSettings(width: nil, projectEditorCompanionWidthRatio: nil)
    }
    return NativeSidebarChromeSettings(
      width: Self.readCGFloat(settings["sidebarWidth"]),
      projectEditorCompanionWidthRatio: Self.readCGFloat(settings["projectEditorCompanionWidthRatio"]))
  }

  func readSidebarSide() -> SidebarSide {
    /**
     CDXC:SidebarPlacement 2026-05-06-17:32
     Native startup must place the sidebar from the persisted Settings value
     before the React sidebar finishes loading, so right-side users do not see
     an initial left-side layout that later jumps.
     */
    guard let settings = readSharedSidebarSettingsDictionary(),
      let side = settings["sidebarSide"] as? String
    else {
      return .left
    }
    return SidebarSide(rawValue: side) ?? .left
  }

  func readHotkeys() -> [String: String] {
    guard let settings = readSharedSidebarSettingsDictionary() else {
      return Self.defaultHotkeys
    }
    var hotkeys = Self.defaultHotkeys
    if let customHotkeys = settings["hotkeys"] as? [String: Any] {
      for (key, value) in customHotkeys {
        guard Self.defaultHotkeys.keys.contains(key) else {
          continue
        }
        if let text = value as? String {
          let normalizedText = Self.normalizeHotkeyText(text)
          /**
           CDXC:Hotkeys 2026-05-11-09:06
           An explicitly blank persisted hotkey disables that command. Missing
           keys continue to fall back to defaults so new commands appear after
           app updates without a migration step.
           */
          hotkeys[key] = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? ""
            : Self.migrateRetiredDefaultHotkey(actionId: key, hotkeyText: normalizedText)
        }
      }
    }
    return hotkeys
  }

  private static func migrateRetiredDefaultHotkey(actionId: String, hotkeyText: String) -> String {
    if retiredDefaultHotkeys[actionId]?.contains(hotkeyText) == true,
      let defaultHotkey = defaultHotkeys[actionId]
    {
      return defaultHotkey
    }
    return hotkeyText
  }

  func persistSidebarWidth(_ width: CGFloat) {
    do {
      let url = settingsURL()
      var settings = readSettingsDictionary() ?? [:]
      settings["sidebarWidth"] = width
      let data = try JSONSerialization.data(
        withJSONObject: settings, options: [.prettyPrinted, .sortedKeys])
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try data.write(to: url, options: [.atomic])
    } catch {
      Self.logger.error("Failed to persist sidebar width: \(error.localizedDescription)")
    }
  }

  func persistProjectEditorCompanionWidthRatio(_ widthRatio: CGFloat) {
    do {
      let url = settingsURL()
      var settings = readSettingsDictionary() ?? [:]
      settings["projectEditorCompanionWidthRatio"] = widthRatio
      let data = try JSONSerialization.data(
        withJSONObject: settings, options: [.prettyPrinted, .sortedKeys])
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try data.write(to: url, options: [.atomic])
    } catch {
      Self.logger.error(
        "Failed to persist project editor companion width ratio: \(error.localizedDescription)")
    }
  }

  /**
   CDXC:NativeWindowChrome 2026-05-07-08:17
   The native host must reopen at the exact main-window size, position, and
   screen from the prior close. Persist the absolute frame and the display's
   identifier/frame; startup can then restore the same display-relative origin
   even when macOS has changed the global coordinates for a monitor.
   */
  func readMainWindowChrome() -> NativeMainWindowChromeSettings {
    guard let settings = readSettingsDictionary() else {
      return NativeMainWindowChromeSettings(
        frame: nil,
        screenID: nil,
        screenFrame: nil,
        width: nil,
        height: nil)
    }
    let frame = Self.readRect(
      x: settings["mainWindowX"],
      y: settings["mainWindowY"],
      width: settings["mainWindowWidth"],
      height: settings["mainWindowHeight"])
    let screenFrame = Self.readRect(
      x: settings["mainWindowScreenFrameX"],
      y: settings["mainWindowScreenFrameY"],
      width: settings["mainWindowScreenFrameWidth"],
      height: settings["mainWindowScreenFrameHeight"])
    return NativeMainWindowChromeSettings(
      frame: frame,
      screenID: Self.readUInt32(settings["mainWindowScreenID"]),
      screenFrame: screenFrame,
      width: Self.readCGFloat(settings["mainWindowWidth"]),
      height: Self.readCGFloat(settings["mainWindowHeight"])
    )
  }

  func persistMainWindowChrome(frame: NSRect, screen: NSScreen) {
    do {
      let url = settingsURL()
      var settings = readSettingsDictionary() ?? [:]
      settings["mainWindowX"] = frame.minX
      settings["mainWindowY"] = frame.minY
      settings["mainWindowWidth"] = frame.width
      settings["mainWindowHeight"] = frame.height
      if let screenID = AppDelegate.screenIdentifier(screen) {
        settings["mainWindowScreenID"] = Int(screenID)
      }
      settings["mainWindowScreenFrameX"] = screen.frame.minX
      settings["mainWindowScreenFrameY"] = screen.frame.minY
      settings["mainWindowScreenFrameWidth"] = screen.frame.width
      settings["mainWindowScreenFrameHeight"] = screen.frame.height
      let data = try JSONSerialization.data(
        withJSONObject: settings, options: [.prettyPrinted, .sortedKeys])
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try data.write(to: url, options: [.atomic])
    } catch {
      Self.logger.error("Failed to persist main window chrome: \(error.localizedDescription)")
    }
  }

  private func readSettingsDictionary() -> [String: Any]? {
    let url = settingsURL()
    guard let data = try? Data(contentsOf: url),
      let object = try? JSONSerialization.jsonObject(with: data),
      let settings = object as? [String: Any]
    else {
      return nil
    }
    return settings
  }

  private func readSharedSidebarSettingsDictionary() -> [String: Any]? {
    let url = GhostexAppStorage.sharedStateDirectory.appendingPathComponent(
      "native-sidebar-settings.json")
    guard let data = try? Data(contentsOf: url),
      let object = try? JSONSerialization.jsonObject(with: data),
      let settings = object as? [String: Any]
    else {
      return nil
    }
    return settings
  }

  private static func normalizeHotkeyText(_ text: String) -> String {
    text.trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased()
      .replacingOccurrences(of: "command", with: "cmd")
      .replacingOccurrences(of: "option", with: "alt")
      .replacingOccurrences(of: "control", with: "ctrl")
      .replacingOccurrences(of: "\\bmod\\b", with: "cmd", options: .regularExpression)
      .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
  }

  private func settingsURL() -> URL {
    if let override = ProcessInfo.processInfo.environment["ghostex_SETTINGS_PATH"], !override.isEmpty {
      return URL(fileURLWithPath: override)
    }

    let appSupport = FileManager.default.urls(
      for: .applicationSupportDirectory, in: .userDomainMask)[0]
    let bundleIdentifier = Bundle.main.bundleIdentifier ?? "com.madda.ghostex.host"
    let primaryURL = appSupport.appendingPathComponent("\(bundleIdentifier)/state/settings.json")
    /**
     CDXC:Distribution 2026-04-27-08:37
     The notarized brew app stores new native settings under its
     com.madda.ghostex.host bundle identity, while still reading older local
     development paths so existing sidebar preferences survive the 1.0.0
     distribution rename.
     CDXC:DevAppFlavor 2026-05-11-12:10
     ghostex-dev must not reuse the installed app's native chrome or overlay
     settings. Non-production bundle ids write to their own Application Support
     container and skip production migration candidates.
     */
    guard bundleIdentifier == "com.madda.ghostex.host" else {
      return primaryURL
    }
    let existingCandidates = [
      primaryURL,
      appSupport.appendingPathComponent("dev.maddada.ghostex/dev/state/settings.json"),
      appSupport.appendingPathComponent("com.ghostex.host/state/settings.json"),
    ]
    return existingCandidates.first { FileManager.default.fileExists(atPath: $0.path) }
      ?? existingCandidates[0]
  }

  private static func readCGFloat(_ value: Any?) -> CGFloat? {
    if let number = value as? NSNumber {
      return CGFloat(truncating: number)
    }
    if let string = value as? String, let double = Double(string) {
      return CGFloat(double)
    }
    return nil
  }

  private static func readDouble(_ value: Any?) -> Double? {
    if let number = value as? NSNumber {
      return Double(truncating: number)
    }
    if let string = value as? String, let double = Double(string) {
      return double
    }
    return nil
  }

  private static func readUInt32(_ value: Any?) -> UInt32? {
    if let number = value as? NSNumber {
      return number.uint32Value
    }
    if let string = value as? String, let integer = UInt32(string) {
      return integer
    }
    return nil
  }

  private static func readRect(x: Any?, y: Any?, width: Any?, height: Any?) -> NSRect? {
    guard let x = readCGFloat(x),
      let y = readCGFloat(y),
      let width = readCGFloat(width),
      let height = readCGFloat(height),
      width > 0,
      height > 0
    else {
      return nil
    }
    return NSRect(x: x, y: y, width: width, height: height)
  }
}

final class AppModalHostWebView: WKWebView {
  private var topLeftHitRegions: [CGRect]?
  private var capturesAllHitTesting = false

  func setTopLeftHitRegions(_ regions: [CGRect]?, capturesAllHitTesting: Bool = false) {
    topLeftHitRegions = regions
    self.capturesAllHitTesting = capturesAllHitTesting
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      return nil
    }
    if capturesAllHitTesting {
      return super.hitTest(point) ?? self
    }
    guard let topLeftHitRegions else {
      return super.hitTest(point)
    }
    /**
     CDXC:PromptEditor 2026-05-15-12:42:
     The floating prompt editor renders in a transparent full-window WKWebView,
     but only the visible editor pane should intercept AppKit events. Compare
     against React-published top-left hit regions so clicks, scrolls, and pin
     interactions outside the panel pass through to the terminal workspace.

     CDXC:PromptEditor 2026-05-15-13:42:
     WKWebView hit-test coordinates can arrive flipped depending on the AppKit
     view path. Accept both direct and inverted y coordinates, with a small
     rounding margin, so the editor panel itself remains clickable and resizable
     while transparent space still passes through.
     */
    let directPoint = CGPoint(x: point.x, y: point.y)
    let invertedPoint = CGPoint(x: point.x, y: bounds.height - point.y)
    let candidatePoints = isFlipped ? [directPoint, invertedPoint] : [invertedPoint, directPoint]
    let isInsideHitRegion = candidatePoints.contains { candidate in
      topLeftHitRegions.contains { region in
        region.insetBy(dx: -2, dy: -2).contains(candidate)
      }
    }
    if isInsideHitRegion {
      return super.hitTest(point) ?? self
    }
    return nil
  }
}

final class SidebarModalBackdropView: NSView {
  var onDismiss: (() -> Void)?

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.black.withAlphaComponent(0.65).cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard !isHidden, alphaValue > 0, bounds.contains(point) else {
      return nil
    }
    return self
  }

  override func mouseDown(with event: NSEvent) {
    onDismiss?()
  }

  override func rightMouseDown(with event: NSEvent) {
    onDismiss?()
  }

  override func otherMouseDown(with event: NSEvent) {
    onDismiss?()
  }
}

final class WorkspaceInteractionShieldView: NSView {
  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard !isHidden, alphaValue > 0, bounds.contains(point) else {
      return nil
    }
    return self
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func mouseDown(with event: NSEvent) {}

  override func rightMouseDown(with event: NSEvent) {}

  override func otherMouseDown(with event: NSEvent) {}
}

final class ghostexRootView: NSView {
  private static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "webview")

  private struct RootLayoutFrames {
    var divider: CGRect
    var modalHost: CGRect
    var sidebar: CGRect
    var titlebarChrome: CGRect
    var workspace: CGRect
  }

  private struct ActiveFloatingPromptEditor {
    let filePath: String
    let originatingSessionId: String?
    let requestId: String
    let statusFile: String?
  }

  private static let workspaceBarWidth: CGFloat = 54
  /**
   CDXC:ReactTitlebar 2026-05-11-08:03
   The React titlebar uses one shared native layout reservation so AppKit
   traffic lights and web titlebar controls stay aligned from the same chrome
   height instead of drifting between Swift and CSS.

   CDXC:NativeWindowChrome 2026-05-25-07:16:
   The app titlebar should now reserve 35px, not the earlier 45px. Use the
   shared app titlebar height so workspace layout, native hit testing, traffic
   light centering, and the React titlebar bundle agree on the same top chrome.
   */
  private static let reactTitlebarHeight: CGFloat = ghostexAppTitlebarHeight
  private static let sidebarMinWidth: CGFloat = 220
  private static let combinedSidebarMinWidthReduction: CGFloat = 70
  private static let sidebarMaxWidth: CGFloat = 520
  private static let dividerWidth: CGFloat = 6
  private static let defaultSidebarWidth: CGFloat = 260
  private static let sidebarResetWidth: CGFloat = 260
  private static let startupOverlayVisibleDuration: TimeInterval = 2.0
  private static let startupOverlayFadeDuration: TimeInterval = 1.0
  private static let startupOverlayIconOpacity: CGFloat = 0.14
  private static let startupOverlayIconSize: CGFloat = 132
  private static let floatingPromptEditorFrameDefaultsKey = "ghostex.floatingPromptEditor.frame.v1"
  private static let floatingPromptEditorPrewarmRequestId = "ghostex-floating-prompt-editor-prewarm"

  let workspaceView: TerminalWorkspaceView
  var sidebarWebView: WKWebView { sidebarView }
  private let sidebarView: WKWebView
  private let modalHostView: AppModalHostWebView
  private let sidebarModalBackdropView = SidebarModalBackdropView(frame: .zero)
  private let workspaceInteractionShieldView = WorkspaceInteractionShieldView(frame: .zero)
  private let titlebarChromeView: ReactTitlebarChromeView
  private let titlebarChromeWebView: WKWebView
  private let startupOverlayView = NSView(frame: .zero)
  private let startupOverlayIconView = NSImageView(frame: .zero)
  private let scriptBridge: SidebarScriptBridge
  private let sidebarCommandRouter = SidebarCommandRouter()
  private let divider: PaneResizeHandleView
  private let eventEncoder = JSONEncoder()
  private let configureZedOverlay: (ConfigureZedOverlay) -> Void
  private let syncGhosttyTerminalSettings: (SyncGhosttyTerminalSettings) -> Void
  private let applyGhosttyConfigSettings: (ApplyGhosttyConfigSettings) -> Void
  private let openGhosttyConfigFile: () -> Void
  private let openAccessibilityPreferences: () -> Void
  private let openBrowserWindow: (OpenBrowserWindow) -> Void
  private let openZedWorkspace: (OpenZedWorkspace) -> Void
  private let openWorkspaceInFinder: (OpenWorkspaceInFinder) -> Void
  private let openWorkspaceInIde: (OpenWorkspaceInIde) -> Void
  private let showBrowserWindow: () -> Void
  private let setAppTitlebarTitle: (String?) -> Void
  private let setSessionStatusIndicators: (SetSessionStatusIndicators) -> Void
  private let setPetOverlayState: (SetPetOverlayState) -> Void
  private let sendHostEvent: (HostEvent) -> Void
  private let nativeSettingsStore = NativeSettingsStore()
  private var isModalHostReady = false
  private var activeAppModalKind: String?
  private var pendingModalHostOpenMessage: [String: Any]?
  private var latestModalHostSidebarState: [String: Any]?
  private var activeFloatingPromptEditor: ActiveFloatingPromptEditor?
  private var hasPrewarmedFloatingPromptEditor = false
  private var isPrewarmingFloatingPromptEditor = false
  private var floatingPromptEditorPrewarmTempFileURL: URL?
  private var pendingHotkeyPrefix: String?
  private var pendingHotkeyPrefixExpiresAt: Date?
  private var t3CodeRuntimeProcess: Process?
  private var t3RuntimeVisibleSessionCwd: String?
  private var t3RuntimeLivenessTimer: Timer?
  private var codeServerRuntimeProcess: Process?
  private var codeServerRuntimeStartedAt: Date?
  private var titlebarOutsideClickMonitor: Any?
  private var isTitlebarOverlayOpen = false
  private var lastWorkspaceInteractionShieldLogKey: String?
  private var sidebarContextMenuOpenCount = 0
  private lazy var sessionAttentionNotificationController =
    SessionAttentionNotificationController { [weak self] sessionId in
      self?.handleSessionAttentionNotificationClick(sessionId)
    }
  private var sidebarWidth: CGFloat
  private var sidebarSide: SidebarSide = .left

  /**
   CDXC:NativeWorkspaceChrome 2026-04-26-00:47
   Native ghostex keeps the project/workspace rail and main sidebar in one React
   webview, and uses an AppKit drag handle to resize that combined sidebar
   without disturbing the embedded Ghostty terminal area.
   CDXC:NativeSidebarChrome 2026-04-28-01:16
   Users need sidebar restarts and drag resizing to honor a 200px minimum,
   increasing the previous 190px lower bound by 10px without adding fallback
   width behavior.
   CDXC:NativeSidebarChrome 2026-04-28-02:21
   New sidebar sessions should start at 260px, and double-clicking the native
   resize handle should snap the sidebar back to the same 260px width.
   */
  init(
    ghostty: GhostexGhosttyApp,
    sendEvent: @escaping (HostEvent) -> Void,
    configureZedOverlay: @escaping (ConfigureZedOverlay) -> Void,
    syncGhosttyTerminalSettings: @escaping (SyncGhosttyTerminalSettings) -> Void,
    applyGhosttyConfigSettings: @escaping (ApplyGhosttyConfigSettings) -> Void,
    openGhosttyConfigFile: @escaping () -> Void,
    openAccessibilityPreferences: @escaping () -> Void,
    openBrowserWindow: @escaping (OpenBrowserWindow) -> Void,
    openZedWorkspace: @escaping (OpenZedWorkspace) -> Void,
    openWorkspaceInFinder: @escaping (OpenWorkspaceInFinder) -> Void,
    openWorkspaceInIde: @escaping (OpenWorkspaceInIde) -> Void,
    showBrowserWindow: @escaping () -> Void,
    setAppTitlebarTitle: @escaping (String?) -> Void,
    setSessionStatusIndicators: @escaping (SetSessionStatusIndicators) -> Void,
    setPetOverlayState: @escaping (SetPetOverlayState) -> Void
  ) {
    let settingsStore = NativeSettingsStore()
    let storedSidebarChrome = settingsStore.readSidebarChrome()
    self.workspaceView = TerminalWorkspaceView(
      ghostty: ghostty,
      sendEvent: sendEvent,
      initialProjectEditorCompanionWidthRatio: storedSidebarChrome.projectEditorCompanionWidthRatio,
      persistProjectEditorCompanionWidthRatio: { widthRatio in
        settingsStore.persistProjectEditorCompanionWidthRatio(widthRatio)
      }
    )
    self.scriptBridge = SidebarScriptBridge(router: sidebarCommandRouter)
    self.configureZedOverlay = configureZedOverlay
    self.syncGhosttyTerminalSettings = syncGhosttyTerminalSettings
    self.applyGhosttyConfigSettings = applyGhosttyConfigSettings
    self.openGhosttyConfigFile = openGhosttyConfigFile
    self.openAccessibilityPreferences = openAccessibilityPreferences
    self.openBrowserWindow = openBrowserWindow
    self.openZedWorkspace = openZedWorkspace
    self.openWorkspaceInFinder = openWorkspaceInFinder
    self.openWorkspaceInIde = openWorkspaceInIde
    self.showBrowserWindow = showBrowserWindow
    self.setAppTitlebarTitle = setAppTitlebarTitle
    self.setSessionStatusIndicators = setSessionStatusIndicators
    self.setPetOverlayState = setPetOverlayState
    self.sendHostEvent = sendEvent
    self.sidebarWidth = storedSidebarChrome.width ?? Self.defaultSidebarWidth
    self.sidebarSide = nativeSettingsStore.readSidebarSide()
    let configuration = WKWebViewConfiguration()
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHostDiagnostics")
    let modalHostConfiguration = WKWebViewConfiguration()
    modalHostConfiguration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    let titlebarConfiguration = WKWebViewConfiguration()
    titlebarConfiguration.userContentController.add(scriptBridge, name: "ghostexNativeHost")
    titlebarConfiguration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    titlebarConfiguration.userContentController.add(scriptBridge, name: "ghostexNativeHostDiagnostics")
    let cwd =
      ProcessInfo.processInfo.environment["ghostex_WORKSPACE_PATH"]
      ?? FileManager.default.currentDirectoryPath
    let workspaceName = URL(fileURLWithPath: cwd).lastPathComponent
    var bootstrap: [String: Any] = [
      "accessibilityPermissionGranted": AXIsProcessTrusted(),
      "cwd": cwd,
      "homeDir": FileManager.default.homeDirectoryForCurrentUser.path,
      "ghostexHomeDir": GhostexAppStorage.sharedRootDirectory.path,
      "sharedSidebarStorage": GhostexAppStorage.readSharedSidebarStorage(),
      "workspaceName": workspaceName.isEmpty ? "Ghostex" : workspaceName,
    ]
    let storedZedOverlay = nativeSettingsStore.readZedOverlay()
    if let enabled = storedZedOverlay.enabled {
      bootstrap["zedOverlayEnabled"] = enabled
    }
    if let targetApp = storedZedOverlay.targetApp {
      bootstrap["zedOverlayTargetApp"] = targetApp.rawValue
    }
    if let hideTitlebarButton = storedZedOverlay.hideTitlebarButton {
      bootstrap["zedOverlayHideTitlebarButton"] = hideTitlebarButton
    }
    if let zedOverlayEnabled = ProcessInfo.processInfo.environment["ghostex_ZED_OVERLAY_ENABLED"] {
      bootstrap["zedOverlayEnabled"] =
        zedOverlayEnabled == "1" || zedOverlayEnabled.lowercased() == "true"
    }
    if let zedOverlayTargetApp = ProcessInfo.processInfo.environment["ghostex_ZED_OVERLAY_TARGET_APP"]
    {
      bootstrap["zedOverlayTargetApp"] = zedOverlayTargetApp
    }
    if let data = try? JSONSerialization.data(withJSONObject: bootstrap),
      let json = String(data: data, encoding: .utf8)
    {
      let bootstrapScript = WKUserScript(
        source: "window.__ghostex_NATIVE_HOST__ = \(json);",
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      )
      /**
       CDXC:AccessibilityPermissions 2026-04-28-16:57
       Settings are rendered in the full-window modal host, while the sidebar
       state lives in the sidebar webview. Inject the native Accessibility
       grant state into both webviews so settings can show a short disabled
       notice without asking the React layer to infer macOS privacy state.
       */
      configuration.userContentController.addUserScript(bootstrapScript)
      modalHostConfiguration.userContentController.addUserScript(bootstrapScript)
      titlebarConfiguration.userContentController.addUserScript(bootstrapScript)
    }
    configuration.userContentController.addUserScript(
      WKUserScript(
        source: Self.diagnosticsScript,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    titlebarConfiguration.userContentController.addUserScript(
      WKUserScript(
        source: Self.diagnosticsScript,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    self.sidebarView = WKWebView(frame: .zero, configuration: configuration)
    self.modalHostView = AppModalHostWebView(frame: .zero, configuration: modalHostConfiguration)
    self.titlebarChromeWebView = WKWebView(frame: .zero, configuration: titlebarConfiguration)
    self.titlebarChromeView = ReactTitlebarChromeView(webView: titlebarChromeWebView)
    self.divider = PaneResizeHandleView()
    super.init(frame: .zero)
    workspaceView.setSidebarSide(sidebarSide)
    titlebarChromeView.titlebarHeight = Self.reactTitlebarHeight

    sidebarCommandRouter.onCommand = { [weak self] command in
      self?.handleSidebarCommand(command)
    }
    sidebarCommandRouter.onAppModalHostMessage = { [weak self] body in
      self?.handleAppModalHostMessage(body)
    }
    sidebarModalBackdropView.onDismiss = { [weak self] in
      self?.closeAppModalHost(reason: "sidebarBackdropClick")
    }
    divider.onDrag = { [weak self] deltaX in
      self?.resizeSidebar(by: deltaX)
    }
    divider.onDragEnded = { [weak self] in
      self?.persistSidebarWidth()
    }
    divider.onDoubleClick = { [weak self] in
      self?.resetSidebarWidth()
    }

    wantsLayer = true
    layer?.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor.cgColor
    sidebarView.setValue(false, forKey: "drawsBackground")
    modalHostView.setValue(false, forKey: "drawsBackground")
    titlebarChromeWebView.setValue(false, forKey: "drawsBackground")
    modalHostView.isHidden = true
    sidebarModalBackdropView.isHidden = true
    workspaceInteractionShieldView.isHidden = true
    sidebarView.navigationDelegate = self
    addSubview(workspaceView)
    /**
     CDXC:OverlayInteractivity 2026-05-25-07:02:
     Backdrop modals and titlebar dropdowns visually cover native workspace
     tabs, so the workspace needs a native event sink while those overlays are
     open. This clear shield blocks hover, tooltip, and click delivery to
     AppKit pane chrome without changing the visible layering.
     */
    addSubview(workspaceInteractionShieldView)
    /**
     CDXC:NativeWorkspaceChrome 2026-04-26-05:40
     Ghostty surfaces can keep native subviews/layers that draw and receive
     events aggressively. Add the terminal workspace behind the sidebar
     chrome so project/session controls always own their visible hit area.
     */
    addSubview(sidebarView)
    addSubview(divider)
    /**
     CDXC:AppModals 2026-04-26-15:10
     Sidebar dialogs need a full-window React host because WKWebView portals
     cannot escape the sidebar's frame. Keep this transparent overlay above
     terminal chrome, and show it only while a modal is active.
     */
    addSubview(modalHostView)
    addSubview(sidebarModalBackdropView)
    /**
     CDXC:ReactTitlebar 2026-05-12-09:58
     Titlebar controls, tooltips, and dropdowns are React-rendered in one
     transparent WKWebView so Radix portals have enough visual canvas. Native
     hit-testing, not the view frame, decides which pixels are interactive so
     workspace clicks still pass through below the fixed titlebar strip.
     */
    addSubview(titlebarChromeView)
    promoteSidebarChrome()
    installStartupOverlay()
    loadSidebar()
    loadModalHost()
    loadTitlebarChrome()
    installTitlebarOutsideClickMonitor()
  }

  deinit {
    if let titlebarOutsideClickMonitor {
      NSEvent.removeMonitor(titlebarOutsideClickMonitor)
    }
  }

  private func installTitlebarOutsideClickMonitor() {
    titlebarOutsideClickMonitor = NSEvent.addLocalMonitorForEvents(
      matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]
    ) { [weak self] event in
      guard let self, event.window === self.window else {
        return event
      }
      let point = self.convert(event.locationInWindow, from: nil)
      self.dismissSidebarContextMenuForOutsideClick(at: point)
      if self.titlebarChromeView.containsInteractiveHitRegion(
        self.titlebarChromeView.convert(point, from: self)
      ) {
        return event
      }
      /**
       CDXC:ReactTitlebar 2026-05-16-20:01:
       Clicking behind a titlebar dropdown lands in AppKit, not the React
       titlebar document. Close Resources, Actions, and Open menus from a native
       local mouse monitor before the original click continues to the sidebar or
       workspace target.
       */
      self.titlebarChromeView.closeOpenDropdowns()
      return event
    }
  }

  func handleWindowMouseDownBeforeDispatch(_ event: NSEvent) {
    guard Self.isMouseDownEvent(event) else {
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    dismissSidebarContextMenuForOutsideClick(at: point)
  }

  private func dismissSidebarContextMenuForOutsideClick(at pointInRoot: NSPoint) {
    guard sidebarContextMenuOpenCount > 0 else {
      return
    }
    guard !sidebarView.frame.contains(pointInRoot) else {
      return
    }
    /**
     CDXC:SidebarContextMenu 2026-05-21-04:35:
     Terminal panes, titlebar chrome, and other non-sidebar surfaces must close
     open sidebar context menus before the original AppKit click continues.
     */
    dismissSidebarContextMenuFromNativeOutsideClick()
  }

  private static func isMouseDownEvent(_ event: NSEvent) -> Bool {
    switch event.type {
    case .leftMouseDown, .rightMouseDown, .otherMouseDown:
      return true
    default:
      return false
    }
  }

  private func noteSidebarContextMenuOpened() {
    sidebarContextMenuOpenCount += 1
  }

  private func noteSidebarContextMenuClosed() {
    sidebarContextMenuOpenCount = max(0, sidebarContextMenuOpenCount - 1)
  }

  func noteSidebarContextMenuOpenedFromHost() {
    noteSidebarContextMenuOpened()
  }

  func noteSidebarContextMenuClosedFromHost() {
    noteSidebarContextMenuClosed()
  }

  private func dismissSidebarContextMenuFromNativeOutsideClick() {
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.dismissSidebarContextMenu?.();
      undefined;
      """
    )
  }

  private func installStartupOverlay() {
    /**
     CDXC:StartupOverlay 2026-05-15-18:46:
     Startup restores can reorder many sidebar sessions while native panes are
     still reconnecting. Cover the whole app content with the same #0e0e0e
     chrome color, then fade the mask out instead of applying opacity to the
     sidebar itself.

     CDXC:StartupOverlay 2026-05-15-19:05:
     The configured overlay view must also be inserted above the React titlebar
     WKWebView. Without adding it to the root hierarchy, the timer runs but no
     full-app mask can draw over startup churn.

     CDXC:StartupOverlay 2026-05-15-19:13:
     The mask should hold for two seconds, not three, and show the app icon in
     the center as a low-opacity grayscale watermark. Keep the icon inside the
     overlay view so removing the overlay also removes every startup mask hit
     target after the fade completes.
     */
    startupOverlayView.wantsLayer = true
    startupOverlayView.layer?.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor.cgColor
    startupOverlayView.alphaValue = 1
    startupOverlayIconView.image = grayscaleStartupOverlayIconImage()
    startupOverlayIconView.imageScaling = .scaleProportionallyUpOrDown
    startupOverlayIconView.alphaValue = Self.startupOverlayIconOpacity
    startupOverlayIconView.wantsLayer = true
    startupOverlayView.addSubview(startupOverlayIconView)
    addSubview(startupOverlayView, positioned: .above, relativeTo: titlebarChromeView)
    DispatchQueue.main.asyncAfter(deadline: .now() + Self.startupOverlayVisibleDuration) {
      [weak self] in
      self?.fadeOutStartupOverlay()
    }
  }

  private func fadeOutStartupOverlay() {
    guard startupOverlayView.superview === self, startupOverlayView.alphaValue > 0 else {
      return
    }

    NSAnimationContext.runAnimationGroup { context in
      context.duration = Self.startupOverlayFadeDuration
      startupOverlayView.animator().alphaValue = 0
    } completionHandler: { [weak self] in
      self?.startupOverlayView.removeFromSuperview()
    }
  }

  private func grayscaleStartupOverlayIconImage() -> NSImage {
    guard let sourceImage = NSApp.applicationIconImage else {
      return NSImage(size: NSSize(width: Self.startupOverlayIconSize, height: Self.startupOverlayIconSize))
    }
    guard let tiffData = sourceImage.tiffRepresentation,
      let inputImage = CIImage(data: tiffData),
      let filter = CIFilter(name: "CIPhotoEffectMono")
    else {
      return sourceImage
    }

    filter.setValue(inputImage, forKey: kCIInputImageKey)
    guard let outputImage = filter.outputImage else {
      return sourceImage
    }

    let image = NSImage(size: sourceImage.size)
    image.addRepresentation(NSCIImageRep(ciImage: outputImage))
    return image
  }

  func openFloatingEditor(_ command: OpenFloatingEditor) {
    guard command.editorKind == "monaco" else {
      workspaceView.openFloatingEditor(command)
      return
    }
    openFloatingPromptEditor(command)
  }

  func closeTerminal(sessionId: String, preservePersistenceSession: Bool = false) {
    if activeFloatingPromptEditor?.originatingSessionId == sessionId {
      /**
       CDXC:PromptEditor 2026-05-13-09:48
       Closing the terminal that launched Ctrl+G prompt editing should close
       the floating prompt editor and persist the current Monaco buffer first.
       Ask the modal-host editor for its live text instead of marking the
       status cancelled, because the source terminal going away is not a user
       discard action.
       */
      dispatchModalHostMessage([
        "requestId": activeFloatingPromptEditor?.requestId ?? "",
        "type": "floatingPromptEditorCloseAndSave",
      ])
    }
    workspaceView.closeTerminal(
      sessionId: sessionId,
      preservePersistenceSession: preservePersistenceSession)
  }

  private func openFloatingPromptEditor(_ command: OpenFloatingEditor) {
    let interruptedPrewarm = isPrewarmingFloatingPromptEditor
    if interruptedPrewarm {
      finishFloatingPromptEditorPrewarm()
    }
    let requestId = command.requestId ?? "floating-monaco-editor-\(UUID().uuidString)"
    guard let filePath = command.filePath?.trimmingCharacters(in: .whitespacesAndNewlines),
      !filePath.isEmpty
    else {
      writeFloatingPromptEditorStatusFile(command.statusFile, status: "cancelled")
      return
    }
    let initialText = (try? String(contentsOfFile: filePath, encoding: .utf8)) ?? ""
    let language = "markdown"
    if let activeFloatingPromptEditor {
      writeFloatingPromptEditorStatusFile(activeFloatingPromptEditor.statusFile, status: "cancelled")
    }
    activeFloatingPromptEditor = ActiveFloatingPromptEditor(
      filePath: filePath,
      originatingSessionId: command.originatingSessionId,
      requestId: requestId,
      statusFile: command.statusFile
    )
    /**
     CDXC:PromptEditor 2026-05-13-09:48
     Monaco prompt editing uses the same full-window modal WKWebView as other
     app dialogs. Native still owns reading the requested temp file, status
     writes, and final save/cancel semantics so the CLI bridge contract remains
     independent from React rendering.

     CDXC:PromptEditor 2026-05-13-10:22
     Ctrl+G prompt editing is always Markdown and opens as a narrow wrapped
     writing pane. Ignore caller language hints so the modal host consistently
     uses Markdown tokenization and text wrapping for prompt composition.
     */
    let initialFrame = floatingPromptEditorInitialFrame(originatingSessionId: command.originatingSessionId)
    updateFloatingPromptEditorHitRegion(frame: initialFrame)
    PromptEditorDebugLog.append(
      event: "native.open",
      details: [
        "filePath": filePath,
        "initialTextLength": initialText.count,
        "interruptedPrewarm": interruptedPrewarm,
        "modalHostHidden": modalHostView.isHidden,
        "originatingSessionId": command.originatingSessionId ?? "",
        "requestId": requestId,
        "startupOverlayVisible": startupOverlayView.superview === self && startupOverlayView.alphaValue > 0,
      ]
    )
    dispatchModalHostOpenMessage([
      "filePath": filePath,
      "initialFrame": initialFrame,
      "initialText": initialText,
      "language": language,
      "modal": "floatingPromptEditor",
      "requestId": requestId,
      "statusFile": command.statusFile ?? "",
      "title": command.title?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        ? command.title!
        : "Prompt Editor",
      "type": "open",
    ])
  }

  private func dispatchModalHostOpenMessage(_ message: [String: Any]) {
    guard isModalHostReady else {
      pendingModalHostOpenMessage = message
      return
    }
    dispatchModalHostMessage(message)
  }

  /**
   CDXC:PromptEditor 2026-05-19-10:05:
   The first real Ctrl+G prompt-editor open is slow because Monaco and the
   modal host pay one-time startup costs. Open and close one hidden editor
   session as soon as the modal host is ready, ideally while the macOS startup
   overlay is still visible, so later launches reuse the warmed Monaco runtime.
   */
  private func prewarmFloatingPromptEditorIfNeeded() {
    guard !hasPrewarmedFloatingPromptEditor,
      !isPrewarmingFloatingPromptEditor,
      activeFloatingPromptEditor == nil
    else {
      return
    }

    isPrewarmingFloatingPromptEditor = true
    let tempURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-prompt-editor-prewarm-\(UUID().uuidString).md")
    do {
      try "".write(to: tempURL, atomically: true, encoding: .utf8)
    } catch {
      isPrewarmingFloatingPromptEditor = false
      PromptEditorDebugLog.append(
        event: "native.prewarm.tempFileFailed",
        details: ["error": error.localizedDescription]
      )
      return
    }
    PromptEditorDebugLog.append(
      event: "native.prewarm.start",
      details: [
        "modalHostHidden": modalHostView.isHidden,
        "modalHostReady": isModalHostReady,
        "startupOverlayVisible": startupOverlayView.superview === self && startupOverlayView.alphaValue > 0,
      ]
    )
    floatingPromptEditorPrewarmTempFileURL = tempURL
    activeFloatingPromptEditor = ActiveFloatingPromptEditor(
      filePath: tempURL.path,
      originatingSessionId: nil,
      requestId: Self.floatingPromptEditorPrewarmRequestId,
      statusFile: nil
    )
    let initialFrame = floatingPromptEditorInitialFrame(originatingSessionId: nil)
    updateFloatingPromptEditorHitRegion(frame: initialFrame)
    dispatchModalHostOpenMessage([
      "filePath": tempURL.path,
      "initialFrame": initialFrame,
      "initialText": "",
      "language": "markdown",
      "modal": "floatingPromptEditor",
      "prewarm": true,
      "requestId": Self.floatingPromptEditorPrewarmRequestId,
      "statusFile": "",
      "title": "Prompt Editor",
      "type": "open",
    ])
  }

  private func finishFloatingPromptEditorPrewarm() {
    guard isPrewarmingFloatingPromptEditor else {
      return
    }
    PromptEditorDebugLog.append(
      event: "native.prewarm.finish",
      details: [
        "modalHostHidden": modalHostView.isHidden,
        "requestId": Self.floatingPromptEditorPrewarmRequestId,
      ]
    )
    hasPrewarmedFloatingPromptEditor = true
    isPrewarmingFloatingPromptEditor = false
    activeFloatingPromptEditor = nil
    modalHostView.setTopLeftHitRegions(nil)
    dispatchModalHostMessage(["type": "close"])
    modalHostView.isHidden = true
    if let tempURL = floatingPromptEditorPrewarmTempFileURL {
      try? FileManager.default.removeItem(at: tempURL)
      floatingPromptEditorPrewarmTempFileURL = nil
    }
  }

  private func floatingPromptEditorInitialFrame(originatingSessionId: String?) -> [String: CGFloat] {
    let margin: CGFloat = 16
    if let storedFrame = storedFloatingPromptEditorFrame() {
      /**
       CDXC:PromptEditor 2026-05-15-19:27:
       The rich prompt editor is a global writing tool, not project-local UI.
       Reopen it at the last user-sized and user-positioned frame across
       projects and app restarts, clamped to the current window so saved frames
       from other displays or window sizes stay reachable.
       */
      return clampedFloatingPromptEditorFrame(storedFrame)
    }
    let maxWidth = min(CGFloat(400), max(240, bounds.width - margin * 2))
    let maxHeight = max(260, bounds.height - margin * 2)
    let width = maxWidth
    let height = min(CGFloat(320), maxHeight)
    var x = max(margin, (bounds.width - width) / 2)
    var y = margin

    if let sourceFrame = workspaceView.promptEditorSourcePaneFrame(
      originatingSessionId: originatingSessionId)
    {
      let sourceFrameInRoot = workspaceView.convert(sourceFrame, to: self)
      /**
       CDXC:PromptEditor 2026-05-13-15:58
       Ctrl+G Monaco prompt editing should open below the pane that launched it and horizontally centered to that pane when there is room. If the lower workspace does not fit the 320px editor, keep the pane aligned to the bottom of the window instead of moving it above the source pane.
       */
      let belowY = sourceFrameInRoot.minY - margin - height
      x = min(
        max(margin, sourceFrameInRoot.midX - width / 2),
        max(margin, bounds.width - width - margin)
      )
      if belowY >= margin {
        y = belowY
      }
    }

    return [
      "height": height,
      "left": x,
      "top": max(margin, bounds.height - y - height),
      "width": width,
    ]
  }

  private func storedFloatingPromptEditorFrame() -> [String: CGFloat]? {
    guard let stored = UserDefaults.standard.string(forKey: Self.floatingPromptEditorFrameDefaultsKey) else {
      return nil
    }
    let frame = NSRectFromString(stored)
    guard frame.width > 1, frame.height > 1 else {
      return nil
    }
    return [
      "height": frame.height,
      "left": frame.minX,
      "top": frame.minY,
      "width": frame.width,
    ]
  }

  private func persistFloatingPromptEditorFrame(_ frame: [String: CGFloat]) {
    let clampedFrame = clampedFloatingPromptEditorFrame(frame)
    guard let left = clampedFrame["left"],
      let top = clampedFrame["top"],
      let width = clampedFrame["width"],
      let height = clampedFrame["height"]
    else {
      return
    }
    let storedFrame = CGRect(x: left, y: top, width: width, height: height)
    UserDefaults.standard.set(NSStringFromRect(storedFrame), forKey: Self.floatingPromptEditorFrameDefaultsKey)
  }

  private func clampedFloatingPromptEditorFrame(_ frame: [String: CGFloat]) -> [String: CGFloat] {
    let margin: CGFloat = 16
    let availableWidth = max(CGFloat(240), bounds.width - margin * 2)
    let maxWidth = min(CGFloat(700), availableWidth)
    let minWidth = min(CGFloat(180), maxWidth)
    let minHeight = min(CGFloat(260), max(CGFloat(180), bounds.height - margin * 2))
    let width = min(max(frame["width"] ?? 400, minWidth), maxWidth)
    let height = min(
      max(frame["height"] ?? 320, minHeight),
      max(minHeight, bounds.height - margin * 2)
    )
    return [
      "height": height,
      "left": min(max(margin, frame["left"] ?? margin), max(margin, bounds.width - width - margin)),
      "top": min(max(margin, frame["top"] ?? margin), max(margin, bounds.height - height - margin)),
      "width": width,
    ]
  }

  private func updateFloatingPromptEditorHitRegion(
    frame: [String: CGFloat],
    imagePreviewOpen: Bool = false
  ) {
    guard let left = frame["left"],
      let top = frame["top"],
      let width = frame["width"],
      let height = frame["height"]
    else {
      modalHostView.setTopLeftHitRegions([])
      return
    }
    let region = CGRect(x: left, y: top, width: width, height: height)
    modalHostView.setTopLeftHitRegions([region], capturesAllHitTesting: imagePreviewOpen)
    PromptEditorDebugLog.append(
      event: "native.hitRegion.applied",
      details: [
        "height": height,
        "imagePreviewOpen": imagePreviewOpen,
        "left": left,
        "modalHostBoundsHeight": modalHostView.bounds.height,
        "modalHostBoundsWidth": modalHostView.bounds.width,
        "modalHostHidden": modalHostView.isHidden,
        "requestId": activeFloatingPromptEditor?.requestId ?? "",
        "top": top,
        "width": width,
      ]
    )
  }

  private func updateFloatingPromptEditorHitRegion(message: [String: Any]) {
    guard let requestId = message["requestId"] as? String,
      let active = activeFloatingPromptEditor,
      active.requestId == requestId,
      let frame = message["frame"] as? [String: Any]
    else {
      PromptEditorDebugLog.append(
        event: "native.hitRegion.messageIgnored",
        details: [
          "activeRequestId": activeFloatingPromptEditor?.requestId ?? "",
          "hasActiveEditor": activeFloatingPromptEditor != nil,
          "hasFrame": message["frame"] != nil,
          "messageRequestId": message["requestId"] as? String ?? "",
        ]
      )
      return
    }
    let hitRegion = [
      "height": Self.cgFloatValue(frame["height"]),
      "left": Self.cgFloatValue(frame["left"]),
      "top": Self.cgFloatValue(frame["top"]),
      "width": Self.cgFloatValue(frame["width"]),
    ].compactMapValues { $0 }
    let imagePreviewOpen = message["imagePreviewOpen"] as? Bool == true
    let clampedFrame = clampedFloatingPromptEditorFrame(hitRegion)
    if !isPrewarmingFloatingPromptEditor {
      persistFloatingPromptEditorFrame(clampedFrame)
    }
    updateFloatingPromptEditorHitRegion(frame: clampedFrame, imagePreviewOpen: imagePreviewOpen)
  }

  private static func cgFloatValue(_ value: Any?) -> CGFloat? {
    if let value = value as? CGFloat {
      return value
    }
    if let value = value as? Double {
      return CGFloat(value)
    }
    if let value = value as? Int {
      return CGFloat(value)
    }
    return nil
  }

  private func saveFloatingPromptEditor(message: [String: Any]) {
    guard let requestId = message["requestId"] as? String,
      let active = activeFloatingPromptEditor,
      active.requestId == requestId
    else {
      return
    }
    let text = message["text"] as? String ?? ""
    do {
      try text.write(toFile: active.filePath, atomically: true, encoding: .utf8)
      writeFloatingPromptEditorStatusFile(active.statusFile, status: "saved")
      finishFloatingPromptEditor(reason: "saved")
    } catch {
      AppDelegate.appendAppModalErrorLog(
        area: "PromptEditor:save",
        message: "Failed to save prompt editor file \(active.filePath): \(error.localizedDescription)",
        stack: nil
      )
    }
  }

  private func pasteImageIntoFloatingPromptEditor(message: [String: Any]) {
    guard let requestId = message["requestId"] as? String,
      let pasteRequestId = message["pasteRequestId"] as? String,
      let active = activeFloatingPromptEditor,
      active.requestId == requestId
    else {
      return
    }

    do {
      let imagePath = try resolveFloatingPromptEditorClipboardImagePath()
      dispatchModalHostMessage([
        "imagePath": imagePath,
        "pasteRequestId": pasteRequestId,
        "requestId": active.requestId,
        "type": "floatingPromptEditorImagePasteResult",
      ])
    } catch {
      AppDelegate.appendAppModalErrorLog(
        area: "PromptEditor:imagePaste",
        message: error.localizedDescription,
        stack: nil
      )
      dispatchModalHostMessage([
        "error": error.localizedDescription,
        "pasteRequestId": pasteRequestId,
        "requestId": active.requestId,
        "type": "floatingPromptEditorImagePasteResult",
      ])
    }
  }

  private func resolveFloatingPromptEditorClipboardImagePath() throws -> String {
    let pasteboard = NSPasteboard.general
    if let imageFileURL = Self.firstFloatingPromptEditorClipboardImageFileURL(in: pasteboard) {
      let copiedURL = try Self.copyFloatingPromptEditorClipboardImageFile(imageFileURL)
      return Self.floatingPromptEditorDisplayImagePath(for: copiedURL)
    }

    guard let pngData = Self.floatingPromptEditorClipboardPNGData(in: pasteboard) else {
      throw NSError(
        domain: "com.madda.ghostex.promptEditor.imagePaste",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Clipboard does not contain an image."]
      )
    }

    /**
     CDXC:PromptEditor 2026-05-16-21:21:
     Rich prompt image paste must produce a durable Markdown file reference.
     Store unsaved clipboard bitmaps under Ghostex-owned storage before React
     inserts [Image #N](path) into Monaco.

     CDXC:PromptEditor 2026-05-16-22:56:
     Pasted image paths must stay short enough to read on one prompt-editor
     line. Always copy image files and unsaved bitmap data into ~/.ghostex/i
     with a compact timestamp filename, then insert the tilde path instead of
     the original absolute source path.
     */
    let fileURL = try Self.uniqueFloatingPromptEditorImageURL(pathExtension: "png")
    try pngData.write(to: fileURL, options: .atomic)
    return Self.floatingPromptEditorDisplayImagePath(for: fileURL)
  }

  private static func copyFloatingPromptEditorClipboardImageFile(_ sourceURL: URL) throws -> URL {
    let fileURL = try uniqueFloatingPromptEditorImageURL(
      pathExtension: normalizedFloatingPromptEditorImageFileExtension(sourceURL.pathExtension))
    try FileManager.default.copyItem(at: sourceURL, to: fileURL)
    return fileURL
  }

  private static func uniqueFloatingPromptEditorImageURL(pathExtension: String) throws -> URL {
    let directory = GhostexAppStorage.sharedRootDirectory.appendingPathComponent("i", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyMMddHHmmss"
    let baseName = formatter.string(from: Date())
    let normalizedExtension = normalizedFloatingPromptEditorImageFileExtension(pathExtension)
    let firstURL = directory.appendingPathComponent("\(baseName).\(normalizedExtension)", isDirectory: false)
    guard FileManager.default.fileExists(atPath: firstURL.path) else {
      return firstURL
    }

    for index in 2...99 {
      let candidate = directory.appendingPathComponent(
        "\(baseName)-\(index).\(normalizedExtension)",
        isDirectory: false
      )
      if !FileManager.default.fileExists(atPath: candidate.path) {
        return candidate
      }
    }

    return directory.appendingPathComponent(
      "\(baseName)-\(UUID().uuidString.lowercased().prefix(4)).\(normalizedExtension)",
      isDirectory: false
    )
  }

  private static func normalizedFloatingPromptEditorImageFileExtension(_ pathExtension: String) -> String {
    let normalizedExtension = pathExtension.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    if normalizedExtension == "jpeg" {
      return "jpg"
    }
    if normalizedExtension == "tiff" {
      return "tif"
    }
    return normalizedExtension.isEmpty ? "png" : normalizedExtension
  }

  private static func floatingPromptEditorDisplayImagePath(for fileURL: URL) -> String {
    "~/.ghostex/i/\(fileURL.lastPathComponent)"
  }

  private func loadFloatingPromptEditorImagePreview(message: [String: Any]) {
    guard let requestId = message["requestId"] as? String,
      let previewRequestId = message["previewRequestId"] as? String,
      let path = message["path"] as? String,
      let active = activeFloatingPromptEditor,
      active.requestId == requestId
    else {
      return
    }

    do {
      let dataUrl = try Self.floatingPromptEditorImagePreviewDataURL(path: path)
      dispatchModalHostMessage([
        "dataUrl": dataUrl,
        "path": path,
        "previewRequestId": previewRequestId,
        "requestId": active.requestId,
        "type": "floatingPromptEditorImagePreviewResult",
      ])
    } catch {
      AppDelegate.appendAppModalErrorLog(
        area: "PromptEditor:imagePreview",
        message: error.localizedDescription,
        stack: nil
      )
      dispatchModalHostMessage([
        "error": error.localizedDescription,
        "path": path,
        "previewRequestId": previewRequestId,
        "requestId": active.requestId,
        "type": "floatingPromptEditorImagePreviewResult",
      ])
    }
  }

  private static func floatingPromptEditorImagePreviewDataURL(path: String) throws -> String {
    /**
     CDXC:PromptEditor 2026-05-16-23:01:
     The rich prompt editor thumbnail shelf must load every image path already
     present in Monaco text. Resolve short ~/.ghostex/i paths natively and send
     display-safe data URLs back to React so WKWebView local-file read limits do
     not block thumbnail or popup rendering.
     */
    guard let fileURL = floatingPromptEditorImageFileURL(path: path),
      FileManager.default.fileExists(atPath: fileURL.path),
      isFloatingPromptEditorImageFileURL(fileURL)
    else {
      throw NSError(
        domain: "com.madda.ghostex.promptEditor.imagePreview",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Image preview path does not point to a local image."]
      )
    }

    let data = try Data(contentsOf: fileURL)
    if fileURL.pathExtension.lowercased() == "svg" {
      return "data:image/svg+xml;base64,\(data.base64EncodedString())"
    }
    guard let image = NSImage(data: data),
      let pngData = floatingPromptEditorPreviewPNGData(from: image)
    else {
      throw NSError(
        domain: "com.madda.ghostex.promptEditor.imagePreview",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "Image preview data could not be decoded."]
      )
    }
    return "data:image/png;base64,\(pngData.base64EncodedString())"
  }

  private static func floatingPromptEditorImageFileURL(path: String) -> URL? {
    let trimmedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmedPath.hasPrefix("file://"), let url = URL(string: trimmedPath), url.isFileURL {
      return url
    }
    if trimmedPath.hasPrefix("~/.ghostex/") {
      let relativePath = String(trimmedPath.dropFirst("~/.ghostex/".count))
      return GhostexAppStorage.sharedRootDirectory.appendingPathComponent(relativePath)
    }
    if trimmedPath.hasPrefix("~/") {
      let relativePath = String(trimmedPath.dropFirst(2))
      return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(relativePath)
    }
    if trimmedPath.hasPrefix("/") {
      return URL(fileURLWithPath: trimmedPath)
    }
    return nil
  }

  private static func floatingPromptEditorPreviewPNGData(from image: NSImage) -> Data? {
    let sourceSize = image.size.width > 0 && image.size.height > 0 ? image.size : NSSize(width: 1, height: 1)
    let maximumDimension = CGFloat(1600)
    let scale = min(1, maximumDimension / max(sourceSize.width, sourceSize.height))
    let drawSize = NSSize(width: max(1, sourceSize.width * scale), height: max(1, sourceSize.height * scale))
    let output = NSImage(size: drawSize)
    output.lockFocus()
    NSColor.clear.setFill()
    NSRect(origin: .zero, size: drawSize).fill()
    image.draw(
      in: NSRect(origin: .zero, size: drawSize),
      from: NSRect(origin: .zero, size: sourceSize),
      operation: .sourceOver,
      fraction: 1.0
    )
    output.unlockFocus()
    guard let tiffData = output.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiffData)
    else {
      return nil
    }
    return bitmap.representation(using: .png, properties: [:])
  }

  private static func firstFloatingPromptEditorClipboardImageFileURL(in pasteboard: NSPasteboard) -> URL? {
    let fileURLType = NSPasteboard.PasteboardType("public.file-url")
    for item in pasteboard.pasteboardItems ?? [] {
      guard let fileURLString = item.string(forType: fileURLType),
        let fileURL = URL(string: fileURLString),
        fileURL.isFileURL,
        FileManager.default.fileExists(atPath: fileURL.path),
        isFloatingPromptEditorImageFileURL(fileURL)
      else {
        continue
      }
      return fileURL
    }

    let filenamesType = NSPasteboard.PasteboardType("NSFilenamesPboardType")
    guard let filenames = pasteboard.propertyList(forType: filenamesType) as? [String] else {
      return nil
    }
    return filenames
      .map { URL(fileURLWithPath: $0) }
      .first { fileURL in
        FileManager.default.fileExists(atPath: fileURL.path)
          && isFloatingPromptEditorImageFileURL(fileURL)
      }
  }

  private static func isFloatingPromptEditorImageFileURL(_ url: URL) -> Bool {
    let pathExtension = url.pathExtension.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !pathExtension.isEmpty else {
      return false
    }
    if let type = UTType(filenameExtension: pathExtension), type.conforms(to: .image) {
      return true
    }
    return ["avif", "gif", "heic", "heif", "jpg", "jpeg", "png", "svg", "tif", "tiff", "webp"]
      .contains(pathExtension.lowercased())
  }

  private static func floatingPromptEditorClipboardPNGData(in pasteboard: NSPasteboard) -> Data? {
    let pngType = NSPasteboard.PasteboardType("public.png")
    if let pngData = pasteboard.data(forType: pngType), NSImage(data: pngData) != nil {
      return pngData
    }

    let tiffType = NSPasteboard.PasteboardType("public.tiff")
    if let tiffData = pasteboard.data(forType: tiffType),
      let image = NSImage(data: tiffData)
    {
      return floatingPromptEditorPNGData(from: image)
    }

    guard let image = NSImage(pasteboard: pasteboard) else {
      return nil
    }
    return floatingPromptEditorPNGData(from: image)
  }

  private static func floatingPromptEditorPNGData(from image: NSImage) -> Data? {
    guard let tiffData = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiffData)
    else {
      return nil
    }
    return bitmap.representation(using: .png, properties: [:])
  }

  private func cancelFloatingPromptEditor(message: [String: Any]) {
    guard let requestId = message["requestId"] as? String,
      let active = activeFloatingPromptEditor,
      active.requestId == requestId
    else {
      return
    }
    writeFloatingPromptEditorStatusFile(active.statusFile, status: "cancelled")
    finishFloatingPromptEditor(reason: "cancelled")
  }

  private func finishFloatingPromptEditor(reason: String) {
    let returnFocusSessionId = activeFloatingPromptEditor?.originatingSessionId
    PromptEditorDebugLog.append(
      event: "native.finish",
      details: [
        "reason": reason,
        "requestId": activeFloatingPromptEditor?.requestId ?? "",
        "returnFocusSessionId": returnFocusSessionId ?? "",
      ]
    )
    activeFloatingPromptEditor = nil
    modalHostView.setTopLeftHitRegions(nil)
    dispatchModalHostMessage(["type": "close"])
    modalHostView.isHidden = true
    if let returnFocusSessionId {
      workspaceView.focusTerminal(sessionId: returnFocusSessionId, reason: "floatingPromptEditor.\(reason)")
    }
  }

  private func writeFloatingPromptEditorStatusFile(_ statusFile: String?, status: String) {
    guard let statusFile = statusFile?.trimmingCharacters(in: .whitespacesAndNewlines),
      !statusFile.isEmpty
    else {
      return
    }
    do {
      let url = URL(fileURLWithPath: statusFile)
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true
      )
      try "\(status)\n".write(to: url, atomically: true, encoding: .utf8)
    } catch {
      AppDelegate.appendAppModalErrorLog(
        area: "PromptEditor:status",
        message: "Failed to write prompt editor status \(status): \(error.localizedDescription)",
        stack: nil
      )
    }
  }

  func postHostEvent(_ event: HostEvent) {
    guard let data = try? eventEncoder.encode(event),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    let script = """
      window.dispatchEvent(new CustomEvent('ghostex-native-host-event', { detail: \(json) }));
      /**
       CDXC:NativeBridge 2026-04-29-22:03
       Native-to-sidebar event delivery is signaled through DOM events; return
       undefined so WebKit never treats a CustomEvent return object as a bridge
       failure.
       */
      undefined;
      """
    sidebarView.evaluateJavaScript(script)
    /**
     CDXC:ReactTitlebar 2026-05-09-17:34
     The React titlebar can request native process work for Git stats. Broadcast
     host events to that webview too so processResult replies resolve in the
     same bridge contract used by the sidebar.
     */
    titlebarChromeWebView.evaluateJavaScript(script)
  }

  private func handleSessionAttentionNotificationClick(_ sessionId: String) {
    /**
     CDXC:SessionAttentionNotifications 2026-05-10-16:46
     Sidebar-hosted notification commands still need click routing through the
     native event bus so direct WKWebView messages and WebSocket bridge clients
     focus sessions with the same project/pane activation behavior.
     */
    let event = HostEvent.sessionAttentionNotificationClicked(sessionId: sessionId)
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
    sendHostEvent(event)
  }

  private func javaScriptStringLiteral(_ value: String) -> String? {
    guard let data = try? JSONEncoder().encode(value) else {
      return nil
    }
    return String(data: data, encoding: .utf8)
  }

  func applyNativeZedOverlayDetached(targetApp: ZedOverlayTargetApp) {
    guard let json = javaScriptStringLiteral(targetApp.rawValue) else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SETTINGS__?.detachZedOverlay(\(json));
      """)
  }

  func applyNativeZedOverlayAttached(targetApp: ZedOverlayTargetApp) {
    guard let json = javaScriptStringLiteral(targetApp.rawValue) else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SETTINGS__?.attachZedOverlay(\(json));
      """)
  }

  func applyNativeTitlebarZedOverlay(
    enabled: Bool,
    hideTitlebarButton: Bool,
    targetApp: ZedOverlayTargetApp
  ) {
    /**
     CDXC:ReactTitlebar 2026-05-09-17:11
     Native Settings and attachment flows can change IDE attachment state
     outside the titlebar webview. Push the authoritative state back into
     React so the titlebar button mirrors the native overlay controller.
     */
    guard
      let data = try? JSONSerialization.data(withJSONObject: [
        "enabled": enabled,
        "hideTitlebarButton": hideTitlebarButton,
        "targetApp": targetApp.rawValue,
      ]),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    titlebarChromeWebView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.setZedOverlay(\(json));
      undefined;
      """)
  }

  func applyReactTitlebarProjectState(_ command: SetActiveTerminalSet) {
    /**
     CDXC:ReactTitlebar 2026-05-11-00:22
     The titlebar project controls are rendered in their own WKWebView, while
     the sidebar remains authoritative for active project, project editor, and
     diff state. Push only the compact project payload React needs instead of
     letting the titlebar infer state by running separate Git or code-server
     checks.
     CDXC:ModeSwitcher 2026-05-15-18:20:
     The titlebar's selected Agents/Code/Git/Project segment must come from
     the same sidebar layout sync that restores the visible workspace surface,
     so a launch directly into Code mode cannot leave Agents highlighted.
     */
    var payload: [String: Any] = [:]
    if let activeProjectMode = command.activeProjectMode {
      payload["activeMode"] = activeProjectMode
    }
    if let activeProjectId = command.activeProjectId {
      payload["projectId"] = activeProjectId
    }
    payload["projectIconDataUrl"] = command.activeProjectIconDataUrl ?? NSNull()
    if let activeProjectName = command.activeProjectName {
      payload["projectName"] = activeProjectName
    }
    if let activeProjectPath = command.activeProjectPath {
      payload["projectPath"] = activeProjectPath
    }
    if let isFocusModeActive = command.isFocusModeActive {
      payload["isFocusModeActive"] = isFocusModeActive
    }
    if let debuggingMode = command.debuggingMode {
      payload["debuggingMode"] = debuggingMode
    }
    if let status = command.activeProjectEditorStatus {
      payload["editorStatus"] = status
    }
    if let isOpen = command.activeProjectEditorIsOpen {
      payload["editorIsOpen"] = isOpen
    }
    if let isSleeping = command.activeProjectEditorIsSleeping {
      payload["editorIsSleeping"] = isSleeping
    }
    if let companionPaneHidden = command.activeProjectEditorCompanionPaneHidden {
      payload["projectEditorCompanionPaneHidden"] = companionPaneHidden
    }
    if let showFileCount = command.showProjectEditorDiffFileCount {
      payload["showProjectEditorDiffFileCount"] = showFileCount
    }
    if let petOverlayEnabled = command.petOverlayEnabled {
      payload["petOverlayEnabled"] = petOverlayEnabled
    }
    if let stats = command.activeProjectDiffStats {
      payload["diffStats"] = [
        "additions": stats.additions,
        "deletions": stats.deletions,
        "files": stats.files,
        "isLoading": stats.isLoading,
        "isRepo": stats.isRepo,
      ]
    }
    if let git = command.activeProjectGitState {
      /**
       CDXC:TitlebarGit 2026-05-24-17:41:
       The titlebar Git split button must mirror the sidebar-owned git status instead of polling separately, so disabled states and commit/push/PR labels stay identical across chrome and sidebar.
       */
      payload["git"] = [
        "additions": git.additions,
        "aheadCount": git.aheadCount,
        "behindCount": git.behindCount,
        "branch": git.branch ?? NSNull(),
        "confirmSuggestedCommit": git.confirmSuggestedCommit,
        "deletions": git.deletions,
        "files": git.files.map { file in
          [
            "additions": file.additions,
            "deletions": file.deletions,
            "path": file.path,
          ] as [String: Any]
        },
        "generateCommitBody": git.generateCommitBody,
        "hasGitHubCli": git.hasGitHubCli,
        "hasOriginRemote": git.hasOriginRemote,
        "hasUpstream": git.hasUpstream,
        "hasWorkingTreeChanges": git.hasWorkingTreeChanges,
        "isBusy": git.isBusy,
        "isRepo": git.isRepo,
        "isWorktree": git.isWorktree,
        "pr": git.pr.map { pr in
          [
            "number": pr.number ?? NSNull(),
            "state": pr.state,
            "title": pr.title,
            "url": pr.url,
          ] as [String: Any]
        } ?? NSNull(),
        "primaryAction": git.primaryAction,
        "worktreeName": git.worktreeName ?? NSNull(),
      ]
    }
    if let sidebarActions = command.sidebarActions {
      payload["sidebarActions"] = [
        "commands": sidebarActions.commands?.map { command in
          var item: [String: Any] = [
            "actionType": command.actionType,
            "closeTerminalOnExit": command.closeTerminalOnExit ?? false,
            "commandId": command.commandId,
            "isDefault": command.isDefault ?? false,
            "name": command.name,
            "playCompletionSound": command.playCompletionSound ?? false,
          ]
          if let commandText = command.command {
            item["command"] = commandText
          }
          if let icon = command.icon {
            item["icon"] = icon
          }
          if let iconColor = command.iconColor {
            item["iconColor"] = iconColor
          }
          if let url = command.url {
            item["url"] = url
          }
          return item
        } ?? []
      ]
    }
    if let resourceGroups = command.titlebarResourceGroups {
      /**
       CDXC:TitlebarResources 2026-05-16-16:08:
       Forward the sidebar-owned session grouping into the isolated React
       titlebar so its resource dropdown can render Quick/project sections
       while the titlebar webview polls process metrics independently.
       */
      payload["resourceGroups"] = resourceGroups.map { group in
        var item: [String: Any] = [
          "groupId": group.groupId,
          "isActive": group.isActive,
          "projectName": group.projectName,
          "projectPath": group.projectPath,
          "sessions": group.sessions.map { session in
            var sessionItem: [String: Any] = [
              "activity": session.activity,
              "isRunning": session.isRunning,
              "sessionId": session.sessionId,
              "title": session.title,
            ]
            if let agentIcon = session.agentIcon {
              sessionItem["agentIcon"] = agentIcon
            }
            if let isSleeping = session.isSleeping {
              sessionItem["isSleeping"] = isSleeping
            }
            if let lastInteractionAt = session.lastInteractionAt {
              sessionItem["lastInteractionAt"] = lastInteractionAt
            }
            if let projectId = session.projectId {
              sessionItem["projectId"] = projectId
            }
            if let sessionKind = session.sessionKind {
              sessionItem["sessionKind"] = sessionKind
            }
            if let sessionPersistenceName = session.sessionPersistenceName {
              sessionItem["sessionPersistenceName"] = sessionPersistenceName
            }
            if let sessionPersistenceProvider = session.sessionPersistenceProvider {
              sessionItem["sessionPersistenceProvider"] = sessionPersistenceProvider
            }
            if let terminalTitle = session.terminalTitle {
              sessionItem["terminalTitle"] = terminalTitle
            }
            return sessionItem
          },
          "title": group.title,
        ]
        if let projectId = group.projectId {
          item["projectId"] = projectId
        }
        return item
      }
    }
    if let sessionPersistenceProvider = command.sessionPersistenceProvider {
      payload["sessionPersistenceProvider"] = sessionPersistenceProvider
    }
    /**
     CDXC:TitlebarResources 2026-05-17-01:25:
     Browser process rows need user-facing tab/view names from native CEF hosts,
     not raw Chromium process labels. Include the workspace-owned Browser tab
     inventory beside sidebar session groups so React can nest renderer
     processes under the tab title and URL that caused the memory usage.
     */
    payload["browserTabs"] = workspaceView.titlebarBrowserResourceTabs()
    if let openTargets = command.workspaceOpenTargets {
      let availability = openTargets.availability
      let availabilityDict: [String: Any] = [
        "availableTargetIds": availability?.availableTargetIds ?? [],
        "checkedAtMs": availability?.checkedAtMs ?? 0,
        "resolvedAppNames": availability?.resolvedAppNames ?? [:],
        "resolvedCommands": availability?.resolvedCommands ?? [:],
      ]
      let customTargetsList: [[String: Any]] = openTargets.customTargets?.map { target in
        [
          "args": target.args ?? [],
          "command": target.command,
          "id": target.id,
          "label": target.label,
        ] as [String: Any]
      } ?? []
      payload["workspaceOpenTargets"] = [
        "availability": availabilityDict,
        "customTargets": customTargetsList,
        "hiddenTargetIds": openTargets.hiddenTargetIds ?? [],
      ] as [String: Any]
    }
    guard
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    titlebarChromeWebView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.setActiveProjectState(\(json));
      undefined;
      """)
  }

  func setReactTitlebarHitRegions(_ regions: [ReactTitlebarHitRegion], overlayOpen: Bool) {
    titlebarChromeView.setHitRegions(regions, overlayOpen: overlayOpen)
    isTitlebarOverlayOpen = overlayOpen
    updateWorkspaceInteractionShield()
    needsLayout = true
  }

  private func openActiveProjectEditorFromTitlebar() {
    /**
     CDXC:TitlebarOpenIn 2026-05-11-00:22
     Titlebar Code and Embedded Editor clicks must enter the same sidebar-owned
     project-editor flow as the project header. Forward the command into the
     sidebar webview instead of reimplementing code-server startup in Swift.

     CDXC:ModeSwitcher 2026-05-16-07:23:
     Code-tab lag reports need a native bridge timestamp before and after the
     sidebar JavaScript hop while Settings Debugging Mode is enabled. These are
     regular diagnostics, so they must not bypass the debug-mode log gate.
     */
    AppDelegate.appendSessionTitleDebugLog(
      event: "titlebarCodeLag.swiftForwardStart",
      details: AppDelegate.jsonObjectString([
        "timeInterval": "\(Date().timeIntervalSince1970)"
      ]))
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.openActiveProjectEditorFromTitlebar?.();
      undefined;
      """
    ) { _, error in
      AppDelegate.appendSessionTitleDebugLog(
        event: "titlebarCodeLag.swiftForwardCompleted",
        details: AppDelegate.jsonObjectString([
          "error": error?.localizedDescription ?? "",
          "timeInterval": "\(Date().timeIntervalSince1970)",
        ]))
    }
  }

  private func exitFocusModeFromTitlebar() {
    /**
     CDXC:SessionFocusMode 2026-05-23-14:35:
     The titlebar exit-focus control must restore the pre-focus Code/Git/Project surface through the same sidebar-owned toggle path as native pane-tab double click.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.exitFocusModeFromTitlebar?.();
      undefined;
      """)
  }

  private func openAgentsModeFromTitlebar() {
    /**
     CDXC:ModeSwitcher 2026-05-15-12:38:
     Titlebar mode buttons are chrome controls, while the sidebar webview owns
     project/session mode transitions. Forward Agents mode there so native
     layout state and the sessions sidebar stay synchronized.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.openAgentsModeFromTitlebar?.();
      undefined;
      """)
  }

  private func openGitHubProjectFromTitlebar() {
    /**
     CDXC:ModeSwitcher 2026-05-15-12:38:
     Git mode must open the active project's GitHub remote inside the workarea,
     not in an external browser, so the sidebar owner resolves the remote and
     creates or focuses the browser-backed project surface.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.openGitHubProjectFromTitlebar?.();
      undefined;
      """)
  }

  private func showProjectEditorCompanionFromTitlebar() {
    /**
     CDXC:ProjectEditorCompanion 2026-05-16-14:42:
     The titlebar restore button must clear the sidebar-owned project
     preference before native reopens the agent side pane. Forward through
     React state so Code, Git, and Project modes continue sharing one value.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.showProjectEditorCompanionFromTitlebar?.();
      undefined;
      """)
  }

  private func openTasksPlaceholderFromTitlebar() {
    /**
     CDXC:ModeSwitcher 2026-05-15-12:38:
     Project mode is currently a bundled placeholder React page backed by the
     existing tasks bridge. Let the sidebar owner open it as a project workarea
     surface so it keeps the sessions list visible like Code mode.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.openTasksPlaceholderFromTitlebar?.();
      undefined;
      """)
  }

  private func runSidebarCommandFromTitlebar(_ command: RunSidebarCommandFromTitlebar) {
    /**
     CDXC:TitlebarActions 2026-05-11-02:46
     The React titlebar can render the relocated Actions split button, but the
     sidebar webview owns command execution state. Forward the command id into
     that webview so existing action launches and run feedback stay unchanged.

     CDXC:TitlebarActions 2026-05-15-18:05
     Titlebar action clicks must not pass a bare Swift String to
     JSONSerialization because Foundation raises an Objective-C exception for
     invalid top-level JSON types before the sidebar can receive the command.
     Encode command ids as JSON string literals so terminal actions reach the
     command-pane runner.
     */
    guard let commandIdJson = javaScriptStringLiteral(command.commandId) else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.runSidebarCommandFromTitlebar?.(\(commandIdJson));
      undefined;
      """)
  }

  private func runSidebarGitActionFromTitlebar(_ command: RunSidebarGitActionFromTitlebar) {
    /**
     CDXC:TitlebarGit 2026-05-24-17:41:
     The React titlebar owns only the compact Git split-button chrome. Forward commit/push/PR actions into the sidebar webview so one owner keeps git status, generated commit-message prompts, toasts, and PR browser opening synchronized.
     */
    guard let actionJson = javaScriptStringLiteral(command.action) else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.runSidebarGitActionFromTitlebar?.(\(actionJson));
      undefined;
      """)
  }

  private func sleepInactiveSessionsFromTitlebar(_ command: SleepInactiveSessionsFromTitlebar) {
    /**
     CDXC:TitlebarResources 2026-05-16-19:53:
     The React titlebar owns the Resources dropdown button, but the sidebar
     owns session sleep state. Forward the selected session ids as JSON so the
     sidebar can revalidate activity and age before sleeping inactive agents.
     */
    guard let sessionIdsData = try? JSONSerialization.data(withJSONObject: command.sessionIds),
      let sessionIdsJson = String(data: sessionIdsData, encoding: .utf8)
    else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.sleepInactiveSessionsFromTitlebar?.(\(sessionIdsJson));
      undefined;
      """)
  }

  private func quitResourcesFromTitlebar(_ command: QuitResourcesFromTitlebar) {
    /**
     CDXC:TitlebarResources 2026-05-21-16:38:
     React titlebar resource Quit controls can only identify sidebar-owned
     sessions and project editors. Forward those ids to the sidebar webview so
     app state, persisted cards, and native surfaces close from one owner.
     */
    guard
      let sessionIdsData = try? JSONSerialization.data(withJSONObject: command.sessionIds),
      let sessionIdsJson = String(data: sessionIdsData, encoding: .utf8),
      let projectIdsData = try? JSONSerialization.data(withJSONObject: command.projectIds),
      let projectIdsJson = String(data: projectIdsData, encoding: .utf8)
    else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.quitResourcesFromTitlebar?.(\(sessionIdsJson), \(projectIdsJson));
      undefined;
      """)
  }

  private func rotateActivePaneLayoutClockwiseFromTitlebar() {
    if NativeDebugLogging.isEnabled {
      print("[ghostex-titlebar] forwarding rotate panes command to sidebar webview")
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.rotateActivePaneLayoutClockwiseFromTitlebar?.();
      undefined;
      """)
  }

  private func toggleCommandsPanelFromTitlebar() {
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.toggleCommandsPanelFromTitlebar?.();
      undefined;
      """)
  }

  private func togglePetOverlayFromTitlebar() {
    /**
     CDXC:PetOverlay 2026-05-15-00:36:
     The React titlebar can request pet wake/sleep, but the sidebar webview
     remains the settings owner. Forward the action there instead of editing
     shared settings directly in AppKit.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.togglePetOverlayFromTitlebar?.();
      undefined;
      """)
  }

  func sleepPetOverlayFromPet() {
    /**
     CDXC:PetOverlay 2026-05-21-02:19:
     The pet right-click menu exposes only Sleep Pet. Forward a one-way sleep
     command to the sidebar settings owner instead of reusing the titlebar
     toggle, because the context-menu action should never wake the overlay.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.sleepPetOverlayFromPet?.();
      undefined;
      """)
  }

  private func refreshWorkspaceOpenTargetAvailabilityFromTitlebar() {
    /**
     CDXC:TitlebarOpenIn 2026-05-11-03:13
     The titlebar reload button lives in the React titlebar, but installed IDE
     detection lives in the sidebar runtime beside settings persistence. Forward
     the click so manual refresh uses the same detector as startup.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.refreshWorkspaceOpenTargetAvailabilityFromTitlebar?.();
      undefined;
      """)
  }

  private func handleSidebarCommand(_ command: HostCommand) {
    switch command {
    case .createTerminal(let command):
      workspaceView.createTerminal(command)
    case .createWebPane(let command):
      workspaceView.createWebPane(command)
    case .openFloatingEditor(let command):
      openFloatingEditor(command)
    case .closeTerminal(let command):
      closeTerminal(
        sessionId: command.sessionId,
        preservePersistenceSession: command.preservePersistenceSession == true)
    case .closeWebPane(let command):
      workspaceView.closeWebPane(sessionId: command.sessionId)
    case .focusTerminal(let command):
      focusWorkspaceSessionAfterSidebarActivation(sessionId: command.sessionId, kind: .terminal)
    case .focusWebPane(let command):
      focusWorkspaceSessionAfterSidebarActivation(sessionId: command.sessionId, kind: .webPane)
    case .reloadWebPane(let command):
      workspaceView.reloadWebPane(sessionId: command.sessionId)
    case .startT3CodeRuntime(let command):
      startT3CodeRuntime(command)
    case .setT3CodeRuntimeSessionState(let command):
      setT3CodeRuntimeSessionState(command, reason: "nativeSidebar")
    case .stopT3CodeRuntime:
      stopT3CodeRuntime(logPrefix: "nativeSidebar")
    case .startCodeServerRuntime(let command):
      AppDelegate.appendSessionTitleDebugLog(
        event: "titlebarCodeLag.swiftStartCodeServerRuntimeReceived",
        details: AppDelegate.jsonObjectString([
          "cwd": command.cwd,
          "timeInterval": "\(Date().timeIntervalSince1970)",
        ]))
      startCodeServerRuntime(command)
    case .stopCodeServerRuntime:
      stopCodeServerRuntime(logPrefix: "nativeSidebar")
    case .createProjectEditorPane(let command):
      AppDelegate.appendSessionTitleDebugLog(
        event: "titlebarCodeLag.swiftCreateProjectEditorPaneReceived",
        details: AppDelegate.jsonObjectString([
          "projectId": command.projectId,
          "timeInterval": "\(Date().timeIntervalSince1970)",
          "title": command.title,
        ]))
      workspaceView.createProjectEditorPane(command)
    case .focusProjectEditorPane(let command):
      AppDelegate.appendSessionTitleDebugLog(
        event: "titlebarCodeLag.swiftFocusProjectEditorPaneReceived",
        details: AppDelegate.jsonObjectString([
          "projectId": command.projectId,
          "timeInterval": "\(Date().timeIntervalSince1970)",
        ]))
      workspaceView.focusProjectEditorPane(projectId: command.projectId)
    case .closeProjectEditorPane(let command):
      workspaceView.closeProjectEditorPane(projectId: command.projectId)
    case .activateApp:
      activateAppWindow()
    case .writeTerminalText(let command):
      workspaceView.writeTerminalText(sessionId: command.sessionId, text: command.text)
    case .writeTerminalScript(let command):
      workspaceView.writeTerminalScript(sessionId: command.sessionId, text: command.text)
    case .sendTerminalEnter(let command):
      workspaceView.sendTerminalEnter(sessionId: command.sessionId)
    case .readTerminalText(let command):
      workspaceView.readTerminalText(command)
    case .checkPersistenceSession(let command):
      workspaceView.checkPersistenceSession(command)
    case .setActiveTerminalSet(let command):
      setAppTitlebarTitle(command.appTitle)
      applyReactTitlebarProjectState(command)
      workspaceView.setActiveTerminalSet(command)
    case .setSessionStatusIndicators(let command):
      setSessionStatusIndicators(command)
    case .setPetOverlayState(let command):
      setPetOverlayState(command)
    case .showSessionAttentionNotification(let command):
      sessionAttentionNotificationController.show(command)
    case .setTerminalLayout(let command):
      workspaceView.setTerminalLayout(command.layout)
    case .setTerminalVisibility(let command):
      workspaceView.setTerminalVisibility(sessionId: command.sessionId, visible: command.visible)
    case .pickWorkspaceFolder:
      presentWorkspaceFolderPicker()
    case .pickWorkspaceIcon(let command):
      presentWorkspaceIconPicker(command)
    case .showMessage(let command):
      showMessage(command)
    case .appendAgentDetectionDebugLog(let command):
      AppDelegate.appendAgentDetectionDebugLog(event: command.event, details: command.details)
    case .appendTerminalFocusDebugLog(let command):
      AppDelegate.appendTerminalFocusDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendRestoreDebugLog(let command):
      AppDelegate.appendRestoreDebugLog(event: command.event, details: command.details)
    case .appendSessionTitleDebugLog(let command):
      AppDelegate.appendSessionTitleDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendSidebarRefreshDebugLog(let command):
      AppDelegate.appendSidebarRefreshDebugLog(event: command.event, details: command.details)
    case .appendWorkspaceDockIndicatorDebugLog(let command):
      AppDelegate.appendWorkspaceDockIndicatorDebugLog(
        event: command.event, details: command.details)
    case .persistSharedSidebarStorage(let command):
      AppDelegate.persistSharedSidebarStorage(command)
    case .projectBoardResponse(let command):
      workspaceView.dispatchProjectBoardBridgeResponse(command)
    case .playSound(let command):
      /**
       CDXC:NativeSound 2026-04-29-16:30
       Sidebar-driven completion sounds are intentionally routed through
       AppDelegate so the native app owns playback and settings previews even
       when the sidebar webview has never unlocked browser audio.
       */
      NativeSoundPlayer.shared.play(command)
    case .runProcess(let command):
      runProcess(command)
    case .syncGhosttyTerminalSettings(let command):
      syncGhosttyTerminalSettings(command)
    case .applyGhosttyConfigSettings(let command):
      applyGhosttyConfigSettings(command)
    case .openGhosttyConfigFile:
      openGhosttyConfigFile()
    case .openAccessibilityPreferences:
      /**
       CDXC:AccessibilityPermissions 2026-05-08-13:08
       The Settings modal owns the one-click path into macOS Accessibility
       settings, so the view-level router forwards the button command to the
       native app instead of showing another permission dialog.
       */
      openAccessibilityPreferences()
    case .requestMacOSNotificationPermission:
      sessionAttentionNotificationController.requestPermissionFromSettings()
    case .openMacOSNotificationSettings:
      SessionAttentionNotificationController.openMacOSNotificationSettings()
    case .openExternalUrl(let command):
      openExternalUrl(command)
    case .openWorkspaceInFinder(let command):
      openWorkspaceInFinder(command)
    case .openWorkspaceInIde(let command):
      openWorkspaceInIde(command)
    case .openBrowserWindow(let command):
      /**
       CDXC:BrowserOverlay 2026-04-26-05:14
       Browser action buttons are routed out of the sidebar webview and
       into AppDelegate so the native host can launch and position Chrome
       Canary above the active ghostex attachment window.
       */
      openBrowserWindow(command)
    case .showBrowserWindow:
      /**
       CDXC:BrowserOverlay 2026-04-26-07:37
       The restored Browsers sidebar section uses this command to raise
       the already-running Canary window through AppDelegate, preserving
       native workarea placement without creating a new browser tab.
       */
      showBrowserWindow()
    case .openBrowserDevTools(let command):
      workspaceView.openBrowserDevTools(sessionId: command.sessionId)
    case .injectBrowserReactGrab(let command):
      workspaceView.injectBrowserReactGrab(sessionId: command.sessionId)
    case .injectBrowserAgentation(let command):
      workspaceView.injectBrowserAgentation(sessionId: command.sessionId)
    case .showBrowserProfilePicker(let command):
      workspaceView.showBrowserProfilePicker(sessionId: command.sessionId)
    case .showBrowserImportSettings(let command):
      workspaceView.showBrowserImportSettings(sessionId: command.sessionId)
    case .setSidebarSide(let command):
      setSidebarSide(command.side)
    case .setReactTitlebarHitRegions(let command):
      /**
       CDXC:ReactTitlebar 2026-05-11-20:24
       React owns titlebar button geometry, but Swift owns window dragging and
       workspace pass-through. Apply reported DOM hit regions at the native
       overlay boundary and relayout so the native titlebar webview only covers
       the titlebar strip plus any open dropdown bounds.
       */
      setReactTitlebarHitRegions(command.regions, overlayOpen: command.overlayOpen)
    case .openActiveProjectEditorFromTitlebar:
      openActiveProjectEditorFromTitlebar()
    case .exitFocusModeFromTitlebar:
      exitFocusModeFromTitlebar()
    case .openAgentsModeFromTitlebar:
      openAgentsModeFromTitlebar()
    case .openGitHubProjectFromTitlebar:
      openGitHubProjectFromTitlebar()
    case .showProjectEditorCompanionFromTitlebar:
      showProjectEditorCompanionFromTitlebar()
    case .openTasksPlaceholderFromTitlebar:
      openTasksPlaceholderFromTitlebar()
    case .refreshWorkspaceOpenTargetAvailabilityFromTitlebar:
      refreshWorkspaceOpenTargetAvailabilityFromTitlebar()
    case .rotateActivePaneLayoutClockwiseFromTitlebar:
      rotateActivePaneLayoutClockwiseFromTitlebar()
    case .togglePetOverlayFromTitlebar:
      togglePetOverlayFromTitlebar()
    case .toggleCommandsPanelFromTitlebar:
      toggleCommandsPanelFromTitlebar()
    case .sleepInactiveSessionsFromTitlebar(let command):
      sleepInactiveSessionsFromTitlebar(command)
    case .quitResourcesFromTitlebar(let command):
      quitResourcesFromTitlebar(command)
    case .runSidebarCommandFromTitlebar(let command):
      runSidebarCommandFromTitlebar(command)
    case .runSidebarGitActionFromTitlebar(let command):
      runSidebarGitActionFromTitlebar(command)
    case .configureZedOverlay(let command):
      /**
       CDXC:ZedOverlay 2026-04-26-03:29
       Zed overlay configuration comes from the sidebar webview, but the
       native overlay controller lives in AppDelegate beside the window it
       moves. Forward this command instead of consuming it in the sidebar
       router so the native button can be positioned over Zed Preview.
       */
      configureZedOverlay(command)
    case .openZedWorkspace(let command):
      /**
       CDXC:ZedOverlay 2026-04-28-05:29
       Sidebar workspace-open commands must use the same native overlay
       path as bridge commands so the selected Zed-family target receives
       the workspace request instead of leaving HostCommand non-exhaustive.
       */
      openZedWorkspace(command)
    case .sidebarCliCommand:
      /**
       CDXC:DebugCli 2026-04-27-07:18
       Sidebar CLI commands are handled by AppDelegate before this
       view-level router. Keep this case explicit so adding the command to
       HostCommand does not make the sidebar command switch non-exhaustive.
       */
      break
    case .sidebarContextMenuOpened:
      noteSidebarContextMenuOpened()
    case .sidebarContextMenuClosed:
      noteSidebarContextMenuClosed()
    }
  }

  private enum SidebarWorkspaceFocusKind {
    case terminal
    case webPane

    var debugName: String {
      switch self {
      case .terminal:
        return "terminal"
      case .webPane:
        return "webPane"
      }
    }
  }

  private func focusWorkspaceSessionAfterSidebarActivation(
    sessionId: String,
    kind: SidebarWorkspaceFocusKind
  ) {
    /**
     CDXC:SidebarSessionFocus 2026-05-15-17:20:
     Sidebar session-card clicks run inside WebKit's click dispatch, and WebKit
     can keep the sidebar as first responder after the native focus command
     returns. Defer only sidebar-originated workspace focus to the next main-loop
     turn so the companion terminal or web pane becomes first responder after the
     sidebar activation has settled.
     CDXC:SidebarSessionFocus 2026-05-15-17:25:
     Keep explicit before/after breadcrumbs around the deferred dispatch so a
     reproduction shows whether focus is lost before the command leaves the
     sidebar bridge, inside TerminalWorkspaceView, or after AppKit accepts the
     new first responder.
     */
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.sidebarFocusCommandQueued",
      details: [
        "kind": kind.debugName,
        "responderBeforeQueue": responderSnapshot(),
        "sessionId": sessionId,
        "webChromeFirstResponder": isWebChromeFirstResponder(),
        "workspaceSnapshotBeforeQueue": workspaceView.activationDebugSnapshot(),
      ])
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        return
      }
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusCommandDispatching",
        details: [
          "kind": kind.debugName,
          "responderBeforeDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotBeforeDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
      switch kind {
      case .terminal:
        self.workspaceView.focusTerminal(sessionId: sessionId, reason: "sidebarFocusCommand")
      case .webPane:
        self.workspaceView.focusWebPane(sessionId: sessionId, reason: "sidebarFocusCommand")
      }
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusCommandDispatched",
        details: [
          "kind": kind.debugName,
          "responderAfterDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotAfterDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
    }
  }

  private func responderSnapshot() -> [String: Any] {
    guard let responder = window?.firstResponder else {
      return [
        "className": "nil",
        "isModalHostResponder": false,
        "isSidebarResponder": false,
        "isTitlebarResponder": false,
      ]
    }
    let responderView = responder as? NSView
    let isSidebarResponder =
      responderView.map { $0 === sidebarView || $0.isDescendant(of: sidebarView) } ?? false
    let isModalHostResponder =
      responderView.map { $0 === modalHostView || $0.isDescendant(of: modalHostView) } ?? false
    let isTitlebarResponder =
      responderView.map { $0 === titlebarChromeWebView || $0.isDescendant(of: titlebarChromeWebView) }
      ?? false
    return [
      "className": String(describing: type(of: responder)),
      "isModalHostResponder": isModalHostResponder,
      "isSidebarResponder": isSidebarResponder,
      "isTitlebarResponder": isTitlebarResponder,
    ]
  }

  /**
   CDXC:T3Code 2026-05-14-09:34:
   The native sidebar can show T3 cards whose provider process was killed when
   the main app closed or while the app was backgrounded. Use the sidebar's
   running-session state as the source of truth and relaunch the provider in
   the background whenever those cards remain awake but localhost is not live.
   */
  private func setT3CodeRuntimeSessionState(_ command: SetT3CodeRuntimeSessionState, reason: String) {
    NativeT3RuntimeLauncher.setRunningSessionHeartbeat(
      runningSessionIds: command.runningSessionIds,
      reason: reason)
    let runtimeCwd = command.runtimeCwd?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !command.runningSessionIds.isEmpty, let runtimeCwd, !runtimeCwd.isEmpty else {
      t3RuntimeVisibleSessionCwd = nil
      t3RuntimeLivenessTimer?.invalidate()
      t3RuntimeLivenessTimer = nil
      return
    }

    t3RuntimeVisibleSessionCwd = runtimeCwd
    ensureT3CodeRuntimeForRunningSessions(reason: reason)
    if t3RuntimeLivenessTimer == nil {
      let timer = Timer(timeInterval: 10.0, repeats: true) { [weak self] _ in
        self?.ensureT3CodeRuntimeForRunningSessions(reason: "livenessTimer")
      }
      t3RuntimeLivenessTimer = timer
      RunLoop.main.add(timer, forMode: .common)
    }
  }

  private func ensureT3CodeRuntimeForRunningSessions(reason: String) {
    guard let runtimeCwd = t3RuntimeVisibleSessionCwd else {
      return
    }
    guard !NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
      return
    }
    NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.runningSessions.autoStart", [
      "cwd": runtimeCwd,
      "reason": reason,
    ])
    startT3CodeRuntime(StartT3CodeRuntime(cwd: runtimeCwd))
  }

  /**
   CDXC:T3Code 2026-04-30-02:38
   Native sidebar T3 Code panes must start the provider in desktop/no-browser
   mode and then render localhost inside the workarea WKWebView. This preserves
   the reference pane model instead of launching an external browser window.
   */
  private func startT3CodeRuntime(_ command: StartT3CodeRuntime) {
    /**
     CDXC:T3Code 2026-05-10-22:07
     Sidebar runtime starts are not proof that a T3 card is shown and awake. Do
     not refresh the managed provider keepalive here; otherwise a hidden
     background t3code server can burn CPU indefinitely while the visible
     sidebar contains only normal terminals.
     */
    if let process = t3CodeRuntimeProcess, process.isRunning {
      /**
       CDXC:T3Code 2026-05-02-00:48
       Native sidebar T3 cards can restore while a previously retained Bun
       server is wedged but still running. Verify auth/session responsiveness
       before reusing the process so the pane does not stay on a white unloaded
       WKWebView.
       */
      guard NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
        if NativeT3RuntimeLauncher.shouldRetainUnresponsiveManagedRuntime(
          pid: Int(process.processIdentifier))
        {
          /**
           CDXC:T3Code 2026-05-08-13:11
           Sidebar-driven T3 starts can race with a newly spawned provider
           finishing startup. Retain only young unresponsive processes; older
           listeners that still time out are stale runtime owners and should be
           replaced instead of blocking the active pane.
           */
          NativeT3CodePaneReproLog.append(
            "nativeSidebar.t3Runtime.start.runningUnhealthyRetained",
            [
              "cwd": command.cwd,
              "pid": process.processIdentifier,
            ])
          workspaceView.reloadManagedT3WebPanes(reason: "runtimeRetainedDuringStartup")
          return
        }
        NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        t3CodeRuntimeProcess = nil
        NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeSidebar")
        return startT3CodeRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
      ])
      return
    }

    /**
     CDXC:T3Code 2026-04-30-09:35
     Native sidebar restores can focus a T3 card while the previous managed
     provider still owns port 3774. Reuse that provider rather than killing it
     after a pane has already created a valid thread route.
     */
    if NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() {
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.adoptedExisting", [
        "cwd": command.cwd,
        "port": NativeT3RuntimeLauncher.port,
      ])
      return
    }

    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeSidebar")
    if NativeT3RuntimeLauncher.hasManagedRuntimeListener() {
      NativeT3CodePaneReproLog.append(
        "nativeSidebar.t3Runtime.start.retainedExistingUnresponsive",
        [
          "cwd": command.cwd,
          "port": NativeT3RuntimeLauncher.port,
        ])
      workspaceView.reloadManagedT3WebPanes(reason: "runtimeRetainedDuringStartup")
      return
    }
    NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.spawn", [
      "cwd": command.cwd,
      "mode": "desktop-bootstrap",
    ])
    do {
      let launch = try NativeT3RuntimeLauncher.createLaunch(cwd: command.cwd)
      let process = launch.process
      try process.run()
      t3CodeRuntimeProcess = process
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
      ])
      workspaceView.reloadManagedT3WebPanes(reason: "runtimeSpawned")
      process.terminationHandler = { [outputCapture = launch.outputCapture] terminatedProcess in
        var details = outputCapture.finish()
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.exit", details)
      }
    } catch {
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      ghostexRootView.logger.error("Failed to start T3 Code runtime: \(error.localizedDescription)")
    }
  }

  /**
   CDXC:T3Code 2026-04-30-09:23
   Native-sidebar Running modal controls must kill the embedded T3 provider
   they display. This command stops tracked process state and any managed T3
   listener on the shared localhost port.
   */
  private func stopT3CodeRuntime(logPrefix: String) {
    if let process = t3CodeRuntimeProcess {
      NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.stop.tracked", [
        "isRunning": process.isRunning,
        "pid": process.processIdentifier,
      ])
      if process.isRunning {
        process.terminate()
      }
      t3CodeRuntimeProcess = nil
    }
    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(
      logPrefix: "\(logPrefix).stop",
      forceOwnedRuntimeStop: true)
  }

  /**
   CDXC:EditorPanes 2026-05-06-14:21
   Sidebar editor buttons open a project-owned VS Code surface while sharing a
   single code-server process. Reuse only a responsive localhost runtime so the
   no-address-bar Chromium embed always points at live editor UI.
   */
  private func startCodeServerRuntime(_ command: StartCodeServerRuntime) {
    if let process = codeServerRuntimeProcess, process.isRunning {
      guard NativeCodeServerRuntimeLauncher.hasResponsiveRuntimeListener() else {
        if let startedAt = codeServerRuntimeStartedAt,
          Date().timeIntervalSince(startedAt)
            < NativeCodeServerRuntimeLauncher.startupGraceInterval
        {
          NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.booting", [
            "cwd": command.cwd,
            "pid": process.processIdentifier,
            "startedAt": startedAt.timeIntervalSince1970,
          ])
          return
        }
        NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        codeServerRuntimeProcess = nil
        codeServerRuntimeStartedAt = nil
        return startCodeServerRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
        "startedAt": codeServerRuntimeStartedAt?.timeIntervalSince1970 ?? NSNull(),
      ])
      return
    }

    if NativeCodeServerRuntimeLauncher.hasResponsiveRuntimeListener() {
      /**
       CDXC:EditorPanes 2026-05-06-15:00
       code-server settings-link options are process launch arguments. Do not
       adopt an untracked listener on the editor port because it may have been
       started without the selected VS Code config flags.
       */
      NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.portBusy", [
        "cwd": command.cwd,
        "origin": NativeCodeServerRuntimeLauncher.origin,
      ])
      _ = NativeCodeServerRuntimeLauncher.waitUntilNotResponsive(timeout: 2.0)
    }

    do {
      let launch = try NativeCodeServerRuntimeLauncher.createLaunch(
        cwd: command.cwd,
        linkVscodeUserConfig: command.linkVscodeUserConfig ?? true,
        vscodeUserConfigDir: command.vscodeUserConfigDir)
      let process = launch.process
      try process.run()
      codeServerRuntimeProcess = process
      let startedAt = Date()
      codeServerRuntimeStartedAt = startedAt
      NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
      ])
      process.terminationHandler = { [outputCapture = launch.outputCapture, startedAt] terminatedProcess in
        var details = outputCapture.finish()
        details["cwd"] = command.cwd
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        details["uptimeSeconds"] = Date().timeIntervalSince(startedAt)
        NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.exit", details)
      }
    } catch {
      NativeT3CodePaneReproLog.append("nativeSidebar.codeServerRuntime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      ghostexRootView.logger.error("Failed to start code-server runtime: \(error.localizedDescription)")
    }
  }

  private func stopCodeServerRuntime(logPrefix: String) {
    if let process = codeServerRuntimeProcess {
      NativeT3CodePaneReproLog.append("\(logPrefix).codeServerRuntime.stop.tracked", [
        "isRunning": process.isRunning,
        "pid": process.processIdentifier,
      ])
      if process.isRunning {
        process.terminate()
      }
      codeServerRuntimeProcess = nil
      codeServerRuntimeStartedAt = nil
    }
  }

  func stopCodeServerRuntimeForAppTermination() {
    stopCodeServerRuntime(logPrefix: "nativeSidebar.applicationWillTerminate")
  }

  private func activateAppWindow() {
    NSApp.activate(ignoringOtherApps: true)
    window?.makeKeyAndOrderFront(nil)
  }

  func handleHotkeyEquivalent(_ event: NSEvent) -> Bool {
    guard event.type == .keyDown else {
      return false
    }
    if shouldCloseAppModalHostOnEscape(event) {
      /**
       CDXC:AppModals 2026-05-22-16:55:
       Full-window app modals such as Previous Sessions must close on Escape
       even when keyboard focus remains in a terminal, titlebar, or another
       native responder instead of the modal-host WKWebView. Keep Ctrl+G prompt
       editor Escape handling in React because it has a two-step cancel flow.
       */
      closeAppModalHost(reason: "keyboardEscape")
      return true
    }
    let hotkeyText = Self.hotkeyText(for: event)
    if isNativeEditableFirstResponder() {
      /**
       CDXC:NativeTerminalSearch 2026-05-20-10:45:
       App-wide hotkey matching runs before AppKit dispatches key equivalents.
       When a native text editor owns focus, including the embedded Ghostty
       search field editor, editing shortcuts such as Cmd+A, copy, paste, and
       selection movement must stay with the focused control instead of being
       claimed as terminal/workspace hotkeys.
       */
      if Self.isHotkeyCandidate(event) {
        logNativeHotkeyDebug(
          "nativeHotkeys.appKitNativeEditableBypass",
          [
            "firstResponder": String(describing: type(of: window?.firstResponder)),
            "hotkeyText": hotkeyText ?? "<none>",
            "keyCode": String(event.keyCode),
          ])
      }
      return false
    }
    if Self.isCommandHorizontalArrowEvent(event) {
      /**
       CDXC:Hotkeys 2026-05-15-12:50:
       Command+Left and Command+Right can be intercepted before the sidebar DOM sees them.
       Persist AppKit-side breadcrumbs without changing routing so a reproduction shows whether the command-arrow shortcut was matched natively or passed through to WebKit/Ghostty.
      */
      logNativeHotkeyDebug(
        "nativeHotkeys.commandArrowAppKitKeyDown",
        [
          "characters": event.charactersIgnoringModifiers ?? "",
          "firstResponder": String(describing: type(of: window?.firstResponder)),
          "hotkeyText": hotkeyText ?? "<none>",
          "keyCode": String(event.keyCode),
          "webChromeFirstResponder": String(isWebChromeFirstResponder()),
        ])
    }
    if isWebChromeFirstResponder() {
      /**
       CDXC:Hotkeys 2026-05-10-12:06
       Settings and sidebar WebKit views need first chance at shortcut recording
       and editable controls. AppKit should only preempt key equivalents while
       Ghostty/native workspace surfaces own focus.
       */
      if Self.isCommandHorizontalArrowEvent(event) {
        logNativeHotkeyDebug(
          "nativeHotkeys.commandArrowAppKitWebChromeBypass",
          [
            "hotkeyText": hotkeyText ?? "<none>",
            "keyCode": String(event.keyCode),
          ])
      }
      return false
    }
    if Self.isHotkeyCandidate(event) {
      logNativeHotkeyDebug(
        "nativeHotkeys.appKitKeyEquivalent",
        [
          "characters": event.charactersIgnoringModifiers ?? "",
          "hotkeyText": hotkeyText ?? "<none>",
          "keyCode": String(event.keyCode),
        ])
    }
    guard let hotkeyText,
      let actionId = matchedHotkeyActionId(for: hotkeyText)
    else {
      if Self.isHotkeyCandidate(event) {
        logNativeHotkeyDebug(
          "nativeHotkeys.appKitNoAction",
          [
            "hotkeyText": hotkeyText ?? "<none>",
            "keyCode": String(event.keyCode),
          ])
      }
      return false
    }
    logNativeHotkeyDebug(
      "nativeHotkeys.appKitMatched",
      [
        "actionId": actionId,
        "hotkeyText": hotkeyText,
      ])
    dispatchNativeHotkey(actionId)
    return true
  }

  private func shouldCloseAppModalHostOnEscape(_ event: NSEvent) -> Bool {
    guard !modalHostView.isHidden,
      activeAppModalKind != nil,
      activeFloatingPromptEditor == nil,
      !isPrewarmingFloatingPromptEditor,
      event.charactersIgnoringModifiers == "\u{1b}"
    else {
      return false
    }
    return event.modifierFlags
      .intersection([.command, .control, .option, .shift])
      .isEmpty
  }

  private func closeAppModalHost(reason: String) {
    AppDelegate.appendAgentDetectionDebugLog(
      event: "nativeBridge.appModal.close.received",
      details: "reason=\(reason) wasHidden=\(modalHostView.isHidden)"
    )
    dispatchModalHostMessage(["type": "close"])
    activeAppModalKind = nil
    pendingModalHostOpenMessage = nil
    modalHostView.setTopLeftHitRegions(nil)
    modalHostView.isHidden = true
    updateSidebarModalBackdrop()
  }

  private func updateSidebarModalBackdrop() {
    /**
     CDXC:SidebarLayering 2026-05-23-12:20:
     Toasts keep the modal host visible without an active modal, and the
     floating prompt editor publishes its own hit regions instead of showing a
     backdrop. Only true backdrop modals should cover and block the sidebar.
     */
    let shouldShowBackdrop = activeAppModalKind != nil && activeAppModalKind != "floatingPromptEditor"
    sidebarModalBackdropView.isHidden = !shouldShowBackdrop
    updateWorkspaceInteractionShield()
  }

  private func updateWorkspaceInteractionShield() {
    /**
     CDXC:OverlayInteractivity 2026-05-25-07:02:
     Native pane tabs must not hover, show AppKit tooltips, or receive clicks
     behind Settings-style backdrop modals or React titlebar dropdown panels.
     Keep the shield off for toast-only modal-host visibility and for the
     floating prompt editor, which intentionally publishes scoped hit regions.

     CDXC:OverlayInteractivity 2026-05-25-10:09:
     Terminal panes must remain clickable after titlebar dropdowns close. React
     now publishes explicit dropdown/menu open state, so stale measured hit
     regions below the titlebar cannot keep the workspace shield active.
     */
    let backdropModalActive = isBackdropAppModalActive()
    let shouldShieldWorkspace =
      backdropModalActive || isTitlebarOverlayOpen
    workspaceInteractionShieldView.isHidden = !shouldShieldWorkspace
    workspaceView.setNativeChromeInteractivitySuppressed(shouldShieldWorkspace)
    logWorkspaceInteractionShieldStateIfNeeded(
      shouldShieldWorkspace: shouldShieldWorkspace,
      backdropModalActive: backdropModalActive
    )
  }

  private func logWorkspaceInteractionShieldStateIfNeeded(
    shouldShieldWorkspace: Bool,
    backdropModalActive: Bool
  ) {
    let modalKind = activeAppModalKind ?? "<none>"
    let logKey =
      "shield=\(shouldShieldWorkspace)|modal=\(modalKind)|backdrop=\(backdropModalActive)|titlebarOverlay=\(isTitlebarOverlayOpen)|regions=\(titlebarChromeView.hitRegionCount)|belowTitlebarRegions=\(titlebarChromeView.belowTitlebarHitRegionCount)"
    guard logKey != lastWorkspaceInteractionShieldLogKey else {
      return
    }
    lastWorkspaceInteractionShieldLogKey = logKey
    AppDelegate.appendNativeHostLifecycleLog(
      "workspaceInteractionShield.state shouldShield=\(shouldShieldWorkspace) backdropModalActive=\(backdropModalActive) activeAppModalKind=\(modalKind) titlebarOverlayOpen=\(isTitlebarOverlayOpen) titlebarHitRegionCount=\(titlebarChromeView.hitRegionCount) titlebarBelowTitlebarHitRegionCount=\(titlebarChromeView.belowTitlebarHitRegionCount) modalHostHidden=\(modalHostView.isHidden)"
    )
  }

  private func isBackdropAppModalActive() -> Bool {
    activeAppModalKind != nil && activeAppModalKind != "floatingPromptEditor"
  }

  private func isWebChromeFirstResponder() -> Bool {
    guard let responderView = window?.firstResponder as? NSView else {
      return false
    }
    return responderView === sidebarView
      || responderView.isDescendant(of: sidebarView)
      || responderView === modalHostView
      || responderView.isDescendant(of: modalHostView)
  }

  private func isNativeEditableFirstResponder() -> Bool {
    guard let responder = window?.firstResponder else {
      return false
    }
    return responder is NSTextView || responder is NSTextField
  }

  private func matchedHotkeyActionId(for hotkeyText: String) -> String? {
    /**
     CDXC:Hotkeys 2026-04-28-05:20
     Terminal surfaces receive key equivalents before the sidebar webview can
     observe DOM keyboard events, so AppKit matches only configured ghostex app
     hotkeys and dispatches their action id into the existing sidebar executor.
     */
    let hotkeys = nativeSettingsStore.readHotkeys()
    let now = Date()
    if let expiresAt = pendingHotkeyPrefixExpiresAt, expiresAt <= now {
      pendingHotkeyPrefix = nil
      pendingHotkeyPrefixExpiresAt = nil
    }
    let sequence =
      pendingHotkeyPrefix.map { "\($0) \(hotkeyText)" } ?? hotkeyText
    if let match = hotkeys.first(where: { $0.value == sequence }) {
      logNativeHotkeyDebug(
        "nativeHotkeys.appKitSequenceMatch",
        [
          "actionId": match.key,
          "configuredCount": String(hotkeys.count),
          "hotkeyText": hotkeyText,
          "sequence": sequence,
        ])
      pendingHotkeyPrefix = nil
      pendingHotkeyPrefixExpiresAt = nil
      return match.key
    }
    if let aliasMatch = matchedDefaultHotkeyAliasActionId(for: sequence, hotkeys: hotkeys) {
      logNativeHotkeyDebug(
        "nativeHotkeys.appKitAliasMatch",
        [
          "actionId": aliasMatch,
          "configuredCount": String(hotkeys.count),
          "hotkeyText": hotkeyText,
          "sequence": sequence,
        ])
      pendingHotkeyPrefix = nil
      pendingHotkeyPrefixExpiresAt = nil
      return aliasMatch
    }
    if hotkeys.values.contains(where: { $0.hasPrefix("\(hotkeyText) ") }) {
      logNativeHotkeyDebug(
        "nativeHotkeys.appKitPrefixStarted",
        [
          "configuredCount": String(hotkeys.count),
          "hotkeyText": hotkeyText,
        ])
      pendingHotkeyPrefix = hotkeyText
      pendingHotkeyPrefixExpiresAt = now.addingTimeInterval(1)
      return nil
    }
    logNativeHotkeyDebug(
      "nativeHotkeys.appKitNoMatch",
      [
        "configuredCount": String(hotkeys.count),
        "hotkeyText": hotkeyText,
        "pendingPrefix": pendingHotkeyPrefix ?? "",
        "sequence": sequence,
      ])
    pendingHotkeyPrefix = nil
    pendingHotkeyPrefixExpiresAt = nil
    return nil
  }

  private func matchedDefaultHotkeyAliasActionId(
    for hotkeyText: String,
    hotkeys: [String: String]
  ) -> String? {
    for (actionId, aliases) in NativeSettingsStore.defaultHotkeyAliases {
      guard hotkeys[actionId] != "" else {
        continue
      }
      if aliases.contains(hotkeyText) {
        return actionId
      }
    }
    return nil
  }

  private func dispatchNativeHotkey(_ actionId: String) {
    /**
     CDXC:Hotkeys 2026-04-28-06:15
     06:12 diagnostics showed AppKit matched shortcuts but the optional
     window.__ghostex_NATIVE_HOTKEYS__ call never reached the sidebar executor.
     Emit a typed host event through the same native event bus as terminal
     focus/title updates so hotkeys cannot disappear at an optional JS bridge.
     */
    logNativeHotkeyDebug("nativeHotkeys.dispatchHostEvent", ["actionId": actionId])
    sendHostEvent(.nativeHotkey(actionId: actionId))
  }

  private func logNativeHotkeyDebug(_ event: String, _ details: [String: String]) {
    /**
     CDXC:Hotkeys 2026-04-28-05:36
     AppKit owns shortcuts while Ghostty has first responder, so hotkey
     diagnostics must be written before dispatching into the sidebar webview.
     */
    AppDelegate.appendTerminalFocusDebugLog(
      event: event,
      details: AppDelegate.jsonObjectString(details))
  }

  private static func hotkeyText(for event: NSEvent) -> String? {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard let key = normalizedHotkeyKey(event) else {
      return nil
    }
    var parts: [String] = []
    if flags.contains(.command) {
      parts.append("cmd")
    }
    if flags.contains(.control) {
      parts.append("ctrl")
    }
    if flags.contains(.option) {
      parts.append("alt")
    }
    if flags.contains(.shift) {
      parts.append("shift")
    }
    parts.append(key)
    return parts.joined(separator: "+")
  }

  private static func isHotkeyCandidate(_ event: NSEvent) -> Bool {
    /**
     CDXC:Hotkeys 2026-05-14-07:10
     Native hotkey matching must preserve bare F12 for the command palette while
     also recognizing AppKit Tab keycode 48 for the newer Cmd+Tab and
     Cmd+Shift+Tab navigation defaults. Treat both as first-class key names in
     native normalization instead of routing through fallback behavior.
     */
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return !flags.isDisjoint(with: [.command, .control, .option, .shift]) || event.keyCode == 111
  }

  private static func isCommandHorizontalArrowEvent(_ event: NSEvent) -> Bool {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    return flags.contains(.command) &&
      flags.isDisjoint(with: [.control, .option]) &&
      (event.keyCode == 123 || event.keyCode == 124)
  }

  private static func normalizedHotkeyKey(_ event: NSEvent) -> String? {
    switch event.keyCode {
    case 126:
      return "up"
    case 124:
      return "right"
    case 125:
      return "down"
    case 123:
      return "left"
    case 111:
      return "f12"
    case 48:
      return "tab"
    default:
      break
    }
    let characters = event.charactersIgnoringModifiers
    guard let characters, !characters.isEmpty else {
      return nil
    }
    let normalizedCharacters = characters.lowercased()
    if event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.shift),
      let unshiftedDigit = shiftedDigitHotkeyKeys[normalizedCharacters]
    {
      /**
       CDXC:ActionsHotkeys 2026-05-26-14:32:
       AppKit reports Ctrl+Shift+1 with keyCode 18 but charactersIgnoringModifiers
       can still be the shifted glyph. Normalize shifted digit glyphs to the
       same physical digit hotkey stored in Settings so action-slot shortcuts
       are consumed before Ghostty receives them.
       */
      return unshiftedDigit
    }
    return normalizedCharacters
  }

  private static let shiftedDigitHotkeyKeys: [String: String] = [
    "!": "1",
    "@": "2",
    "#": "3",
    "$": "4",
    "%": "5",
    "^": "6",
    "&": "7",
    "*": "8",
    "(": "9",
    ")": "0",
  ]

  private func showMessage(_ command: ShowMessage) {
    let alert = NSAlert()
    switch command.level {
    case .info:
      alert.alertStyle = .informational
    case .warning:
      alert.alertStyle = .warning
    case .error:
      alert.alertStyle = .critical
    }
    alert.messageText = "Ghostex"
    alert.informativeText = command.message
    alert.addButton(withTitle: "OK")
    if let window {
      alert.beginSheetModal(for: window)
    } else {
      alert.runModal()
    }
  }

  func setSidebarSide(_ side: SidebarSide) {
    sidebarSide = side
    workspaceView.setSidebarSide(side)
    needsLayout = true
  }

  func workspaceScreenFrame() -> NSRect? {
    guard let window = workspaceView.window else {
      return nil
    }
    /**
     CDXC:BrowserOverlay 2026-04-26-05:22
     Chrome Canary should cover only the ghostex workarea, leaving the
     workspace switcher rail and sidebar visible for project/session
     context while the browser is open above the attached app.
     */
    let windowFrame = workspaceView.convert(workspaceView.bounds, to: nil)
    return window.convertToScreen(windowFrame)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let frames = rootLayoutFrames()
    validateRootLayoutFrames(frames)
    sidebarView.frame = frames.sidebar
    divider.frame = frames.divider
    workspaceView.frame = frames.workspace
    workspaceInteractionShieldView.frame = frames.workspace
    modalHostView.frame = frames.modalHost
    sidebarModalBackdropView.frame = frames.sidebar.union(frames.divider)
    titlebarChromeView.frame = frames.titlebarChrome
    promoteSidebarChrome()
    startupOverlayView.frame = bounds
    let startupOverlayIconSize = min(
      Self.startupOverlayIconSize,
      max(min(bounds.width, bounds.height) * 0.28, 64)
    )
    startupOverlayIconView.frame = CGRect(
      x: (bounds.width - startupOverlayIconSize) / 2,
      y: (bounds.height - startupOverlayIconSize) / 2,
      width: startupOverlayIconSize,
      height: startupOverlayIconSize
    )
    titlebarChromeView.titlebarHeight = Self.reactTitlebarHeight
  }

  private func promoteSidebarChrome() {
    /**
     CDXC:SidebarLayering 2026-05-23-01:51:
     Toasts and app-modal portals render through a full-window transparent
     WKWebView, but no app overlay should cover the sidebar. Keep the sidebar
     and its resize divider visually above modal/titlebar web layers so session
     cards remain clickable while bottom-center toasts are visible.
     */
    addSubview(sidebarView, positioned: .above, relativeTo: titlebarChromeView)
    addSubview(divider, positioned: .above, relativeTo: sidebarView)
    addSubview(sidebarModalBackdropView, positioned: .above, relativeTo: divider)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      return nil
    }
    /**
     CDXC:RootHitBoundaries 2026-05-12-10:55
     Full-frame transparent WKWebViews are allowed for titlebar portals and
     app modals, but pane chrome must still receive native AppKit clicks in the
     workspace frame. Route root hit testing by the resolved layout bands so a
     narrow right-side pane tab cannot be intercepted by sidebar/titlebar web
     surfaces that visually pass through.
     */
    if startupOverlayView.superview === self,
      startupOverlayView.alphaValue > 0,
      startupOverlayView.frame.contains(point)
    {
      return startupOverlayView
    }
    if let hitView = sidebarModalBackdropView.hitTest(convert(point, to: sidebarModalBackdropView))
    {
      /**
       CDXC:SidebarLayering 2026-05-23-12:20:
       The sidebar stays above normal webview layers so toasts cannot steal
       session-card clicks. Real backdrop modals are the exception: mirror the
       work-area scrim over sidebar chrome and let that click dismiss the same
       modal.
       */
      return hitView
    }
    if divider.frame.contains(point),
      let hitView = divider.hitTest(convert(point, to: divider))
    {
      return hitView
    }
    if sidebarView.frame.contains(point),
      let hitView = sidebarView.hitTest(convert(point, to: sidebarView))
    {
      /**
       CDXC:SidebarLayering 2026-05-23-01:51:
       Sidebar input must not pass through modal-host or titlebar WKWebViews.
       Bottom-center toast overlays can remain visible over the work area, but
       project/session navigation in the sidebar always owns its frame first.
      */
      return hitView
    }
    if !modalHostView.isHidden {
      let modalPoint = convert(point, to: modalHostView)
      if let hitView = modalHostView.hitTest(modalPoint) {
        /**
         CDXC:AppModals 2026-05-23-13:05:
         Real app modals must beat all workspace chrome hit targets. Otherwise
         narrow Settings/Agents Hub layouts can visually cover native pane tabs
         while AppKit still lets those tabs receive clicks behind the modal.
         Toast-only hosts still pass through here because their hit regions are
         explicitly empty.
         */
        return hitView
      }
      if isBackdropAppModalActive(), modalHostView.bounds.contains(modalPoint) {
        /**
         CDXC:AppModals 2026-05-25-12:10:
         Settings and other backdrop modals are never pass-through surfaces. If
         WebKit returns nil for transparent dialog/backdrop pixels, keep the
         click inside the modal host instead of letting terminal panes behind it
         receive the event.
         */
        return modalHostView
      }
    }
    if let hitView = titlebarChromeView.hitTest(convert(point, to: titlebarChromeView)) {
      /**
       CDXC:ReactTitlebar 2026-05-24-14:35:
       Open titlebar dropdowns are rendered in the full-window React titlebar
       WKWebView and can visually cover native pane tabs. Give reported
       titlebar/dropdown hit regions priority over workspace chrome so clicks
       land on visible dropdown items instead of the pane tab bar behind them.
      */
      return hitView
    }
    if let hitView = workspaceInteractionShieldView.hitTest(convert(point, to: workspaceInteractionShieldView))
    {
      /**
       CDXC:OverlayInteractivity 2026-05-25-07:02:
       When a modal or dropdown is open, transparent workspace pixels are still
       part of that overlay interaction model. Swallow them here so AppKit title
       tabs behind the overlay cannot react to hover, click, or tooltip lookup.
       */
      return hitView
    }
    if workspaceView.frame.contains(point),
      let nativeChromeHitView = workspaceView.nativeChromeHitView(at: convert(point, to: workspaceView))
    {
      /**
       CDXC:RootHitBoundaries 2026-05-22-22:48:
       Visible native workspace chrome should beat transparent webview space,
       but only after active modal and titlebar/dropdown hit regions had the
       first chance above.
       */
      return nativeChromeHitView
    }
    if workspaceView.frame.contains(point),
      let hitView = workspaceView.hitTest(convert(point, to: workspaceView))
    {
      return hitView
    }
    return super.hitTest(point)
  }

  private func rootLayoutFrames() -> RootLayoutFrames {
    let maxSidebarWidth = currentMaxSidebarWidth()
    let minSidebarWidth = currentSidebarMinWidth()
    let sidebarWidth = min(max(self.sidebarWidth, minSidebarWidth), maxSidebarWidth)
    self.sidebarWidth = sidebarWidth
    let workspaceBarWidth = currentWorkspaceBarWidth()
    let contentHeight = max(bounds.height - Self.reactTitlebarHeight, 1)
    let chromeWidth = workspaceBarWidth + sidebarWidth + Self.dividerWidth
    let chromeX: CGFloat = sidebarSide == .left ? 0 : max(bounds.width - chromeWidth, 0)
    let workspaceX: CGFloat = sidebarSide == .left ? chromeWidth : 0
    let workspaceWidth = max(bounds.width - chromeWidth, 1)
    let sidebarResizeEdgeExtension = min(
      max(workspaceView.sidebarResizeEdgeExtensionWidth, 0),
      workspaceWidth)
    /**
     CDXC:EditorPanes 2026-05-08-13:02
     Resizing the sidebar while a VS Code editor pane is visible can crash the
     native host. Log the root layout inputs and child frames before assignment
     so crash repros show whether the editor pane died during sidebar chrome
     layout, workspace layout, or embedded Chromium refresh.
     */
    NativeT3CodePaneReproLog.append("nativeSidebar.chrome.layout", [
      "bounds": Self.describeFrame(bounds),
      "chromeWidth": Double(chromeWidth),
      "contentHeight": Double(contentHeight),
      "maxSidebarWidth": Double(maxSidebarWidth),
      "minSidebarWidth": Double(minSidebarWidth),
      "sidebarSide": sidebarSide.rawValue,
      "sidebarWidth": Double(sidebarWidth),
      "workspaceFrameBefore": Self.describeFrame(workspaceView.frame),
      "workspaceWidth": Double(workspaceWidth),
      "workspaceX": Double(workspaceX),
    ])
    /**
     CDXC:SidebarPlacement 2026-05-06-18:26
     The resize handle must sit between the workspace and sidebar. Left-side
     sidebars keep the handle on their right edge; right-side sidebars put the
     same handle on their left edge so dragging grows/shrinks the visible
     sidebar boundary instead of the outside window edge.

     CDXC:SidebarResizeRails 2026-05-15-03:59:
     The sidebar/workspace boundary must have one native resize owner. Extend the
     existing root divider over the adjacent workspace edge gap so split-pane
     layouts do not show a second sidebar rail with separate drag handling.
     */
    let sidebarX: CGFloat
    let dividerX: CGFloat
    let dividerWidth = Self.dividerWidth + sidebarResizeEdgeExtension
    if sidebarSide == .left {
      sidebarX = chromeX
      dividerX = chromeX + workspaceBarWidth + sidebarWidth
    } else {
      sidebarX = chromeX + Self.dividerWidth
      dividerX = chromeX - sidebarResizeEdgeExtension
    }

    let sidebarFrame = CGRect(
      x: sidebarX,
      y: 0,
      width: workspaceBarWidth + sidebarWidth,
      height: contentHeight
    )
    let dividerFrame = CGRect(
      x: dividerX,
      y: 0,
      width: dividerWidth,
      height: contentHeight
    )
    let workspaceFrame = CGRect(
      x: workspaceX,
      y: 0,
      width: workspaceWidth,
      height: contentHeight
    )
    /**
     CDXC:RootHitBoundaries 2026-05-12-09:58
     The titlebar WKWebView keeps a full-window visual frame so portaled
     tooltips and dropdowns are not clipped. Its NSView hitTest remains the
     click boundary: only reported React controls/menus and the fixed titlebar
     drag strip consume events; all other workspace pixels pass through.
     */
    /**
     CDXC:SidebarLayering 2026-05-23-12:20:
     App modals should occupy the work area, not the sidebar band. A separate
     native sidebar backdrop mirrors modal blocking over sidebar chrome only
     while true backdrop modals are active, which keeps toast-only modal-host
     visibility from ever covering sidebar clicks.
     */
    let modalHostFrame = workspaceFrame
    let titlebarChromeFrame = bounds
    return RootLayoutFrames(
      divider: dividerFrame,
      modalHost: modalHostFrame,
      sidebar: sidebarFrame,
      titlebarChrome: titlebarChromeFrame,
      workspace: workspaceFrame)
  }

  private func validateRootLayoutFrames(_ frames: RootLayoutFrames) {
    guard bounds.width > 0, bounds.height > 0 else {
      return
    }
    let workspaceSidebarOverlap = frames.workspace.intersection(frames.sidebar)
    let titlebarStrip = CGRect(
      x: 0,
      y: max(bounds.height - Self.reactTitlebarHeight, 0),
      width: bounds.width,
      height: min(Self.reactTitlebarHeight, max(bounds.height, 0)))
    let workspaceTitlebarOverlap = frames.workspace.intersection(titlebarStrip)
    guard workspaceSidebarOverlap.isNull || workspaceSidebarOverlap.isEmpty,
      workspaceTitlebarOverlap.isNull || workspaceTitlebarOverlap.isEmpty
    else {
      /**
       CDXC:RootHitBoundaries 2026-05-11-20:24
       Unexpected base-region overlap means transparent chrome may steal clicks.
       Log the frames instead of widening hit-test fallbacks so layout bugs stay
       visible during click/drag reliability work.
       */
      NativeT3CodePaneReproLog.append("nativeRoot.layout.unexpectedOverlap", [
        "bounds": Self.describeFrame(bounds),
        "divider": Self.describeFrame(frames.divider),
        "modalHost": Self.describeFrame(frames.modalHost),
        "sidebar": Self.describeFrame(frames.sidebar),
        "titlebarChrome": Self.describeFrame(frames.titlebarChrome),
        "workspace": Self.describeFrame(frames.workspace),
        "workspaceSidebarOverlap": Self.describeFrame(workspaceSidebarOverlap),
        "workspaceTitlebarOverlap": Self.describeFrame(workspaceTitlebarOverlap),
      ])
      return
    }
  }

  private func resizeSidebar(by deltaX: CGFloat) {
    let maxSidebarWidth = currentMaxSidebarWidth()
    let effectiveDelta = sidebarSide == .left ? deltaX : -deltaX
    let previousSidebarWidth = sidebarWidth
    sidebarWidth = min(
      max(sidebarWidth + effectiveDelta, currentSidebarMinWidth()),
      maxSidebarWidth
    )
    /**
     CDXC:EditorPanes 2026-05-08-13:02
     Sidebar drag crashes with visible VS Code panes need the exact resize
     delta and clamped width recorded before AppKit schedules child layout.
     */
    NativeT3CodePaneReproLog.append("nativeSidebar.chrome.resize", [
      "bounds": Self.describeFrame(bounds),
      "deltaX": Double(deltaX),
      "effectiveDelta": Double(effectiveDelta),
      "maxSidebarWidth": Double(maxSidebarWidth),
      "previousSidebarWidth": Double(previousSidebarWidth),
      "sidebarSide": sidebarSide.rawValue,
      "sidebarWidth": Double(sidebarWidth),
    ])
    needsLayout = true
    /**
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     Sidebar width drags resize the workspace and therefore the surfaced terminal panes, but they are owned by root chrome rather than pane resize rails.
     Schedule the same trailing surfaced-only zmx refresh from the workspace owner.
     */
    workspaceView.scheduleZmxPersistenceRefreshForSurfacedTerminalsAfterResize(reason: "sidebarWidthResize")
  }

  private func resetSidebarWidth() {
    sidebarWidth = min(
      max(Self.sidebarResetWidth, currentSidebarMinWidth()),
      currentMaxSidebarWidth()
    )
    needsLayout = true
    persistSidebarWidth()
    /**
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     Resetting the sidebar width is a one-shot workspace resize, so zmx terminals need the same surfaced-only trailing refresh as drag resize.
     */
    workspaceView.scheduleZmxPersistenceRefreshForSurfacedTerminalsAfterResize(reason: "sidebarWidthReset")
  }

  private func currentMaxSidebarWidth() -> CGFloat {
    let minSidebarWidth = currentSidebarMinWidth()
    return max(
      minSidebarWidth,
      min(Self.sidebarMaxWidth, bounds.width - currentWorkspaceBarWidth() - Self.dividerWidth - 240))
  }

  private func currentSidebarMinWidth() -> CGFloat {
    /**
     CDXC:SidebarLayout 2026-05-13-08:11
     Combined is the only supported sidebar layout, so the native resize floor
     permanently uses the rail-free width that used to belong to combined mode.
     */
    return Self.sidebarMinWidth - Self.combinedSidebarMinWidthReduction
  }

  private func currentWorkspaceBarWidth() -> CGFloat {
    0
  }

  func persistNativeChromeForAppLifecycle() {
    /**
     CDXC:NativeSidebarChrome 2026-05-16-06:55:
     The app sidebar width must survive normal app restarts even when shutdown happens outside the resize handle's mouse-up path. Persist the currently clamped native width during window-close and terminate lifecycle hooks as the same setting used by drag resize.
     */
    persistSidebarWidth()
  }

  private func persistSidebarWidth() {
    nativeSettingsStore.persistSidebarWidth(sidebarWidth)
  }

  private static func describeFrame(_ frame: CGRect) -> [String: Double] {
    [
      "height": Double(frame.height),
      "maxX": Double(frame.maxX),
      "maxY": Double(frame.maxY),
      "minX": Double(frame.minX),
      "minY": Double(frame.minY),
      "width": Double(frame.width),
    ]
  }

  private func handleAppModalHostMessage(_ body: Any) {
    guard let message = body as? [String: Any],
      let type = message["type"] as? String
    else {
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeBridge",
        message: "Malformed modal host message: \(String(describing: body))",
        stack: nil
      )
      return
    }

    switch type {
    case "debugLog":
      let event = message["event"] as? String ?? "nativeBridge.appModal.debug"
      let details = message["details"] as? String
      AppDelegate.appendAgentDetectionDebugLog(event: "nativeBridge.appModal.\(event)", details: details)
    case "promptEditorDebugLog":
      let event = message["event"] as? String ?? "modalHost.promptEditor.unknown"
      if let details = message["details"] as? String, !details.isEmpty {
        PromptEditorDebugLog.append(event: "modalHost.\(event)", details: details)
      } else {
        PromptEditorDebugLog.append(event: "modalHost.\(event)")
      }
    case "logError":
      let area = message["area"] as? String ?? "AppModals:unknown"
      let errorMessage = message["message"] as? String ?? String(describing: message)
      let stack = message["stack"] as? String
      AppDelegate.appendAppModalErrorLog(area: area, message: errorMessage, stack: stack)
    case "floatingPromptEditorHitRegion":
      updateFloatingPromptEditorHitRegion(message: message)
    case "floatingPromptEditorSave":
      saveFloatingPromptEditor(message: message)
    case "floatingPromptEditorPasteImage":
      pasteImageIntoFloatingPromptEditor(message: message)
    case "floatingPromptEditorLoadImagePreview":
      loadFloatingPromptEditorImagePreview(message: message)
    case "floatingPromptEditorCancel":
      cancelFloatingPromptEditor(message: message)
    case "floatingPromptEditorPrewarmReady":
      guard isPrewarmingFloatingPromptEditor,
        let requestId = message["requestId"] as? String,
        requestId == Self.floatingPromptEditorPrewarmRequestId
      else {
        PromptEditorDebugLog.append(
          event: "native.prewarm.readyIgnored",
          details: [
            "isPrewarming": isPrewarmingFloatingPromptEditor,
            "requestId": message["requestId"] as? String ?? "",
          ]
        )
        return
      }
      PromptEditorDebugLog.append(event: "native.prewarm.ready", details: ["requestId": requestId])
      finishFloatingPromptEditorPrewarm()
    case "ready":
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.ready",
        details: "hasLatestState=\(latestModalHostSidebarState != nil) hasPendingOpen=\(pendingModalHostOpenMessage != nil)"
      )
      isModalHostReady = true
      if let latestModalHostSidebarState {
        dispatchModalHostMessage(latestModalHostSidebarState)
      }
      if let pendingModalHostOpenMessage {
        dispatchModalHostMessage(pendingModalHostOpenMessage)
        self.pendingModalHostOpenMessage = nil
      }
      prewarmFloatingPromptEditorIfNeeded()
    case "open":
      /**
       CDXC:AppModals 2026-04-28-12:06
       Persistent helper mode was removed, so full-window modal presentation no
       longer pauses or resurfaces external terminal windows. The modal host
       only needs to show its overlay above the embedded terminal view.
       */
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.open.received",
        details:
          "modal=\(message["modal"] as? String ?? "unknown") ready=\(isModalHostReady) hasLatestState=\(latestModalHostSidebarState != nil) wasHidden=\(modalHostView.isHidden)"
      )
      if !isModalHostReady {
        pendingModalHostOpenMessage = message
        return
      }
      pendingModalHostOpenMessage = nil
      if (message["modal"] as? String) != "floatingPromptEditor" {
        modalHostView.setTopLeftHitRegions(nil)
      }
      if let latestModalHostSidebarState {
        dispatchModalHostMessage(latestModalHostSidebarState)
      }
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.open.dispatch",
        details: "modal=\(message["modal"] as? String ?? "unknown")"
      )
      dispatchModalHostMessage(message)
    case "presented":
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.presented",
        details: "modal=\(message["modal"] as? String ?? "unknown") wasHidden=\(modalHostView.isHidden)"
      )
      if (message["modal"] as? String) == "floatingPromptEditor" {
        PromptEditorDebugLog.append(
          event: "native.presented",
          details: [
            "isPrewarming": isPrewarmingFloatingPromptEditor,
            "modalHostHiddenBefore": modalHostView.isHidden,
            "requestId": activeFloatingPromptEditor?.requestId ?? "",
          ]
        )
      }
      if isPrewarmingFloatingPromptEditor {
        return
      }
      activeAppModalKind = message["modal"] as? String
      if activeAppModalKind != "floatingPromptEditor" {
        modalHostView.setTopLeftHitRegions(nil)
      }
      modalHostView.isHidden = false
      updateSidebarModalBackdrop()
      if (message["modal"] as? String) == "floatingPromptEditor" {
        PromptEditorDebugLog.append(
          event: "native.presented.modalHostShown",
          details: [
            "modalHostHiddenAfter": modalHostView.isHidden,
            "requestId": activeFloatingPromptEditor?.requestId ?? "",
          ]
        )
      }
    case "close":
      closeAppModalHost(reason: "bridgeMessage")
    case "toast":
      /**
       CDXC:Worktrees 2026-05-18-23:07:
       Worktree and git progress messages are transient app-modal toasts. Keep the transparent modal host visible only while a toast or real modal is active so terminal panes are not covered by an idle overlay.

       CDXC:AppModals 2026-05-23-01:51:
       Toast-only modal-host visibility must be visual, not interactive. When
       no real app modal is open, constrain the modal host to an empty hit
       region so bottom-center delayed-send/status toasts cannot steal clicks
       from terminal panes or other workspace surfaces.
       */
      if !isModalHostReady {
        return
      }
      dispatchModalHostMessage(message)
      if activeAppModalKind == nil {
        modalHostView.setTopLeftHitRegions([])
      }
      modalHostView.isHidden = false
      updateSidebarModalBackdrop()
    case "toastDismissed":
      if (message["keepOpen"] as? Bool) != true {
        modalHostView.setTopLeftHitRegions(nil)
        modalHostView.isHidden = true
      }
      updateSidebarModalBackdrop()
    case "pickWorktreeImages":
      presentWorktreeImagePicker()
    case "sidebarState":
      latestModalHostSidebarState = message
      dispatchModalHostMessage(message)
    case "sidebarCommand":
      guard let sidebarMessage = message["message"] else {
        AppDelegate.appendAppModalErrorLog(
          area: "AppModals:sidebarCommand",
          message: "Sidebar command envelope was missing message payload: \(message)",
          stack: nil
        )
        return
      }
      /**
       CDXC:PreviousSessions 2026-05-07-16:02
       Previous-session search leaves the modal WKWebView as a sidebarCommand
       envelope. Log receipt before dispatching to the sidebar WKWebView so
       click repros can separate modal-host delivery from sidebar handling.
       */
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.sidebarCommand.received",
        details: String(describing: sidebarMessage)
      )
      dispatchSidebarModalCommand(sidebarMessage)
    default:
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeBridge",
        message: "Unknown modal host message type: \(type)",
        stack: nil
      )
    }
  }

  private func dispatchModalHostMessage(_ message: [String: Any]) {
    guard JSONSerialization.isValidJSONObject(message),
      let data = try? JSONSerialization.data(withJSONObject: message),
      let json = String(data: data, encoding: .utf8)
    else {
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeBridge",
        message: "Failed to serialize modal host message: \(message)",
        stack: nil
      )
      return
    }
    modalHostView.evaluateJavaScript(
      """
      window.dispatchEvent(new CustomEvent('ghostex-app-modal-host-message', { detail: \(json) }));
      /**
       CDXC:AppModals 2026-04-29-22:03
       WKWebView reports a successful dispatch as an error when the evaluated
       script returns the CustomEvent object. Return undefined so only actual
       modal bridge failures reach the app-modal error log.
       */
      undefined;
      """
    ) { _, error in
      if let error {
        AppDelegate.appendAppModalErrorLog(
          area: "AppModals:nativeBridge",
          message: "Failed to dispatch modal host message: \(error.localizedDescription)",
          stack: nil
        )
      }
    }
  }

  private func dispatchSidebarModalCommand(_ message: Any) {
    guard JSONSerialization.isValidJSONObject(message),
      let data = try? JSONSerialization.data(withJSONObject: message),
      let json = String(data: data, encoding: .utf8)
    else {
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:sidebarCommand",
        message: "Failed to serialize sidebar modal command: \(message)",
        stack: nil
      )
      return
    }
    AppDelegate.appendAgentDetectionDebugLog(
      event: "nativeBridge.appModal.sidebarCommand.dispatch",
      details: json
    )
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_MODAL_BRIDGE__?.handleSidebarMessage(\(json));
      /**
       CDXC:AppModals 2026-04-29-22:03
       Sidebar modal commands are fire-and-forget at the WebKit boundary; state
       changes carry the result, so the evaluated script should return nothing.
       */
      undefined;
      """
    ) { _, error in
      if let error {
        AppDelegate.appendAppModalErrorLog(
          area: "AppModals:sidebarCommand",
          message: "Failed to dispatch sidebar modal command: \(error.localizedDescription)",
          stack: nil
        )
      }
    }
  }

  private func presentWorkspaceFolderPicker() {
    /**
     CDXC:NativeWorkspacePicker 2026-04-26-00:47
     The workspace rail plus button must use the native folder picker. The
     selected project is sent back into the sidebar webview, which owns the
     per-project session/sidebar state.
     */
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.allowsMultipleSelection = false
    panel.canCreateDirectories = true
    panel.prompt = "Add Project"
    panel.message = "Choose a project folder to add to Ghostex."

    let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
      guard response == .OK,
        let url = panel.url
      else {
        return
      }
      self?.addWorkspaceProject(path: url.path, name: url.lastPathComponent)
    }

    if let window {
      panel.beginSheetModal(for: window, completionHandler: completion)
    } else {
      completion(panel.runModal())
    }
  }

  private func presentWorktreeImagePicker() {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = false
    panel.canChooseFiles = true
    panel.allowsMultipleSelection = true
    panel.allowedContentTypes = [.image]
    panel.prompt = "Add Images"
    panel.message = "Choose images to attach to the worktree prompt."

    let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
      guard response == .OK else {
        return
      }
      self?.dispatchModalHostMessage([
        "paths": panel.urls.map(\.path),
        "type": "worktreeImageFilesPicked",
      ])
    }

    if let window {
      panel.beginSheetModal(for: window, completionHandler: completion)
    } else {
      completion(panel.runModal())
    }
  }

  private func addWorkspaceProject(path: String, name: String) {
    let payload = ["path": path, "name": name]
    guard let data = try? JSONSerialization.data(withJSONObject: payload),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      (() => {
        const project = \(json);
        window.__ghostex_NATIVE_WORKSPACE_BAR__?.addProject(project.path, project.name);
      })();
      """)
  }

  private func presentWorkspaceIconPicker(_ command: PickWorkspaceIcon) {
    /**
     CDXC:WorkspaceDock 2026-04-27-08:53
     Workspace icon selection must use the native macOS picker because the
     React context menu lives inside WKWebView, where hidden file inputs can
     fail to open from synthetic/custom menu activation. Return a PNG/SVG
     data URL to the React workspace API so persistence stays with the
     workspace record.
     */
    let panel = NSOpenPanel()
    panel.canChooseDirectories = false
    panel.canChooseFiles = true
    panel.allowsMultipleSelection = false
    panel.allowedContentTypes = [.png, UTType(filenameExtension: "svg") ?? .image]
    panel.prompt = "Pick Icon"
    panel.message = "Choose a PNG or SVG icon for this workspace."

    let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
      guard response == .OK,
        let url = panel.url
      else {
        return
      }
      do {
        let data = try Data(contentsOf: url)
        let mimeType = url.pathExtension.lowercased() == "svg" ? "image/svg+xml" : "image/png"
        self?.setWorkspaceIcon(
          projectId: command.projectId,
          iconDataUrl: "data:\(mimeType);base64,\(data.base64EncodedString())"
        )
      } catch {
        self?.showMessage(
          ShowMessage(
            level: .error, message: "Could not read workspace icon: \(error.localizedDescription)"))
      }
    }

    if let window {
      panel.beginSheetModal(for: window, completionHandler: completion)
    } else {
      completion(panel.runModal())
    }
  }

  private func setWorkspaceIcon(projectId: String, iconDataUrl: String) {
    let payload = ["projectId": projectId, "iconDataUrl": iconDataUrl]
    guard let data = try? JSONSerialization.data(withJSONObject: payload),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      (() => {
        const icon = \(json);
        window.__ghostex_NATIVE_WORKSPACE_BAR__?.setProjectIcon(icon.projectId, icon.iconDataUrl);
      })();
      """)
  }

  private func openExternalUrl(_ command: OpenExternalUrl) {
    guard let url = URL(string: command.url) else {
      return
    }
    NSWorkspace.shared.open(url)
  }

  /**
   CDXC:NativeCommandBridge 2026-04-26-03:16
   Sidebar actions that need shell access, such as Git commit/push/PR, must
   run in the background without opening macOS Terminal. Process output is
   returned to the sidebar webview through HostEvent.processResult.
   */
  private func runProcess(_ command: RunProcess) {
    Task.detached { [weak self] in
      let process = Process()
      process.executableURL = URL(fileURLWithPath: command.executable)
      process.arguments = command.args
      if let cwd = command.cwd {
        process.currentDirectoryURL = URL(fileURLWithPath: cwd, isDirectory: true)
      }
      process.environment = normalizedNativeProcessEnvironment(overrides: command.env)
      let stdoutPipe = Pipe()
      let stderrPipe = Pipe()
      process.standardInput = FileHandle.nullDevice
      process.standardOutput = stdoutPipe
      process.standardError = stderrPipe
      let outputLock = NSLock()
      var stdoutData = Data()
      var stderrData = Data()
      let stdoutHandle = stdoutPipe.fileHandleForReading
      let stderrHandle = stderrPipe.fileHandleForReading
      /**
       CDXC:AgentsHub 2026-05-14-08:43
       Agents Hub catalog discovery can return megabytes of real profile, skill,
       hook, and config metadata. Drain process output while the command is
       running so large stdout/stderr payloads cannot fill the pipe and block
       the scanner before native posts processResult back to the webview.
       */
      stdoutHandle.readabilityHandler = { handle in
        let data = handle.availableData
        if data.isEmpty {
          return
        }
        outputLock.lock()
        stdoutData.append(data)
        outputLock.unlock()
      }
      stderrHandle.readabilityHandler = { handle in
        let data = handle.availableData
        if data.isEmpty {
          return
        }
        outputLock.lock()
        stderrData.append(data)
        outputLock.unlock()
      }

      let result: HostEvent
      do {
        try process.run()
        process.waitUntilExit()
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil
        let remainingStdoutData = stdoutHandle.readDataToEndOfFile()
        let remainingStderrData = stderrHandle.readDataToEndOfFile()
        outputLock.lock()
        stdoutData.append(remainingStdoutData)
        stderrData.append(remainingStderrData)
        let stdout = String(data: stdoutData, encoding: .utf8) ?? ""
        let stderr = String(data: stderrData, encoding: .utf8) ?? ""
        outputLock.unlock()
        result = .processResult(
          requestId: command.requestId,
          exitCode: process.terminationStatus,
          stdout: stdout,
          stderr: stderr
        )
      } catch {
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil
        result = .processResult(
          requestId: command.requestId,
          exitCode: 127,
          stdout: "",
          stderr: error.localizedDescription
        )
      }
      await MainActor.run { [weak self] in
        guard let self else {
          return
        }
        self.postHostEvent(result)
      }
    }
  }

  private func loadSidebar() {
    if let urlString = ProcessInfo.processInfo.environment["ghostex_SIDEBAR_URL"],
      let url = URL(string: urlString)
    {
      if NativeDebugLogging.isEnabled {
        Self.logger.info("Loading sidebar URL \(url.absoluteString, privacy: .public)")
      }
      sidebarView.load(URLRequest(url: url))
      return
    }

    let webAssets = Self.resolveWebAssets()
    let builtSidebar = webAssets.appendingPathComponent("index.html")
    if FileManager.default.fileExists(atPath: builtSidebar.path) {
      if NativeDebugLogging.isEnabled {
        Self.logger.info("Loading built sidebar from \(builtSidebar.path, privacy: .public)")
      }
      sidebarView.loadFileURL(builtSidebar, allowingReadAccessTo: webAssets)
      return
    }

    Self.logger.error("Built sidebar not found at \(builtSidebar.path, privacy: .public)")
    let repoRoot = Self.resolveRepoRoot()
    let html = """
      <!doctype html>
      <html>
        <body style="margin:0;background:#111827;color:#d1d5db;font:13px -apple-system,BlinkMacSystemFont,sans-serif;height:100vh">
          <div style="padding:18px;line-height:1.45">
            <h1 style="font-size:14px;margin:0 0 14px">ghostex Native Ghostty</h1>
            <button id="shell" style="width:100%;margin:0 0 8px;padding:9px">New shell</button>
            <button id="codex" style="width:100%;margin:0 0 8px;padding:9px">Codex agent</button>
            <button id="close" style="width:100%;padding:9px">Close active</button>
            <p style="color:#9ca3af;margin-top:16px">
              Set ghostex_SIDEBAR_URL to load the full sidebar bundle.
            </p>
          </div>
          <script>
            let activeSessionId = "";
            function send(command) {
              window.webkit.messageHandlers.ghostexNativeHost.postMessage(command);
            }
            function create(title, input) {
              activeSessionId = crypto.randomUUID();
              send({
                type: "createTerminal",
                sessionId: activeSessionId,
                cwd: "\(NSHomeDirectory())",
                title,
                initialInput: input || ""
              });
            }
            shell.onclick = () => create("Shell", "");
            codex.onclick = () => create("Codex", "codex\\r");
            close.onclick = () => activeSessionId && send({ type: "closeTerminal", sessionId: activeSessionId });
          </script>
        </body>
      </html>
      """
    sidebarView.loadHTMLString(html, baseURL: repoRoot)
  }

  private func loadModalHost() {
    let webAssets = Self.resolveWebAssets()
    let builtModalHost = webAssets.appendingPathComponent("modal-host.html")
    if FileManager.default.fileExists(atPath: builtModalHost.path) {
      if NativeDebugLogging.isEnabled {
        Self.logger.info("Loading modal host from \(builtModalHost.path, privacy: .public)")
      }
      modalHostView.loadFileURL(
        builtModalHost,
        allowingReadAccessTo: webAssets
      )
      return
    }

    Self.logger.error("Built modal host not found at \(builtModalHost.path, privacy: .public)")
    let repoRoot = Self.resolveRepoRoot()
    modalHostView.loadHTMLString(
      "<!doctype html><html><body style=\"margin:0;background:transparent\"></body></html>",
      baseURL: repoRoot
    )
  }

  private func loadTitlebarChrome() {
    let webAssets = Self.resolveWebAssets()
    let builtTitlebarChrome = webAssets.appendingPathComponent("titlebar-host.html")
    if FileManager.default.fileExists(atPath: builtTitlebarChrome.path) {
      if NativeDebugLogging.isEnabled {
        Self.logger.info("Loading React titlebar chrome from \(builtTitlebarChrome.path, privacy: .public)")
      }
      titlebarChromeWebView.loadFileURL(
        builtTitlebarChrome,
        allowingReadAccessTo: webAssets
      )
      return
    }

    /**
     CDXC:ReactTitlebar 2026-05-09-17:11
     Development builds may start before the titlebar bundle exists. Keep the
     missing-asset behavior observable and blank instead of silently falling
     back to native AppKit controls, because the requirement is React chrome.
     */
    Self.logger.error("Built React titlebar chrome not found at \(builtTitlebarChrome.path, privacy: .public)")
    let repoRoot = Self.resolveRepoRoot()
    titlebarChromeWebView.loadHTMLString(
      "<!doctype html><html><body style=\"margin:0;background:transparent\"></body></html>",
      baseURL: repoRoot
    )
  }

  static func resolveWebAssets() -> URL {
    // CDXC:NativeSidebar 2026-04-27-06:19: Sidebar assets should be loaded
    // from the app bundle first because users normally launch the installed
    // app from /Applications, where FileManager.currentDirectoryPath is not
    // the repository root.
    if let bundledWebAssets = Bundle.main.resourceURL?.appendingPathComponent(
      "Web", isDirectory: true),
      FileManager.default.fileExists(
        atPath: bundledWebAssets.appendingPathComponent("index.html").path)
    {
      return bundledWebAssets
    }

    return resolveRepoRoot().appendingPathComponent("native/macos/ghostexHost/Web", isDirectory: true)
  }

  private static func resolveRepoRoot() -> URL {
    if let repoRootPath = ProcessInfo.processInfo.environment["ghostex_REPO_ROOT"],
      !repoRootPath.isEmpty
    {
      return URL(fileURLWithPath: repoRootPath, isDirectory: true)
    }

    // CDXC:PublicRelease 2026-04-27-05:36: The native host must discover
    // local development assets without committing maintainer-specific
    // absolute paths into public source.
    let currentDirectory = FileManager.default.currentDirectoryPath
    return URL(fileURLWithPath: currentDirectory, isDirectory: true)
  }

  private static let diagnosticsScript = """
    (() => {
      const post = (payload) => {
        try {
          window.webkit?.messageHandlers?.ghostexNativeHostDiagnostics?.postMessage(payload);
        } catch {}
      };
      window.addEventListener("error", (event) => {
        post({
          type: "error",
          message: String(event.message || ""),
          source: String(event.filename || ""),
          line: event.lineno || 0,
          column: event.colno || 0,
          stack: event.error && event.error.stack ? String(event.error.stack) : ""
        });
      });
      window.addEventListener("unhandledrejection", (event) => {
        const reason = event.reason;
        post({
          type: "unhandledrejection",
          message: reason && reason.message ? String(reason.message) : String(reason || ""),
          stack: reason && reason.stack ? String(reason.stack) : ""
        });
      });
      const originalError = console.error.bind(console);
      console.error = (...args) => {
        post({ type: "console.error", message: args.map((arg) => String(arg)).join(" ") });
        originalError(...args);
      };
      post({ type: "diagnostics-ready", href: location.href });
    })();
    """

  private static let workspaceBarHTML = """
    <!doctype html>
    <html>
      <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <style>
          :root {
            color-scheme: dark;
            font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
          }
          * { box-sizing: border-box; }
          html, body {
            height: 100%;
            margin: 0;
            overflow: hidden;
            width: 100%;
          }
          body {
            align-items: center;
            background: #080d14;
            color: #d8e1f1;
            display: flex;
            flex-direction: column;
            gap: 8px;
            padding: 10px 7px;
          }
          #projects {
            align-items: center;
            display: flex;
            flex: 1;
            flex-direction: column;
            gap: 8px;
            min-height: 0;
            overflow: hidden auto;
            width: 100%;
          }
          button {
            appearance: none;
            align-items: center;
            background: #121a26;
            border: 1px solid #263346;
            border-radius: 12px;
            color: #d8e1f1;
            cursor: default;
            display: flex;
            font: 700 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
            height: 40px;
            justify-content: center;
            padding: 0;
            position: relative;
            width: 40px;
          }
          button[data-dragging="true"] {
            opacity: 0.28;
            transform: scale(0.96);
          }
          #drop-line {
            background: #8fb4ff;
            border-radius: 999px;
            box-shadow:
              0 0 0 1px rgba(143, 180, 255, 0.34),
              0 0 12px rgba(143, 180, 255, 0.42);
            height: 3px;
            left: 8px;
            opacity: 0;
            pointer-events: none;
            position: fixed;
            top: 0;
            transform: translateY(-50%);
            transition: opacity 90ms ease;
            width: 38px;
            z-index: 20;
          }
          #drop-line[data-visible="true"] {
            opacity: 1;
          }
          #drag-ghost {
            align-items: center;
            background: #121a26;
            border: 1px solid #263346;
            border-radius: 12px;
            color: #d8e1f1;
            display: none;
            font: 700 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
            height: 40px;
            justify-content: center;
            left: 0;
            opacity: 0.92;
            pointer-events: none;
            position: fixed;
            top: 0;
            transform: translate(-50%, -50%);
            width: 40px;
            z-index: 21;
          }
          #drag-ghost[data-visible="true"] {
            display: flex;
          }
          button:hover {
            background: #172235;
            border-color: #3b4e69;
          }
          button[data-active="true"] {
            background: #1e3762;
            border-color: #5b8df6;
            box-shadow: 0 0 0 2px rgba(91, 141, 246, 0.18);
          }
          .indicators {
            /* CDXC:WorkspaceDock 2026-04-27-06:58: Done and working badges sit
               together at the top-right of the workspace button, ordered green
               then orange from left to right. The orange badge uses "working"
               to match session-card activity and avoid overloading "active". */
            align-items: center;
            display: flex;
            gap: 1px;
            pointer-events: none;
            position: absolute;
            right: -1px;
            top: -7px;
            z-index: 2;
          }
          .indicator {
            align-items: center;
            border: 2px solid #080d14;
            border-radius: 999px;
            box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
            color: #ffffff;
            display: grid;
            font: 800 9px/1 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
            height: 18px;
            justify-content: center;
            min-width: 18px;
            padding: 0 4px;
            white-space: nowrap;
          }
          .indicator[data-status="working"] {
            background: #d08a2d;
          }
          .indicator[data-status="done"] {
            background: #2e9d68;
          }
          .indicator[data-status="running"] {
            /* CDXC:WorkspaceDock 2026-04-27-06:27: The gray total-running
               terminal count belongs at the bottom-left of each workspace
               button, distinct from top-right done/working session badges. */
            background: #6f7785;
            bottom: -7px;
            left: -1px;
            position: absolute;
          }
          #add {
            flex: 0 0 auto;
          }
        </style>
      </head>
      <body>
        <div id="projects"></div>
        <div id="drop-line"></div>
        <div id="drag-ghost"></div>
        <button id="add" title="New workspace">+</button>
        <script>
          const projectsElement = document.getElementById("projects");
          const addButton = document.getElementById("add");
          const dropLineElement = document.getElementById("drop-line");
          const dragGhostElement = document.getElementById("drag-ghost");
          let state = { projects: [], activeProjectId: "" };
          const pointerDrag = {
            button: null,
            didDrag: false,
            ghostText: "",
            placeAfterTarget: false,
            pointerId: undefined,
            projectId: "",
            startX: 0,
            startY: 0,
            targetProjectId: "",
          };
          const post = (message) => {
            window.webkit?.messageHandlers?.ghostexWorkspaceBar?.postMessage(message);
          };
          const initials = (title, index) => {
            const trimmed = String(title || "").trim();
            if (!trimmed) return String(index + 1);
            const words = trimmed.split(/\\s+/).filter(Boolean);
            if (words.length > 1) return words.slice(0, 2).map((word) => word[0]).join("").toUpperCase();
            return trimmed.slice(0, 2).toUpperCase();
          };
          const render = () => {
            projectsElement.replaceChildren();
            state.projects.forEach((project, index) => {
              const button = document.createElement("button");
              button.type = "button";
              button.dataset.projectId = project.projectId;
              button.dataset.active = project.isActive ? "true" : "false";
              const running = Number(project.sessionCounts?.running || 0);
              const done = Number(project.sessionCounts?.done || 0);
              const working = Number(project.sessionCounts?.working || 0);
              const summary = [
                running > 0 ? `${running} running` : "",
                working > 0 ? `${working} working` : "",
                done > 0 ? `${done} done` : "",
              ].filter(Boolean).join(", ");
              button.title = summary ? `${project.path || project.title} - ${summary}` : (project.path || project.title);
              button.textContent = initials(project.title, index);
              const focusProject = () => post({ type: "focusProject", projectId: project.projectId });
              /**
               * CDXC:WorkspaceDock 2026-04-27-08:30
               * Project selection used to run on pointerdown to avoid dropped
               * clicks during rail re-renders. Native HTML drag cannot start
               * after that preventDefault, so the rail now owns a tiny pointer
               * drag recognizer: release without movement selects; movement
               * reorders and persists workareas. Drag feedback is a faded
               * source button, a plain floating ghost, and an insertion line
               * only when release would change the order.
               */
              button.onpointerdown = (event) => {
                if (event.button !== 0) return;
                event.preventDefault();
                pointerDrag.button = button;
                pointerDrag.didDrag = false;
                pointerDrag.ghostText = button.textContent || "";
                pointerDrag.placeAfterTarget = false;
                pointerDrag.pointerId = event.pointerId;
                pointerDrag.projectId = project.projectId;
                pointerDrag.startX = event.clientX;
                pointerDrag.startY = event.clientY;
                pointerDrag.targetProjectId = "";
                button.setPointerCapture?.(event.pointerId);
              };
              button.onpointermove = (event) => {
                if (pointerDrag.pointerId !== event.pointerId || pointerDrag.projectId !== project.projectId) return;
                const deltaX = event.clientX - pointerDrag.startX;
                const deltaY = event.clientY - pointerDrag.startY;
                if (!pointerDrag.didDrag && Math.hypot(deltaX, deltaY) < 5) return;
                pointerDrag.didDrag = true;
                button.dataset.dragging = "true";
                const dropTarget = getDropTarget(event.clientY, pointerDrag.projectId);
                const target = dropTarget?.button;
                clearDragState(button);
                updateDragGhost(event.clientX, event.clientY);
                if (target && wouldReorder(pointerDrag.projectId, target.dataset.projectId, dropTarget.placeAfterTarget)) {
                  const bounds = target.getBoundingClientRect();
                  pointerDrag.targetProjectId = target.dataset.projectId;
                  pointerDrag.placeAfterTarget = dropTarget.placeAfterTarget;
                  updateDropLine(bounds, pointerDrag.placeAfterTarget);
                } else {
                  pointerDrag.targetProjectId = "";
                  hideDropLine();
                }
              };
              button.onpointerup = (event) => {
                if (pointerDrag.pointerId !== event.pointerId || pointerDrag.projectId !== project.projectId) return;
                event.preventDefault();
                button.releasePointerCapture?.(event.pointerId);
                const sourceProjectId = pointerDrag.projectId;
                const didDrag = pointerDrag.didDrag;
                const targetProjectId = pointerDrag.targetProjectId;
                const placeAfterTarget = pointerDrag.placeAfterTarget;
                resetPointerDrag();
                if (!didDrag) {
                  focusProject();
                  return;
                }
                if (targetProjectId) {
                  reorderProjects(sourceProjectId, targetProjectId, placeAfterTarget);
                }
              };
              button.onpointercancel = (event) => {
                if (pointerDrag.pointerId !== event.pointerId) return;
                resetPointerDrag();
              };
              button.onclick = (event) => {
                if (event.detail > 0) return;
                focusProject();
              };
              if (done > 0 || working > 0) {
                const indicators = document.createElement("span");
                indicators.className = "indicators";
                if (done > 0) {
                  const doneIndicator = document.createElement("span");
                  doneIndicator.className = "indicator";
                  doneIndicator.dataset.status = "done";
                  doneIndicator.textContent = formatCount(done);
                  indicators.appendChild(doneIndicator);
                }
                if (working > 0) {
                  const workingIndicator = document.createElement("span");
                  workingIndicator.className = "indicator";
                  workingIndicator.dataset.status = "working";
                  workingIndicator.textContent = formatCount(working);
                  indicators.appendChild(workingIndicator);
                }
                button.appendChild(indicators);
              }
              if (running > 0) {
                const runningIndicator = document.createElement("span");
                runningIndicator.className = "indicator";
                runningIndicator.dataset.status = "running";
                runningIndicator.textContent = formatCount(running);
                button.appendChild(runningIndicator);
              }
              projectsElement.appendChild(button);
            });
          };
          const clearDragState = (except) => {
            projectsElement.querySelectorAll("[data-dragging]").forEach((element) => {
              if (element !== except) delete element.dataset.dragging;
            });
          };
          const resetPointerDrag = () => {
            pointerDrag.button?.releasePointerCapture?.(pointerDrag.pointerId);
            pointerDrag.button = null;
            pointerDrag.didDrag = false;
            pointerDrag.ghostText = "";
            pointerDrag.placeAfterTarget = false;
            pointerDrag.pointerId = undefined;
            pointerDrag.projectId = "";
            pointerDrag.startX = 0;
            pointerDrag.startY = 0;
            pointerDrag.targetProjectId = "";
            hideDragGhost();
            hideDropLine();
            clearDragState();
          };
          const updateDragGhost = (clientX, clientY) => {
            dragGhostElement.textContent = pointerDrag.ghostText;
            dragGhostElement.style.left = `${clientX}px`;
            dragGhostElement.style.top = `${clientY}px`;
            dragGhostElement.dataset.visible = "true";
          };
          const hideDragGhost = () => {
            delete dragGhostElement.dataset.visible;
          };
          const updateDropLine = (targetBounds, placeAfterTarget) => {
            dropLineElement.style.left = `${targetBounds.left + 1}px`;
            dropLineElement.style.top = `${placeAfterTarget ? targetBounds.bottom + 4 : targetBounds.top - 4}px`;
            dropLineElement.style.width = `${Math.max(34, targetBounds.width - 2)}px`;
            dropLineElement.dataset.visible = "true";
          };
          const hideDropLine = () => {
            delete dropLineElement.dataset.visible;
          };
          const getDropTarget = (clientY, sourceProjectId) => {
            const buttons = Array.from(projectsElement.querySelectorAll("button[data-project-id]"))
              .filter((button) => button.dataset.projectId !== sourceProjectId);
            if (buttons.length === 0) return undefined;
            for (const button of buttons) {
              const bounds = button.getBoundingClientRect();
              if (clientY < bounds.top + bounds.height / 2) {
                return { button, placeAfterTarget: false };
              }
            }
            return { button: buttons[buttons.length - 1], placeAfterTarget: true };
          };
          const nextProjectOrder = (sourceProjectId, targetProjectId, placeAfterTarget) => {
            if (!sourceProjectId || !targetProjectId || sourceProjectId === targetProjectId) return;
            const ids = state.projects.map((project) => project.projectId);
            const fromIndex = ids.indexOf(sourceProjectId);
            const toIndex = ids.indexOf(targetProjectId);
            if (fromIndex < 0 || toIndex < 0) return;
            const [movedProjectId] = ids.splice(fromIndex, 1);
            const adjustedTargetIndex = ids.indexOf(targetProjectId);
            ids.splice(adjustedTargetIndex + (placeAfterTarget ? 1 : 0), 0, movedProjectId);
            return ids;
          };
          const wouldReorder = (sourceProjectId, targetProjectId, placeAfterTarget) => {
            const nextIds = nextProjectOrder(sourceProjectId, targetProjectId, placeAfterTarget);
            if (!nextIds) return false;
            return nextIds.some((projectId, index) => projectId !== state.projects[index]?.projectId);
          };
          const reorderProjects = (sourceProjectId, targetProjectId, placeAfterTarget) => {
            clearDragState();
            const ids = nextProjectOrder(sourceProjectId, targetProjectId, placeAfterTarget);
            if (!ids) return;
            if (!ids.some((projectId, index) => projectId !== state.projects[index]?.projectId)) return;
            post({ type: "reorderProjects", projectIds: ids });
          };
          const formatCount = (count) => count > 99 ? "99+" : String(count);
          window.addEventListener("ghostex-workspace-bar-state", (event) => {
            state = event.detail || state;
            render();
          });
          addButton.onclick = () => post({ type: "pickProject" });
          post({ type: "workspaceBarReady" });
        </script>
      </body>
    </html>
    """
}

final class ghostexFocusReportingWindow: NSWindow {
  var onFirstResponderChanged: ((NSResponder?) -> Void)?
  var onKeyDownDispatch: ((NSEvent) -> Void)?
  var onKeyEquivalent: ((NSEvent) -> Bool)?
  var onActivationBoundaryEvent: ((NSEvent, String) -> Void)?

  /**
   CDXC:NativeTerminalFocus 2026-04-26-21:32
   User clicks inside split Ghostty surfaces change AppKit's first responder
   without going through sidebar focus commands. Report every successful
   responder transition so native terminal focus becomes the source that
   updates sidebar/store focus before the next layout sync.
   */
  override func makeFirstResponder(_ responder: NSResponder?) -> Bool {
    let previousResponder = firstResponder
    let didBecomeFirstResponder = super.makeFirstResponder(responder)
    if didBecomeFirstResponder && firstResponder !== previousResponder {
      onFirstResponderChanged?(firstResponder)
    }
    return didBecomeFirstResponder
  }

  override func sendEvent(_ event: NSEvent) {
    /**
     CDXC:NativeTerminalFocus 2026-05-11-11:48
     Keyboard-route repros need the AppKit dispatch target before Ghostty
     handles the key. Report keyDown metadata from the window boundary so the
     log can compare first responder, visible focus ring, and terminal surface
     delivery without recording typed characters.

     CDXC:FocusStealDiagnostics 2026-05-15-20:09:
     Recent focus-steal repros showed Ghostex becoming active with no fresh internal activation request. Report low-volume mouse events at the NSWindow boundary before and after AppKit dispatch so the next activation can be correlated with a real click, a synthetic companion click, or no local input at all.
     */
    let shouldReportActivationBoundaryEvent = Self.shouldReportActivationBoundaryEvent(event)
    if shouldReportActivationBoundaryEvent {
      onActivationBoundaryEvent?(event, "windowSendEvent.beforeSuper")
    }
    if event.type == .keyDown {
      onKeyDownDispatch?(event)
      if onKeyEquivalent?(event) == true {
        return
      }
    }
    super.sendEvent(event)
    if shouldReportActivationBoundaryEvent && Self.isMouseActivationBoundaryEvent(event) {
      onActivationBoundaryEvent?(event, "windowSendEvent.afterSuper")
    }
  }

  override func performKeyEquivalent(with event: NSEvent) -> Bool {
    if onKeyEquivalent?(event) == true {
      return true
    }
    return super.performKeyEquivalent(with: event)
  }

  private static func shouldReportActivationBoundaryEvent(_ event: NSEvent) -> Bool {
    isMouseActivationBoundaryEvent(event)
  }

  private static func isMouseActivationBoundaryEvent(_ event: NSEvent) -> Bool {
    switch event.type {
    case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown,
      .otherMouseUp:
      return true
    default:
      return false
    }
  }
}

final class PaneResizeHandleView: NSView {
  var onDrag: ((CGFloat) -> Void)?
  var onDragEnded: (() -> Void)?
  var onDoubleClick: (() -> Void)?
  private var lastDragWindowX: CGFloat = 0

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    super.init(coder: coder)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  override func resetCursorRects() {
    addCursorRect(bounds, cursor: .resizeLeftRight)
  }

  /**
   CDXC:NativeSidebarChrome 2026-04-26-07:27
   The resize hit target stays wide enough to drag comfortably, but the
   visible sidebar edge is intentionally transparent so the reference sidebar
   does not show a light vertical drag-area separator.
   */
  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
  }

  override func mouseDown(with event: NSEvent) {
    if event.clickCount >= 2 {
      onDoubleClick?()
      return
    }
    lastDragWindowX = event.locationInWindow.x
  }

  override func mouseDragged(with event: NSEvent) {
    /**
     CDXC:NativeSidebarChrome 2026-05-04-08:19
     Sidebar resize drags must track the pointer in stable window coordinates.
     The handle's local coordinate space moves after each width update, so
     local deltas can invert during a continuous drag and make the sidebar jump
     between widths until the user releases the handle.
     */
    let currentWindowX = event.locationInWindow.x
    let deltaX = currentWindowX - lastDragWindowX
    lastDragWindowX = currentWindowX
    onDrag?(deltaX)
  }

  override func mouseUp(with event: NSEvent) {
    onDragEnded?()
  }
}

extension ghostexRootView: WKNavigationDelegate {
  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    Self.logger.info("Sidebar webview finished loading")
    webView.evaluateJavaScript(
      "JSON.stringify({ text: document.body.innerText.slice(0, 240), rootHTML: document.getElementById('root')?.innerHTML.slice(0, 240) || '', bootError: window.__ghostex_BOOT_ERROR__ || null })"
    ) { result, error in
      if let error {
        Self.logger.error(
          "Sidebar DOM probe failed: \(error.localizedDescription, privacy: .public)")
        return
      }
      Self.logger.info("Sidebar DOM probe: \(String(describing: result), privacy: .public)")
    }
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
    Self.logger.error(
      "Sidebar webview navigation failed: \(error.localizedDescription, privacy: .public)")
  }

  func webView(
    _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
    withError error: Error
  ) {
    Self.logger.error(
      "Sidebar webview provisional navigation failed: \(error.localizedDescription, privacy: .public)"
    )
  }

  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    /**
     CDXC:CrashDiagnostics 2026-04-27-17:38
     WebKit renderer exits can look like an app crash from the UI. Persist
     this delegate callback so native process exits are not confused with
     web content process termination.
     */
    Self.logger.error("Sidebar webview content process terminated")
    AppDelegate.appendNativeHostLifecycleLog(
      "sidebarWebContentProcessDidTerminate url=\(webView.url?.absoluteString ?? "<missing>")")
  }
}

final class ReactTitlebarChromeView: NSView {
  var titlebarHeight: CGFloat = 30
  private let webView: WKWebView
  private var hitRegions: [CGRect] = []
  private var overlayOpen = false
  private var frameBeforeTitlebarMaximize: NSRect?

  var hitRegionCount: Int {
    hitRegions.count
  }

  var belowTitlebarHitRegionCount: Int {
    hitRegions.filter { $0.maxY > titlebarHeight + 1 }.count
  }

  init(webView: WKWebView) {
    self.webView = webView
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
    webView.autoresizingMask = [.width, .height]
    webView.frame = bounds
    addSubview(webView)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    webView.frame = bounds
  }

  func setHitRegions(_ regions: [ReactTitlebarHitRegion], overlayOpen: Bool) {
    self.overlayOpen = overlayOpen
    hitRegions = regions.map {
      CGRect(
        x: CGFloat($0.x),
        y: CGFloat($0.y),
        width: CGFloat($0.width),
        height: CGFloat($0.height)
      )
    }
    /**
     CDXC:ReactTitlebar 2026-05-12-09:58
     DOM hit regions are measured from the top of the full titlebar document.
     The WKWebView may render full-window for unclipped portals, but native
     hit-testing allows events through only when a point lands inside one of
     these reported regions or inside the fixed blank titlebar drag strip.

     CDXC:ReactTitlebar 2026-05-25-10:27:
     Below-titlebar regions are valid only while React says a titlebar dropdown
     is open. Stale dropdown measurements must not keep routing workspace clicks
     into the titlebar WKWebView after the dropdown closes.
     */
  }

  func containsInteractiveHitRegion(_ point: NSPoint) -> Bool {
    guard bounds.contains(point) else {
      return false
    }
    let webPoint = CGPoint(x: point.x, y: bounds.height - point.y)
    return hitRegions.contains { region in
      region.contains(webPoint) && (overlayOpen || region.maxY <= titlebarHeight + 1)
    }
  }

  func closeOpenDropdowns() {
    webView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.closeOpenDropdowns?.();
      undefined;
      """)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      return nil
    }
    if containsInteractiveHitRegion(point) {
      return webView.hitTest(point)
    }
    if isPointInFixedTitlebarStrip(point) {
      return self
    }
    return nil
  }

  override func mouseDown(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    guard isPointInFixedTitlebarStrip(point) else {
      return
    }
    if event.clickCount >= 2 {
      toggleWindowMaximizedToVisibleScreen()
      return
    }
    window?.performDrag(with: event)
  }

  override var mouseDownCanMoveWindow: Bool {
    /**
     CDXC:ReactTitlebar 2026-05-12-07:02
     The React titlebar wrapper may extend below the fixed titlebar strip for
     dropdown hit regions, so it must not globally advertise itself as movable
     window background. Dragging and double-click maximize are allowed only by
     mouseDown after revalidating the event is inside the fixed top strip.
     */
    false
  }

  private func isPointInFixedTitlebarStrip(_ point: NSPoint) -> Bool {
    guard bounds.contains(point), titlebarHeight > 0 else {
      return false
    }
    let stripMinY = max(bounds.height - titlebarHeight, 0)
    return point.y >= stripMinY
  }

  private func toggleWindowMaximizedToVisibleScreen() {
    /**
     CDXC:ReactTitlebar 2026-05-10-17:23
     AppKit owns blank titlebar mouse gestures because React hit regions only
     cover real controls. Double-clicking that draggable chrome should behave
     like a windowed maximize: fill the current screen's visible frame without
     entering macOS full-screen spaces.
     */
    guard let window, let screen = window.screen ?? NSScreen.main else {
      return
    }
    let visibleFrame = screen.visibleFrame
    if Self.framesApproximatelyEqual(window.frame, visibleFrame),
      let restoreFrame = frameBeforeTitlebarMaximize
    {
      window.setFrame(restoreFrame, display: true, animate: true)
      frameBeforeTitlebarMaximize = nil
      return
    }
    frameBeforeTitlebarMaximize = window.frame
    window.setFrame(visibleFrame, display: true, animate: true)
  }

  private static func framesApproximatelyEqual(_ lhs: NSRect, _ rhs: NSRect) -> Bool {
    abs(lhs.minX - rhs.minX) < 1
      && abs(lhs.minY - rhs.minY) < 1
      && abs(lhs.width - rhs.width) < 1
      && abs(lhs.height - rhs.height) < 1
  }
}

final class SidebarScriptBridge: NSObject, WKScriptMessageHandler {
  private static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "webview")
  private let decoder = JSONDecoder()
  private let router: SidebarCommandRouter

  init(router: SidebarCommandRouter) {
    self.router = router
  }

  func userContentController(
    _ userContentController: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    if message.name == "ghostexNativeHostDiagnostics" {
      let diagnostic = String(describing: message.body)
      if diagnostic.contains("diagnostics-ready") {
        if NativeDebugLogging.isEnabled {
          Self.logger.info("Sidebar diagnostic: \(diagnostic, privacy: .public)")
        }
      } else {
        Self.logger.error("Sidebar diagnostic: \(diagnostic, privacy: .public)")
      }
      return
    }

    if message.name == "ghostexAppModalHost" {
      router.onAppModalHostMessage?(message.body)
      return
    }

    guard JSONSerialization.isValidJSONObject(message.body) else {
      let bodyDescription = String(String(describing: message.body).prefix(1000))
      /**
       CDXC:EditorPanes 2026-05-08-13:31
       Sidebar-to-native editor commands must fail observably. A malformed
       WebKit bridge payload can otherwise drop createProjectEditorPane before
       focusProjectEditorPane runs, leaving the VS Code button apparently dead.
       */
      NativeT3CodePaneReproLog.append("nativeSidebar.bridge.command.invalidJson", [
        "body": bodyDescription,
        "messageName": message.name,
      ])
      return
    }

    do {
      let data = try JSONSerialization.data(withJSONObject: message.body)
      let command = try decoder.decode(HostCommand.self, from: data)
      if (message.body as? [String: Any])?["type"] as? String
        == "rotateActivePaneLayoutClockwiseFromTitlebar"
      {
        if NativeDebugLogging.isEnabled {
          print("[ghostex-titlebar] native bridge received rotateActivePaneLayoutClockwiseFromTitlebar")
        }
      }
      router.onCommand?(command)
    } catch {
      let body = message.body as? [String: Any]
      let bodyDescription = String(String(describing: message.body).prefix(1000))
      let commandType = body?["type"] as? String ?? "<missing>"
      NativeT3CodePaneReproLog.append("nativeSidebar.bridge.command.decodeFailed", [
        "body": bodyDescription,
        "error": error.localizedDescription,
        "messageName": message.name,
        "type": commandType,
      ])
      Self.logger.error(
        "Sidebar command decode failed type=\(commandType, privacy: .public) error=\(error.localizedDescription, privacy: .public)"
      )
    }
  }
}

final class SidebarCommandRouter {
  var onAppModalHostMessage: ((Any) -> Void)?
  var onCommand: ((HostCommand) -> Void)?
}
