import AppKit
import ApplicationServices
import CoreImage
import Darwin
import GhosttyKit
import OSLog
import QuartzCore
import Security
import Sparkle
import UniformTypeIdentifiers
import UserNotifications
import WebKit

private final class NativeProcessRegistry {
  static let shared = NativeProcessRegistry()

  private let lock = NSLock()
  private var canceledRequestIds = Set<String>()
  private var processesByRequestId: [String: Process] = [:]

  func register(requestId: String, process: Process) -> Bool {
    /*
     CDXC:AddRepository 2026-06-01-10:33:
     Repository clone cancellation is a native process concern, not just a toast
     dismissal. Track runProcess children by request id so the sidebar can cancel
     the active Git clone and so an early cancel wins before Process.run starts.
     */
    lock.lock()
    defer { lock.unlock() }
    if canceledRequestIds.remove(requestId) != nil {
      return false
    }
    processesByRequestId[requestId] = process
    return true
  }

  func unregister(requestId: String) {
    lock.lock()
    processesByRequestId.removeValue(forKey: requestId)
    canceledRequestIds.remove(requestId)
    lock.unlock()
  }

  func cancel(requestId: String) {
    lock.lock()
    let process = processesByRequestId[requestId]
    if process?.isRunning != true {
      canceledRequestIds.insert(requestId)
    }
    lock.unlock()

    if process?.isRunning == true {
      process?.terminate()
    }
  }

  func isCanceled(requestId: String) -> Bool {
    lock.lock()
    defer { lock.unlock() }
    return canceledRequestIds.contains(requestId)
  }
}

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
 The traffic-light buttons should sit below exact vertical center in the 35px
 app titlebar by the configured visual offset. Keep this as a named visual-down
 offset so flipped and non-flipped AppKit titlebar coordinate systems apply the
 same requirement.
 */
private let ghostexTrafficLightVisualDownOffset: CGFloat = 2
/**
 CDXC:NativeWindowChrome 2026-05-28-14:59:
 The main app window must not resize or restore below 500px wide by 400px tall.
 Keep the AppKit resize minimum and persisted-frame clamps on the same value so saved older window sizes cannot reopen below the supported app minimum.
 */
private let ghostexMainWindowMinimumSize = NSSize(width: 500, height: 400)
private let ghostexOSIntegrationEditorExtensions = [
  "txt", "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "ini", "env", "xml", "csv",
  "html", "css", "scss", "js", "jsx", "ts", "tsx", "sh", "bash", "zsh", "fish", "py", "rb", "go",
  "rs", "swift", "java", "kt", "c", "h", "cpp", "hpp", "cs", "php", "lua", "sql",
]
private let ghostexOSIntegrationScriptExtensions = ["command", "tool", "sh"]
private let ghostexNativeShellPathSentinel = "__GHOSTEX_NATIVE_SHELL_PATH__"
private let ghostexNativeShellPathDiscoveryTimeout: DispatchTimeInterval = .seconds(2)
private let ghostexNativeShellPathCacheLock = NSLock()
private var ghostexNativeShellPathCache: [String]?
private let ghostexNativeColorDisablingEnvironmentKeys = [
  "ANSI_COLORS_DISABLED",
  "NO_COLOR",
  "NODE_DISABLE_COLORS",
]
/**
 CDXC:AutoUpdate 2026-06-08-18:21:
 Ghostex must check for available app updates at launch and then every 15 minutes while it remains running. Keep the cadence in native code because Sparkle owns appcast evaluation and the React titlebar should only render the resulting availability state.
 */
private let ghostexSparkleAvailabilityProbeInterval: TimeInterval = 15 * 60

private func normalizedNativeProcessEnvironment(overrides: [String: String]?) -> [String: String] {
  /**
   CDXC:NativeCommandBridge 2026-05-10-12:08
   macOS GUI launches do not reliably inherit the user's shell PATH. Native
   background commands must still find common developer tools installed through
   Homebrew, mise, asdf, or ~/.local/bin, because features such as session title
   generation run Codex through this process bridge instead of inside a terminal.

   CDXC:NativeCommandBridge 2026-06-07-00:38:
   Native helper subprocesses must not inherit NO_COLOR from the app process or command-specific env overlays. Keep helper environments color-capable by stripping color-disabling keys at the normalized process boundary.
   */
  var environment = ProcessInfo.processInfo.environment
  environment["PATH"] = normalizedNativeProcessPath(environment["PATH"], environment: environment)
  if let overrides {
    environment.merge(overrides) { _, newValue in newValue }
    environment["PATH"] = normalizedNativeProcessPath(environment["PATH"], environment: environment)
  }
  for key in ghostexNativeColorDisablingEnvironmentKeys {
    environment.removeValue(forKey: key)
  }
  return environment
}

private func normalizedNativeProcessPath(_ path: String?, environment: [String: String]) -> String {
  let homeDirectory = NSHomeDirectory()
  let defaultEntries = [
    "\(homeDirectory)/.opencode/bin",
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
  let shellEntries = nativeShellPathEntries(environment: environment)
  var seen = Set<String>()
  return (shellEntries + existingEntries + defaultEntries)
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

private func nativeShellPathEntries(environment: [String: String]) -> [String] {
  ghostexNativeShellPathCacheLock.lock()
  if let cached = ghostexNativeShellPathCache {
    ghostexNativeShellPathCacheLock.unlock()
    return cached
  }

  let discovered = discoverNativeShellPathEntries(environment: environment)
  ghostexNativeShellPathCache = discovered
  ghostexNativeShellPathCacheLock.unlock()
  return discovered
}

private func discoverNativeShellPathEntries(environment: [String: String]) -> [String] {
  /**
   CDXC:NativeCommandBridge 2026-06-03-20:28:
   Some macOS-local commands still launch through the native bridge after
   gxserver took shared agent/tool ownership. Probe the user's interactive login
   shell once so GUI-launched setup probes can find shell-mutated PATH entries
   such as NVM/npm, mise/asdf, Homebrew, and ~/.opencode/bin.
   */
  let configuredShell = environment["SHELL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
  let shellPath = configuredShell?.isEmpty == false ? configuredShell! : "/bin/zsh"
  let candidates = shellPath == "/bin/zsh" ? [shellPath] : [shellPath, "/bin/zsh"]

  for candidate in candidates {
    guard FileManager.default.isExecutableFile(atPath: candidate) else {
      continue
    }
    if let entries = runNativeShellPathDiscovery(shellPath: candidate, environment: environment),
       !entries.isEmpty
    {
      return entries
    }
  }

  return []
}

private func runNativeShellPathDiscovery(
  shellPath: String,
  environment: [String: String]
) -> [String]? {
  let process = Process()
  process.executableURL = URL(fileURLWithPath: shellPath)
  process.arguments = [
    "-ilc",
    "printf '\\n\(ghostexNativeShellPathSentinel)%s\\n' \"$PATH\"",
  ]
  process.environment = environment
  process.standardInput = FileHandle.nullDevice
  process.standardError = FileHandle.nullDevice

  let stdoutPipe = Pipe()
  process.standardOutput = stdoutPipe
  let outputLock = NSLock()
  var stdoutData = Data()
  let stdoutHandle = stdoutPipe.fileHandleForReading
  stdoutHandle.readabilityHandler = { handle in
    let data = handle.availableData
    if data.isEmpty {
      return
    }
    outputLock.lock()
    if stdoutData.count < 128 * 1024 {
      stdoutData.append(data)
    }
    outputLock.unlock()
  }

  let finished = DispatchSemaphore(value: 0)
  process.terminationHandler = { _ in
    finished.signal()
  }

  do {
    try process.run()
  } catch {
    stdoutHandle.readabilityHandler = nil
    return nil
  }

  if finished.wait(timeout: .now() + ghostexNativeShellPathDiscoveryTimeout) == .timedOut {
    process.terminate()
    if process.isRunning {
      kill(process.processIdentifier, SIGKILL)
    }
    stdoutHandle.readabilityHandler = nil
    return nil
  }

  stdoutHandle.readabilityHandler = nil
  let remainingData = stdoutHandle.readDataToEndOfFile()
  outputLock.lock()
  stdoutData.append(remainingData)
  let output = String(data: stdoutData, encoding: .utf8) ?? ""
  outputLock.unlock()

  return output
    .split(whereSeparator: \.isNewline)
    .compactMap { line -> [String]? in
      let value = String(line)
      guard value.hasPrefix(ghostexNativeShellPathSentinel) else {
        return nil
      }
      return value
        .dropFirst(ghostexNativeShellPathSentinel.count)
        .split(separator: ":")
        .map(String.init)
    }
    .last
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

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, SPUUpdaterDelegate,
  SPUStandardUserDriverDelegate
{
  static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "app")
  private static let standardWindowButtonTypes: [NSWindow.ButtonType] = [
    .closeButton, .miniaturizeButton, .zoomButton,
  ]
  private static let standardWindowButtonLeadingOffsets: [NSWindow.ButtonType: CGFloat] = [
    .closeButton: 0,
    .miniaturizeButton: 23,
    .zoomButton: 46,
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
  private var sessionStatusIndicatorController: SessionStatusIndicatorController?
  private var petOverlayController: PetOverlayController?
  private var lastVisibleMainWindowFrameForPersistence: NSRect?
  private var pendingGhosttyConfigReloadTimer: Timer?
  private var isFlushingCEFBeforeTerminate = false
  private var didFlushCEFBeforeTerminate = false
  private var workspaceActivationObserver: NSObjectProtocol?
  private var trafficLightLayoutObservers: [NSObjectProtocol] = []
  private weak var trafficLightLayoutObservedWindow: NSWindow?
  private weak var trafficLightLayoutObservedTitlebarView: NSView?
  private var isPositioningMainWindowTrafficLightButtons = false
  private var appHotkeyEventMonitor: Any?
  private var appShotsLocalEventMonitor: Any?
  private var appShotsGlobalEventMonitor: Any?
  private var appShotsPressedModifierKeyCodes = Set<UInt16>()
  private var lastAppShotsDoubleTap: (keyCode: UInt16, timestamp: TimeInterval)?
  private var lastAppShotsCaptureAt: Date?
  private var lastNativeActivationRequest: NativeActivationRequest?
  private var lastNativeInputEventPayload: [String: Any]?
  private var lastNativeInputEventRecordedAt: Date?
  private weak var appTitlebarLabel: NSTextField?
  private let nativeSettingsStore = NativeSettingsStore()
  private let lidSleepHelperClient = LidSleepPrivilegedHelperClient.shared
  private let gxserverClient = GxserverClient()
  private var isSparkleUpdateAvailable = false
  private var sparkleAvailabilityProbeTimer: Timer?
  private var didStartSparkleUpdater = false
  private lazy var sparkleUserDriver = GhostexSparkleUserDriver(
    hostBundle: Bundle.main,
    delegate: self)
  private lazy var sparkleUpdater = SPUUpdater(
    hostBundle: Bundle.main,
    applicationBundle: Bundle.main,
    userDriver: sparkleUserDriver,
    delegate: self)
  private var t3CodeRuntimeProcess: Process?
  private var t3CodeRuntimeStartedAt: Date?
  private var t3RuntimeVisibleSessionCwd: String?
  private var t3RuntimeLivenessTimer: Timer?
  private var t3RuntimeAutoStartBackoffUntil: Date?
  private var codeServerRuntimeProcess: Process?
  private var codeServerRuntimeStartedAt: Date?
  private var pendingOSIntegrationCommands: [(action: String, payloadJson: String)] = []
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
    super.init()
    /**
     CDXC:AutoUpdate 2026-05-28-14:19:
     Ghostex should still initialize Sparkle at launch, but scheduled update
     presentation is mediated by AppDelegate so new releases surface first as
     quiet titlebar chrome instead of an immediate modal prompt.

     CDXC:AutoUpdate 2026-06-08-19:16:
     Ghostex now provides a compact Sparkle user driver, so AppDelegate owns
     the SPUUpdater instance directly instead of using SPUStandardUpdaterController,
     which always creates Sparkle's full download and extraction status UI.
     */
    _ = sparkleUpdater
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

       CDXC:GxserverMigration 2026-05-30-19:30:
       First upgraded launch must let gxserver finish the legacy macOS shared-state import before WKWebView injects sidebar storage. Creating the sidebar earlier can hydrate old `project-*`/`g-*` IDs and later persist them over the canonical P/G rewrite, so window creation waits for the local gxserver bootstrap result.
       */
      startGxserverBootstrapThenCreateWindow()
      if startSparkleUpdater() {
        startSparkleUpdateAvailabilityProbes()
      }
      scheduleOSIntegrationFlushRetry()
    }
    tickTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
      self?.ghostty.appTick()
    }
  }

  @MainActor func application(_ application: NSApplication, open urls: [URL]) {
    var filePaths: [String] = []
    for url in urls {
      if url.isFileURL {
        filePaths.append(url.path)
        continue
      }
      handleOSIntegrationURL(url)
    }
    if !filePaths.isEmpty {
      dispatchOSIntegrationFileOpenPaths(filePaths)
    }
  }

  @MainActor func application(_ sender: NSApplication, openFiles filenames: [String]) {
    dispatchOSIntegrationFileOpenPaths(filenames)
    sender.reply(toOpenOrPrint: .success)
  }

  @MainActor func application(_ sender: NSApplication, openFile filename: String) -> Bool {
    dispatchOSIntegrationFileOpenPaths([filename])
    return true
  }

  func applicationWillTerminate(_ notification: Notification) {
    if let workspaceActivationObserver {
      NSWorkspace.shared.notificationCenter.removeObserver(workspaceActivationObserver)
    }
    if let appHotkeyEventMonitor {
      NSEvent.removeMonitor(appHotkeyEventMonitor)
      self.appHotkeyEventMonitor = nil
    }
    if let appShotsLocalEventMonitor {
      NSEvent.removeMonitor(appShotsLocalEventMonitor)
      self.appShotsLocalEventMonitor = nil
    }
    if let appShotsGlobalEventMonitor {
      NSEvent.removeMonitor(appShotsGlobalEventMonitor)
      self.appShotsGlobalEventMonitor = nil
    }
    sparkleAvailabilityProbeTimer?.invalidate()
    sparkleAvailabilityProbeTimer = nil
    persistMainWindowChrome()
    (window?.contentView as? ghostexRootView)?.persistNativeChromeForAppLifecycle()
    Self.appendNativeHostLifecycleLog(
      "applicationWillTerminate pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false)"
    )
    stopCodeServerRuntime(logPrefix: "nativeHost.applicationWillTerminate")
    (window?.contentView as? ghostexRootView)?.stopCodeServerRuntimeForAppTermination()
    /**
     CDXC:GxserverBootstrap 2026-05-30-15:39:
     Closing or quitting the macOS app must not stop gxserver. The desktop host starts or reuses the daemon during launch, then treats it as an independent backend process so terminal/session backend state survives window and app lifetime changes.
     */
    /**
     CDXC:TitlebarKeepAwake 2026-05-28-19:28:
     Closing Ghostex must restore normal lid-close sleep even if the React
     titlebar cannot run cleanup. The privileged helper also expires crashed
     leases, but normal app termination should proactively disable the policy.
     */
    let semaphore = DispatchSemaphore(value: 0)
    lidSleepHelperClient.setEnabled(false, installIfNeeded: false) { _ in
      semaphore.signal()
    }
    _ = semaphore.wait(timeout: .now() + 2)
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
     CDXC:NativeActivation 2026-05-27-07:24
     Keep app activation breadcrumbs for focus diagnostics after removing the
     old IDE and Chrome Canary attachment controllers.
     */
    Self.appendNativeHostLifecycleLog(
      "applicationWillBecomeActive pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.activation.willBecomeActive")
  }

  func applicationDidBecomeActive(_ notification: Notification) {
    Self.appendNativeHostLifecycleLog(
      "applicationDidBecomeActive pid=\(ProcessInfo.processInfo.processIdentifier) windowVisible=\(window?.isVisible ?? false) keyWindow=\(window?.isKeyWindow ?? false) frontmost=\(NSWorkspace.shared.frontmostApplication?.localizedName ?? "<missing>") lastActivationRequest=\(describeLastNativeActivationRequest()) recentInput=\(describeRecentNativeInputEvent()) workspace=\(describeWorkspaceActivationSnapshot())"
    )
    logNativeActivationLifecycleEvent("nativeHost.activation.didBecomeActive")
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
    case .leftMouseDragged: return "leftMouseDragged"
    case .leftMouseUp: return "leftMouseUp"
    case .rightMouseDown: return "rightMouseDown"
    case .rightMouseDragged: return "rightMouseDragged"
    case .rightMouseUp: return "rightMouseUp"
    case .otherMouseDown: return "otherMouseDown"
    case .otherMouseDragged: return "otherMouseDragged"
    case .otherMouseUp: return "otherMouseUp"
    case .keyDown: return "keyDown"
    default: return "\(eventType.rawValue)"
    }
  }

  private static func isNativeMouseActivationBoundaryEvent(_ eventType: NSEvent.EventType) -> Bool {
    switch eventType {
    case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown,
      .otherMouseUp:
      return true
    default:
      return false
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
      appSupportURL?.appendingPathComponent("com.mitchellh.ghostty/config.ghostty").path,
      appSupportURL?.appendingPathComponent("com.ghostty.org/config.ghostty").path,
      appSupportURL?.appendingPathComponent("Ghostty/config.ghostty").path,
      appSupportURL?.appendingPathComponent("com.mitchellh.ghostty/config").path,
      appSupportURL?.appendingPathComponent("com.ghostty.org/config").path,
      appSupportURL?.appendingPathComponent("Ghostty/config").path,
    ].compactMap { $0 }
    /**
     CDXC:NativeTerminals 2026-04-26-06:53
     Installed Ghostty for macOS stores user settings in Application Support
     on this machine. Prefer that real app config before falling back to
     Ghostty's default loader so embedded terminals match the user's app.

     CDXC:NativeIME 2026-06-13-02:32:
     Current Ghostty prefers config.ghostty over the legacy config filename. Match that order so user keybinds such as Shift+Enter are loaded into embedded surfaces instead of silently falling back to a Ghostex-generated legacy config.
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

  private func ghosttyConfigColor(_ key: String) -> NSColor? {
    guard let color = ghosttyConfigRawColor(key) else {
      return nil
    }
    return NSColor(
      calibratedRed: CGFloat(color.r) / 255,
      green: CGFloat(color.g) / 255,
      blue: CGFloat(color.b) / 255,
      alpha: 1)
  }

  private func ghosttyConfigColorHex(_ key: String) -> String? {
    guard let color = ghosttyConfigRawColor(key) else {
      return nil
    }
    return String(format: "#%02X%02X%02X", color.r, color.g, color.b)
  }

  private func ghosttyConfigRawColor(_ key: String) -> ghostty_config_color_s? {
    guard let config = ghostty.config.config else {
      return nil
    }
    var color = ghostty_config_color_s()
    guard ghostty_config_get(config, &color, key, UInt(key.lengthOfBytes(using: .utf8))) else {
      return nil
    }
    return color
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

     CDXC:Diagnostics 2026-06-06-07:09:
     The force flag is not a normal-mode logging override for routine
     breadcrumbs. Persist warning/error/failure-like session-title events with
     Debugging Mode off, and keep all other session-title diagnostics behind the
     settings toggle.
     */
    guard isNativePersistentLogImportantDiagnostic(event) || NativeDebugLogging.isEnabled else {
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
    guard isNativePersistentLogImportantDiagnostic(event) || NativeDebugLogging.isEnabled else {
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

  fileprivate static func appendLayoutLayeringDebugLog(
    event: String, details: String?, force: Bool = false
  ) {
    NativeLayoutLayeringDebugLog.append(
      event: event,
      details: [
        "details": nullableLogString(details),
        "source": "native-sidebar",
      ],
      force: force)
  }

  fileprivate static func appendRestoreDebugLog(event: String, details: String?) {
    /**
     CDXC:WorkspaceRestore 2026-06-02-15:27:
     The native sidebar owns current-window layout restore while gxserver owns shared project/session persistence. Write restore diagnostics into a dedicated app storage logs file so local layout cache, localStorage persistence, and native terminal recreation can be traced independently from session-title logs.
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

  fileprivate static func appendSidebarCollapseStateDebugLog(event: String, details: String?) {
    /**
     CDXC:SidebarCollapseDiagnostics 2026-06-02-23:52:
     Sidebar disclosure-state restart repros need a dedicated log under the
     shared support-bundle logs directory. Keep writes behind Debugging Mode and
     persist only the already-sanitized webview summary so project names, paths,
     and raw localStorage payloads never reach disk.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("sidebar-collapse-state-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(
      message, to: logURL, logsDirectory: logsDirectory, label: "sidebar collapse state debug")
  }

  fileprivate static func appendProjectBoardDebugLog(event: String, details: String?) {
    /**
     CDXC:ProjectBoardDiagnostics 2026-05-28-12:32:
     Project-page create/start diagnostics need their own app-storage log file
     so Beads creation, title generation, agent launch, and worktree setup
     breadcrumbs can be inspected without mixing them into terminal-focus or
     session-title logs. These are regular diagnostics, so Settings Debugging
     Mode is the final gate before any file write.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("project-board-debug.log")
    let message = details.map { "\(event) \($0)" } ?? event
    appendLogLine(message, to: logURL, logsDirectory: logsDirectory, label: "project board debug")
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
        key: command.key.rawValue, payloadJson: command.payloadJson)
    } catch {
      appendRestoreDebugLog(
        event: "nativeSidebar.sharedStorage.persistFailed",
        details: jsonObjectString([
          "error": error.localizedDescription,
          "key": command.key.rawValue,
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
    let line = "[\(logDateFormatter.string(from: Date()))] \(NativeLogPrivacy.sanitizeLogLine(message))\n"

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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      logger.warning("failed to write \(label) log: \(sanitizedError)")
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
    removeMainWindowTrafficLightLayoutObservers()
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
  private func installAppShotsEventMonitors() {
    if let appShotsLocalEventMonitor {
      NSEvent.removeMonitor(appShotsLocalEventMonitor)
    }
    if let appShotsGlobalEventMonitor {
      NSEvent.removeMonitor(appShotsGlobalEventMonitor)
    }
    let handler: (NSEvent) -> Void = { [weak self] event in
      Task { @MainActor in
        self?.handleAppShotsModifierEvent(event)
      }
    }
    appShotsLocalEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) {
      event in
      handler(event)
      return event
    }
    appShotsGlobalEventMonitor = NSEvent.addGlobalMonitorForEvents(matching: .flagsChanged) {
      event in
      handler(event)
    }
  }

  @MainActor
  private func handleAppShotsModifierEvent(_ event: NSEvent) {
    let settings = nativeSettingsStore.readAppShotsSettings()
    guard settings.enabled else {
      appShotsPressedModifierKeyCodes.removeAll()
      lastAppShotsDoubleTap = nil
      return
    }
    guard shouldTriggerAppShot(for: event, hotkey: settings.hotkey) else {
      return
    }
    captureAppShot(trigger: settings.hotkey)
  }

  @MainActor
  private func shouldTriggerAppShot(for event: NSEvent, hotkey: String) -> Bool {
    let now = event.timestamp
    switch hotkey {
    case "double-left-shift":
      return shouldTriggerAppShotDoubleTap(event: event, keyCode: 56, modifier: .shift, now: now)
    case "double-left-option":
      return shouldTriggerAppShotDoubleTap(event: event, keyCode: 58, modifier: .option, now: now)
    default:
      let commandKeyCodes: Set<UInt16> = [54, 55]
      guard commandKeyCodes.contains(event.keyCode) else {
        return false
      }
      if event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command) {
        appShotsPressedModifierKeyCodes.insert(event.keyCode)
      } else {
        appShotsPressedModifierKeyCodes.remove(event.keyCode)
      }
      let shouldTrigger = commandKeyCodes.isSubset(of: appShotsPressedModifierKeyCodes)
      if shouldTrigger {
        appShotsPressedModifierKeyCodes.removeAll()
      }
      return shouldTrigger
    }
  }

  @MainActor
  private func shouldTriggerAppShotDoubleTap(
    event: NSEvent,
    keyCode: UInt16,
    modifier: NSEvent.ModifierFlags,
    now: TimeInterval
  ) -> Bool {
    guard event.keyCode == keyCode else {
      return false
    }
    let isPress = event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(modifier)
    guard isPress else {
      return false
    }
    defer {
      lastAppShotsDoubleTap = (keyCode: keyCode, timestamp: now)
    }
    guard let previous = lastAppShotsDoubleTap,
      previous.keyCode == keyCode,
      now - previous.timestamp <= 0.45
    else {
      return false
    }
    lastAppShotsDoubleTap = nil
    return true
  }

  @MainActor
  private func captureAppShot(trigger: String) {
    let now = Date()
    if let lastAppShotsCaptureAt, now.timeIntervalSince(lastAppShotsCaptureAt) < 0.9 {
      return
    }
    lastAppShotsCaptureAt = now
    guard let root = window?.contentView as? ghostexRootView else {
      return
    }
    do {
      try ghostexRootView.postFrontmostAppShot(trigger: trigger, to: root)
    } catch {
      root.postHostEvent(.appShotCaptureFailed(message: error.localizedDescription))
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
  @discardableResult
  private func startSparkleUpdater() -> Bool {
    /**
     CDXC:AutoUpdate 2026-06-08-19:16:
     The compact update flow still relies on Sparkle's updater engine. Start
     SPUUpdater directly before quiet availability probes so Ghostex can replace
     only the user driver UI while preserving Sparkle's appcast and install
     state machine.
     */
    if didStartSparkleUpdater {
      return true
    }
    do {
      try sparkleUpdater.start()
      didStartSparkleUpdater = true
      return true
    } catch {
      showSparkleStartupError(error)
      return false
    }
  }

  @MainActor
  private func showSparkleStartupError(_ error: Error) {
    let alert = NSAlert()
    alert.alertStyle = .warning
    alert.messageText = "Ghostex updates are unavailable"
    alert.informativeText = "Sparkle could not start the updater. \(error.localizedDescription)"
    alert.addButton(withTitle: "OK")
    alert.runModal()
  }

  @MainActor
  private func startSparkleUpdateAvailabilityProbes() {
    /**
     CDXC:AutoUpdate 2026-05-28-14:19:
     Launch should still check whether a newer Ghostex build exists, but the
     first user-facing surface must be the quiet titlebar download button.
     Use Sparkle's informational probe so launch never offers or downloads the
     update before the user clicks the titlebar control.

     CDXC:AutoUpdate 2026-06-08-18:21:
     Ghostex should also repeat that quiet availability check every 15 minutes
     while the app stays open, so users who keep the app running still get the
     titlebar update affordance soon after a new appcast is published.
     */
    sparkleAvailabilityProbeTimer?.invalidate()
    runSparkleUpdateAvailabilityProbe()
    let timer = Timer.scheduledTimer(
      withTimeInterval: ghostexSparkleAvailabilityProbeInterval,
      repeats: true
    ) { [weak self] _ in
      Task { @MainActor in
        self?.runSparkleUpdateAvailabilityProbe()
      }
    }
    timer.tolerance = 60
    sparkleAvailabilityProbeTimer = timer
  }

  @MainActor
  private func runSparkleUpdateAvailabilityProbe() {
    sparkleUpdater.checkForUpdateInformation()
  }

  @IBAction func checkForUpdates(_ sender: Any?) {
    sparkleUpdater.checkForUpdates()
  }

  @MainActor private func showUpdateDialogFromTitlebar() {
    /**
     CDXC:AutoUpdate 2026-05-28-14:19:
     The titlebar download button is the consent boundary for update UI. Once
     the user clicks it, hand off to Sparkle so signing, release notes,
     download, and install behavior stay on the supported path.

     CDXC:AutoUpdate 2026-06-08-19:16:
     The handoff uses GhostexSparkleUserDriver rather than Sparkle's full
     standard UI so the release notes and final relaunch prompt remain visible
     while the download and extraction progress windows stay hidden.
     */
    sparkleUpdater.checkForUpdates()
  }

  @MainActor private func setSparkleUpdateAvailable(_ available: Bool) {
    isSparkleUpdateAvailable = available
    /**
     CDXC:AutoUpdate 2026-05-28-14:26:
     Repeat availability pushes are intentional because Sparkle can learn about
     scheduled updates before the titlebar webview finishes loading. Re-sending
     the current boolean lets later probes hydrate the titlebar without adding a
     fallback cache in React.
     */
    (window?.contentView as? ghostexRootView)?.setTitlebarUpdateAvailable(available)
  }

  @IBAction nonisolated func closeAllWindows(_ sender: Any?) {}

  @IBAction nonisolated func toggleQuickTerminal(_ sender: Any?) {}

  nonisolated func toggleVisibility(_ sender: Any?) {}

  var supportsGentleScheduledUpdateReminders: Bool {
    true
  }

  func standardUserDriverShouldHandleShowingScheduledUpdate(
    _ update: SUAppcastItem,
    andInImmediateFocus immediateFocus: Bool
  ) -> Bool {
    /**
     CDXC:AutoUpdate 2026-05-28-14:19:
     Sparkle scheduled checks must not raise the standard update alert on their
     own. Ghostex handles scheduled availability as the titlebar download
     affordance, while user-initiated checks still use Sparkle's normal dialog.
     */
    setSparkleUpdateAvailable(true)
    return false
  }

  func standardUserDriverWillHandleShowingUpdate(
    _ handleShowingUpdate: Bool,
    forUpdate update: SUAppcastItem,
    state: SPUUserUpdateState
  ) {
    if !state.userInitiated {
      setSparkleUpdateAvailable(true)
    }
  }

  func standardUserDriverDidReceiveUserAttention(forUpdate update: SUAppcastItem) {
    /**
     CDXC:AutoUpdate 2026-06-08-08:50:
     Clicking the titlebar update button should not consume the update affordance.
     Keep it visible while the installed app build remains behind the Sparkle appcast; only a confirmed latest-version check should hide it.
     */
    setSparkleUpdateAvailable(true)
  }

  func standardUserDriverWillFinishUpdateSession() {
    /**
     CDXC:AutoUpdate 2026-06-08-08:50:
     Closing or finishing Sparkle's user-facing update dialog is not proof that
     Ghostex is on the latest build. Preserve the titlebar button so users can
     reopen the update flow until Sparkle later reports no valid update.
     */
    setSparkleUpdateAvailable(isSparkleUpdateAvailable)
  }

  func updater(_ updater: SPUUpdater, didFindValidUpdate item: SUAppcastItem) {
    setSparkleUpdateAvailable(true)
  }

  func updaterDidNotFindUpdate(_ updater: SPUUpdater) {
    setSparkleUpdateAvailable(false)
  }

  func updater(_ updater: SPUUpdater, didAbortWithError error: Error) {
    setSparkleUpdateAvailable(isSparkleUpdateAvailable)
  }

  nonisolated func syncFloatOnTopMenu(_ window: NSWindow) {}

  nonisolated func setSecureInput(_ mode: Ghostty.SetSecureInput) {}

  @MainActor
  private func makeWindow(gxserverStatus: GxserverClientStatus? = nil) {
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
    /*
     CDXC:WorkspaceLayout 2026-06-07-16:53:
     Native workspace chrome should derive its automatic background from the same loaded Ghostty config as embedded terminals. Resolve the color after Ghostty initialization so themes and user config participate, with black only when Ghostty cannot provide a background.
     */
    let root = ghostexRootView(
      ghostty: ghostty,
      defaultWorkspaceBackgroundColor: ghosttyConfigColor("background") ?? .black,
      gxserverBootstrap: gxserverClient.webBootstrap(status: gxserverStatus),
      initialUpdateAvailable: isSparkleUpdateAvailable,
      sendEvent: { [weak self] event in
        self?.bridge?.send(event)
        (self?.window?.contentView as? ghostexRootView)?.postHostEvent(event)
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
      openWorkspaceInFinder: { [weak self] command in
        self?.handle(.openWorkspaceInFinder(command))
      },
      openWorkspaceInIde: { [weak self] command in
        self?.handle(.openWorkspaceInIde(command))
      },
      setAppTitlebarTitle: { [weak self] title in
        self?.updateAppTitlebarTitle(title)
      },
      setSessionStatusIndicators: { [weak sessionStatusIndicatorController] command in
        sessionStatusIndicatorController?.apply(command)
      },
      setPetOverlayState: { [weak petOverlayController] command in
        petOverlayController?.apply(command)
      },
      showUpdateDialogFromTitlebar: { [weak self] in
        self?.showUpdateDialogFromTitlebar()
      },
      startGxserverFromTitlebar: { [weak self] in
        self?.startGxserverFromUserAction(reason: "start")
      },
      stopGxserverFromTitlebar: { [weak self] in
        self?.stopGxserverFromUserAction()
      },
      restartGxserverFromTitlebar: { [weak self] in
        self?.restartGxserverFromUserAction()
      },
      setGxserverAlwaysStartFromTitlebar: { [weak self] enabled in
        self?.setGxserverAlwaysStartFromUserAction(enabled: enabled)
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
      root?.handleWindowFirstResponderChanged(responder)
    }
    window.onKeyDownDispatch = { [weak root] event in
      root?.workspaceView.windowKeyDownDispatch(event)
    }
    window.onKeyEquivalent = { [weak root] event in
      root?.handleHotkeyEquivalent(event) ?? false
    }
    window.onTitlebarMouseEvent = { [weak root] event in
      root?.routeTitlebarMouseEventFromWindow(event) ?? false
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
    window.acceptsMouseMovedEvents = true
    window.minSize = ghostexMainWindowMinimumSize
    window.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor
    window.contentView = root
    window.delegate = self
    self.window = window
    scheduleMainWindowTrafficLightPositioning(on: window)
    window.makeKeyAndOrderFront(nil)
    scheduleMainWindowTrafficLightPositioning(on: window)
    recordNativeActivationRequest(reason: "startup.makeWindow")
    NSApp.activate(ignoringOtherApps: true)
    scheduleMainWindowTrafficLightPositioning(on: window)
    root.scheduleFloatingPromptEditorPrewarmAfterLaunch()
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
     settings do not fork #95d7f6 attention or orange working navigation behavior.
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

    let size = CGSize(
      width: max(stored.width ?? 1440, ghostexMainWindowMinimumSize.width),
      height: max(stored.height ?? 900, ghostexMainWindowMinimumSize.height))
    return Self.defaultInitialWindowFrame(size: size)
  }

  private static func restoredMainWindowFrame(from stored: NativeMainWindowChromeSettings)
    -> NSRect?
  {
    guard let storedFrame = stored.frame else {
      return nil
    }
    guard let launchScreen = resolvedMainWindowLaunchScreen(from: stored, storedFrame: storedFrame)
    else {
      return nil
    }
    let size = constrainedMainWindowSize(storedFrame.size, for: launchScreen.screen)
    let proposedFrame: NSRect
    /**
     CDXC:NativeWindowChrome 2026-06-05-05:06:
     Relaunch must reopen the macOS app at the same outer-window size and
     display-relative position saved at close. If that monitor is absent, choose
     the nearest available display and preserve the saved relative placement
     while shrinking and clamping the frame to the best visible size available
     on that display.
     */
    if launchScreen.isStoredDisplayConnected, let storedScreenFrame = stored.screenFrame {
      proposedFrame = NSRect(
        x: launchScreen.screen.frame.minX + (storedFrame.minX - storedScreenFrame.minX),
        y: launchScreen.screen.frame.minY + (storedFrame.minY - storedScreenFrame.minY),
        width: size.width,
        height: size.height)
    } else if let storedScreenFrame = stored.screenFrame {
      proposedFrame = remappedMainWindowFrame(
        storedFrame: storedFrame,
        storedScreenFrame: storedScreenFrame,
        targetScreen: launchScreen.screen,
        targetSize: size)
    } else {
      proposedFrame = NSRect(origin: storedFrame.origin, size: size)
    }
    return clampedMainWindowFrame(proposedFrame, to: launchScreen.screen.visibleFrame)
  }

  private static func defaultInitialWindowFrame(size: CGSize) -> NSRect {
    let screenFrame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
    let width = size.width
    let height = size.height
    let x = screenFrame.minX + min(100, max(0, screenFrame.width - width))
    let y = screenFrame.minY + min(80, max(0, screenFrame.height - height))
    return NSRect(x: x, y: y, width: width, height: height)
  }

  private struct MainWindowLaunchScreen {
    let screen: NSScreen
    let isStoredDisplayConnected: Bool
  }

  private static func resolvedMainWindowLaunchScreen(
    from stored: NativeMainWindowChromeSettings,
    storedFrame: NSRect
  ) -> MainWindowLaunchScreen? {
    if let screen = screen(matchingIdentifier: stored.screenID) {
      return MainWindowLaunchScreen(screen: screen, isStoredDisplayConnected: true)
    }
    if let storedScreenFrame = stored.screenFrame {
      if let screen = screen(containingLargestVisibleAreaOf: storedScreenFrame) {
        return MainWindowLaunchScreen(screen: screen, isStoredDisplayConnected: false)
      }
      let storedScreenCenter = NSPoint(x: storedScreenFrame.midX, y: storedScreenFrame.midY)
      if let screen = screen(nearestTo: storedScreenCenter) {
        return MainWindowLaunchScreen(screen: screen, isStoredDisplayConnected: false)
      }
    }
    if let screen = screen(containingLargestVisibleAreaOf: storedFrame) {
      return MainWindowLaunchScreen(screen: screen, isStoredDisplayConnected: false)
    }
    return (NSScreen.main ?? NSScreen.screens.first).map {
      MainWindowLaunchScreen(screen: $0, isStoredDisplayConnected: false)
    }
  }

  private static func remappedMainWindowFrame(
    storedFrame: NSRect,
    storedScreenFrame: NSRect,
    targetScreen: NSScreen,
    targetSize: NSSize
  ) -> NSRect {
    let targetFrame = targetScreen.visibleFrame
    let xRatio = mainWindowPositionRatio(
      origin: storedFrame.minX,
      containerOrigin: storedScreenFrame.minX,
      containerLength: storedScreenFrame.width,
      windowLength: storedFrame.width)
    let yRatio = mainWindowPositionRatio(
      origin: storedFrame.minY,
      containerOrigin: storedScreenFrame.minY,
      containerLength: storedScreenFrame.height,
      windowLength: storedFrame.height)
    return NSRect(
      x: targetFrame.minX + xRatio * max(0, targetFrame.width - targetSize.width),
      y: targetFrame.minY + yRatio * max(0, targetFrame.height - targetSize.height),
      width: targetSize.width,
      height: targetSize.height)
  }

  private static func mainWindowPositionRatio(
    origin: CGFloat,
    containerOrigin: CGFloat,
    containerLength: CGFloat,
    windowLength: CGFloat
  ) -> CGFloat {
    let availableLength = containerLength - windowLength
    guard availableLength > 0 else {
      return 0.5
    }
    return min(1, max(0, (origin - containerOrigin) / availableLength))
  }

  private static func constrainedMainWindowSize(_ size: NSSize, for screen: NSScreen) -> NSSize {
    let visibleFrame = screen.visibleFrame
    return NSSize(
      width: min(
        max(size.width, ghostexMainWindowMinimumSize.width),
        max(visibleFrame.width, ghostexMainWindowMinimumSize.width)),
      height: min(
        max(size.height, ghostexMainWindowMinimumSize.height),
        max(visibleFrame.height, ghostexMainWindowMinimumSize.height)))
  }

  private static func clampedMainWindowFrame(_ frame: NSRect, to visibleFrame: NSRect) -> NSRect {
    let width = frame.width
    let height = frame.height
    let maxX = max(visibleFrame.minX, visibleFrame.maxX - width)
    let maxY = max(visibleFrame.minY, visibleFrame.maxY - height)
    return NSRect(
      x: min(max(frame.minX, visibleFrame.minX), maxX),
      y: min(max(frame.minY, visibleFrame.minY), maxY),
      width: width,
      height: height)
  }

  private func persistMainWindowChrome() {
    guard let window else {
      return
    }
    let frame = window.frame
    /**
     CDXC:NativeWindowChrome 2026-05-27-07:24
     Main-window persistence now records the actual visible AppKit window frame.
     The offscreen IDE-attachment helper state was removed with the attachment controllers.
     */
    let frameForPersistence = frame
    guard let screen = Self.screen(containingLargestVisibleAreaOf: frameForPersistence) else {
      return
    }
    lastVisibleMainWindowFrameForPersistence = frameForPersistence
    nativeSettingsStore.persistMainWindowChrome(frame: frameForPersistence, screen: screen)
  }

  private func positionMainWindowTrafficLightButtons(on window: NSWindow) {
    /**
     CDXC:NativeWindowChrome 2026-05-25-07:22:
     The macOS traffic-light buttons should be positioned from the custom 35px
     titlebar center, then pushed visually lower by the configured offset.
     Compute the absolute frame on every AppKit relayout so close/minimize/zoom
     do not snap back to AppKit's default 30px placement.

     CDXC:NativeWindowChrome 2026-05-28-11:14:
     The traffic-light group should also move right until the close button's
     left inset matches its computed top inset. Derive the horizontal offset
     from the final vertical placement so top and left spacing stay equal when
     the titlebar height or visual-down offset changes. Frame observers must
     ignore frames set by this function so AppKit notifications correct external
     relayouts without recursively re-entering the positioning path.

     CDXC:NativeWindowChrome 2026-05-28-11:38:
     AppKit can reset only one standard button during titlebar churn, which left
     the yellow minimize button behind after the red and green buttons moved.
     Set each button's absolute leading position from the close button target
     and AppKit's standard 23px button cadence instead of applying one relative
     delta to whatever partial state AppKit last produced.
     */
    guard !isPositioningMainWindowTrafficLightButtons else {
      return
    }
    guard
      let closeButton = window.standardWindowButton(.closeButton),
      let closeTitlebarView = closeButton.superview
    else {
      return
    }
    isPositioningMainWindowTrafficLightButtons = true
    defer {
      isPositioningMainWindowTrafficLightButtons = false
    }
    let desiredOriginY = { (frame: CGRect, titlebarView: NSView) -> CGFloat in
      if titlebarView.isFlipped {
        return (ghostexAppTitlebarHeight - frame.height) / 2
          + ghostexTrafficLightVisualDownOffset
      }
      return titlebarView.bounds.height - ((ghostexAppTitlebarHeight + frame.height) / 2)
        - ghostexTrafficLightVisualDownOffset
    }
    let closeDesiredOriginY = desiredOriginY(closeButton.frame, closeTitlebarView)
    let closeTopInset = closeTitlebarView.isFlipped
      ? closeDesiredOriginY
      : closeTitlebarView.bounds.height - closeDesiredOriginY - closeButton.frame.height
    for buttonType in Self.standardWindowButtonTypes {
      guard let button = window.standardWindowButton(buttonType), let titlebarView = button.superview else {
        continue
      }
      let frame = button.frame
      let leadingOffset = Self.standardWindowButtonLeadingOffsets[buttonType] ?? 0
      let desiredFrame = CGRect(
        x: closeTopInset + leadingOffset,
        y: desiredOriginY(frame, titlebarView),
        width: frame.width,
        height: frame.height
      )
      guard abs(frame.origin.x - desiredFrame.origin.x) > 0.5
        || abs(frame.origin.y - desiredFrame.origin.y) > 0.5
      else {
        continue
      }
      button.frame = desiredFrame
    }
  }

  private func scheduleMainWindowTrafficLightPositioning(on window: NSWindow) {
    /**
     CDXC:NativeWindowChrome 2026-05-25-07:22:
     AppKit can restore standard window-button frames during activation and resize layout passes after the custom titlebar is already visible. Reapply the 35px-titlebar positioning plus the configured visual-down offset at the end of those passes so the final on-screen traffic lights remain in the requested spot.

     CDXC:NativeWindowChrome 2026-05-28-11:05:
     First launch can run another AppKit titlebar layout after makeKeyAndOrderFront and NSApp.activate, before a later key-window transition occurs. Observe the titlebar container and standard button frames so startup relayouts use the same final correction instead of waiting for Alt-Tab to trigger windowDidBecomeKey.
     */
    installMainWindowTrafficLightLayoutObservers(on: window)
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

  private func installMainWindowTrafficLightLayoutObservers(on window: NSWindow) {
    guard
      let closeButton = window.standardWindowButton(.closeButton),
      let titlebarView = closeButton.superview
    else {
      return
    }
    guard trafficLightLayoutObservedWindow !== window
      || trafficLightLayoutObservedTitlebarView !== titlebarView
      || trafficLightLayoutObservers.isEmpty
    else {
      return
    }
    removeMainWindowTrafficLightLayoutObservers()

    titlebarView.postsFrameChangedNotifications = true
    titlebarView.postsBoundsChangedNotifications = true
    trafficLightLayoutObservedWindow = window
    trafficLightLayoutObservedTitlebarView = titlebarView

    let notificationCenter = NotificationCenter.default
    let titlebarFrameObserver = notificationCenter.addObserver(
      forName: NSView.frameDidChangeNotification,
      object: titlebarView,
      queue: .main
    ) { [weak self, weak window] _ in
      guard let self, let window, self.window === window,
        !self.isPositioningMainWindowTrafficLightButtons
      else {
        return
      }
      self.scheduleMainWindowTrafficLightPositioning(on: window)
    }
    let titlebarBoundsObserver = notificationCenter.addObserver(
      forName: NSView.boundsDidChangeNotification,
      object: titlebarView,
      queue: .main
    ) { [weak self, weak window] _ in
      guard let self, let window, self.window === window,
        !self.isPositioningMainWindowTrafficLightButtons
      else {
        return
      }
      self.scheduleMainWindowTrafficLightPositioning(on: window)
    }

    trafficLightLayoutObservers = [titlebarFrameObserver, titlebarBoundsObserver]
    for buttonType in Self.standardWindowButtonTypes {
      guard let button = window.standardWindowButton(buttonType) else {
        continue
      }
      button.postsFrameChangedNotifications = true
      trafficLightLayoutObservers.append(
        notificationCenter.addObserver(
          forName: NSView.frameDidChangeNotification,
          object: button,
          queue: .main
        ) { [weak self, weak window] _ in
          guard let self, let window, self.window === window,
            !self.isPositioningMainWindowTrafficLightButtons
          else {
            return
          }
          self.scheduleMainWindowTrafficLightPositioning(on: window)
        }
      )
    }
  }

  private func removeMainWindowTrafficLightLayoutObservers() {
    guard !trafficLightLayoutObservers.isEmpty else {
      return
    }
    let notificationCenter = NotificationCenter.default
    for observer in trafficLightLayoutObservers {
      notificationCenter.removeObserver(observer)
    }
    trafficLightLayoutObservers = []
    trafficLightLayoutObservedWindow = nil
    trafficLightLayoutObservedTitlebarView = nil
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

  private static func screen(nearestTo point: NSPoint) -> NSScreen? {
    NSScreen.screens.min { lhs, rhs in
      let lhsDistanceX = lhs.frame.midX - point.x
      let lhsDistanceY = lhs.frame.midY - point.y
      let rhsDistanceX = rhs.frame.midX - point.x
      let rhsDistanceY = rhs.frame.midY - point.y
      return lhsDistanceX * lhsDistanceX + lhsDistanceY * lhsDistanceY
        < rhsDistanceX * rhsDistanceX + rhsDistanceY * rhsDistanceY
    }
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

  @MainActor
  private func startGxserverBootstrapThenCreateWindow() {
    Task { [weak self] in
      guard let self else { return }
      let status = await self.gxserverClient.startOrReuse()
      await MainActor.run {
        /*
         CDXC:GxserverBootstrap 2026-06-07-12:02:
         The sidebar may miss the first native gxserverStatus host event while the WebKit document is still installing listeners. Seed the startup status into the injected bootstrap object so React sees a running daemon before it decides whether startup API work is allowed.
        */
        self.makeWindow(gxserverStatus: status)
        self.installAppHotkeyEventMonitor()
        self.installAppShotsEventMonitors()
        self.startBridge()
        self.publishGxserverBootstrapStatus(status)
      }
    }
  }

  @MainActor
  private func publishGxserverBootstrapStatus(_ status: GxserverClientStatus) {
    guard
      let payloadData = try? JSONSerialization.data(withJSONObject: gxserverClient.statusPayload(status)),
      let payloadJson = String(data: payloadData, encoding: .utf8)
    else {
      return
    }
    let event = HostEvent.gxserverStatus(payloadJson: payloadJson)
    bridge?.send(event)
    (window?.contentView as? ghostexRootView)?.postHostEvent(event)
    Self.appendNativeHostLifecycleLog(
      "gxserver.bootstrap state=\(status.state) ok=\(status.ok) message=\(status.message)")
  }

  @MainActor
  private func startGxserverFromUserAction(reason: String) {
    publishGxserverBootstrapStatus(gxserverClient.startingStatus(message: "Starting gxserver..."))
    Task { [weak self] in
      guard let self else { return }
      let status = await self.gxserverClient.startOrReuse(allowStart: true)
      await MainActor.run {
        Self.appendNativeHostLifecycleLog("gxserver.\(reason) state=\(status.state) ok=\(status.ok)")
        self.publishGxserverBootstrapStatus(status)
      }
    }
  }

  @MainActor
  private func stopGxserverFromUserAction() {
    publishGxserverBootstrapStatus(gxserverClient.startingStatus(message: "Stopping gxserver..."))
    Task { [weak self] in
      guard let self else { return }
      let status = await self.gxserverClient.stopControlPlane()
      await MainActor.run {
        Self.appendNativeHostLifecycleLog("gxserver.stop state=\(status.state) ok=\(status.ok)")
        self.publishGxserverBootstrapStatus(status)
      }
    }
  }

  @MainActor
  private func restartGxserverFromUserAction() {
    publishGxserverBootstrapStatus(gxserverClient.startingStatus(message: "Restarting gxserver..."))
    Task { [weak self] in
      guard let self else { return }
      _ = await self.gxserverClient.stopControlPlane()
      let status = await self.gxserverClient.startOrReuse(allowStart: true)
      await MainActor.run {
        Self.appendNativeHostLifecycleLog("gxserver.restart state=\(status.state) ok=\(status.ok)")
        self.publishGxserverBootstrapStatus(status)
      }
    }
  }

  @MainActor
  private func setGxserverAlwaysStartFromUserAction(enabled: Bool) {
    /**
     CDXC:GxserverBootstrap 2026-05-31-03:56:
     The Resources dropdown owns the compact daemon controls. The Always start
     checkbox changes only future Ghostex launch behavior; explicit Start and
     Restart still run immediately so users can recover a stopped daemon.
    */
    gxserverClient.alwaysStartOnLaunch = enabled
    Task { [weak self] in
      guard let self else { return }
      let status = await self.gxserverClient.startOrReuse(allowStart: false)
      await MainActor.run {
        self.publishGxserverBootstrapStatus(status)
      }
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

       CDXC:GxserverBootstrap 2026-05-30-15:39:
       gxserver owns port 58744 in the hard cutover. The dev-only native CLI bridge uses 58742 so local desktop automation cannot bind or mask the daemon API port.

       CDXC:GxserverMacBootstrap 2026-05-30-15:13:
       gxserver owns fixed local API port 58744, so the dev-only native CLI
       bridge must not bind that port before daemon bootstrap. Keep production
       on 58743 and move dev bridge traffic to 58742.
       */
      let bridgePort: UInt16 = Self.isDevBundleIdentifier(Bundle.main.bundleIdentifier)
        ? 58742
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
	          persistenceSessionCreated: nil,
	          sessionId: "bridge-error",
	          sessionPersistenceName: nil,
	          sessionPersistenceProvider: nil,
	          /**
	           CDXC:CliBridgeTransport 2026-05-21-00:56:
	           The bridge-error terminal must remain a normal shell session that receives diagnostic initial input, so the explicit shellCommand contract is nil here.

	           CDXC:GxserverBootstrap 2026-05-30-16:16:
	           This diagnostic pane is created only when the native bridge fails before sidebar startup, so it must not claim a gxserver-created zmx attach command or persistence-created state.
	           */
	          shellAttachCommand: nil,
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

  private static func isDevBundleIdentifier(_ bundleIdentifier: String?) -> Bool {
    /**
     CDXC:GxserverVerification 2026-05-30-16:25:
     Worktree verification uses a uniquely identified Ghostex dev app so Cua Driver can launch the built bundle instead of the installed /Applications copy. Every com.madda.ghostex-dev... bundle keeps the dev bridge on 58742 because gxserver owns 58744 and production keeps 58743.
     */
    bundleIdentifier?.hasPrefix("com.madda.ghostex-dev") == true
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
  private func startGxserverBootstrap() {
    Task { [weak self] in
      guard let self else { return }
      let result = await self.gxserverClient.startOrReuse()
      await MainActor.run {
        Self.appendNativeHostLifecycleLog("gxserver.bootstrap ok=\(result.ok) message=\(result.message)")
        if !result.ok {
          self.showMessage(.init(level: .error, message: result.message))
        }
      }
    }
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
    case .focusProjectEditorCompanionSession(let command):
      workspaceView?.focusProjectEditorCompanionSession(sessionId: command.sessionId)
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
    case .setSessionPaneChrome(let command):
      workspaceView?.setSessionPaneChrome(command)
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
    case .appendLayoutLayeringDebugLog(let command):
      Self.appendLayoutLayeringDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendProjectBoardDebugLog(let command):
      Self.appendProjectBoardDebugLog(event: command.event, details: command.details)
    case .appendTerminalFocusDebugLog(let command):
      Self.appendTerminalFocusDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendRestoreDebugLog(let command):
      Self.appendRestoreDebugLog(event: command.event, details: command.details)
    case .appendSessionTitleDebugLog(let command):
      Self.appendSessionTitleDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendSidebarCollapseStateDebugLog(let command):
      Self.appendSidebarCollapseStateDebugLog(event: command.event, details: command.details)
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
    case .cancelRunProcess(let command):
      NativeProcessRegistry.shared.cancel(requestId: command.requestId)
    case .gxserverRequest(let command):
      Task { [weak self] in
        let event = await GxserverClient.request(command)
        await MainActor.run {
          self?.bridge?.send(event)
        }
      }
    case .remoteGxserverConnect(let command):
      bridge?.send(RemoteGxserverClient.shared.connectingStatus(
        remoteMachineId: command.remoteMachineId,
        requestId: command.requestId
      ))
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.connect(command)
        await MainActor.run {
          self?.bridge?.send(event)
        }
      }
    case .remoteGxserverRequest(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.request(command)
        await MainActor.run {
          self?.bridge?.send(event)
        }
      }
    case .remoteGxserverSubscribePresentation(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.subscribePresentation(command) { event in
          Task { [weak self] in
            await MainActor.run {
              self?.bridge?.send(event)
            }
          }
        }
        await MainActor.run {
          self?.bridge?.send(event)
        }
      }
    case .remoteSshPasswordSave(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.saveSshPassword(command)
        await MainActor.run {
          self?.bridge?.send(event)
        }
      }
    case .setKeepAwakeLidSleepPrevention(let command):
      LidSleepPrivilegedHelperClient.shared.setEnabled(
        command.enabled,
        requestId: command.requestId,
        installIfNeeded: command.installIfNeeded ?? command.enabled
      ) { [weak self] event in
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
    case .setOSIntegrationDefaults(let command):
      guard let bundleIdentifier = Bundle.main.bundleIdentifier else {
        showMessage(.init(level: .error, message: "Ghostex bundle identifier is missing."))
        return
      }
      let failures = AppDelegate.osIntegrationDefaultFailures(
        target: command.target,
        bundleIdentifier: bundleIdentifier)
      if failures.isEmpty {
        (window?.contentView as? ghostexRootView)?.presentAppToast(
          level: "success",
          title: "Updated macOS OS Integration defaults."
        )
      } else {
        showMessage(.init(level: .error, message: "Could not set defaults: \(failures.joined(separator: ", "))"))
      }
      sendOSIntegrationStatus()
    case .requestOSIntegrationStatus:
      sendOSIntegrationStatus()
    case .openExternalUrl(let command):
      openExternalUrl(command)
    case .openWorkspaceInFinder(let command):
      openWorkspaceInFinder(command)
    case .openWorkspaceInIde(let command):
      openWorkspaceInIde(command)
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
    case .toggleSidebarCollapsed:
      (window?.contentView as? ghostexRootView)?.toggleSidebarCollapsed()
    case .setReactTitlebarHitRegions(let command):
      (window?.contentView as? ghostexRootView)?.setReactTitlebarHitRegions(
        command.regions, overlayOpen: command.overlayOpen)
    case .showTitlebarDropdownPanel(let command):
      (window?.contentView as? ghostexRootView)?.showTitlebarDropdownPanel(command)
    case .closeTitlebarDropdownPanel:
      (window?.contentView as? ghostexRootView)?.closeTitlebarDropdownPanel()
    case .resizeTitlebarDropdownPanel(let command):
      (window?.contentView as? ghostexRootView)?.resizeTitlebarDropdownPanel(command)
    case .titlebarDropdownPanelReady(let command):
      (window?.contentView as? ghostexRootView)?.titlebarDropdownPanelReady(command)
    case .openActiveProjectEditorFromTitlebar:
      break
    case .openAgentsModeFromTitlebar:
      break
    case .openGitHubProjectFromTitlebar:
      break
    case .toggleProjectEditorCompanionFromTitlebar:
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
    case .showUpdateDialogFromTitlebar:
      showUpdateDialogFromTitlebar()
    case .startGxserverFromTitlebar:
      startGxserverFromUserAction(reason: "start")
    case .stopGxserverFromTitlebar:
      stopGxserverFromUserAction()
    case .restartGxserverFromTitlebar:
      restartGxserverFromUserAction()
    case .setGxserverAlwaysStartFromTitlebar(let command):
      setGxserverAlwaysStartFromUserAction(enabled: command.enabled)
    case .focusResourceSessionFromTitlebar:
      break
    case .sleepInactiveSessionsFromTitlebar:
      break
    case .quitResourcesFromTitlebar:
      break
    case .runSidebarCommandFromTitlebar:
      break
    case .runSidebarGitActionFromTitlebar:
      break
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
   CDXC:T3Code 2026-06-06-05:13:
   The native-host command path keeps accepting sidebar T3 session-state messages
   for protocol compatibility, but those messages cannot own provider lifetime.
   Live managed T3 panes now refresh and repair t3code through the root workspace
   pane registry.
   */
  @MainActor
  private func setT3CodeRuntimeSessionState(_ command: SetT3CodeRuntimeSessionState, reason: String) {
    /**
     CDXC:T3Code 2026-06-06-05:13:
     Sidebar-projected T3 session state is no longer allowed to own provider
     lifetime. Live native managed T3 panes are the authoritative signal, so a
     stale or gxserver-filtered hydrate payload cannot stop the runtime while a
     real T3 tab remains open.
     */
    NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.sidebarSessionState.ignored", [
      "hasRuntimeCwd": command.runtimeCwd?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
      "reason": reason,
      "runningSessionCount": command.runningSessionIds.count,
    ])
  }

  @MainActor
  private func ensureT3CodeRuntimeForRunningSessions(reason: String) {
    guard let runtimeCwd = t3RuntimeVisibleSessionCwd else {
      return
    }
    guard !NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
      return
    }
    guard !isT3RuntimeAutoStartBackedOff(logPrefix: "nativeHost", reason: reason) else {
      return
    }
    NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.runningSessions.autoStart", [
      "cwd": runtimeCwd,
      "reason": reason,
    ])
    startT3CodeRuntime(StartT3CodeRuntime(cwd: runtimeCwd))
  }

  @MainActor
  private func isT3RuntimeAutoStartBackedOff(logPrefix: String, reason: String) -> Bool {
    guard let until = t3RuntimeAutoStartBackoffUntil else {
      return false
    }
    let remainingSeconds = until.timeIntervalSinceNow
    guard remainingSeconds > 0 else {
      t3RuntimeAutoStartBackoffUntil = nil
      return false
    }
    NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.start.backoffActive", [
      "reason": reason,
      "remainingSeconds": remainingSeconds,
    ])
    return true
  }

  @MainActor
  private func recordT3RuntimeLaunchFailure(logPrefix: String, reason: String) {
    t3RuntimeAutoStartBackoffUntil = Date().addingTimeInterval(
      NativeT3RuntimeFailureNotice.autoStartBackoffInterval)
    NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.start.backoffSet", [
      "backoffSeconds": NativeT3RuntimeFailureNotice.autoStartBackoffInterval,
      "reason": reason,
    ])
    (window?.contentView as? ghostexRootView)?.postHostEvent(
      .t3RuntimeStartFailed(sessionId: nil, message: NativeT3RuntimeFailureNotice.message))
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

     CDXC:T3CodeStartup 2026-06-09-07:07:
     Passive retained startup states must not reload managed T3 web panes.
     Only an actual runtime replacement should repaint the WKWebView; otherwise
     the ten-second liveness timer can interrupt terminal typing with a spinner.
     */
    t3RuntimeAutoStartBackoffUntil = nil
    if let process = t3CodeRuntimeProcess, process.isRunning {
      /**
       CDXC:T3Code 2026-05-02-00:48
       A retained Process handle does not prove the T3 server is usable. A Bun
       runtime can keep running at high CPU while `/api/auth/session` and bearer
       bootstrap requests time out, leaving the pane as a white WKWebView. Reuse
       the handle only after the same health probe used for listener adoption.
       */
      guard NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
        if let startedAt = t3CodeRuntimeStartedAt {
          let runtimeAgeSeconds = Date().timeIntervalSince(startedAt)
          if runtimeAgeSeconds <= NativeT3RuntimeLauncher.startupGraceInterval {
            NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.booting", [
              "pid": process.processIdentifier,
              "runtimeAgeSeconds": runtimeAgeSeconds,
              "startupGraceSeconds": NativeT3RuntimeLauncher.startupGraceInterval,
            ])
            return
          }
        }
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
          return
        }
        NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        t3CodeRuntimeProcess = nil
        t3CodeRuntimeStartedAt = nil
        NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeHost")
        return startT3CodeRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
      ])
      return
    }
    if let process = t3CodeRuntimeProcess, !process.isRunning {
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.trackedExited", [
        "pid": process.processIdentifier
      ])
      t3CodeRuntimeProcess = nil
      t3CodeRuntimeStartedAt = nil
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

    let launchStartedAt: Date
    switch NativeT3RuntimeLauncher.claimLaunchStart() {
    case .retained(let launchAgeSeconds):
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.launchInProgressRetained", [
        "launchAgeSeconds": launchAgeSeconds,
        "startupGraceSeconds": NativeT3RuntimeLauncher.startupGraceInterval,
      ])
      return
    case .claimed(let claimedStartedAt):
      launchStartedAt = claimedStartedAt
    }

    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeHost")
    if NativeT3RuntimeLauncher.hasManagedRuntimeListener() {
      NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.retainedExistingUnresponsive", [
        "cwd": command.cwd,
        "port": NativeT3RuntimeLauncher.port,
      ])
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
      t3CodeRuntimeStartedAt = launchStartedAt
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
        "startedAt": launchStartedAt.timeIntervalSince1970,
      ])
      workspaceView?.reloadManagedT3WebPanes(reason: "runtimeSpawned")
      process.terminationHandler = { [weak self, outputCapture = launch.outputCapture, launchStartedAt] terminatedProcess in
        NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
        var details = outputCapture.finish()
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.exit", details)
        let status = terminatedProcess.terminationStatus
        guard NativeT3RuntimeFailureNotice.shouldNotifyLaunchExit(status: status) else {
          return
        }
        DispatchQueue.main.async {
          self?.recordT3RuntimeLaunchFailure(
            logPrefix: "nativeHost",
            reason: "processExitStatus\(status)")
        }
      }
    } catch {
      NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
      NativeT3CodePaneReproLog.append("nativeHost.t3Runtime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      recordT3RuntimeLaunchFailure(logPrefix: "nativeHost", reason: "processRunFailed")
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to start T3 Code runtime: \(sanitizedError)")
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
      t3CodeRuntimeStartedAt = nil
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
        /*
         CDXC:EditorPanes 2026-06-08-20:12:
         Missing sidebar link flags should follow the bundled editor default so new macOS code-server launches start from Ghostex-owned Dark 2026 settings instead of resurrecting local VS Code settings.
         */
        linkVscodeUserConfig: command.linkVscodeUserConfig ?? false,
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
        "level": "error",
        "projectId": command.projectId ?? NSNull(),
      ])
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      /**
       CDXC:EditorPanes 2026-06-06-23:50:
       VS Code server launch failures should surface immediately in the app as a
       toast and project-editor error, while the support log records the same
       failure as an error-level diagnostic after privacy sanitization.
      */
      let failureMessage = sanitizedError.isEmpty ? "Unknown startup error." : sanitizedError
      (window?.contentView as? ghostexRootView)?.postHostEvent(
        .codeServerRuntimeStartFailed(projectId: command.projectId, message: failureMessage))
      Self.logger.error("Failed to start code-server runtime: \(sanitizedError)")
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

  @MainActor private func handleOSIntegrationURL(_ url: URL) {
    guard url.scheme?.lowercased() == "ghostex" else {
      return
    }
    let action = (url.host ?? "").lowercased()
    let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
    let value: (String) -> String? = { name in
      items.first { $0.name == name }?.value
    }
    if action == "terminal" {
      var payload: [String: Any] = [:]
      if let command = value("command") {
        payload["command"] = command
      }
      if let cwd = value("cwd") {
        payload["cwd"] = cwd
      }
      if let title = value("title") {
        payload["title"] = title
      }
      dispatchOSIntegrationCommand(action: "createQuickTerminal", payload: payload)
      return
    }
    if action == "open" || action == "edit" {
      let path = value("path") ?? value("file")
      guard let path, !path.isEmpty else {
        return
      }
      var target: [String: Any] = ["path": path, "raw": path]
      if let line = value("line").flatMap(Int.init) {
        target["line"] = line
      }
      if let column = value("column").flatMap(Int.init) {
        target["column"] = column
      }
      dispatchOSIntegrationCommand(
        action: "openPaths",
        payload: ["mode": action == "edit" ? "edit" : "open", "targets": [target]])
    }
  }

  @MainActor private func dispatchOSIntegrationFileOpenPaths(_ paths: [String]) {
    /**
     CDXC:OSIntegration 2026-05-29-18:44:
     Finder Open With and `open -a Ghostex file.md` can arrive through either
     AppKit document delegate: `openFiles` string paths or modern `open urls`
     file URLs. Route both through one helper so markdown/text documents reach
     the same sidebar open-request router instead of file URLs being ignored as
     non-ghostex schemes.
     */
    let editPaths = paths.filter { !presentScriptOpenDialogIfNeeded(path: $0) }
    if !editPaths.isEmpty {
      dispatchOSIntegrationCommand(
        action: "openPaths",
        payload: [
          "mode": "open",
          "targets": editPaths.map { ["path": $0, "raw": $0] },
        ])
    }
  }

  @MainActor private func presentScriptOpenDialogIfNeeded(path: String) -> Bool {
    let url = URL(fileURLWithPath: path)
    guard ["command", "tool", "sh"].contains(url.pathExtension.lowercased()) else {
      return false
    }
    /**
     CDXC:OSIntegration 2026-05-27-18:06:
     Opening .command, .tool, or .sh files through Launch Services must never
     execute immediately. Ghostex asks whether to Run in a Quick terminal, Edit
     through the normal path classifier, or Cancel.
     */
    let alert = NSAlert()
    alert.messageText = "Open Script"
    alert.informativeText = path
    alert.addButton(withTitle: "Run")
    alert.addButton(withTitle: "Edit")
    alert.addButton(withTitle: "Cancel")
    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
      dispatchOSIntegrationCommand(
        action: "createQuickTerminal",
        payload: [
          "command": scriptRunCommand(path: path),
          "cwd": url.deletingLastPathComponent().path,
          "title": url.lastPathComponent,
        ])
      return true
    }
    if response == .alertSecondButtonReturn {
      dispatchOSIntegrationCommand(
        action: "openPaths",
        payload: ["mode": "edit", "targets": [["path": path, "raw": path]]])
      return true
    }
    return true
  }

  private func scriptRunCommand(path: String) -> String {
    let url = URL(fileURLWithPath: path)
    let attributes = (try? FileManager.default.attributesOfItem(atPath: path)) ?? [:]
    let permissions = (attributes[.posixPermissions] as? NSNumber)?.intValue ?? 0
    let executable = permissions & 0o111 != 0
    if executable {
      return "./\(Self.shellQuote(url.lastPathComponent))"
    }
    let shell = ProcessInfo.processInfo.environment["SHELL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedShell = shell?.isEmpty == false ? shell! : "/bin/zsh"
    return "\(Self.shellQuote(resolvedShell)) \(Self.shellQuote(path))"
  }

  private static func shellQuote(_ value: String) -> String {
    return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
  }

  @MainActor private func setOSIntegrationDefaults(_ command: SetOSIntegrationDefaults) {
    /**
     CDXC:OSIntegration 2026-05-27-18:06:
     Default editor, terminal-link, and script-runner ownership is opt-in from
     Settings. Registration makes Ghostex available in Open With; this method is
     the explicit user action that mutates Launch Services defaults.
     */
    guard let bundleIdentifier = Bundle.main.bundleIdentifier else {
      showMessage(.init(level: .error, message: "Ghostex bundle identifier is missing."))
      return
    }
    let target = command.target
    var failures: [String] = []
    failures.append(contentsOf: Self.osIntegrationDefaultFailures(target: target, bundleIdentifier: bundleIdentifier))
    if failures.isEmpty {
      (window?.contentView as? ghostexRootView)?.presentAppToast(
        level: "success",
        title: "Updated macOS OS Integration defaults."
      )
    } else {
      showMessage(.init(level: .error, message: "Could not set defaults: \(failures.joined(separator: ", "))"))
    }
    sendOSIntegrationStatus()
  }

  @MainActor private func sendOSIntegrationStatus() {
    /**
     CDXC:OSIntegration 2026-05-27-18:06:
     Settings -> OS Integration must show both availability and current
     Launch Services defaults. Native owns these diagnostics because React
     cannot reliably inspect Info.plist registrations or LS default handlers
     from a sandboxed webview.
     */
    let bundleIdentifier = Bundle.main.bundleIdentifier ?? ""
    let event = Self.osIntegrationStatusEvent(bundleIdentifier: bundleIdentifier)
    bridge?.send(event)
    (window?.contentView as? ghostexRootView)?.postHostEvent(event)
  }

  fileprivate static func osIntegrationDefaultFailures(
    target: String,
    bundleIdentifier: String
  ) -> [String] {
    var failures: [String] = []
    if target == "editor" || target == "all" {
      failures.append(contentsOf: setDefaultEditorHandlers(bundleIdentifier: bundleIdentifier))
    }
    if target == "terminalLinks" || target == "all" {
      let status = LSSetDefaultHandlerForURLScheme("ghostex" as CFString, bundleIdentifier as CFString)
      if status != noErr {
        failures.append("ghostex:// (\(status))")
      }
    }
    if target == "scriptRunner" || target == "all" {
      failures.append(contentsOf: setDefaultScriptHandlers(bundleIdentifier: bundleIdentifier))
    }
    return failures
  }

  fileprivate static func setDefaultEditorHandlers(bundleIdentifier: String) -> [String] {
    return ghostexOSIntegrationEditorExtensions.compactMap { fileExtension in
      guard let contentType = UTType(filenameExtension: fileExtension) else {
        return fileExtension
      }
      let status = LSSetDefaultRoleHandlerForContentType(
        contentType.identifier as CFString,
        LSRolesMask.editor,
        bundleIdentifier as CFString)
      return status == noErr ? nil : "\(fileExtension) (\(status))"
    }
  }

  fileprivate static func setDefaultScriptHandlers(bundleIdentifier: String) -> [String] {
    return ghostexOSIntegrationScriptExtensions.compactMap { fileExtension in
      guard let contentType = UTType(filenameExtension: fileExtension) else {
        return fileExtension
      }
      let status = LSSetDefaultRoleHandlerForContentType(
        contentType.identifier as CFString,
        LSRolesMask.shell,
        bundleIdentifier as CFString)
      return status == noErr ? nil : "\(fileExtension) (\(status))"
    }
  }

  fileprivate static func osIntegrationStatusPayload(bundleIdentifier: String) -> [String: Any] {
    let info = Bundle.main.infoDictionary ?? [:]
    let documentTypes = info["CFBundleDocumentTypes"] as? [[String: Any]] ?? []
    let urlTypes = info["CFBundleURLTypes"] as? [[String: Any]] ?? []
    let hasEditableRegistration = documentTypes.contains { type in
      (type["CFBundleTypeRole"] as? String) == "Editor"
        && ((type["CFBundleTypeExtensions"] as? [String])?.contains("*") == true
          || ((type["LSItemContentTypes"] as? [String])?.isEmpty == false))
    }
    let hasScriptRegistration = documentTypes.contains { type in
      (type["CFBundleTypeRole"] as? String) == "Shell"
        && ghostexOSIntegrationScriptExtensions.allSatisfy { fileExtension in
          (type["CFBundleTypeExtensions"] as? [String])?.contains(fileExtension) == true
        }
    }
    let hasGhostexURLRegistration = urlTypes.contains { type in
      (type["CFBundleURLSchemes"] as? [String])?.contains("ghostex") == true
    }
    let terminalLinkDefaultBundleId =
      LSCopyDefaultHandlerForURLScheme("ghostex" as CFString)?.takeRetainedValue() as String?
    return [
      "bundleIdentifier": bundleIdentifier,
      "editorDefaults": defaultRoleHandlers(
        extensions: ["txt", "md", "json", "js", "ts", "sh"],
        role: LSRolesMask.editor),
      "generatedAt": ISO8601DateFormatter().string(from: Date()),
      "registeredEditableFiles": hasEditableRegistration,
      "registeredGhostexURLScheme": hasGhostexURLRegistration,
      "registeredScriptRunner": hasScriptRegistration,
      "scriptDefaults": defaultRoleHandlers(
        extensions: ghostexOSIntegrationScriptExtensions,
        role: LSRolesMask.shell),
      "terminalLinkDefaultBundleId": terminalLinkDefaultBundleId as Any,
      "type": "osIntegrationStatus",
    ]
  }

  fileprivate static func osIntegrationStatusEvent(bundleIdentifier: String) -> HostEvent {
    return .osIntegrationStatus(
      payloadJson: jsonObjectString(osIntegrationStatusPayload(bundleIdentifier: bundleIdentifier)))
  }

  fileprivate static func defaultRoleHandlers(
    extensions: [String],
    role: LSRolesMask
  ) -> [String: String] {
    var handlers: [String: String] = [:]
    for fileExtension in extensions {
      guard let contentType = UTType(filenameExtension: fileExtension) else {
        continue
      }
      if let handler = LSCopyDefaultRoleHandlerForContentType(
        contentType.identifier as CFString,
        role
      )?.takeRetainedValue() as String? {
        handlers[fileExtension] = handler
      }
    }
    return handlers
  }

  @MainActor private func dispatchOSIntegrationCommand(action: String, payload: [String: Any]) {
    guard
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let payloadJson = String(data: data, encoding: .utf8)
    else {
      showMessage(.init(level: .error, message: "Could not encode OS Integration request."))
      return
    }
    dispatchOSIntegrationCommand(action: action, payloadJson: payloadJson)
  }

  @MainActor private func dispatchOSIntegrationCommand(action: String, payloadJson: String) {
    guard let sidebarView = (window?.contentView as? ghostexRootView)?.sidebarWebView,
      let actionJson = Self.javascriptStringLiteral(action),
      let payloadJsonLiteral = Self.javascriptStringLiteral(payloadJson)
    else {
      pendingOSIntegrationCommands.append((action: action, payloadJson: payloadJson))
      return
    }
    let script = """
      (async () => {
        const handler = window.__ghostex_NATIVE_CLI__;
        if (!handler || typeof handler.handleCommand !== 'function') {
          return 'sidebar-cli-handler-missing';
        }
        return JSON.stringify(await handler.handleCommand(\(actionJson), JSON.parse(\(payloadJsonLiteral))));
      })();
      """
    sidebarView.evaluateJavaScript(script) { [weak self] result, error in
      if let error {
        let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
        Self.logger.error("OS Integration sidebar dispatch failed: \(sanitizedError, privacy: .public)")
        self?.pendingOSIntegrationCommands.append((action: action, payloadJson: payloadJson))
        return
      }
      if let text = result as? String, text.contains(#""ok":false"#) {
        let sanitizedText = NativeLogPrivacy.sanitizeLogLine(text)
        Self.logger.error("OS Integration sidebar command failed: \(sanitizedText, privacy: .public)")
      }
    }
  }

  @MainActor private func flushPendingOSIntegrationCommands() {
    guard !pendingOSIntegrationCommands.isEmpty else {
      return
    }
    let pending = pendingOSIntegrationCommands
    pendingOSIntegrationCommands.removeAll()
    for command in pending {
      dispatchOSIntegrationCommand(action: command.action, payloadJson: command.payloadJson)
    }
  }

  @MainActor private func scheduleOSIntegrationFlushRetry() {
    for delay in [0.5, 1.5, 3.0, 6.0] {
      DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
        MainActor.assumeIsolated {
          self?.flushPendingOSIntegrationCommands()
        }
      }
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

  fileprivate static func jsonObjectString(_ value: [String: Any]) -> String {
    guard JSONSerialization.isValidJSONObject(value),
      let data = try? JSONSerialization.data(withJSONObject: value),
      let text = String(data: data, encoding: .utf8)
    else {
      return #"{"error":"json-encoding-failed"}"#
    }
    return text
  }

  private func syncGhosttyTerminalSettings(_ command: SyncGhosttyTerminalSettings) {
    /**
     CDXC:TerminalSettings 2026-04-26-19:02
     ghostex settings run in the native sidebar webview and must write the
     same Ghostty config file selected for embedded terminals. Keep the
     merge narrow so themes, keybinds, and unrelated Ghostty settings stay
     user-owned.

     CDXC:TerminalImagePaste 2026-06-08-13:32:
     Paste previewable images is a native runtime preference. Apply it during
     the same settings sync so the macOS clipboard handler changes immediately,
     but do not merge it into Ghostty config output for runtime-only syncs.
     */
    setTerminalPanePastePreviewableImagesEnabled(command.pastePreviewableImages ?? true)
    if command.runtimeOnly == true {
      return
    }
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to sync Ghostty terminal settings: \(sanitizedError)")
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to apply Ghostty config settings: \(sanitizedError)")
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
    /**
     CDXC:NativeIME 2026-06-13-02:32:
     New Ghostty installs write config.ghostty. Keep Ghostex-created configs on the same filename so future embedded-terminal keybinds load through the same path as Ghostty's own preferred loader.
     */
    return appSupport.appendingPathComponent("com.mitchellh.ghostty/config.ghostty")
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
    (window?.contentView as? ghostexRootView)?.presentAppToast(command)
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
     Project right-click "Open Folder" should reveal the actual stored
     workspace folder through the platform file viewer instead of routing through a URL opener or
     creating a fallback path when the project record is wrong.

     CDXC:WorkspaceActions 2026-06-04-13:39
     Keep the native reveal implementation while using Open Folder as the user-facing label so filesystem actions are OS-agnostic.
     */
    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path, isDirectory: true)])
  }

  @MainActor private func openWorkspaceInIde(_ command: OpenWorkspaceInIde) {
    let path = command.workspacePath.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !path.isEmpty else {
      return
    }

    /**
     CDXC:WorkspaceActions 2026-05-27-07:24
     Project right-click "Open in IDE" is an explicit command and must use the
     command target directly now that IDE attachment settings and overlay
     controllers are removed. Keep the command-line launcher so Zed, Zed
     Preview, VS Code, and Insiders retain their existing workspace behavior.
     */
    runOpenWorkspaceProcess(targetApp: command.targetApp, workspacePath: path)
  }

  private func runOpenWorkspaceProcess(targetApp: WorkspaceIdeTargetApp, workspacePath path: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = workspaceOpenCommandArguments(targetApp: targetApp, workspacePath: path)
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    do {
      try process.run()
    } catch {
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to open workspace in IDE: \(sanitizedError)")
    }
  }

  private func workspaceOpenCommandArguments(
    targetApp: WorkspaceIdeTargetApp,
    workspacePath: String
  ) -> [String] {
    switch targetApp {
    case .zed, .zedPreview:
      return ["zed", workspacePath, "--existing"]
    case .vscode:
      return ["code", workspacePath, "--reuse-window"]
    case .vscodeInsiders:
      return ["code-insiders", workspacePath, "--reuse-window"]
    }
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to open Ghostty config file: \(sanitizedError)")
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
      guard NativeProcessRegistry.shared.register(requestId: command.requestId, process: process) else {
        await MainActor.run {
          sendEvent(
            .processResult(
              requestId: command.requestId,
              exitCode: 130,
              stdout: "",
              stderr: "Process canceled."
            ))
        }
        return
      }
      let outputLock = NSLock()
      var stdoutData = Data()
      var stderrData = Data()
      let stdoutHandle = stdoutPipe.fileHandleForReading
      let stderrHandle = stderrPipe.fileHandleForReading
      /**
       CDXC:AgentsHub 2026-05-14-08:43
       Agents Hub process helpers can return real profile, skill, hook, and
       config data. Drain output while the command is running so stdout/stderr
       cannot fill the pipe and block the helper before native posts
       processResult back to the webview.

       CDXC:AgentsHub 2026-06-12-02:53
       The catalog helper now returns metadata only, while selected file
       contents use a separate smaller request. Keep pipe draining here because
       save/read helpers still return user-editable buffers and diagnostics.
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
        if NativeProcessRegistry.shared.isCanceled(requestId: command.requestId) {
          process.terminate()
        }
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
          NativeProcessRegistry.shared.unregister(requestId: command.requestId)
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
          NativeProcessRegistry.shared.unregister(requestId: command.requestId)
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

private struct NativeSidebarChromeSettings {
  let width: CGFloat?
  let projectEditorCompanionWidthRatio: CGFloat?
}

private struct NativeAppShotsSettings {
  let enabled: Bool
  let hotkey: String
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
    "createSession": "cmd+t",
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
    /**
     CDXC:SidebarCollapse 2026-06-12-02:23:
     Cmd+B toggles complete sidebar collapse from terminal focus. Sidebar side switching remains an available command, but it is unset by default so it cannot move the sidebar when users expect collapse.
     */
    "moveSidebar": "",
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
    "openBrowserPane": "cmd+n",
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
    /**
     CDXC:Hotkeys 2026-06-07-14:24:
     Terminal-focused AppKit dispatch must use the same default hotkey table as
     the shared sidebar model. Cmd+T creates a terminal tab, Cmd+N creates a
     browser tab, and Option+1..4 switch Agents, Source, Browser, and Kanban
     without depending on the sidebar WebKit DOM receiving the keydown.
     */
    "switchAgentsView": "alt+1",
    "switchSourceView": "alt+2",
    "switchGitHubView": "alt+3",
    "switchKanbanView": "alt+4",
    "toggleSidebarCollapsed": "cmd+b",
  ]
  fileprivate static let defaultHotkeyAliases: [String: [String]] = [
    "focusNextSession": ["cmd+shift+]"],
    "focusPreviousSession": ["cmd+shift+["],
  ]
  private static let retiredDefaultHotkeys: [String: [String]] = [
    "createSession": ["cmd+n"],
    "focusDown": ["cmd+down"],
    "focusLeft": ["cmd+left"],
    "focusNextGroup": ["cmd+shift+]"],
    "focusNextSession": ["cmd+]"],
    "focusPreviousGroup": ["cmd+shift+["],
    "focusPreviousSession": ["cmd+["],
    "focusRight": ["cmd+right"],
    "focusUp": ["cmd+up"],
    "moveSidebar": ["cmd+b"],
    "openBrowserPane": ["ctrl+shift+b"],
  ]
  private static let shiftedDigitHotkeyTextKeys: [String: String] = [
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
  private static let shiftedSymbolHotkeyTextKeys: [String: String] = [
    "{": "[",
    "}": "]",
  ]

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

  func readSidebarDefaultWidth() -> CGFloat? {
    /**
     CDXC:SidebarChrome 2026-06-05-04:40:
     The shared Settings file owns the sidebar handle reset target, but native
     startup must keep using settings.json sidebarWidth from readSidebarChrome.
     Read this value only for explicit resize-handle double-click resets.
     */
    guard let settings = readSharedSidebarSettingsDictionary() else {
      return nil
    }
    return Self.readCGFloat(settings["sidebarDefaultWidthPx"])
  }

  func readAppShotsSettings() -> NativeAppShotsSettings {
    guard let settings = readSharedSidebarSettingsDictionary() else {
      return NativeAppShotsSettings(enabled: true, hotkey: "both-command")
    }
    let enabled = settings["appShotsEnabled"] as? Bool ?? true
    let rawHotkey = settings["appShotsHotkey"] as? String
    let hotkey =
      rawHotkey == "double-left-shift" || rawHotkey == "double-left-option"
      ? rawHotkey!
      : "both-command"
    return NativeAppShotsSettings(enabled: enabled, hotkey: hotkey)
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to persist sidebar width: \(sanitizedError)")
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error(
        "Failed to persist project editor companion width ratio: \(sanitizedError)")
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
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error("Failed to persist main window chrome: \(sanitizedError)")
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
      .split(separator: " ")
      .map { normalizeHotkeyChordText(String($0)) }
      .joined(separator: " ")
  }

  private static func normalizeHotkeyChordText(_ chord: String) -> String {
    var parts = chord.split(separator: "+").map(String.init).filter { !$0.isEmpty }
    guard let key = parts.last else {
      return chord
    }
    if parts.contains("shift"), let unshiftedDigit = shiftedDigitHotkeyTextKeys[key] {
      parts[parts.count - 1] = unshiftedDigit
    }
    if parts.contains("shift"), let unshiftedSymbol = shiftedSymbolHotkeyTextKeys[key] {
      parts[parts.count - 1] = unshiftedSymbol
    }
    return parts.joined(separator: "+")
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

final class TitlebarChromeWebView: WKWebView {
  override var mouseDownCanMoveWindow: Bool {
    /*
     CDXC:ReactTitlebar 2026-06-11-22:10:
     Titlebar controls live inside the full-size content titlebar strip, where
     AppKit may otherwise treat mouse-down targets as draggable window chrome.
     The visible titlebar WKWebView owns actual controls, so it must never opt
     into window dragging; blank strip dragging is handled by ReactTitlebarChromeView.
     */
    false
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    /*
     CDXC:ReactTitlebar 2026-06-11-22:10:
     A titlebar control click should activate Ghostex and press the control in
     the same gesture, matching the native sidebar's behavior.
     */
    true
  }
}

final class SidebarWebView: WKWebView {
  var resizeHitExclusionSide: SidebarSide = .left
  var resizeHitExclusionWidth: CGFloat = 0
  var onNativePointerInsideChanged: ((Bool) -> Void)?
  private var nativePointerInside: Bool?
  private var nativePointerTrackingArea: NSTrackingArea?

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let nativePointerTrackingArea {
      removeTrackingArea(nativePointerTrackingArea)
    }
    /*
     CDXC:SidebarHover 2026-06-10-23:44:
     Sidebar hover state must follow AppKit's effective WebView boundary, not only
     WebKit's last mouse target. Track mouse movement in native code so crossing
     into the resize-excluded strip or leaving the sidebar can invalidate stale
     CSS :hover before delayed tooltips open.
     */
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeAlways, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
      owner: self,
      userInfo: nil
    )
    nativePointerTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func mouseEntered(with event: NSEvent) {
    updateNativePointerInside(for: event)
    super.mouseEntered(with: event)
  }

  override func mouseMoved(with event: NSEvent) {
    updateNativePointerInside(for: event)
    super.mouseMoved(with: event)
  }

  override func mouseExited(with event: NSEvent) {
    setNativePointerInside(false)
    super.mouseExited(with: event)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      setNativePointerInside(false)
      return nil
    }
    /**
     CDXC:NativeSidebarChrome 2026-06-04-22:20:
     The sidebar webview visually paints under the native resize divider to
     remove the right-edge gap, but WebKit must not receive pointer events in
     that divider strip. Yield that band so the AppKit resize handle remains
     the single drag owner.
     */
    let excludedWidth = min(max(resizeHitExclusionWidth, 0), bounds.width)
    if excludedWidth > 0 {
      switch resizeHitExclusionSide {
      case .left:
        if point.x >= bounds.maxX - excludedWidth {
          setNativePointerInside(false)
          return nil
        }
      case .right:
        if point.x <= bounds.minX + excludedWidth {
          setNativePointerInside(false)
          return nil
        }
      }
    }
    setNativePointerInside(true)
    return super.hitTest(point)
  }

  private func updateNativePointerInside(for event: NSEvent) {
    setNativePointerInside(isInteractivePoint(convert(event.locationInWindow, from: nil)))
  }

  func containsInteractiveHitPoint(_ point: NSPoint) -> Bool {
    isInteractivePoint(point)
  }

  private func isInteractivePoint(_ point: NSPoint) -> Bool {
    guard bounds.contains(point) else {
      return false
    }
    let excludedWidth = min(max(resizeHitExclusionWidth, 0), bounds.width)
    guard excludedWidth > 0 else {
      return true
    }
    switch resizeHitExclusionSide {
    case .left:
      return point.x < bounds.maxX - excludedWidth
    case .right:
      return point.x > bounds.minX + excludedWidth
    }
  }

  private func setNativePointerInside(_ isInside: Bool) {
    guard nativePointerInside != isInside else {
      return
    }
    nativePointerInside = isInside
    onNativePointerInsideChanged?(isInside)
  }

  func forceNativePointerInside(_ isInside: Bool) {
    nativePointerInside = isInside
    onNativePointerInsideChanged?(isInside)
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

final class ghostexRootView: NSView {
  private static let logger = Logger(subsystem: "com.madda.ghostex.host", category: "webview")

  private struct RootLayoutFrames {
    var divider: CGRect
    var sidebar: CGRect
    var sidebarWorkareaBorder: CGRect
    var titlebarChrome: CGRect
    var workareaTitlebarBorder: CGRect
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
  /**
   CDXC:NativeWindowChrome 2026-05-30-06:23:
   The main work area needs a #252525 separator below the React titlebar
   without continuing above the sidebar. Use native non-interactive chrome lines
   so the horizontal titlebar divider starts at the workspace frame while the
   matching vertical divider tracks the sidebar/workarea boundary.

   CDXC:NativeWindowChrome 2026-05-30-06:51:
   The workarea separators should be 1px thick instead of the original 2px.

   CDXC:NativeWindowChrome 2026-05-30-07:35:
   The workarea separators should use #252525 instead of #2b2b2b so native
   chrome boundaries stay subtle against the darker titlebar and workspace.
   */
  private static let workareaSeparatorWidth: CGFloat = 1
  private static let workareaSeparatorColor = NSColor(
    srgbRed: 37.0 / 255.0,
    green: 37.0 / 255.0,
    blue: 37.0 / 255.0,
    alpha: 1.0)
  private static let defaultSidebarWidth: CGFloat = 235
  private static let sidebarResetWidth: CGFloat = 235
  private static let startupOverlayVisibleDuration: TimeInterval = 2.0
  private static let startupOverlayFadeDuration: TimeInterval = 1.0
  private static let startupOverlayIconOpacity: CGFloat = 0.14
  private static let startupOverlayIconSize: CGFloat = 132
  private static let floatingPromptEditorFrameDefaultsKey = "ghostex.floatingPromptEditor.frame.v1"
  private static let floatingPromptEditorPrewarmRequestId = "ghostex-floating-prompt-editor-prewarm"
  private static let floatingPromptEditorPrewarmDelay: TimeInterval = 0.75

  private static func promptEditorMonotonicMilliseconds() -> Int {
    Int((ProcessInfo.processInfo.systemUptime * 1000).rounded())
  }

  private static func javascriptStringLiteral(_ value: String) -> String? {
    guard let data = try? JSONEncoder().encode(value) else {
      return nil
    }
    return String(data: data, encoding: .utf8)
  }

  let workspaceView: TerminalWorkspaceView
  var sidebarWebView: WKWebView { sidebarView }
  private let sidebarView: SidebarWebView
  private let sidebarModalBackdropView = SidebarModalBackdropView(frame: .zero)
  private let titlebarChromeView: ReactTitlebarChromeView
  private let titlebarChromeWebView: WKWebView
  private let startupOverlayView = NSView(frame: .zero)
  private let startupOverlayIconView = NSImageView(frame: .zero)
  private let scriptBridge: SidebarScriptBridge
  private let sidebarCommandRouter = SidebarCommandRouter()
  private let divider: PaneResizeHandleView
  private let sidebarWorkareaBorderView = NonInteractiveChromeLineView()
  private let workareaTitlebarBorderView = NonInteractiveChromeLineView()
  private let eventEncoder = JSONEncoder()
  private let syncGhosttyTerminalSettings: (SyncGhosttyTerminalSettings) -> Void
  private let applyGhosttyConfigSettings: (ApplyGhosttyConfigSettings) -> Void
  private let openGhosttyConfigFile: () -> Void
  private let openAccessibilityPreferences: () -> Void
  private let openWorkspaceInFinder: (OpenWorkspaceInFinder) -> Void
  private let openWorkspaceInIde: (OpenWorkspaceInIde) -> Void
  private let setAppTitlebarTitle: (String?) -> Void
  private let setSessionStatusIndicators: (SetSessionStatusIndicators) -> Void
  private let setPetOverlayState: (SetPetOverlayState) -> Void
  private let showUpdateDialogFromTitlebar: () -> Void
  private let startGxserverFromTitlebar: () -> Void
  private let stopGxserverFromTitlebar: () -> Void
  private let restartGxserverFromTitlebar: () -> Void
  private let setGxserverAlwaysStartFromTitlebar: (Bool) -> Void
  private let sendHostEvent: (HostEvent) -> Void
  private let nativeSettingsStore = NativeSettingsStore()
  private var activeAppModalKind: String?
  private var appModalPresentationPending = false
  private var activeNativeAppModalKind: String?
  private var nativeAppModalWindowController: AppModalWindowController?
  private var nativeToastController: NativeAppToastController?
  private var sidebarWorkspaceFocusRequestId: UInt64 = 0
  private var floatingPromptEditorReturnFocusRequestId: UInt64 = 0
  private var appModalReturnFocusSessionId: String?
  private var latestModalHostSidebarState: [String: Any]?
  private var activeFloatingPromptEditor: ActiveFloatingPromptEditor?
  private var hasPrewarmedFloatingPromptEditor = false
  private var hasScheduledFloatingPromptEditorPrewarm = false
  private var isPrewarmingFloatingPromptEditor = false
  private var floatingPromptEditorPrewarmTempFileURL: URL?
  private var isFloatingPromptEditorActiveForUserInput: Bool {
    (activeFloatingPromptEditor != nil && !isPrewarmingFloatingPromptEditor)
      || activeAppModalKind == "floatingPromptEditor"
  }
  private var pendingHotkeyPrefix: String?
  private var pendingHotkeyPrefixExpiresAt: Date?
  private var t3CodeRuntimeProcess: Process?
  private var t3CodeRuntimeStartedAt: Date?
  private var t3RuntimeVisibleSessionCwd: String?
  private var t3RuntimeLivenessTimer: Timer?
  private var pendingT3RuntimeStartWorkItem: DispatchWorkItem?
  private var t3RuntimePaneStateGeneration: UInt64 = 0
  private var t3RuntimeAutoStartBackoffUntil: Date?
  private var codeServerRuntimeProcess: Process?
  private var codeServerRuntimeStartedAt: Date?
  private var titlebarOutsideClickMonitor: Any?
  private var titlebarBootstrapScriptSource: String?
  private var titlebarDropdownPanelController: TitlebarDropdownPanelController?
  private var latestReactTitlebarProjectStateJson: String?
  private var isTitlebarOverlayOpen = false
  private var lastWorkspaceInteractionShieldLogKey: String?
  private var sidebarContextMenuOpenCount = 0
  private lazy var sessionAttentionNotificationController =
    SessionAttentionNotificationController { [weak self] sessionId in
      self?.handleSessionAttentionNotificationClick(sessionId)
    }
  private var isSidebarCollapsed = false
  private var sidebarWidth: CGFloat
  private var sidebarSide: SidebarSide = .left
  private var lastSidebarFirstResponderIntentAt: Date?
  private static let sidebarFirstResponderIntentWindow: TimeInterval = 1.0

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
   CDXC:NativeSidebarChrome 2026-05-28-12:18:
   New sidebar sessions should now start at 235px, and double-clicking the native resize handle should snap back to the same 235px default.
   CDXC:SidebarChrome 2026-06-05-04:40:
   The Settings-owned sidebar default width now controls only explicit
   double-click resets. Startup continues restoring the last native sidebarWidth
   from settings.json so user-resized chrome survives normal restarts.
   */
  init(
    ghostty: GhostexGhosttyApp,
    defaultWorkspaceBackgroundColor: NSColor,
    gxserverBootstrap: [String: Any],
    initialUpdateAvailable: Bool,
    sendEvent: @escaping (HostEvent) -> Void,
    syncGhosttyTerminalSettings: @escaping (SyncGhosttyTerminalSettings) -> Void,
    applyGhosttyConfigSettings: @escaping (ApplyGhosttyConfigSettings) -> Void,
    openGhosttyConfigFile: @escaping () -> Void,
    openAccessibilityPreferences: @escaping () -> Void,
    openWorkspaceInFinder: @escaping (OpenWorkspaceInFinder) -> Void,
    openWorkspaceInIde: @escaping (OpenWorkspaceInIde) -> Void,
    setAppTitlebarTitle: @escaping (String?) -> Void,
    setSessionStatusIndicators: @escaping (SetSessionStatusIndicators) -> Void,
    setPetOverlayState: @escaping (SetPetOverlayState) -> Void,
    showUpdateDialogFromTitlebar: @escaping () -> Void,
    startGxserverFromTitlebar: @escaping () -> Void,
    stopGxserverFromTitlebar: @escaping () -> Void,
    restartGxserverFromTitlebar: @escaping () -> Void,
    setGxserverAlwaysStartFromTitlebar: @escaping (Bool) -> Void
  ) {
    let settingsStore = NativeSettingsStore()
    let storedSidebarChrome = settingsStore.readSidebarChrome()
    self.workspaceView = TerminalWorkspaceView(
      ghostty: ghostty,
      sendEvent: sendEvent,
      defaultWorkspaceBackgroundColor: defaultWorkspaceBackgroundColor,
      initialProjectEditorCompanionWidthRatio: storedSidebarChrome.projectEditorCompanionWidthRatio,
      persistProjectEditorCompanionWidthRatio: { widthRatio in
        settingsStore.persistProjectEditorCompanionWidthRatio(widthRatio)
      }
    )
    self.scriptBridge = SidebarScriptBridge(router: sidebarCommandRouter)
    self.syncGhosttyTerminalSettings = syncGhosttyTerminalSettings
    self.applyGhosttyConfigSettings = applyGhosttyConfigSettings
    self.openGhosttyConfigFile = openGhosttyConfigFile
    self.openAccessibilityPreferences = openAccessibilityPreferences
    self.openWorkspaceInFinder = openWorkspaceInFinder
    self.openWorkspaceInIde = openWorkspaceInIde
    self.setAppTitlebarTitle = setAppTitlebarTitle
    self.setSessionStatusIndicators = setSessionStatusIndicators
    self.setPetOverlayState = setPetOverlayState
    self.showUpdateDialogFromTitlebar = showUpdateDialogFromTitlebar
    self.startGxserverFromTitlebar = startGxserverFromTitlebar
    self.stopGxserverFromTitlebar = stopGxserverFromTitlebar
    self.restartGxserverFromTitlebar = restartGxserverFromTitlebar
    self.setGxserverAlwaysStartFromTitlebar = setGxserverAlwaysStartFromTitlebar
    self.sendHostEvent = sendEvent
    self.sidebarWidth = storedSidebarChrome.width ?? Self.defaultSidebarWidth
    self.sidebarSide = nativeSettingsStore.readSidebarSide()
    let configuration = WKWebViewConfiguration()
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHostDiagnostics")
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
      "bundleIdentifier": Bundle.main.bundleIdentifier ?? "",
      "cwd": cwd,
      "gxserver": gxserverBootstrap,
      "homeDir": FileManager.default.homeDirectoryForCurrentUser.path,
      "ghostexHomeDir": GhostexAppStorage.sharedRootDirectory.path,
      "sharedSidebarStorage": GhostexAppStorage.readSharedSidebarStorage(),
      "sidebarCollapsed": isSidebarCollapsed,
      "updateAvailable": initialUpdateAvailable,
      "workspaceName": workspaceName.isEmpty ? "Ghostex" : workspaceName,
    ]
    if let data = try? JSONSerialization.data(withJSONObject: bootstrap),
      let json = String(data: data, encoding: .utf8)
    {
      let bootstrapScriptSource = "window.__ghostex_NATIVE_HOST__ = \(json);"
      self.titlebarBootstrapScriptSource = bootstrapScriptSource
      let bootstrapScript = WKUserScript(
        source: bootstrapScriptSource,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      )
      /**
       CDXC:AccessibilityPermissions 2026-04-28-16:57
       Settings render in native child-window modal hosts, while the sidebar
       state lives in the sidebar webview. Keep the native Accessibility grant
       state in web bootstrap so settings can show a short disabled notice
       without asking the React layer to infer macOS privacy state.

       CDXC:CliInstall 2026-06-07-13:53:
       Include the bundle identifier in the native bootstrap so the sidebar's
       production-only CLI auto-linker does not let ghostex-dev local starts
       overwrite the user's public ghostex/gx command symlinks.

       CDXC:AutoUpdate 2026-06-08-18:21:
       Sparkle can detect an update before the titlebar WKWebView has loaded.
       Seed the native availability boolean into bootstrap so the initial React
       render shows the download button without waiting for the next 15-minute
       appcast probe.
       */
      configuration.userContentController.addUserScript(bootstrapScript)
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
    self.sidebarView = SidebarWebView(frame: .zero, configuration: configuration)
    self.titlebarChromeWebView = TitlebarChromeWebView(frame: .zero, configuration: titlebarConfiguration)
    self.titlebarChromeView = ReactTitlebarChromeView(webView: titlebarChromeWebView)
    self.divider = PaneResizeHandleView()
    super.init(frame: .zero)
    nativeToastController = NativeAppToastController { [weak self] sidebarMessage in
      self?.dispatchSidebarModalCommand(sidebarMessage)
    }
    workspaceView.sourceCEFDragOverlaySnapshotProvider = { [weak self] in
      self?.sourceCEFDragOverlaySnapshot() ?? [:]
    }
    workspaceView.setSidebarSide(sidebarSide)
    /*
     CDXC:ReactTitlebar 2026-06-11-13:22:
     Titlebar dropdowns now use native child windows and the titlebar webview is
     clipped to the fixed 35px strip, so the titlebar wrapper must not register
     as a workspace drag destination. Keeping it registered can make AppKit
     choose the titlebar layer during editor tab drags before CEF/WKWebView sees
     VS Code's drag stream.
     */
    workspaceView.onManagedT3PaneRuntimeStateChanged = { [weak self] state in
      self?.setT3CodeRuntimePaneState(state)
    }
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
    divider.onPointerEntered = { [weak self] in
      /*
       CDXC:SidebarHover 2026-06-11-10:23:
       Hovering the native sidebar resize rail is outside the React sidebar even
       though the rail sits over the sidebar webview edge. Force the sidebar
       hover gate off when AppKit routes pointer ownership to the divider so a
       previously hovered session row cannot remain highlighted while the user
       moves from the rail into the workspace.
       */
      self?.sidebarView.forceNativePointerInside(false)
    }
    sidebarView.onNativePointerInsideChanged = { [weak self] isInside in
      self?.setSidebarNativePointerInside(isInside)
    }

    wantsLayer = true
    layer?.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor.cgColor
    sidebarView.setValue(false, forKey: "drawsBackground")
    titlebarChromeWebView.setValue(false, forKey: "drawsBackground")
    sidebarModalBackdropView.isHidden = true
    sidebarView.navigationDelegate = self
    addSubview(workspaceView)
    /**
     CDXC:NativeWorkspaceChrome 2026-04-26-05:40
     Ghostty surfaces can keep native subviews/layers that draw and receive
     events aggressively. Add the terminal workspace behind the sidebar
     chrome so project/session controls always own their visible hit area.
    */
    addSubview(sidebarView)
    divider.separatorColor = Self.workareaSeparatorColor
    addSubview(divider)
    sidebarWorkareaBorderView.lineColor = Self.workareaSeparatorColor
    /**
     CDXC:NativeSidebarChrome 2026-06-08-19:58:
     The visible sidebar/workarea separator must be the same native view that owns resize dragging, the resize cursor, and the delayed hover affordance. Keep the older standalone border view hidden so the apparent drag bar cannot become a separate hover surface.
     */
    sidebarWorkareaBorderView.isHidden = true
    workareaTitlebarBorderView.lineColor = Self.workareaSeparatorColor
    addSubview(sidebarWorkareaBorderView)
    addSubview(workareaTitlebarBorderView)
    /*
     CDXC:AppModals 2026-06-11-23:07:
     No app-modal WKWebView may be mounted over the main workspace. The Source
     drag/drop harness proved that even hidden sibling WKWebViews above CEF/WK
     editor panes can prevent VS Code tab drag/drop from reaching browser-native
     drop targets, so all rich modal content now renders in native child windows.
     */
    addSubview(sidebarModalBackdropView)
    /**
     CDXC:ReactTitlebar 2026-05-12-09:58
     Titlebar controls are React-rendered in a transparent WKWebView.

     CDXC:ReactTitlebar 2026-06-11-13:22:
     Dropdown content now renders in native child windows, so the main titlebar
     WKWebView is clipped to the titlebar strip instead of using a full-window
     transparent portal surface above the workspace.
     */
    addSubview(titlebarChromeView)
    promoteSidebarChrome()
    installStartupOverlay()
    loadSidebar()
    loadTitlebarChrome()
    installTitlebarOutsideClickMonitor()
  }

  deinit {
    if let titlebarOutsideClickMonitor {
      NSEvent.removeMonitor(titlebarOutsideClickMonitor)
    }
    titlebarDropdownPanelController?.close()
    nativeAppModalWindowController?.close(sendReactClose: false)
    nativeToastController?.closeAll()
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
      let titlebarPoint = self.titlebarChromeView.convert(point, from: self)
      let titlebarContainsInteractiveRegion = self.titlebarChromeView.containsInteractiveHitRegion(titlebarPoint)
      if titlebarContainsInteractiveRegion {
        return event
      }
      /**
       CDXC:ReactTitlebar 2026-05-16-20:01:
       Clicking behind a titlebar dropdown lands in AppKit, not the React
       titlebar document. Close Resources, Actions, and Open menus from a native
       local mouse monitor before the original click continues to the sidebar or
       workspace target.

       CDXC:ReactTitlebar 2026-06-11-13:22:
       The close hook now dismisses the native child dropdown panel instead of
       toggling Radix menu state inside the titlebar WKWebView.
       */
      self.titlebarChromeView.closeOpenDropdowns()
      return event
    }
  }

  private func setSidebarNativePointerInside(_ isInside: Bool) {
    /*
     CDXC:SidebarHover 2026-06-10-23:44:
     WKWebView can keep a stale CSS :hover target after AppKit routes the pointer
     out through native sidebar chrome. Native owns the true sidebar boundary, so
     tell React when the pointer is outside and let the sidebar root ignore hover
     hit testing until native reports entry again.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.setNativePointerInside?.(\(isInside ? "true" : "false"));
      undefined;
      """)
  }

  func handleWindowMouseDownBeforeDispatch(_ event: NSEvent) {
    guard Self.isMouseDownEvent(event) else {
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    dismissSidebarContextMenuForOutsideClick(at: point)
    if isInsideInteractiveSidebarContent(point) {
      markSidebarFirstResponderIntent(reason: "mouseDown")
    }
  }

  func handleWindowFirstResponderChanged(_ responder: NSResponder?) {
    if restoreTerminalFocusAfterPassiveSidebarFirstResponder(responder) {
      return
    }
    workspaceView.windowFirstResponderChanged(responder, reason: "windowMakeFirstResponder")
  }

  private func isInsideInteractiveSidebarContent(_ pointInRoot: NSPoint) -> Bool {
    guard sidebarView.frame.contains(pointInRoot) else {
      return false
    }
    let pointInSidebar = sidebarView.convert(pointInRoot, from: self)
    return sidebarView.hitTest(pointInSidebar) != nil
  }

  private func markSidebarFirstResponderIntent(reason: String) {
    lastSidebarFirstResponderIntentAt = Date()
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.sidebarFirstResponderIntent",
      details: [
        "reason": reason,
      ])
  }

  private func restoreTerminalFocusAfterPassiveSidebarFirstResponder(_ responder: NSResponder?) -> Bool {
    guard isSidebarResponder(responder) else {
      return false
    }
    let now = Date()
    guard !hasRecentSidebarFirstResponderIntent(now: now) else {
      return false
    }
    guard activeAppModalKind == nil, activeNativeAppModalKind == nil, !appModalPresentationPending else {
      return false
    }
    if workspaceView.restoreProjectEditorFocusAfterPassiveSidebarFirstResponder(now: now) {
      /*
       CDXC:ProjectBoardFocus 2026-06-12-08:44:
       Passive sidebar WKWebView hydration must not convert active Project/Kanban typing into companion-terminal focus.
       Restore the recent project-editor first responder before considering terminal recovery so board text entry remains the focus owner during sidebar refresh churn.
       */
      return true
    }
    guard let restoreSessionId = workspaceView.passiveSidebarReturnFocusTerminalSessionId() else {
      return false
    }
    let intentAgeMs: Any = sidebarFirstResponderIntentAgeMs(now: now).map { $0 as Any } ?? NSNull()
    /*
     CDXC:NativeTerminalFocus 2026-06-08-09:30:
     gxserver presentation deltas can hydrate the sidebar WKWebView while the user is typing in a terminal. A passive WKWebView first-responder handoff must not take keyboard focus from the selected terminal; allow sidebar focus only after recent user input inside the sidebar, otherwise restore terminal first responder at the native boundary.

     CDXC:NativeTerminalFocus 2026-06-09-23:14:
     Passive sidebar recovery is not modal return-focus. Use the terminal first-responder target instead of app-modal focus priority so a stale commandsPanelFocusedSessionId cannot steal focus when the user did not click that command panel.
     */
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.passiveSidebarFirstResponderRestored",
      details: [
        "intentAgeMs": intentAgeMs,
        "rootModalHostMounted": false,
        "requestedSessionId": restoreSessionId,
        "responder": responder.map { String(describing: type(of: $0)) } ?? "nil",
      ])
    workspaceView.focusTerminal(sessionId: restoreSessionId, reason: "passiveSidebarFirstResponderRestore")
    return true
  }

  private func isSidebarResponder(_ responder: NSResponder?) -> Bool {
    guard let responderView = responder as? NSView else {
      return false
    }
    return responderView === sidebarView || responderView.isDescendant(of: sidebarView)
  }

  private func hasRecentSidebarFirstResponderIntent(now: Date) -> Bool {
    guard let lastSidebarFirstResponderIntentAt else {
      return false
    }
    return now.timeIntervalSince(lastSidebarFirstResponderIntentAt) <= Self.sidebarFirstResponderIntentWindow
  }

  private func sidebarFirstResponderIntentAgeMs(now: Date) -> Int? {
    guard let lastSidebarFirstResponderIntentAt else {
      return nil
    }
    return max(0, Int(now.timeIntervalSince(lastSidebarFirstResponderIntentAt) * 1000))
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

  func scheduleFloatingPromptEditorPrewarmAfterLaunch() {
    guard !hasScheduledFloatingPromptEditorPrewarm else {
      return
    }
    hasScheduledFloatingPromptEditorPrewarm = true
    /**
     CDXC:PromptEditor 2026-06-12-04:37:
     The first Ctrl+G prompt editor open should not pay the full native
     child-window, WKWebView, React, and Monaco cold-start cost. Schedule a
     real hidden prompt-editor native window after the main window is visible
     so startup chrome can settle before WebKit prewarm work begins.
     */
    DispatchQueue.main.asyncAfter(deadline: .now() + Self.floatingPromptEditorPrewarmDelay) {
      [weak self] in
      self?.prewarmFloatingPromptEditorIfNeeded()
    }
  }

  func closeTerminal(sessionId: String, preservePersistenceSession: Bool = false) {
    if activeFloatingPromptEditor?.originatingSessionId == sessionId {
      /**
       CDXC:PromptEditor 2026-05-13-09:48
       Closing the terminal that launched Ctrl+G prompt editing should close
       the floating prompt editor and persist the current Monaco buffer first.
       Ask the native prompt-editor window for its live text instead of marking
       the status cancelled, because the source terminal going away is not a
       user discard action.
       */
      dispatchFloatingPromptEditorHostMessage([
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
      promoteFloatingPromptEditorPrewarmToUserOpen()
    }
    let openStartedAtMs = Self.promptEditorMonotonicMilliseconds()
    let requestId = command.requestId ?? "floating-monaco-editor-\(UUID().uuidString)"
    guard let filePath = command.filePath?.trimmingCharacters(in: .whitespacesAndNewlines),
      !filePath.isEmpty
    else {
      writeFloatingPromptEditorStatusFile(command.statusFile, status: "cancelled")
      return
    }
    let initialText = (try? String(contentsOfFile: filePath, encoding: .utf8)) ?? ""
    let language = "markdown"
    let originatingSessionId = ghostexNativeFocusSessionId(from: command.originatingSessionId)
    if let activeFloatingPromptEditor {
      writeFloatingPromptEditorStatusFile(activeFloatingPromptEditor.statusFile, status: "cancelled")
    }
    activeFloatingPromptEditor = ActiveFloatingPromptEditor(
      filePath: filePath,
      originatingSessionId: originatingSessionId,
      requestId: requestId,
      statusFile: command.statusFile
    )
    /**
     CDXC:PromptEditor 2026-05-13-09:48:
     Native owns reading the requested temp file, status writes, and final
     save/cancel semantics so the CLI bridge contract remains independent from
     React rendering.

     CDXC:PromptEditor 2026-05-13-10:22:
     Ctrl+G prompt editing is always Markdown and opens as a narrow wrapped
     writing pane. Ignore caller language hints so the React prompt editor
     consistently uses Markdown tokenization and text wrapping for prompt
     composition.

     CDXC:PromptEditor 2026-06-11-22:51:
     The rich prompt editor must not float inside the full-workspace overlay.
     Open the existing React/Monaco component in a native child window so
     AppKit owns movement, resizing, focus, and hit testing without placing a
     transparent web layer above the workspace.
     */
    let initialFrame = floatingPromptEditorInitialFrame(originatingSessionId: originatingSessionId)
    PromptEditorDebugLog.append(
      event: "native.open",
      details: [
        "initialTextLength": initialText.count,
        "interruptedPrewarm": interruptedPrewarm,
        "nativeWindow": true,
        "openStartedAtMs": openStartedAtMs,
        "requestId": requestId,
        "rootModalHostMounted": false,
        "startupOverlayVisible": startupOverlayView.superview === self && startupOverlayView.alphaValue > 0,
      ]
    )
    let openMessage: [String: Any] = [
      "filePath": filePath,
      "initialFrame": initialFrame,
      "initialText": initialText,
      "language": language,
      "modal": "floatingPromptEditor",
      "nativeOpenStartedAtMs": openStartedAtMs,
      "requestId": requestId,
      "statusFile": command.statusFile ?? "",
      "title": command.title?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        ? command.title!
        : "Prompt Editor",
      "type": "open",
    ]
    let opened = openNativeAppModalWindow(
      message: openMessage,
      modal: "floatingPromptEditor",
      preferredContentFrame: floatingPromptEditorScreenContentFrame(fromTopLeftFrame: initialFrame))
    if !opened {
      writeFloatingPromptEditorStatusFile(command.statusFile, status: "cancelled")
      activeFloatingPromptEditor = nil
    }
  }

  /**
   CDXC:PromptEditor 2026-06-11-22:51:
   The rich prompt editor now opens in its own native child WKWebView instead
   of the full-window modal overlay. Do not create a hidden prompt-editor
   session in the overlay for prewarm; that would keep the old overlay path
   alive without warming the child-window editor instance that users interact
   with.

   CDXC:PromptEditor 2026-06-12-04:37:
   Prewarm the actual native child-window prompt host instead of the removed
   overlay. Keep the window hidden, load the same React modal host and Monaco
   path used by Ctrl+G, and leave the reusable WKWebView available for the
   first real prompt open.
   */
  private func prewarmFloatingPromptEditorIfNeeded() {
    guard !hasPrewarmedFloatingPromptEditor, !isPrewarmingFloatingPromptEditor else {
      PromptEditorDebugLog.append(
        event: "native.prewarm.skipped",
        details: [
          "hasPrewarmed": hasPrewarmedFloatingPromptEditor,
          "isPrewarming": isPrewarmingFloatingPromptEditor,
          "reason": "alreadyStarted",
          "requestId": Self.floatingPromptEditorPrewarmRequestId,
        ])
      return
    }
    guard activeFloatingPromptEditor == nil,
      activeNativeAppModalKind == nil,
      !appModalPresentationPending
    else {
      PromptEditorDebugLog.append(
        event: "native.prewarm.skipped",
        details: [
          "activeNativeAppModalKind": activeNativeAppModalKind ?? "",
          "appModalPresentationPending": appModalPresentationPending,
          "hasActiveFloatingPromptEditor": activeFloatingPromptEditor != nil,
          "reason": "modalBusy",
          "requestId": Self.floatingPromptEditorPrewarmRequestId,
        ])
      return
    }
    guard window != nil else {
      PromptEditorDebugLog.append(
        event: "native.prewarm.skipped",
        details: [
          "reason": "missingWindow",
          "requestId": Self.floatingPromptEditorPrewarmRequestId,
        ])
      return
    }

    let tempDirectory = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-floating-prompt-editor-prewarm-\(UUID().uuidString)", isDirectory: true)
    let tempFileURL = tempDirectory.appendingPathComponent("prompt.md", isDirectory: false)
    do {
      try FileManager.default.createDirectory(at: tempDirectory, withIntermediateDirectories: true)
      try "".write(to: tempFileURL, atomically: true, encoding: .utf8)
    } catch {
      PromptEditorDebugLog.append(
        event: "native.prewarm.skipped",
        details: [
          "error": error.localizedDescription,
          "reason": "tempFileFailed",
          "requestId": Self.floatingPromptEditorPrewarmRequestId,
        ])
      return
    }

    floatingPromptEditorPrewarmTempFileURL = tempFileURL
    isPrewarmingFloatingPromptEditor = true
    activeFloatingPromptEditor = ActiveFloatingPromptEditor(
      filePath: tempFileURL.path,
      originatingSessionId: nil,
      requestId: Self.floatingPromptEditorPrewarmRequestId,
      statusFile: nil
    )
    let prewarmStartedAtMs = Self.promptEditorMonotonicMilliseconds()
    let initialFrame = floatingPromptEditorInitialFrame(originatingSessionId: nil)
    PromptEditorDebugLog.append(
      event: "native.prewarm.start",
      details: [
        "nativeWindow": true,
        "prewarmStartedAtMs": prewarmStartedAtMs,
        "requestId": Self.floatingPromptEditorPrewarmRequestId,
      ])
    let openMessage: [String: Any] = [
      "filePath": tempFileURL.path,
      "initialFrame": initialFrame,
      "initialText": "",
      "language": "markdown",
      "modal": "floatingPromptEditor",
      "nativeOpenStartedAtMs": prewarmStartedAtMs,
      "prewarm": true,
      "requestId": Self.floatingPromptEditorPrewarmRequestId,
      "statusFile": "",
      "title": "Prompt Editor",
      "type": "open",
    ]
    let opened = openNativeAppModalWindow(
      message: openMessage,
      modal: "floatingPromptEditor",
      preferredContentFrame: floatingPromptEditorScreenContentFrame(fromTopLeftFrame: initialFrame))
    if !opened {
      PromptEditorDebugLog.append(
        event: "native.prewarm.failed",
        details: [
          "reason": "openNativeWindowFailed",
          "requestId": Self.floatingPromptEditorPrewarmRequestId,
        ])
      cleanupFloatingPromptEditorPrewarmTempFile()
      isPrewarmingFloatingPromptEditor = false
      activeFloatingPromptEditor = nil
    }
  }

  private func promoteFloatingPromptEditorPrewarmToUserOpen() {
    guard isPrewarmingFloatingPromptEditor else {
      return
    }
    /**
     CDXC:PromptEditor 2026-06-12-04:37:
     A user Ctrl+G can arrive while startup prewarm is still loading. Promote
     the in-flight native prompt host instead of closing it, so the first real
     open can reuse whatever WebKit/React/Monaco work has already completed.
     */
    PromptEditorDebugLog.append(
      event: "native.prewarm.promote",
      details: [
        "requestId": Self.floatingPromptEditorPrewarmRequestId,
      ])
    isPrewarmingFloatingPromptEditor = false
    activeFloatingPromptEditor = nil
    cleanupFloatingPromptEditorPrewarmTempFile()
  }

  private func cancelFloatingPromptEditorPrewarm(reason: String) {
    guard isPrewarmingFloatingPromptEditor else {
      return
    }
    PromptEditorDebugLog.append(
      event: "native.prewarm.cancel",
      details: [
        "reason": reason,
        "requestId": Self.floatingPromptEditorPrewarmRequestId,
      ])
    isPrewarmingFloatingPromptEditor = false
    activeFloatingPromptEditor = nil
    appModalPresentationPending = false
    cleanupFloatingPromptEditorPrewarmTempFile()
  }

  private func finishFloatingPromptEditorPrewarm() {
    guard isPrewarmingFloatingPromptEditor else {
      return
    }
    PromptEditorDebugLog.append(
      event: "native.prewarm.finish",
      details: [
        "rootModalHostMounted": false,
        "requestId": Self.floatingPromptEditorPrewarmRequestId,
      ]
    )
    hasPrewarmedFloatingPromptEditor = true
    isPrewarmingFloatingPromptEditor = false
    activeFloatingPromptEditor = nil
    appModalPresentationPending = false
    nativeAppModalWindowController?.hideReusablePromptEditor(sendReactClose: true)
    cleanupFloatingPromptEditorPrewarmTempFile()
    updateSidebarModalBackdrop()
  }

  private func cleanupFloatingPromptEditorPrewarmTempFile() {
    guard let tempURL = floatingPromptEditorPrewarmTempFileURL else {
      return
    }
    try? FileManager.default.removeItem(at: tempURL.deletingLastPathComponent())
    floatingPromptEditorPrewarmTempFileURL = nil
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
    /*
     CDXC:PromptEditor 2026-06-11-23:06:
     Native prompt-editor windows can be resized by AppKit edge drags, so
     persistence should preserve the user's chosen width across restarts rather
     than shrinking it back to the old 700px overlay maximum.
     */
    let maxWidth = availableWidth
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

  private func floatingPromptEditorScreenContentFrame(
    fromTopLeftFrame frame: [String: CGFloat]
  ) -> CGRect? {
    guard let window,
      let left = frame["left"],
      let top = frame["top"],
      let width = frame["width"],
      let height = frame["height"]
    else {
      return nil
    }
    let rootFrame = CGRect(
      x: left,
      y: bounds.height - top - height,
      width: width,
      height: height)
    return window.convertToScreen(convert(rootFrame, to: nil))
  }

  private func persistFloatingPromptEditorContentScreenFrame(_ contentScreenFrame: CGRect) {
    guard let window else {
      return
    }
    let windowFrame = window.convertFromScreen(contentScreenFrame)
    let rootFrame = convert(windowFrame, from: nil)
    persistFloatingPromptEditorFrame([
      "height": rootFrame.height,
      "left": rootFrame.minX,
      "top": bounds.height - rootFrame.maxY,
      "width": rootFrame.width,
    ])
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
      return
    }
    /*
     CDXC:PromptEditor 2026-06-11-23:07:
     Prompt-editor hit regions are no longer applied to a transparent root
     WKWebView. The native child window owns its AppKit frame, so these bridge
     messages are retained only for frame persistence and diagnostics.
     */
    PromptEditorDebugLog.append(
      event: "native.hitRegion.rootHostSkipped",
      details: [
        "height": height,
        "imagePreviewOpen": imagePreviewOpen,
        "left": left,
        "requestId": activeFloatingPromptEditor?.requestId ?? "",
        "rootModalHostMounted": false,
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
      dispatchFloatingPromptEditorHostMessage([
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
      dispatchFloatingPromptEditorHostMessage([
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
      dispatchFloatingPromptEditorHostMessage([
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
      dispatchFloatingPromptEditorHostMessage([
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

  private func finishFloatingPromptEditor(reason: String, closeNativeWindow: Bool = true) {
    let returnFocusSessionId = activeFloatingPromptEditor?.originatingSessionId
    let isNativePromptWindow =
      activeNativeAppModalKind == "floatingPromptEditor"
      || nativeAppModalWindowController?.currentModalKind == "floatingPromptEditor"
    PromptEditorDebugLog.append(
      event: "native.finish",
      details: [
        "nativeWindow": isNativePromptWindow,
        "reason": reason,
        "requestId": activeFloatingPromptEditor?.requestId ?? "",
        "returnFocusSessionId": returnFocusSessionId ?? "",
      ]
    )
    activeFloatingPromptEditor = nil
    if activeAppModalKind == "floatingPromptEditor" {
      activeAppModalKind = nil
    }
    if activeNativeAppModalKind == "floatingPromptEditor" {
      activeNativeAppModalKind = nil
    }
    appModalPresentationPending = false
    if isNativePromptWindow {
      if closeNativeWindow {
        nativeAppModalWindowController?.hideReusablePromptEditor(sendReactClose: true)
      }
    }
    updateSidebarModalBackdrop()
    if let returnFocusSessionId {
      restoreFloatingPromptEditorReturnFocus(sessionId: returnFocusSessionId, reason: reason)
    }
  }

  private func restoreFloatingPromptEditorReturnFocus(sessionId rawSessionId: String, reason: String) {
    /*
     CDXC:PromptEditor 2026-06-09-09:05:
     Saving or closing the Monaco rich prompt editor must return typing focus to the terminal that launched Ctrl+G. Clear the floating modal state first, then restore focus after the current WebKit bridge turn and reinforce once after WebKit close events settle so Ctrl+G, Cmd+S, and Save leave the source terminal ready for input.

     CDXC:PromptEditor 2026-06-09-21:50:
     Return-focus dispatch accepts gxserver S:P:G refs but native AppKit focus
     remains keyed by P:G. Normalize once before logging, direct focus, sidebar
     fallback, and delayed reinforcement.
     */
    let sessionId = ghostexNativeFocusSessionId(from: rawSessionId) ?? rawSessionId
    floatingPromptEditorReturnFocusRequestId &+= 1
    let focusRequestId = floatingPromptEditorReturnFocusRequestId
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.floatingPromptEditorReturnFocusQueued",
      details: [
        "focusRequestId": focusRequestId,
        "reason": reason,
        "responderBeforeQueue": responderSnapshot(),
        "sessionId": sessionId,
        "webChromeFirstResponder": isWebChromeFirstResponder(),
        "workspaceSnapshotBeforeQueue": workspaceView.activationDebugSnapshot(),
      ])
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        return
      }
      guard self.floatingPromptEditorReturnFocusRequestId == focusRequestId else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorReturnFocusSkipped",
          details: [
            "focusRequestId": focusRequestId,
            "latestFocusRequestId": self.floatingPromptEditorReturnFocusRequestId,
            "reason": reason,
            "sessionId": sessionId,
            "skipReason": "staleFocusRequest",
          ])
        return
      }
      guard self.activeFloatingPromptEditor == nil,
        self.activeAppModalKind == nil,
        self.activeNativeAppModalKind == nil,
        !self.appModalPresentationPending
      else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorReturnFocusSkipped",
          details: [
            "activeAppModalKind": self.activeAppModalKind ?? "<none>",
            "activeNativeAppModalKind": self.activeNativeAppModalKind ?? "<none>",
            "appModalPresentationPending": self.appModalPresentationPending,
            "focusRequestId": focusRequestId,
            "hasActiveFloatingPromptEditor": self.activeFloatingPromptEditor != nil,
            "rootModalHostMounted": false,
            "reason": reason,
            "sessionId": sessionId,
            "skipReason": "modalStillActive",
          ])
        return
      }
      guard self.workspaceView.canDirectlyRestorePromptEditorFocus(sessionId: sessionId) else {
        /*
         CDXC:PromptEditor 2026-06-09-11:19:
         If the terminal that launched the Ctrl+G Monaco prompt editor is hidden or no longer the selected workspace focus target when the editor closes, return through the sidebar's focusTerminal path instead of directly focusing native AppKit views. The sidebar path owns project activation, tab reveal, sleeping-session wake, selection state, and layout sync, matching a user click on that session in the sidebar.
         */
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorReturnFocusSidebarRoute",
          details: [
            "focusRequestId": focusRequestId,
            "reason": reason,
            "responderBeforeRoute": self.responderSnapshot(),
            "routeReason": "launcherNotDirectlyFocusable",
            "sessionId": sessionId,
            "workspaceSnapshotBeforeRoute": self.workspaceView.activationDebugSnapshot(),
          ])
        self.requestSidebarFocusForFloatingPromptEditorClose(
          sessionId: sessionId,
          reason: reason,
          focusRequestId: focusRequestId)
        return
      }
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.floatingPromptEditorReturnFocusDispatching",
        details: [
          "focusRequestId": focusRequestId,
          "reason": reason,
          "responderBeforeDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotBeforeDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
      self.workspaceView.focusTerminal(sessionId: sessionId, reason: "floatingPromptEditor.\(reason)")
      let immediateReinforceResult = self.workspaceView.reinforceWorkspaceFocus(
        sessionId: sessionId,
        reason: "floatingPromptEditor.immediate.\(reason)")
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.floatingPromptEditorReturnFocusDispatched",
        details: [
          "focusRequestId": focusRequestId,
          "immediateReinforceResult": immediateReinforceResult,
          "reason": reason,
          "responderAfterDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotAfterDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
      self.scheduleFloatingPromptEditorReturnFocusReinforcement(
        sessionId: sessionId,
        reason: reason,
        focusRequestId: focusRequestId)
    }
  }

  private func requestSidebarFocusForFloatingPromptEditorClose(
    sessionId: String,
    reason: String,
    focusRequestId: UInt64
  ) {
    let normalizedSessionId = ghostexNativeFocusSessionId(from: sessionId) ?? sessionId
    guard let sessionIdJson = Self.javascriptStringLiteral(normalizedSessionId) else {
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.floatingPromptEditorReturnFocusSidebarRouteSkipped",
        details: [
          "focusRequestId": focusRequestId,
          "reason": reason,
          "sessionId": normalizedSessionId,
          "skipReason": "sessionIdJsonEncodingFailed",
        ])
      return
    }
    sidebarView.evaluateJavaScript(
      """
      (() => {
        const bridge = window.__ghostex_NATIVE_SIDEBAR__;
        if (!bridge?.focusSessionFromPromptEditorClose) {
          return false;
        }
        bridge.focusSessionFromPromptEditorClose(\(sessionIdJson));
        return true;
      })();
      """
    ) { result, error in
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.floatingPromptEditorReturnFocusSidebarRouteCompleted",
        details: [
          "bridgeHandled": (result as? Bool) == true,
          "focusRequestId": focusRequestId,
          "hasError": error != nil,
          "reason": reason,
          "sessionId": normalizedSessionId,
        ])
    }
  }

  private func scheduleFloatingPromptEditorReturnFocusReinforcement(
    sessionId: String,
    reason: String,
    focusRequestId: UInt64
  ) {
    DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(140)) { [weak self] in
      guard let self else {
        return
      }
      guard self.floatingPromptEditorReturnFocusRequestId == focusRequestId else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorReturnFocusReinforcementSkipped",
          details: [
            "focusRequestId": focusRequestId,
            "latestFocusRequestId": self.floatingPromptEditorReturnFocusRequestId,
            "reason": reason,
            "sessionId": sessionId,
            "skipReason": "staleFocusRequest",
          ])
        return
      }
      guard self.activeFloatingPromptEditor == nil,
        self.activeAppModalKind == nil,
        self.activeNativeAppModalKind == nil,
        !self.appModalPresentationPending
      else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorReturnFocusReinforcementSkipped",
          details: [
            "activeAppModalKind": self.activeAppModalKind ?? "<none>",
            "activeNativeAppModalKind": self.activeNativeAppModalKind ?? "<none>",
            "appModalPresentationPending": self.appModalPresentationPending,
            "focusRequestId": focusRequestId,
            "hasActiveFloatingPromptEditor": self.activeFloatingPromptEditor != nil,
            "rootModalHostMounted": false,
            "reason": reason,
            "sessionId": sessionId,
            "skipReason": "modalStillActive",
          ])
        return
      }
      let reinforceResult = self.workspaceView.reinforceWorkspaceFocus(
        sessionId: sessionId,
        reason: "floatingPromptEditor.delayed.\(reason)")
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.floatingPromptEditorReturnFocusReinforcementCompleted",
        details: [
          "focusRequestId": focusRequestId,
          "reason": reason,
          "reinforceResult": reinforceResult,
          "responderAfterReinforcement": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotAfterReinforcement": self.workspaceView.activationDebugSnapshot(),
        ])
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
     CDXC:ReactTitlebar 2026-06-02-13:41:
     The React titlebar can request native process work for macOS-local actions
     such as resource checks and Open In targets. Broadcast host events to that
     webview too so processResult replies resolve in the same bridge contract
     used by the sidebar, while shared Git state remains gxserver-owned.
     */
    titlebarChromeWebView.evaluateJavaScript(script)
    titlebarDropdownPanelController?.dispatchHostEventScript(script)
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
    if let activeProjectIsQuick = command.activeProjectIsQuick {
      payload["projectIsQuick"] = activeProjectIsQuick
    }
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
       CDXC:TitlebarGit 2026-06-02-15:27:
       The titlebar Git split button mirrors the sidebar adapter's gxserver-backed Git status instead of polling separately, so disabled states and commit/push/PR labels stay identical across chrome and sidebar while repository command execution remains gxserver-owned.
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
        "hasCheckedGitHubRemote": git.hasCheckedGitHubRemote,
        "hasGitHubCli": git.hasGitHubCli,
        "hasGitHubRemote": git.hasGitHubRemote,
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
    if let keepAwake = command.keepAwake {
      payload["keepAwake"] = [
        "activateOnExternalDisplay": keepAwake.activateOnExternalDisplay,
        "activateOnLaunch": keepAwake.activateOnLaunch,
        "allowDisplaySleep": keepAwake.allowDisplaySleep,
        "batteryThresholdPercent": keepAwake.batteryThresholdPercent,
        "deactivateBelowBatteryThreshold": keepAwake.deactivateBelowBatteryThreshold,
        "deactivateOnLowPowerMode": keepAwake.deactivateOnLowPowerMode,
        "deactivateOnUserSwitch": keepAwake.deactivateOnUserSwitch,
        "defaultDurationMinutes": keepAwake.defaultDurationMinutes,
        "preventLidSleep": keepAwake.preventLidSleep,
      ]
    }
    if let daemon = command.gxserverDaemon {
      var daemonPayload: [String: Any] = [
        "alwaysStart": daemon.alwaysStart ?? true,
        "state": daemon.state,
      ]
      if let message = daemon.message {
        daemonPayload["message"] = message
      }
      if let nodePath = daemon.nodePath {
        daemonPayload["nodePath"] = nodePath
      }
      if let nodeVersion = daemon.nodeVersion {
        daemonPayload["nodeVersion"] = nodeVersion
      }
      if let ok = daemon.ok {
        daemonPayload["ok"] = ok
      }
      if let pid = daemon.pid {
        daemonPayload["pid"] = pid
      }
      if let startedAt = daemon.startedAt {
        daemonPayload["startedAt"] = startedAt
      }
      if let version = daemon.version {
        daemonPayload["version"] = version
      }
      payload["gxserverDaemon"] = daemonPayload
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
    if let agentHookStatus = command.agentHookStatus {
      /**
       CDXC:AgentHookSettings 2026-06-04-03:05:
       Titlebar Tips & Tricks only needs normalized hook status to warn about
       live affected agents. Do not forward hook file paths or state-directory
       paths into the isolated titlebar payload because the notice is not a
       diagnostics surface.
       */
      var hookPayload: [String: Any] = [
        "agents": agentHookStatus.agents.map { agent in
          [
            "agentId": agent.agentId,
            "cliCommand": agent.cliCommand,
            "cliInstalled": agent.cliInstalled,
            "detail": "",
            "hookInstalled": agent.hookInstalled,
            "paths": [],
            "status": agent.status,
          ] as [String: Any]
        },
        "generatedAt": agentHookStatus.generatedAt,
        "hookStateDirectory": "",
        "notifyHookPath": "",
        "type": agentHookStatus.type,
      ]
      if let errorMessage = agentHookStatus.errorMessage {
        hookPayload["errorMessage"] = errorMessage
      }
      payload["agentHookStatus"] = hookPayload
    }
    if let ghostexCliStatus = command.ghostexCliStatus {
      /**
       CDXC:CliInstall 2026-06-07-15:26:
       Titlebar Tips & Tricks only needs whether the app-owned CLI is
       accessible. Forward booleans and timestamps, not command paths or status
       detail text, so the isolated titlebar notice is actionable without
       becoming a diagnostics surface.
       */
      payload["ghostexCliStatus"] = [
        "generatedAt": ghostexCliStatus.generatedAt,
        "gxUsable": ghostexCliStatus.gxUsable,
        "installed": ghostexCliStatus.installed,
        "type": ghostexCliStatus.type,
      ] as [String: Any]
    }
    if let resourceGroups = command.titlebarResourceGroups {
      /**
       CDXC:TitlebarResources 2026-06-02-15:27:
       Forward the sidebar adapter's gxserver-backed presentation grouping into the isolated React titlebar so its resource dropdown can render shared project rows plus local Quick sections while the titlebar webview polls process metrics independently.
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
      payload["workspaceOpenTargets"] = [
        "availability": [
          "availableTargetIds": availability?.availableTargetIds ?? [],
          "checkedAtMs": availability?.checkedAtMs ?? 0,
          "resolvedAppNames": availability?.resolvedAppNames ?? [:],
          "resolvedCommands": availability?.resolvedCommands ?? [:],
        ],
        "customTargets": openTargets.customTargets?.map { target in
          [
            "args": target.args ?? [],
            "command": target.command,
            "id": target.id,
            "label": target.label,
          ] as [String: Any]
        } ?? [],
        "hiddenTargetIds": openTargets.hiddenTargetIds ?? [],
      ]
    }
    guard
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    latestReactTitlebarProjectStateJson = json
    dispatchReactTitlebarProjectState(json)
  }

  private func dispatchReactTitlebarProjectState(_ json: String) {
    titlebarChromeWebView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.setActiveProjectState(\(json));
      undefined;
      """)
    titlebarDropdownPanelController?.setActiveProjectState(json)
  }

  func setReactTitlebarHitRegions(_ regions: [ReactTitlebarHitRegion], overlayOpen: Bool) {
    /*
     CDXC:ReactTitlebar 2026-06-11-13:22:
     Main-window titlebar dropdown content has moved to native child panels, so
     Swift must ignore any below-strip overlay-open state from the titlebar
     document and keep the workspace interaction shield off for titlebar menus.
     */
    titlebarChromeView.setHitRegions(regions, overlayOpen: false)
    isTitlebarOverlayOpen = false
    updateWorkspaceInteractionShield()
    needsLayout = true
  }

  func routeTitlebarMouseEventFromWindow(_ event: NSEvent) -> Bool {
    /*
     CDXC:NativePaneTabClicks 2026-06-12-07:11:
     The Browser project tab Add button can sit in a top workspace band where
     AppKit window-chrome dispatch bypasses root/content hit testing entirely.
     Route pane-titlebar mouse streams from the NSWindow boundary before React
     titlebar routing so the visible native titlebar owns Add, Close, activation,
     and drag by its current AppKit geometry.
     */
    if routeWorkspacePaneTitleBarMouseEventFromWindow(event) {
      return true
    }
    return titlebarChromeView.routeWindowMouseEvent(event)
  }

  private func routeWorkspacePaneTitleBarMouseEventFromWindow(_ event: NSEvent) -> Bool {
    guard Self.isWorkspacePaneTitleBarWindowMouseEvent(event.type),
      event.window === window,
      !workspaceView.isHidden
    else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)
    guard workspaceView.frame.contains(point) else {
      return false
    }
    let workspacePoint = convert(point, to: workspaceView)
    let source = "windowPaneTitleBarPrepass"
    guard workspaceView.routePaneTitleBarMouseEvent(event, at: workspacePoint, source: source) else {
      return false
    }
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.root.windowMouseEvent.titleBarPrepass", details: [
      "eventType": Self.workspacePaneTitleBarWindowMouseEventTypeName(event.type),
      "rootPoint": ["x": Double(point.x), "y": Double(point.y)],
      "source": source,
      "workspaceFrame": [
        "height": Double(workspaceView.frame.height),
        "maxX": Double(workspaceView.frame.maxX),
        "maxY": Double(workspaceView.frame.maxY),
        "minX": Double(workspaceView.frame.minX),
        "minY": Double(workspaceView.frame.minY),
        "width": Double(workspaceView.frame.width),
      ],
      "workspacePoint": ["x": Double(workspacePoint.x), "y": Double(workspacePoint.y)],
    ])
    return true
  }

  private static func isWorkspacePaneTitleBarWindowMouseEvent(_ eventType: NSEvent.EventType) -> Bool {
    switch eventType {
    case .leftMouseDown, .leftMouseDragged, .leftMouseUp:
      return true
    default:
      return false
    }
  }

  private static func workspacePaneTitleBarWindowMouseEventTypeName(_ eventType: NSEvent.EventType) -> String {
    switch eventType {
    case .leftMouseDown:
      return "leftMouseDown"
    case .leftMouseDragged:
      return "leftMouseDragged"
    case .leftMouseUp:
      return "leftMouseUp"
    default:
      return "other"
    }
  }

  func showTitlebarDropdownPanel(_ command: ShowTitlebarDropdownPanel) {
    guard let window,
      let anchorScreenRect = titlebarDropdownAnchorScreenRect(command.anchorRect)
    else {
      return
    }
    if titlebarDropdownPanelController == nil {
      titlebarDropdownPanelController = TitlebarDropdownPanelController(
        scriptBridge: scriptBridge,
        bootstrapScriptSource: titlebarBootstrapScriptSource,
        diagnosticsScript: Self.diagnosticsScript,
        shouldLetTitlebarHandleOutsideMouseEvent: { [weak self] event in
          guard let self, event.window === self.window else {
            return false
          }
          let point = self.convert(event.locationInWindow, from: nil)
          let titlebarPoint = self.titlebarChromeView.convert(point, from: self)
          return self.titlebarChromeView.containsInteractiveHitRegion(titlebarPoint)
        }
      ) { [weak self] kind in
        self?.titlebarChromeView.setNativeDropdownOpen(kind)
      }
    }
    titlebarDropdownPanelController?.show(
      kind: command.kind,
      anchorScreenRect: anchorScreenRect,
      parentWindow: window,
      preferredSize: command.preferredSize.map {
        CGSize(width: CGFloat($0.width), height: CGFloat($0.height))
      },
      webAssets: Self.resolveWebAssets(),
      latestStateJson: latestReactTitlebarProjectStateJson)
  }

  func closeTitlebarDropdownPanel() {
    titlebarDropdownPanelController?.close()
  }

  func resizeTitlebarDropdownPanel(_ command: ResizeTitlebarDropdownPanel) {
    titlebarDropdownPanelController?.resize(
      kind: command.kind,
      width: CGFloat(command.width),
      height: CGFloat(command.height))
  }

  func titlebarDropdownPanelReady(_ command: TitlebarDropdownPanelReady) {
    titlebarDropdownPanelController?.showWhenReady(kind: command.kind)
  }

  private func titlebarDropdownAnchorScreenRect(
    _ anchor: TitlebarDropdownAnchorRect
  ) -> CGRect? {
    guard let window else {
      return nil
    }
    /*
     CDXC:ReactTitlebar 2026-06-11-13:22:
     React reports dropdown anchors in the titlebar webview's top-left CSS
     coordinate space. Convert to AppKit's bottom-left local coordinates before
     producing the screen rect used by the native child dropdown panel.
     */
    let height = CGFloat(anchor.height)
    let width = CGFloat(anchor.width)
    guard width > 0, height > 0 else {
      return nil
    }
    let titlebarHeight = max(titlebarChromeView.bounds.height, 1)
    let rectInTitlebar = CGRect(
      x: CGFloat(anchor.x),
      y: max(titlebarHeight - CGFloat(anchor.y) - height, 0),
      width: width,
      height: height
    )
    return window.convertToScreen(titlebarChromeView.convert(rectInTitlebar, to: nil))
  }

  func setTitlebarUpdateAvailable(_ available: Bool) {
    let payload: [String: Any] = ["updateAvailable": available]
    guard
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    /**
     CDXC:AutoUpdate 2026-05-28-14:19:
     Sparkle update availability is native state, but the visible affordance
     lives in the isolated React titlebar beside the project identity. Push a
     tiny boolean payload so React can render or hide the quiet download button
     without owning appcast parsing or update installation.

     CDXC:AutoUpdate 2026-06-08-18:21:
     Native may learn about an update before React installs the titlebar bridge.
     Store the latest boolean on the titlebar window before invoking the bridge
     so React can hydrate the current update state when it becomes ready.
     */
    titlebarChromeWebView.evaluateJavaScript(
      """
      window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__ = \(available ? "true" : "false");
      window.__ghostex_TITLEBAR__?.setActiveProjectState(\(json));
      undefined;
      """)
    titlebarDropdownPanelController?.setActiveProjectState(json)
  }

  private func openActiveProjectEditorFromTitlebar() {
    /**
     CDXC:TitlebarOpenIn 2026-06-02-15:27:
     Titlebar Code and Embedded Editor clicks are app-chrome actions that enter the same sidebar-adapter project-editor flow as the project header. Forward the command into the sidebar webview instead of reimplementing code-server startup or project surface state in Swift.

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
     CDXC:SessionFocusMode 2026-06-02-15:27:
     The titlebar exit-focus control restores current-window layout state. Route it through the sidebar adapter, matching native pane-tab double click, so Swift does not own pane focus or mode history.
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
     CDXC:ProjectBrowserTabs 2026-06-13-00:12:
     Browser mode opens or restores the active project's Browser tab group inside the workarea, not in an external browser. Forward to the sidebar adapter so GitHub seed selection, fallback URL handling, and macOS-owned browser surface focus stay in the normal paths.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.openGitHubProjectFromTitlebar?.();
      undefined;
      """)
  }

  private func toggleProjectEditorCompanionFromTitlebar() {
    /**
     CDXC:ProjectEditorCompanion 2026-06-12-03:18:
     The titlebar companion control now toggles the project-owned hidden
     preference instead of only restoring it. Forward through React sidebar
     state so Code, Browser, and Kanban modes keep one shared local value before
     native AppKit applies layout.
     */
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.toggleProjectEditorCompanionFromTitlebar?.();
      undefined;
      """)
  }

  private func openTasksPlaceholderFromTitlebar() {
    /**
     CDXC:ModeSwitcher 2026-06-02-15:27:
     Project mode is a bundled React workarea backed by the project-board bridge. Let the sidebar adapter open it as a macOS project surface while gxserver remains responsible for Beads/project-board data and mutations.
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
    guard let commandIdJson = Self.javascriptStringLiteral(command.commandId) else {
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
    guard let actionJson = Self.javascriptStringLiteral(command.action) else {
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

  private func focusResourceSessionFromTitlebar(_ command: FocusResourceSessionFromTitlebar) {
    /**
     CDXC:TitlebarResources 2026-06-02-15:27:
     Resource-row Focus is React titlebar chrome, but current-window focus routing belongs to the sidebar adapter. Forward the selected combined session id into the sidebar webview so cross-project focus and gxserver-backed sleeping-session wake behavior stay in one path.
     */
    guard let sessionIdJson = Self.javascriptStringLiteral(command.sessionId) else {
      return
    }
    sidebarView.evaluateJavaScript(
      """
      window.__ghostex_NATIVE_SIDEBAR__?.focusResourceSessionFromTitlebar?.(\(sessionIdJson));
      undefined;
      """)
  }

  private func quitResourcesFromTitlebar(_ command: QuitResourcesFromTitlebar) {
    /**
     CDXC:TitlebarResources 2026-06-02-15:27:
     React titlebar resource Quit controls identify presentation session ids and local project-editor ids. Forward them to the sidebar adapter so shared terminal lifecycle routes through gxserver while native surfaces and local panes close from the current-window coordinator.
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
    case .focusProjectEditorCompanionSession(let command):
      focusWorkspaceSessionAfterSidebarActivation(
        sessionId: command.sessionId,
        kind: .projectEditorCompanion)
    case .focusWebPane(let command):
      focusWorkspaceSessionAfterSidebarActivation(sessionId: command.sessionId, kind: .webPane)
    case .reloadWebPane(let command):
      workspaceView.reloadWebPane(sessionId: command.sessionId)
    case .startT3CodeRuntime(let command):
      scheduleT3CodeRuntimeStart(
        command,
        reason: "sidebarCommand",
        requiredPaneStateGeneration: nil)
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
      let suppressExplicitWorkspaceFocus = isFloatingPromptEditorActiveForUserInput
      if suppressExplicitWorkspaceFocus, command.focusRequestId != nil {
        /*
         CDXC:PromptEditor 2026-06-09-10:43:
         Sidebar session clicks are allowed to update the visible workspace behind the Ctrl+G Monaco prompt editor, but they must not turn the layout-sync focus request into AppKit first-responder focus while the editor's launching terminal process is still waiting. Suppress only the native focus side effect; keep the sidebar-owned layout and selection state current so closing the editor can route through the normal sidebar reveal path when needed.
         */
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.floatingPromptEditorLayoutFocusSuppressed",
          details: [
            "activeAppModalKind": activeAppModalKind ?? "<none>",
            "focusRequestId": command.focusRequestId ?? 0,
            "focusedSessionId": command.focusedSessionId ?? "",
            "hasActiveFloatingPromptEditor": activeFloatingPromptEditor != nil,
          ])
      }
      workspaceView.setActiveTerminalSet(
        command,
        suppressExplicitFocus: suppressExplicitWorkspaceFocus)
    case .setSessionPaneChrome(let command):
      workspaceView.setSessionPaneChrome(command)
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
    case .appendLayoutLayeringDebugLog(let command):
      AppDelegate.appendLayoutLayeringDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendProjectBoardDebugLog(let command):
      AppDelegate.appendProjectBoardDebugLog(event: command.event, details: command.details)
    case .appendTerminalFocusDebugLog(let command):
      AppDelegate.appendTerminalFocusDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendRestoreDebugLog(let command):
      AppDelegate.appendRestoreDebugLog(event: command.event, details: command.details)
    case .appendSessionTitleDebugLog(let command):
      AppDelegate.appendSessionTitleDebugLog(
        event: command.event, details: command.details, force: command.force == true)
    case .appendSidebarCollapseStateDebugLog(let command):
      AppDelegate.appendSidebarCollapseStateDebugLog(event: command.event, details: command.details)
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
    case .cancelRunProcess(let command):
      NativeProcessRegistry.shared.cancel(requestId: command.requestId)
    case .gxserverRequest(let command):
      Task { [weak self] in
        let event = await GxserverClient.request(command)
        await MainActor.run {
          self?.postHostEvent(event)
        }
      }
    case .remoteGxserverConnect(let command):
      postHostEvent(RemoteGxserverClient.shared.connectingStatus(
        remoteMachineId: command.remoteMachineId,
        requestId: command.requestId
      ))
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.connect(command)
        await MainActor.run {
          self?.postHostEvent(event)
        }
      }
    case .remoteGxserverRequest(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.request(command)
        await MainActor.run {
          self?.postHostEvent(event)
        }
      }
    case .remoteGxserverSubscribePresentation(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.subscribePresentation(command) { event in
          Task { [weak self] in
            await MainActor.run {
              self?.postHostEvent(event)
            }
          }
        }
        await MainActor.run {
          self?.postHostEvent(event)
        }
      }
    case .remoteSshPasswordSave(let command):
      Task { [weak self] in
        let event = await RemoteGxserverClient.shared.saveSshPassword(command)
        await MainActor.run {
          self?.postHostEvent(event)
        }
      }
    case .setKeepAwakeLidSleepPrevention(let command):
      LidSleepPrivilegedHelperClient.shared.setEnabled(
        command.enabled,
        requestId: command.requestId,
        installIfNeeded: command.installIfNeeded ?? command.enabled
      ) { [weak self] event in
        self?.postHostEvent(event)
      }
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
    case .setOSIntegrationDefaults(let command):
      guard let bundleIdentifier = Bundle.main.bundleIdentifier else {
        showMessage(.init(level: .error, message: "Ghostex bundle identifier is missing."))
        return
      }
      let failures = AppDelegate.osIntegrationDefaultFailures(
        target: command.target,
        bundleIdentifier: bundleIdentifier)
      if failures.isEmpty {
        presentAppToast(level: "success", title: "Updated macOS OS Integration defaults.")
      } else {
        showMessage(.init(level: .error, message: "Could not set defaults: \(failures.joined(separator: ", "))"))
      }
      postHostEvent(AppDelegate.osIntegrationStatusEvent(bundleIdentifier: bundleIdentifier))
    case .requestOSIntegrationStatus:
      postHostEvent(AppDelegate.osIntegrationStatusEvent(bundleIdentifier: Bundle.main.bundleIdentifier ?? ""))
    case .openExternalUrl(let command):
      openExternalUrl(command)
    case .openWorkspaceInFinder(let command):
      openWorkspaceInFinder(command)
    case .openWorkspaceInIde(let command):
      openWorkspaceInIde(command)
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
    case .toggleSidebarCollapsed:
      toggleSidebarCollapsed()
    case .setReactTitlebarHitRegions(let command):
      /**
       CDXC:ReactTitlebar 2026-05-11-20:24
       React owns titlebar button geometry, but Swift owns window dragging and
       workspace pass-through. Apply reported DOM hit regions at the native
       overlay boundary and relayout so the native titlebar webview only covers
       the titlebar strip plus any open dropdown bounds.
       */
      setReactTitlebarHitRegions(command.regions, overlayOpen: command.overlayOpen)
    case .showTitlebarDropdownPanel(let command):
      showTitlebarDropdownPanel(command)
    case .closeTitlebarDropdownPanel:
      closeTitlebarDropdownPanel()
    case .resizeTitlebarDropdownPanel(let command):
      resizeTitlebarDropdownPanel(command)
    case .titlebarDropdownPanelReady(let command):
      titlebarDropdownPanelReady(command)
    case .openActiveProjectEditorFromTitlebar:
      openActiveProjectEditorFromTitlebar()
    case .exitFocusModeFromTitlebar:
      exitFocusModeFromTitlebar()
    case .openAgentsModeFromTitlebar:
      openAgentsModeFromTitlebar()
    case .openGitHubProjectFromTitlebar:
      openGitHubProjectFromTitlebar()
    case .toggleProjectEditorCompanionFromTitlebar:
      toggleProjectEditorCompanionFromTitlebar()
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
    case .showUpdateDialogFromTitlebar:
      showUpdateDialogFromTitlebar()
    case .startGxserverFromTitlebar:
      startGxserverFromTitlebar()
    case .stopGxserverFromTitlebar:
      stopGxserverFromTitlebar()
    case .restartGxserverFromTitlebar:
      restartGxserverFromTitlebar()
    case .setGxserverAlwaysStartFromTitlebar(let command):
      setGxserverAlwaysStartFromTitlebar(command.enabled)
    case .focusResourceSessionFromTitlebar(let command):
      focusResourceSessionFromTitlebar(command)
    case .sleepInactiveSessionsFromTitlebar(let command):
      sleepInactiveSessionsFromTitlebar(command)
    case .quitResourcesFromTitlebar(let command):
      quitResourcesFromTitlebar(command)
    case .runSidebarCommandFromTitlebar(let command):
      runSidebarCommandFromTitlebar(command)
    case .runSidebarGitActionFromTitlebar(let command):
      runSidebarGitActionFromTitlebar(command)
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
    case projectEditorCompanion
    case terminal
    case webPane

    var debugName: String {
      switch self {
      case .projectEditorCompanion:
        return "projectEditorCompanion"
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
    sidebarWorkspaceFocusRequestId += 1
    let focusRequestId = sidebarWorkspaceFocusRequestId
    let projectEditorFocusOwnerRevisionBeforeQueue =
      workspaceView.currentProjectEditorFocusOwnerRevision()
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

     CDXC:SidebarSessionFocus 2026-06-05-22:12:
     Sidebar session clicks must leave the clicked session ready for typing.
     WebKit can still win first responder after the deferred focus command, so
     tag each click with a monotonic request id and run one idempotent
     first-responder reinforcement after the sidebar event has settled.

     CDXC:PromptEditor 2026-06-09-10:43:
     Sidebar clicks while the Ctrl+G Monaco prompt editor is open may change sidebar selection and native layout behind the editor, but they must not close the editor or move keyboard focus away from it. Skip the explicit native focus command until the editor save/cancel path runs return-focus routing.

     CDXC:ProjectBoardFocus 2026-06-12-08:44:
     Deferred sidebar focus commands must lose to newer Project/Kanban editor input.
     Capture the project-editor focus-owner revision before queueing and skip dispatch/reinforcement if the editor reports focus after this command was requested, while still allowing deliberate session clicks when no newer board input occurs.
     */
    guard !isFloatingPromptEditorActiveForUserInput else {
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusCommandSkipped",
        details: [
          "activeAppModalKind": activeAppModalKind ?? "<none>",
          "focusRequestId": focusRequestId,
          "hasActiveFloatingPromptEditor": activeFloatingPromptEditor != nil,
          "kind": kind.debugName,
          "sessionId": sessionId,
          "skipReason": "floatingPromptEditorActive",
        ])
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.sidebarFocusCommandQueued",
      details: [
        "focusRequestId": focusRequestId,
        "kind": kind.debugName,
        "projectEditorFocusOwnerRevisionBeforeQueue": projectEditorFocusOwnerRevisionBeforeQueue,
        "responderBeforeQueue": responderSnapshot(),
        "sessionId": sessionId,
        "webChromeFirstResponder": isWebChromeFirstResponder(),
        "workspaceSnapshotBeforeQueue": workspaceView.activationDebugSnapshot(),
      ])
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        return
      }
      guard !self.isFloatingPromptEditorActiveForUserInput else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.sidebarFocusCommandSkipped",
          details: [
            "activeAppModalKind": self.activeAppModalKind ?? "<none>",
            "focusRequestId": focusRequestId,
            "hasActiveFloatingPromptEditor": self.activeFloatingPromptEditor != nil,
            "kind": kind.debugName,
            "sessionId": sessionId,
            "skipReason": "floatingPromptEditorActiveAfterQueue",
          ])
        return
      }
      guard !self.workspaceView.hasProjectEditorFocusOwnerChanged(
        since: projectEditorFocusOwnerRevisionBeforeQueue)
      else {
        let latestProjectEditorFocusOwnerRevision =
          self.workspaceView.currentProjectEditorFocusOwnerRevision()
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.sidebarFocusCommandSkipped",
          details: [
            "focusRequestId": focusRequestId,
            "kind": kind.debugName,
            "latestProjectEditorFocusOwnerRevision": latestProjectEditorFocusOwnerRevision,
            "projectEditorFocusOwnerRevisionBeforeQueue": projectEditorFocusOwnerRevisionBeforeQueue,
            "sessionId": sessionId,
            "skipReason": "projectEditorFocusOwnerChangedAfterQueue",
          ])
        return
      }
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusCommandDispatching",
        details: [
          "focusRequestId": focusRequestId,
          "kind": kind.debugName,
          "responderBeforeDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotBeforeDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
      switch kind {
      case .projectEditorCompanion:
        self.workspaceView.focusProjectEditorCompanionSession(
          sessionId: sessionId,
          reason: "sidebarFocusCommand")
      case .terminal:
        self.workspaceView.focusTerminal(sessionId: sessionId, reason: "sidebarFocusCommand")
      case .webPane:
        self.workspaceView.focusWebPane(sessionId: sessionId, reason: "sidebarFocusCommand")
      }
      let immediateReinforceResult = self.workspaceView.reinforceSidebarWorkspaceFocus(
        sessionId: sessionId,
        reason: "sidebarFocusCommand.immediate.\(kind.debugName)")
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusCommandDispatched",
        details: [
          "focusRequestId": focusRequestId,
          "immediateReinforceResult": immediateReinforceResult,
          "kind": kind.debugName,
          "responderAfterDispatch": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotAfterDispatch": self.workspaceView.activationDebugSnapshot(),
        ])
      self.scheduleSidebarWorkspaceFocusReinforcement(
        sessionId: sessionId,
        kind: kind,
        focusRequestId: focusRequestId,
        projectEditorFocusOwnerRevisionBeforeQueue: projectEditorFocusOwnerRevisionBeforeQueue)
    }
  }

  private func scheduleSidebarWorkspaceFocusReinforcement(
    sessionId: String,
    kind: SidebarWorkspaceFocusKind,
    focusRequestId: UInt64,
    projectEditorFocusOwnerRevisionBeforeQueue: UInt64
  ) {
    DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(140)) { [weak self] in
      guard let self else {
        return
      }
      guard !self.isFloatingPromptEditorActiveForUserInput else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.sidebarFocusReinforcementSkipped",
          details: [
            "activeAppModalKind": self.activeAppModalKind ?? "<none>",
            "focusRequestId": focusRequestId,
            "hasActiveFloatingPromptEditor": self.activeFloatingPromptEditor != nil,
            "kind": kind.debugName,
            "sessionId": sessionId,
            "skipReason": "floatingPromptEditorActive",
          ])
        return
      }
      guard !self.workspaceView.hasProjectEditorFocusOwnerChanged(
        since: projectEditorFocusOwnerRevisionBeforeQueue)
      else {
        let latestProjectEditorFocusOwnerRevision =
          self.workspaceView.currentProjectEditorFocusOwnerRevision()
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.sidebarFocusReinforcementSkipped",
          details: [
            "focusRequestId": focusRequestId,
            "kind": kind.debugName,
            "latestProjectEditorFocusOwnerRevision": latestProjectEditorFocusOwnerRevision,
            "projectEditorFocusOwnerRevisionBeforeQueue": projectEditorFocusOwnerRevisionBeforeQueue,
            "sessionId": sessionId,
            "skipReason": "projectEditorFocusOwnerChangedAfterQueue",
          ])
        return
      }
      guard self.sidebarWorkspaceFocusRequestId == focusRequestId else {
        TerminalFocusDebugLog.append(
          event: "nativeFocusTrace.sidebarFocusReinforcementSkipped",
          details: [
            "focusRequestId": focusRequestId,
            "kind": kind.debugName,
            "latestFocusRequestId": self.sidebarWorkspaceFocusRequestId,
            "sessionId": sessionId,
            "skipReason": "staleFocusRequest",
          ])
        return
      }
      let reinforceResult = self.workspaceView.reinforceSidebarWorkspaceFocus(
        sessionId: sessionId,
        reason: "sidebarFocusCommand.delayed.\(kind.debugName)")
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.sidebarFocusReinforcementCompleted",
        details: [
          "focusRequestId": focusRequestId,
          "kind": kind.debugName,
          "reinforceResult": reinforceResult,
          "responderAfterReinforcement": self.responderSnapshot(),
          "sessionId": sessionId,
          "webChromeFirstResponder": self.isWebChromeFirstResponder(),
          "workspaceSnapshotAfterReinforcement": self.workspaceView.activationDebugSnapshot(),
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
    let isTitlebarResponder =
      responderView.map { $0 === titlebarChromeWebView || $0.isDescendant(of: titlebarChromeWebView) }
      ?? false
    return [
      "className": String(describing: type(of: responder)),
      "isModalHostResponder": false,
      "isSidebarResponder": isSidebarResponder,
      "isTitlebarResponder": isTitlebarResponder,
    ]
  }

  /**
   CDXC:T3Code 2026-06-06-05:13:
   Runtime lifetime follows live native managed T3 panes, not sidebar session
   cards. The pane registry reports all open T3 web panes, including inactive
   tabs, so native can keep the heartbeat fresh and repair localhost when the
   user still has an embedded T3 tab open.
   */
  private func setT3CodeRuntimePaneState(_ state: ManagedT3PaneRuntimeState) {
    NativeT3RuntimeLauncher.setLiveManagedPaneHeartbeat(
      paneSessionIds: state.paneSessionIds,
      reason: "nativePane.\(state.reason)")
    t3RuntimePaneStateGeneration &+= 1
    let runtimeCwd = state.runtimeCwd?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !state.paneSessionIds.isEmpty, let runtimeCwd, !runtimeCwd.isEmpty else {
      t3RuntimeVisibleSessionCwd = nil
      pendingT3RuntimeStartWorkItem?.cancel()
      pendingT3RuntimeStartWorkItem = nil
      t3RuntimeLivenessTimer?.invalidate()
      t3RuntimeLivenessTimer = nil
      return
    }

    t3RuntimeVisibleSessionCwd = runtimeCwd
    scheduleT3CodeRuntimeStartForLivePanes(reason: "nativePane.\(state.reason)")
    if t3RuntimeLivenessTimer == nil {
      let timer = Timer(timeInterval: 10.0, repeats: true) { [weak self] _ in
        self?.scheduleT3CodeRuntimeStartForLivePanes(reason: "livenessTimer", debounceMilliseconds: 0)
      }
      t3RuntimeLivenessTimer = timer
      RunLoop.main.add(timer, forMode: .common)
    }
  }

  private func scheduleT3CodeRuntimeStartForLivePanes(
    reason: String,
    debounceMilliseconds: Int = 180
  ) {
    guard let runtimeCwd = t3RuntimeVisibleSessionCwd else {
      return
    }
    scheduleT3CodeRuntimeStart(
      StartT3CodeRuntime(cwd: runtimeCwd),
      reason: reason,
      requiredPaneStateGeneration: t3RuntimePaneStateGeneration,
      debounceMilliseconds: debounceMilliseconds)
  }

  private func scheduleT3CodeRuntimeStart(
    _ command: StartT3CodeRuntime,
    reason: String,
    requiredPaneStateGeneration: UInt64?,
    debounceMilliseconds: Int = 180
  ) {
    /*
     CDXC:T3Code 2026-06-08-13:04:
     T3 pane close/open transitions can emit managed-pane state and explicit runtime-start commands while AppKit is retargeting the Project Editor companion pane. Coalesce those requests and run the first localhost responsiveness probe off the immediate sidebar command stack so lsof/ps and HTTP waits cannot participate in the same layout/update-constraints recursion.
     */
    pendingT3RuntimeStartWorkItem?.cancel()
    let workItem = DispatchWorkItem { [weak self] in
      guard let self else {
        return
      }
      self.pendingT3RuntimeStartWorkItem = nil
      let expectedGeneration = requiredPaneStateGeneration
      let expectedCwd = command.cwd.trimmingCharacters(in: .whitespacesAndNewlines)
      DispatchQueue.global(qos: .utility).async { [weak self] in
        let hasResponsiveRuntime = NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener()
        DispatchQueue.main.async { [weak self] in
          guard let self else {
            return
          }
          if let expectedGeneration {
            guard self.t3RuntimePaneStateGeneration == expectedGeneration else {
              return
            }
            guard self.t3RuntimeVisibleSessionCwd?.trimmingCharacters(in: .whitespacesAndNewlines) == expectedCwd
            else {
              return
            }
          }
          guard !hasResponsiveRuntime else {
            return
          }
          if expectedGeneration == nil {
            self.startT3CodeRuntime(command)
          } else {
            self.ensureT3CodeRuntimeForLivePanes(reason: reason)
          }
        }
      }
    }
    pendingT3RuntimeStartWorkItem = workItem
    DispatchQueue.main.asyncAfter(
      deadline: .now() + .milliseconds(max(debounceMilliseconds, 0)),
      execute: workItem)
  }

  /**
   CDXC:T3Code 2026-06-06-05:13:
   A live managed T3 pane means the shared localhost provider is required even
   if the sidebar projection currently omits the card. Probe and restart from
   the pane-derived workspace root so an open T3 tab does not drift offline.
   */
  private func ensureT3CodeRuntimeForLivePanes(reason: String) {
    guard let runtimeCwd = t3RuntimeVisibleSessionCwd else {
      return
    }
    guard !NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
      return
    }
    guard !isT3RuntimeAutoStartBackedOff(logPrefix: "nativeSidebar", reason: reason) else {
      return
    }
    NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.livePanes.autoStart", [
      "cwd": runtimeCwd,
      "reason": reason,
    ])
    startT3CodeRuntime(StartT3CodeRuntime(cwd: runtimeCwd))
  }

  private func isT3RuntimeAutoStartBackedOff(logPrefix: String, reason: String) -> Bool {
    guard let until = t3RuntimeAutoStartBackoffUntil else {
      return false
    }
    let remainingSeconds = until.timeIntervalSinceNow
    guard remainingSeconds > 0 else {
      t3RuntimeAutoStartBackoffUntil = nil
      return false
    }
    NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.start.backoffActive", [
      "reason": reason,
      "remainingSeconds": remainingSeconds,
    ])
    return true
  }

  private func recordT3RuntimeLaunchFailure(logPrefix: String, reason: String) {
    t3RuntimeAutoStartBackoffUntil = Date().addingTimeInterval(
      NativeT3RuntimeFailureNotice.autoStartBackoffInterval)
    NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.start.backoffSet", [
      "backoffSeconds": NativeT3RuntimeFailureNotice.autoStartBackoffInterval,
      "reason": reason,
    ])
    sendHostEvent(.t3RuntimeStartFailed(sessionId: nil, message: NativeT3RuntimeFailureNotice.message))
  }

  /**
   CDXC:T3Code 2026-06-06-05:13:
   Sidebar-projected T3 session state is retained only for protocol
   compatibility. It must not refresh or stop the managed provider because
   gxserver presentation can exclude local T3 panes while native still owns a
   live embedded tab.
   */
  private func setT3CodeRuntimeSessionState(_ command: SetT3CodeRuntimeSessionState, reason: String) {
    NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.sidebarSessionState.ignored", [
      "hasRuntimeCwd": command.runtimeCwd?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
      "reason": reason,
      "runningSessionCount": command.runningSessionIds.count,
    ])
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

     CDXC:T3CodeStartup 2026-06-09-07:07:
     Liveness checks that retain a booting or already-claimed T3 launch must
     not repaint managed web panes. Only a spawned replacement runtime reloads
     the WKWebView, which keeps terminal typing from seeing periodic spinners.
     */
    t3RuntimeAutoStartBackoffUntil = nil
    if let process = t3CodeRuntimeProcess, process.isRunning {
      /**
       CDXC:T3Code 2026-05-02-00:48
       Native sidebar T3 cards can restore while a previously retained Bun
       server is wedged but still running. Verify auth/session responsiveness
       before reusing the process so the pane does not stay on a white unloaded
       WKWebView.
       */
      guard NativeT3RuntimeLauncher.hasResponsiveManagedRuntimeListener() else {
        if let startedAt = t3CodeRuntimeStartedAt {
          let runtimeAgeSeconds = Date().timeIntervalSince(startedAt)
          if runtimeAgeSeconds <= NativeT3RuntimeLauncher.startupGraceInterval {
            NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.booting", [
              "pid": process.processIdentifier,
              "runtimeAgeSeconds": runtimeAgeSeconds,
              "startupGraceSeconds": NativeT3RuntimeLauncher.startupGraceInterval,
            ])
            return
          }
        }
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
          return
        }
        NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.runningUnhealthy", [
          "cwd": command.cwd,
          "pid": process.processIdentifier,
        ])
        process.terminate()
        t3CodeRuntimeProcess = nil
        t3CodeRuntimeStartedAt = nil
        NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeSidebar")
        return startT3CodeRuntime(command)
      }
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.reused", [
        "cwd": command.cwd,
        "pid": process.processIdentifier,
      ])
      return
    }
    if let process = t3CodeRuntimeProcess, !process.isRunning {
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.trackedExited", [
        "pid": process.processIdentifier
      ])
      t3CodeRuntimeProcess = nil
      t3CodeRuntimeStartedAt = nil
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

    let launchStartedAt: Date
    switch NativeT3RuntimeLauncher.claimLaunchStart() {
    case .retained(let launchAgeSeconds):
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.launchInProgressRetained", [
        "launchAgeSeconds": launchAgeSeconds,
        "startupGraceSeconds": NativeT3RuntimeLauncher.startupGraceInterval,
      ])
      return
    case .claimed(let claimedStartedAt):
      launchStartedAt = claimedStartedAt
    }

    NativeT3RuntimeLauncher.clearStaleRuntimeIfNeeded(logPrefix: "nativeSidebar")
    if NativeT3RuntimeLauncher.hasManagedRuntimeListener() {
      NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
      NativeT3CodePaneReproLog.append(
        "nativeSidebar.t3Runtime.start.retainedExistingUnresponsive",
        [
          "cwd": command.cwd,
          "port": NativeT3RuntimeLauncher.port,
        ])
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
      t3CodeRuntimeStartedAt = launchStartedAt
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.spawned", [
        "args": process.arguments ?? [],
        "cwd": command.cwd,
        "executable": process.executableURL?.path ?? NSNull(),
        "pid": process.processIdentifier,
        "startedAt": launchStartedAt.timeIntervalSince1970,
      ])
      workspaceView.reloadManagedT3WebPanes(reason: "runtimeSpawned")
      process.terminationHandler = { [weak self, outputCapture = launch.outputCapture, launchStartedAt] terminatedProcess in
        NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
        var details = outputCapture.finish()
        details["pid"] = terminatedProcess.processIdentifier
        details["reason"] = terminatedProcess.terminationReason.rawValue
        details["status"] = terminatedProcess.terminationStatus
        NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.exit", details)
        let status = terminatedProcess.terminationStatus
        guard NativeT3RuntimeFailureNotice.shouldNotifyLaunchExit(status: status) else {
          return
        }
        DispatchQueue.main.async {
          self?.recordT3RuntimeLaunchFailure(
            logPrefix: "nativeSidebar",
            reason: "processExitStatus\(status)")
        }
      }
    } catch {
      NativeT3RuntimeLauncher.clearLaunchAttempt(startedAt: launchStartedAt)
      NativeT3CodePaneReproLog.append("nativeSidebar.t3Runtime.start.failed", [
        "cwd": command.cwd,
        "error": error.localizedDescription,
      ])
      recordT3RuntimeLaunchFailure(logPrefix: "nativeSidebar", reason: "processRunFailed")
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      ghostexRootView.logger.error("Failed to start T3 Code runtime: \(sanitizedError)")
    }
  }

  /**
   CDXC:T3Code 2026-04-30-09:23
   Native-sidebar Running modal controls must kill the embedded T3 provider
   they display. This command stops tracked process state and any managed T3
   listener on the shared localhost port.
   */
  private func stopT3CodeRuntime(logPrefix: String) {
    pendingT3RuntimeStartWorkItem?.cancel()
    pendingT3RuntimeStartWorkItem = nil
    if let process = t3CodeRuntimeProcess {
      NativeT3CodePaneReproLog.append("\(logPrefix).t3Runtime.stop.tracked", [
        "isRunning": process.isRunning,
        "pid": process.processIdentifier,
      ])
      if process.isRunning {
        process.terminate()
      }
      t3CodeRuntimeProcess = nil
      t3CodeRuntimeStartedAt = nil
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
        /*
         CDXC:EditorPanes 2026-06-08-20:12:
         Missing sidebar link flags should follow the bundled editor default so new macOS code-server launches start from Ghostex-owned Dark 2026 settings instead of resurrecting local VS Code settings.
         */
        linkVscodeUserConfig: command.linkVscodeUserConfig ?? false,
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
        "level": "error",
        "projectId": command.projectId ?? NSNull(),
      ])
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      /**
       CDXC:EditorPanes 2026-06-06-23:50:
       VS Code server launch failures should surface immediately in the app as a
       toast and project-editor error, while the support log records the same
       failure as an error-level diagnostic after privacy sanitization.
       */
      let failureMessage = sanitizedError.isEmpty ? "Unknown startup error." : sanitizedError
      sendHostEvent(.codeServerRuntimeStartFailed(projectId: command.projectId, message: failureMessage))
      ghostexRootView.logger.error("Failed to start code-server runtime: \(sanitizedError)")
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
    if workspaceView.handleFocusedChromiumZoomShortcut(event) {
      /**
       CDXC:ChromiumBrowserPanes 2026-06-10-15:55:
       CEF panes need Chrome-style Cmd+=, Cmd+-, and Cmd+0 zoom before AppKit's generic hotkey path or embedded Chromium can consume those key equivalents. Keep this rooted in the workspace so only the focused Chromium pane receives the command.
       */
      return true
    }
    if workspaceView.handleFocusedChromiumFindShortcut(event) {
      /**
       CDXC:BrowserSearch 2026-06-13-00:00:
       Focused CEF panes need Chrome-style Cmd+F browser search before AppKit's generic hotkey path routes the same key to terminal search or app-wide actions.
       The workspace resolver scopes this to embedded Chromium hosts so native text fields and modals keep normal editing behavior.
       */
      return true
    }
    let hotkeyText = Self.hotkeyText(for: event)
    if Self.isAppQuitHotkeyText(hotkeyText) {
      /**
       CDXC:MacQuit 2026-06-12-03:09:
       Cmd+Q is a reserved macOS app command, not a configurable Ghostex workspace hotkey. Handle it at the native key boundary so focused terminal, browser, sidebar, or modal responders cannot swallow the standard app-quit shortcut before the menu item runs.
       */
      NSApp.terminate(nil)
      return true
    }
    if Self.isSessionNavigationHotkeyText(hotkeyText) {
      logNativeHotkeyNavigationRepro(
        "appKitObserved",
        [
          "hotkey": hotkeyText ?? "",
          "keyCode": String(event.keyCode),
          "modalActive": String(activeAppModalKind != nil || activeNativeAppModalKind != nil),
          "nativeEditable": String(isNativeEditableFirstResponder()),
          "webChromeFirstResponder": String(isWebChromeFirstResponder()),
        ])
    }
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

       CDXC:Hotkeys 2026-06-07-14:24:
       Cmd+Tab, Cmd+Shift+Tab, Cmd+Shift+[ and Cmd+Shift+] must work even when
       the native sidebar webview owns focus. WebKit does not reliably deliver
       those app-navigation chords, so AppKit may handle those next/previous
       session actions here when no modal/recorder surface is open.

       CDXC:SidebarCollapse 2026-06-12-02:23:
       Cmd+B should also collapse or expand the whole native sidebar from
       sidebar focus, because the collapsed state can hide the WebView that
       would otherwise receive the DOM hotkey path.
       */
      if let hotkeyText,
        let actionId = matchedHotkeyActionId(for: hotkeyText)
      {
        logNativeHotkeyNavigationRepro(
          "webChromeMatch",
          [
            "actionId": actionId,
            "hotkey": hotkeyText,
            "keyCode": String(event.keyCode),
            "modalActive": String(activeAppModalKind != nil || activeNativeAppModalKind != nil),
          ])
        if shouldHandleHotkeyWhileWebChromeOwnsFocus(actionId: actionId) {
          logNativeHotkeyNavigationRepro(
            "webChromeDispatch",
            [
              "actionId": actionId,
              "hotkey": hotkeyText,
              "keyCode": String(event.keyCode),
            ])
          dispatchNativeHotkey(actionId)
          return true
        }
      }
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

  private func shouldUseNativeAppModalWindow(for modal: String?) -> Bool {
    guard let modal else {
      return false
    }
    return !modal.isEmpty
  }

  private func shouldRouteAppModalOpenIntoActiveNativeWindow(modal: String?) -> Bool {
    guard modal == "gitFileDiff" else {
      return false
    }
    /*
     CDXC:TitlebarGit 2026-06-11-20:13:
     Git commit review requests file diffs immediately after opening so the
     right-hand inline diff pane can populate. Do not treat those gitFileDiff
     bridge messages as a new app modal window while gitCommit owns the active
     native modal host, or the child-window router tears down commit review as
     soon as the first diff arrives.
     */
    return activeNativeAppModalKind == "gitCommit"
      || nativeAppModalWindowController?.currentModalKind == "gitCommit"
  }

  private func nativeAppModalWindowControllerIfNeeded() -> AppModalWindowController {
    if let nativeAppModalWindowController {
      return nativeAppModalWindowController
    }
    /*
     CDXC:AppModals 2026-06-11-19:46:
     Settings, Agents Hub, Previous Sessions, and other app dialogs must render in owned native windows instead of a transparent WKWebView over the workspace. Keep one reusable controller that loads the existing React modal host in an AppKit child window.

     CDXC:PromptEditor 2026-06-11-22:51:
     The rich prompt editor now uses this same native modal-window host, with a
     prompt-specific titled/resizable chrome configuration, so no prompt editor
     overlay remains above the main workspace.
     */
    let controller = AppModalWindowController(
      scriptBridge: scriptBridge,
      bootstrapScriptSource: titlebarBootstrapScriptSource,
      diagnosticsScript: Self.diagnosticsScript,
      onClosed: { [weak self] reason, modal in
        self?.nativeAppModalWindowDidClose(reason: reason, modal: modal)
      },
      onContentFrameChanged: { [weak self] modal, contentScreenFrame in
        guard modal == "floatingPromptEditor" else {
          return
        }
        self?.persistFloatingPromptEditorContentScreenFrame(contentScreenFrame)
      })
    nativeAppModalWindowController = controller
    return controller
  }

  @discardableResult
  private func openNativeAppModalWindow(
    message: [String: Any],
    modal: String,
    preferredContentFrame: CGRect? = nil
  ) -> Bool {
    guard let window else {
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeWindow",
        message: "Cannot open native app modal window without a parent window.",
        stack: nil
      )
      return false
    }
    if modal != "floatingPromptEditor", isPrewarmingFloatingPromptEditor {
      cancelFloatingPromptEditorPrewarm(reason: "replacedByAppModal")
    }
    rememberAppModalReturnFocusTarget(modal: modal)
    appModalPresentationPending = true
    activeNativeAppModalKind = nil
    let controller = nativeAppModalWindowControllerIfNeeded()
    controller.open(
      modal: modal,
      message: message,
      parentWindow: window,
      webAssets: Self.resolveWebAssets(),
      latestSidebarState: latestModalHostSidebarState,
      preferredContentFrame: preferredContentFrame)
    updateSidebarModalBackdrop()
    return true
  }

  private func closeNativeAppModalWindow(reason: String, sendReactClose: Bool) {
    if activeNativeAppModalKind == "floatingPromptEditor"
      || nativeAppModalWindowController?.currentModalKind == "floatingPromptEditor"
    {
      if let active = activeFloatingPromptEditor {
        writeFloatingPromptEditorStatusFile(active.statusFile, status: "cancelled")
      }
      finishFloatingPromptEditor(reason: reason)
      return
    }
    let returnFocusSessionId = appModalReturnFocusSessionId
    AppDelegate.appendAgentDetectionDebugLog(
      event: "nativeBridge.appModal.nativeWindow.close",
      details: "reason=\(reason) modal=\(activeNativeAppModalKind ?? "<none>") sendReactClose=\(sendReactClose)"
    )
    nativeAppModalWindowController?.close(sendReactClose: sendReactClose)
    activeNativeAppModalKind = nil
    appModalPresentationPending = false
    updateSidebarModalBackdrop()
    restoreAppModalReturnFocusIfNeeded(sessionId: returnFocusSessionId, reason: reason)
  }

  private func nativeAppModalWindowDidClose(reason: String, modal: String?) {
    if modal == "floatingPromptEditor" {
      if let active = activeFloatingPromptEditor {
        writeFloatingPromptEditorStatusFile(active.statusFile, status: "cancelled")
      }
      finishFloatingPromptEditor(reason: reason, closeNativeWindow: false)
      return
    }
    let returnFocusSessionId = appModalReturnFocusSessionId
    activeNativeAppModalKind = nil
    appModalPresentationPending = false
    updateSidebarModalBackdrop()
    restoreAppModalReturnFocusIfNeeded(sessionId: returnFocusSessionId, reason: reason)
  }

  private func dispatchNativeAppModalWindowMessage(_ message: [String: Any]) {
    nativeAppModalWindowController?.dispatch(message)
  }

  private func dispatchActiveAppModalWindowMessage(_ message: [String: Any]) {
    if nativeAppModalWindowController?.canReceiveMessages == true {
      dispatchNativeAppModalWindowMessage(message)
    }
  }

  private func dispatchFloatingPromptEditorHostMessage(_ message: [String: Any]) {
    if activeNativeAppModalKind == "floatingPromptEditor"
      || nativeAppModalWindowController?.currentModalKind == "floatingPromptEditor"
    {
      dispatchNativeAppModalWindowMessage(message)
    }
  }

  private func closeAppModalHost(reason: String) {
    let returnFocusSessionId = appModalReturnFocusSessionId
    AppDelegate.appendAgentDetectionDebugLog(
      event: "nativeBridge.appModal.close.received",
      details: "reason=\(reason) returnFocusSessionId=\(returnFocusSessionId ?? "<none>") rootModalHostMounted=false"
    )
    guard !isFloatingPromptEditorActiveForUserInput else {
      /*
       CDXC:PromptEditor 2026-06-09-10:43:
       The Ctrl+G Monaco prompt editor is coupled to a terminal process waiting on its save/cancel status file. Generic modal close paths such as sidebar backdrop, Escape routing, bridge close echoes, or toast cleanup must not hide that editor; only the prompt-editor save/cancel handlers may finish it and release the launcher.
       */
      PromptEditorDebugLog.append(
        event: "native.genericCloseIgnored",
        details: [
          "activeAppModalKind": activeAppModalKind ?? "",
          "hasActiveFloatingPromptEditor": activeFloatingPromptEditor != nil,
          "reason": reason,
        ])
      return
    }
    activeAppModalKind = nil
    activeNativeAppModalKind = nil
    appModalPresentationPending = false
    nativeAppModalWindowController?.close(sendReactClose: true)
    updateSidebarModalBackdrop()
    restoreAppModalReturnFocusIfNeeded(sessionId: returnFocusSessionId, reason: reason)
  }

  private func rememberAppModalReturnFocusTarget(modal: String?) {
    guard modal != "floatingPromptEditor" else {
      return
    }
    if appModalReturnFocusSessionId != nil {
      return
    }
    appModalReturnFocusSessionId = workspaceView.appModalReturnFocusTerminalSessionId()
    /**
     CDXC:AppModals 2026-05-28-14:52:
     App modals can take first responder while open. Capture the currently
     focused terminal before presenting so Escape, close buttons, and
     React-driven dismissals can return typing focus to the pane the user was
     using.

     CDXC:AppModals 2026-06-11-23:07:
     This return-focus contract now belongs to native child windows only; the
     main workspace no longer mounts a transparent modal-host WKWebView.
     */
    AppDelegate.appendAgentDetectionDebugLog(
      event: "nativeBridge.appModal.returnFocusCaptured",
      details: "modal=\(modal ?? "unknown") returnFocusSessionId=\(appModalReturnFocusSessionId ?? "<none>")"
    )
  }

  private func restoreAppModalReturnFocusIfNeeded(sessionId: String?, reason: String) {
    guard let sessionId else {
      appModalReturnFocusSessionId = nil
      return
    }
    DispatchQueue.main.async { [weak self] in
      guard let self else {
        return
      }
      guard self.activeAppModalKind == nil,
        self.activeNativeAppModalKind == nil,
        !self.appModalPresentationPending
      else {
        AppDelegate.appendAgentDetectionDebugLog(
          event: "nativeBridge.appModal.returnFocusDeferred",
          details: "reason=\(reason) returnFocusSessionId=\(sessionId) activeAppModalKind=\(self.activeAppModalKind ?? "<none>") activeNativeAppModalKind=\(self.activeNativeAppModalKind ?? "<none>") presentationPending=\(self.appModalPresentationPending) rootModalHostMounted=false"
        )
        return
      }
      self.appModalReturnFocusSessionId = nil
      self.workspaceView.focusTerminal(sessionId: sessionId, reason: "appModalClosed.\(reason)")
    }
  }

  private func updateSidebarModalBackdrop() {
    /**
     CDXC:SidebarLayering 2026-05-23-12:20:
     Only true backdrop modals should cover and block the sidebar.

     CDXC:AppModals 2026-06-11-19:46:
     Non-prompt modals are native child windows now, not root-view backdrops.
     Do not show the sidebar backdrop or workspace shield for those windows;
     their AppKit window frame owns its own input without covering the workspace.

     CDXC:AppModals 2026-06-11-23:07:
     With the main-window modal host removed, keep the old sidebar backdrop
     hidden as well so root chrome never contributes an overlay while a CEF/WK
     editor pane is handling native drag/drop.
     */
    /*
     CDXC:OverlayInteractivity 2026-06-11-21:10:
     Backdrop blocking now belongs to native child windows instead of the main
     root view. Keep the old sidebar backdrop hidden so no root-level overlay
     participates in editor drag/drop hit testing.
     */
    let shouldShowBackdrop = false
    sidebarModalBackdropView.isHidden = !shouldShowBackdrop
    updateWorkspaceInteractionShield()
  }

  private func updateWorkspaceInteractionShield() {
    /**
     CDXC:OverlayInteractivity 2026-05-25-07:02:
     Native pane tabs must not hover, show AppKit tooltips, or receive clicks
     behind Settings-style backdrop modals. Native child windows and native
     toast panels own their exact frames, so keep this root-view shield disabled
     outside surfaces that explicitly publish scoped hit regions.

     CDXC:OverlayInteractivity 2026-05-25-10:09:
     Terminal panes must remain clickable while titlebar dropdown child windows
     are open because those windows no longer cover the workspace.

     CDXC:OverlayInteractivity 2026-06-11-21:10:
     The Source drag/drop harness works without any transparent AppKit layer
     above the embedded browser. Keep this former shield path as a no-op so
     native child windows, not the root view, own modal and dropdown blocking.
     */
    let backdropModalActive = isBackdropAppModalActive()
    let shouldShieldWorkspace = false
    workspaceView.setNativeChromeInteractivitySuppressed(false)
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
    let nativeModalKind = activeNativeAppModalKind ?? "<none>"
    let logKey =
      "shield=\(shouldShieldWorkspace)|modal=\(modalKind)|nativeModal=\(nativeModalKind)|backdrop=\(backdropModalActive)|titlebarOverlay=\(isTitlebarOverlayOpen)|regions=\(titlebarChromeView.hitRegionCount)|belowTitlebarRegions=\(titlebarChromeView.belowTitlebarHitRegionCount)"
    guard logKey != lastWorkspaceInteractionShieldLogKey else {
      return
    }
    lastWorkspaceInteractionShieldLogKey = logKey
    AppDelegate.appendNativeHostLifecycleLog(
      "workspaceInteractionShield.state shouldShield=\(shouldShieldWorkspace) backdropModalActive=\(backdropModalActive) activeAppModalKind=\(modalKind) activeNativeAppModalKind=\(nativeModalKind) titlebarOverlayOpen=\(isTitlebarOverlayOpen) titlebarHitRegionCount=\(titlebarChromeView.hitRegionCount) titlebarBelowTitlebarHitRegionCount=\(titlebarChromeView.belowTitlebarHitRegionCount) rootModalHostMounted=false"
    )
  }

  private func sourceCEFDragOverlaySnapshot() -> [String: Any] {
    /*
     CDXC:SourceCEFDragDrop 2026-06-11-19:40:
     VS Code tab drag/drop works in the isolated CEF/WKWebView repro app but loses `dragover/drop` in the full Ghostex macOS window. Snapshot only overlay visibility, hit-region counts, child-window presence, and geometry at native drag time so diagnostics can rule in or out a remaining transparent overlay without logging user content.
     */
    let childWindows = window?.childWindows ?? []
    return [
      "activeAppModalKind": activeAppModalKind ?? "<none>",
      "activeNativeAppModalKind": activeNativeAppModalKind ?? "<none>",
      "appModalPresentationPending": appModalPresentationPending,
      "childWindowCount": childWindows.count,
      "childWindows": childWindows.map { childWindow in
        [
          "frame": Self.describeFrame(childWindow.frame),
          "isKeyWindow": childWindow.isKeyWindow,
          "isVisible": childWindow.isVisible,
          "level": Int(childWindow.level.rawValue),
          "windowNumber": childWindow.windowNumber,
        ] as [String: Any]
      },
      "isTitlebarOverlayOpen": isTitlebarOverlayOpen,
      "rootModalHostMounted": false,
      "sidebarContextMenuOpenCount": sidebarContextMenuOpenCount,
      "sidebarModalBackdropFrame": Self.describeFrame(sidebarModalBackdropView.frame),
      "sidebarModalBackdropHidden": sidebarModalBackdropView.isHidden,
      "titlebarBelowTitlebarHitRegionCount": titlebarChromeView.belowTitlebarHitRegionCount,
      "titlebarChromeFrame": Self.describeFrame(titlebarChromeView.frame),
      "titlebarDropdownPanel": titlebarDropdownPanelController?.debugSnapshot() ?? ["present": false],
      "titlebarHitRegionCount": titlebarChromeView.hitRegionCount,
      "windowIsKey": window?.isKeyWindow ?? false,
      "windowNumber": window?.windowNumber ?? NSNull(),
      "workspaceInteractionShieldFrame": Self.describeFrame(.zero),
      "workspaceInteractionShieldHidden": true,
    ]
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
    if actionId == "toggleSidebarCollapsed" {
      (window?.contentView as? ghostexRootView)?.toggleSidebarCollapsed()
      return
    }
    if actionId == "openCommandPalette", isCommandPaletteNativeModalOpenOrPending() {
      /*
       CDXC:CommandPalette 2026-06-12-05:45:
       Cmd+K/F12 should toggle the native command palette. Once the palette has
       created its child-window host, repeat openCommandPalette hotkeys close
       that host instead of dispatching another open event into React.
       */
      logNativeHotkeyDebug(
        "nativeHotkeys.commandPaletteToggleClose",
        [
          "activeNativeAppModalKind": activeNativeAppModalKind ?? "",
          "controllerModal": nativeAppModalWindowController?.currentModalKind ?? "",
        ])
      closeNativeAppModalWindow(reason: "commandPaletteHotkeyToggle", sendReactClose: true)
      return
    }
    logNativeHotkeyDebug("nativeHotkeys.dispatchHostEvent", ["actionId": actionId])
    sendHostEvent(.nativeHotkey(actionId: actionId))
  }

  private func isCommandPaletteNativeModalOpenOrPending() -> Bool {
    activeNativeAppModalKind == "commandPalette"
      || nativeAppModalWindowController?.currentModalKind == "commandPalette"
  }

  private func shouldHandleHotkeyWhileWebChromeOwnsFocus(actionId: String) -> Bool {
    if actionId == "openCommandPalette", isCommandPaletteNativeModalOpenOrPending() {
      return true
    }
    guard activeAppModalKind == nil && activeNativeAppModalKind == nil else {
      return false
    }
    return actionId == "toggleSidebarCollapsed" || Self.isSessionNavigationHotkeyActionId(actionId)
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

  private func logNativeHotkeyNavigationRepro(_ phase: String, _ details: [String: Any]) {
    /**
     CDXC:Hotkeys 2026-06-07-14:24:
     A 14:14 repro showed no persistent hotkey breadcrumb while Debugging Mode
     was off. Persist this narrow next/previous-session repro stream in the
     existing sanitized terminal-focus log so a repeated failure can prove
     whether AppKit observed, matched, and dispatched the shortcut without
     writing session titles, project names, paths, URLs, or terminal content.
     */
    var payload = details
    payload["phase"] = phase
    payload["source"] = "appkit"
    TerminalFocusDebugLog.append(
      event: "nativeHotkeys.navigationRepro",
      details: payload,
      force: true)
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

  private static func isSessionNavigationHotkeyActionId(_ actionId: String) -> Bool {
    actionId == "focusNextSession" || actionId == "focusPreviousSession"
  }

  private static func isSessionNavigationHotkeyText(_ hotkeyText: String?) -> Bool {
    guard let hotkeyText else {
      return false
    }
    return hotkeyText == "cmd+tab" || hotkeyText == "cmd+shift+tab"
      || hotkeyText == "cmd+shift+[" || hotkeyText == "cmd+shift+]"
      || hotkeyText == "cmd+shift+{" || hotkeyText == "cmd+shift+}"
  }

  private static func isAppQuitHotkeyText(_ hotkeyText: String?) -> Bool {
    hotkeyText == "cmd+q"
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
    if event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.shift),
      let unshiftedSymbol = shiftedSymbolHotkeyKeys[normalizedCharacters]
    {
      /**
       CDXC:Hotkeys 2026-06-07-14:24:
       Some native paths report shifted bracket keys as "{" or "}" instead of
       the physical "[" or "]" key. Normalize the glyph before matching
       Cmd+Shift+[ and Cmd+Shift+] next/previous-session aliases.
       */
      return unshiftedSymbol
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

  private static let shiftedSymbolHotkeyKeys: [String: String] = [
    "{": "[",
    "}": "]",
  ]

  private struct AppShotCapture {
    let appName: String?
    let bundleIdentifier: String?
    let imagePath: String
    let text: String?
    let title: String?
  }

  fileprivate static func postFrontmostAppShot(trigger: String, to root: ghostexRootView) throws {
    let capture = try captureFrontmostAppShot()
    root.postHostEvent(.appShotCaptured(
      appName: capture.appName,
      bundleIdentifier: capture.bundleIdentifier,
      imagePath: capture.imagePath,
      text: capture.text,
      title: capture.title,
      trigger: trigger
    ))
  }

  private static func captureFrontmostAppShot() throws -> AppShotCapture {
    guard let frontmostApplication = NSWorkspace.shared.frontmostApplication else {
      throw NSError(domain: "GhostexAppShots", code: 1, userInfo: [
        NSLocalizedDescriptionKey: "No frontmost application was available."
      ])
    }
    let pid = frontmostApplication.processIdentifier
    guard let windowInfo = frontmostWindowInfo(for: pid) else {
      throw NSError(domain: "GhostexAppShots", code: 2, userInfo: [
        NSLocalizedDescriptionKey: "No frontmost window was available to capture."
      ])
    }
    let windowId = windowInfo[kCGWindowNumber as String] as? NSNumber
    guard let windowId else {
      throw NSError(domain: "GhostexAppShots", code: 3, userInfo: [
        NSLocalizedDescriptionKey: "The frontmost window could not be identified."
      ])
    }
    guard let cgImage = CGWindowListCreateImage(
      .null,
      [.optionIncludingWindow],
      CGWindowID(windowId.uint32Value),
      [.boundsIgnoreFraming, .bestResolution]
    ) else {
      throw NSError(domain: "GhostexAppShots", code: 4, userInfo: [
        NSLocalizedDescriptionKey: "Screen Recording permission is required to capture app shots."
      ])
    }
    let fileURL = try writeAppShotImage(cgImage)
    let displayPath = displayAppShotPath(for: fileURL)
    let title = (windowInfo[kCGWindowName as String] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
    let text = accessibilityTextForFrontmostWindow(pid: pid)
    return AppShotCapture(
      appName: frontmostApplication.localizedName,
      bundleIdentifier: frontmostApplication.bundleIdentifier,
      imagePath: displayPath,
      text: text,
      title: title?.isEmpty == true ? nil : title
    )
  }

  private static func frontmostWindowInfo(for pid: pid_t) -> [String: Any]? {
    guard let windowInfoList = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)
      as? [[String: Any]]
    else {
      return nil
    }
    return windowInfoList.first { info in
      guard
        (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
        (info[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
        ((info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 1) > 0
      else {
        return false
      }
      let bounds = info[kCGWindowBounds as String] as? [String: Any]
      let width = (bounds?["Width"] as? NSNumber)?.doubleValue ?? 0
      let height = (bounds?["Height"] as? NSNumber)?.doubleValue ?? 0
      return width >= 20 && height >= 20
    }
  }

  private static func writeAppShotImage(_ image: CGImage) throws -> URL {
    let directory = URL(fileURLWithPath: NSHomeDirectory())
      .appendingPathComponent(".ghostex/i", isDirectory: true)
    try FileManager.default.createDirectory(
      at: directory,
      withIntermediateDirectories: true,
      attributes: [.posixPermissions: 0o700]
    )
    let formatter = DateFormatter()
    formatter.dateFormat = "yyMMddHHmmss"
    formatter.locale = Locale(identifier: "en_US_POSIX")
    let fileURL = directory.appendingPathComponent("appshot-\(formatter.string(from: Date())).png")
    let bitmap = NSBitmapImageRep(cgImage: image)
    guard let data = bitmap.representation(using: .png, properties: [:]) else {
      throw NSError(domain: "GhostexAppShots", code: 5, userInfo: [
        NSLocalizedDescriptionKey: "The app shot image could not be encoded."
      ])
    }
    try data.write(to: fileURL, options: [.atomic])
    return fileURL
  }

  private static func displayAppShotPath(for fileURL: URL) -> String {
    let home = URL(fileURLWithPath: NSHomeDirectory()).path
    let path = fileURL.path
    if path == home {
      return "~"
    }
    if path.hasPrefix("\(home)/") {
      return "~\(path.dropFirst(home.count))"
    }
    return path
  }

  private static func accessibilityTextForFrontmostWindow(pid: pid_t) -> String? {
    let appElement = AXUIElementCreateApplication(pid)
    let rootElement = focusedAccessibilityWindow(in: appElement) ?? appElement
    var visited = Set<String>()
    var collected: [String] = []
    collectAccessibilityText(
      from: rootElement,
      depth: 0,
      visited: &visited,
      collected: &collected
    )
    let normalized = collected
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
      .joined(separator: "\n")
      .trimmingCharacters(in: .whitespacesAndNewlines)
    return normalized.isEmpty ? nil : String(normalized.prefix(20_000))
  }

  private static func focusedAccessibilityWindow(in appElement: AXUIElement) -> AXUIElement? {
    var focusedWindow: CFTypeRef?
    if AXUIElementCopyAttributeValue(
      appElement,
      kAXFocusedWindowAttribute as CFString,
      &focusedWindow
    ) == .success,
      let window = focusedWindow
    {
      return unsafeBitCast(window, to: AXUIElement.self)
    }
    var windowsValue: CFTypeRef?
    if AXUIElementCopyAttributeValue(
      appElement,
      kAXWindowsAttribute as CFString,
      &windowsValue
    ) == .success,
      let windows = windowsValue as? [AXUIElement]
    {
      return windows.first
    }
    return nil
  }

  private static func collectAccessibilityText(
    from element: AXUIElement,
    depth: Int,
    visited: inout Set<String>,
    collected: inout [String]
  ) {
    guard depth < 10, collected.count < 600 else {
      return
    }
    let identity = "\(CFHash(element))"
    guard !visited.contains(identity) else {
      return
    }
    visited.insert(identity)

    for attribute in [kAXTitleAttribute, kAXValueAttribute, kAXDescriptionAttribute, kAXHelpAttribute] {
      var value: CFTypeRef?
      guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success,
        let value
      else {
        continue
      }
      if let text = value as? String {
        collected.append(text)
      } else if let attributedText = value as? NSAttributedString {
        collected.append(attributedText.string)
      } else if let number = value as? NSNumber {
        collected.append(number.stringValue)
      }
    }

    var childrenValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(
      element,
      kAXChildrenAttribute as CFString,
      &childrenValue
    ) == .success,
      let children = childrenValue as? [AXUIElement]
    else {
      return
    }
    for child in children.prefix(250) {
      collectAccessibilityText(
        from: child,
        depth: depth + 1,
        visited: &visited,
        collected: &collected
      )
    }
  }

  func presentAppToast(_ command: ShowMessage) {
    presentAppToast(level: Self.appToastLevel(for: command.level), title: command.message)
  }

  func presentAppToast(level: String, title: String, description: String? = nil, interactive: Bool = false) {
    /**
     CDXC:AppToasts 2026-06-07-12:20:
     Native-host status feedback that previously used blocking NSAlert sheets
     should render through non-modal app toasts so Settings, OS Integration, and
     workspace actions stay non-blocking like the sidebar webview.

     CDXC:AppToasts 2026-06-11-21:04:
     Native-host status feedback now renders through AppKit toast panels instead
     of the modal-host WKWebView, so app status messages do not create a web
     overlay above the CEF/WKWebView workspace.
     */
    nativeToastController?.show(
      NativeAppToastRequest(
        level: NativeAppToastLevel(rawValue: level),
        title: title,
        description: description.flatMap { $0.isEmpty ? nil : $0 }))
  }

  private static func appToastLevel(for level: MessageLevel) -> String {
    switch level {
    case .info:
      return "info"
    case .warning:
      return "warning"
    case .error:
      return "error"
    }
  }

  private func showMessage(_ command: ShowMessage) {
    presentAppToast(command)
  }

  func setSidebarSide(_ side: SidebarSide) {
    sidebarSide = side
    workspaceView.setSidebarSide(side)
    needsLayout = true
  }

  func toggleSidebarCollapsed() {
    /**
     CDXC:SidebarCollapse 2026-06-12-02:23:
     The Cmd+B sidebar toggle must collapse the entire native sidebar chrome,
     including the WKWebView, resize divider, and workarea border. Preserve the
     expanded sidebarWidth so the next toggle restores the user's resized width
     instead of treating collapse as a zero-width resize.

     CDXC:SidebarCollapse 2026-06-12-10:57:
     The React titlebar owns the visible sidebar collapse button beside the
     project name, but AppKit owns the actual collapsed layout. Push the native
     collapsed boolean back into the titlebar bridge after every toggle so the
     chevron always shows the next expand/collapse direction.
     */
    isSidebarCollapsed.toggle()
    if isSidebarCollapsed {
      sidebarView.forceNativePointerInside(false)
    }
    setTitlebarSidebarCollapsed(isSidebarCollapsed)
    needsLayout = true
    workspaceView.scheduleZmxPersistenceRefreshForSurfacedTerminalsAfterResize(reason: "sidebarCollapseToggle")
  }

  private func setTitlebarSidebarCollapsed(_ collapsed: Bool) {
    let collapsedLiteral = collapsed ? "true" : "false"
    let json = "{\"sidebarCollapsed\":\(collapsedLiteral)}"
    titlebarChromeWebView.evaluateJavaScript(
      """
      (() => {
        const state = \(json);
        const pending = window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__;
        window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__ =
          pending && typeof pending === "object" ? Object.assign({}, pending, state) : state;
        window.__ghostex_TITLEBAR__?.setActiveProjectState(state);
      })();
      undefined;
      """)
    titlebarDropdownPanelController?.setActiveProjectState(json)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let frames = rootLayoutFrames()
    validateRootLayoutFrames(frames)
    let sidebarVisualFrame = visualSidebarFrame(for: frames)
    let sidebarResizeHitWidth = min(Self.dividerWidth, max(frames.divider.width, 0))
    /**
     CDXC:NativeSidebarChrome 2026-05-31-18:58:
     The native #252525 sidebar/workarea border and resize strip must remain
     owned by AppKit. Keep the divider and border views above the sidebar
     WKWebView so web content cannot interfere with the transparent native drag
     handle.

     CDXC:NativeSidebarChrome 2026-06-04-22:14:
     The reference sidebar must visually reach the workspace edge in the macOS
     app. Paint the sidebar webview under the standard transparent resize
     divider while keeping the divider above it for hit testing and drag
     ownership.

     CDXC:NativeSidebarChrome 2026-06-04-22:20:
     The sidebar webview's excluded hit-test strip must match the fixed native
     divider paint width so dragging continues to resize the sidebar after the
     webview starts painting under that transparent divider.
     */
    let shouldRefreshDividerCursorAfterLayout =
      isSidebarCollapsed && !divider.isHidden && divider.needsCursorRefreshBeforeHide()
    sidebarView.frame = sidebarVisualFrame
    sidebarView.resizeHitExclusionSide = sidebarSide
    sidebarView.resizeHitExclusionWidth = sidebarResizeHitWidth
    divider.frame = frames.divider
    divider.separatorFrame = dividerSeparatorFrame(for: frames)
    sidebarView.isHidden = isSidebarCollapsed
    divider.isHidden = isSidebarCollapsed
    window?.invalidateCursorRects(for: self)
    window?.invalidateCursorRects(for: divider)
    if shouldRefreshDividerCursorAfterLayout {
      divider.refreshCursorAfterVisibilityChange()
    }
    workspaceView.frame = frames.workspace
    nativeToastController?.setLayout(parentWindow: window, rootView: self, anchorFrame: frames.workspace)
    sidebarModalBackdropView.frame = isSidebarCollapsed ? .zero : frames.sidebar.union(frames.divider)
    sidebarWorkareaBorderView.frame = frames.sidebarWorkareaBorder
    if isSidebarCollapsed {
      sidebarWorkareaBorderView.isHidden = true
    }
    workareaTitlebarBorderView.frame = frames.workareaTitlebarBorder
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

  private func visualSidebarFrame(for frames: RootLayoutFrames) -> CGRect {
    /**
     CDXC:NativeSidebarChrome 2026-06-04-22:14:
     The transparent root divider is the source of the visible right-edge gap
     beside left-side sidebars. Extend only the webview's paint frame across
     the fixed divider width; do not include the dynamic workspace edge
     extension because that belongs to split-pane resize ownership.
     */
    let dividerPaintWidth = min(Self.dividerWidth, max(frames.divider.width, 0))
    guard dividerPaintWidth > 0 else {
      return frames.sidebar
    }
    if sidebarSide == .left {
      return CGRect(
        x: frames.sidebar.minX,
        y: frames.sidebar.minY,
        width: frames.sidebar.width + dividerPaintWidth,
        height: frames.sidebar.height)
    }
    return CGRect(
      x: frames.sidebar.minX - dividerPaintWidth,
      y: frames.sidebar.minY,
      width: frames.sidebar.width + dividerPaintWidth,
      height: frames.sidebar.height)
  }

  private func dividerSeparatorFrame(for frames: RootLayoutFrames) -> CGRect? {
    let dividerBounds = CGRect(origin: .zero, size: frames.divider.size)
    let separatorFrame = frames.sidebarWorkareaBorder
      .offsetBy(dx: -frames.divider.minX, dy: -frames.divider.minY)
      .intersection(dividerBounds)
    guard !separatorFrame.isNull, !separatorFrame.isEmpty else {
      return nil
    }
    return separatorFrame
  }

  private func promoteSidebarChrome() {
    /**
     CDXC:SidebarLayering 2026-05-23-01:51:
     App-modal portals historically rendered through a full-window transparent
     WKWebView, but no app overlay should cover the sidebar. Keep the sidebar
     and its resize divider visually above modal/titlebar web layers; app
     toasts now use separate native child panels instead of this z-order path.

     CDXC:NativeSidebarChrome 2026-06-05-05:01:
     The #252525 sidebar/workarea separator must remain visible on the
     sidebar's right edge after both shrink and expand drags. The sidebar
     WKWebView now paints under the transparent resize divider, so the native
     resize handle draws the separator inside the same AppKit view that owns
     the drag gesture, resize cursor, and delayed hover affordance.

     CDXC:NativeSidebarChrome 2026-06-08-19:58:
     Z-order-only cursor fixes were not reliable enough for the macOS sidebar
     boundary. Keep the previous divider-before-border ordering while making
     the visible boundary belong to PaneResizeHandleView, not the hidden
     standalone border view.

     CDXC:NativeSidebarChrome 2026-06-13-07:26:
     The sidebar divider must match the stable workspace splitter model: only
     the concrete PaneResizeHandleView owns resize cursor rects. Do not register
     a parent/root cursor rect over the same frame, because that second owner can
     leave the resize cursor active after the pointer leaves the divider.
     */
    addSubview(sidebarView, positioned: .above, relativeTo: titlebarChromeView)
    addSubview(divider, positioned: .above, relativeTo: sidebarView)
    addSubview(sidebarWorkareaBorderView, positioned: .above, relativeTo: divider)
    addSubview(sidebarModalBackdropView, positioned: .above, relativeTo: divider)
  }

  /*
   CDXC:RootHitBoundaries 2026-06-11-21:10:
   The root view intentionally keeps normal AppKit traversal for broad
   workspace, titlebar, modal, and embedded browser hits. Browser-native
   drag/drop in Source should see the same ordinary view ownership model as the
   minimal CEF/WKWebView harness.

   CDXC:NativePaneResize 2026-06-12-03:18:
   The 03:15 companion resize repro still showed the five-point rail receiving
   hover while root hit testing selected CEF/Ghostty descendants on mouseDown.
   Let the root view ask TerminalWorkspaceView's mounted resize-handle views
   before ordinary child traversal so the concrete rail owns drag start without
   widening the rail or routing by cached pane geometry.

   CDXC:NativePaneTabClicks 2026-06-12-06:33:
   Split-pane tab bars can be visible and AX-visible while a sibling embedded
   surface wins root hit testing before TerminalWorkspaceView gets to enforce
   pane-titlebar ownership. Give mounted workspace titlebars the same root
   pre-pass as resize rails so tab activation, tab Close, and tab-bar action
   buttons are owned by the visible native titlebar under the pointer.

   CDXC:SidebarPlacement 2026-06-13-08:14:
   Right-side sidebar clicks must not enter workspace prepasses before the
   sidebar webview has a chance to claim visible content. Keep this as a narrow
   sidebar-content prepass that still yields the native resize divider strip.
   */
  override func hitTest(_ point: NSPoint) -> NSView? {
    if let sidebarHitView = sidebarContentHitView(at: point) {
      return sidebarHitView
    }
    if let resizeHandleHitView = workspaceResizeHandleHitView(at: point) {
      return resizeHandleHitView
    }
    if let paneTitleBarHitView = workspacePaneTitleBarHitView(at: point) {
      return paneTitleBarHitView
    }
    return super.hitTest(point)
  }

  private func sidebarContentHitView(at point: NSPoint) -> NSView? {
    /*
     CDXC:SidebarPlacement 2026-06-13-08:14:
     Right-side sidebar content must remain clickable after the webview moves to
     the trailing edge. Resolve visible sidebar content before workspace resize
     and pane-titlebar prepasses, while still yielding the native divider strip
     so resizing keeps its single AppKit owner.
     */
    guard !isSidebarCollapsed, !sidebarView.isHidden, sidebarView.frame.contains(point) else {
      return nil
    }
    let sidebarPoint = convert(point, to: sidebarView)
    guard sidebarView.containsInteractiveHitPoint(sidebarPoint) else {
      return nil
    }
    return sidebarView.hitTest(sidebarPoint) ?? sidebarView
  }

  private func workspaceResizeHandleHitView(at point: NSPoint) -> NSView? {
    guard !workspaceView.isHidden, workspaceView.frame.contains(point) else {
      return nil
    }
    return workspaceView.resizeHandleHitView(at: convert(point, to: workspaceView))
  }

  private func workspacePaneTitleBarHitView(at point: NSPoint) -> NSView? {
    guard !workspaceView.isHidden, workspaceView.frame.contains(point) else {
      return nil
    }
    let workspacePoint = convert(point, to: workspaceView)
    guard let hitView = workspaceView.paneTitleBarHitView(at: workspacePoint) else {
      return nil
    }
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.root.hitTest.titleBarPrepass", details: [
      "hitView": String(describing: type(of: hitView)),
      "rootPoint": ["x": Double(point.x), "y": Double(point.y)],
      "workspaceFrame": [
        "height": Double(workspaceView.frame.height),
        "maxX": Double(workspaceView.frame.maxX),
        "maxY": Double(workspaceView.frame.maxY),
        "minX": Double(workspaceView.frame.minX),
        "minY": Double(workspaceView.frame.minY),
        "width": Double(workspaceView.frame.width),
      ],
      "workspacePoint": ["x": Double(workspacePoint.x), "y": Double(workspacePoint.y)],
    ])
    return hitView
  }

  private func rootLayoutFrames() -> RootLayoutFrames {
    let maxSidebarWidth = currentMaxSidebarWidth()
    let minSidebarWidth = currentSidebarMinWidth()
    let sidebarWidth = min(max(self.sidebarWidth, minSidebarWidth), maxSidebarWidth)
    self.sidebarWidth = sidebarWidth
    let workspaceBarWidth = currentWorkspaceBarWidth()
    let contentHeight = max(bounds.height - Self.reactTitlebarHeight, 1)
    let titlebarChromeFrame = CGRect(
      x: bounds.minX,
      y: max(bounds.maxY - Self.reactTitlebarHeight, bounds.minY),
      width: bounds.width,
      height: min(Self.reactTitlebarHeight, max(bounds.height, 0)))
    if isSidebarCollapsed {
      /**
       CDXC:SidebarCollapse 2026-06-12-02:23:
       Collapsed sidebar layout must reserve no sidebar, divider, or workarea-border width. Keep only the titlebar strip and let the workspace fill the whole content width.
       */
      let workspaceFrame = CGRect(
        x: bounds.minX,
        y: 0,
        width: max(bounds.width, 1),
        height: contentHeight
      )
      let separatorWidth = Self.workareaSeparatorWidth
      let workareaTitlebarBorderFrame = CGRect(
        x: workspaceFrame.minX,
        y: max(workspaceFrame.maxY - separatorWidth, workspaceFrame.minY),
        width: workspaceFrame.width,
        height: min(separatorWidth, workspaceFrame.height)
      )
      return RootLayoutFrames(
        divider: .zero,
        sidebar: .zero,
        sidebarWorkareaBorder: .zero,
        titlebarChrome: titlebarChromeFrame,
        workareaTitlebarBorder: workareaTitlebarBorderFrame,
        workspace: workspaceFrame)
    }
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
    let separatorWidth = Self.workareaSeparatorWidth
    let sidebarWorkareaBorderX: CGFloat
    if sidebarSide == .left {
      sidebarWorkareaBorderX = max(workspaceFrame.minX - separatorWidth, 0)
    } else {
      sidebarWorkareaBorderX = min(workspaceFrame.maxX, max(bounds.width - separatorWidth, 0))
    }
    let sidebarWorkareaBorderFrame = CGRect(
      x: sidebarWorkareaBorderX,
      y: workspaceFrame.minY,
      width: separatorWidth,
      height: workspaceFrame.height
    )
    let workareaTitlebarBorderFrame = CGRect(
      x: workspaceFrame.minX,
      y: max(workspaceFrame.maxY - separatorWidth, workspaceFrame.minY),
      width: workspaceFrame.width,
      height: min(separatorWidth, workspaceFrame.height)
    )
    /**
     CDXC:RootHitBoundaries 2026-05-12-09:58
     The titlebar WKWebView originally kept a full-window visual frame so
     portaled tooltips and dropdowns were not clipped. Native hit-testing was
     the click boundary.

     CDXC:ReactTitlebar 2026-06-11-13:22:
     Titlebar dropdowns now render in native child windows, so the main
     titlebar WKWebView must be clipped to the fixed titlebar strip and never
     cover the CEF/WKWebView workspace during editor drag/drop.
     */
    return RootLayoutFrames(
      divider: dividerFrame,
      sidebar: sidebarFrame,
      sidebarWorkareaBorder: sidebarWorkareaBorderFrame,
      titlebarChrome: titlebarChromeFrame,
      workareaTitlebarBorder: workareaTitlebarBorderFrame,
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
    let resetWidth = nativeSettingsStore.readSidebarDefaultWidth() ?? Self.sidebarResetWidth
    sidebarWidth = min(
      max(resetWidth, currentSidebarMinWidth()),
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
      /*
       CDXC:PromptEditor 2026-06-12-04:37:
       React prompt-editor speed diagnostics are emitted as JSON with
       privacy-safe fields such as request ids, durations, booleans, and text
       lengths. Parse that JSON before writing so the native sanitizer can
       preserve useful structured timings instead of redacting the whole
       details string as opaque user text.
       */
      if let details = promptEditorDebugLogDetails(from: message["details"]) {
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
        event: "nativeBridge.appModal.nativeWindow.ready",
        details: "hasLatestState=\(latestModalHostSidebarState != nil)"
      )
      nativeAppModalWindowController?.hostReady(latestSidebarState: latestModalHostSidebarState)
    case "open":
      /**
       CDXC:AppModals 2026-04-28-12:06
       Persistent helper mode was removed, so modal presentation no longer
       pauses or resurfaces external terminal windows.

       CDXC:AppModals 2026-06-11-23:07:
       App modal opens must create native child windows only. Do not queue or
       dispatch into a transparent root WKWebView above the workspace because
       that sibling overlay breaks browser-native VS Code tab drag/drop in CEF
       and WKWebView editor panes.
       */
      let requestedModal = message["modal"] as? String
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.open.received",
        details:
          "modal=\(requestedModal ?? "unknown") hasLatestState=\(latestModalHostSidebarState != nil) rootModalHostMounted=false"
      )
      if isFloatingPromptEditorActiveForUserInput,
        requestedModal != "floatingPromptEditor"
      {
        /*
         CDXC:PromptEditor 2026-06-09-10:43:
         While Ctrl+G Monaco prompt editing is open, other sidebar or titlebar modal requests must wait until the user saves or cancels the editor. Replacing the single modal-host active modal would make the editor disappear while its launcher still waits for a status file.
         */
        PromptEditorDebugLog.append(
          event: "native.genericOpenIgnored",
          details: [
            "activeAppModalKind": activeAppModalKind ?? "",
            "hasActiveFloatingPromptEditor": activeFloatingPromptEditor != nil,
            "requestedModal": requestedModal ?? "unknown",
          ])
        return
      }
      if shouldRouteAppModalOpenIntoActiveNativeWindow(modal: requestedModal) {
        AppDelegate.appendAgentDetectionDebugLog(
          event: "nativeBridge.appModal.open.routeActiveNativeWindow",
          details: "modal=\(requestedModal ?? "unknown") activeNativeAppModalKind=\(activeNativeAppModalKind ?? "<none>")"
        )
        dispatchNativeAppModalWindowMessage(message)
        return
      }
      if shouldUseNativeAppModalWindow(for: requestedModal), let requestedModal {
        openNativeAppModalWindow(message: message, modal: requestedModal)
        return
      }
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeBridge",
        message: "Ignored app modal open without a native-window modal id.",
        stack: nil
      )
    case "presented":
      let modal = message["modal"] as? String
      let requestId = message["requestId"] as? String
      AppDelegate.appendAgentDetectionDebugLog(
        event: "nativeBridge.appModal.nativeWindow.presented",
        details: "modal=\(modal ?? "unknown")"
      )
      if modal == "floatingPromptEditor" {
        if let requestId,
          activeFloatingPromptEditor?.requestId != requestId
        {
          PromptEditorDebugLog.append(
            event: "native.presented.ignored",
            details: [
              "activeRequestId": activeFloatingPromptEditor?.requestId ?? "",
              "messageRequestId": requestId,
              "reason": "staleRequest",
            ]
          )
          return
        }
        PromptEditorDebugLog.append(
          event: "native.presented",
          details: [
            "isPrewarming": isPrewarmingFloatingPromptEditor,
            "rootModalHostMounted": false,
            "requestId": activeFloatingPromptEditor?.requestId ?? "",
          ]
        )
      }
      if isPrewarmingFloatingPromptEditor {
        return
      }
      appModalPresentationPending = false
      activeNativeAppModalKind = modal
      nativeAppModalWindowController?.presentIfCurrent(modal: modal)
      updateSidebarModalBackdrop()
    case "close":
      closeNativeAppModalWindow(reason: "bridgeMessage", sendReactClose: false)
    case "toast":
      /**
       CDXC:Worktrees 2026-05-18-23:07:
       Worktree and git progress messages are transient app toasts. They should
       remain non-modal status feedback instead of blocking terminal panes.

       CDXC:AppModals 2026-05-23-01:51:
       Historical toast-only modal-host visibility had to be visual, not
       interactive. Native toasts remove that web overlay entirely; normal toast
       panels ignore mouse events and action panels are exact-sized.

       CDXC:AppToasts 2026-06-11-21:04:
       Toast bridge messages now terminate in the native toast controller. Do
       not dispatch them into the modal-host WKWebView or reveal that overlay;
       only the exact-sized NSPanel toast should sit above the workspace.
       */
      if let request = NativeAppToastRequest(message: message) {
        nativeToastController?.show(request)
      }
      updateSidebarModalBackdrop()
    case "toastDismissed":
      updateSidebarModalBackdrop()
    case "pickRepositoryFolder":
      presentRepositoryFolderPicker(initialPath: message["initialPath"] as? String)
    case "pickWorktreeImages":
      presentWorktreeImagePicker()
    case "sidebarState":
      latestModalHostSidebarState = message
      dispatchNativeAppModalWindowMessage(message)
    case "projectWorktreesResult", "repositoryCloneResult", "repositoryClonePreviewResult",
      "remoteProjectDirectoryBrowseResult", "remoteProjectAddResult":
      /**
       CDXC:WorktreeProjectRegistration 2026-06-01-21:33:
       The Add Worktree modal asks the sidebar webview to list existing Git
       worktrees, then the sidebar sends this result back through the native
       modal bridge. Forward the result into the modal host instead of treating
       it as an unknown bridge command, otherwise the Open Existing selector
       remains stuck in its loading state.
       */
      dispatchActiveAppModalWindowMessage(message)
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
      if handleNativeSidebarModalCommand(sidebarMessage) {
        return
      }
      dispatchSidebarModalCommand(sidebarMessage)
    default:
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeBridge",
        message: "Unknown modal host message type: \(type)",
        stack: nil
      )
    }
  }

  private func handleNativeSidebarModalCommand(_ message: Any) -> Bool {
    guard let command = message as? [String: Any],
      command["type"] as? String == "runGhostexHotkeyAction",
      command["actionId"] as? String == "toggleSidebarCollapsed"
    else {
      return false
    }
    /**
     CDXC:SidebarCollapse 2026-06-12-10:57:
     The native Command Palette runs inside an app-modal child WKWebView, so
     its sidebarCommand envelope cannot rely on the sidebar webview to perform
     native-only chrome actions. Handle Toggle Sidebar at the AppKit bridge so
     the palette command collapses or expands the full native sidebar exactly
     like Cmd+B.
     */
    closeNativeAppModalWindow(reason: "commandPaletteToggleSidebar", sendReactClose: true)
    (window?.contentView as? ghostexRootView)?.toggleSidebarCollapsed()
    return true
  }

  private func promptEditorDebugLogDetails(from rawDetails: Any?) -> [String: Any]? {
    guard let details = rawDetails as? String,
      !details.isEmpty,
      let data = details.data(using: .utf8),
      let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return nil
    }
    return normalizePromptEditorDebugLogPayload(payload)
  }

  private func normalizePromptEditorDebugLogPayload(_ payload: [String: Any]) -> [String: Any] {
    payload.mapValues { normalizePromptEditorDebugLogValue($0) }
  }

  private func normalizePromptEditorDebugLogValue(_ value: Any) -> Any {
    if let number = value as? NSNumber {
      if CFGetTypeID(number) == CFBooleanGetTypeID() {
        return number.boolValue
      }
      let doubleValue = number.doubleValue
      if doubleValue.rounded(.towardZero) == doubleValue {
        return Int64(doubleValue)
      }
      return doubleValue
    }
    if let array = value as? [Any] {
      return array.map { normalizePromptEditorDebugLogValue($0) }
    }
    if let dictionary = value as? [String: Any] {
      return normalizePromptEditorDebugLogPayload(dictionary)
    }
    return value
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
      self?.dispatchAppModalResponse([
        "paths": panel.urls.map(\.path),
        "type": "worktreeImageFilesPicked",
      ])
    }

    if let sheetWindow = appModalSheetWindow() {
      panel.beginSheetModal(for: sheetWindow, completionHandler: completion)
    } else {
      completion(panel.runModal())
    }
  }

  private func presentRepositoryFolderPicker(initialPath: String?) {
    /**
     CDXC:AddRepository 2026-05-29-11:45:
     The Clone Repository modal owns clone configuration, but folder selection
     must still use a trusted native directory picker. Return only the selected
     parent path to the modal host, which persists it as the app-wide last clone
     location.
     */
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.allowsMultipleSelection = false
    panel.canCreateDirectories = true
    panel.prompt = "Choose"
    panel.message = "Choose where to clone the repository."
    if let initialPath = initialPath?.trimmingCharacters(in: .whitespacesAndNewlines),
      !initialPath.isEmpty
    {
      let expandedPath =
        initialPath == "~"
        ? FileManager.default.homeDirectoryForCurrentUser.path
        : initialPath.replacingOccurrences(
          of: "~/",
          with: "\(FileManager.default.homeDirectoryForCurrentUser.path)/",
          options: [.anchored])
      panel.directoryURL = URL(fileURLWithPath: expandedPath, isDirectory: true)
    }

    let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
      guard response == .OK,
        let url = panel.url
      else {
        return
      }
      self?.dispatchAppModalResponse([
        "path": url.path,
        "type": "repositoryFolderPicked",
      ])
    }

    if let sheetWindow = appModalSheetWindow() {
      panel.beginSheetModal(for: sheetWindow, completionHandler: completion)
    } else {
      completion(panel.runModal())
    }
  }

  private func appModalSheetWindow() -> NSWindow? {
    nativeAppModalWindowController?.window ?? window
  }

  private func dispatchAppModalResponse(_ message: [String: Any]) {
    dispatchNativeAppModalWindowMessage(message)
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
      guard NativeProcessRegistry.shared.register(requestId: command.requestId, process: process) else {
        let result = HostEvent.processResult(
          requestId: command.requestId,
          exitCode: 130,
          stdout: "",
          stderr: "Process canceled."
        )
        await MainActor.run { [weak self] in
          guard let self else {
            return
          }
          self.postHostEvent(result)
        }
        return
      }
      let outputLock = NSLock()
      var stdoutData = Data()
      var stderrData = Data()
      let stdoutHandle = stdoutPipe.fileHandleForReading
      let stderrHandle = stderrPipe.fileHandleForReading
      /**
       CDXC:AgentsHub 2026-05-14-08:43
       Agents Hub process helpers can return real profile, skill, hook, and
       config data. Drain output while the command is running so stdout/stderr
       cannot fill the pipe and block the helper before native posts
       processResult back to the webview.

       CDXC:AgentsHub 2026-06-12-02:53
       The catalog helper now returns metadata only, while selected file
       contents use a separate smaller request. Keep pipe draining here because
       save/read helpers still return user-editable buffers and diagnostics.
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
        if NativeProcessRegistry.shared.isCanceled(requestId: command.requestId) {
          process.terminate()
        }
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
        NativeProcessRegistry.shared.unregister(requestId: command.requestId)
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
        let sanitizedURL = NativeLogPrivacy.sanitizeLogLine(url.absoluteString)
        Self.logger.info("Loading sidebar URL \(sanitizedURL, privacy: .public)")
      }
      sidebarView.load(URLRequest(url: url))
      return
    }

    let webAssets = Self.resolveWebAssets()
    let builtSidebar = webAssets.appendingPathComponent("index.html")
    if FileManager.default.fileExists(atPath: builtSidebar.path) {
      if NativeDebugLogging.isEnabled {
        let sanitizedPath = NativeLogPrivacy.sanitizeLogLine(builtSidebar.path)
        Self.logger.info("Loading built sidebar from \(sanitizedPath, privacy: .public)")
      }
      sidebarView.loadFileURL(builtSidebar, allowingReadAccessTo: webAssets)
      return
    }

    let sanitizedSidebarPath = NativeLogPrivacy.sanitizeLogLine(builtSidebar.path)
    Self.logger.error("Built sidebar not found at \(sanitizedSidebarPath, privacy: .public)")
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

  private func loadTitlebarChrome() {
    let webAssets = Self.resolveWebAssets()
    let builtTitlebarChrome = webAssets.appendingPathComponent("titlebar-host.html")
    if FileManager.default.fileExists(atPath: builtTitlebarChrome.path) {
      if NativeDebugLogging.isEnabled {
        let sanitizedPath = NativeLogPrivacy.sanitizeLogLine(builtTitlebarChrome.path)
        Self.logger.info("Loading React titlebar chrome from \(sanitizedPath, privacy: .public)")
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
    let sanitizedTitlebarChromePath = NativeLogPrivacy.sanitizeLogLine(builtTitlebarChrome.path)
    Self.logger.error("Built React titlebar chrome not found at \(sanitizedTitlebarChromePath, privacy: .public)")
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
               together at the top-right of the workspace button, ordered #95d7f6
               then orange from left to right. The orange badge uses "working"
               to match session-card activity and avoid overloading "active".
               CDXC:WorkspaceDock 2026-06-12-02:32: Done badges use #95d7f6 instead of the previous green so workspace status matches macOS sidebar, Android, and iOS done/attention indicators. */
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
            background: #95d7f6;
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
  var onTitlebarMouseEvent: ((NSEvent) -> Bool)?
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
    /*
     CDXC:ReactTitlebar 2026-06-11-22:10:
     Real mouse clicks in the full-size content titlebar strip can be consumed
     by AppKit's window-chrome dispatch before content hit-testing reaches the
     titlebar WKWebView. Give the clipped React titlebar one narrow pre-super
     routing hook so measured controls receive normal WebKit mouse events
     without reinstating any workspace/CEF hit-test overlay.
     */
    if onTitlebarMouseEvent?(event) == true {
      if shouldReportActivationBoundaryEvent && Self.isMouseActivationBoundaryEvent(event) {
        onActivationBoundaryEvent?(event, "windowSendEvent.titlebarRerouted")
      }
      return
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

final class NonInteractiveChromeLineView: NSView {
  var lineColor: NSColor = .clear {
    didSet {
      layer?.backgroundColor = lineColor.cgColor
    }
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = lineColor.cgColor
  }

  required init?(coder: NSCoder) {
    super.init(coder: coder)
    wantsLayer = true
    layer?.backgroundColor = lineColor.cgColor
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }
}

final class NativeResizeHoverIndicator {
  enum LineAxis {
    case horizontal
    case vertical
  }

  private static let hoverDelay: TimeInterval = 0.05
  private static let fadeDuration: TimeInterval = 0.18
  private static let lineWidth: CGFloat = 3
  private static let lineColor = NSColor.white.cgColor

  private let lineLayer = CALayer()
  private var explicitLineFrame: CGRect?
  private var hoverTimer: Timer?
  private var isHovering = false
  private var lineAxis: LineAxis
  private var trackingArea: NSTrackingArea?
  private weak var trackingView: NSView?

  init(lineAxis: LineAxis) {
    self.lineAxis = lineAxis
    lineLayer.backgroundColor = Self.lineColor
    lineLayer.opacity = 0
    lineLayer.isHidden = true
  }

  deinit {
    hoverTimer?.invalidate()
  }

  func configure(lineAxis: LineAxis, explicitLineFrame: CGRect? = nil, in view: NSView) {
    /**
     CDXC:ResizeHoverAffordance 2026-06-09-14:34:
     Every native resize drag line should reveal a 3px hover affordance after a short delay, then fade it in instead of drawing an always-visible rail. Keep the behavior as a visual layer on the native handle so drag delivery stays unchanged.

     CDXC:ResizeHoverAffordance 2026-06-09-14:48:
     Keep the hover line as supplemental resize feedback without changing resize hit geometry.

     CDXC:ResizeHoverAffordance 2026-06-09-17:10:
     The resize hover line color should be #fff so the affordance reads clearly against the native dark workspace.

     CDXC:ResizeHoverAffordance 2026-06-09-15:32:
     The line-only affordance did not look right for native resize rails. Restore the AppKit resize cursor while preserving the delayed hover line.

     CDXC:ResizeHoverAffordance 2026-06-09-15:37:
     The hover line should reveal quickly after a 50ms delay so resize feedback feels immediate while still fading in.
     */
    self.lineAxis = lineAxis
    self.explicitLineFrame = explicitLineFrame
    layout(in: view)
  }

  func updateTrackingArea(in view: NSView) {
    if let trackingArea, let trackingView {
      trackingView.removeTrackingArea(trackingArea)
    }
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeAlways, .inVisibleRect, .mouseEnteredAndExited],
      owner: view,
      userInfo: nil
    )
    self.trackingArea = trackingArea
    trackingView = view
    view.addTrackingArea(trackingArea)
  }

  func layout(in view: NSView) {
    ensureLayer(in: view)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    lineLayer.frame = resolvedLineFrame(in: view)
    CATransaction.commit()
    refreshHoverState(in: view)
  }

  func mouseEntered(in view: NSView) {
    beginHover(in: view)
  }

  func mouseExited(in view: NSView) {
    guard !pointerIsInside(view) else {
      return
    }
    cancel(in: view)
  }

  func cancel(in view: NSView? = nil) {
    hoverTimer?.invalidate()
    hoverTimer = nil
    isHovering = false
    hideImmediately()
    if let view {
      layoutWithoutRefreshingHover(in: view)
    }
  }

  private func refreshHoverState(in view: NSView) {
    guard isHandleVisible(view) else {
      cancel()
      return
    }
    if pointerIsInside(view) {
      beginHover(in: view)
    } else if isHovering {
      cancel(in: view)
    }
  }

  private func beginHover(in view: NSView) {
    guard isHandleVisible(view) else {
      cancel()
      return
    }
    ensureLayer(in: view)
    layoutWithoutRefreshingHover(in: view)
    guard !isHovering else {
      return
    }
    isHovering = true
    hoverTimer?.invalidate()
    let timer = Timer(timeInterval: Self.hoverDelay, repeats: false) { [weak self, weak view] _ in
      guard let self, let view else {
        return
      }
      self.revealIfStillHovering(in: view)
    }
    hoverTimer = timer
    RunLoop.main.add(timer, forMode: .common)
  }

  private func revealIfStillHovering(in view: NSView) {
    hoverTimer = nil
    guard isHovering, isHandleVisible(view), pointerIsInside(view) else {
      cancel(in: view)
      return
    }
    layoutWithoutRefreshingHover(in: view)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    lineLayer.isHidden = false
    lineLayer.opacity = 0
    CATransaction.commit()

    CATransaction.begin()
    CATransaction.setAnimationDuration(Self.fadeDuration)
    CATransaction.setAnimationTimingFunction(CAMediaTimingFunction(name: .easeOut))
    lineLayer.opacity = 1
    CATransaction.commit()
  }

  private func hideImmediately() {
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    lineLayer.removeAllAnimations()
    lineLayer.opacity = 0
    lineLayer.isHidden = true
    CATransaction.commit()
  }

  private func ensureLayer(in view: NSView) {
    view.wantsLayer = true
    guard let hostLayer = view.layer, lineLayer.superlayer !== hostLayer else {
      return
    }
    lineLayer.removeFromSuperlayer()
    hostLayer.addSublayer(lineLayer)
  }

  private func layoutWithoutRefreshingHover(in view: NSView) {
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    lineLayer.frame = resolvedLineFrame(in: view)
    CATransaction.commit()
  }

  private func resolvedLineFrame(in view: NSView) -> CGRect {
    let bounds = view.bounds
    guard bounds.width > 0, bounds.height > 0 else {
      return .zero
    }
    if let explicitLineFrame {
      let clamped = explicitLineFrame.intersection(bounds)
      if !clamped.isNull, !clamped.isEmpty {
        return clamped
      }
    }
    switch lineAxis {
    case .horizontal:
      return CGRect(
        x: bounds.minX,
        y: bounds.midY - Self.lineWidth / 2,
        width: bounds.width,
        height: Self.lineWidth)
    case .vertical:
      return CGRect(
        x: bounds.midX - Self.lineWidth / 2,
        y: bounds.minY,
        width: Self.lineWidth,
        height: bounds.height)
    }
  }

  private func isHandleVisible(_ view: NSView) -> Bool {
    !view.isHidden && view.alphaValue > 0 && view.bounds.width > 0 && view.bounds.height > 0
  }

  private func pointerIsInside(_ view: NSView) -> Bool {
    guard let window = view.window else {
      return false
    }
    return view.bounds.contains(view.convert(window.mouseLocationOutsideOfEventStream, from: nil))
  }
}

final class PaneResizeHandleView: NSView {
  var onDrag: ((CGFloat) -> Void)?
  var onDragEnded: (() -> Void)?
  var onDoubleClick: (() -> Void)?
  var onPointerEntered: (() -> Void)?
  var separatorColor: NSColor = .clear {
    didSet {
      needsDisplay = true
    }
  }
  var separatorFrame: CGRect? {
    didSet {
      if oldValue != separatorFrame {
        needsDisplay = true
        updateResizeHoverIndicator()
      }
    }
  }
  private let resizeHoverIndicator = NativeResizeHoverIndicator(lineAxis: .vertical)
  private var isResizeDragging = false
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

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    resizeHoverIndicator.updateTrackingArea(in: self)
  }

  override func layout() {
    super.layout()
    updateResizeHoverIndicator()
  }

  override func mouseEntered(with event: NSEvent) {
    onPointerEntered?()
    resizeHoverIndicator.mouseEntered(in: self)
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.mouseEntered",
      event: event,
      cursorAction: "none")
  }

  override func mouseExited(with event: NSEvent) {
    resizeHoverIndicator.mouseExited(in: self)
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.mouseExited",
      event: event,
      cursorAction: isResizeDragging ? "skipActiveDrag" : "refresh")
    if !isResizeDragging {
      refreshCursorForCurrentPointer(reason: "mouseExited", event: event)
    }
  }

  override func resetCursorRects() {
    super.resetCursorRects()
    /**
     CDXC:NativeSidebarChrome 2026-06-09-15:32:
     Sidebar resize keeps the AppKit left-right resize cursor on the concrete handle view while the hover layer provides the delayed visual line.

     CDXC:NativeSidebarChrome 2026-06-13-03:40:
     The sidebar divider must release the resize cursor when pointer ownership
     leaves the visible rail or the resize drag ends. Keep the cursor rect for
     normal hover, but refresh the current cursor explicitly on exit/up/reset/hide.
     */
    addCursorRect(bounds, cursor: .resizeLeftRight)
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.resetCursorRects",
      event: nil,
      cursorAction: "addCursorRectResizeLeftRight")
  }

  /**
   CDXC:NativeSidebarChrome 2026-04-26-07:27
   The resize hit target stays wide enough to drag comfortably, but the
   visible sidebar edge is intentionally transparent so the reference sidebar
   does not show a light vertical drag-area separator.
   */
  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    guard let separatorFrame else {
      return
    }
    let visibleSeparatorFrame = separatorFrame.intersection(bounds)
    guard !visibleSeparatorFrame.isNull, !visibleSeparatorFrame.isEmpty else {
      return
    }
    separatorColor.setFill()
    visibleSeparatorFrame.fill()
  }

  private func updateResizeHoverIndicator() {
    let lineFrame = resizeHoverLineFrame()
    resizeHoverIndicator.configure(lineAxis: .vertical, explicitLineFrame: lineFrame, in: self)
  }

  private func resizeHoverLineFrame() -> CGRect? {
    guard let separatorFrame else {
      return nil
    }
    return CGRect(
      x: separatorFrame.midX - 1.5,
      y: bounds.minY,
      width: 3,
      height: bounds.height)
  }

  private func refreshCursorForCurrentPointer(reason: String, event: NSEvent? = nil) {
    /*
     CDXC:NativeSidebarChrome 2026-06-13-03:40:
     Sidebar resize cursor ownership belongs only to the visible AppKit divider.
     If a drag, reset, collapse, or pointer exit leaves the cursor outside the
     divider, return to the default cursor immediately instead of waiting for a
     later AppKit cursor-rect update.
     */
    let pointerInside = isCurrentPointerInsideVisibleHandle()
    guard pointerInside else {
      NSCursor.arrow.set()
      appendSidebarResizeCursorLog(
        "nativeSidebarResize.handle.cursorRefresh",
        event: event,
        reason: reason,
        pointerInside: pointerInside,
        cursorAction: "setArrow")
      return
    }
    NSCursor.resizeLeftRight.set()
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.cursorRefresh",
      event: event,
      reason: reason,
      pointerInside: pointerInside,
      cursorAction: "setResizeLeftRight")
  }

  func needsCursorRefreshBeforeHide() -> Bool {
    let needsRefresh = isResizeDragging || isCurrentPointerInsideVisibleHandle()
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.needsCursorRefreshBeforeHide",
      event: nil,
      extra: ["needsRefresh": needsRefresh],
      cursorAction: "none")
    return needsRefresh
  }

  func refreshCursorAfterVisibilityChange() {
    refreshCursorForCurrentPointer(reason: "visibilityChange")
  }

  private func isCurrentPointerInsideVisibleHandle() -> Bool {
    guard let window,
      superview != nil,
      !isHidden,
      alphaValue > 0,
      bounds.width > 0,
      bounds.height > 0,
      bounds.contains(convert(window.mouseLocationOutsideOfEventStream, from: nil))
    else {
      return false
    }
    return true
  }

  override func mouseDown(with event: NSEvent) {
    onPointerEntered?()
    if event.clickCount >= 2 {
      isResizeDragging = false
      onDoubleClick?()
      appendSidebarResizeCursorLog(
        "nativeSidebarResize.handle.mouseDown.doubleClick",
        event: event,
        cursorAction: "reset")
      refreshCursorForCurrentPointer(reason: "doubleClickReset", event: event)
      return
    }
    isResizeDragging = true
    lastDragWindowX = event.locationInWindow.x
    NSCursor.resizeLeftRight.set()
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.mouseDown",
      event: event,
      cursorAction: "setResizeLeftRight")
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
    NSCursor.resizeLeftRight.set()
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.mouseDragged",
      event: event,
      extra: ["deltaX": Double(deltaX)],
      cursorAction: "setResizeLeftRight")
    onDrag?(deltaX)
  }

  override func mouseUp(with event: NSEvent) {
    isResizeDragging = false
    onDragEnded?()
    appendSidebarResizeCursorLog(
      "nativeSidebarResize.handle.mouseUp",
      event: event,
      cursorAction: "refresh")
    refreshCursorForCurrentPointer(reason: "mouseUp", event: event)
  }

  private func appendSidebarResizeCursorLog(
    _ eventName: String,
    event: NSEvent?,
    reason: String? = nil,
    pointerInside: Bool? = nil,
    extra: [String: Any] = [:],
    cursorAction: String
  ) {
    /**
     CDXC:NativeSidebarChrome 2026-06-13-07:38:
     The sticky sidebar resize cursor repro needs the same native event stream
     as working workspace splitters. Log only AppKit geometry, visibility,
     pointer containment, and cursor actions so support can compare enter/exit
     delivery without persisting project names, paths, URLs, command text, or
     terminal content.
     */
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let currentWindowPoint = window?.mouseLocationOutsideOfEventStream
    let currentLocalPoint = currentWindowPoint.map { convert($0, from: nil) }
    var details: [String: Any] = [
      "alphaValue": Double(alphaValue),
      "bounds": Self.debugFrame(bounds),
      "cursorAction": cursorAction,
      "frame": Self.debugFrame(frame),
      "hasSuperview": superview != nil,
      "hasWindow": window != nil,
      "isHidden": isHidden,
      "isResizeDragging": isResizeDragging,
      "pointerInside": pointerInside ?? isCurrentPointerInsideVisibleHandle(),
    ]
    if let reason {
      details["reason"] = reason
    }
    if let currentWindowPoint {
      details["currentWindowPoint"] = Self.debugPoint(currentWindowPoint)
    }
    if let currentLocalPoint {
      details["currentLocalPoint"] = Self.debugPoint(currentLocalPoint)
    }
    if let windowNumber = event?.window?.windowNumber ?? window?.windowNumber {
      details["windowNumber"] = windowNumber
    } else {
      details["windowNumber"] = NSNull()
    }
    if let event {
      /*
       CDXC:NativeSidebarChrome 2026-06-13-07:59:
       AppKit tracking events such as mouseEntered and mouseExited can raise
       NSGenericException when code reads clickCount. Persist clickCount only
       for real mouse button streams; hover and cursor logs still keep event
       type and pointer geometry.
       */
      if Self.sidebarResizeCursorEventSupportsClickCount(event.type) {
        details["clickCount"] = event.clickCount
      }
      details["eventType"] = String(describing: event.type)
      details["eventWindowPoint"] = Self.debugPoint(event.locationInWindow)
      details["eventLocalPoint"] = Self.debugPoint(convert(event.locationInWindow, from: nil))
    }
    for (key, value) in extra {
      details[key] = value
    }
    NativePaneTabDragReproLog.append(event: eventName, details: details)
  }

  private static func debugFrame(_ frame: CGRect) -> [String: Double] {
    [
      "height": Double(frame.height),
      "maxX": Double(frame.maxX),
      "maxY": Double(frame.maxY),
      "minX": Double(frame.minX),
      "minY": Double(frame.minY),
      "width": Double(frame.width),
    ]
  }

  private static func debugPoint(_ point: CGPoint) -> [String: Double] {
    [
      "x": Double(point.x),
      "y": Double(point.y),
    ]
  }

  private static func sidebarResizeCursorEventSupportsClickCount(_ eventType: NSEvent.EventType) -> Bool {
    switch eventType {
    case .leftMouseDown, .leftMouseDragged, .leftMouseUp,
      .rightMouseDown, .rightMouseDragged, .rightMouseUp,
      .otherMouseDown, .otherMouseDragged, .otherMouseUp:
      return true
    default:
      return false
    }
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
        let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
        Self.logger.error(
          "Sidebar DOM probe failed: \(sanitizedError, privacy: .public)")
        return
      }
      let sanitizedResult = NativeLogPrivacy.sanitizeLogLine(String(describing: result))
      Self.logger.info("Sidebar DOM probe: \(sanitizedResult, privacy: .public)")
    }
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
    let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
    Self.logger.error(
      "Sidebar webview navigation failed: \(sanitizedError, privacy: .public)")
  }

  func webView(
    _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
    withError error: Error
  ) {
    let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
    Self.logger.error(
      "Sidebar webview provisional navigation failed: \(sanitizedError, privacy: .public)"
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

private func terminalPaneChromeDropRegistrationDetails(
  registeredTypes: [NSPasteboard.PasteboardType],
  operationSource: String
) -> [String: Any] {
  [
    "operationSource": operationSource,
    "registeredTypeCount": registeredTypes.count,
    "registeredTypes": registeredTypes.map(\.rawValue).sorted(),
  ]
}

private func terminalPaneChromeDropPasteboardDetails(
  pasteboard: NSPasteboard,
  registeredTypes: [NSPasteboard.PasteboardType],
  operationSource: String,
  phase: String
) -> [String: Any] {
  let types = (pasteboard.types ?? []).map(\.rawValue).sorted()
  return [
    "operationSource": operationSource,
    "pasteboardChangeCount": pasteboard.changeCount,
    "phase": phase,
    "registeredTypeMatchCount": Set(pasteboard.types ?? []).intersection(Set(registeredTypes)).count,
    "typeCount": types.count,
    "types": types,
  ]
}

private enum NativeAppToastLevel: String {
  case info
  case success
  case warning
  case error

  init(rawValue: String?) {
    switch rawValue {
    case Self.success.rawValue:
      self = .success
    case Self.warning.rawValue:
      self = .warning
    case Self.error.rawValue:
      self = .error
    default:
      self = .info
    }
  }
}

private struct NativeAppToastAction {
  let label: String
  let sidebarMessage: Any
}

private struct NativeAppToastRequest {
  private static let defaultDurationMs: Double = 4_200

  let action: NativeAppToastAction?
  let description: String?
  let durationMs: Double
  let id: String
  let isPersistent: Bool
  let level: NativeAppToastLevel
  let title: String

  var showsSpinner: Bool {
    isPersistent && level == .info
  }

  init(
    id: String = "native-toast-\(UUID().uuidString)",
    level: NativeAppToastLevel,
    title: String,
    description: String? = nil,
    isPersistent: Bool = false,
    durationMs: Double = Self.defaultDurationMs,
    action: NativeAppToastAction? = nil
  ) {
    self.action = action
    self.description = description
    self.durationMs = durationMs
    self.id = id
    self.isPersistent = isPersistent
    self.level = level
    self.title = title
  }

  init?(message: [String: Any]) {
    guard let title = message["title"] as? String, !title.isEmpty else {
      return nil
    }
    let action: NativeAppToastAction?
    if let actionPayload = message["action"] as? [String: Any],
      let label = actionPayload["label"] as? String,
      !label.isEmpty,
      let sidebarMessage = actionPayload["sidebarMessage"]
    {
      action = NativeAppToastAction(label: label, sidebarMessage: sidebarMessage)
    } else {
      action = nil
    }
    let durationMs = Self.doubleValue(message["durationMs"]) ?? Self.defaultDurationMs
    self.init(
      id: (message["toastId"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        ?? "native-toast-\(UUID().uuidString)",
      level: NativeAppToastLevel(rawValue: message["level"] as? String),
      title: title,
      description: (message["description"] as? String).flatMap { $0.isEmpty ? nil : $0 },
      isPersistent: Self.boolValue(message["persistent"]),
      durationMs: durationMs,
      action: action)
  }

  private static func boolValue(_ value: Any?) -> Bool {
    if let value = value as? Bool {
      return value
    }
    return (value as? NSNumber)?.boolValue ?? false
  }

  private static func doubleValue(_ value: Any?) -> Double? {
    if let value = value as? Double {
      return value
    }
    if let value = value as? NSNumber {
      return value.doubleValue
    }
    return nil
  }
}

private final class NativeAppToastController {
  private static let bottomMargin: CGFloat = 47
  private static let gap: CGFloat = 10
  private static let maxVisibleToasts = 4
  private static let preferredWidth: CGFloat = 356
  private static let minimumWidth: CGFloat = 280

  private final class Item {
    let id: String
    let panel: NSPanel
    let view: NativeAppToastView
    var dismissTimer: Timer?

    init(id: String, panel: NSPanel, view: NativeAppToastView) {
      self.id = id
      self.panel = panel
      self.view = view
    }
  }

  private let onAction: (Any) -> Void
  private weak var parentWindow: NSWindow?
  private weak var rootView: NSView?
  private var anchorFrame = CGRect.zero
  private var itemsById: [String: Item] = [:]
  private var orderedIds: [String] = []

  init(onAction: @escaping (Any) -> Void) {
    self.onAction = onAction
  }

  func setLayout(parentWindow: NSWindow?, rootView: NSView, anchorFrame: CGRect) {
    self.parentWindow = parentWindow
    self.rootView = rootView
    self.anchorFrame = anchorFrame
    layoutPanels(animated: false)
  }

  func show(_ request: NativeAppToastRequest) {
    guard let parentWindow else {
      return
    }
    let item: Item
    if let existing = itemsById[request.id] {
      item = existing
      item.dismissTimer?.invalidate()
      item.view.update(request)
      item.panel.ignoresMouseEvents = request.action == nil
    } else {
      item = makeItem(request)
      itemsById[request.id] = item
      orderedIds.append(request.id)
      parentWindow.addChildWindow(item.panel, ordered: .above)
      item.panel.alphaValue = 0
      item.panel.orderFront(nil)
      NSAnimationContext.runAnimationGroup { context in
        context.duration = 0.16
        item.panel.animator().alphaValue = 1
      }
    }
    while orderedIds.count > Self.maxVisibleToasts, let oldestId = orderedIds.first {
      dismiss(id: oldestId, animated: true)
    }
    scheduleDismissalIfNeeded(item: item, request: request)
    layoutPanels(animated: true)
  }

  func closeAll() {
    for id in orderedIds {
      dismiss(id: id, animated: false)
    }
  }

  private func makeItem(_ request: NativeAppToastRequest) -> Item {
    /*
     CDXC:AppToasts 2026-06-11-21:04:
     macOS app toasts must not use the modal-host WKWebView overlay. Render the
     shared toast contract as exact-sized borderless child panels so normal
     toasts pass mouse events through and action toasts only capture inside the
     visible shadcn-style card.
     */
    let view = NativeAppToastView(request: request, onAction: onAction)
    let panel = NSPanel(
      contentRect: CGRect(origin: .zero, size: view.preferredSize(forWidth: Self.preferredWidth)),
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false
    )
    panel.backgroundColor = .clear
    panel.collectionBehavior = [.fullScreenAuxiliary, .transient]
    panel.contentView = view
    panel.hasShadow = true
    panel.hidesOnDeactivate = false
    panel.ignoresMouseEvents = request.action == nil
    panel.isMovable = false
    panel.isOpaque = false
    panel.isReleasedWhenClosed = false
    panel.level = parentWindow?.level ?? .normal
    return Item(id: request.id, panel: panel, view: view)
  }

  private func scheduleDismissalIfNeeded(item: Item, request: NativeAppToastRequest) {
    item.dismissTimer?.invalidate()
    guard !request.isPersistent else {
      item.dismissTimer = nil
      return
    }
    item.dismissTimer = Timer.scheduledTimer(withTimeInterval: request.durationMs / 1_000, repeats: false) {
      [weak self, weak item] _ in
      guard let item else {
        return
      }
      self?.dismiss(id: item.id, animated: true)
    }
  }

  private func dismiss(id: String, animated: Bool) {
    guard let item = itemsById[id] else {
      return
    }
    item.dismissTimer?.invalidate()
    itemsById[id] = nil
    orderedIds.removeAll { $0 == id }
    let tearDown = { [weak self, weak item] in
      guard let item else {
        return
      }
      self?.parentWindow?.removeChildWindow(item.panel)
      item.panel.contentView = nil
      item.panel.orderOut(nil)
      self?.layoutPanels(animated: true)
    }
    guard animated else {
      tearDown()
      return
    }
    NSAnimationContext.runAnimationGroup { context in
      context.duration = 0.14
      item.panel.animator().alphaValue = 0
    } completionHandler: {
      tearDown()
    }
  }

  private func layoutPanels(animated: Bool) {
    guard let parentWindow,
      let rootView,
      !orderedIds.isEmpty
    else {
      return
    }
    let windowAnchorFrame = rootView.convert(anchorFrame, to: nil)
    let screenAnchorFrame = parentWindow.convertToScreen(windowAnchorFrame)
    let toastWidth = min(
      Self.preferredWidth,
      max(Self.minimumWidth, screenAnchorFrame.width - 32))
    var y = screenAnchorFrame.minY + Self.bottomMargin
    for id in orderedIds.reversed() {
      guard let item = itemsById[id] else {
        continue
      }
      let size = item.view.preferredSize(forWidth: toastWidth)
      let frame = CGRect(
        x: floor(screenAnchorFrame.midX - size.width / 2),
        y: floor(y),
        width: size.width,
        height: size.height
      )
      item.view.frame = CGRect(origin: .zero, size: size)
      if animated {
        NSAnimationContext.runAnimationGroup { context in
          context.duration = 0.16
          item.panel.animator().setFrame(frame, display: true)
        }
      } else {
        item.panel.setFrame(frame, display: true)
      }
      y += size.height + Self.gap
    }
  }
}

private final class NativeAppToastView: NSView {
  private static let actionGap: CGFloat = 14
  private static let actionHeight: CGFloat = 24
  private static let horizontalPadding: CGFloat = 20
  private static let iconSize: CGFloat = 18
  private static let iconTextGap: CGFloat = 14

  private let actionButton = NativeToastActionButton()
  private let descriptionField = NSTextField(labelWithString: "")
  private let spinnerView = NativeToastSpinnerView(frame: .zero)
  private let titleField = NSTextField(labelWithString: "")
  private let onAction: (Any) -> Void
  private var request: NativeAppToastRequest

  override var isFlipped: Bool { true }

  init(request: NativeAppToastRequest, onAction: @escaping (Any) -> Void) {
    self.onAction = onAction
    self.request = request
    super.init(frame: CGRect(origin: .zero, size: Self.preferredSize(for: request, width: 356)))
    wantsLayer = true
    layer?.backgroundColor = NSColor(srgbRed: 23.0 / 255.0, green: 23.0 / 255.0, blue: 23.0 / 255.0, alpha: 1).cgColor
    layer?.borderColor = NSColor.white.withAlphaComponent(0.14).cgColor
    layer?.borderWidth = 1
    layer?.cornerRadius = 12
    layer?.masksToBounds = true

    titleField.font = .systemFont(ofSize: 14, weight: .semibold)
    titleField.lineBreakMode = .byTruncatingTail
    titleField.textColor = NSColor(srgbRed: 244.0 / 255.0, green: 244.0 / 255.0, blue: 245.0 / 255.0, alpha: 1)
    titleField.usesSingleLineMode = true
    descriptionField.font = .systemFont(ofSize: 14, weight: .regular)
    descriptionField.lineBreakMode = .byTruncatingTail
    descriptionField.maximumNumberOfLines = 2
    descriptionField.textColor = NSColor(srgbRed: 212.0 / 255.0, green: 212.0 / 255.0, blue: 216.0 / 255.0, alpha: 1)

    addSubview(spinnerView)
    addSubview(titleField)
    addSubview(descriptionField)
    addSubview(actionButton)
    update(request)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func update(_ request: NativeAppToastRequest) {
    self.request = request
    titleField.stringValue = request.title
    descriptionField.stringValue = request.description ?? ""
    descriptionField.isHidden = request.description == nil
    spinnerView.isHidden = !request.showsSpinner
    if let action = request.action {
      actionButton.isHidden = false
      actionButton.title = action.label
      actionButton.onClick = { [weak self] in
        self?.onAction(action.sidebarMessage)
      }
    } else {
      actionButton.isHidden = true
      actionButton.onClick = nil
    }
    needsLayout = true
  }

  func preferredSize(forWidth width: CGFloat) -> CGSize {
    Self.preferredSize(for: request, width: width)
  }

  override func layout() {
    super.layout()
    let hasDescription = request.description != nil
    let actionWidth = actionButton.isHidden ? CGFloat(0) : actionButton.preferredWidth
    let leadingContentX = Self.horizontalPadding
      + (request.showsSpinner ? Self.iconSize + Self.iconTextGap : 0)
    let actionFrame = CGRect(
      x: bounds.width - Self.horizontalPadding - actionWidth,
      y: floor((bounds.height - Self.actionHeight) / 2),
      width: actionWidth,
      height: Self.actionHeight
    )
    actionButton.frame = actionButton.isHidden ? .zero : actionFrame
    spinnerView.frame = request.showsSpinner
      ? CGRect(
        x: Self.horizontalPadding + 4,
        y: floor((bounds.height - Self.iconSize) / 2),
        width: Self.iconSize,
        height: Self.iconSize)
      : .zero
    let textRight = actionButton.isHidden
      ? bounds.width - Self.horizontalPadding
      : actionFrame.minX - Self.actionGap
    let textWidth = max(40, textRight - leadingContentX)
    if hasDescription {
      titleField.frame = CGRect(x: leadingContentX, y: 13, width: textWidth, height: 22)
      descriptionField.frame = CGRect(x: leadingContentX, y: 37, width: textWidth, height: bounds.height - 49)
    } else {
      titleField.frame = CGRect(
        x: leadingContentX,
        y: floor((bounds.height - 22) / 2),
        width: textWidth,
        height: 22)
      descriptionField.frame = .zero
    }
  }

  private static func preferredSize(for request: NativeAppToastRequest, width: CGFloat) -> CGSize {
    let hasDescription = request.description != nil
    let baseHeight: CGFloat = hasDescription ? 72 : 52
    return CGSize(width: width, height: baseHeight)
  }
}

private final class NativeToastActionButton: NSView {
  private let titleField = NSTextField(labelWithString: "")
  private var isPressed = false

  var onClick: (() -> Void)?
  var title: String = "" {
    didSet {
      titleField.stringValue = title
      needsLayout = true
    }
  }

  var preferredWidth: CGFloat {
    let size = (title as NSString).size(withAttributes: [
      .font: NSFont.systemFont(ofSize: 14, weight: .medium)
    ])
    return min(max(ceil(size.width) + 22, 46), 128)
  }

  override var isFlipped: Bool { true }

  init() {
    super.init(frame: .zero)
    wantsLayer = true
    layer?.cornerRadius = 6
    layer?.masksToBounds = true
    titleField.alignment = .center
    titleField.font = .systemFont(ofSize: 14, weight: .medium)
    titleField.textColor = NSColor(srgbRed: 24.0 / 255.0, green: 24.0 / 255.0, blue: 27.0 / 255.0, alpha: 1)
    titleField.usesSingleLineMode = true
    addSubview(titleField)
    updateBackground()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    titleField.frame = bounds.insetBy(dx: 6, dy: 2)
  }

  override func mouseDown(with event: NSEvent) {
    isPressed = true
    updateBackground()
  }

  override func mouseUp(with event: NSEvent) {
    let shouldClick = bounds.contains(convert(event.locationInWindow, from: nil))
    isPressed = false
    updateBackground()
    if shouldClick {
      onClick?()
    }
  }

  private func updateBackground() {
    let color = isPressed
      ? NSColor(srgbRed: 228.0 / 255.0, green: 228.0 / 255.0, blue: 231.0 / 255.0, alpha: 1)
      : NSColor(srgbRed: 244.0 / 255.0, green: 244.0 / 255.0, blue: 245.0 / 255.0, alpha: 1)
    layer?.backgroundColor = color.cgColor
  }
}

private final class NativeToastSpinnerView: NSView {
  private let spinnerLayer = CAShapeLayer()

  override var isFlipped: Bool { true }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    spinnerLayer.fillColor = nil
    spinnerLayer.lineCap = .round
    spinnerLayer.lineWidth = 2
    spinnerLayer.strokeColor = NSColor.white.withAlphaComponent(0.95).cgColor
    layer?.addSublayer(spinnerLayer)

    let animation = CABasicAnimation(keyPath: "transform.rotation.z")
    animation.byValue = CGFloat.pi * 2
    animation.duration = 0.85
    animation.repeatCount = .infinity
    spinnerLayer.add(animation, forKey: "nativeToastSpinner")
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    spinnerLayer.frame = bounds
    let insetBounds = bounds.insetBy(dx: 2, dy: 2)
    let radius = min(insetBounds.width, insetBounds.height) / 2
    let center = CGPoint(x: bounds.midX, y: bounds.midY)
    let path = CGMutablePath()
    path.addArc(
      center: center,
      radius: radius,
      startAngle: -CGFloat.pi * 0.35,
      endAngle: CGFloat.pi * 1.15,
      clockwise: false)
    spinnerLayer.path = path
  }
}

private final class AppModalWindowPanel: NSPanel {
  private struct ResizeEdges: OptionSet {
    let rawValue: Int

    static let left = ResizeEdges(rawValue: 1 << 0)
    static let right = ResizeEdges(rawValue: 1 << 1)
    static let bottom = ResizeEdges(rawValue: 1 << 2)
    static let top = ResizeEdges(rawValue: 1 << 3)
  }

  var promptEditorTitleDragHeight: CGFloat = 0
  var promptEditorTitleDragExcludedTrailingWidth: CGFloat = 0
  var promptEditorResizeMargin: CGFloat = 0
  var promptEditorMinimumContentSize = CGSize(width: 180, height: 260)

  override var canBecomeKey: Bool { true }
  override var canBecomeMain: Bool { true }

  override func sendEvent(_ event: NSEvent) {
    guard event.type == .leftMouseDown,
      promptEditorTitleDragHeight > 0
    else {
      super.sendEvent(event)
      return
    }

    let point = event.locationInWindow
    let resizeEdges = promptEditorResizeEdges(for: point)
    if !resizeEdges.isEmpty {
      resizePromptEditorWindow(from: event, edges: resizeEdges)
      return
    }

    let isInTitleDragBand = point.y >= frame.height - promptEditorTitleDragHeight
    let isInTrailingControlArea =
      point.x >= frame.width - promptEditorTitleDragExcludedTrailingWidth
    if isInTitleDragBand, !isInTrailingControlArea {
      performDrag(with: event)
      return
    }

    super.sendEvent(event)
  }

  private func promptEditorResizeEdges(for point: CGPoint) -> ResizeEdges {
    guard promptEditorResizeMargin > 0 else {
      return []
    }
    var edges: ResizeEdges = []
    if point.x <= promptEditorResizeMargin {
      edges.insert(.left)
    }
    if point.x >= frame.width - promptEditorResizeMargin {
      edges.insert(.right)
    }
    if point.y <= promptEditorResizeMargin {
      edges.insert(.bottom)
    }
    return edges
  }

  private func resizePromptEditorWindow(from event: NSEvent, edges: ResizeEdges) {
    let startMouse = NSEvent.mouseLocation
    let startFrame = frame
    while true {
      guard let nextEvent = nextEvent(matching: [.leftMouseDragged, .leftMouseUp]) else {
        break
      }
      if nextEvent.type == .leftMouseUp {
        break
      }

      let currentMouse = NSEvent.mouseLocation
      let delta = CGPoint(
        x: currentMouse.x - startMouse.x,
        y: currentMouse.y - startMouse.y)
      setFrame(
        promptEditorResizedFrame(from: startFrame, delta: delta, edges: edges),
        display: true)
    }
  }

  private func promptEditorResizedFrame(
    from startFrame: CGRect,
    delta: CGPoint,
    edges: ResizeEdges
  ) -> CGRect {
    var nextFrame = startFrame
    if edges.contains(.left) {
      nextFrame.origin.x = startFrame.origin.x + delta.x
      nextFrame.size.width = startFrame.width - delta.x
    }
    if edges.contains(.right) {
      nextFrame.size.width = startFrame.width + delta.x
    }
    if edges.contains(.bottom) {
      nextFrame.origin.y = startFrame.origin.y + delta.y
      nextFrame.size.height = startFrame.height - delta.y
    }
    if edges.contains(.top) {
      nextFrame.size.height = startFrame.height + delta.y
    }

    if nextFrame.width < promptEditorMinimumContentSize.width {
      if edges.contains(.left) {
        nextFrame.origin.x = startFrame.maxX - promptEditorMinimumContentSize.width
      }
      nextFrame.size.width = promptEditorMinimumContentSize.width
    }
    if nextFrame.height < promptEditorMinimumContentSize.height {
      if edges.contains(.bottom) {
        nextFrame.origin.y = startFrame.maxY - promptEditorMinimumContentSize.height
      }
      nextFrame.size.height = promptEditorMinimumContentSize.height
    }
    return nextFrame
  }
}

private final class AppModalWindowWebView: WKWebView {
  var nativeWindowTitleDragHeight: CGFloat = 0
  var nativeWindowTitleDragExcludedTrailingWidth: CGFloat = 0

  override func mouseDown(with event: NSEvent) {
    guard nativeWindowTitleDragHeight > 0 else {
      super.mouseDown(with: event)
      return
    }

    let point = convert(event.locationInWindow, from: nil)
    let isInTitleDragBand = point.y >= bounds.height - nativeWindowTitleDragHeight
    let isInTrailingControlArea =
      point.x >= bounds.width - nativeWindowTitleDragExcludedTrailingWidth
    guard isInTitleDragBand, !isInTrailingControlArea else {
      super.mouseDown(with: event)
      return
    }

    guard let window else {
      super.mouseDown(with: event)
      return
    }
    window.performDrag(with: event)
  }
}

private final class AppModalWindowController: NSObject, NSWindowDelegate, WKNavigationDelegate {
  private static let screenMargin: CGFloat = 24
  private static let minimumSize = CGSize(width: 520, height: 360)
  private static let floatingPromptEditorMinimumSize = CGSize(width: 180, height: 260)
  private static let floatingPromptEditorResizeMargin: CGFloat = 8
  private static let floatingPromptEditorTitleDragHeight: CGFloat = 32
  private static let floatingPromptEditorTrailingActionReserve: CGFloat = 170

  private let scriptBridge: SidebarScriptBridge
  private let bootstrapScriptSource: String?
  private let diagnosticsScript: String
  private let onClosed: (String, String?) -> Void
  private let onContentFrameChanged: (String, CGRect) -> Void

  private weak var parentWindow: NSWindow?
  private var panel: AppModalWindowPanel?
  private var webView: WKWebView?
  private var loadedModal: String?
  private var currentModal: String?
  private var pendingOpenMessage: [String: Any]?
  private var pendingMessages: [[String: Any]] = []
  private var latestSidebarState: [String: Any]?
  private var outsideEventMonitor: Any?
  private var isReady = false
  private var isProgrammaticClose = false
  private var openStartedAtMs: Int?
  private var webViewLoadStartedAtMs: Int?

  var window: NSWindow? {
    currentModal == nil ? nil : panel
  }

  var canReceiveMessages: Bool {
    webView != nil && currentModal != nil
  }

  var currentModalKind: String? {
    currentModal
  }

  private static func monotonicMilliseconds() -> Int {
    Int((ProcessInfo.processInfo.systemUptime * 1000).rounded())
  }

  init(
    scriptBridge: SidebarScriptBridge,
    bootstrapScriptSource: String?,
    diagnosticsScript: String,
    onClosed: @escaping (String, String?) -> Void,
    onContentFrameChanged: @escaping (String, CGRect) -> Void
  ) {
    self.scriptBridge = scriptBridge
    self.bootstrapScriptSource = bootstrapScriptSource
    self.diagnosticsScript = diagnosticsScript
    self.onClosed = onClosed
    self.onContentFrameChanged = onContentFrameChanged
    super.init()
  }

  deinit {
    close(sendReactClose: false)
  }

  func open(
    modal: String,
    message: [String: Any],
    parentWindow: NSWindow,
    webAssets: URL,
    latestSidebarState: [String: Any]?,
    preferredContentFrame: CGRect? = nil
  ) {
    /*
     CDXC:AppModals 2026-06-11-19:46:
     Non-prompt app modals must not create any transparent overlay above the main workspace. Present the existing React modal host inside a real AppKit child window, keep the panel hidden until React reports `presented`, and make the panel keyable so Settings search, agent forms, and previous-session filters receive normal keyboard input.

     CDXC:AppModals 2026-06-11-20:46:
     Native modal windows should look like the titlebar Tips & Tricks child
     panel rather than macOS document windows. Use a borderless keyable panel so
     React owns the visible close affordance and no AppKit titlebar or traffic
     lights wrap Settings, Agents Hub, Previous Sessions, or smaller modals.

     CDXC:PromptEditor 2026-06-11-22:51:
     The rich prompt editor no longer uses the transparent full-workspace modal
     overlay. It reuses the native modal-window host, but keeps AppKit resize
     and move behavior through a titled/resizable child window with hidden
     traffic-light buttons so the editor can float without covering the
     workspace with a hit-test layer.

     CDXC:PromptEditor 2026-06-11-23:06:
     The prompt editor window should have square corners and a reliable drag
     target across the whole React titlebar. Use a prompt-specific borderless
     panel and intercept titlebar drag plus edge resize at the NSWindow event
     layer, so WKWebView subviews cannot swallow the move/resize gestures.

     CDXC:PromptEditor 2026-06-12-04:37:
     Ctrl+G should be fast on first launch and subsequent launches. Reuse the
     hidden prompt-editor native window when it already loaded the real modal
     host, but keep other app modals on the existing fresh-open lifecycle so
     unrelated dialogs do not inherit prompt editor state.
     */
    if canReusePromptEditorHost(for: modal) {
      reusePromptEditorHost(
        message: message,
        parentWindow: parentWindow,
        latestSidebarState: latestSidebarState,
        preferredContentFrame: preferredContentFrame)
      return
    }
    close(sendReactClose: false)
    self.parentWindow = parentWindow
    self.loadedModal = modal
    self.currentModal = modal
    self.pendingOpenMessage = message
    self.pendingMessages = []
    self.latestSidebarState = latestSidebarState
    self.isReady = false
    self.openStartedAtMs = Self.monotonicMilliseconds()
    self.webViewLoadStartedAtMs = nil
    logPromptWindowEvent(
      "nativeWindow.open.start",
      details: [
        "isReusableHost": false,
        "modal": modal,
        "requestId": message["requestId"] as? String ?? "",
      ])

    let size = constrainedSize(
      preferredContentFrame?.size ?? defaultSize(for: modal),
      parentWindow: parentWindow,
      modal: modal)
    let contentFrame = constrainedContentFrame(
      preferredContentFrame: preferredContentFrame,
      size: size,
      parentWindow: parentWindow,
      modal: modal)
    let panel = AppModalWindowPanel(
      contentRect: contentFrame,
      styleMask: appModalStyleMask(for: modal),
      backing: .buffered,
      defer: false
    )
    panel.backgroundColor = NSColor(
      srgbRed: 14.0 / 255.0,
      green: 14.0 / 255.0,
      blue: 14.0 / 255.0,
      alpha: 1.0)
    panel.collectionBehavior = [.fullScreenAuxiliary]
    panel.delegate = self
    panel.hasShadow = true
    panel.hidesOnDeactivate = false
    /*
     CDXC:AppModals 2026-06-11-20:43:
     Native child-window modals must not reveal a darker #050505 backing around
     the React dialog while WKWebView is transparent or before CSS first paints.
     Keep the AppKit panel and WebView layer on the same #0e0e0e surface.
     */
    panel.isOpaque = true
    panel.isMovable = true
    panel.isReleasedWhenClosed = false
    panel.level = parentWindow.level
    panel.contentMinSize = minimumContentSize(for: modal)
    if modal == "floatingPromptEditor" {
      panel.promptEditorMinimumContentSize = Self.floatingPromptEditorMinimumSize
      panel.promptEditorResizeMargin = Self.floatingPromptEditorResizeMargin
      panel.promptEditorTitleDragHeight = Self.floatingPromptEditorTitleDragHeight
      panel.promptEditorTitleDragExcludedTrailingWidth =
        Self.floatingPromptEditorTrailingActionReserve
    }
    if shouldLockContentSize(modal: modal) {
      panel.contentMinSize = size
      panel.contentMaxSize = size
    }
    panel.title = title(for: modal)
    configurePanelChrome(panel, modal: modal)

    let webView = AppModalWindowWebView(
      frame: CGRect(origin: .zero, size: contentFrame.size),
      configuration: makeConfiguration())
    if modal == "floatingPromptEditor" {
      webView.nativeWindowTitleDragHeight = Self.floatingPromptEditorTitleDragHeight
      webView.nativeWindowTitleDragExcludedTrailingWidth =
        Self.floatingPromptEditorTrailingActionReserve
    }
    webView.autoresizingMask = [.width, .height]
    webView.navigationDelegate = self
    /*
     CDXC:AppModals 2026-06-11-20:43:
     The native modal WKWebView can be transparent before React/CSS finishes
     painting. Give the WebView layer the same #0e0e0e backing as the panel so
     no darker host border appears around the dialog component.
     */
    webView.wantsLayer = true
    webView.layer?.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor.cgColor
    webView.setValue(false, forKey: "drawsBackground")
    panel.contentView = webView

    self.panel = panel
    self.webView = webView
    installOutsideEventMonitorIfNeeded(for: modal)
    loadModalHost(webAssets: webAssets, webView: webView)
  }

  private func canReusePromptEditorHost(for modal: String) -> Bool {
    modal == "floatingPromptEditor"
      && loadedModal == "floatingPromptEditor"
      && panel != nil
      && webView != nil
  }

  private func reusePromptEditorHost(
    message: [String: Any],
    parentWindow: NSWindow,
    latestSidebarState: [String: Any]?,
    preferredContentFrame: CGRect?
  ) {
    self.parentWindow = parentWindow
    self.currentModal = "floatingPromptEditor"
    self.pendingOpenMessage = nil
    self.pendingMessages = []
    if let latestSidebarState {
      self.latestSidebarState = latestSidebarState
    }
    self.openStartedAtMs = Self.monotonicMilliseconds()
    logPromptWindowEvent(
      "nativeWindow.open.reuse",
      details: [
        "isReady": isReady,
        "requestId": message["requestId"] as? String ?? "",
      ])

    if let panel {
      let size = constrainedSize(
        preferredContentFrame?.size ?? defaultSize(for: "floatingPromptEditor"),
        parentWindow: parentWindow,
        modal: "floatingPromptEditor")
      let contentFrame = constrainedContentFrame(
        preferredContentFrame: preferredContentFrame,
        size: size,
        parentWindow: parentWindow)
      panel.setFrame(panel.frameRect(forContentRect: contentFrame), display: false)
      panel.level = parentWindow.level
      panel.contentMinSize = minimumContentSize(for: "floatingPromptEditor")
    }

    guard isReady else {
      pendingOpenMessage = message
      return
    }
    if let latestSidebarState = self.latestSidebarState {
      dispatch(latestSidebarState)
    }
    dispatch(message)
  }

  func hostReady(latestSidebarState: [String: Any]?) {
    isReady = true
    logPromptWindowEvent(
      "nativeWindow.hostReady",
      details: [
        "msSinceOpen": elapsedSinceOpenMs(),
      ])
    if let latestSidebarState {
      self.latestSidebarState = latestSidebarState
    }
    if let latestSidebarState = self.latestSidebarState {
      dispatch(latestSidebarState)
    }
    if let pendingOpenMessage {
      dispatch(pendingOpenMessage)
      self.pendingOpenMessage = nil
    }
    let queuedMessages = pendingMessages
    pendingMessages = []
    queuedMessages.forEach { dispatch($0) }
  }

  func presentIfCurrent(modal: String?) {
    guard modal == currentModal,
      let parentWindow,
      let panel
    else {
      return
    }
    if panel.parent !== parentWindow {
      parentWindow.addChildWindow(panel, ordered: .above)
    }
    logPromptWindowEvent(
      "nativeWindow.present",
      details: [
        "msSinceOpen": elapsedSinceOpenMs(),
      ])
    panel.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
  }

  func dispatch(_ message: [String: Any]) {
    if (message["type"] as? String) == "sidebarState" {
      latestSidebarState = message
    }
    guard let webView else {
      return
    }
    guard isReady else {
      /*
       CDXC:AppModals 2026-06-11-23:07:
       The main-window modal WKWebView fallback is gone, so fast follow-up
       bridge messages for native child windows must queue until the child
       WKWebView reports ready. Keep sidebar state as latest-only because it is
       replayed separately during hostReady.
       */
      if (message["type"] as? String) != "sidebarState" {
        pendingMessages.append(message)
      }
      return
    }
    if loadedModal == "floatingPromptEditor",
      (message["modal"] as? String) == "floatingPromptEditor"
        || (message["type"] as? String)?.hasPrefix("floatingPromptEditor") == true
    {
      logPromptWindowEvent(
        "nativeWindow.dispatch",
        details: [
          "messageModal": message["modal"] as? String ?? "",
          "messageType": message["type"] as? String ?? "",
          "msSinceOpen": elapsedSinceOpenMs(),
          "requestId": message["requestId"] as? String ?? "",
        ])
    }
    guard JSONSerialization.isValidJSONObject(message),
      let data = try? JSONSerialization.data(withJSONObject: message),
      let json = String(data: data, encoding: .utf8)
    else {
      AppDelegate.appendAppModalErrorLog(
        area: "AppModals:nativeWindow",
        message: "Failed to serialize native-window modal host message: \(message)",
        stack: nil
      )
      return
    }
    webView.evaluateJavaScript(
      """
      window.dispatchEvent(new CustomEvent('ghostex-app-modal-host-message', { detail: \(json) }));
      undefined;
      """
    ) { _, error in
      if let error {
        AppDelegate.appendAppModalErrorLog(
          area: "AppModals:nativeWindow",
          message: "Failed to dispatch native-window modal host message: \(error.localizedDescription)",
          stack: nil
        )
      }
    }
  }

  func close(sendReactClose: Bool) {
    if sendReactClose {
      dispatch(["type": "close"])
    }
    isProgrammaticClose = true
    tearDownPanel()
    isProgrammaticClose = false
  }

  func hideReusablePromptEditor(sendReactClose: Bool) {
    guard loadedModal == "floatingPromptEditor",
      panel != nil,
      webView != nil
    else {
      close(sendReactClose: sendReactClose)
      return
    }
    /*
     CDXC:PromptEditor 2026-06-12-04:37:
     Prompt-editor save, cancel, and prewarm completion should clear React's
     active prompt state but keep the native WKWebView process and loaded
     Monaco runtime alive. Ordering the child window out preserves privacy
     on-screen while avoiding the next Ctrl+G WebKit cold start.
     */
    if sendReactClose {
      dispatch(["type": "close"])
    }
    publishContentFrameChanged()
    isProgrammaticClose = true
    parentWindow?.removeChildWindow(panel!)
    panel?.orderOut(nil)
    isProgrammaticClose = false
    currentModal = nil
    pendingOpenMessage = nil
    pendingMessages = []
    openStartedAtMs = nil
    logPromptWindowEvent(
      "nativeWindow.promptHostHidden",
      details: [
        "sendReactClose": sendReactClose,
      ])
  }

  func windowWillClose(_ notification: Notification) {
    guard !isProgrammaticClose else {
      return
    }
    let closedModal = currentModal
    tearDownPanel()
    onClosed("nativeWindowCloseButton", closedModal)
  }

  func windowDidMove(_ notification: Notification) {
    publishContentFrameChanged()
  }

  func windowDidResize(_ notification: Notification) {
    publishContentFrameChanged()
  }

  private func tearDownPanel() {
    removeOutsideEventMonitor()
    publishContentFrameChanged()
    if let panel {
      parentWindow?.removeChildWindow(panel)
      panel.delegate = nil
      panel.orderOut(nil)
      panel.contentView = nil
    }
    webView?.navigationDelegate = nil
    webView = nil
    panel = nil
    loadedModal = nil
    currentModal = nil
    pendingOpenMessage = nil
    pendingMessages = []
    isReady = false
    openStartedAtMs = nil
    webViewLoadStartedAtMs = nil
  }

  private func installOutsideEventMonitorIfNeeded(for modal: String) {
    removeOutsideEventMonitor()
    guard modal == "commandPalette" else {
      return
    }
    /*
     CDXC:CommandPalette 2026-06-12-05:45:
     The command palette is a compact native child window, so clicks in the
     parent Ghostex window happen outside the palette WKWebView and cannot be
     seen by Base UI's dialog backdrop. Close the palette on the next app-local
     mouse down outside its NSPanel while letting the original click continue to
     its target.
     */
    outsideEventMonitor = NSEvent.addLocalMonitorForEvents(
      matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]
    ) { [weak self] event in
      guard let self else {
        return event
      }
      guard self.currentModal == "commandPalette" else {
        return event
      }
      if let panel = self.panel,
        event.window === panel
      {
        return event
      }
      self.closeFromOutsideMouseDown()
      return event
    }
  }

  private func closeFromOutsideMouseDown() {
    let closedModal = currentModal
    close(sendReactClose: true)
    onClosed("outsideMouseDown", closedModal)
  }

  private func removeOutsideEventMonitor() {
    if let outsideEventMonitor {
      NSEvent.removeMonitor(outsideEventMonitor)
      self.outsideEventMonitor = nil
    }
  }

  private func publishContentFrameChanged() {
    guard let currentModal,
      currentModal == "floatingPromptEditor",
      let panel
    else {
      return
    }
    onContentFrameChanged(currentModal, panel.contentRect(forFrameRect: panel.frame))
  }

  private func makeConfiguration() -> WKWebViewConfiguration {
    let configuration = WKWebViewConfiguration()
    configuration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    configuration.userContentController.addUserScript(
      WKUserScript(
        source: "window.__ghostex_APP_MODAL_HOST_SURFACE__ = \"nativeWindow\";",
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    if let bootstrapScriptSource {
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: bootstrapScriptSource,
          injectionTime: .atDocumentStart,
          forMainFrameOnly: true
        ))
    }
    configuration.userContentController.addUserScript(
      WKUserScript(
        source: diagnosticsScript,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    return configuration
  }

  private func loadModalHost(webAssets: URL, webView: WKWebView) {
    webViewLoadStartedAtMs = Self.monotonicMilliseconds()
    logPromptWindowEvent(
      "nativeWindow.webView.loadStart",
      details: [
        "modal": loadedModal ?? "",
      ])
    let builtModalHost = webAssets.appendingPathComponent("modal-host.html")
    guard FileManager.default.fileExists(atPath: builtModalHost.path) else {
      webView.loadHTMLString(
        "<!doctype html><html><body style=\"margin:0;background:#0e0e0e\"></body></html>",
        baseURL: webAssets
      )
      return
    }
    webView.loadFileURL(builtModalHost, allowingReadAccessTo: webAssets)
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    logPromptWindowEvent(
      "nativeWindow.webView.didFinish",
      details: [
        "msSinceLoadStart": elapsedSinceWebViewLoadMs(),
        "msSinceOpen": elapsedSinceOpenMs(),
      ])
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
    logPromptWindowEvent(
      "nativeWindow.webView.failed",
      details: [
        "error": error.localizedDescription,
      ])
  }

  func webView(
    _ webView: WKWebView,
    didFailProvisionalNavigation navigation: WKNavigation!,
    withError error: Error
  ) {
    logPromptWindowEvent(
      "nativeWindow.webView.provisionalFailed",
      details: [
        "error": error.localizedDescription,
      ])
  }

  private func elapsedSinceOpenMs() -> Int {
    guard let openStartedAtMs else {
      return -1
    }
    return max(0, Self.monotonicMilliseconds() - openStartedAtMs)
  }

  private func elapsedSinceWebViewLoadMs() -> Int {
    guard let webViewLoadStartedAtMs else {
      return -1
    }
    return max(0, Self.monotonicMilliseconds() - webViewLoadStartedAtMs)
  }

  private func logPromptWindowEvent(_ event: String, details: [String: Any] = [:]) {
    guard loadedModal == "floatingPromptEditor"
      || currentModal == "floatingPromptEditor"
      || (details["modal"] as? String) == "floatingPromptEditor"
    else {
      return
    }
    var payload = details
    payload["currentModal"] = currentModal ?? ""
    payload["loadedModal"] = loadedModal ?? ""
    payload["source"] = "nativeWindow"
    PromptEditorDebugLog.append(event: event, details: payload)
  }

  private func defaultSize(for modal: String) -> CGSize {
    switch modal {
    case "floatingPromptEditor":
      return CGSize(width: 400, height: 320)
    case "settings", "configureAgents", "configureActions", "openTargets", "hotkeys":
      return CGSize(width: 1120, height: 780)
    case "agentsHub":
      return CGSize(width: 1120, height: 760)
    case "firstLaunchSetup", "tipsAndTricks":
      /*
       CDXC:FirstLaunchSetup 2026-06-12-07:13:
       The macOS first-launch setup child window needs 90px more vertical room so the hooks step and footer fit without clipping in the native app.
       Keep the legacy Tips & Tricks modal id aligned because it routes to the same first-launch setup surface.
       */
      return CGSize(width: 1120, height: 850)
    case "gitFileDiff":
      return CGSize(width: 1180, height: 820)
    case "gitCommit":
      /*
       CDXC:TitlebarGit 2026-06-12-11:30:
       Commit review needs 20px more horizontal room from the right side of the native modal. Size the child WebView to 1020px wide while the placement helper preserves the old 1000px frame's left edge.
       */
      return CGSize(width: 1020, height: 760)
    case "worktree":
      /*
       CDXC:WorktreeModal 2026-06-12-10:51:
       Add Worktree should be a compact macOS child-window modal sized exactly 570x550, not the larger Git Commit review frame. Keep this modal-specific so commit review still has room for file selection and diff-related controls.

       CDXC:WorktreeModal 2026-06-12-11:10:
       Add Worktree needs 80px more vertical room after the 570x550 compact pass. Keep the 570px width and lock the native child window at 570x630 so the React/WebView surface can fill the frame with exact 18px edge padding.
       */
      return CGSize(width: 570, height: 630)
    case "previousSessions":
      /*
       CDXC:PreviousSessions 2026-06-11-20:39:
       Previous Sessions remains a compact picker whose React shell is 546px wide inside the modal host's 12px viewport padding. Size the native child window to that fitted component instead of the generic management-modal size, and keep it fixed so users do not reveal empty #0e0e0e gutters around the React component.
       */
      return CGSize(width: 570, height: 568)
    case "renameSession":
      /*
       CDXC:SidebarRename 2026-06-12-02:50:
       Rename Session is a compact macOS app dialog, not a management surface. Size and lock the native child window to a fitted fixed frame so the shadcn dialog avoids the generic 640x460 gutters shown around the React form.

	      CDXC:SidebarRename 2026-06-12-05:05:
	      Rename Session needs 20px more horizontal room after the compact native-window pass. Widen the native child window to 570px so the React dialog can grow without reintroducing oversized generic modal gutters.

	      CDXC:SidebarRename 2026-06-12-06:35:
	      Rename Session needs 80px more native-window height so the generated-name controls and bottom action area are not clipped in the macOS app. Keep the 570px width while raising the fixed child window to 480px tall.
	      */
	      return CGSize(width: 570, height: 480)
    case "delayedSend":
      /*
       CDXC:DelayedSend 2026-06-12-04:07:
       Delayed Send is a small timer dialog. Size and lock the native child window to 472x269 so AppKit does not inherit the generic app-modal frame or clamp it back up through the shared minimum.
       */
      return CGSize(width: 472, height: 269)
    case "pinnedPrompts", "daemonSessions":
      return CGSize(width: 760, height: 680)
    case "scratchPad", "addRepository", "remoteProjectPicker":
      return CGSize(width: 760, height: 640)
    case "commandPalette":
      /*
       CDXC:CommandPalette 2026-06-12-05:04:
       The macOS Command Palette native window should be 15px tighter on each horizontal side than the previous 720px frame while giving the React/WebView surface 15px more vertical room at the bottom. Use a 690x535 content size so the centered palette keeps the narrower left/right footprint and has more command-list area.
       */
      return CGSize(width: 690, height: 535)
    default:
      return CGSize(width: 640, height: 460)
    }
  }

  private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize {
    constrainedSize(size, parentWindow: parentWindow, modal: nil)
  }

  private func constrainedSize(
    _ size: CGSize,
    parentWindow: NSWindow,
    modal: String?
  ) -> CGSize {
    let visibleFrame = parentWindow.screen?.visibleFrame ?? parentWindow.frame
    let minimumSize = minimumContentSize(for: modal)
    let maxWidth = max(minimumSize.width, visibleFrame.width - Self.screenMargin * 2)
    let maxHeight = max(minimumSize.height, visibleFrame.height - Self.screenMargin * 2)
    return CGSize(
      width: min(max(size.width, minimumSize.width), maxWidth),
      height: min(max(size.height, minimumSize.height), maxHeight)
    )
  }

  private func centeredFrame(size: CGSize, parentWindow: NSWindow) -> CGRect {
    let visibleFrame = parentWindow.screen?.visibleFrame ?? parentWindow.frame
    let frame = CGRect(
      x: parentWindow.frame.midX - size.width / 2,
      y: parentWindow.frame.midY - size.height / 2,
      width: size.width,
      height: size.height
    )
    return clampFrameToVisibleScreen(frame, visibleFrame: visibleFrame)
  }

  private func commandPaletteContentFrame(size: CGSize, parentWindow: NSWindow) -> CGRect {
    /*
     CDXC:CommandPalette 2026-06-12-05:14:
     The palette width reduction should stay centered so 15px is removed from both left and right. The new 15px of height is bottom-only, so compute placement from the old 520px centered top edge and extend the content frame downward to 535px.
     */
    let visibleFrame = parentWindow.screen?.visibleFrame ?? parentWindow.frame
    let previousCenteredHeight: CGFloat = 520
    let bottomOnlyHeightIncrease = max(0, size.height - previousCenteredHeight)
    let frame = CGRect(
      x: parentWindow.frame.midX - size.width / 2,
      y: parentWindow.frame.midY - previousCenteredHeight / 2 - bottomOnlyHeightIncrease,
      width: size.width,
      height: size.height
    )
    return clampFrameToVisibleScreen(frame, visibleFrame: visibleFrame)
  }

  private func gitCommitContentFrame(size: CGSize, parentWindow: NSWindow) -> CGRect {
    /*
     CDXC:TitlebarGit 2026-06-12-11:30:
     Widening commit review should add space on the right only. Keep the old
     1000px centered frame's left edge as the anchor so the files/message column
     stays visually fixed while the right diff pane gains the extra 20px.
     */
    let visibleFrame = parentWindow.screen?.visibleFrame ?? parentWindow.frame
    let previousCenteredWidth: CGFloat = 1000
    let frame = CGRect(
      x: parentWindow.frame.midX - previousCenteredWidth / 2,
      y: parentWindow.frame.midY - size.height / 2,
      width: size.width,
      height: size.height
    )
    return clampFrameToVisibleScreen(frame, visibleFrame: visibleFrame)
  }

  private func clampFrameToVisibleScreen(_ frame: CGRect, visibleFrame: CGRect) -> CGRect {
    var frame = frame
    frame.origin.x = min(
      max(frame.origin.x, visibleFrame.minX + Self.screenMargin),
      visibleFrame.maxX - frame.width - Self.screenMargin)
    frame.origin.y = min(
      max(frame.origin.y, visibleFrame.minY + Self.screenMargin),
      visibleFrame.maxY - frame.height - Self.screenMargin)
    return frame
  }

  private func constrainedContentFrame(
    preferredContentFrame: CGRect?,
    size: CGSize,
    parentWindow: NSWindow,
    modal: String? = nil
  ) -> CGRect {
    guard let preferredContentFrame else {
      if modal == "commandPalette" {
        return commandPaletteContentFrame(size: size, parentWindow: parentWindow)
      }
      if modal == "gitCommit" {
        return gitCommitContentFrame(size: size, parentWindow: parentWindow)
      }
      return centeredFrame(size: size, parentWindow: parentWindow)
    }
    let visibleFrame = parentWindow.screen?.visibleFrame ?? parentWindow.frame
    var frame = CGRect(origin: preferredContentFrame.origin, size: size)
    frame.origin.x = min(
      max(frame.origin.x, visibleFrame.minX + Self.screenMargin),
      visibleFrame.maxX - size.width - Self.screenMargin)
    frame.origin.y = min(
      max(frame.origin.y, visibleFrame.minY + Self.screenMargin),
      visibleFrame.maxY - size.height - Self.screenMargin)
    return frame
  }

  private func shouldLockContentSize(modal: String) -> Bool {
    modal == "previousSessions" || modal == "renameSession" || modal == "delayedSend"
      || modal == "worktree"
  }

  private func minimumContentSize(for modal: String?) -> CGSize {
    switch modal {
    case "delayedSend":
      return CGSize(width: 472, height: 269)
    case "floatingPromptEditor":
      return Self.floatingPromptEditorMinimumSize
    default:
      return Self.minimumSize
    }
  }

  private func appModalStyleMask(for modal: String) -> NSWindow.StyleMask {
    if modal == "floatingPromptEditor" {
      return .borderless
    }
    return .borderless
  }

  private func configurePanelChrome(_ panel: NSPanel, modal: String) {
    guard modal == "floatingPromptEditor" else {
      return
    }
    panel.hasShadow = true
    panel.isMovableByWindowBackground = false
  }

  private func title(for modal: String) -> String {
    switch modal {
    case "addRepository":
      return "Clone Repository"
    case "agentConfig", "configureAgents":
      return "Agents"
    case "agentsHub":
      return "Agents Hub"
    case "commandPalette":
      return "Command Palette"
    case "commandConfig", "configureActions":
      return "Actions"
    case "daemonSessions":
      return "Running Sessions"
    case "delayedSend":
      return "Delayed Send"
    case "previousSessions":
      return "Previous Sessions"
    case "firstLaunchSetup", "tipsAndTricks":
      return "Tips & Tricks"
    case "firstUserMessage":
      return "First Message"
    case "floatingPromptEditor":
      return "Prompt Editor"
    case "gitCommit":
      return "Git Commit"
    case "gitFileDiff":
      return "File Diff"
    case "hotkeys":
      return "Hotkeys"
    case "openTargets":
      return "Open Targets"
    case "pinnedPrompts":
      return "Pinned Prompts"
    case "remoteGxserverInstall":
      return "Remote Setup"
    case "remoteProjectPicker":
      return "Add Remote Project"
    case "renameSession":
      return "Rename Session"
    case "scratchPad":
      return "Scratch Pad"
    case "settings":
      return "Settings"
    case "t3BrowserAccess":
      return "Browser Access"
    case "t3ThreadId":
      return "Thread ID"
    case "worktree":
      return "Worktree"
    case "deleteWorktree":
      return "Delete Worktree"
    default:
      return "Ghostex"
    }
  }
}

private final class TitlebarDropdownPanelController: NSObject, NSWindowDelegate, WKNavigationDelegate {
  private static let screenMargin: CGFloat = 8
  private static let panelGap: CGFloat = 6

  private let scriptBridge: SidebarScriptBridge
  private let bootstrapScriptSource: String?
  private let diagnosticsScript: String
  private let shouldLetTitlebarHandleOutsideMouseEvent: (NSEvent) -> Bool
  private let onNativeDropdownOpenChanged: (String?) -> Void

  private weak var parentWindow: NSWindow?
  private var panel: NSPanel?
  private var webView: WKWebView?
  private var currentKind: String?
  private var currentAnchorScreenRect = CGRect.zero
  private var latestStateJson: String?
  private var outsideEventMonitor: Any?
  private var appDeactivateObserver: NSObjectProtocol?

  init(
    scriptBridge: SidebarScriptBridge,
    bootstrapScriptSource: String?,
    diagnosticsScript: String,
    shouldLetTitlebarHandleOutsideMouseEvent: @escaping (NSEvent) -> Bool,
    onNativeDropdownOpenChanged: @escaping (String?) -> Void
  ) {
    self.scriptBridge = scriptBridge
    self.bootstrapScriptSource = bootstrapScriptSource
    self.diagnosticsScript = diagnosticsScript
    self.shouldLetTitlebarHandleOutsideMouseEvent = shouldLetTitlebarHandleOutsideMouseEvent
    self.onNativeDropdownOpenChanged = onNativeDropdownOpenChanged
    super.init()
  }

  deinit {
    close()
  }

  func show(
    kind: String,
    anchorScreenRect: CGRect,
    parentWindow: NSWindow,
    preferredSize: CGSize?,
    webAssets: URL,
    latestStateJson: String?
  ) {
    /*
     CDXC:ReactTitlebar 2026-06-11-13:22:
     Titlebar dropdowns must be native child windows that render the existing
     React dropdown surfaces in their own WKWebView document. The main titlebar
     WKWebView stays clipped to the 35px strip so no transparent overlay sits
     above the CEF/WKWebView workspace during editor drag/drop.

     CDXC:ReactTitlebar 2026-06-11-15:58:
     Loading a local titlebar-host.html file with synthetic query parameters is
     fragile during WebKit panel creation. Inject the panel kind at document
     start and keep the loaded file URL as the real resource path.

     CDXC:ReactTitlebar 2026-06-11-18:06:
     Resources sometimes rendered with default titlebar state because the
     post-load setActiveProjectState call could run before React installed its
     bridge. Keep the latest payload in the document-start bootstrap so child
     panels render current project/resource state on their first React pass.

     CDXC:TitlebarResources 2026-06-11-18:13:
     The Resources panel also waits for an async process snapshot. Load that
     child WKWebView hidden and order the panel onscreen only after React reports
     that the first non-loading resources view has committed.

     CDXC:ReactTitlebar 2026-06-12-02:50:
     Short titlebar dropdowns should size from their rendered option count
     before the native child panel opens, so menus such as Git, Open In, and
     Actions do not hide rows below a fixed compact panel fold.
     */
    close(notifyTitlebar: false)
    self.parentWindow = parentWindow
    self.currentKind = kind
    self.currentAnchorScreenRect = anchorScreenRect
    self.latestStateJson = latestStateJson

    let size = constrainedSize(preferredSize ?? defaultSize(for: kind), parentWindow: parentWindow)
    let frame = panelFrame(size: size, parentWindow: parentWindow)
    let panel = NSPanel(
      contentRect: frame,
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false
    )
    panel.backgroundColor = NSColor(
      srgbRed: 14.0 / 255.0,
      green: 14.0 / 255.0,
      blue: 14.0 / 255.0,
      alpha: 1.0)
    panel.collectionBehavior = [.fullScreenAuxiliary, .transient]
    panel.delegate = self
    panel.hasShadow = true
    panel.hidesOnDeactivate = false
    panel.isMovable = false
    panel.isOpaque = true
    panel.isReleasedWhenClosed = false
    panel.level = parentWindow.level

    let webView = WKWebView(
      frame: CGRect(origin: .zero, size: frame.size),
      configuration: makeConfiguration(panelKind: kind))
    webView.autoresizingMask = [.width, .height]
    webView.navigationDelegate = self
    /*
     CDXC:ReactTitlebar 2026-06-11-20:43:
     Native titlebar dropdown panels also render transparent WKWebView content.
     Use the same #0e0e0e backing as modal child windows so reused React
     dropdown surfaces cannot show a darker host border during first paint.
     */
    webView.wantsLayer = true
    webView.layer?.backgroundColor = ghostexReferenceSidebarChromeBackgroundColor.cgColor
    webView.setValue(false, forKey: "drawsBackground")
    panel.contentView = webView

    self.panel = panel
    self.webView = webView
    loadPanel(webAssets: webAssets, webView: webView)
    installOutsideEventMonitors()
    if kind == "resources" {
      onNativeDropdownOpenChanged(kind)
    } else {
      showWhenReady(kind: kind)
    }
  }

  func showWhenReady(kind: String) {
    guard kind == currentKind,
      let parentWindow,
      let panel,
      !panel.isVisible
    else {
      return
    }
    parentWindow.addChildWindow(panel, ordered: .above)
    panel.orderFront(nil)
    onNativeDropdownOpenChanged(kind)
  }

  func close() {
    close(notifyTitlebar: true)
  }

  private func close(notifyTitlebar: Bool) {
    removeOutsideEventMonitors()
    if let panel {
      parentWindow?.removeChildWindow(panel)
      panel.delegate = nil
      panel.orderOut(nil)
      panel.contentView = nil
    }
    webView?.navigationDelegate = nil
    webView = nil
    panel = nil
    currentKind = nil
    if notifyTitlebar {
      onNativeDropdownOpenChanged(nil)
    }
  }

  func resize(kind: String, width: CGFloat, height: CGFloat) {
    guard kind == currentKind,
      width.isFinite,
      height.isFinite,
      width > 0,
      height > 0
    else {
      return
    }
    /*
     CDXC:ReactTitlebar 2026-06-11-16:46:
     WebKit can briefly report viewport-coupled wrapper sizes after right-click
     opens, and applying those values creates a ResizeObserver/AppKit feedback
     loop that collapses the child panel until it disappears.

     CDXC:ReactTitlebar 2026-06-12-02:50:
     The native dropdown contract uses React's pre-open preferred size instead
     of post-open DOM measurement. Ignore resize messages so WebKit content
     layout cannot shrink the NSPanel after it opens.
     */
  }

  func setActiveProjectState(_ json: String) {
    latestStateJson = json
    applyLatestState()
  }

  func dispatchHostEventScript(_ script: String) {
    webView?.evaluateJavaScript(script)
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

  func debugSnapshot() -> [String: Any] {
    [
      "anchorScreenRect": Self.describeFrame(currentAnchorScreenRect),
      "currentKind": currentKind ?? "<none>",
      "hasAppDeactivateObserver": appDeactivateObserver != nil,
      "hasOutsideEventMonitor": outsideEventMonitor != nil,
      "panelFrame": panel.map { Self.describeFrame($0.frame) } ?? NSNull(),
      "panelIsVisible": panel?.isVisible ?? false,
      "present": panel != nil,
      "webViewFrame": webView.map { Self.describeFrame($0.frame) } ?? NSNull(),
      "webViewHidden": webView?.isHidden ?? false,
      "windowNumber": panel?.windowNumber ?? NSNull(),
    ]
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    applyLatestState()
  }

  private func makeConfiguration(panelKind: String) -> WKWebViewConfiguration {
    let configuration = WKWebViewConfiguration()
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexAppModalHost")
    configuration.userContentController.add(scriptBridge, name: "ghostexNativeHostDiagnostics")
    if let panelKindScript = Self.panelKindBootstrapScript(for: panelKind) {
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: panelKindScript,
          injectionTime: .atDocumentStart,
          forMainFrameOnly: true
        ))
    }
    if let bootstrapScriptSource {
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: bootstrapScriptSource,
          injectionTime: .atDocumentStart,
          forMainFrameOnly: true
        ))
    }
    if let latestStateJson,
      let latestStateBootstrapScript = Self.latestTitlebarStateBootstrapScript(
        json: latestStateJson)
    {
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: latestStateBootstrapScript,
          injectionTime: .atDocumentStart,
          forMainFrameOnly: true
        ))
    }
    configuration.userContentController.addUserScript(
      WKUserScript(
        source: diagnosticsScript,
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    return configuration
  }

  private func loadPanel(webAssets: URL, webView: WKWebView) {
    let builtTitlebarChrome = webAssets.appendingPathComponent("titlebar-host.html")
    guard FileManager.default.fileExists(atPath: builtTitlebarChrome.path) else {
      webView.loadHTMLString(
        "<!doctype html><html><body style=\"margin:0;background:#0e0e0e\"></body></html>",
        baseURL: webAssets
      )
      return
    }
    webView.loadFileURL(builtTitlebarChrome, allowingReadAccessTo: webAssets)
  }

  private static func panelKindBootstrapScript(for kind: String) -> String? {
    guard let kindLiteral = javascriptStringLiteral(kind) else {
      return nil
    }
    return "window.__ghostex_TITLEBAR_PANEL_KIND__ = \(kindLiteral);"
  }

  private static func latestTitlebarStateBootstrapScript(json: String) -> String? {
    guard !json.isEmpty else {
      return nil
    }
    return """
      (() => {
        const state = \(json);
        const host = window.__ghostex_NATIVE_HOST__ && typeof window.__ghostex_NATIVE_HOST__ === "object"
          ? window.__ghostex_NATIVE_HOST__
          : {};
        window.__ghostex_NATIVE_HOST__ = Object.assign({}, host, state);
        window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__ = state;
      })();
      """
  }

  private static func javascriptStringLiteral(_ value: String) -> String? {
    guard let data = try? JSONEncoder().encode(value) else {
      return nil
    }
    return String(data: data, encoding: .utf8)
  }

  private func applyLatestState() {
    guard let latestStateJson,
      let webView
    else {
      return
    }
    webView.evaluateJavaScript(
      """
      window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__ = \(latestStateJson);
      window.__ghostex_TITLEBAR__?.setActiveProjectState(\(latestStateJson));
      undefined;
      """)
  }

  private func installOutsideEventMonitors() {
    removeOutsideEventMonitors()
    outsideEventMonitor = NSEvent.addLocalMonitorForEvents(
      matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown, .keyDown]
    ) { [weak self] event in
      guard let self else {
        return event
      }
      if event.type == .keyDown,
        event.charactersIgnoringModifiers == "\u{1b}"
      {
        self.close()
        return nil
      }
      if event.type == .keyDown {
        return event
      }
      if let panel = self.panel,
        event.window === panel
      {
        return event
      }
      /*
       CDXC:ReactTitlebar 2026-06-11-23:24:
       Clicking the already-open titlebar dropdown trigger must let the React
       titlebar receive the repeat button click and close the native child
       window. The child panel outside monitor must not pre-close titlebar
       control clicks because that clears nativeDropdownOpen before the trigger
       can compare the current dropdown kind.
       */
      if self.shouldLetTitlebarHandleOutsideMouseEvent(event) {
        return event
      }
      self.close()
      return event
    }
    appDeactivateObserver = NotificationCenter.default.addObserver(
      forName: NSApplication.didResignActiveNotification,
      object: NSApp,
      queue: .main
    ) { [weak self] _ in
      self?.close()
    }
  }

  private func removeOutsideEventMonitors() {
    if let outsideEventMonitor {
      NSEvent.removeMonitor(outsideEventMonitor)
      self.outsideEventMonitor = nil
    }
    if let appDeactivateObserver {
      NotificationCenter.default.removeObserver(appDeactivateObserver)
      self.appDeactivateObserver = nil
    }
  }

  private func panelFrame(size: CGSize, parentWindow: NSWindow) -> CGRect {
    let visibleFrame = parentWindow.screen?.visibleFrame
      ?? NSScreen.main?.visibleFrame
      ?? CGRect(x: 0, y: 0, width: 1200, height: 800)
    let width = size.width
    let height = size.height
    var x = currentAnchorScreenRect.maxX - width
    x = min(
      max(x, visibleFrame.minX + Self.screenMargin),
      visibleFrame.maxX - width - Self.screenMargin)
    var y = currentAnchorScreenRect.minY - height - Self.panelGap
    if y < visibleFrame.minY + Self.screenMargin {
      y = min(
        currentAnchorScreenRect.maxY + Self.panelGap,
        visibleFrame.maxY - height - Self.screenMargin)
    }
    return CGRect(x: x, y: y, width: width, height: height)
  }

  private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize {
    let visibleFrame = parentWindow.screen?.visibleFrame
      ?? NSScreen.main?.visibleFrame
      ?? CGRect(x: 0, y: 0, width: 1200, height: 800)
    let maxWidth = max(160, visibleFrame.width - Self.screenMargin * 2)
    let maxHeight = max(120, visibleFrame.height - Self.screenMargin * 2)
    return CGSize(
      width: min(max(size.width, 160), maxWidth),
      height: min(max(size.height, 1), maxHeight))
  }

  private func defaultSize(for kind: String) -> CGSize {
    switch kind {
    case "resources":
      return CGSize(width: 656, height: 650)
    case "tips":
      /*
       CDXC:TipsAndTricks 2026-06-12-08:56:
       The macOS Tips & Tricks child panel should be 100px narrower than the
       Resources reading panel. Keep Swift's native fallback aligned with the
       titlebar React preferred size so stale or missing preferred-size payloads
       do not reopen the wider frame.
       */
      return CGSize(width: 556, height: 650)
    default:
      return CGSize(width: 240, height: 130)
    }
  }
}

final class ReactTitlebarChromeView: NSView {
  var titlebarHeight: CGFloat = 30
  private let webView: WKWebView
  private var hitRegions: [CGRect] = []
  private var overlayOpen = false
  private var frameBeforeTitlebarMaximize: NSRect?
  private var windowStateObserverTokens: [NSObjectProtocol] = []
  private var nativePointerInside: Bool?
  private var nativePointerTrackingArea: NSTrackingArea?

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

  deinit {
    removeWindowStateObservers()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    installWindowStateObservers()
    updateTitlebarWebViewFrame(reason: "viewDidMoveToWindow")
    refreshNativePointerInside()
  }

  override func layout() {
    super.layout()
    updateTitlebarWebViewFrame(reason: "layout")
    refreshNativePointerInside()
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let nativePointerTrackingArea {
      removeTrackingArea(nativePointerTrackingArea)
    }
    /*
     CDXC:ReactTitlebar 2026-06-10-23:44:
     The titlebar WKWebView is now clipped to the fixed titlebar strip, so native
     pointer ownership can follow the strip itself instead of measured React
     hit rectangles. WebKit receives normal movement inside the strip and clears
     hover the same way the sidebar WKWebView does.
     */
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeAlways, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
      owner: self,
      userInfo: nil
    )
    nativePointerTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func mouseEntered(with event: NSEvent) {
    updateNativePointerInside(for: event)
    super.mouseEntered(with: event)
  }

  override func mouseMoved(with event: NSEvent) {
    updateNativePointerInside(for: event)
    super.mouseMoved(with: event)
  }

  override func mouseExited(with event: NSEvent) {
    setNativePointerInside(false)
    super.mouseExited(with: event)
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
     Native hit-testing allows events through only when a point lands inside one
     of these reported regions or inside the fixed blank titlebar drag strip.

     CDXC:ReactTitlebar 2026-05-25-10:27:
     Below-titlebar regions are valid only while React says a titlebar dropdown
     is open. Stale dropdown measurements must not keep routing workspace clicks
     into the titlebar WKWebView after the dropdown closes.

     CDXC:ReactTitlebar 2026-06-08-06:33:
     Native hit-testing restricts normal pointer events to measured React hit
     regions plus the blank titlebar drag strip.
     */
    needsLayout = true
    updateTitlebarWebViewFrame(reason: "hitRegionsUpdated")
    refreshNativePointerInside()
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

  func setNativeDropdownOpen(_ kind: String?) {
    let argument = kind.flatMap(Self.javascriptStringLiteral) ?? "undefined"
    webView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.setNativeDropdownOpen?.(\(argument));
      undefined;
      """)
  }

  func routeWindowMouseEvent(_ event: NSEvent) -> Bool {
    guard Self.isTitlebarWindowMouseEvent(event.type), event.window === window else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)
    guard isPointInFixedTitlebarStrip(point) else {
      return false
    }
    setNativePointerInside(true)

    let containsInteractiveRegion = containsInteractiveHitRegion(point)
    let shouldDispatch = containsInteractiveRegion || event.type == .mouseMoved
    guard shouldDispatch else {
      return false
    }

    /*
     CDXC:ReactTitlebar 2026-06-11-22:10:
     AppKit's titled-window chrome can swallow real clicks in the 35px
     full-size content titlebar before the titlebar WKWebView receives a normal
     hit-test. Dispatch only measured titlebar controls directly to WebKit,
     leaving blank titlebar drag and workspace browser drag/drop outside this route.
     */
    dispatchWindowMouseEventToWebView(event, point: point)
    let shouldConsume = containsInteractiveRegion && Self.shouldConsumeTitlebarWindowMouseEvent(event.type)
    return shouldConsume
  }

  private func updateNativePointerInside(for event: NSEvent) {
    setNativePointerInside(isPointInFixedTitlebarStrip(convert(event.locationInWindow, from: nil)))
  }

  private func refreshNativePointerInside() {
    guard let window else {
      setNativePointerInside(false)
      return
    }
    setNativePointerInside(
      isPointInFixedTitlebarStrip(convert(window.mouseLocationOutsideOfEventStream, from: nil)))
  }

  private func setNativePointerInside(_ isInside: Bool) {
    guard nativePointerInside != isInside else {
      return
    }
    nativePointerInside = isInside
    webView.evaluateJavaScript(
      """
      window.__ghostex_TITLEBAR__?.setNativePointerInside?.(\(isInside ? "true" : "false"));
      undefined;
      """)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      setNativePointerInside(false)
      return nil
    }
    if isPointInFixedTitlebarStrip(point) {
      setNativePointerInside(true)
      if containsInteractiveHitRegion(point) {
        /*
         CDXC:ReactTitlebar 2026-06-11-22:10:
         User clicks on visible titlebar controls were reaching the titlebar
         strip but not React when AppKit targeted WebKit's private inner view in
         the full-size window titlebar area. Return the titlebar WKWebView
         itself for measured controls so its non-draggable mouse contract owns
         the event path; keep blank strip pixels on the wrapper for window drag.
         */
        return webView
      }
      return self
    }
    setNativePointerInside(false)
    return nil
  }

  override func mouseDown(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    guard isPointInFixedTitlebarStrip(point) else {
      return
    }
    let isInteractive = containsInteractiveHitRegion(point)
    if !isInteractive {
      /*
       CDXC:ProjectEditorCompanion 2026-05-27-08:42:
       The Companion Sidepane restore button repro showed clicks landing in
       ReactTitlebarChromeView instead of the titlebar WKWebView. Record the
       blank-titlebar click point and closest React hit regions so the next
       repro can prove whether the button rect was missing, stale, or simply
       missed by the pointer.
       */
      let webPoint = CGPoint(x: point.x, y: bounds.height - point.y)
      AppDelegate.appendNativeHostLifecycleLog(
        "reactTitlebar.blankStripMouseDown eventNumber=\(event.eventNumber) point=(\(formatPoint(point))) webPoint=(\(formatPoint(webPoint))) overlayOpen=\(overlayOpen) hitRegionCount=\(hitRegions.count) closestHitRegions=\(closestHitRegionDescription(to: webPoint))"
      )
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
     The React titlebar wrapper is the fixed titlebar strip, but it must not
     globally advertise itself as movable window background. Dragging and
     double-click maximize are allowed only by mouseDown after revalidating the
     event is inside the fixed top strip.
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

  private func updateTitlebarWebViewFrame(reason: String) {
    /*
     CDXC:ReactTitlebar 2026-06-08-06:33:
     The titlebar WKWebView frame follows the native wrapper bounds.

     CDXC:ReactTitlebar 2026-06-11-13:22:
     The wrapper bounds are the fixed titlebar strip; dropdown space belongs to
     native child panels, not a full-window titlebar portal host.
     */
    let target = titlebarWebViewFootprint()
    let didChange = !NSEqualRects(webView.frame, target.frame)
    if didChange {
      webView.frame = target.frame
    }
  }

  private func titlebarWebViewFootprint() -> (frame: NSRect, mode: String) {
    /*
     CDXC:ReactTitlebar 2026-06-11-13:22:
     The titlebar wrapper itself is now laid out as the fixed titlebar strip.
     Returning bounds keeps the embedded titlebar WKWebView clipped to that
     strip in every state; dropdown panels are separate native child windows.
     */
    return (bounds, "titlebarStrip")
  }

  @discardableResult
  private func dispatchWindowMouseEventToWebView(_ event: NSEvent, point: NSPoint) -> NSView? {
    let webPoint = convert(point, to: webView)
    let target = webView.hitTest(webPoint) ?? webView
    switch event.type {
    case .leftMouseDown:
      target.mouseDown(with: event)
    case .leftMouseUp:
      target.mouseUp(with: event)
    case .rightMouseDown:
      target.rightMouseDown(with: event)
    case .rightMouseUp:
      target.rightMouseUp(with: event)
    case .otherMouseDown:
      target.otherMouseDown(with: event)
    case .otherMouseUp:
      target.otherMouseUp(with: event)
    case .leftMouseDragged:
      target.mouseDragged(with: event)
    case .rightMouseDragged:
      target.rightMouseDragged(with: event)
    case .otherMouseDragged:
      target.otherMouseDragged(with: event)
    case .mouseMoved:
      target.mouseMoved(with: event)
    default:
      break
    }
    return target
  }

  private static func isTitlebarWindowMouseEvent(_ eventType: NSEvent.EventType) -> Bool {
    switch eventType {
    case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown,
      .otherMouseUp, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged, .mouseMoved:
      return true
    default:
      return false
    }
  }

  private static func shouldConsumeTitlebarWindowMouseEvent(_ eventType: NSEvent.EventType) -> Bool {
    switch eventType {
    case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown,
      .otherMouseUp, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged:
      return true
    default:
      return false
    }
  }

  private static func javascriptStringLiteral(_ value: String) -> String? {
    guard let data = try? JSONEncoder().encode(value) else {
      return nil
    }
    return String(data: data, encoding: .utf8)
  }

  private func installWindowStateObservers() {
    removeWindowStateObservers()
    let center = NotificationCenter.default
    if let window {
      windowStateObserverTokens.append(center.addObserver(
        forName: NSWindow.didBecomeKeyNotification,
        object: window,
        queue: .main
      ) { [weak self] _ in
        self?.updateTitlebarWebViewFrame(reason: "windowDidBecomeKey")
        self?.refreshNativePointerInside()
      })
      windowStateObserverTokens.append(center.addObserver(
        forName: NSWindow.didResignKeyNotification,
        object: window,
        queue: .main
      ) { [weak self] _ in
        self?.updateTitlebarWebViewFrame(reason: "windowDidResignKey")
        self?.setNativePointerInside(false)
      })
    }
    windowStateObserverTokens.append(center.addObserver(
      forName: NSApplication.didBecomeActiveNotification,
      object: NSApp,
      queue: .main
    ) { [weak self] _ in
      self?.updateTitlebarWebViewFrame(reason: "appDidBecomeActive")
      self?.refreshNativePointerInside()
    })
    windowStateObserverTokens.append(center.addObserver(
      forName: NSApplication.didResignActiveNotification,
      object: NSApp,
      queue: .main
    ) { [weak self] _ in
      self?.updateTitlebarWebViewFrame(reason: "appDidResignActive")
      self?.setNativePointerInside(false)
    })
  }

  private func removeWindowStateObservers() {
    let center = NotificationCenter.default
    for token in windowStateObserverTokens {
      center.removeObserver(token)
    }
    windowStateObserverTokens.removeAll()
  }

  private func formatPoint(_ point: CGPoint) -> String {
    "x=\(String(format: "%.1f", point.x)),y=\(String(format: "%.1f", point.y))"
  }

  private func formatRect(_ rect: CGRect) -> String {
    "x=\(String(format: "%.1f", rect.minX)),y=\(String(format: "%.1f", rect.minY)),w=\(String(format: "%.1f", rect.width)),h=\(String(format: "%.1f", rect.height))"
  }

  private func closestHitRegionDescription(to point: CGPoint) -> String {
    hitRegions.enumerated()
      .map { index, region -> (String, CGFloat) in
        let closestX = min(max(point.x, region.minX), region.maxX)
        let closestY = min(max(point.y, region.minY), region.maxY)
        let distance = hypot(point.x - closestX, point.y - closestY)
        return ("#\(index){\(formatRect(region)),contains=\(region.contains(point)),distance=\(String(format: "%.1f", distance))}", distance)
      }
      .sorted { $0.1 < $1.1 }
      .prefix(4)
      .map(\.0)
      .joined(separator: ";")
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
      let sanitizedDiagnostic = NativeLogPrivacy.sanitizeLogLine(diagnostic)
      if diagnostic.contains("diagnostics-ready") {
        if NativeDebugLogging.isEnabled {
          Self.logger.info("Sidebar diagnostic: \(sanitizedDiagnostic, privacy: .public)")
        }
      } else {
        Self.logger.error("Sidebar diagnostic: \(sanitizedDiagnostic, privacy: .public)")
      }
      return
    }

    if message.name == "ghostexAppModalHost" {
      router.onAppModalHostMessage?(message.body)
      return
    }

    guard JSONSerialization.isValidJSONObject(message.body) else {
      let bodyDescription = sidebarBridgeBodyDescription(message.body)
      /**
       CDXC:EditorPanes 2026-05-08-13:31
       Sidebar-to-native editor commands must fail observably. A malformed
       WebKit bridge payload can otherwise drop createProjectEditorPane before
       focusProjectEditorPane runs, leaving the VS Code button apparently dead.

       CDXC:RemoteMachines 2026-06-09-18:41:
       The Remote SSH password save command carries a transient credential. Bridge
       failure diagnostics must redact that field before writing command bodies so
       malformed payload repro logs cannot capture SSH passwords.
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
      let bodyDescription = sidebarBridgeBodyDescription(message.body)
      let commandType = body?["type"] as? String ?? "<missing>"
      NativeT3CodePaneReproLog.append("nativeSidebar.bridge.command.decodeFailed", [
        "body": bodyDescription,
        "error": error.localizedDescription,
        "messageName": message.name,
        "type": commandType,
      ])
      let sanitizedError = NativeLogPrivacy.sanitizeLogLine(error.localizedDescription)
      Self.logger.error(
        "Sidebar command decode failed type=\(commandType, privacy: .public) error=\(sanitizedError, privacy: .public)"
      )
    }
  }

  private func sidebarBridgeBodyDescription(_ body: Any) -> String {
    if var command = body as? [String: Any],
       command["type"] as? String == "remoteSshPasswordSave" {
      command["password"] = "[redacted]"
      return String(String(describing: command).prefix(1000))
    }
    if var command = body as? [String: Any],
       command["type"] as? String == "saveRemoteMachinePassword" {
      command["password"] = "[redacted]"
      return String(String(describing: command).prefix(1000))
    }
    return String(String(describing: body).prefix(1000))
  }
}

final class SidebarCommandRouter {
  var onAppModalHostMessage: ((Any) -> Void)?
  var onCommand: ((HostCommand) -> Void)?
}
