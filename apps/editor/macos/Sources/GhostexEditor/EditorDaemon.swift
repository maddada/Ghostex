import AppKit
import Darwin
import Foundation

final class EditorDaemon: NSObject, NSApplicationDelegate {
  private static let savedWindowFrameDefaultsKey = "GhostexEditor.savedWindowFrame"
  private static let minimumWindowSize = NSSize(width: 480, height: 320)

  private let socketPath: String
  private var listenerFileDescriptor: Int32
  private let acceptQueue = DispatchQueue(label: "com.madda.ghostex.editor.accept")
  private var acceptSource: DispatchSourceRead?
  private var signalSources: [DispatchSourceSignal] = []
  private var connections: [UUID: ClientConnection] = [:]
  private var openCountWatcherIds: Set<UUID> = []
  private var sessions: [String: EditorSession] = [:]
  private var warmWindow: EditorWindowController?
  private var retiredWindows: [EditorWindowController] = []
  private var warmWaiters: [() -> Void] = []
  private var pendingShutdown = false
  private var isExiting = false
  private var nextCascadeTopLeft: NSPoint?
  private var webRoot: URL?
  private var indexURL: URL?
  private var lastExternalFrontmostApplication: NSRunningApplication?
  private var lastCursorSnapshot: EditorCursorSnapshot?

  init(socketPath: String, listenerFileDescriptor: Int32) {
    self.socketPath = socketPath
    self.listenerFileDescriptor = listenerFileDescriptor
    super.init()
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.accessory)
    installMainMenu()

    guard let webRoot = resolveWebRoot() else {
      writeStderr("GhostexEditor: Unable to resolve Ghostex Editor web root.\n")
      cleanupAndExit(2)
      return
    }
    let indexURL = webRoot.appendingPathComponent("index.html", isDirectory: false)
    guard FileManager.default.fileExists(atPath: indexURL.path) else {
      writeStderr("GhostexEditor: Missing editor web entry at \(indexURL.path).\n")
      cleanupAndExit(2)
      return
    }

