import AppKit
import Foundation
import UniformTypeIdentifiers

enum EditorCloseAction {
  case save
  case cancel
}

struct EditorCursorSnapshot {
  let text: String
  let cursorOffset: Int
}

final class EditorSession {
  weak var daemon: EditorDaemon?
  weak var openerConnection: ClientConnection?
  var editorWindow: EditorWindowController?

  let requestId: String
  let fileURL: URL
  let statusFileURL: URL
  let language: String?
  let originatingSessionId: String?
  private(set) var title: String
  let initialText: String
  let initialCursorOffset: Int?
  var latestDraft: String
  var latestCursorOffset: Int?

  private var hasOpened = false
  private var isFinishing = false
  private var returnFocusApplication: NSRunningApplication?

  init(
    daemon: EditorDaemon,
    requestId: String,
    fileURL: URL,
    statusFileURL: URL,
    language: String?,
    originatingSessionId: String?,
    title: String,
    initialText: String,
    initialCursorOffset: Int?,
    openerConnection: ClientConnection
  ) {
    self.daemon = daemon
    self.requestId = requestId
    self.fileURL = fileURL
    self.statusFileURL = statusFileURL
    self.language = language
    self.originatingSessionId = originatingSessionId
    self.title = title
    self.initialText = initialText
    self.initialCursorOffset = initialCursorOffset
    self.latestDraft = initialText
    self.latestCursorOffset = initialCursorOffset
    self.openerConnection = openerConnection
  }

  func presentEditorWindow() {
    /*
     * Presentation happens right at open handling, before the configure
     * round-trip through the web layer completes: a warm window already has
     * Monaco loaded, so waiting for the "configured" reply only delays window
     * visibility. Capture the frontmost app first — present() activates this
     * daemon, and focus must return to the terminal that pressed Ctrl+G.
     */
    returnFocusApplication = daemon?.captureReturnFocusApplication()
    editorWindow?.present()
  }

  func retitle(_ newTitle: String) {
    title = newTitle
    editorWindow?.applySessionTitle(newTitle)
  }

  func editorConfigured() {
    guard !hasOpened else {
      return
    }
    hasOpened = true
    writeStatus("started")
    openerConnection?.send(["type": "opened", "requestId": requestId])
  }

  func requestSaveAndClose() {
    guard !isFinishing else {
      return
    }
    if let editorWindow, editorWindow.isReady {
      editorWindow.requestWebSaveAndClose()
    } else {
      /*
       * The window can be closed before Monaco ever loaded (cold-start open
       * that was presented immediately). The web layer cannot answer the
       * save keystroke yet, and latestDraft still holds the initial text, so
       * finishing directly is lossless.
       */
      finish(action: .save)
    }
  }

  func saveDraftWithoutClosing() {
    do {
      try writeDraft(latestDraft)
    } catch {
      writeStderr("GhostexEditor: save failed: \(error.localizedDescription)\n")
    }
  }

  func finish(action: EditorCloseAction) {
    guard !isFinishing else {
      return
    }
    isFinishing = true

    switch action {
    case .save:
      do {
        try writeDraft(latestDraft)
      } catch {
        writeStderr(
          "GhostexEditor: save failed for \(fileURL.path): \(error.localizedDescription)\n")
      }
      writeStatus("saved")
      openerConnection?.send([
        "type": "closed",
        "requestId": requestId,
        "status": "saved",
      ])
    case .cancel:
      writeStatus("cancelled")
      openerConnection?.send([
        "type": "closed",
        "requestId": requestId,
        "status": "cancelled",
      ])
    }

    restoreReturnFocus()
    daemon?.sessionDidFinish(self)
  }

