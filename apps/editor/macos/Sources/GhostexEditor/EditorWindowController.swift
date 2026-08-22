import AppKit
import Foundation
import WebKit

final class WeakEditorScriptMessageHandler: NSObject, WKScriptMessageHandler {
  weak var target: EditorWindowController?

  init(target: EditorWindowController) {
    self.target = target
    super.init()
  }

  func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
    target?.userContentController(userContentController, didReceive: message)
  }
}

final class EditorWindowController: NSObject, NSWindowDelegate, WKScriptMessageHandler {
  weak var daemon: EditorDaemon?
  weak var session: EditorSession?

  private let webRoot: URL
  private let indexURL: URL
  private(set) var isReady = false
  private(set) var hasPresented = false
  private var readyCallbacks: [() -> Void] = []

  let window: NSWindow
  let webView: WKWebView

  init(daemon: EditorDaemon, webRoot: URL, indexURL: URL) throws {
    self.daemon = daemon
    self.webRoot = webRoot
    self.indexURL = indexURL
    let contentController = WKUserContentController()

    let script = WKUserScript(
      source: """
      Object.defineProperty(window, "__require", {
        configurable: true,
        get: function() { return window.require; }
      });
      """,
      injectionTime: .atDocumentStart,
      forMainFrameOnly: true
    )
    contentController.addUserScript(script)

    let webConfiguration = WKWebViewConfiguration()
    webConfiguration.userContentController = contentController
    webConfiguration.suppressesIncrementalRendering = false

    self.webView = WKWebView(frame: .zero, configuration: webConfiguration)
    webView.autoresizingMask = [.width, .height]

    self.window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 900, height: 620),
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.minSize = NSSize(width: 480, height: 320)
    /*
     * The titlebar names the app; each native window tab names the terminal
     * session it edits (applySessionTitle), mirroring the sidebar row title.
     */
    window.title = "Ghostex Prompt Editor"
    window.contentView = webView

    super.init()

    contentController.add(WeakEditorScriptMessageHandler(target: self), name: "ghostexEditorHost")
    window.delegate = self
    webView.loadFileURL(indexURL, allowingReadAccessTo: webRoot)
  }

  func configure(with session: EditorSession) {
    self.session = session
    session.editorWindow = self
    applySessionTitle(session.title)

    let sendConfiguration = { [weak self, weak session] in
      guard let self, let session else {
        return
      }
      var detail: [String: Any] = [
        "type": "configure",
        "initialText": session.initialText,
        "language": session.language as Any,
        "filePath": session.fileURL.path,
        "title": session.title,
      ]
      if let cursorOffset = session.initialCursorOffset {
        detail["cursorOffset"] = cursorOffset
      }
      self.dispatchHostMessage(detail)
    }

    if isReady {
      sendConfiguration()
    } else {
      readyCallbacks.append(sendConfiguration)
    }
  }

  func applySessionTitle(_ title: String) {
    window.tab.title = title
  }

  func present() {
    hasPresented = true
    daemon?.applySavedFrameOrCascade(window)
    window.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    webView.window?.makeFirstResponder(webView)
  }

  func requestWebSaveAndClose() {
    let javascript = """
    document.dispatchEvent(new KeyboardEvent("keydown", {
      key: "s",
      metaKey: true,
      bubbles: true,
      cancelable: true
    }));
    """
    webView.evaluateJavaScript(javascript)
  }

  func dispatchHostMessage(_ detail: [String: Any]) {
    guard JSONSerialization.isValidJSONObject(detail),
      let data = try? JSONSerialization.data(withJSONObject: detail),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    let javascript = """
    window.dispatchEvent(new CustomEvent("ghostex-editor-host-message", { detail: \(json) }));
    """
    webView.evaluateJavaScript(javascript)
  }

  func cleanup() {
    /*
     * Gate on hasPresented, not session: by the time cleanup runs the daemon
     * has already dropped its strong session reference, so the weak session
     * is nil and a session check would silently skip persisting the frame.
     * Warm windows were never presented and must not clobber the saved frame.
     */
    if hasPresented {
      daemon?.saveWindowFrame(window)
    }
    webView.configuration.userContentController.removeScriptMessageHandler(forName: "ghostexEditorHost")
    webView.stopLoading()
    window.delegate = nil
    window.contentView = nil
    session = nil
    window.close()
  }

  /*
   * Persist the frame as the user moves or resizes the window, not only at
   * cleanup: daemon shutdown (SIGTERM, terminal exit) calls exit() before the
   * async window cleanup runs, so a cleanup-only save would lose the frame.
   */
  func windowDidMove(_ notification: Notification) {
    saveFrameIfPresented()
  }

  func windowDidEndLiveResize(_ notification: Notification) {
    saveFrameIfPresented()
  }

  private func saveFrameIfPresented() {
    guard hasPresented else {
      return
    }
    daemon?.saveWindowFrame(window)
  }

  func windowShouldClose(_ sender: NSWindow) -> Bool {
    if let session {
      session.requestSaveAndClose()
    } else {
      sender.orderOut(nil)
    }
    return false
  }

  func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
    guard message.name == "ghostexEditorHost",
      let body = message.body as? [String: Any],
      let type = body["type"] as? String
    else {
      return
    }

    switch type {
    case "ready":
      isReady = true
      let callbacks = readyCallbacks
      readyCallbacks.removeAll()
      callbacks.forEach { $0() }
      daemon?.warmWindowDidBecomeReady(self)
    case "configured":
      session?.editorConfigured()
    case "draftUpdate":
      if let text = body["text"] as? String {
        session?.latestDraft = text
      }
      if let cursorOffset = cursorOffsetValue(body["cursorOffset"]) {
        session?.latestCursorOffset = cursorOffset
      }
    case "cursorUpdate":
      if let cursorOffset = cursorOffsetValue(body["cursorOffset"]) {
        session?.latestCursorOffset = cursorOffset
      }
    case "saveAndClose":
      if let text = body["text"] as? String {
        session?.latestDraft = text
      }
      if let cursorOffset = cursorOffsetValue(body["cursorOffset"]) {
        session?.latestCursorOffset = cursorOffset
      }
      session?.finish(action: .save)
    case "save":
      if let text = body["text"] as? String {
        session?.latestDraft = text
      }
      if let cursorOffset = cursorOffsetValue(body["cursorOffset"]) {
        session?.latestCursorOffset = cursorOffset
      }
      session?.saveDraftWithoutClosing()
    case "cancel":
      if let text = body["text"] as? String {
        session?.latestDraft = text
      }
      if let cursorOffset = cursorOffsetValue(body["cursorOffset"]) {
        session?.latestCursorOffset = cursorOffset
      }
      session?.finish(action: .cancel)
    case "pasteImage":
      session?.handlePasteImage(body)
    case "loadImagePreview":
      session?.handleLoadImagePreview(body)
    default:
      break
    }
  }

  private func cursorOffsetValue(_ value: Any?) -> Int? {
    guard let number = value as? NSNumber else {
      return nil
    }
    let offset = number.intValue
    return offset >= 0 ? offset : nil
  }
}