    self.webRoot = webRoot
    self.indexURL = indexURL
    installSignalHandlers()
    startAcceptingConnections()
    ensureWarmWindow()
  }

  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    saveAllSessionsAndExit()
    return .terminateCancel
  }

  private func installMainMenu() {
    /*
     * WKWebView only performs clipboard operations for Cmd+X/C/V/A when the
     * application main menu carries the standard Edit actions, so a menu-less
     * accessory daemon silently drops those shortcuts. Undo/Redo stay off the
     * menu on purpose: Monaco owns its undo stack via keydown, and a native
     * undo: menu item would capture Cmd+Z before the page sees it.
     */
    let mainMenu = NSMenu()

    let appMenuItem = NSMenuItem()
    let appMenu = NSMenu()
    appMenu.addItem(
      withTitle: "Quit Ghostex Editor",
      action: #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q"
    )
    appMenuItem.submenu = appMenu
    mainMenu.addItem(appMenuItem)

    let editMenuItem = NSMenuItem()
    let editMenu = NSMenu(title: "Edit")
    editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    editMenu.addItem(
      withTitle: "Select All",
      action: #selector(NSText.selectAll(_:)),
      keyEquivalent: "a"
    )
    editMenuItem.submenu = editMenu
    mainMenu.addItem(editMenuItem)

    NSApp.mainMenu = mainMenu
  }

  func handleRequest(_ request: [String: Any], from connection: ClientConnection) {
    guard intValue(request["v"]) == ghostexEditorProtocolVersion else {
      connection.sendError("unsupported protocol version")
      return
    }
    guard let type = request["type"] as? String else {
      connection.sendError("missing request type")
      return
    }

    switch type {
    case "ping":
      connection.send([
        "type": "pong",
        "v": ghostexEditorProtocolVersion,
        "openCount": sessions.count,
        "warm": warmWindowIsReady,
      ])
    case "warm":
      handleWarm(connection)
    case "open":
      handleOpen(request, from: connection)
    case "close":
      handleClose(request, from: connection)
    case "status":
      let sessionList = sessions.values.map { session in
        ["requestId": session.requestId, "title": session.title]
      }
      connection.send([
        "type": "status",
        "v": ghostexEditorProtocolVersion,
        "sessions": sessionList,
        "warm": warmWindowIsReady,
      ])
    case "front":
      handleFront(request, connection)
    case "retitle":
      /*
       * No-reply notification: the CLI resolves the originating terminal
       * session's display title from gxserver after `open`, so a reply (or an
       * unknown-requestId error for a session that already closed) would only
       * inject noise into the opener's opened/closed message waiters.
       */
      if let requestId = request["requestId"] as? String,
        let title = request["title"] as? String,
        !title.isEmpty,
        let session = sessions[requestId]
      {
        session.retitle(title)
      }
    case "watch":
      /*
       * Watch subscriptions push openCount changes over a held-open
       * connection so hosts (the Ghostex titlebar) can reflect editor windows
       * the moment they open or close instead of polling with ping.
       */
      openCountWatcherIds.insert(connection.id)
      connection.send([
        "type": "watching",
        "v": ghostexEditorProtocolVersion,
        "openCount": sessions.count,
      ])
    case "shutdown":
      pendingShutdown = true
      connection.send(["type": "ok", "v": ghostexEditorProtocolVersion]) { [weak self] in
        DispatchQueue.main.async {
          guard let self else {
            return
          }
          if self.sessions.isEmpty {
            self.cleanupAndExit(0)
          }
        }
      }
    default:
      connection.sendError("unknown request type")
    }
  }

  func connectionClosed(_ connection: ClientConnection) {
    connections.removeValue(forKey: connection.id)
    openCountWatcherIds.remove(connection.id)
  }

  private func notifyOpenCountWatchers() {
    guard !openCountWatcherIds.isEmpty else {
      return
    }
    let message: [String: Any] = [
      "type": "openCountChanged",
      "v": ghostexEditorProtocolVersion,
      "openCount": sessions.count,
    ]
    for watcherId in openCountWatcherIds {
      connections[watcherId]?.send(message)
    }
  }

  func warmWindowDidBecomeReady(_ controller: EditorWindowController) {
    guard controller === warmWindow, controller.session == nil else {
      return
    }
    let waiters = warmWaiters
    warmWaiters.removeAll()
    waiters.forEach { $0() }
  }

  func applySavedFrameOrCascade(_ window: NSWindow) {
    if let frame = savedWindowFrame(for: window) {
      window.setFrame(frame, display: false)
      return
    }
    cascade(window)
  }

  func saveWindowFrame(_ window: NSWindow) {
    UserDefaults.standard.set(
      NSStringFromRect(window.frame), forKey: Self.savedWindowFrameDefaultsKey)
  }

  func cascade(_ window: NSWindow) {
    if nextCascadeTopLeft == nil {
      let frame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
      nextCascadeTopLeft = NSPoint(x: frame.minX + 80, y: frame.maxY - 60)
    }
    if let topLeft = nextCascadeTopLeft {
      nextCascadeTopLeft = window.cascadeTopLeft(from: topLeft)
    }
  }

  func captureReturnFocusApplication() -> NSRunningApplication? {
    /*
     * The daemon steals app-level focus when it presents an editor window, so
     * the app that was frontmost at present time is the terminal app the user
     * pressed Ctrl+G in. When a second editor opens while an editor window is
     * already key, the frontmost app is this daemon itself; keep the last
     * external app so that session can still return focus to the terminal.
     */
    if let frontmost = NSWorkspace.shared.frontmostApplication,
      frontmost.processIdentifier != ProcessInfo.processInfo.processIdentifier
    {
      lastExternalFrontmostApplication = frontmost
    }
    return lastExternalFrontmostApplication
  }

  private func savedWindowFrame(for window: NSWindow) -> NSRect? {
    guard let stored = UserDefaults.standard.string(forKey: Self.savedWindowFrameDefaultsKey) else {
      return nil
    }
    let frame = NSRectFromString(stored)
    guard frame.width.isFinite,
      frame.height.isFinite,
      frame.minX.isFinite,
      frame.minY.isFinite,
      frame.width > 0,
      frame.height > 0
    else {
      return nil
    }
    return Self.clampedWindowFrame(frame, preferredScreen: window.screen ?? NSScreen.main)
  }

  private static func clampedWindowFrame(_ frame: NSRect, preferredScreen: NSScreen?) -> NSRect {
    let visibleFrame = visibleFrameForRestoring(frame, preferredScreen: preferredScreen)
    let width = min(
      max(frame.width, minimumWindowSize.width), max(visibleFrame.width, minimumWindowSize.width))
    let height = min(
      max(frame.height, minimumWindowSize.height),
      max(visibleFrame.height, minimumWindowSize.height))
    let maxX = max(visibleFrame.minX, visibleFrame.maxX - width)
    let maxY = max(visibleFrame.minY, visibleFrame.maxY - height)
    return NSRect(
      x: min(max(frame.minX, visibleFrame.minX), maxX),
      y: min(max(frame.minY, visibleFrame.minY), maxY),
      width: width,
      height: height
    )
  }

  private static func visibleFrameForRestoring(_ frame: NSRect, preferredScreen: NSScreen?)
    -> NSRect
  {
    if let matchingScreen = NSScreen.screens.first(where: { $0.visibleFrame.intersects(frame) }) {
      return matchingScreen.visibleFrame
    }
    if let preferredScreen {
      return preferredScreen.visibleFrame
    }
    if let mainScreen = NSScreen.main {
      return mainScreen.visibleFrame
    }
    if let firstScreen = NSScreen.screens.first {
      return firstScreen.visibleFrame
    }
    return NSRect(x: 0, y: 0, width: 1440, height: 900)
  }

  func sessionDidFinish(_ session: EditorSession) {
    if let cursorOffset = session.latestCursorOffset {
      lastCursorSnapshot = EditorCursorSnapshot(
        text: session.latestDraft, cursorOffset: cursorOffset)
    }
    sessions.removeValue(forKey: session.requestId)
    notifyOpenCountWatchers()
    let editorWindow = session.editorWindow
    session.editorWindow = nil
    let shouldExitAfterCleanup = pendingShutdown && sessions.isEmpty

    DispatchQueue.main.async { [weak self, editorWindow] in
      guard let self else {
        return
      }
      if let editorWindow {
        editorWindow.cleanup()
        self.retiredWindows.append(editorWindow)
      }
      if shouldExitAfterCleanup {
        self.cleanupAndExit(0)
      } else if !self.pendingShutdown {
        self.ensureWarmWindow()
      }
    }
  }

  private var warmWindowIsReady: Bool {
    guard let warmWindow else {
      return false
    }
    return warmWindow.isReady && warmWindow.session == nil
  }

  private func resolveWebRoot() -> URL? {
    if let override = ProcessInfo.processInfo.environment["GHOSTEX_EDITOR_WEB_ROOT"],
      !override.isEmpty
    {
      return standardizedFileURL(override)
    }
    return Bundle.main.resourceURL?.appendingPathComponent("Web", isDirectory: true)
  }

  private func startAcceptingConnections() {
    let source = DispatchSource.makeReadSource(
      fileDescriptor: listenerFileDescriptor, queue: acceptQueue)
    acceptSource = source
    source.setEventHandler { [weak self] in
      self?.acceptAvailableConnections()
    }
    source.resume()
  }

  private func acceptAvailableConnections() {
    while true {
      let acceptedFileDescriptor = accept(listenerFileDescriptor, nil, nil)
      if acceptedFileDescriptor >= 0 {
        DispatchQueue.main.async { [weak self] in
          guard let self else {
            Darwin.close(acceptedFileDescriptor)
            return
          }
          let connection = ClientConnection(
            fileDescriptor: acceptedFileDescriptor,
            daemon: self,
            readQueue: self.acceptQueue
          )
          self.connections[connection.id] = connection
          connection.start()
        }
        continue
      }

      if errno == EINTR {
        continue
      }
      if errno == EAGAIN || errno == EWOULDBLOCK {
        return
      }
      return
    }
  }

  private func handleWarm(_ connection: ClientConnection) {
    if warmWindowIsReady {
      connection.send(["type": "warmed", "v": ghostexEditorProtocolVersion])
      return
    }

    warmWaiters.append { [weak connection] in
      connection?.send(["type": "warmed", "v": ghostexEditorProtocolVersion])
    }
    ensureWarmWindow()
  }

  private func handleFront(_ request: [String: Any], _ connection: ClientConnection) {
    let originatingSessionId = normalizedOriginatingSessionId(request["originatingSessionId"])
    var frontedCount = 0
    for session in sessions.values {
      if let originatingSessionId, session.originatingSessionId != originatingSessionId {
        continue
      }
      guard let controller = session.editorWindow else {
        continue
      }
      controller.window.makeKeyAndOrderFront(nil)
      controller.webView.window?.makeFirstResponder(controller.webView)
      frontedCount += 1
    }
    if frontedCount > 0 {
      NSApp.activate(ignoringOtherApps: true)
    }
    connection.send([
      "type": "fronted",
      "v": ghostexEditorProtocolVersion,
      "frontedCount": frontedCount,
      "openCount": sessions.count,
    ])
  }

  private func handleOpen(_ request: [String: Any], from connection: ClientConnection) {
    guard let requestId = request["requestId"] as? String, !requestId.isEmpty else {
      connection.sendError("open request requires requestId")
      return
    }
    guard sessions[requestId] == nil else {
      connection.sendError("requestId already open")
      return
    }
    guard let filePath = request["filePath"] as? String, filePath.hasPrefix("/") else {
      connection.sendError("open request requires absolute filePath")
      return
    }
    guard let statusFilePath = request["statusFile"] as? String, statusFilePath.hasPrefix("/")
    else {
      connection.sendError("open request requires absolute statusFile")
      return
    }

    let fileURL = standardizedFileURL(filePath)
    let statusFileURL = standardizedFileURL(statusFilePath)
    let language = (request["language"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "markdown"
    let originatingSessionId = normalizedOriginatingSessionId(request["originatingSessionId"])
    let title = (request["title"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Prompt Editor"

    let initialText: String
    do {
      if FileManager.default.fileExists(atPath: fileURL.path) {
        initialText = try String(contentsOf: fileURL, encoding: .utf8)
      } else {
        initialText = ""
      }
    } catch {
      connection.sendError("unable to read file: \(error.localizedDescription)")
      return
    }

    do {
      let initialCursorOffset =
        lastCursorSnapshot?.text == initialText ? lastCursorSnapshot?.cursorOffset : nil
      let session = EditorSession(
        daemon: self,
        requestId: requestId,
        fileURL: fileURL,
        statusFileURL: statusFileURL,
        language: language,
        originatingSessionId: originatingSessionId,
        title: title,
        initialText: initialText,
        initialCursorOffset: initialCursorOffset,
        openerConnection: connection
      )
      sessions[requestId] = session

      let controller = try takeReadyWarmWindow() ?? makeEditorWindow()
      controller.configure(with: session)
      session.presentEditorWindow()
      notifyOpenCountWatchers()
      ensureWarmWindow()
    } catch {
      sessions.removeValue(forKey: requestId)
      connection.sendError("unable to open editor window: \(error.localizedDescription)")
    }
  }

  private func normalizedOriginatingSessionId(_ rawValue: Any?) -> String? {
    guard let value = rawValue as? String else {
      return nil
    }
    let parts = value.split(separator: ":", omittingEmptySubsequences: false)
    guard parts.count == 2,
      isValidOriginatingSessionIdPart(parts[0], prefix: "P"),
      isValidOriginatingSessionIdPart(parts[1], prefix: "G")
    else {
      return nil
    }
    return value
  }

  private func isValidOriginatingSessionIdPart(_ part: Substring, prefix: Character) -> Bool {
    let characters = Array(part)
    guard characters.count == 5,
      characters[0] == prefix,
      characters[1].isNumber
    else {
      return false
    }
    return characters.dropFirst(2).allSatisfy { character in
      character.isNumber || (character.isLetter && character.isLowercase)
    }
  }

  private func handleClose(_ request: [String: Any], from connection: ClientConnection) {
    guard let requestId = request["requestId"] as? String, !requestId.isEmpty else {
      connection.sendError("close request requires requestId")
      return
    }
    guard let action = request["action"] as? String else {
      connection.sendError("close request requires action")
      return
    }
    guard let session = sessions[requestId] else {
      connection.sendError("unknown requestId")
      return
    }

    switch action {
    case "save":
      connection.send(["type": "ok", "v": ghostexEditorProtocolVersion])
      session.requestSaveAndClose()
    case "cancel":
      connection.send(["type": "ok", "v": ghostexEditorProtocolVersion])
      session.finish(action: .cancel)
    default:
      connection.sendError("close action must be save or cancel")
    }
  }

  private func ensureWarmWindow() {
    guard !pendingShutdown else {
      return
    }
    guard warmWindow == nil else {
      return
    }
    do {
      warmWindow = try makeEditorWindow()
    } catch {
      writeStderr("GhostexEditor: unable to warm editor window: \(error.localizedDescription)\n")
    }
  }

  private func takeReadyWarmWindow() throws -> EditorWindowController? {
    guard warmWindowIsReady else {
      return nil
    }
    let controller = warmWindow
    warmWindow = nil
    return controller
  }

  private func makeEditorWindow() throws -> EditorWindowController {
    guard let webRoot, let indexURL else {
      throw ghostexError("Editor web root is not ready.")
    }
    return try EditorWindowController(daemon: self, webRoot: webRoot, indexURL: indexURL)
  }

  private func installSignalHandlers() {
    for signalNumber in [SIGTERM, SIGINT] {
      signal(signalNumber, SIG_IGN)
      let source = DispatchSource.makeSignalSource(signal: signalNumber, queue: .main)
      source.setEventHandler { [weak self] in
        self?.saveAllSessionsAndExit()
      }
      source.resume()
      signalSources.append(source)
    }
  }

  private func saveAllSessionsAndExit() {
    guard !isExiting else {
      return
    }
    pendingShutdown = true
    for session in Array(sessions.values) {
      session.finish(action: .save)
    }
    cleanupAndExit(0)
  }

  private func cleanupAndExit(_ code: Int32) {
    guard !isExiting else {
      return
    }
    isExiting = true

    acceptSource?.cancel()
    acceptSource = nil
    if listenerFileDescriptor >= 0 {
      Darwin.close(listenerFileDescriptor)
      listenerFileDescriptor = -1
    }
    for source in signalSources {
      source.cancel()
    }
    signalSources.removeAll()
    for connection in Array(connections.values) {
      connection.close()
    }
    connections.removeAll()
    warmWindow?.cleanup()
    warmWindow = nil
    removeSocketOnExit()
    exit(code)
  }

  private func removeSocketOnExit() {
    var status = stat()
    guard lstat(socketPath, &status) == 0 else {
      return
    }
    guard (status.st_mode & S_IFMT) == S_IFSOCK else {
      return
    }
    _ = unlink(socketPath)
  }
}