  private func restoreReturnFocus() {
    /*
     * Only hand focus back when this daemon is still the active app: a close
     * driven remotely (CLI signal, shutdown) while the user works in another
     * app must not yank them to the terminal.
     */
    guard NSApp.isActive else {
      return
    }
    guard let application = returnFocusApplication, !application.isTerminated else {
      return
    }
    if #available(macOS 14.0, *) {
      application.activate()
    } else {
      application.activate(options: [])
    }
  }

  func handleLoadImagePreview(_ body: [String: Any]) {
    guard let previewRequestId = body["requestId"] as? String,
      let path = body["path"] as? String
    else {
      return
    }

    /*
     * The thumbnail shelf must load every image path already present in the
     * Monaco text. Resolve short home-relative image paths, including legacy
     * ~/.ghostex/i references, natively and send
     * display-safe data URLs back to the web layer so WKWebView local-file
     * read limits do not block thumbnail or popup rendering. Decode and
     * downsample off the main queue so large images cannot stall typing.
     */
    let editorWindow = self.editorWindow
    DispatchQueue.global(qos: .userInitiated).async {
      var response: [String: Any] = [
        "type": "imagePreviewResult",
        "requestId": previewRequestId,
        "path": path,
      ]
      do {
        response["dataUrl"] = try Self.imagePreviewDataURL(path: path)
      } catch {
        response["error"] = error.localizedDescription
      }
      DispatchQueue.main.async {
        editorWindow?.dispatchHostMessage(response)
      }
    }
  }

  private static func imagePreviewDataURL(path: String) throws -> String {
    guard let fileURL = imagePreviewFileURL(path: path),
      FileManager.default.fileExists(atPath: fileURL.path),
      isImageFileURL(fileURL)
    else {
      throw ghostexError("Image preview path does not point to a local image.")
    }

    let data = try Data(contentsOf: fileURL)
    if fileURL.pathExtension.lowercased() == "svg" {
      return "data:image/svg+xml;base64,\(data.base64EncodedString())"
    }
    guard let image = NSImage(data: data),
      let pngData = previewPNGData(from: image)
    else {
      throw ghostexError("Image preview data could not be decoded.")
    }
    return "data:image/png;base64,\(pngData.base64EncodedString())"
  }

  private static func imagePreviewFileURL(path: String) -> URL? {
    let trimmedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmedPath.hasPrefix("file://"), let url = URL(string: trimmedPath), url.isFileURL {
      return url
    }
    if trimmedPath.hasPrefix("~/.ghostex/") {
      let relativePath = String(trimmedPath.dropFirst("~/.ghostex/".count))
      return ghostexDataDirectory().appendingPathComponent(relativePath)
    }
    let legacyAbsolutePrefix =
      FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".ghostex", isDirectory: true).path + "/"
    if trimmedPath.hasPrefix(legacyAbsolutePrefix) {
      let relativePath = String(trimmedPath.dropFirst(legacyAbsolutePrefix.count))
      return ghostexDataDirectory().appendingPathComponent(relativePath)
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

  private static func previewPNGData(from image: NSImage) -> Data? {
    /*
     * Thumbnails and the popup share one data URL, so cap the longest edge at
     * 1600px: large screenshots stay crisp in the popup while the base64
     * payload crossing into the webview stays bounded.
     */
    let sourceSize =
      image.size.width > 0 && image.size.height > 0 ? image.size : NSSize(width: 1, height: 1)
    let maximumDimension = CGFloat(1600)
    let scale = min(1, maximumDimension / max(sourceSize.width, sourceSize.height))
    let drawSize = NSSize(
      width: max(1, sourceSize.width * scale), height: max(1, sourceSize.height * scale))
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

  func handlePasteImage(_ body: [String: Any]) {
    guard let pasteRequestId = body["requestId"] as? String else {
      return
    }

    do {
      let imagePath = try Self.resolveClipboardImagePath()
      editorWindow?.dispatchHostMessage([
        "type": "imagePasteResult",
        "requestId": pasteRequestId,
        "path": imagePath,
      ])
    } catch {
      editorWindow?.dispatchHostMessage([
        "type": "imagePasteResult",
        "requestId": pasteRequestId,
        "error": error.localizedDescription,
      ])
    }
  }

  private func writeDraft(_ draft: String) throws {
    guard let data = draft.data(using: .utf8) else {
      throw ghostexError("Draft is not valid UTF-8.")
    }
    try data.write(to: fileURL, options: .atomic)
  }

  private func writeStatus(_ status: String) {
    do {
      try status.data(using: .utf8)?.write(to: statusFileURL, options: .atomic)
    } catch {
      writeStderr("GhostexEditor: status write failed: \(error.localizedDescription)\n")
    }
  }

  /*
   * Pasting an image must insert a durable Markdown file reference, never
   * binary content, and the inserted path must stay short enough to read on
   * one prompt-editor line. Native owns path resolution: clipboard image
   * files are copied and unsaved bitmaps saved under the resolved Ghostex data
   * directory with a compact timestamp filename, then a home-relative or
   * absolute path is returned to the web layer for [Image #N](path) insertion.
   */
  private static func resolveClipboardImagePath() throws -> String {
    let pasteboard = NSPasteboard.general
    if let imageFileURL = firstClipboardImageFileURL(in: pasteboard) {
      let copiedURL = try copyClipboardImageFile(imageFileURL)
      return displayImagePath(for: copiedURL)
    }

    guard let pngData = clipboardPNGData(in: pasteboard) else {
      throw ghostexError("Clipboard does not contain an image.")
    }

    let fileURL = try uniqueImageURL(pathExtension: "png")
    try pngData.write(to: fileURL, options: .atomic)
    return displayImagePath(for: fileURL)
  }

  private static func firstClipboardImageFileURL(in pasteboard: NSPasteboard) -> URL? {
    let fileURLType = NSPasteboard.PasteboardType("public.file-url")
    for item in pasteboard.pasteboardItems ?? [] {
      guard let fileURLString = item.string(forType: fileURLType),
        let fileURL = URL(string: fileURLString),
        fileURL.isFileURL,
        FileManager.default.fileExists(atPath: fileURL.path),
        isImageFileURL(fileURL)
      else {
        continue
      }
      return fileURL
    }

    let filenamesType = NSPasteboard.PasteboardType("NSFilenamesPboardType")
    guard let filenames = pasteboard.propertyList(forType: filenamesType) as? [String] else {
      return nil
    }
    return
      filenames
      .map { URL(fileURLWithPath: $0) }
      .first { fileURL in
        FileManager.default.fileExists(atPath: fileURL.path) && isImageFileURL(fileURL)
      }
  }

  private static func isImageFileURL(_ url: URL) -> Bool {
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

  private static func clipboardPNGData(in pasteboard: NSPasteboard) -> Data? {
    let pngType = NSPasteboard.PasteboardType("public.png")
    if let pngData = pasteboard.data(forType: pngType), NSImage(data: pngData) != nil {
      return pngData
    }

    let tiffType = NSPasteboard.PasteboardType("public.tiff")
    if let tiffData = pasteboard.data(forType: tiffType),
      let image = NSImage(data: tiffData)
    {
      return pngData(from: image)
    }

    guard let image = NSImage(pasteboard: pasteboard) else {
      return nil
    }
    return pngData(from: image)
  }

  private static func pngData(from image: NSImage) -> Data? {
    guard let tiffData = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiffData)
    else {
      return nil
    }
    return bitmap.representation(using: .png, properties: [:])
  }

  private static func copyClipboardImageFile(_ sourceURL: URL) throws -> URL {
    let fileURL = try uniqueImageURL(
      pathExtension: normalizedImageFileExtension(sourceURL.pathExtension))
    try FileManager.default.copyItem(at: sourceURL, to: fileURL)
    return fileURL
  }

  private static func uniqueImageURL(pathExtension: String) throws -> URL {
    let directory = imageStoreDirectory()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyMMddHHmmss"
    let baseName = formatter.string(from: Date())
    let normalizedExtension = normalizedImageFileExtension(pathExtension)
    let firstURL = directory.appendingPathComponent(
      "\(baseName).\(normalizedExtension)", isDirectory: false)
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

  private static func imageStoreDirectory() -> URL {
    ghostexDataDirectory().appendingPathComponent("i", isDirectory: true)
  }

  private static func ghostexDataDirectory() -> URL {
    let environment = ProcessInfo.processInfo.environment
    if let ghostexHome = absoluteEnvironmentDirectory("GHOSTEX_HOME", environment: environment) {
      return ghostexHome
    }
    let dataRoot =
      absoluteEnvironmentDirectory("XDG_DATA_HOME", environment: environment)
      ?? FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".local/share", isDirectory: true)
    return dataRoot.appendingPathComponent("ghostex", isDirectory: true)
  }

  private static func absoluteEnvironmentDirectory(
    _ name: String,
    environment: [String: String]
  ) -> URL? {
    guard let value = environment[name]?.trimmingCharacters(in: .whitespacesAndNewlines),
      !value.isEmpty,
      (value as NSString).isAbsolutePath
    else {
      return nil
    }
    return URL(fileURLWithPath: value, isDirectory: true).standardizedFileURL
  }

  private static func normalizedImageFileExtension(_ pathExtension: String) -> String {
    let normalizedExtension = pathExtension.trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased()
    if normalizedExtension == "jpeg" {
      return "jpg"
    }
    if normalizedExtension == "tiff" {
      return "tif"
    }
    return normalizedExtension.isEmpty ? "png" : normalizedExtension
  }

  private static func displayImagePath(for fileURL: URL) -> String {
    let path = fileURL.standardizedFileURL.path
    let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
    if path == home {
      return "~"
    }
    let homePrefix = home.hasSuffix("/") ? home : "\(home)/"
    if path.hasPrefix(homePrefix) {
      return "~/\(path.dropFirst(homePrefix.count))"
    }
    return path
  }
}
