import Foundation
import OSLog

enum NativeBrowserImportDebugLog {
  private static let maxLogFileBytes: UInt64 = 25 * 1024 * 1024
  private static let maxRotatedLogFiles = 3
  private static let logger = Logger(
    subsystem: "com.madda.ghostex.host", category: "native-browser-import-debug")
  private static let logDateFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS ZZZZ"
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = .current
    return formatter
  }()
  private static var didCreateLogsDirectory = false

  /**
   CDXC:BrowserImportDiagnostics 2026-06-13-06:59:
   Chromium cookie import failures must be diagnosable after a user repro without persisting cookie names, domains, values, profile names, URLs, or filesystem paths. Write aggregate SQLite, keychain, decrypt, and CEF handoff diagnostics into a dedicated support-bundle log; keep routine entries behind Settings Debugging Mode while warning/error/failure-like import events persist in normal mode.
   */
  static func append(event: String, details: [String: Any] = [:]) {
    guard isNativePersistentLogImportantDiagnostic(event) || NativeDebugLogging.isEnabled else {
      return
    }
    let logsDirectory = GhostexAppStorage.logsDirectory
    let logURL = logsDirectory.appendingPathComponent("native-browser-import-debug.log")

    var payload = details
    payload["event"] = event
    let line = "[\(logDateFormatter.string(from: Date()))] \(serialize(NativeLogPrivacy.sanitizePayload(payload)))\n"

    do {
      if !didCreateLogsDirectory {
        try FileManager.default.createDirectory(at: logsDirectory, withIntermediateDirectories: true)
        didCreateLogsDirectory = true
      }
      try rotateLogIfNeeded(logURL: logURL, incomingByteCount: UInt64(line.lengthOfBytes(using: .utf8)))
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
      logger.warning("failed to write native browser import debug log: \(sanitizedError)")
    }
  }

  private static func serialize(_ payload: [String: Any]) -> String {
    guard JSONSerialization.isValidJSONObject(payload),
      let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]),
      let json = String(data: data, encoding: .utf8)
    else {
      return "{\"event\":\"serializationFailed\"}"
    }
    return json
  }

  private static func rotateLogIfNeeded(logURL: URL, incomingByteCount: UInt64) throws {
    let manager = FileManager.default
    let size = (try? manager.attributesOfItem(atPath: logURL.path)[.size] as? NSNumber)?.uint64Value ?? 0
    guard size + incomingByteCount > maxLogFileBytes else {
      return
    }
    let oldest = rotatedLogURL(logURL, index: maxRotatedLogFiles)
    if manager.fileExists(atPath: oldest.path) {
      try manager.removeItem(at: oldest)
    }
    for index in stride(from: maxRotatedLogFiles - 1, through: 1, by: -1) {
      let source = rotatedLogURL(logURL, index: index)
      let destination = rotatedLogURL(logURL, index: index + 1)
      if manager.fileExists(atPath: source.path) {
        try manager.moveItem(at: source, to: destination)
      }
    }
    let firstRotation = rotatedLogURL(logURL, index: 1)
    if manager.fileExists(atPath: firstRotation.path) {
      try manager.removeItem(at: firstRotation)
    }
    try manager.moveItem(at: logURL, to: firstRotation)
  }

  private static func rotatedLogURL(_ logURL: URL, index: Int) -> URL {
    logURL.deletingLastPathComponent().appendingPathComponent("\(logURL.lastPathComponent).\(index)")
  }
}
