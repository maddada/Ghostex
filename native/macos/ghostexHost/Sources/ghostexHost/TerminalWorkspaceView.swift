import AppKit
import Combine
import Darwin
import GhosttyKit
import QuartzCore
import WebKit

private func nativePaneImage(fromDataUrl dataUrl: String?, isTemplate: Bool = false) -> NSImage? {
  guard let dataUrl,
    let commaIndex = dataUrl.firstIndex(of: ",")
  else {
    return nil
  }
  let metadata = dataUrl[..<commaIndex]
  let payload = String(dataUrl[dataUrl.index(after: commaIndex)...])
  let data: Data?
  if metadata.contains(";base64") {
    data = Data(base64Encoded: payload)
  } else {
    data = payload.removingPercentEncoding?.data(using: .utf8)
  }
  guard let data else {
    return nil
  }
  guard let image = NSImage(data: data) else {
    return nil
  }
  image.isTemplate = isTemplate
  return image
}

private func nativePaneColor(fromHex hex: String?) -> NSColor? {
  guard let hex else {
    return nil
  }
  let value = hex.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
  guard value.count == 6, let rgb = UInt32(value, radix: 16) else {
    return nil
  }
  return NSColor(
    calibratedRed: CGFloat((rgb >> 16) & 0xff) / 255,
    green: CGFloat((rgb >> 8) & 0xff) / 255,
    blue: CGFloat(rgb & 0xff) / 255,
    alpha: 1
  )
}

private let nativeTerminalColorEnvironmentKeys = [
  "ANSI_COLORS_DISABLED",
  "CI",
  "CLICOLOR",
  "CLICOLOR_FORCE",
  "COLORTERM",
  "FORCE_COLOR",
  "NO_COLOR",
  "NODE_DISABLE_COLORS",
  "TERM",
  "TERM_PROGRAM",
  "TERM_PROGRAM_VERSION",
]

private let projectBeadsResponseEventName = "ghostex-project-beads-response"
private let projectBoardResponseEventName = "ghostex-project-board-response"

private struct ProjectBeadsBridgeRequest: Decodable {
  let action: String
  let comment: String?
  let cwd: String
  let dependsOnId: String?
  let depType: String?
  let description: String?
  let estimate: Int?
  let issueId: String?
  let label: String?
  let labels: [String]?
  let priority: String?
  let prompt: String?
  let query: String?
  let requestId: String
  let status: String?
  let title: String?
  let value: String?
}

private struct ProjectBeadsBridgeResponse: Encodable {
  let error: String?
  let exitCode: Int32
  let requestId: String
  let stderr: String
  let stdout: String
}

private enum ProjectBeadsBridgeError: Error, LocalizedError {
  case invalidRequest(String)

  var errorDescription: String? {
    switch self {
    case .invalidRequest(let message):
      return message
    }
  }
}

private func projectBoardNativeProcessEnvironment() -> [String: String] {
  /**
   CDXC:ProjectBoard 2026-05-23-03:00:
   Project board commands execute the upstream bd binary from a WebKit project pane.
   GUI launches often miss Homebrew, mise, asdf, and ~/.local paths, so normalize PATH here instead of making the React board know CLI installation details.
   */
  var environment = ProcessInfo.processInfo.environment
  environment["PATH"] = projectBoardNativeProcessPath(environment["PATH"])
  return environment
}

private func projectBoardNativeProcessPath(_ path: String?) -> String {
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
  let existingEntries = (path ?? "").split(separator: ":").map(String.init)
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

private func projectEditorModeFromNativeEditorId(_ nativeEditorId: String) -> String? {
  guard nativeEditorId.hasPrefix("project-editor:"),
    let mode = nativeEditorId.split(separator: ":").last.map(String.init),
    ["code", "git", "tasks"].contains(mode)
  else {
    return nil
  }
  return mode
}

private let nativeGhosttyTerminalColorDisablingEnvironmentKeys = [
  "ANSI_COLORS_DISABLED",
  "NO_COLOR",
  "NODE_DISABLE_COLORS",
]

private let nativeSshConnectionEnvironmentKeys = [
  "SSH_CONNECTION",
  "SSH_CLIENT",
  "SSH_TTY",
]

private func nativePromptEditorCommand(backend: String, customCommand: String? = nil) -> String {
  if backend == "gte" {
    return "gte"
  }
  if backend == "custom" {
    let trimmed = customCommand?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return trimmed.isEmpty ? "code --wait" : trimmed
  }
  if let executablePath = Bundle.main.executableURL?.path,
    !executablePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  {
    return "\(nativeShellQuote(executablePath)) floating-monaco-editor"
  }
  return "ghostex floating-monaco-editor"
}

private func nativePromptEditorBackend(from environment: [String: String]) -> String? {
  let backend = environment["GHOSTEX_PROMPT_EDITOR_BACKEND"]?.trimmingCharacters(
    in: .whitespacesAndNewlines)
  if backend == "monaco" || backend == "gte" || backend == "custom" {
    return backend
  }
  if environment["GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE"] == "1" {
    return "gte"
  }
  if environment["GHOSTEX_PROMPT_EDITING_ENABLED"] == "1" {
    return "monaco"
  }
  return nil
}

private func nativeEffectivePromptEditorBackend(from environment: [String: String]) -> String? {
  guard let backend = nativePromptEditorBackend(from: environment) else {
    return nil
  }
  if backend == "monaco" && nativeIsSshConnectionEnvironment(environment) {
    return "gte"
  }
  return backend
}

private func nativeHasNonEmptyEnvironmentValue(
  _ key: String,
  in environment: [String: String]
) -> Bool {
  if let value = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
    !value.isEmpty
  {
    return true
  }
  if let value = nativeProcessEnvironmentValue(key)?.trimmingCharacters(in: .whitespacesAndNewlines),
    !value.isEmpty
  {
    return true
  }
  return false
}

private func nativeIsSshConnectionEnvironment(_ environment: [String: String]) -> Bool {
  nativeSshConnectionEnvironmentKeys.contains { key in
    nativeHasNonEmptyEnvironmentValue(key, in: environment)
  }
}

private func nativeIsGhostexPromptEditorValue(
  _ value: String?,
  backend: String? = nil,
  customCommand: String? = nil
) -> Bool {
  guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines),
    !trimmed.isEmpty
  else {
    return false
  }
  if trimmed.contains("floating-monaco-editor") || trimmed.contains("floating-editor -- gte") {
    return true
  }
  if backend == "custom" {
    let custom = customCommand?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return !custom.isEmpty && trimmed == custom
  }
  return backend == "gte" && trimmed == "gte"
}

private func nativeRemoveStaleSshPromptEditorEnvironment(_ environment: inout [String: String]) -> Bool {
  guard nativeIsSshConnectionEnvironment(environment) else {
    return false
  }
  let promptEditorBackend = nativePromptEditorBackend(from: environment)
  guard promptEditorBackend == nil else {
    return false
  }
  let hasGhostexPromptEditorOverlay =
    nativeIsGhostexPromptEditorValue(environment["EDITOR"])
    || nativeIsGhostexPromptEditorValue(environment["VISUAL"])
  guard hasGhostexPromptEditorOverlay else {
    return false
  }
  let customPromptEditorCommand = environment["GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND"]

  /**
   CDXC:PromptEditorBackend 2026-05-17-08:46:
   SSH-connected shells without an explicit Ghostex prompt-editor backend should
   keep the editor chosen by that SSH login. Remove only stale Ghostex
   prompt-editor overlay and markers so normal EDITOR/VISUAL values continue to
   come from the login environment.
   */
  for key in [
    "GHOSTEX_PROMPT_EDITING_ENABLED",
    "GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE",
    "GHOSTEX_DEBUGGING_MODE",
    "GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND",
    "GHOSTEX_GTE_PROMPT_EDITOR_LOG",
    "ZDOTDIR",
    "GHOSTEX_ORIGINAL_ZDOTDIR",
  ] {
    environment.removeValue(forKey: key)
  }
  for key in ["EDITOR", "VISUAL"] {
    if nativeIsGhostexPromptEditorValue(
      environment[key],
      backend: promptEditorBackend,
      customCommand: customPromptEditorCommand
    ) {
      environment.removeValue(forKey: key)
    }
  }
  return true
}

private func nativeGtePromptEditorCommand() -> String {
  nativePromptEditorCommand(backend: "gte")
}

private func nativeMonacoPromptEditorCommand() -> String {
  nativePromptEditorCommand(backend: "monaco")
}

private func nativeGtePromptEditorLogURL() -> URL {
  FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent("Library", isDirectory: true)
    .appendingPathComponent("Logs", isDirectory: true)
    .appendingPathComponent("ghostex", isDirectory: true)
    .appendingPathComponent("gte-prompt-editor.log")
}

private func nativeLogGtePromptEditor(_ event: String, details: [String: String] = [:]) {
  /**
   CDXC:Diagnostics 2026-05-16-07:23:
   Gte prompt-editor breadcrumbs are persistent regular diagnostics. Do not
   create or append gte-prompt-editor.log unless Settings Debugging Mode is
   enabled.
   */
  guard NativeDebugLogging.isEnabled else {
    return
  }
  let url = nativeGtePromptEditorLogURL()
  let directory = url.deletingLastPathComponent()
  try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

  var payload = details
  payload["event"] = event
  payload["source"] = "ghostex-native"
  payload["timestamp"] = ISO8601DateFormatter().string(from: Date())

  let json =
    (try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]))
    .flatMap { String(data: $0, encoding: .utf8) }
    ?? "\(payload)"
  guard let data = (json + "\n").data(using: .utf8) else {
    return
  }

  if FileManager.default.fileExists(atPath: url.path),
    let handle = try? FileHandle(forWritingTo: url)
  {
    defer { handle.closeFile() }
    _ = try? handle.seekToEnd()
    _ = try? handle.write(contentsOf: data)
  } else {
    try? data.write(to: url, options: .atomic)
  }
}

private enum TerminalPaneRoundedBottomCorner {
  case left
  case none
  case right
}

private func nativeGhosttyTerminalEnvironment(
  _ environment: [String: String]?,
  sessionId: String? = nil
) -> [String: String] {
  /**
   CDXC:GhosttyTerminalColorEnv 2026-05-04-22:46
   Embedded Ghostty terminals are interactive color-capable PTYs. Agent-managed
   launch environments can carry NO_COLOR into ghostex; strip color-disabling keys
   at the native Ghostty boundary and set non-forcing color opt-in without
   forcing ANSI output in non-Ghostty child processes.
   */
  var result = environment ?? [:]
  for key in nativeGhosttyTerminalColorDisablingEnvironmentKeys {
    result.removeValue(forKey: key)
  }
  result["CLICOLOR"] = "1"
  if let sessionId, !sessionId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
    result["GHOSTEX_NATIVE_SESSION_ID"] = sessionId
  }
  nativeApplyGtePromptEditingEnvironment(&result)
  return result
}

private func nativeGhosttyFloatingEditorEnvironment(
  _ environment: [String: String]?
) -> [String: String] {
  var result = nativeGhosttyTerminalEffectiveProcessEnvironment()
  for (key, value) in environment ?? [:] {
    result[key] = value
  }
  for key in [
    "EDITOR",
    "VISUAL",
    "GHOSTEX_NATIVE_SESSION_ID",
    "ZDOTDIR",
    "GHOSTEX_ORIGINAL_ZDOTDIR",
    "GHOSTEX_PROMPT_EDITOR_BACKEND",
    "GHOSTEX_PROMPT_EDITING_ENABLED",
    "GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE",
    "GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND",
    "GHOSTEX_GTE_PROMPT_EDITOR_LOG",
  ] {
    result.removeValue(forKey: key)
  }
  result["CLICOLOR"] = "1"
  result["GHOSTEX_FLOATING_EDITOR"] = "1"
  return result
}

private func nativeApplyGtePromptEditingEnvironment(_ environment: inout [String: String]) {
  if nativeRemoveStaleSshPromptEditorEnvironment(&environment) {
    return
  }
  guard let promptEditorBackend = nativeEffectivePromptEditorBackend(from: environment) else {
    return
  }

  /**
   CDXC:GtePromptEditing 2026-05-10-11:27
   Zsh startup files can export EDITOR after Ghostty receives the process
   environment. When gte is enabled, launch zsh through a ghostex-owned ZDOTDIR
   shim that sources the user's real startup files first, then exports the
   gte editor command last so Ctrl+G/edit-command-line uses gte instead
   of the profile editor.
   CDXC:PromptEditorBackend 2026-05-22-09:56
   The terminal prompt editor is named gte for Ghostex Terminal Editor. Native launch shims must export the gte command and keep logs/state under gte names so Ctrl+G behavior, diagnostics, and Settings copy use one name.
   CDXC:PromptEditorBackend 2026-05-22-10:16
   The gte backend is an in-terminal editor, not a native overlay. Export plain `gte` for EDITOR/VISUAL so Ctrl+G opens inside the launching terminal; keep the Ghostex floating command only for the Monaco backend.
   CDXC:PromptEditorBackend 2026-05-25-11:23
   When Settings selects Monaco, SSH-connected terminal sessions cannot use the local floating overlay. Resolve only that runtime case to gte while preserving the saved Monaco preference for app-local rich prompt editing. Explicit gte selections must stay gte in every terminal context, including SSH.
   CDXC:GtePromptEditing 2026-05-11-17:31
   Dev terminals can be launched from inside the production ghostex shim. Unwrap
   inherited ghostex ZDOTDIR values to the original user dotdir so zsh sources the
   user's real prompt, aliases, and startup files instead of recursively
   sourcing another ghostex shim.
  */
  let promptEditor = nativePromptEditorCommand(
    backend: promptEditorBackend,
    customCommand: environment["GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND"]
  )
  environment["EDITOR"] = promptEditor
  environment["VISUAL"] = promptEditor
  environment["GHOSTEX_DEBUGGING_MODE"] = NativeDebugLogging.isEnabled ? "1" : "0"
  environment["GHOSTEX_GTE_PROMPT_EDITOR_LOG"] = nativeGtePromptEditorLogURL().path
  if let appVariant = ProcessInfo.processInfo.environment["GHOSTEX_APP_VARIANT"], !appVariant.isEmpty {
    environment["GHOSTEX_APP_VARIANT"] = appVariant
  }
  let originalZdotdir = nativeOriginalZdotdir(for: environment)
  guard let shimZdotdir = nativeEnsureGteZdotdirShim(promptEditorCommand: promptEditor) else {
    return
  }
  environment["GHOSTEX_ORIGINAL_ZDOTDIR"] = originalZdotdir
  environment["ZDOTDIR"] = shimZdotdir
  nativeLogGtePromptEditor("environment.applied", details: [
    "editor": promptEditor,
    "promptEditorBackend": promptEditorBackend,
    "logPath": environment["GHOSTEX_GTE_PROMPT_EDITOR_LOG"] ?? "",
    "originalZdotdir": originalZdotdir,
    "shimZdotdir": shimZdotdir,
    "visual": promptEditor,
  ])
}

private func nativeOriginalZdotdir(for environment: [String: String]) -> String {
  for value in [
    environment["GHOSTEX_ORIGINAL_ZDOTDIR"],
    ProcessInfo.processInfo.environment["GHOSTEX_ORIGINAL_ZDOTDIR"],
    environment["ZDOTDIR"],
    ProcessInfo.processInfo.environment["ZDOTDIR"],
  ] {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
      continue
    }
    if !nativeIsGteShimZdotdir(value) {
      return value
    }
  }
  return NSHomeDirectory()
}

private func nativeIsGteShimZdotdir(_ value: String) -> Bool {
  URL(fileURLWithPath: value).lastPathComponent == "gte-zdotdir"
}

private func nativeEnsureGteZdotdirShim(promptEditorCommand: String) -> String? {
  let directory = GhostexAppStorage.sharedStateDirectory.appendingPathComponent(
    "gte-zdotdir",
    isDirectory: true
  )
  do {
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    for startupFile in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
      let shouldExportGte = startupFile != ".zshenv"
      let contents = nativeGteZshStartupShim(
        fileName: startupFile,
        exportGte: shouldExportGte,
        promptEditorCommand: promptEditorCommand
      )
      try contents.write(
        to: directory.appendingPathComponent(startupFile),
        atomically: true,
        encoding: .utf8
      )
    }
    return directory.path
  } catch {
    NSLog("Failed to prepare gte zsh startup shim: \(error.localizedDescription)")
    nativeLogGtePromptEditor("shim.prepare_failed", details: [
      "error": error.localizedDescription
    ])
    return nil
  }
}

private func nativeGteZshStartupShim(
  fileName: String,
  exportGte: Bool,
  promptEditorCommand: String
) -> String {
  let originalZdotdirUpdateBlock =
    fileName == ".zshenv"
    ? """

      if [ -n "${ZDOTDIR}" ] && [ "${ZDOTDIR}" != "${_ghostex_shim_zdotdir}" ]; then
        export GHOSTEX_ORIGINAL_ZDOTDIR="${ZDOTDIR}"
      fi
      ZDOTDIR="${_ghostex_shim_zdotdir}"
      """
    : ""
  let exportBlock =
    exportGte
    ? """

      # CDXC:PromptEditorBackend 2026-05-17-08:46: SSH logins without an explicit Ghostex prompt-editor backend keep their existing EDITOR/VISUAL instead of being rewritten to a stale local floating editor command.
      # CDXC:PromptEditorBackend 2026-05-25-11:23: SSH sessions use the already-resolved terminal-native prompt editor command, so explicit gte and Monaco-over-SSH both export gte instead of leaving Ctrl+G pointed at an unavailable floating overlay.
      export EDITOR=\(nativeShellQuote(promptEditorCommand))
      export VISUAL=\(nativeShellQuote(promptEditorCommand))
      if [ "${_ghostex_gte_debug}" = "1" ]; then
        {
          printf '[%s] zsh-shim.export file=%s pid=%s editor=%s visual=%s pwd=%s\\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "\(fileName)" "$$" "${EDITOR}" "${VISUAL}" "${PWD}"
        } >> "${_ghostex_gte_log}" 2>/dev/null
      fi
      """
    : ""
  return """
    # CDXC:GtePromptEditing 2026-05-10-11:27
    # Source the user's real zsh startup file, then let ghostex force the gte
    # prompt editor command after profile exports that would otherwise override
    # EDITOR.
    _ghostex_shim_zdotdir="${ZDOTDIR}"
    _ghostex_original_zdotdir="${GHOSTEX_ORIGINAL_ZDOTDIR:-$HOME}"
    _ghostex_gte_debug="${GHOSTEX_DEBUGGING_MODE:-0}"
    if [ "${_ghostex_gte_debug}" = "1" ]; then
      _ghostex_gte_log="${GHOSTEX_GTE_PROMPT_EDITOR_LOG:-$HOME/Library/Logs/ghostex/gte-prompt-editor.log}"
      mkdir -p "${_ghostex_gte_log:h}" 2>/dev/null
      {
        printf '[%s] zsh-shim.enter file=%s pid=%s editor_before=%s visual_before=%s zdotdir=%s original_zdotdir=%s\\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "\(fileName)" "$$" "${EDITOR}" "${VISUAL}" "${ZDOTDIR}" "${_ghostex_original_zdotdir}"
      } >> "${_ghostex_gte_log}" 2>/dev/null
    fi
    if [ -r "${_ghostex_original_zdotdir}/\(fileName)" ]; then
      ZDOTDIR="${_ghostex_original_zdotdir}"
      source "${_ghostex_original_zdotdir}/\(fileName)"
      ZDOTDIR="${_ghostex_shim_zdotdir}"
    fi\(originalZdotdirUpdateBlock)\(exportBlock)
    if [ "${_ghostex_gte_debug}" = "1" ]; then
      {
        printf '[%s] zsh-shim.leave file=%s pid=%s editor_after=%s visual_after=%s zdotdir=%s\\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "\(fileName)" "$$" "${EDITOR}" "${VISUAL}" "${ZDOTDIR}"
      } >> "${_ghostex_gte_log}" 2>/dev/null
      unset _ghostex_gte_log
    fi
    unset _ghostex_gte_debug
    unset _ghostex_shim_zdotdir
    unset _ghostex_original_zdotdir

    """
}

private func nativeShellQuote(_ value: String) -> String {
  "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
}

private func nativeJavaScriptLiteral(_ value: String) -> String {
  guard let data = try? JSONSerialization.data(withJSONObject: [value]),
    let arrayLiteral = String(data: data, encoding: .utf8),
    arrayLiteral.first == "[",
    arrayLiteral.last == "]"
  else {
    return "\"\""
  }
  return String(arrayLiteral.dropFirst().dropLast())
}

private func nativeGhosttyTerminalEffectiveProcessEnvironment() -> [String: String] {
  var environment = ProcessInfo.processInfo.environment
  for key in nativeGhosttyTerminalColorDisablingEnvironmentKeys {
    environment.removeValue(forKey: key)
  }
  environment["CLICOLOR"] = "1"
  return environment
}

private func nativeProcessEnvironmentValue(_ key: String) -> String? {
  guard let value = getenv(key) else {
    return nil
  }
  return String(cString: value)
}

private func withNativeGhosttyTerminalProcessEnvironment<T>(_ body: () -> T) -> T {
  /**
   CDXC:GhosttyTerminalColorEnv 2026-05-04-22:46
   Ghostty embedded surfaces snapshot the host process environment when the
   surface is created. Temporarily sanitize only that creation window so spawned
   shells do not inherit NO_COLOR from the app launch context, then restore the
   app process environment for unrelated native work.
   */
  var savedEnvironment: [String: String?] = [:]
  let keysToSave = nativeGhosttyTerminalColorDisablingEnvironmentKeys + ["CLICOLOR"]
  for key in keysToSave {
    savedEnvironment[key] = nativeProcessEnvironmentValue(key)
  }

  for key in nativeGhosttyTerminalColorDisablingEnvironmentKeys {
    unsetenv(key)
  }
  setenv("CLICOLOR", "1", 1)

  defer {
    for key in keysToSave {
      if let value = savedEnvironment[key] ?? nil {
        setenv(key, value, 1)
      } else {
        unsetenv(key)
      }
    }
  }

  return body()
}

private func nativeTerminalColorEnvironmentSnapshot(_ environment: [String: String]) -> [String: Any] {
  /**
   CDXC:AgentCliColorDiagnostics 2026-05-04-15:39
   Agent CLIs can render without color when their PTY process inherits
   color-disabling environment values. Capture both the app process env and the
   sidebar-provided Ghostty env overlay at surface creation without changing
   launch behavior.
   */
  var snapshot: [String: Any] = [:]
  for key in nativeTerminalColorEnvironmentKeys {
    snapshot[key] = environment[key] ?? NSNull()
  }
  return snapshot
}

final class GhostexGhosttyApp {
  let configPath: String?
  private(set) var config: Ghostty.Config
  private(set) var app: ghostty_app_t?

  init(configPath: String?) {
    self.configPath = configPath
    self.config = Ghostty.Config(at: configPath)
    /**
     CDXC:NativeTerminals 2026-05-11-14:01
     ghostex follows the direct Ghostty embedding model: the host app owns
     ghostty_app_t and runtime callbacks itself instead of routing terminal
     lifecycle through Ghostty.App and Ghostty.SurfaceView wrapper ownership.
     */
    var runtimeConfig = ghostty_runtime_config_s()
    runtimeConfig.userdata = Unmanaged.passUnretained(self).toOpaque()
    runtimeConfig.supports_selection_clipboard = true
    runtimeConfig.wakeup_cb = { userdata in
      guard let userdata else { return }
      let app = Unmanaged<GhostexGhosttyApp>.fromOpaque(userdata).takeUnretainedValue()
      DispatchQueue.main.async {
        app.appTick()
      }
    }
    runtimeConfig.action_cb = { app, target, action in
      GhostexGhosttyApp.handleAction(app: app, target: target, action: action)
    }
    runtimeConfig.read_clipboard_cb = { userdata, location, state in
      GhostexGhosttyApp.readClipboard(userdata: userdata, location: location, state: state)
    }
    runtimeConfig.confirm_read_clipboard_cb = { userdata, content, state, _ in
      GhostexGhosttyApp.confirmReadClipboard(userdata: userdata, content: content, state: state)
    }
    runtimeConfig.write_clipboard_cb = { _, location, content, len, _ in
      GhostexGhosttyApp.writeClipboard(location: location, content: content, len: UInt(len))
    }
    runtimeConfig.close_surface_cb = { userdata, _ in
      GhostexGhosttyApp.closeSurface(userdata: userdata)
    }

    if let rawConfig = config.config {
      self.app = ghostty_app_new(&runtimeConfig, rawConfig)
    }

    if let app {
      ghostty_app_set_focus(app, NSApp.isActive)
    }
    NotificationCenter.default.addObserver(
      self,
      selector: #selector(applicationDidBecomeActive),
      name: NSApplication.didBecomeActiveNotification,
      object: nil)
    NotificationCenter.default.addObserver(
      self,
      selector: #selector(applicationDidResignActive),
      name: NSApplication.didResignActiveNotification,
      object: nil)
  }

  deinit {
    NotificationCenter.default.removeObserver(self)
    if let app {
      ghostty_app_free(app)
    }
  }

  func appTick() {
    guard let app else { return }
    ghostty_app_tick(app)
  }

  func reloadConfig(soft: Bool = false) {
    guard let app else { return }
    if soft, let rawConfig = config.config {
      ghostty_app_update_config(app, rawConfig)
      return
    }
    let nextConfig = Ghostty.Config(at: configPath)
    guard let rawConfig = nextConfig.config else { return }
    ghostty_app_update_config(app, rawConfig)
    config = nextConfig
  }

  func requestClose(surface: ghostty_surface_t) {
    ghostty_surface_request_close(surface)
  }

  @objc private func applicationDidBecomeActive() {
    guard let app else { return }
    ghostty_app_set_focus(app, true)
  }

  @objc private func applicationDidResignActive() {
    guard let app else { return }
    ghostty_app_set_focus(app, false)
  }

  private static func handleAction(
    app: ghostty_app_t?,
    target: ghostty_target_s,
    action: ghostty_action_s
  ) -> Bool {
    switch action.tag {
    case GHOSTTY_ACTION_SET_TITLE:
      surfaceView(from: target)?.setTerminalTitle(action.action.set_title.title)
      return true
    case GHOSTTY_ACTION_PWD:
      guard let surfaceView = surfaceView(from: target),
            let pwdPtr = action.action.pwd.pwd,
            let pwd = String(cString: pwdPtr, encoding: .utf8)
      else { return true }
      surfaceView.pwd = pwd
      return true
    case GHOSTTY_ACTION_CELL_SIZE:
      surfaceView(from: target)?.setCellSize(
        width: action.action.cell_size.width,
        height: action.action.cell_size.height)
      return true
    case GHOSTTY_ACTION_RING_BELL:
      surfaceView(from: target)?.ringBell()
      return true
    case GHOSTTY_ACTION_START_SEARCH:
      surfaceView(from: target)?.startSearch(action.action.start_search)
      return true
    case GHOSTTY_ACTION_END_SEARCH:
      surfaceView(from: target)?.endSearch()
      return true
    case GHOSTTY_ACTION_SEARCH_TOTAL:
      surfaceView(from: target)?.setSearchTotal(action.action.search_total.total)
      return true
    case GHOSTTY_ACTION_SEARCH_SELECTED:
      surfaceView(from: target)?.setSearchSelected(action.action.search_selected.selected)
      return true
    case GHOSTTY_ACTION_SCROLLBAR:
      surfaceView(from: target)?.setScrollbar(Ghostty.Action.Scrollbar(c: action.action.scrollbar))
      return true
    case GHOSTTY_ACTION_OPEN_URL:
      guard let urlPtr = action.action.open_url.url, action.action.open_url.len > 0 else {
        return false
      }
      let length = Int(action.action.open_url.len)
      let urlString = urlPtr.withMemoryRebound(to: UInt8.self, capacity: length) { rawPtr in
        String(bytes: UnsafeBufferPointer(start: rawPtr, count: length), encoding: .utf8)
      }
      guard let urlString, let url = resolvedGhosttyOpenURL(urlString) else {
        return false
      }
      NSWorkspace.shared.open(url)
      return true
    case GHOSTTY_ACTION_RELOAD_CONFIG:
      guard let app, let userdata = ghostty_app_userdata(app) else { return false }
      let ghosttyApp = Unmanaged<GhostexGhosttyApp>.fromOpaque(userdata).takeUnretainedValue()
      Task { @MainActor in
        ghosttyApp.reloadConfig(soft: action.action.reload_config.soft)
      }
      return true
    default:
      return false
    }
  }

  private static func surfaceView(from target: ghostty_target_s) -> GhostexGhosttySurfaceView? {
    guard target.tag == GHOSTTY_TARGET_SURFACE,
      let surface = target.target.surface,
      let userdata = ghostty_surface_userdata(surface)
    else {
      return nil
    }
    return Unmanaged<GhostexGhosttySurfaceView>.fromOpaque(userdata).takeUnretainedValue()
  }

  private static func resolvedGhosttyOpenURL(_ value: String) -> URL? {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else {
      return nil
    }
    /**
     CDXC:TerminalLinks 2026-05-16-22:45:
     Command-clicking rich prompt image references should open the image file
     from `[Image #N](/path/to/image.png)`. Embedded Ghostty sends plain matched
     text through `open_url`, so ghostex must treat schemeless matches as file
     paths instead of passing relative URL objects to NSWorkspace, which fails
     with AppKit error -50 for normal filesystem paths.
     */
    let openValue = markdownImageReferencePath(in: trimmed) ?? trimmed
    if let candidate = URL(string: openValue), candidate.scheme?.isEmpty == false {
      return candidate
    }
    guard isGhosttyOpenFilePath(openValue) else {
      return nil
    }
    return URL(fileURLWithPath: NSString(string: openValue).standardizingPath)
  }

  private static func isGhosttyOpenFilePath(_ value: String) -> Bool {
    value.hasPrefix("/")
      || value.hasPrefix("~/")
      || value.hasPrefix("./")
      || value.hasPrefix("../")
      || value.contains("/")
  }

  private static func markdownImageReferencePath(in value: String) -> String? {
    guard value.hasPrefix("[Image #"),
      let openParen = value.firstIndex(of: "("),
      value.hasSuffix(")")
    else {
      return nil
    }
    let pathStart = value.index(after: openParen)
    let pathEnd = value.index(before: value.endIndex)
    guard pathStart < pathEnd else {
      return nil
    }
    let path = value[pathStart..<pathEnd].trimmingCharacters(in: .whitespacesAndNewlines)
    return path.isEmpty ? nil : path
  }

  private static func readClipboard(
    userdata: UnsafeMutableRawPointer?,
    location: ghostty_clipboard_e,
    state: UnsafeMutableRawPointer?
  ) -> Bool {
    let text = NSPasteboard.general.string(forType: .string) ?? ""
    text.withCString { ptr in
      ghostty_surface_complete_clipboard_request(callbackSurface(from: userdata), ptr, state, false)
    }
    return true
  }

  private static func confirmReadClipboard(
    userdata: UnsafeMutableRawPointer?,
    content: UnsafePointer<CChar>?,
    state: UnsafeMutableRawPointer?
  ) {
    guard let content else { return }
    ghostty_surface_complete_clipboard_request(callbackSurface(from: userdata), content, state, true)
  }

  private static func writeClipboard(
    location: ghostty_clipboard_e,
    content: UnsafePointer<ghostty_clipboard_content_s>?,
    len: UInt
  ) {
    guard let content, len > 0 else { return }
    let buffer = UnsafeBufferPointer(start: content, count: Int(len))
    for item in buffer {
      guard let data = item.data, let mime = item.mime else { continue }
      guard String(cString: mime).hasPrefix("text/plain") else { continue }
      NSPasteboard.general.clearContents()
      NSPasteboard.general.setString(String(cString: data), forType: .string)
      return
    }
  }

  private static func closeSurface(userdata: UnsafeMutableRawPointer?) {
    guard let userdata else { return }
    let view = Unmanaged<GhostexGhosttySurfaceView>.fromOpaque(userdata).takeUnretainedValue()
    Task { @MainActor in
      view.markProcessExited()
    }
  }

  private static func callbackSurface(from userdata: UnsafeMutableRawPointer?) -> ghostty_surface_t? {
    guard let userdata else { return nil }
    return Unmanaged<GhostexGhosttySurfaceView>.fromOpaque(userdata).takeUnretainedValue().surface
  }
}

struct GhostexGhosttySurfaceConfiguration {
  var command: String?
  var environmentVariables: [String: String] = [:]
  var initialInput: String?
  var waitAfterCommand = false
  var workingDirectory: String?
  /**
   CDXC:NativeTerminals 2026-05-11-14:18
   Direct Ghostty surfaces must preserve the wrapper launch semantics that
   loaded the user's normal interactive shell prompt and zsh startup files.
   Keep the default surface context as WINDOW, matching Ghostty.SurfaceConfiguration,
   instead of adopting a split default and changing shell startup behavior.
   */
  var context: ghostty_surface_context_e = GHOSTTY_SURFACE_CONTEXT_WINDOW

  func withCValue<T>(
    view: GhostexGhosttySurfaceView,
    _ body: (inout ghostty_surface_config_s) throws -> T
  ) rethrows -> T {
    var config = ghostty_surface_config_new()
    config.platform_tag = GHOSTTY_PLATFORM_MACOS
    config.platform = ghostty_platform_u(
      macos: ghostty_platform_macos_s(nsview: Unmanaged.passUnretained(view).toOpaque())
    )
    config.userdata = Unmanaged.passUnretained(view).toOpaque()
    config.scale_factor = Double(view.window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2)
    config.context = context
    config.wait_after_command = waitAfterCommand

    return try workingDirectory.withCString { cwd in
      config.working_directory = cwd
      return try command.withCString { command in
        config.command = command
        return try initialInput.withCString { initialInput in
          config.initial_input = initialInput
          let keys = Array(environmentVariables.keys)
          let values = Array(environmentVariables.values)
          return try keys.withCStrings { keyCStrings in
            try values.withCStrings { valueCStrings in
              var envVars: [ghostty_env_var_s] = []
              envVars.reserveCapacity(environmentVariables.count)
              for index in 0..<environmentVariables.count {
                envVars.append(ghostty_env_var_s(key: keyCStrings[index], value: valueCStrings[index]))
              }
              let envVarCount = envVars.count
              return try envVars.withUnsafeMutableBufferPointer { buffer in
                config.env_vars = buffer.baseAddress
                config.env_var_count = envVarCount
                return try body(&config)
              }
            }
          }
        }
      }
    }
  }
}

final class GhostexGhosttySearchState: ObservableObject {
  @Published var needle: String
  @Published var selected: UInt?
  @Published var total: UInt?

  init(needle: String = "", selected: UInt? = nil, total: UInt? = nil) {
    self.needle = needle
    self.selected = selected
    self.total = total
  }
}

struct GhostexGhosttySurfaceModel {
  let surface: ghostty_surface_t

  var foregroundPID: Int? {
    let pid = ghostty_surface_foreground_pid(surface)
    return pid > 0 ? Int(pid) : nil
  }

  var ttyName: String? {
    let value = Ghostty.AllocatedString(ghostty_surface_tty_name(surface)).string
    return value.isEmpty ? nil : value
  }

  func sendText(_ text: String) {
    text.withCString { ptr in
      ghostty_surface_text(surface, ptr, UInt(text.utf8.count))
    }
  }

  func readText(source: String?) -> String? {
    let pointTag: ghostty_point_tag_e =
      source == "visible" ? GHOSTTY_POINT_VIEWPORT : GHOSTTY_POINT_SCREEN
    let topLeft = ghostty_point_s(
      tag: pointTag,
      coord: GHOSTTY_POINT_COORD_TOP_LEFT,
      x: 0,
      y: 0
    )
    let bottomRight = ghostty_point_s(
      tag: pointTag,
      coord: GHOSTTY_POINT_COORD_BOTTOM_RIGHT,
      x: 0,
      y: 0
    )
    let selection = ghostty_selection_s(
      top_left: topLeft,
      bottom_right: bottomRight,
      rectangle: false
    )
    var result = ghostty_text_s(
      tl_px_x: 0,
      tl_px_y: 0,
      offset_start: 0,
      offset_len: 0,
      text: nil,
      text_len: 0
    )
    guard ghostty_surface_read_text(surface, selection, &result) else {
      return nil
    }
    defer {
      ghostty_surface_free_text(surface, &result)
    }
    guard let text = result.text, result.text_len > 0 else {
      return ""
    }
    let data = Data(bytes: text, count: Int(result.text_len))
    return String(data: data, encoding: .utf8) ?? String(decoding: data, as: UTF8.self)
  }

  @discardableResult
  func perform(action: String) -> Bool {
    action.withCString { ptr in
      ghostty_surface_binding_action(surface, ptr, UInt(action.lengthOfBytes(using: .utf8)))
    }
  }
}

private final class GhostexGhosttySurfaceHostView: NSView {
  private static let scrollButtonSize = CGSize(width: 37.5, height: 37.5)
  private static let scrollButtonRightInset: CGFloat = 17
  private static let scrollButtonBottomInset: CGFloat = 17
  private static let scrollButtonGap: CGFloat = 8.5
  private static let scrollButtonVisibilityThresholdPoints: CGFloat = 200
  private let scrollView = NSScrollView()
  private let documentView = NSView()
  private let scrollToBottomButton = TerminalPaneScrollButton(direction: .bottom)
  private let scrollToTopButton = TerminalPaneScrollButton(direction: .top)
  let surfaceView: GhostexGhosttySurfaceView
  private var observers: [NSObjectProtocol] = []
  private var isLiveScrolling = false
  private var lastSentRow: Int?

  init(surfaceView: GhostexGhosttySurfaceView) {
    self.surfaceView = surfaceView
    super.init(frame: .zero)
    /**
     CDXC:NativeTerminals 2026-05-14-09:24:
     Direct Ghostty runtime still needs Ghostty's native scroll-container model.
     Keep the terminal surface as the visible viewport renderer while an
     NSScrollView owns scrollbar rendering, scrollback geometry, and drag-to-row
     behavior. A plain host NSView removes the scrollbar and leaves wheel
     scrolling dependent only on surface event delivery.
     */
    scrollView.hasVerticalScroller = surfaceView.scrollbarConfiguration != .never
    scrollView.hasHorizontalScroller = false
    scrollView.autohidesScrollers = false
    scrollView.usesPredominantAxisScrolling = true
    scrollView.scrollerStyle = .overlay
    scrollView.drawsBackground = false
    scrollView.contentView.clipsToBounds = false
    scrollView.documentView = documentView
    documentView.addSubview(surfaceView)
    addSubview(scrollView)
    /*
     CDXC:NativeTerminalScroll 2026-05-26-13:58:
     Native terminal panes need the same in-pane scroll-to-bottom and
     scroll-to-top overlay UX as the prior VSmux terminal panes. Keep the
     controls inside the Ghostty NSScrollView host so they follow split layout,
     sit above terminal content, and invoke Ghostty's native viewport actions
     instead of synthesizing wheel input.

     CDXC:NativeTerminalScroll 2026-05-26-14:05:
     The scroll controls should sit 5px higher than the first pass and use plain
     chevrons, with the top action drawing an upward chevron and the bottom
     action drawing a downward chevron.
     */
    scrollToBottomButton.target = self
    scrollToBottomButton.action = #selector(scrollToBottomButtonPressed)
    scrollToTopButton.target = self
    scrollToTopButton.action = #selector(scrollToTopButtonPressed)
    addSubview(scrollToBottomButton)
    addSubview(scrollToTopButton)
    scrollView.contentView.postsBoundsChangedNotifications = true
    observers.append(NotificationCenter.default.addObserver(
      forName: NSView.boundsDidChangeNotification,
      object: scrollView.contentView,
      queue: .main
    ) { [weak self] _ in
      self?.synchronizeSurfaceView()
    })
    observers.append(NotificationCenter.default.addObserver(
      forName: NSScrollView.willStartLiveScrollNotification,
      object: scrollView,
      queue: .main
    ) { [weak self] _ in
      self?.isLiveScrolling = true
    })
    observers.append(NotificationCenter.default.addObserver(
      forName: NSScrollView.didEndLiveScrollNotification,
      object: scrollView,
      queue: .main
    ) { [weak self] _ in
      self?.isLiveScrolling = false
    })
    observers.append(NotificationCenter.default.addObserver(
      forName: NSScrollView.didLiveScrollNotification,
      object: scrollView,
      queue: .main
    ) { [weak self] _ in
      self?.handleLiveScroll()
    })
    observers.append(NotificationCenter.default.addObserver(
      forName: NSScroller.preferredScrollerStyleDidChangeNotification,
      object: nil,
      queue: nil
    ) { [weak self] _ in
      self?.scrollView.scrollerStyle = .overlay
    })
    surfaceView.onScrollbarChange = { [weak self] in
      self?.synchronizeScrollView()
    }
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  deinit {
    observers.forEach { NotificationCenter.default.removeObserver($0) }
    surfaceView.onScrollbarChange = nil
  }

  override var safeAreaInsets: NSEdgeInsets { NSEdgeInsetsZero }

  override func layout() {
    super.layout()
    scrollView.frame = bounds
    layoutScrollButtons()
    surfaceView.frame.size = scrollView.bounds.size
    documentView.frame.size.width = scrollView.bounds.width
    synchronizeScrollView()
    synchronizeSurfaceView()
  }

  private func synchronizeSurfaceView() {
    surfaceView.frame.origin = scrollView.contentView.documentVisibleRect.origin
  }

  private func synchronizeScrollView() {
    documentView.frame.size.height = documentHeight()
    if !isLiveScrolling {
      let cellHeight = surfaceView.cellSize.height
      if cellHeight > 0, let scrollbar = surfaceView.scrollbar {
        let offsetRows = max(
          CGFloat(scrollbar.total) - CGFloat(scrollbar.offset) - CGFloat(scrollbar.len),
          0)
        scrollView.contentView.scroll(to: CGPoint(x: 0, y: offsetRows * cellHeight))
        lastSentRow = Int(scrollbar.offset)
      }
    }
    scrollView.reflectScrolledClipView(scrollView.contentView)
    updateScrollButtonVisibility()
  }

  private func handleLiveScroll() {
    let cellHeight = surfaceView.cellSize.height
    guard cellHeight > 0 else { return }
    let visibleRect = scrollView.contentView.documentVisibleRect
    let scrollOffset = documentView.frame.height - visibleRect.origin.y - visibleRect.height
    let row = max(Int(scrollOffset / cellHeight), 0)
    guard row != lastSentRow else { return }
    lastSentRow = row
    _ = surfaceView.surfaceModel?.perform(action: "scroll_to_row:\(row)")
    updateScrollButtonVisibility()
  }

  private func documentHeight() -> CGFloat {
    let contentHeight = scrollView.contentSize.height
    let cellHeight = surfaceView.cellSize.height
    if cellHeight > 0, let scrollbar = surfaceView.scrollbar {
      let documentGridHeight = CGFloat(scrollbar.total) * cellHeight
      let padding = contentHeight - (CGFloat(scrollbar.len) * cellHeight)
      return documentGridHeight + padding
    }
    return contentHeight
  }

  override func mouseMoved(with event: NSEvent) {
    guard NSScroller.preferredScrollerStyle == .legacy else { return }
    scrollView.flashScrollers()
  }

  private func layoutScrollButtons() {
    let size = Self.scrollButtonSize
    let x = max(bounds.maxX - Self.scrollButtonRightInset - size.width, bounds.minX)
    let bottomY = bounds.minY + Self.scrollButtonBottomInset
    scrollToBottomButton.frame = CGRect(origin: CGPoint(x: x, y: bottomY), size: size)
    scrollToTopButton.frame = CGRect(
      origin: CGPoint(x: x, y: bottomY + size.height + Self.scrollButtonGap),
      size: size)
  }

  private func updateScrollButtonVisibility() {
    guard !isHidden, bounds.width >= 80, bounds.height >= 96 else {
      setScrollButton(scrollToBottomButton, visible: false)
      setScrollButton(scrollToTopButton, visible: false)
      return
    }

    let visibleRect = scrollView.contentView.documentVisibleRect
    let documentHeight = documentView.frame.height
    let distanceFromBottom = max(visibleRect.minY, 0)
    let distanceFromTop = max(documentHeight - visibleRect.maxY, 0)
    let shouldShowBottom = distanceFromBottom > Self.scrollButtonVisibilityThresholdPoints
    let shouldShowTop =
      distanceFromTop > Self.scrollButtonVisibilityThresholdPoints &&
      distanceFromBottom > Self.scrollButtonVisibilityThresholdPoints

    setScrollButton(scrollToBottomButton, visible: shouldShowBottom)
    setScrollButton(scrollToTopButton, visible: shouldShowTop)
  }

  private func setScrollButton(_ button: TerminalPaneScrollButton, visible: Bool) {
    guard button.isVisible != visible else {
      return
    }
    button.isVisible = visible
  }

  @objc private func scrollToBottomButtonPressed() {
    window?.makeFirstResponder(surfaceView)
    guard let scrollbar = surfaceView.scrollbar else {
      return
    }
    scrollTerminal(toRow: max(Int(scrollbar.total) - Int(scrollbar.len), 0))
  }

  @objc private func scrollToTopButtonPressed() {
    window?.makeFirstResponder(surfaceView)
    scrollTerminal(toRow: 0)
  }

  private func scrollTerminal(toRow row: Int) {
    let cellHeight = surfaceView.cellSize.height
    guard cellHeight > 0 else {
      return
    }
    let visibleHeight = scrollView.contentView.documentVisibleRect.height
    let y = max(documentView.frame.height - visibleHeight - (CGFloat(row) * cellHeight), 0)
    scrollView.contentView.scroll(to: CGPoint(x: 0, y: y))
    scrollView.reflectScrolledClipView(scrollView.contentView)
    lastSentRow = row
    _ = surfaceView.surfaceModel?.perform(action: "scroll_to_row:\(row)")
    updateScrollButtonVisibility()
  }
}

@MainActor
final class TerminalWorkspaceView: NSView {
  private struct TerminalSession {
    let containerView: TerminalPaneLeafContainerView
    let sessionId: String
    let view: GhostexGhosttySurfaceView
    let scrollView: GhostexGhosttySurfaceHostView
    let searchBarView: TerminalSearchBarView
    let titleBarView: TerminalSessionTitleBarView
    let borderView: TerminalPaneBorderView
    let persistenceLabelView: TerminalPanePersistenceLabelView
    let delayedSendLabelView: TerminalPaneDelayedSendLabelView
    var foregroundPid: Int?
    var sessionPersistenceName: String?
    var sessionPersistenceProvider: NativeSessionPersistenceProvider?
    var ttyName: String?
    var cancellables: Set<AnyCancellable> = []
  }

  private struct WebPaneSession {
    let browserTitleObservation: NSKeyValueObservation?
    let containerView: TerminalPaneLeafContainerView
    let chromiumView: GhostexCEFBrowserView?
    let diagnosticsBridge: T3CodePaneDiagnosticsBridge
    let hostView: WebPaneHostView
    let isManagedT3Pane: Bool
    let projectId: String?
    let sessionId: String
    let threadId: String?
    let title: String
    let workspaceRoot: String?
    let browserProfileID: UUID?
    let webView: WKWebView?
    let titleBarView: TerminalSessionTitleBarView
    let borderView: TerminalPaneBorderView

    var browserContentView: NSView {
      chromiumView ?? webView ?? hostView
    }

    var currentURLString: String? {
      chromiumView?.currentURLString ?? webView?.url?.absoluteString
    }

    var isLoading: Bool {
      chromiumView?.isLoading ?? webView?.isLoading ?? false
    }

    var canNavigateBack: Bool {
      chromiumView?.canGoBack ?? webView?.canGoBack ?? false
    }

    var canNavigateForward: Bool {
      chromiumView?.canGoForward ?? webView?.canGoForward ?? false
    }
  }

  private struct ProjectEditorBrowserTab {
    let tabId: String
    let chromiumView: GhostexCEFBrowserView?
    let hostView: WebPaneHostView
    let webView: WKWebView?
    var title: String
    var url: String
  }

  private struct ProjectEditorPaneSession {
    var activeTabId: String
    var chromiumView: GhostexCEFBrowserView?
    var hostView: WebPaneHostView
    let mode: String
    let projectId: String
    let projectTitle: String
    let showsProjectTabs: Bool
    var tabs: [ProjectEditorBrowserTab]
    let titleBarView: TerminalSessionTitleBarView?
    var webView: WKWebView?
    var title: String
    var url: String
  }

  private struct PaneResizeHit {
    let availableLength: CGFloat
    let boundaryIndex: Int
    let direction: NativeTerminalLayout.SplitDirection
    let path: String
    let rect: CGRect
    let trackCount: Int
  }

  private enum PaneContentHitRole: Equatable {
    case commands
    case workspace
  }

  private struct PaneContentHitRegion {
    let path: String
    let rect: CGRect
    let role: PaneContentHitRole
    let sessionId: String
  }

  private struct PaneResizeDrag {
    let availableLength: CGFloat
    let boundaryIndex: Int
    let direction: NativeTerminalLayout.SplitDirection
    let minimumAfter: CGFloat
    let minimumBefore: CGFloat
    let path: String
    let startCoordinate: CGFloat
    let startRatios: [CGFloat]
  }

  private struct CommandsPanelResizeDrag {
    let startHeight: CGFloat
    let startY: CGFloat
  }

  private struct ProjectEditorCompanionResizeDrag {
    let startWidth: CGFloat
    let startX: CGFloat
    let workspaceBounds: CGRect
  }

  private struct ProjectEditorCompanionLayout {
    let companionFrame: CGRect
    let contentFrame: CGRect
    let editorFrame: CGRect
    let resizeHandleFrame: CGRect
    let sessionId: String
  }

  private struct PaneHeaderDrag {
    var isDragging: Bool
    var lastLoggedMoveAt: TimeInterval
    var moveEventCount: Int
    let sourceSessionId: String
    let startedFromTab: Bool
    let startPoint: CGPoint
    var targetSessionId: String?
  }

  private struct PaneTabReorderDropTarget {
    let lineFrame: CGRect
    let ownerSessionId: String
    let position: PaneTabReorderPosition
    let targetSessionId: String
  }

  private struct PaneHeaderDropTarget {
    let feedbackSessionId: String
    let placement: PaneDropPlacement
    let targetSessionId: String
  }

  private struct CEFNativeDragSourceRelease {
    let chromiumView: GhostexCEFBrowserView
    let startWindowPoint: CGPoint
    var didDrag: Bool
    var didStartHoverBridge: Bool
    var lastHoverEventTime: TimeInterval
    var lastHoverWindowPoint: CGPoint?
    var lastHoverLogEventTime: TimeInterval
  }

  /**
   CDXC:PaneTabs 2026-05-15-12:00:
   Workspace pane tabs are 36pt tall, so the non-command titlebar must use the
   same height. Keeping the bar at 33pt centers tabs outside their owning chrome
   and makes the tab strip background/border visibly miss the tab edges.

   CDXC:PaneTabs 2026-05-15-09:27:
   The non-command tab bar needs to be taller than the tabs while preserving the
   existing 36pt tab controls. Use a 40pt bar so the tab strip has 2pt vertical
   breathing room above and below the unchanged tab buttons.

   CDXC:PaneTabs 2026-05-15-09:30:
   The non-command tab controls still render about 3px taller than the visible
   bar in the native host. Keep the tabs at 36pt and raise only the owning bar
   by 3pt so the bar catches up to the rendered tab extent.
   */
  private static let terminalTitleBarHeight: CGFloat = 42
  /**
   CDXC:PaneTabs 2026-05-15-08:29:
   Command pane tabs must match the visible command tab bar height exactly.
   Keep the command titlebar height as the single source of truth for command
   tab button layout and collapsed command-panel sizing.
  */
  private static let commandPanelTitleBarHeight: CGFloat = 26
  /**
   CDXC:CommandsPanel 2026-05-15-10:06
   Command panes render above workspace/editor surfaces, so pane-tab drag
   feedback must be higher than command terminal containers and resize rails.
   The feedback views are visual-only and hit-test transparent, so a high layer
   fixes command-pane drop visibility without stealing input.
   */
  private static let paneHeaderDragFeedbackZPosition: CGFloat = 10_700
  private static let collapsedCommandsPanelLeftMargin: CGFloat = 4
  private static let collapsedCommandsPanelRightMargin: CGFloat = 8
  /**
   CDXC:NativePaneResize 2026-05-11-18:42
   The native fallback pane gap must match the shared default used by sidebar
   settings so outside pane edges and internal split dividers stay one pixel
   wider on all sides even before the React settings payload arrives.
   */
  private static let defaultPaneGap: CGFloat = 13
  private static let singlePaneInset: CGFloat = 1
  private static let paneResizeMinimumHeight: CGFloat = 160
  private static let paneResizeMinimumWidth: CGFloat = 220
  private static let paneResizeOuterEdgeExclusion: CGFloat = 8
  private static let paneResizeRailWidth: CGFloat = 5
  /**
   CDXC:ZmxPersistence 2026-05-18-07:20:
   zmx-backed Ghostty panes can keep stale visual state after session switches
   and split-pane resize settles. Ask zmx to repaint the attached terminal
   surface, and debounce resize-triggered refreshes on the trailing edge until
   the user has stopped resizing.

   CDXC:ZmxPersistence 2026-05-18-15:03:
   Resize-triggered zmx refresh should fire sooner after the final resize event.
   Keep the debounce trailing-only and reduce the settle window to 800 ms.
   */
  private static let zmxPersistenceRefreshDebounceInterval: TimeInterval = 0.8
  /**
   CDXC:CommandsPanel 2026-05-14-08:12:
   The native command pane default must match the shared 27% sidebar model so first layout, resize reset, and persisted height normalization agree.

   CDXC:CommandsPanel 2026-05-16-07:36:
   Users need the bottom command pane to resize down to 5% of the window height. Keep the 27% default but clamp native drag geometry and persisted ratios to the same minimum.
   */
  private static let minimumCommandsPanelHeightRatio: CGFloat = 0.05
  private static let maximumCommandsPanelHeightRatio: CGFloat = 0.55
  private static let defaultCommandsPanelHeightRatio: CGFloat = 0.27
  /**
   CDXC:ProjectEditorCompanion 2026-05-14-09:19:
   Embedded VS Code should open with the currently active terminal or T3 Code session visible as a simple left companion pane.
   Keep this state native because the editor surface, terminal surface, and resize rail are AppKit/CEF/Ghostty views outside the React sidebar DOM.

   CDXC:ProjectEditorCompanion 2026-05-16-06:55:
   The companion pane width is a user preference shared by every project and app restart. New installs start at 32% of the workarea, and user resize/reset writes the same normalized ratio to native settings instead of storing it on a per-project workspace snapshot.
   */
  private static let defaultProjectEditorCompanionWidthRatio: CGFloat = 0.32
  private static let projectEditorMinimumWidth: CGFloat = 360
  private static let floatingCommandsPanelMargin: CGFloat = 25
  /**
   CDXC:NativePaneResize 2026-05-11-10:40
   Pane split resizing must match native model: only the actual split rail owns
   cursor and drag events. Do not use a window-local resize monitor, because it
   can compete with the sidebar resize handle.
   CDXC:NativePaneResize 2026-05-11-17:53
   Split rails must be real AppKit divider siblings that occupy the visible
   pane gap, not transparent overlays extending across pane content. The drag
   target is the divider rail itself.
   */
  private static let paneHeaderDragThreshold: CGFloat = 6
  private static let paneHeaderDragGhostMaxWidth: CGFloat = 230
  private static let cefNativeDragHoverInterval: TimeInterval = 1.0 / 30.0
  private static let cefNativeDragStationaryHoverInterval: TimeInterval = 0.12
  private static let cefNativeDragHoverMinimumDistance: CGFloat = 3
  private static let browserPaneApplicationNameForUserAgent = "Version/18.4 Safari/605.1.15"
  private static let floatingEditorMargin: CGFloat = 24
  private static let floatingEditorMinimumHeight: CGFloat = 260
  private static let floatingEditorMinimumWidth: CGFloat = 420
  private static let floatingEditorFrameDefaultsKey = "ghostex.floatingEditor.frame.v1"
  private static let defaultWorkspaceBackgroundColor = NSColor(
    calibratedRed: 14.0 / 255.0,
    green: 14.0 / 255.0,
    blue: 14.0 / 255.0,
    alpha: 1)
  private let ghostty: GhostexGhosttyApp
  private let sendEvent: (HostEvent) -> Void
  private var sessions: [String: TerminalSession] = [:]
  private var webPaneSessions: [String: WebPaneSession] = [:]
  private var projectEditorPaneSessions: [String: ProjectEditorPaneSession] = [:]
  private var webPaneFaviconTasksBySessionId: [String: Task<Void, Never>] = [:]
  private var completedWebPaneLoadSessionIds = Set<String>()
  private var pendingAuthenticatedWebPaneLoadSessionIds = Set<String>()
  private var t3ThreadRouteRetryAttemptsBySessionId = [String: Int]()
  private var activeSessionIds = Set<String>()
  private var commandsPanelActiveSessionIds = Set<String>()
  private var commandsPanelFocusedSessionId: String?
  private var commandsPanelHeightRatio: CGFloat = TerminalWorkspaceView.defaultCommandsPanelHeightRatio
  private var commandsPanelIsVisible = false
  private var commandsPanelLayout: NativeTerminalLayout?
  private var commandsPanelMode: String = "pinned"
  private var attentionSessionIds = Set<String>()
  private var poppedOutSessionIds = Set<String>()
  private var poppedOutPaneControllers: [String: PoppedOutPaneWindowController] = [:]
  private var poppedOutPlaceholderViews: [String: PoppedOutPanePlaceholderView] = [:]
  private var sleepingSessionIds = Set<String>()
  private var sessionAgentIconColors = [String: String]()
  private var sessionAgentIconDataUrls = [String: String]()
  private var sessionActivities = [String: NativeTerminalActivity]()
  private var sessionDelayedSendRemainingLabels = [String: String]()
  private var sessionFaviconDataUrls = [String: String]()
  private var sessionTitleBarActions = [String: [TerminalTitleBarAction]]()
  private var sessionTitles = [String: String]()
  private var showSessionIdInTerminalPanes = false
  private var activeProjectEditorId: String?
  private var focusedSessionId: String?
  private var lastEmittedFocusedSessionId: String?
  private var lastAppliedLayoutFocusRequestId: Int?
  private var workspaceBackgroundColorValue: String?
  private var paneGap = TerminalWorkspaceView.defaultPaneGap
  private var sidebarSide: SidebarSide = .left
  private var programmaticFocusDepth = 0
  private var terminalLayout: NativeTerminalLayout?
  private var paneContentHitRegions: [PaneContentHitRegion] = []
  private var paneResizeHits: [PaneResizeHit] = []
  private var paneResizeRatiosByPath: [String: [CGFloat]] = [:]
  private var paneResizeDrag: PaneResizeDrag?
  private var commandsPanelResizeDrag: CommandsPanelResizeDrag?
  private let commandsPanelChromeView = CommandsPanelChromeView()
  private let commandsPanelReservedBottomBarView = CommandsPanelChromeView(frame: .zero)
  private let commandsPanelCollapsedRightMarginView = NSView(frame: .zero)
  private let commandsPanelResizeHandleView = TerminalWorkspacePaneResizeHandleView()
  private var projectEditorCompanionSessionId: String?
  private var projectEditorCompanionIsVisible = false
  private var projectEditorCompanionPaneHidden = false
  private var projectEditorCompanionWidthRatio = TerminalWorkspaceView.defaultProjectEditorCompanionWidthRatio
  private var projectEditorCompanionResizeDrag: ProjectEditorCompanionResizeDrag?
  private var projectEditorCompanionResizeWorkspaceBounds: CGRect = .zero
  private let projectEditorCompanionResizeHandleView = TerminalWorkspacePaneResizeHandleView()
  private let persistProjectEditorCompanionWidthRatio: (CGFloat) -> Void
  private var paneResizeHandleViews: [TerminalWorkspacePaneResizeHandleView] = []
  private var paneHeaderDrag: PaneHeaderDrag?
  private var paneTabDragCaptureEventMonitor: Any?
  private var paneTabDragHiddenBrowserContentViews: [ObjectIdentifier: NSView] = [:]
  private var paneHeaderDragGhostView: TerminalPaneHeaderDragGhostView?
  private var paneHeaderDragTargetView: TerminalPaneHeaderDragTargetView?
  private var paneTabReorderTargetView: TerminalPaneTabReorderTargetView?
  private var cefNativeDragSourceRelease: CEFNativeDragSourceRelease?
  private var cefNativeDragSourceReleaseEventMonitor: Any?
  private var cefNativeDragHoverTimer: Timer?
  private var zmxPersistenceRefreshTimer: Timer?
  private var resizeLogSignatureBySessionId = [String: String]()
  private var exitPollTimer: Timer?
  private var floatingEditorOverlayView: FloatingEditorOverlayView?
  private var floatingEditorExitPollTimer: Timer?
  private var floatingEditorStatusFile: String?
  private var floatingEditorStatusWritten = false
  private var suppressNativeChromeInteractivity = false

  /**
   CDXC:EditorPanes 2026-05-06-18:51
   Project editor panes embed code-server, whose VS Code workbench owns
   browser-native drag/drop inside the primary sidebar. While an editor pane is
   active, the native pane resize/header-reorder layer must stand down so it
   cannot decorate or inspect the mouse stream before VS Code receives drop.
   CDXC:EditorPanes 2026-05-06-19:19
   Treat a visibly hosted project editor as the source of truth for drag/drop
   ownership. Native state can briefly lag focus/layout updates, but VS Code
   sidebar drops must still receive an unintercepted mouse stream.
   */
  private var isProjectEditorInteractionSurfaceActive: Bool {
    if let activeProjectEditorId, projectEditorPaneSessions[activeProjectEditorId] != nil {
      return true
    }
    return !visibleProjectEditorInteractionSessionIds.isEmpty
  }

  private var visibleProjectEditorInteractionSessionIds: [String] {
    projectEditorPaneSessions.values
      .filter(isProjectEditorInteractionSurfaceVisible)
      .map(\.projectId)
      .sorted()
  }

  private func isProjectEditorInteractionSurfaceVisible(_ session: ProjectEditorPaneSession) -> Bool {
    guard session.hostView.superview === self, !session.hostView.isHidden else {
      return false
    }
    let hostFrame = session.hostView.frame
    guard hostFrame.width > 1, hostFrame.height > 1 else {
      return false
    }
    return bounds.intersects(hostFrame)
  }

  /**
   CDXC:NativeTerminals 2026-04-26-06:44
   Project switching should show only the selected project's terminals.
   Inactive terminal surfaces are moved offscreen, and sidebar/native id
   translation decides which native Ghostty session is active.
   */
  init(
    ghostty: GhostexGhosttyApp,
    sendEvent: @escaping (HostEvent) -> Void,
    initialProjectEditorCompanionWidthRatio: CGFloat? = nil,
    persistProjectEditorCompanionWidthRatio: @escaping (CGFloat) -> Void = { _ in }
  ) {
    self.ghostty = ghostty
    self.sendEvent = sendEvent
    self.projectEditorCompanionWidthRatio = Self.normalizedProjectEditorCompanionWidthRatio(
      initialProjectEditorCompanionWidthRatio)
    self.persistProjectEditorCompanionWidthRatio = persistProjectEditorCompanionWidthRatio
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = Self.defaultWorkspaceBackgroundColor.cgColor
    commandsPanelChromeView.isHidden = true
    commandsPanelReservedBottomBarView.isHidden = true
    commandsPanelCollapsedRightMarginView.wantsLayer = true
    commandsPanelCollapsedRightMarginView.layer?.backgroundColor = NSColor.black.cgColor
    commandsPanelCollapsedRightMarginView.isHidden = true
    commandsPanelResizeHandleView.configure(direction: .vertical, cursor: .resizeUpDown)
    commandsPanelResizeHandleView.onMouseDown = { [weak self] event in
      _ = self?.beginCommandsPanelResize(with: event)
    }
    commandsPanelResizeHandleView.onMouseDragged = { [weak self] event in
      _ = self?.continueCommandsPanelResize(with: event)
    }
    commandsPanelResizeHandleView.onMouseUp = { [weak self] event in
      _ = self?.endCommandsPanelResize(with: event)
    }
    projectEditorCompanionResizeHandleView.configure(direction: .horizontal, cursor: .resizeLeftRight)
    projectEditorCompanionResizeHandleView.onMouseDown = { [weak self] event in
      _ = self?.beginProjectEditorCompanionResize(with: event)
    }
    projectEditorCompanionResizeHandleView.onMouseDragged = { [weak self] event in
      _ = self?.continueProjectEditorCompanionResize(with: event)
    }
    projectEditorCompanionResizeHandleView.onMouseUp = { [weak self] event in
      _ = self?.endProjectEditorCompanionResize(with: event)
    }
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  deinit {
    if let paneTabDragCaptureEventMonitor {
      NSEvent.removeMonitor(paneTabDragCaptureEventMonitor)
    }
    if let cefNativeDragSourceReleaseEventMonitor {
      NSEvent.removeMonitor(cefNativeDragSourceReleaseEventMonitor)
    }
    cefNativeDragHoverTimer?.invalidate()
    zmxPersistenceRefreshTimer?.invalidate()
    floatingEditorExitPollTimer?.invalidate()
  }

  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    window?.acceptsMouseMovedEvents = true
    syncCEFNativeDragSourceReleaseMonitor(reason: "viewDidMoveToWindow")
  }

  func openFloatingEditor(_ command: OpenFloatingEditor) {
    if command.editorKind == "monaco" {
      openFloatingMonacoEditor(command)
      return
    }

    guard let app = ghostty.app else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.ghosttyMissing",
        details: [
          "requestId": command.requestId ?? "",
          "title": command.title ?? "",
        ])
      return
    }
    guard let editorCommand = command.command?.trimmingCharacters(in: .whitespacesAndNewlines),
      !editorCommand.isEmpty
    else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.commandMissing",
        details: [
          "requestId": command.requestId ?? "",
          "title": command.title ?? "",
        ])
      return
    }

    closeFloatingEditorOverlay(requestGhosttyClose: true, reason: "replaceFloatingEditor")

    var config = GhostexGhosttySurfaceConfiguration()
    config.command = editorCommand
    config.environmentVariables = nativeGhosttyFloatingEditorEnvironment(command.env)
    config.waitAfterCommand = false
    if let cwd = command.cwd, !cwd.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      config.workingDirectory = cwd
    }

    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.open",
      details: [
        "command": editorCommand,
        "cwd": command.cwd ?? "",
        "envCount": config.environmentVariables.count,
        "requestId": command.requestId ?? "",
        "title": command.title ?? "",
      ])

    let surfaceView = withNativeGhosttyTerminalProcessEnvironment {
      GhostexGhosttySurfaceView(app, baseConfig: config)
    }
    surfaceView.scrollbarConfiguration = ghostty.config.scrollbar
    surfaceView.translatesAutoresizingMaskIntoConstraints = false
    let returnFocusSessionId =
      command.originatingSessionId?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
      ? command.originatingSessionId
      : focusedSessionId
    let overlayView = FloatingEditorOverlayView(
      title: command.title?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        ? command.title!
        : "Floating Editor",
      returnFocusSessionId: returnFocusSessionId,
      surfaceView: surfaceView
    )
    overlayView.translatesAutoresizingMaskIntoConstraints = true
    let storedFrame = storedFloatingEditorFrame()
    overlayView.frame = storedFrame.map(clampedFloatingEditorFrame) ?? defaultFloatingEditorFrame()
    overlayView.isUserPositioned = storedFrame != nil
    overlayView.closeHandler = { [weak self] in
      self?.closeFloatingEditorOverlay(requestGhosttyClose: true, reason: "titlebarClose")
    }
    overlayView.saveHandler = { [weak self, weak overlayView] in
      guard let self, let overlayView else {
        return
      }
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.saveRequested",
        details: [
          "returnFocusSessionId": overlayView.returnFocusSessionId ?? "",
      ])
      overlayView.setSaving()
      overlayView.surfaceView?.surfaceModel?.sendText("\u{7}")
      if let surfaceView = overlayView.surfaceView {
        self.window?.makeFirstResponder(surfaceView)
      }
    }
    overlayView.dragHandler = { [weak self, weak overlayView] delta in
      guard let self, let overlayView else {
        return
      }
      var frame = overlayView.frame
      frame.origin.x += delta.x
      frame.origin.y += delta.y
      overlayView.frame = self.clampedFloatingEditorFrame(frame)
      overlayView.isUserPositioned = true
      self.persistFloatingEditorFrame(overlayView.frame)
    }
    overlayView.resizeHandler = { [weak self, weak overlayView] delta in
      guard let self, let overlayView else {
        return
      }
      var frame = overlayView.frame
      frame.size.width += delta.x
      frame.size.height -= delta.y
      frame.origin.y += delta.y
      overlayView.frame = self.clampedFloatingEditorFrame(frame)
      overlayView.isUserPositioned = true
      self.persistFloatingEditorFrame(overlayView.frame)
    }
    addSubview(overlayView)
    floatingEditorOverlayView = overlayView
    logFloatingEditorOverlayState("mounted.ghostty")
    floatingEditorStatusFile = command.statusFile
    floatingEditorStatusWritten = false
    startFloatingEditorExitPolling()
    needsLayout = true
    orderFloatingEditorOverlayToFront()
    window?.makeFirstResponder(surfaceView)
  }

  private func openFloatingMonacoEditor(_ command: OpenFloatingEditor) {
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoReceived",
      details: [
        "filePath": command.filePath ?? "",
        "language": command.language ?? "",
        "requestId": command.requestId ?? "",
        "statusFile": command.statusFile ?? "",
        "title": command.title ?? "",
      ])
    guard let filePath = command.filePath?.trimmingCharacters(in: .whitespacesAndNewlines),
      !filePath.isEmpty
    else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.monacoFileMissing",
        details: [
          "requestId": command.requestId ?? "",
          "title": command.title ?? "",
        ])
      return
    }
    guard
      let webAssets = Bundle.main.resourceURL?.appendingPathComponent("Web", isDirectory: true),
      FileManager.default.fileExists(
        atPath: webAssets.appendingPathComponent("floating-monaco-editor.html").path)
    else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.monacoAssetsMissing",
        details: [
          "filePath": filePath,
          "requestId": command.requestId ?? "",
        ])
      writeFloatingEditorStatusFile(command.statusFile, status: "cancelled")
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoAssetsReady",
      details: [
        "requestId": command.requestId ?? "",
        "webAssets": webAssets.path,
      ])

    closeFloatingEditorOverlay(requestGhosttyClose: true, reason: "replaceFloatingEditor")

    let initialText = (try? String(contentsOfFile: filePath, encoding: .utf8)) ?? ""
    let language =
      command.language?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
      ? command.language!
      : "markdown"
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoInitialTextRead",
      details: [
        "filePath": filePath,
        "initialTextLength": "\(initialText.count)",
        "language": language,
        "requestId": command.requestId ?? "",
      ])
    let userContentController = WKUserContentController()
    userContentController.addUserScript(
      WKUserScript(
        source:
          "window.__GHOSTEX_MONACO_INITIAL_TEXT__ = \(nativeJavaScriptLiteral(initialText)); window.__GHOSTEX_MONACO_LANGUAGE__ = \(nativeJavaScriptLiteral(language));",
        injectionTime: .atDocumentStart,
        forMainFrameOnly: true
      ))
    let configuration = WKWebViewConfiguration()
    configuration.userContentController = userContentController
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoWebViewCreateStart",
      details: [
        "requestId": command.requestId ?? ""
      ])
    let webView = WKWebView(frame: .zero, configuration: configuration)
    webView.translatesAutoresizingMaskIntoConstraints = false
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoWebViewCreated",
      details: [
        "requestId": command.requestId ?? ""
      ])

    let returnFocusSessionId =
      command.originatingSessionId?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
      ? command.originatingSessionId
      : focusedSessionId
    let overlayView = FloatingEditorOverlayView(
      title: command.title?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        ? command.title!
        : "Prompt Editor",
      returnFocusSessionId: returnFocusSessionId,
      webView: webView
    )
    overlayView.translatesAutoresizingMaskIntoConstraints = true
    let storedFrame = storedFloatingEditorFrame()
    overlayView.frame = storedFrame.map(clampedFloatingEditorFrame) ?? defaultFloatingEditorFrame()
    overlayView.isUserPositioned = storedFrame != nil
    overlayView.closeHandler = { [weak self] in
      self?.closeFloatingEditorOverlay(requestGhosttyClose: false, reason: "titlebarClose")
    }
    overlayView.saveHandler = { [weak self, weak overlayView, weak webView] in
      guard let self, let overlayView, let webView else {
        return
      }
      overlayView.setSaving()
      self.saveFloatingMonacoEditor(
        overlayView: overlayView,
        webView: webView,
        filePath: filePath,
        statusFile: command.statusFile
      )
    }
    overlayView.dragHandler = { [weak self, weak overlayView] delta in
      guard let self, let overlayView else {
        return
      }
      var frame = overlayView.frame
      frame.origin.x += delta.x
      frame.origin.y += delta.y
      overlayView.frame = self.clampedFloatingEditorFrame(frame)
      overlayView.isUserPositioned = true
      self.persistFloatingEditorFrame(overlayView.frame)
    }
    overlayView.resizeHandler = { [weak self, weak overlayView] delta in
      guard let self, let overlayView else {
        return
      }
      var frame = overlayView.frame
      frame.size.width += delta.x
      frame.size.height -= delta.y
      frame.origin.y += delta.y
      overlayView.frame = self.clampedFloatingEditorFrame(frame)
      overlayView.isUserPositioned = true
      self.persistFloatingEditorFrame(overlayView.frame)
    }

    addSubview(overlayView)
    floatingEditorOverlayView = overlayView
    logFloatingEditorOverlayState("mounted.monaco")
    floatingEditorStatusFile = command.statusFile
    floatingEditorStatusWritten = false
    needsLayout = true
    orderFloatingEditorOverlayToFront()
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoLoadFileStart",
      details: [
        "htmlPath": webAssets.appendingPathComponent("floating-monaco-editor.html").path,
        "requestId": command.requestId ?? "",
      ])
    webView.loadFileURL(
      webAssets.appendingPathComponent("floating-monaco-editor.html"),
      allowingReadAccessTo: webAssets
    )
    window?.makeFirstResponder(webView)

    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.monacoOpen",
      details: [
        "filePath": filePath,
        "language": language,
        "requestId": command.requestId ?? "",
        "statusFile": command.statusFile ?? "",
      ])
  }

  private func saveFloatingMonacoEditor(
    overlayView: FloatingEditorOverlayView,
    webView: WKWebView,
    filePath: String,
    statusFile: String?
  ) {
    webView.evaluateJavaScript("window.ghostexMonacoGetValue ? window.ghostexMonacoGetValue() : ''") {
      [weak self, weak overlayView] result, error in
      DispatchQueue.main.async {
        guard let self, let overlayView, self.floatingEditorOverlayView === overlayView else {
          return
        }
        if let error {
          TerminalFocusDebugLog.append(
            event: "nativeWorkspace.floatingEditor.monacoSaveFailed",
            details: [
              "error": error.localizedDescription,
              "filePath": filePath,
            ])
          overlayView.resetSaveButton()
          return
        }
        let text = result as? String ?? ""
        do {
          try text.write(toFile: filePath, atomically: true, encoding: .utf8)
          self.writeFloatingEditorStatusFile(statusFile, status: "saved")
          self.closeFloatingEditorOverlay(requestGhosttyClose: false, reason: "monacoSaved")
        } catch {
          TerminalFocusDebugLog.append(
            event: "nativeWorkspace.floatingEditor.monacoSaveFailed",
            details: [
              "error": error.localizedDescription,
              "filePath": filePath,
            ])
          overlayView.resetSaveButton()
        }
      }
    }
  }

  private func writeFloatingEditorStatusFile(_ statusFile: String?, status: String) {
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
      floatingEditorStatusWritten = true
    } catch {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.floatingEditor.statusWriteFailed",
        details: [
          "error": error.localizedDescription,
          "status": status,
          "statusFile": statusFile,
        ])
    }
  }

  private func defaultFloatingEditorFrame() -> CGRect {
    let margin = Self.floatingEditorMargin
    let availableWidth = max(Self.floatingEditorMinimumWidth, bounds.width - margin * 2)
    let availableHeight = max(Self.floatingEditorMinimumHeight, bounds.height - margin * 2)
    let width = min(max(Self.floatingEditorMinimumWidth, bounds.width * 0.74), availableWidth)
    let height = min(max(Self.floatingEditorMinimumHeight, bounds.height * 0.44), availableHeight)
    return CGRect(
      x: max(margin, (bounds.width - width) / 2),
      y: margin,
      width: width,
      height: height
    )
  }

  private func storedFloatingEditorFrame() -> CGRect? {
    guard
      let stored = UserDefaults.standard.string(forKey: Self.floatingEditorFrameDefaultsKey)
    else {
      return nil
    }
    let frame = NSRectFromString(stored)
    guard frame.width > 1, frame.height > 1 else {
      return nil
    }
    return frame
  }

  private func persistFloatingEditorFrame(_ frame: CGRect) {
    UserDefaults.standard.set(NSStringFromRect(frame), forKey: Self.floatingEditorFrameDefaultsKey)
  }

  private func clampedFloatingEditorFrame(_ frame: CGRect) -> CGRect {
    guard bounds.width > 1, bounds.height > 1 else {
      return frame
    }
    let margin = min(Self.floatingEditorMargin, max(4, min(bounds.width, bounds.height) / 8))
    let maxWidth = max(240, bounds.width - margin * 2)
    let maxHeight = max(180, bounds.height - margin * 2)
    var next = frame
    next.size.width = min(max(240, next.width), maxWidth)
    next.size.height = min(max(180, next.height), maxHeight)
    next.origin.x = min(max(margin, next.origin.x), max(margin, bounds.width - next.width - margin))
    next.origin.y = min(max(margin, next.origin.y), max(margin, bounds.height - next.height - margin))
    return next
  }

  private func layoutFloatingEditorOverlay() {
    guard let overlayView = floatingEditorOverlayView else {
      return
    }
    logFloatingEditorOverlayState("layout")
    if overlayView.frame.isEmpty || !overlayView.isUserPositioned {
      overlayView.frame = defaultFloatingEditorFrame()
    } else {
      let clampedFrame = clampedFloatingEditorFrame(overlayView.frame)
      overlayView.frame = clampedFrame
      persistFloatingEditorFrame(clampedFrame)
    }
    orderFloatingEditorOverlayToFront()
  }

  private func orderFloatingEditorOverlayToFront() {
    guard let overlayView = floatingEditorOverlayView, overlayView.superview === self else {
      return
    }
    overlayView.layer?.zPosition = 10_000
    logFloatingEditorOverlayState("orderedToFront")
  }

  private func logFloatingEditorOverlayState(_ reason: String) {
    guard let overlayView = floatingEditorOverlayView else {
      return
    }
    /**
     CDXC:FloatingEditor 2026-05-11-20:24
     The floating editor is an intentional interactive overlay only while
     visible. Log hidden/transparent mounted states because those would become
     invisible click blockers over workspace panes.
     */
    guard overlayView.superview != nil && (overlayView.isHidden || overlayView.alphaValue <= 0) else {
      return
    }
    TerminalFocusDebugLog.append(event: "nativeWorkspace.floatingEditor.invisibleMounted", details: [
      "alpha": overlayView.alphaValue,
      "isHidden": overlayView.isHidden,
      "reason": reason,
    ])
  }

  private func startFloatingEditorExitPolling() {
    floatingEditorExitPollTimer?.invalidate()
    floatingEditorExitPollTimer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) {
      [weak self] _ in
      Task { @MainActor in
        self?.pollFloatingEditorExit()
      }
    }
  }

  private func pollFloatingEditorExit() {
    guard let overlayView = floatingEditorOverlayView else {
      floatingEditorExitPollTimer?.invalidate()
      floatingEditorExitPollTimer = nil
      return
    }
    if overlayView.surfaceView?.processExited == true {
      closeFloatingEditorOverlay(requestGhosttyClose: false, reason: "processExited")
    }
  }

  private func closeFloatingEditorOverlay(requestGhosttyClose: Bool, reason: String) {
    guard let overlayView = floatingEditorOverlayView else {
      return
    }
    let processExited = overlayView.surfaceView?.processExited ?? true
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.floatingEditor.close",
      details: [
        "processExited": processExited,
        "reason": reason,
        "requestGhosttyClose": requestGhosttyClose,
      ])
    if requestGhosttyClose, !processExited, let surface = overlayView.surfaceView?.surface {
      ghostty.requestClose(surface: surface)
    }
    if let surfaceView = overlayView.surfaceView {
      if !processExited, !floatingEditorStatusWritten, reason != "processExited" {
        writeFloatingEditorStatusFile(floatingEditorStatusFile, status: "cancelled")
      }
      NativeTerminalProcessMonitor.terminateSessionProcesses(
        ttyName: surfaceView.surfaceModel?.ttyName,
        foregroundPid: surfaceView.surfaceModel?.foregroundPID,
        reason: reason)
    } else if !floatingEditorStatusWritten, reason != "monacoSaved" {
      writeFloatingEditorStatusFile(floatingEditorStatusFile, status: "cancelled")
    }
    persistFloatingEditorFrame(overlayView.frame)
    floatingEditorExitPollTimer?.invalidate()
    floatingEditorExitPollTimer = nil
    overlayView.removeFromSuperview()
    floatingEditorOverlayView = nil
    floatingEditorStatusFile = nil
    floatingEditorStatusWritten = false
    if let returnFocusSessionId = overlayView.returnFocusSessionId,
      sessions[returnFocusSessionId] != nil
    {
      focusTerminal(sessionId: returnFocusSessionId, reason: "floatingEditor.\(reason)")
    }
  }

  func checkPersistenceSession(_ command: CheckPersistenceSession) {
    guard let provider = NativeSessionPersistenceProvider(rawValue: command.provider) else {
      sendEvent(
        .persistenceSessionState(
          requestId: command.requestId,
          provider: command.provider,
          sessionName: command.sessionName,
          exists: false,
          error: "unsupported-provider"
        ))
      return
    }
    let sendEvent = self.sendEvent
    DispatchQueue.global(qos: .utility).async {
      let result = NativeSessionPersistenceMode.sessionExists(
        provider: provider,
        sessionName: command.sessionName
      )
      DispatchQueue.main.async {
        sendEvent(
          .persistenceSessionState(
            requestId: command.requestId,
            provider: command.provider,
            sessionName: command.sessionName,
            exists: result.exists,
            error: result.error
          ))
      }
    }
  }

  func createTerminal(_ command: CreateTerminal) {
    let activateOnCreate = command.activateOnCreate ?? true
    let forcePreviousSessionRestoreDiagnostics = command.diagnosticSource == "previousSessionRestore"
    /**
     CDXC:CrashDiagnostics 2026-05-04-09:10
     Rapid sidebar agent launches must identify whether the crash happens
     before Ghostty surface allocation, during mount, or after ready events.
     Keep these breadcrumbs in the native focus log alongside layout sync.

     CDXC:PreviousSessions 2026-05-17-03:18:
     Previous-session restore repros need native create breadcrumbs even when Settings Debugging Mode was not enabled before the bad click.
     Force only commands tagged by the sidebar restore flow so zmx attach and AppKit surface creation can be correlated with the sidebar restore trace.
     */
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.createTerminal.received",
      details: [
        "activateOnCreate": activateOnCreate,
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "diagnosticSource": command.diagnosticSource ?? "",
        "hasInitialInput": command.initialInput?.isEmpty == false,
        "initialInputPreview": command.initialInput.map { summarizeTerminalText($0) } ?? "",
        "knownSessionIds": Array(sessions.keys).sorted(),
        "requestedSessionId": command.sessionId,
        "title": command.title ?? "",
      ],
      force: forcePreviousSessionRestoreDiagnostics)
    if let existingSession = sessions[command.sessionId] {
      let hasInitialInput = command.initialInput?.isEmpty == false
      let shouldForceExistingInitialInputDiagnostics =
        forcePreviousSessionRestoreDiagnostics
        || (hasInitialInput && existingSession.sessionPersistenceProvider != nil)
      /**
       CDXC:SessionRestoreDiagnostics 2026-05-23-10:05:
       Existing native surfaces must expose whether createTerminal is about to
       paste sidebar-supplied restore input into an already alive provider
       session. This is the exact boundary that can leak `Restoring session...`
       and `x resume ...` into an agent CLI prompt when JS state forgot a live
       native/zmx runtime.
       */
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.createTerminal.existing",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "diagnosticSource": command.diagnosticSource ?? "",
          "hasInitialInput": hasInitialInput,
          "initialInputPreview": command.initialInput.map { summarizeTerminalText($0) } ?? "",
          "requestedSessionId": command.sessionId,
          "sessionPersistenceName": existingSession.sessionPersistenceName ?? "",
          "sessionPersistenceProvider": existingSession.sessionPersistenceProvider?.rawValue ?? "off",
        ],
        force: shouldForceExistingInitialInputDiagnostics)
      focusTerminal(sessionId: command.sessionId, reason: "createTerminalExisting")
      if let initialInput = command.initialInput, !initialInput.isEmpty {
        /**
         CDXC:SessionRestoreDiagnostics 2026-05-23-14:17:
         Provider-backed existing surfaces are already attached to their live
         tmux/zmx/zellij session, so restore input is a creation-only command.
         Focusing must not paste `Restoring session...` or agent resume scripts
         into an already running CLI prompt.
         */
        if existingSession.sessionPersistenceProvider != nil {
          TerminalFocusDebugLog.append(
            event: "nativeWorkspace.createTerminal.existing.initialInputSkipped",
            details: [
              "activeSessionIds": Array(activeSessionIds).sorted(),
              "diagnosticSource": command.diagnosticSource ?? "",
              "requestedSessionId": command.sessionId,
              "sessionPersistenceName": existingSession.sessionPersistenceName ?? "",
              "sessionPersistenceProvider": existingSession.sessionPersistenceProvider?.rawValue ?? "off",
              "textLength": initialInput.count,
              "textPreview": summarizeTerminalText(initialInput),
            ],
            force: shouldForceExistingInitialInputDiagnostics)
        } else {
          TerminalFocusDebugLog.append(
            event: "nativeWorkspace.createTerminal.existing.initialInputWrite",
            details: [
              "activeSessionIds": Array(activeSessionIds).sorted(),
              "diagnosticSource": command.diagnosticSource ?? "",
              "requestedSessionId": command.sessionId,
              "sessionPersistenceName": existingSession.sessionPersistenceName ?? "",
              "sessionPersistenceProvider": existingSession.sessionPersistenceProvider?.rawValue ?? "off",
              "textLength": initialInput.count,
              "textPreview": summarizeTerminalText(initialInput),
            ],
            force: shouldForceExistingInitialInputDiagnostics)
          writeTerminalText(sessionId: command.sessionId, text: initialInput)
        }
      }
      return
    }

    guard let app = ghostty.app else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.createTerminal.ghosttyMissing",
        details: [
          "requestedSessionId": command.sessionId,
          "title": command.title ?? "",
        ])
      sendEvent(
        .terminalError(sessionId: command.sessionId, message: "Ghostty runtime is not ready"))
      return
    }

    let sessionPersistenceProvider = NativeSessionPersistenceProvider.resolve(command)
    let sessionPersistenceName: String?
    if let sessionPersistenceProvider {
      sessionPersistenceName =
        NativeSessionPersistenceMode.compactSessionName(command.sessionId)
        ??
        NativeSessionPersistenceMode.normalizedSessionName(
          command.sessionPersistenceName ?? command.tmuxSessionName,
          provider: sessionPersistenceProvider)
        ?? NativeSessionPersistenceMode.sessionName(
          provider: sessionPersistenceProvider,
          sessionId: command.sessionId,
          title: command.title)
    } else {
      sessionPersistenceName = nil
    }
    let persistenceSessionExisted =
      sessionPersistenceProvider.flatMap { provider in
        sessionPersistenceName.map { sessionName in
          NativeSessionPersistenceMode.sessionExists(
            provider: provider,
            sessionName: sessionName
          ).exists
        }
      }
    /**
     CDXC:SessionPersistence 2026-05-26-17:20:
     Sidebar sleeping state means "no native pane is mounted"; it no longer
     implies the tmux/zmx/zellij backend is missing. Probe provider liveness
     before attach so terminalReady can tell the sidebar whether queued restore
     text is safe to run only for newly created provider sessions.
     */
    var config = GhostexGhosttySurfaceConfiguration()
    config.workingDirectory = command.cwd
    config.environmentVariables = nativeGhosttyTerminalEnvironment(command.env, sessionId: command.sessionId)
    /**
     CDXC:CommandPanes 2026-05-20-22:52:
     Command-pane actions need a hidden launch command so Ghostty executes the
     status wrapper as process setup rather than echoed terminal input. Provider
     attach commands always create normal shells; the sidebar sends provider
     startup text only after terminalReady.
     */
    config.command = command.shellCommand
    config.initialInput = sessionPersistenceProvider == nil ? command.initialInput : nil
    if let sessionPersistenceProvider, let sessionPersistenceName {
      /**
       CDXC:SessionPersistence 2026-05-05-07:28
       When a persistence provider is selected, each ghostex sidebar terminal
       creates or attaches to one named provider session. The app does not
       inspect provider-internal panes/windows/tabs; the sidebar card remains
       mapped to the original attached terminal surface.

       CDXC:SessionPersistence 2026-05-05-07:28
       App restart must reconnect to existing provider sessions without
       replaying agent launch or resume input into the live pane. Provider
       creation scripts must stay normal shells; the sidebar owns one-shot
       startup input after terminalReady.
       */
      config.command = NativeSessionPersistenceMode.attachCommand(
        provider: sessionPersistenceProvider,
        cwd: command.cwd,
        title: command.title,
        sessionName: sessionPersistenceName
      )
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.createTerminal.surfaceInit.start",
      details: [
        "commandColorEnv": nativeTerminalColorEnvironmentSnapshot(config.environmentVariables),
        "envCount": config.environmentVariables.count,
        "hasShellCommand": command.shellCommand?.isEmpty == false,
        "hasInitialInput": command.initialInput?.isEmpty == false,
        "processColorEnv": nativeTerminalColorEnvironmentSnapshot(ProcessInfo.processInfo.environment),
        "requestedSessionId": command.sessionId,
        "surfaceProcessColorEnv": nativeTerminalColorEnvironmentSnapshot(
          nativeGhosttyTerminalEffectiveProcessEnvironment()),
        "diagnosticSource": command.diagnosticSource ?? "",
        "title": command.title ?? "",
        "sessionPersistenceName": sessionPersistenceName ?? "",
        "sessionPersistenceProvider": sessionPersistenceProvider?.rawValue ?? "off",
        "workingDirectory": command.cwd,
      ],
      force: forcePreviousSessionRestoreDiagnostics)
    let surfaceView = withNativeGhosttyTerminalProcessEnvironment {
      GhostexGhosttySurfaceView(app, baseConfig: config)
    }
    surfaceView.scrollbarConfiguration = ghostty.config.scrollbar
    surfaceView.ghostexSessionId = command.sessionId
    surfaceView.onKeyDownProbe = { [weak self] surfaceView, event, phase in
      self?.logSurfaceKeyDownProbe(surfaceView: surfaceView, event: event, phase: phase)
    }
    surfaceView.onTextInputProbe = { [weak self] surfaceView, text, replacementRange in
      self?.logSurfaceTextInputProbe(
        surfaceView: surfaceView,
        text: text,
        replacementRange: replacementRange)
    }
    surfaceView.onMouseDownFocus = { [weak self] surfaceView, event in
      self?.focusTerminalFromContentMouseDown(surfaceView: surfaceView, event: event)
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.createTerminal.surfaceInit.completed",
      details: [
        "diagnosticSource": command.diagnosticSource ?? "",
        "hasSurfaceModel": surfaceView.surfaceModel != nil,
        "requestedSessionId": command.sessionId,
      ],
      force: forcePreviousSessionRestoreDiagnostics)
    surfaceView.translatesAutoresizingMaskIntoConstraints = false
    /**
     CDXC:NativeTerminals 2026-04-28-03:09
     Embedded Ghostty terminals must expose the same visible scrollback
     scrollbar as Ghostty windows. Mount the surface through Ghostty's native
     scroll wrapper so scrollbar state, dragging, and scrollback positioning
     are driven by the terminal core instead of a separate overlay.
    */
    let scrollView = GhostexGhosttySurfaceHostView(surfaceView: surfaceView)
    scrollView.translatesAutoresizingMaskIntoConstraints = false
    let searchBarView = TerminalSearchBarView(surfaceView: surfaceView)
    searchBarView.translatesAutoresizingMaskIntoConstraints = false
    let titleBarView = TerminalSessionTitleBarView(
      title: normalizedTerminalSessionTitle(command.title, sessionId: command.sessionId)
    )
    titleBarView.setDebugContext(ownerSessionId: command.sessionId, paneKind: "terminal")
    titleBarView.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
    titleBarView.translatesAutoresizingMaskIntoConstraints = false
    titleBarView.onMouseDown = { [weak self] event in
      self?.handlePaneTitleBarMouseDown(
        event,
        sessionId: command.sessionId,
        focusReason: "nativeTitleBarMouseDown")
    }
    titleBarView.onAction = { [weak self] action in
      guard let self else { return }
      self.focusTerminal(sessionId: command.sessionId, reason: "nativeTitleBarAction")
      self.applyOptimisticPanePopOutAction(
        sessionId: command.sessionId,
        action: action,
        reason: "nativeTitleBarAction")
      self.sendEvent(.terminalTitleBarAction(sessionId: command.sessionId, action: action))
    }
    titleBarView.onTabMouseDown = { [weak self] event, tabSessionId in
      self?.handlePaneTabMouseDown(event, sessionId: tabSessionId)
    }
    titleBarView.onTabMouseDragged = { [weak self] event, tabSessionId in
      self?.handlePaneTitleBarMouseDragged(event, sessionId: tabSessionId)
    }
    titleBarView.onTabMouseUp = { [weak self] event, tabSessionId in
      self?.handlePaneTabMouseUp(event, sessionId: tabSessionId)
    }
    titleBarView.onTabCloseRequested = { [weak self] tabSessionId, scope in
      self?.sendEvent(.paneTabCloseRequested(sessionId: tabSessionId, scope: scope))
    }
    titleBarView.onTabSleepRequested = { [weak self] tabSessionId, scope in
      self?.sendEvent(.paneTabSleepRequested(sessionId: tabSessionId, scope: scope))
    }
    titleBarView.onTabFocusRequested = { [weak self] tabSessionId in
      self?.sendEvent(.paneTabFocusRequested(sessionId: tabSessionId))
    }
    titleBarView.onTabActionRequested = { [weak self] tabSessionId, action in
      self?.handlePaneTabActionRequested(sessionId: tabSessionId, action: action)
    }
    let borderView = TerminalPaneBorderView()
    borderView.translatesAutoresizingMaskIntoConstraints = false
    let persistenceLabelView = TerminalPanePersistenceLabelView()
    persistenceLabelView.translatesAutoresizingMaskIntoConstraints = false
    persistenceLabelView.setProvider(sessionPersistenceProvider, sessionName: sessionPersistenceName)
    let delayedSendLabelView = TerminalPaneDelayedSendLabelView()
    delayedSendLabelView.translatesAutoresizingMaskIntoConstraints = false
    let containerView = TerminalPaneLeafContainerView()
    containerView.translatesAutoresizingMaskIntoConstraints = true

    var session = TerminalSession(
      containerView: containerView,
      sessionId: command.sessionId,
      view: surfaceView,
      scrollView: scrollView,
      searchBarView: searchBarView,
      titleBarView: titleBarView,
      borderView: borderView,
      persistenceLabelView: persistenceLabelView,
      delayedSendLabelView: delayedSendLabelView,
      foregroundPid: nil,
      sessionPersistenceName: sessionPersistenceName,
      sessionPersistenceProvider: sessionPersistenceProvider,
      ttyName: nil)
    surfaceView.$title
      .removeDuplicates()
      .sink { [weak self] title in
        guard !title.isEmpty else { return }
        /**
         CDXC:NativeTerminals 2026-04-30-03:41
         Ghostty terminal/window titles are still forwarded to the sidebar for
         agent detection, but they must not directly replace the native pane
         title. The pane title comes from setActiveTerminalSet.sessionTitles so
         already-ellipsized OSC/window titles cannot poison AppKit chrome.
         */
        guard let self else { return }
        let sessionPersistenceName = self.updatePersistenceSessionForTerminalTitle(
          sessionId: command.sessionId,
          title: title
        )
        self.sendEvent(
          .terminalTitleChanged(
            sessionId: command.sessionId,
            title: title,
            sessionPersistenceName: sessionPersistenceName
          ))
      }
      .store(in: &session.cancellables)
    surfaceView.$pwd
      .removeDuplicates()
      .compactMap { $0 }
      .sink { [weak self] pwd in
        guard !pwd.isEmpty else { return }
        self?.sendEvent(.terminalCwdChanged(sessionId: command.sessionId, cwd: pwd))
      }
      .store(in: &session.cancellables)
    surfaceView.$bell
      .removeDuplicates()
      .sink { [weak self] didRing in
        if didRing {
          self?.sendEvent(.terminalBell(sessionId: command.sessionId))
        }
      }
      .store(in: &session.cancellables)
    surfaceView.$searchState
      .receive(on: DispatchQueue.main)
      .sink { [weak searchBarView] searchState in
        searchBarView?.setSearchState(searchState)
      }
      .store(in: &session.cancellables)
    surfaceView.$cellSize
      .removeDuplicates()
      .receive(on: DispatchQueue.main)
      .sink { [weak self] _ in
        /**
         CDXC:NativeTerminalResize 2026-04-29-07:29
         Cell-stepped embedded terminal layout depends on Ghostty's measured
         cell size. Relayout when the initial measurement arrives or font
         settings change so pane geometry stays aligned to terminal columns.
         */
        self?.needsLayout = true
      }
      .store(in: &session.cancellables)

    sessions[command.sessionId] = session
    mountTerminalPaneContainer(for: session)
    logFocusSurfaceState(
      event: "nativeFocusTrace.createTerminalSurfaceState",
      reason: "createTerminal.registered",
      details: [
        "activateOnCreate": activateOnCreate,
        "requestedSessionId": command.sessionId,
      ])
    if activateOnCreate {
      activeSessionIds.insert(command.sessionId)
      terminalLayout = terminalLayout ?? .leaf(sessionId: command.sessionId)
    } else {
      /**
       CDXC:CrashRootCause 2026-05-04-09:19
       Sidebar-created terminals are mounted inactive because the sidebar sends
       setActiveTerminalSet immediately after creation. This prevents rapid
       launches from transiently laying out and focusing both the previous and
       new Ghostty surfaces before the authoritative visible-session snapshot
       arrives.
       */
      moveOffscreen(scrollView)
      moveOffscreen(searchBarView)
      moveOffscreen(titleBarView)
      moveOffscreen(borderView)
      moveOffscreen(containerView)
      searchBarView.isHidden = true
      titleBarView.isHidden = true
      borderView.isHidden = true
    }
    needsLayout = true
    if activateOnCreate {
      focusTerminal(sessionId: command.sessionId, reason: "createTerminalNew")
    }

    let ttyName = surfaceView.surfaceModel?.ttyName
    let foregroundPid = surfaceView.surfaceModel?.foregroundPID
    sessions[command.sessionId]?.ttyName = ttyName
    sessions[command.sessionId]?.foregroundPid = foregroundPid
    sendEvent(
      .terminalReady(
        sessionId: command.sessionId,
        ttyName: ttyName,
        foregroundPid: foregroundPid,
        sessionPersistenceName: sessionPersistenceName,
        persistenceSessionCreated: sessionPersistenceProvider == nil
          ? nil
          : persistenceSessionExisted.map { !$0 }
      ))
    sendEvent(.terminalCwdChanged(sessionId: command.sessionId, cwd: command.cwd))
    startExitPollingIfNeeded()
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.createTerminal.completed",
      details: [
        "activateOnCreate": activateOnCreate,
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "diagnosticSource": command.diagnosticSource ?? "",
        "foregroundPid": foregroundPid ?? 0,
        "requestedSessionId": command.sessionId,
        "ttyName": ttyName ?? "",
        "visibleSessionIds": orderedVisibleSessionIds(),
      ],
      force: forcePreviousSessionRestoreDiagnostics)
  }

  private func updatePersistenceSessionForTerminalTitle(sessionId: String, title: String) -> String? {
    guard
      let provider = sessions[sessionId]?.sessionPersistenceProvider,
      let currentSessionName = sessions[sessionId]?.sessionPersistenceName
    else {
      return nil
    }
    guard provider == .tmux else {
      /**
       CDXC:SessionPersistence 2026-05-05-07:28
       zmx 0.4 exposes attach/run/list/kill/history but no rename command, and
       zellij session rename is an in-session action rather than a targetable
       external command. Keep non-tmux durable attach identities stable while
       the sidebar still follows Ghostty title events for user-visible card
       names.
       */
      return currentSessionName
    }
    let nextSessionName = NativeSessionPersistenceMode.sessionName(
      provider: provider,
      sessionId: sessionId,
      title: title)
    guard nextSessionName != currentSessionName else {
      return currentSessionName
    }
    /**
     CDXC:SessionPersistence 2026-05-05-07:28
     Agent CLIs reveal useful task names through terminal title changes after
     launch. tmux can rename the backing session from that live title so SSH
     users can find the same session by task-oriented name instead of opaque id.
     */
    sessions[sessionId]?.sessionPersistenceName = nextSessionName
    sessions[sessionId]?.persistenceLabelView.setProvider(provider, sessionName: nextSessionName)
    NativeSessionPersistenceMode.renameTmuxSession(
      from: currentSessionName,
      sessionId: sessionId,
      title: title,
      to: nextSessionName
    )
    return nextSessionName
  }

  func closeTerminal(sessionId: String, preservePersistenceSession: Bool = false) {
    closeTerminal(
      sessionId: sessionId,
      requestGhosttyClose: true,
      preservePersistenceSession: preservePersistenceSession,
      reason: "closeTerminal")
  }

  private func closeTerminal(
    sessionId: String,
    requestGhosttyClose: Bool,
    preservePersistenceSession: Bool = false,
    reason: String
  ) {
    guard let session = sessions.removeValue(forKey: sessionId) else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.closeTerminal.missing",
        details: [
          "reason": reason,
          "sessionId": sessionId,
        ])
      return
    }
    let processExited = session.view.processExited
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.closeTerminal.start",
      details: [
        "activeProjectEditorId": nullableString(activeProjectEditorId),
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "hasSurface": session.view.surface != nil,
        "processExited": processExited,
        "preservePersistenceSession": preservePersistenceSession,
        "reason": reason,
        "requestGhosttyClose": requestGhosttyClose,
        "sessionId": sessionId,
      ])
    activeSessionIds.remove(sessionId)
    sessionActivities.removeValue(forKey: sessionId)
    sessionAgentIconDataUrls.removeValue(forKey: sessionId)
    sessionDelayedSendRemainingLabels.removeValue(forKey: sessionId)
    sessionFaviconDataUrls.removeValue(forKey: sessionId)
    sessionTitleBarActions.removeValue(forKey: sessionId)
    sessionTitles.removeValue(forKey: sessionId)
    closePoppedOutPaneWindow(sessionId: sessionId, reason: "closeTerminal")
    resizeLogSignatureBySessionId.removeValue(forKey: sessionId)
    /**
     CDXC:TerminalExitCleanup 2026-05-06-01:01
     Exit polling runs after Ghostty has already marked the PTY surface exited.
     Do not send Ghostty a second close request for that surface; the log at
     2026-05-06 00:52:50 showed the app voluntarily terminating immediately
     after this path handled an already-exited split pane.
     */
    if requestGhosttyClose, !processExited, let surface = session.view.surface {
      ghostty.requestClose(surface: surface)
    }
    if requestGhosttyClose, !preservePersistenceSession {
      NativeSessionPersistenceMode.killSession(
        provider: session.sessionPersistenceProvider,
        sessionName: session.sessionPersistenceName,
        reason: reason,
        sessionId: sessionId)
    }
    NativeTerminalProcessMonitor.terminateSessionProcesses(
      ttyName: session.view.surfaceModel?.ttyName ?? session.ttyName,
      foregroundPid: session.view.surfaceModel?.foregroundPID ?? session.foregroundPid,
      reason: reason)
    session.containerView.removeFromSuperview()
    terminalLayout = prunedLayout(removing: sessionId, from: terminalLayout)
    attentionSessionIds.remove(sessionId)
    if focusedSessionId == sessionId {
      focusedSessionId = nil
    }
    needsLayout = true
    sendEvent(.terminalExited(sessionId: sessionId, exitCode: nil))
    stopExitPollingIfIdle()
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.closeTerminal.completed",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "focusedSessionId": nullableString(focusedSessionId),
        "processExited": processExited,
        "preservePersistenceSession": preservePersistenceSession,
        "reason": reason,
        "sessionId": sessionId,
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
  }

  /**
   CDXC:T3Code 2026-04-30-02:38
   T3 Code is a web pane in the reference workspace, not a terminal command.
   Native ghostex therefore mounts a WKWebView surface in the same pane layout so
   the T3 button embeds the app instead of typing `npx --yes t3` into Ghostty.
   */
  private static func normalizedBrowserFeedbackTool(_ value: String?) -> String {
    value == "react-grab" ? "react-grab" : "agentation"
  }

  func createWebPane(_ command: CreateWebPane) {
    let initialUrl = URL(string: command.url)
    let isManagedT3Pane = initialUrl.map(NativeT3RuntimeLauncher.isManagedRuntimeURL) ?? false
    let browserFeedbackTool = Self.normalizedBrowserFeedbackTool(command.browserFeedbackTool)
    if let existingSession = webPaneSessions[command.sessionId] {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.create.reused", [
        "sessionId": command.sessionId,
        "threadId": command.threadId ?? NSNull(),
        "url": command.url,
      ])
      existingSession.hostView.setBrowserFeedbackTool(browserFeedbackTool)
      if existingSession.isManagedT3Pane, isManagedT3Pane {
        webPaneSessions[command.sessionId] = WebPaneSession(
          browserTitleObservation: existingSession.browserTitleObservation,
          containerView: existingSession.containerView,
          chromiumView: existingSession.chromiumView,
          diagnosticsBridge: existingSession.diagnosticsBridge,
          hostView: existingSession.hostView,
          isManagedT3Pane: existingSession.isManagedT3Pane,
          projectId: command.projectId,
          sessionId: existingSession.sessionId,
          threadId: command.threadId,
          title: command.title,
          workspaceRoot: command.cwd ?? existingSession.workspaceRoot,
          browserProfileID: existingSession.browserProfileID,
          webView: existingSession.webView,
          titleBarView: existingSession.titleBarView,
          borderView: existingSession.borderView
        )
        existingSession.titleBarView.setTitle(
          normalizedTerminalSessionTitle(command.title, sessionId: command.sessionId))
        if let url = initialUrl {
          completedWebPaneLoadSessionIds.remove(command.sessionId)
          pendingAuthenticatedWebPaneLoadSessionIds.remove(command.sessionId)
          loadWebPane(sessionId: command.sessionId, url: url, reason: "createWebPaneExistingReroute")
        }
      }
      focusWebPane(sessionId: command.sessionId, reason: "createWebPaneExisting")
      return
    }

    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.create.start", [
      "sessionId": command.sessionId,
      "title": command.title,
      "url": command.url,
      "workspaceRoot": command.cwd ?? NSNull(),
    ])
    let browserProfileID = isManagedT3Pane ? nil : NativeBrowserProfileStore.shared.effectiveLastUsedProfileID
    let diagnosticsBridge = T3CodePaneDiagnosticsBridge(
      sessionId: command.sessionId,
      onThreadChanged: { [weak self] sessionId, threadId, title in
        self?.sendEvent(.t3ThreadChanged(sessionId: sessionId, threadId: threadId, title: title))
      })

    let webView: WKWebView?
    let chromiumView: GhostexCEFBrowserView?
    if isManagedT3Pane {
      let configuration = WKWebViewConfiguration()
      configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
      configuration.preferences.setValue(true, forKey: "developerExtrasEnabled")
      configuration.websiteDataStore = .default()
      configuration.userContentController.add(
        diagnosticsBridge,
        name: T3CodePaneDiagnosticsBridge.messageHandlerName
      )
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: Self.t3WebPaneDiagnosticsScript,
          injectionTime: .atDocumentStart,
          forMainFrameOnly: false
        ))
      configuration.userContentController.addUserScript(
        WKUserScript(
          source: Self.t3WebPaneBridgeScript(
            sessionId: command.sessionId, title: command.title, workspaceRoot: command.cwd),
          injectionTime: .atDocumentStart,
          forMainFrameOnly: true
        ))
      let managedWebView = WKWebView(frame: .zero, configuration: configuration)
      if #available(macOS 13.3, *) {
        managedWebView.isInspectable = true
      }
      managedWebView.translatesAutoresizingMaskIntoConstraints = true
      managedWebView.allowsBackForwardNavigationGestures = true
      managedWebView.navigationDelegate = self
      managedWebView.uiDelegate = self
      /**
       CDXC:T3Code 2026-04-30-19:17
       Native T3 panes must use WKWebView's default opaque drawing path. The
       accessibility tree can report a live T3 DOM even when transparent WebKit
       compositing only shows the workspace's gray backing layer, so do not make
       the embedded app transparent while debugging or rendering production panes.
       */
      managedWebView.wantsLayer = true
      managedWebView.layer?.masksToBounds = true
      managedWebView.underPageBackgroundColor = NSColor(calibratedRed: 0.086, green: 0.086, blue: 0.086, alpha: 1)
      webView = managedWebView
      chromiumView = nil
    } else {
      guard GhostexCEFIsRuntimeAvailable() else {
        /**
         CDXC:ChromiumBrowserPanes 2026-05-04-16:38
         Normal browser panes must render with Chromium. If the bundled CEF
         runtime is missing, fail visibly instead of opening a WebKit pane that
         looks like the requested workflow but uses the wrong engine.
         */
        sendEvent(
          .terminalError(
            sessionId: command.sessionId,
            message: "Chromium runtime is not bundled for browser panes"))
        return
      }
      let profileIdentifier = browserProfileID?.uuidString ?? "default"
      let browserView = GhostexCEFBrowserView(
        frame: .zero,
        initialURL: command.url,
        profileIdentifier: profileIdentifier)
      browserView.translatesAutoresizingMaskIntoConstraints = true
      webView = nil
      chromiumView = browserView
    }
    /**
     CDXC:BrowserPanes 2026-05-02-16:58
     Browser panes need in-pane navigation chrome like the embedded browser
     reference: back/forward/reload, a URL field, and browser tooling buttons.
     The chrome is owned by the native pane host, not an overlay, so the pane
     still participates in the same splitter layout as T3 Code and terminals.
     */
    let hostView = WebPaneHostView(
      browserView: chromiumView ?? webView!,
      chromiumView: chromiumView,
      webView: webView,
      showsBrowserToolbar: !isManagedT3Pane,
      initialAddress: command.url,
      browserFeedbackTool: browserFeedbackTool,
      onFocus: { [weak self] in
        self?.focusWebPane(sessionId: command.sessionId, reason: "browserToolbar")
      },
      onOpenDevTools: { [weak self] in
        self?.openBrowserDevTools(sessionId: command.sessionId)
      },
      onInjectFeedbackTool: { [weak self] in
        if browserFeedbackTool == "react-grab" {
          self?.injectBrowserReactGrab(sessionId: command.sessionId)
        } else {
          self?.injectBrowserAgentation(sessionId: command.sessionId)
        }
      },
      onShowProfilePicker: { [weak self] in
        self?.showBrowserProfilePicker(sessionId: command.sessionId)
      },
      onShowImportSettings: { [weak self] in
        self?.showBrowserImportSettings(sessionId: command.sessionId)
      }
    )
    hostView.translatesAutoresizingMaskIntoConstraints = false
    if let chromiumView {
      chromiumView.titleChangedHandler = { [weak self, weak hostView, weak chromiumView] title in
        self?.updateChromiumWebPaneMetadata(
          sessionId: command.sessionId,
          title: title,
          url: chromiumView?.currentURLString,
          reason: "chromiumTitleChanged")
        hostView?.refreshBrowserToolbar(reason: "chromiumTitleChanged")
      }
      chromiumView.urlChangedHandler = { [weak self, weak hostView, weak chromiumView] url in
        self?.updateChromiumWebPaneMetadata(
          sessionId: command.sessionId,
          title: chromiumView?.pageTitle,
          url: url,
          reason: "chromiumUrlChanged")
        hostView?.refreshBrowserToolbar(reason: "chromiumUrlChanged")
      }
      chromiumView.faviconURLChangedHandler = { [weak self] faviconURL in
        self?.updateChromiumWebPaneFavicon(
          sessionId: command.sessionId,
          faviconURL: URL(string: faviconURL),
          reason: "chromiumFaviconChanged")
      }
      chromiumView.navigationStateChangedHandler = { [weak hostView] _, _, _ in
        hostView?.refreshBrowserToolbar(reason: "chromiumNavigationStateChanged")
      }
      /**
       CDXC:BrowserFeedbackTools 2026-05-26-22:09:
       Agentation import/render failures happen asynchronously inside the page
       after CEF accepts the injected JavaScript. Normal browser panes therefore
       need the same console forwarding as project-editor CEF panes so the
       toolbar action can be diagnosed from app logs.
       */
      chromiumView.consoleMessageHandler = { [weak self, weak chromiumView] message, source, line in
        guard let self else {
          return
        }
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.cef.console", [
          "currentUrl": chromiumView?.currentURLString ?? NSNull(),
          "line": line,
          "message": message,
          "sessionId": command.sessionId,
          "source": source,
          "windowNumber": self.window?.windowNumber ?? NSNull(),
        ])
        if message.contains("[Ghostex Agentation]") || message.contains("Agentation") {
          NSLog("Browser CEF console [%@:%ld] %@", source, line, message)
        }
      }
    }

    let titleBarView = TerminalSessionTitleBarView(
      title: normalizedTerminalSessionTitle(command.title, sessionId: command.sessionId),
      actions: TerminalSessionTitleBarView.webPaneCreationActions
    )
    titleBarView.setDebugContext(ownerSessionId: command.sessionId, paneKind: "web")
    titleBarView.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
    titleBarView.translatesAutoresizingMaskIntoConstraints = false
    titleBarView.onMouseDown = { [weak self] event in
      self?.handlePaneTitleBarMouseDown(
        event,
        sessionId: command.sessionId,
        focusReason: "nativeWebTitleBarMouseDown")
    }
    titleBarView.onAction = { [weak self] action in
      guard let self else { return }
      self.focusWebPane(sessionId: command.sessionId, reason: "nativeWebTitleBarAction")
      self.applyOptimisticPanePopOutAction(
        sessionId: command.sessionId,
        action: action,
        reason: "nativeWebTitleBarAction")
      self.sendEvent(.terminalTitleBarAction(sessionId: command.sessionId, action: action))
    }
    titleBarView.onTabMouseDown = { [weak self] event, tabSessionId in
      self?.handlePaneTabMouseDown(event, sessionId: tabSessionId)
    }
    titleBarView.onTabMouseDragged = { [weak self] event, tabSessionId in
      self?.handlePaneTitleBarMouseDragged(event, sessionId: tabSessionId)
    }
    titleBarView.onTabMouseUp = { [weak self] event, tabSessionId in
      self?.handlePaneTabMouseUp(event, sessionId: tabSessionId)
    }
    titleBarView.onTabCloseRequested = { [weak self] tabSessionId, scope in
      self?.sendEvent(.paneTabCloseRequested(sessionId: tabSessionId, scope: scope))
    }
    titleBarView.onTabSleepRequested = { [weak self] tabSessionId, scope in
      self?.sendEvent(.paneTabSleepRequested(sessionId: tabSessionId, scope: scope))
    }
    titleBarView.onTabFocusRequested = { [weak self] tabSessionId in
      self?.sendEvent(.paneTabFocusRequested(sessionId: tabSessionId))
    }
    titleBarView.onTabActionRequested = { [weak self] tabSessionId, action in
      self?.handlePaneTabActionRequested(sessionId: tabSessionId, action: action)
    }
    let borderView = TerminalPaneBorderView()
    borderView.translatesAutoresizingMaskIntoConstraints = false
    let containerView = TerminalPaneLeafContainerView()
    containerView.translatesAutoresizingMaskIntoConstraints = true
    /**
     CDXC:BrowserPanes 2026-05-03-01:58
     Browser panes should name native chrome from the loaded page, not the
     launch URL or localhost wrapper. Observe WKWebView title changes and feed
     them through the existing session-title sync path so later layout syncs do
     not overwrite the AppKit title bar with the initial browser card title.
     */
    let browserTitleObservation: NSKeyValueObservation? = nil

    webPaneSessions[command.sessionId] = WebPaneSession(
      browserTitleObservation: browserTitleObservation,
      containerView: containerView,
      chromiumView: chromiumView,
      diagnosticsBridge: diagnosticsBridge,
      hostView: hostView,
      isManagedT3Pane: isManagedT3Pane,
      projectId: command.projectId,
      sessionId: command.sessionId,
      threadId: command.threadId,
      title: command.title,
      workspaceRoot: command.cwd,
      browserProfileID: browserProfileID,
      webView: webView,
      titleBarView: titleBarView,
      borderView: borderView
    )
    activeSessionIds.insert(command.sessionId)
    if let session = webPaneSessions[command.sessionId] {
      mountWebPaneContainer(for: session)
      orderWebPaneViewsToFront(session)
    }
    terminalLayout = terminalLayout ?? .leaf(sessionId: command.sessionId)

    if let url = initialUrl {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.requested", [
        "isManagedT3Pane": isManagedT3Pane,
        "sessionId": command.sessionId,
        "url": url.absoluteString,
        "workspaceRoot": command.cwd ?? NSNull(),
      ])
      loadWebPaneStatus(
        sessionId: command.sessionId,
        title: command.title,
        message: isManagedT3Pane ? "Loading T3 Code…" : "Loading Browser…",
        caption: isManagedT3Pane ? "Preparing the embedded workspace" : url.absoluteString,
        loading: true,
        reason: "createWebPane")
      loadWebPane(sessionId: command.sessionId, url: url, reason: "initial")
    } else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.invalidUrl", [
        "sessionId": command.sessionId,
        "url": command.url,
      ])
    }

    needsLayout = true
    scheduleDeferredWebPaneLayout(sessionId: command.sessionId, reason: "createWebPaneNew")
    focusWebPane(sessionId: command.sessionId, reason: "createWebPaneNew")
  }

  func closeWebPane(sessionId: String) {
    guard let session = webPaneSessions.removeValue(forKey: sessionId) else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.close.missing", [
        "sessionId": sessionId,
      ])
      return
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.close.start", [
      "currentUrl": session.currentURLString ?? NSNull(),
      "sessionId": sessionId,
    ])
    activeSessionIds.remove(sessionId)
    sessionActivities.removeValue(forKey: sessionId)
    sessionAgentIconDataUrls.removeValue(forKey: sessionId)
    sessionDelayedSendRemainingLabels.removeValue(forKey: sessionId)
    sessionFaviconDataUrls.removeValue(forKey: sessionId)
    sessionTitleBarActions.removeValue(forKey: sessionId)
    sessionTitles.removeValue(forKey: sessionId)
    closePoppedOutPaneWindow(sessionId: sessionId, reason: "closeWebPane")
    completedWebPaneLoadSessionIds.remove(sessionId)
    pendingAuthenticatedWebPaneLoadSessionIds.remove(sessionId)
    t3ThreadRouteRetryAttemptsBySessionId.removeValue(forKey: sessionId)
    webPaneFaviconTasksBySessionId.removeValue(forKey: sessionId)?.cancel()
    if let webView = session.webView {
      webView.navigationDelegate = nil
      webView.uiDelegate = nil
      webView.configuration.userContentController.removeScriptMessageHandler(
        forName: T3CodePaneDiagnosticsBridge.messageHandlerName
      )
      webView.stopLoading()
    }
    /**
     CDXC:ChromiumBrowserPanes 2026-05-07-07:31
     Middle-clicking a browser card must close only that sidebar pane. The
     2026-05-07 07:08 log showed pane close entering Chromium teardown and then
     closing the top-level app window, so keep explicit before/after logs around
     the embedded browser close request.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.close.browserTeardown.start", [
      "hasChromiumView": session.chromiumView != nil,
      "hasWebView": session.webView != nil,
      "sessionId": sessionId,
    ])
    session.chromiumView?.closeBrowser()
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.close.browserTeardown.completed", [
      "sessionId": sessionId,
    ])
    session.containerView.removeFromSuperview()
    terminalLayout = prunedLayout(removing: sessionId, from: terminalLayout)
    attentionSessionIds.remove(sessionId)
    if focusedSessionId == sessionId {
      focusedSessionId = nil
    }
    needsLayout = true
    sendEvent(.terminalExited(sessionId: sessionId, exitCode: nil))
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.close.completed", [
      "activeSessionIds": Array(activeSessionIds).sorted(),
      "focusedSessionId": nullableString(focusedSessionId),
      "sessionId": sessionId,
      "visibleSessionIds": orderedVisibleSessionIds(),
    ])
  }

  func focusWebPane(sessionId: String, reason: String = "explicitFocusWebPaneCommand") {
    guard let session = webPaneSessions[sessionId] else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.focus.missing", [
        "knownSessionIds": Array(webPaneSessions.keys).sorted(),
        "reason": reason,
        "sessionId": sessionId,
      ])
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.focusWebPane.missingSession",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "knownSessionIds": Array(webPaneSessions.keys).sorted(),
          "reason": reason,
          "requestedSessionId": sessionId,
      ])
      return
    }
    if activateProjectEditorCompanionPane(sessionId: sessionId, focus: true, reason: reason) {
      return
    }
    let view = session.browserContentView
    let hadActiveProjectEditor = activeProjectEditorId != nil
    activeProjectEditorId = nil
    if hadActiveProjectEditor {
      needsLayout = true
      layoutSubtreeIfNeeded()
    }
    focusedSessionId = sessionId
    orderWebPaneViewsToFront(session)
    updateAllTerminalBorders()
    if let controller = poppedOutPaneControllers[sessionId] {
      controller.window?.makeKeyAndOrderFront(nil)
      _ = controller.window?.makeFirstResponder(view)
    } else {
      _ = window?.makeFirstResponder(view)
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.focus.applied", [
      "currentUrl": session.currentURLString ?? NSNull(),
      "reason": reason,
      "sessionId": sessionId,
    ])
    sendEvent(.terminalFocused(sessionId: sessionId))
  }

  func createProjectEditorPane(_ command: CreateProjectEditorPane) {
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.create.received", [
      "projectId": command.projectId,
      "title": command.title,
      "url": command.url,
    ])
    applyProjectEditorCompanionPaneHiddenPreference(
      command.companionPaneHidden,
      reason: "createProjectEditorPane")
    let browserFeedbackTool = Self.normalizedBrowserFeedbackTool(command.browserFeedbackTool)
    if let existingSession = projectEditorPaneSessions[command.projectId] {
      var nextSession = existingSession
      for tab in nextSession.tabs {
        tab.hostView.setBrowserFeedbackTool(browserFeedbackTool)
      }
      nextSession = updateProjectEditorSessionActiveTab(
        nextSession,
        url: command.url,
        title: projectEditorTabTitle(for: command.url, fallback: command.title))
      nextSession = projectEditorSession(nextSession, activating: nextSession.activeTabId)
      nextSession.title = command.title
      nextSession.url = command.url
      projectEditorPaneSessions[command.projectId] = nextSession
      nextSession.titleBarView?.setTitle(nextSession.projectTitle)
      if existingSession.url != command.url {
        loadProjectEditorPaneWhenReady(
          projectId: command.projectId, url: command.url, reason: "createProjectEditorPaneReroute")
      }
      focusProjectEditorPane(projectId: command.projectId, reason: "createProjectEditorPaneExisting")
      return
    }

    let requestedMode = command.mode ?? projectEditorModeFromNativeEditorId(command.projectId) ?? "code"
    guard requestedMode == "tasks" || GhostexCEFIsRuntimeAvailable() else {
      /**
       CDXC:EditorPanes 2026-05-06-14:21
       Project editors must embed code-server through Chromium without browser
       chrome. If CEF is unavailable, fail visibly instead of creating a WebKit
       substitute that would have different VS Code rendering and websocket
       behavior.

       CDXC:ProjectBoard 2026-05-23-03:16:
       Project mode is the exception to the Chromium requirement because it is
       now a first-party bundled board backed by WKWebView. Keep the CEF guard
       scoped to Code and Git so Project can open even when Chromium is not
       available.
       */
      sendEvent(
        .terminalError(
          sessionId: "project-editor-\(command.projectId)",
          message: "Chromium runtime is not bundled for editor panes"))
      sendEvent(
        .projectEditorLoadState(
          projectId: command.projectId,
          status: "error",
          message: "Chromium runtime is not bundled for editor panes"))
      return
    }

    let titleBarView: TerminalSessionTitleBarView?
    if command.showsProjectTabs == true {
      let view = TerminalSessionTitleBarView(title: command.projectTitle ?? command.title, actions: [])
      view.setDebugContext(ownerSessionId: command.projectId, paneKind: "projectEditorGit")
      view.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
      view.setShowsTabAddButton(true)
      view.setAllowsTabClosing(true)
      view.onAction = { [weak self] action in
        guard action == .newTerminal else { return }
        self?.addProjectEditorGitTab(
          projectId: command.projectId,
          url: command.url,
          title: "GitHub",
          reason: "projectEditorGitTabAddButton")
      }
      view.onTabMouseUp = { [weak self] _, selectedTabId in
        self?.selectProjectEditorTab(projectId: command.projectId, tabId: selectedTabId)
      }
      view.onTabCloseRequested = { [weak self] tabId, _ in
        self?.closeProjectEditorGitTab(projectId: command.projectId, tabId: tabId)
      }
      view.translatesAutoresizingMaskIntoConstraints = true
      titleBarView = view
    } else {
      titleBarView = nil
    }
    let initialTab = makeProjectEditorBrowserTab(
      projectId: command.projectId,
      tabId: createProjectEditorGitTabId(),
      title: projectEditorTabTitle(for: command.url, fallback: command.title),
      url: command.url,
      browserFeedbackTool: browserFeedbackTool,
      showsBrowserToolbar: command.showsBrowserToolbar ?? false,
      showsInitialLoadingOverlay: true,
      reason: "createProjectEditorPaneNew")
    projectEditorPaneSessions[command.projectId] = ProjectEditorPaneSession(
      activeTabId: initialTab.tabId,
      chromiumView: initialTab.chromiumView,
      hostView: initialTab.hostView,
      mode: requestedMode,
      projectId: command.projectId,
      projectTitle: command.projectTitle ?? command.title,
      showsProjectTabs: command.showsProjectTabs ?? false,
      tabs: [initialTab],
      titleBarView: titleBarView,
      webView: initialTab.webView,
      title: command.title,
      url: command.url
    )
    addSubview(initialTab.hostView)
    if let titleBarView {
      addSubview(titleBarView)
      moveOffscreen(titleBarView)
    }
    moveOffscreen(initialTab.hostView)
    syncProjectEditorTabBars()
    loadProjectEditorPaneWhenReady(
      projectId: command.projectId, url: command.url, reason: "createProjectEditorPaneNew")
    focusProjectEditorPane(projectId: command.projectId, reason: "createProjectEditorPaneNew")
  }

  private func makeProjectEditorBrowserTab(
    projectId: String,
    tabId: String,
    title: String,
    url: String,
    browserFeedbackTool: String,
    showsBrowserToolbar: Bool,
    showsInitialLoadingOverlay: Bool,
    reason: String
  ) -> ProjectEditorBrowserTab {
    /**
     CDXC:EditorPanes 2026-05-07-07:53
     Embedded VS Code panel positions must survive app restarts without making
     code-server boot in a fresh browser profile. The VS Code workbench stores
     layout in browser-side origin storage, so project editor CEF views use the
     persistent default Chromium profile; project ownership stays in the native
     editor session and code-server folder URL, not in a separate CEF profile.

     CDXC:GitProjectTabs 2026-05-16-10:32:
     Git project tabs need stable browser state when switching tabs. Each Git
     tab owns its own CEF view and WebPaneHostView, while the project-editor
     session points at whichever tab is active for existing layout, focus, and
     toolbar commands.
     */
    let useWebKitProjectView = projectEditorModeFromNativeEditorId(projectId) == "tasks"
    let chromiumView: GhostexCEFBrowserView?
    let webView: WKWebView?
    let browserView: NSView
    if useWebKitProjectView {
      /**
       CDXC:ProjectBoard 2026-05-23-03:00:
       The Project mode board is a first-party local React app and should use WKWebView, not the Chromium/CEF browser path used by Code and Git.
       WebKit gives the board a direct native message handler for whitelisted bd CLI calls while preserving the project-editor companion layout.
       */
      let configuration = WKWebViewConfiguration()
      configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
      configuration.preferences.setValue(true, forKey: "developerExtrasEnabled")
      configuration.websiteDataStore = .default()
      let beadsBridge = ProjectBeadsBridge { [weak self] request, webView in
        self?.handleProjectBeadsBridgeRequest(request, webView: webView)
      }
      let projectBoardBridge = ProjectBoardBridge { [weak self] request in
        self?.sendEvent(.projectBoardRequest(request))
      }
      configuration.userContentController.add(beadsBridge, name: ProjectBeadsBridge.messageHandlerName)
      configuration.userContentController.add(
        projectBoardBridge,
        name: ProjectBoardBridge.messageHandlerName)
      let projectWebView = WKWebView(frame: .zero, configuration: configuration)
      beadsBridge.webView = projectWebView
      if #available(macOS 13.3, *) {
        projectWebView.isInspectable = true
      }
      projectWebView.translatesAutoresizingMaskIntoConstraints = true
      projectWebView.allowsBackForwardNavigationGestures = false
      projectWebView.navigationDelegate = self
      projectWebView.uiDelegate = self
      projectWebView.wantsLayer = true
      projectWebView.layer?.masksToBounds = true
      projectWebView.underPageBackgroundColor = NSColor(
        calibratedRed: 0.063,
        green: 0.067,
        blue: 0.071,
        alpha: 1)
      chromiumView = nil
      webView = projectWebView
      browserView = projectWebView
    } else {
      let projectChromiumView = GhostexCEFBrowserView(
        frame: .zero,
        initialURL: "about:blank",
        profileIdentifier: "default")
      projectChromiumView.trustedClipboardOrigin = NativeCodeServerRuntimeLauncher.origin
      projectChromiumView.translatesAutoresizingMaskIntoConstraints = true
      chromiumView = projectChromiumView
      webView = nil
      browserView = projectChromiumView
    }
    let hostView = WebPaneHostView(
      browserView: browserView,
      chromiumView: chromiumView,
      webView: webView,
      showsBrowserToolbar: showsBrowserToolbar,
      showsInitialLoadingOverlay: showsInitialLoadingOverlay,
      initialAddress: url,
      browserFeedbackTool: browserFeedbackTool,
      onFocus: { [weak self] in
        self?.focusProjectEditorPaneFromUserInteraction(
          projectId: projectId,
          reason: "projectEditorHostFocus")
      },
      onOpenDevTools: { [weak self] in
        self?.openProjectEditorDevTools(projectId: projectId)
      },
      onInjectFeedbackTool: { [weak self] in
        if browserFeedbackTool == "react-grab" {
          self?.injectProjectEditorReactGrab(projectId: projectId)
        } else {
          self?.injectProjectEditorAgentation(projectId: projectId)
        }
      },
      onShowProfilePicker: { [weak self] in
        self?.showProjectEditorProfilePicker(projectId: projectId)
      },
      onShowImportSettings: { [weak self] in
        self?.showProjectEditorImportSettings(projectId: projectId)
      }
    )
    hostView.translatesAutoresizingMaskIntoConstraints = true
    if let chromiumView {
      configureProjectEditorChromiumCallbacks(chromiumView, projectId: projectId, reason: reason)
    }
    return ProjectEditorBrowserTab(
      tabId: tabId,
      chromiumView: chromiumView,
      hostView: hostView,
      webView: webView,
      title: title,
      url: url)
  }

  func focusProjectEditorPane(
    projectId: String,
    reason: String = "explicitFocusProjectEditorPaneCommand"
  ) {
    guard let session = projectEditorPaneSessions[projectId] else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.focus.missing", [
        "knownProjectIds": Array(projectEditorPaneSessions.keys).sorted(),
        "projectId": projectId,
        "reason": reason,
      ])
      return
    }
    /**
     CDXC:EditorPanes 2026-05-06-14:21
     Editor panes are full-workspace project surfaces. Focusing one hides every
     terminal/browser split view and brings the project's Chromium code-server
     view forward without changing the saved terminal split layout.
     CDXC:ProjectEditorCompanion 2026-05-14-09:19:
     The editor is no longer a completely empty workarea takeover. When a project
     editor becomes active, keep the current sidebar-focused terminal or T3 Code
     session mounted in the left companion pane unless the user already closed
     that companion for this editor visit.
     CDXC:ModeSwitcher 2026-05-15-14:42:
     Code, Git, and tasks-backed Project modes use separate mode-scoped
     project-editor pane IDs for the same project. Focusing one pane must hide
     every other project-editor host immediately so mode switches keep each CEF
     tab alive without showing stale content from the previously active mode.
   */
    let didSwitchProjectEditor = activeProjectEditorId != projectId
    let currentCompanionLayout = projectEditorCompanionLayout(in: bounds)
    let currentEditorFrame = currentCompanionLayout?.editorFrame ?? bounds
    let currentProjectEditorFrames = projectEditorPaneFrames(session, in: currentEditorFrame)
    let companionStateSettled = projectEditorCompanionPaneHidden || projectEditorCompanionSessionId != nil
    let activeHostSettled =
      session.hostView.superview === self
      && !session.hostView.isHidden
      && rectsMatch(session.hostView.frame, currentProjectEditorFrames.hostFrame)
    let currentTitleBarShouldBeHidden = currentProjectEditorFrames.titleBarFrame.height <= 0
    let activeTitleBarSettled =
      session.titleBarView == nil
      || (
        session.titleBarView?.superview === self
        && session.titleBarView?.isHidden == currentTitleBarShouldBeHidden
        && rectsMatch(session.titleBarView?.frame ?? .zero, currentProjectEditorFrames.titleBarFrame)
      )
    /*
     CDXC:ChromiumBrowserPanes 2026-05-16-22:37:
     Disabled browser-toolbar clicks call the project-editor focus path even though the same editor is already visible.
     Do not re-run host visibility, ordering, companion sync, AppKit layout, or CEF first-responder work when the active project editor is already settled; redundant focus layouts can cause Chromium's internal compositor layer to drift while native frames stay fixed.
     */
    if !didSwitchProjectEditor && companionStateSettled && activeHostSettled && activeTitleBarSettled {
      return
    }
    activeProjectEditorId = projectId
    if projectEditorCompanionPaneHidden {
      projectEditorCompanionIsVisible = false
      projectEditorCompanionResizeDrag = nil
    } else if didSwitchProjectEditor || projectEditorCompanionSessionId == nil {
      openDefaultProjectEditorCompanionPane(reason: reason)
    }
    hideSplitSessionSurfacesForActiveEditor()
    let companionLayout = projectEditorCompanionLayout(in: bounds)
    projectEditorCompanionResizeWorkspaceBounds = bounds
    for otherSession in projectEditorPaneSessions.values where otherSession.projectId != projectId {
      setProjectEditorTabHostVisibility(otherSession, isActive: false)
      if let titleBarView = otherSession.titleBarView {
        titleBarView.isHidden = true
        moveOffscreen(titleBarView)
      }
    }
    setProjectEditorTabHostVisibility(session, isActive: true)
    session.titleBarView?.isHidden = false
    syncProjectEditorTabBars()
    layoutProjectEditorPane(session, in: companionLayout?.editorFrame ?? bounds)
    orderProjectEditorPaneToFront(session)
    syncProjectEditorCompanionPane(layout: companionLayout)
    if didSwitchProjectEditor {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:03:
       Switching from Agents into Code, Git, or Project mode surfaces the current companion terminal without always taking the ordinary terminal-focus path.
       Refresh the zmx terminal pane that is actually visible after the editor and companion frames are applied.
       */
      refreshZmxPersistenceTerminalsForSurfacedPanes(reason: "focusProjectEditorPane.modeSwitch")
    }
    /**
     CDXC:EditorPanes 2026-05-13-23:13
     VS Code drag/drop inside the embedded CEF editor depends on ghostex's native
     drag hover/release bridge being armed as soon as the project editor becomes
     the visible interaction surface. Focus can happen before the next AppKit
     layout pass, so install the CEF drag monitor directly after the Chromium
     host is visible and ordered.
    */
    syncCEFNativeDragSourceReleaseMonitor(reason: "focusProjectEditorPane")
    _ = window?.makeFirstResponder(session.chromiumView ?? session.webView ?? session.hostView)
    needsLayout = true
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.focus.applied", [
      "projectId": projectId,
      "reason": reason,
      "title": session.title,
      "url": session.url,
    ])
  }

  private func syncProjectEditorTabBars() {
    for session in projectEditorPaneSessions.values where session.mode == "git" && session.showsProjectTabs {
      let items = session.tabs.map { tab in
        TerminalSessionTitleBarView.TabItem(
          actions: [],
          isSleeping: false,
          sessionId: tab.tabId,
          title: tab.title)
      }
      session.titleBarView?.setTabs(items, activeSessionId: session.activeTabId)
      session.titleBarView?.setFocusedPane(session.projectId == activeProjectEditorId)
    }
  }

  private func projectEditorSession(
    _ session: ProjectEditorPaneSession,
    activating tabId: String
  ) -> ProjectEditorPaneSession {
    guard let tab = session.tabs.first(where: { $0.tabId == tabId }) else {
      return session
    }
    var nextSession = session
    nextSession.activeTabId = tab.tabId
    nextSession.chromiumView = tab.chromiumView
    nextSession.hostView = tab.hostView
    nextSession.webView = tab.webView
    nextSession.title = tab.title
    nextSession.url = tab.url
    return nextSession
  }

  private func projectEditorHostViews(_ session: ProjectEditorPaneSession) -> [WebPaneHostView] {
    var seen = Set<ObjectIdentifier>()
    var hostViews: [WebPaneHostView] = []
    for tab in session.tabs {
      let identifier = ObjectIdentifier(tab.hostView)
      if !seen.contains(identifier) {
        seen.insert(identifier)
        hostViews.append(tab.hostView)
      }
    }
    if !seen.contains(ObjectIdentifier(session.hostView)) {
      hostViews.append(session.hostView)
    }
    return hostViews
  }

  private func setProjectEditorTabHostVisibility(_ session: ProjectEditorPaneSession, isActive: Bool) {
    for hostView in projectEditorHostViews(session) {
      let isActiveHost = isActive && hostView === session.hostView
      hostView.isHidden = !isActiveHost
      if !isActiveHost {
        moveOffscreen(hostView)
      }
    }
  }

  private func projectEditorHostView(
    projectId: String,
    chromiumView: GhostexCEFBrowserView
  ) -> WebPaneHostView? {
    guard let session = projectEditorPaneSessions[projectId] else {
      return nil
    }
    return session.tabs.first(where: { tab in
      guard let tabChromiumView = tab.chromiumView else {
        return false
      }
      return tabChromiumView === chromiumView
    })?.hostView
      ?? (session.chromiumView.map { $0 === chromiumView } == true ? session.hostView : nil)
  }

  private func focusProjectEditorPaneFromUserInteraction(projectId: String, reason: String) {
    focusProjectEditorPane(projectId: projectId, reason: reason)
    guard projectEditorPaneSessions[projectId]?.mode == "git" else {
      return
    }
    /**
     CDXC:GitProjectTabs 2026-05-16-09:50:
     Git browser chrome focus is a user-visible project-mode selection. Report
     toolbar-originated focus, including Back button clicks and address-field
     edits, to the sidebar before React sends another layout command so stale
     Code-mode state cannot bring the Code CEF pane forward.
     */
    sendProjectEditorTabSelected(projectId: projectId)
  }

  private func sendProjectEditorTabSelected(projectId: String) {
    sendEvent(
      .projectEditorTabSelected(
        projectId: projectId,
        url: activeProjectEditorTabURL(projectId: projectId)))
  }

  private func activeProjectEditorTabURL(projectId: String) -> String? {
    guard let session = projectEditorPaneSessions[projectId] else {
      return nil
    }
    if let activeTab = session.tabs.first(where: { $0.tabId == session.activeTabId }) {
      return activeTab.url
    }
    return session.chromiumView?.currentURLString ?? session.webView?.url?.absoluteString ?? session.url
  }

  func titlebarBrowserResourceTabs() -> [[String: Any]] {
    /**
     CDXC:TitlebarResources 2026-05-17-01:25:
     The Resources dropdown should explain browser memory like terminal memory:
     show the visible Browser tab or project editor view first, then nest the
     Chromium processes below it. Export CEF browser identifiers with the
     tracked title and URL so the React titlebar can correlate renderer process
     `--renderer-client-id` values without showing implementation labels to users.
     */
    var tabs: [[String: Any]] = []
    for session in webPaneSessions.values {
      guard let chromiumView = session.chromiumView, chromiumView.browserIdentifier >= 0 else {
        continue
      }
      let currentURL = chromiumView.currentURLString ?? session.currentURLString ?? ""
      let title = chromiumWebPaneDisplayTitle(
        title: chromiumView.pageTitle,
        url: currentURL,
        fallbackTitle: session.title)
      tabs.append([
        "browserId": chromiumView.browserIdentifier,
        "id": "browser:\(session.sessionId)",
        "isActive": orderedVisibleSessionIds().contains(session.sessionId),
        "kind": "browser",
        "sessionId": session.sessionId,
        "title": title,
        "url": currentURL,
      ])
    }
    for session in projectEditorPaneSessions.values {
      for tab in session.tabs {
        guard let chromiumView = tab.chromiumView, chromiumView.browserIdentifier >= 0 else {
          continue
        }
        let currentURL = chromiumView.currentURLString ?? tab.url
        tabs.append([
          "browserId": chromiumView.browserIdentifier,
          "id": "project-editor:\(session.projectId):\(tab.tabId)",
          "isActive": activeProjectEditorId == session.projectId && session.activeTabId == tab.tabId,
          "kind": session.mode,
          "projectId": session.projectId,
          "title": titlebarBrowserResourceTitle(mode: session.mode, title: tab.title),
          "url": currentURL,
        ])
      }
    }
    return tabs
  }

  private func titlebarBrowserResourceTitle(mode: String, title: String) -> String {
    let trimmedTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
    let visibleTitle = trimmedTitle.isEmpty ? "Browser tab" : trimmedTitle
    switch mode {
    case "code":
      return "Code - \(visibleTitle)"
    case "git":
      return "Git - \(visibleTitle)"
    case "tasks":
      return "Project - \(visibleTitle)"
    default:
      return visibleTitle
    }
  }

  private func selectProjectEditorTab(projectId: String, tabId: String) {
    guard let session = projectEditorPaneSessions[projectId],
      let tab = session.tabs.first(where: { $0.tabId == tabId })
    else {
      return
    }
    /**
     CDXC:GitProjectTabs 2026-05-16-10:32:
     Git mode uses project-local browser tabs inside the Git view, not one
     global project tab row. Selecting a tab must switch to that tab's existing
     CEF host instead of navigating the active host to the tab URL, otherwise
     tab switching reloads GitHub pages and loses scroll/input state.
     */
    let nextSession = projectEditorSession(session, activating: tab.tabId)
    projectEditorPaneSessions[projectId] = nextSession
    setProjectEditorTabHostVisibility(nextSession, isActive: activeProjectEditorId == projectId)
    nextSession.hostView.refreshBrowserToolbar(reason: "projectEditorGitTabSelected")
    syncProjectEditorTabBars()
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorGitTabSelected")
    sendProjectEditorTabSelected(projectId: projectId)
  }

  private func addProjectEditorGitTab(
    projectId: String,
    url: String,
    title: String? = nil,
    reason: String
  ) {
    guard let session = projectEditorPaneSessions[projectId] else {
      return
    }
    let tab = makeProjectEditorBrowserTab(
      projectId: projectId,
      tabId: createProjectEditorGitTabId(),
      title: title ?? projectEditorTabTitle(for: url, fallback: "GitHub"),
      url: url,
      browserFeedbackTool: session.hostView.browserFeedbackToolRawValue,
      showsBrowserToolbar: true,
      showsInitialLoadingOverlay: true,
      reason: reason)
    var nextSession = session
    nextSession.tabs.append(tab)
    nextSession = projectEditorSession(nextSession, activating: tab.tabId)
    projectEditorPaneSessions[projectId] = nextSession
    addSubview(tab.hostView)
    moveOffscreen(tab.hostView)
    tab.chromiumView?.loadURLString(tab.url)
    tab.hostView.refreshHostedWebView(reason: reason)
    setProjectEditorTabHostVisibility(nextSession, isActive: activeProjectEditorId == projectId)
    syncProjectEditorTabBars()
    focusProjectEditorPane(projectId: projectId, reason: reason)
    sendProjectEditorTabSelected(projectId: projectId)
  }

  private func closeProjectEditorGitTab(projectId: String, tabId: String) {
    guard let session = projectEditorPaneSessions[projectId], session.tabs.count > 1 else {
      return
    }
    let removedIndex = session.tabs.firstIndex(where: { $0.tabId == tabId })
    let removedTab = session.tabs.first(where: { $0.tabId == tabId })
    let nextTabs = session.tabs.filter { $0.tabId != tabId }
    guard !nextTabs.isEmpty else {
      return
    }
    let nextActiveTab: ProjectEditorBrowserTab
    if session.activeTabId == tabId {
      let nextIndex = min(removedIndex ?? 0, nextTabs.count - 1)
      nextActiveTab = nextTabs[nextIndex]
    } else {
      nextActiveTab = nextTabs.first(where: { $0.tabId == session.activeTabId }) ?? nextTabs[0]
    }
    var nextSession = session
    nextSession.tabs = nextTabs
    nextSession = projectEditorSession(nextSession, activating: nextActiveTab.tabId)
    projectEditorPaneSessions[projectId] = nextSession
    if let removedTab {
      removedTab.chromiumView?.closeBrowser()
      removedTab.hostView.removeFromSuperview()
    }
    setProjectEditorTabHostVisibility(nextSession, isActive: activeProjectEditorId == projectId)
    syncProjectEditorTabBars()
    if session.activeTabId == tabId {
      sendProjectEditorTabSelected(projectId: projectId)
    }
  }

  private func createProjectEditorGitTabId() -> String {
    "git-tab-\(UUID().uuidString)"
  }

  private func projectEditorTabTitle(for url: String, fallback: String) -> String {
    guard let parsed = URL(string: url) else {
      return fallback
    }
    let lastPath = parsed.pathComponents.last?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if !lastPath.isEmpty && lastPath != "/" {
      return lastPath
    }
    return parsed.host ?? fallback
  }

  private func updateProjectEditorSessionActiveTab(
    _ session: ProjectEditorPaneSession,
    url: String,
    title: String? = nil
  ) -> ProjectEditorPaneSession {
    var nextSession = session
    let normalizedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
    nextSession.tabs = nextSession.tabs.map { tab in
      guard tab.tabId == nextSession.activeTabId else {
        return tab
      }
      return ProjectEditorBrowserTab(
        tabId: tab.tabId,
        chromiumView: tab.chromiumView,
        hostView: tab.hostView,
        webView: tab.webView,
        title: normalizedTitle?.isEmpty == false
          ? normalizedTitle!
          : projectEditorTabTitle(for: url, fallback: tab.title),
        url: url)
    }
    nextSession = projectEditorSession(nextSession, activating: nextSession.activeTabId)
    return nextSession
  }

  func closeProjectEditorPane(projectId: String) {
    guard let session = projectEditorPaneSessions.removeValue(forKey: projectId) else {
      return
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.close", [
      "projectId": projectId,
      "url": session.url,
    ])
    for tab in session.tabs {
      tab.chromiumView?.closeBrowser()
      tab.hostView.removeFromSuperview()
    }
    if session.tabs.isEmpty {
      session.chromiumView?.closeBrowser()
      session.hostView.removeFromSuperview()
    }
    session.titleBarView?.removeFromSuperview()
    syncProjectEditorTabBars()
    if activeProjectEditorId == projectId {
      activeProjectEditorId = nil
      projectEditorCompanionSessionId = nil
      projectEditorCompanionIsVisible = false
      projectEditorCompanionResizeDrag = nil
      needsLayout = true
    }
    syncCEFNativeDragSourceReleaseMonitor(reason: "closeProjectEditorPane")
  }

  func closeFocusedSession(reason: String) -> Bool {
    /**
     CDXC:PaneClose 2026-05-10-11:56
     Cmd-W must close the user's focused workspace surface, not the native app
     window. Prefer AppKit's current responder so embedded Chrome/Ghostty focus
     wins over stale sidebar state, then fall back to the last focused session id.

     CDXC:PaneClose 2026-05-23-10:03:
     Cmd-W is a session-removal command, not a local native-surface disposal.
     Route focused terminal/web pane closes through the sidebar owner so the
     sidebar card, workspace layout, Ghostty surface, and provider-backed zmx/tmux
     runtime are removed together by the normal close path.
     */
    if let activeProjectEditorId, projectEditorPaneSessions[activeProjectEditorId] != nil {
      closeProjectEditorPane(projectId: activeProjectEditorId)
      return true
    }

    let candidates = [currentResponderSessionId(), focusedSessionId].compactMap { $0 }
    guard let sessionId = candidates.first(where: { activeSessionIds.contains($0) }) else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.closeFocusedSession.missingFocus",
        details: [
          "activeProjectEditorId": nullableString(activeProjectEditorId),
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "focusedSessionId": nullableString(focusedSessionId),
          "reason": reason,
          "responder": responderSnapshot(),
        ])
      return false
    }

    if sessions[sessionId] != nil {
      sendEvent(.paneTabCloseRequested(sessionId: sessionId, scope: .close))
      return true
    }
    if webPaneSessions[sessionId] != nil {
      sendEvent(.paneTabCloseRequested(sessionId: sessionId, scope: .close))
      return true
    }

    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.closeFocusedSession.staleFocus",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "focusedSessionId": nullableString(focusedSessionId),
        "reason": reason,
        "sessionId": sessionId,
      ])
    return false
  }

  func openBrowserDevTools(sessionId: String) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserDevTools")
    if let chromiumView = session.chromiumView {
      chromiumView.toggleDevTools()
    } else if let webView = session.webView, !NativeBrowserDevTools.toggle(for: webView) {
      NSSound.beep()
    }
  }

  func openProjectEditorDevTools(projectId: String) {
    guard let session = projectEditorPaneSessions[projectId] else {
      return
    }
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorDevTools")
    if let chromiumView = session.chromiumView {
      chromiumView.toggleDevTools()
    } else if let webView = session.webView, !NativeBrowserDevTools.toggle(for: webView) {
      NSSound.beep()
    }
  }

  func injectBrowserReactGrab(sessionId: String) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserReactGrab")
    Task { @MainActor in
      if let chromiumView = session.chromiumView {
        await NativeBrowserReactGrabInjector.toggleOrInject(into: chromiumView)
      } else if let webView = session.webView {
        await NativeBrowserReactGrabInjector.toggleOrInject(into: webView)
      }
    }
  }

  func injectBrowserAgentation(sessionId: String) {
    guard let session = webPaneSessions[sessionId] else {
      NSLog("Agentation: missing browser session %@", sessionId)
      NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.agentation.missingSession", [
        "sessionId": sessionId,
      ])
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserAgentation")
    Task { @MainActor in
      if let chromiumView = session.chromiumView {
        NSLog("Agentation: dispatching to browser CEF session %@", sessionId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.agentation.dispatch", [
          "currentUrl": chromiumView.currentURLString ?? NSNull(),
          "renderer": "cef",
          "sessionId": sessionId,
        ])
        await NativeBrowserAgentationInjector.toggleOrInject(into: chromiumView)
      } else if let webView = session.webView {
        NSLog("Agentation: dispatching to browser WKWebView session %@", sessionId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.agentation.dispatch", [
          "currentUrl": webView.url?.absoluteString ?? NSNull(),
          "renderer": "webkit",
          "sessionId": sessionId,
        ])
        await NativeBrowserAgentationInjector.toggleOrInject(into: webView)
      } else {
        NSLog("Agentation: browser session %@ has no web renderer", sessionId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.agentation.missingRenderer", [
          "sessionId": sessionId,
        ])
      }
    }
  }

  func injectProjectEditorReactGrab(projectId: String) {
    guard let session = projectEditorPaneSessions[projectId] else {
      return
    }
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorReactGrab")
    Task { @MainActor in
      if let chromiumView = session.chromiumView {
        await NativeBrowserReactGrabInjector.toggleOrInject(into: chromiumView)
      } else if let webView = session.webView {
        await NativeBrowserReactGrabInjector.toggleOrInject(into: webView)
      }
    }
  }

  func injectProjectEditorAgentation(projectId: String) {
    guard let session = projectEditorPaneSessions[projectId] else {
      NSLog("Agentation: missing project editor session %@", projectId)
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.agentation.missingSession", [
        "projectId": projectId,
      ])
      return
    }
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorAgentation")
    Task { @MainActor in
      if let chromiumView = session.chromiumView {
        NSLog("Agentation: dispatching to project editor CEF session %@", projectId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.agentation.dispatch", [
          "currentUrl": chromiumView.currentURLString ?? NSNull(),
          "projectId": projectId,
          "renderer": "cef",
        ])
        await NativeBrowserAgentationInjector.toggleOrInject(into: chromiumView)
      } else if let webView = session.webView {
        NSLog("Agentation: dispatching to project editor WKWebView session %@", projectId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.agentation.dispatch", [
          "currentUrl": webView.url?.absoluteString ?? NSNull(),
          "projectId": projectId,
          "renderer": "webkit",
        ])
        await NativeBrowserAgentationInjector.toggleOrInject(into: webView)
      } else {
        NSLog("Agentation: project editor session %@ has no web renderer", projectId)
        NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.agentation.missingRenderer", [
          "projectId": projectId,
        ])
      }
    }
  }

  func showBrowserProfilePicker(sessionId: String) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserProfilePicker")
    NativeBrowserProfileUI.showPicker(
      parentWindow: window,
      currentProfileID: session.browserProfileID
    )
  }

  func showProjectEditorProfilePicker(projectId: String) {
    guard projectEditorPaneSessions[projectId] != nil else {
      return
    }
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorProfilePicker")
    NativeBrowserProfileUI.showPicker(parentWindow: window, currentProfileID: nil)
  }

  func showBrowserImportSettings(sessionId: String) {
    guard webPaneSessions[sessionId] != nil else {
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserImportSettings")
    NativeBrowserProfileUI.showImportSettings(parentWindow: window)
  }

  func showProjectEditorImportSettings(projectId: String) {
    guard projectEditorPaneSessions[projectId] != nil else {
      return
    }
    focusProjectEditorPane(projectId: projectId, reason: "projectEditorImportSettings")
    NativeBrowserProfileUI.showImportSettings(parentWindow: window)
  }

  func reloadWebPane(sessionId: String) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    focusWebPane(sessionId: sessionId, reason: "browserPaneReload")
    /**
     CDXC:BrowserPanes 2026-05-02-17:39
     Browser-pane title-bar reload must operate on the embedded WKWebView,
     because browser panes are first-class AppKit panes and their title-bar
     controls should not be terminal-only no-ops.
     */
    if session.isLoading {
      if let chromiumView = session.chromiumView {
        chromiumView.stopLoading()
      } else {
        session.webView?.stopLoading()
      }
    } else {
      if let chromiumView = session.chromiumView {
        chromiumView.reload()
      } else {
        session.webView?.reload()
      }
    }
  }

  func focusTerminal(sessionId: String, reason: String = "explicitFocusTerminalCommand") {
    guard let view = sessions[sessionId]?.view else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.focusTerminal.missingSession",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "knownSessionIds": Array(sessions.keys).sorted(),
          "reason": reason,
          "requestedSessionId": sessionId,
          "responderBefore": responderSnapshot(),
      ])
      return
    }
    let isCommandPanelSession = commandsPanelActiveSessionIds.contains(sessionId)
    let previousFocusedTerminalSessionId =
      isCommandPanelSession ? commandsPanelFocusedSessionId : focusedSessionId
    if !isCommandPanelSession,
      activateProjectEditorCompanionPane(sessionId: sessionId, focus: true, reason: reason)
    {
      return
    }
    if !isCommandPanelSession {
      let hadActiveProjectEditor = activeProjectEditorId != nil
      activeProjectEditorId = nil
      if hadActiveProjectEditor {
        needsLayout = true
        layoutSubtreeIfNeeded()
      }
    }
    if isCommandPanelSession {
      /**
       CDXC:CommandsPanel 2026-05-14-09:31:
       Command-pane tabs and titlebar actions are an overlay surface, including
       while embedded VS Code is the active workspace view. Focusing a command
       terminal updates only commandsPanelFocusedSessionId and must not clear
       activeProjectEditorId, otherwise clicking Pin, Close, Sleep, or another
       command tab switches the workspace out of Code before the control action
       reaches the sidebar.
       */
      commandsPanelFocusedSessionId = sessionId
    } else {
      focusedSessionId = sessionId
    }
    orderTerminalPaneViewsToFront(sessions[sessionId])
    updateAllTerminalBorders()
    let targetWindow = poppedOutPaneControllers[sessionId]?.window ?? window
    poppedOutPaneControllers[sessionId]?.window?.makeKeyAndOrderFront(nil)
    let didChangeFocus = targetWindow?.firstResponder !== view
    let responderBefore = responderSnapshot()
    programmaticFocusDepth += 1
    let makeFirstResponderResult = targetWindow?.makeFirstResponder(view) ?? false
    programmaticFocusDepth -= 1
    let responderAfter = responderSnapshot()
    logFocusSurfaceState(
      event: "nativeFocusTrace.focusTerminalSurfaceState",
      reason: reason,
      details: [
        "didChangeFocus": didChangeFocus,
        "makeFirstResponderResult": makeFirstResponderResult,
        "requestedSessionId": sessionId,
        "targetWindowNumber": targetWindow?.windowNumber ?? 0,
      ])
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.focusTerminal.completed",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "didChangeFocus": didChangeFocus,
        "makeFirstResponderResult": makeFirstResponderResult,
        "reason": reason,
        "requestedSessionId": sessionId,
        "responderAfter": responderAfter,
        "responderBefore": responderBefore,
        "viewFrame": describeFrame(view.frame),
        "visibleSessionIds": orderedVisibleSessionIds(),
        "windowIsKey": window?.isKeyWindow ?? false,
      ])
    refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
      sessionId: sessionId,
      previousSessionId: previousFocusedTerminalSessionId,
      didSurface: false,
      reason: "focusTerminal.\(reason)")
    if isCommandPanelSession {
      emitFocusedSessionSelectionIfNeeded(sessionId: sessionId, reason: reason)
    }
  }

  private func refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
    sessionId: String,
    previousSessionId: String?,
    didSurface: Bool,
    reason: String
  ) {
    guard didSurface || (previousSessionId != nil && previousSessionId != sessionId) else {
      return
    }
    refreshZmxPersistenceTerminalIfNeeded(sessionId: sessionId, reason: reason)
  }

  private func refreshZmxPersistenceTerminalIfNeeded(sessionId: String, reason: String) {
    let session = sessions[sessionId]
    let isWorkspaceActive = activeSessionIds.contains(sessionId)
    let isCommandActive = commandsPanelActiveSessionIds.contains(sessionId)
    let skipReason: String?
    if session == nil {
      skipReason = "missing-terminal-session"
    } else if session?.sessionPersistenceProvider != .zmx {
      skipReason = "provider-not-zmx"
    } else if !isWorkspaceActive && !isCommandActive {
      skipReason = "session-not-active"
    } else {
      skipReason = nil
    }
    if let skipReason {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.zmxPersistenceViewportRefresh.skipped",
        details: zmxPersistenceRefreshDiagnosticDetails(
          sessionId: sessionId,
          reason: reason,
          session: session,
          extra: ["skipReason": skipReason]),
        force: true)
      return
    }
    guard let session else { return }
    session.view.refreshZmxPersistenceViewport(reason: reason)
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.zmxPersistenceViewportRefresh.sent",
      details: zmxPersistenceRefreshDiagnosticDetails(
        sessionId: sessionId,
        reason: reason,
        session: session),
      force: true)
  }

  private func scheduleZmxPersistenceTerminalRefreshAfterResize(reason: String) {
    scheduleZmxPersistenceTerminalRefreshAfterResize(
      sessionIds: zmxPersistenceTerminalSessionIdsForSurfacedPanes(),
      reason: reason)
  }

  func scheduleZmxPersistenceRefreshForSurfacedTerminalsAfterResize(reason: String) {
    /*
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     External resize owners such as the main window and sidebar chrome can resize terminal panes without entering TerminalWorkspaceView's split-drag handlers.
     Expose a surfaced-only scheduler so those owners can request the same trailing zmx repaint without targeting hidden zmx tab siblings.
     */
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: reason)
  }

  private func scheduleZmxPersistenceTerminalRefreshAfterResize(sessionIds: [String], reason: String) {
    guard !sessionIds.isEmpty else {
      zmxPersistenceRefreshTimer?.invalidate()
      zmxPersistenceRefreshTimer = nil
      return
    }
    zmxPersistenceRefreshTimer?.invalidate()
    /**
     CDXC:ZmxPersistenceDiagnostics 2026-05-18-08:52:
     Intermittent broken text after sidebar session surfacing needs forced,
     low-volume breadcrumbs that survive normal Debugging Mode settings. Log
     only zmx refresh scheduling, firing, send, and skip decisions so a repro
     can be located by session id and minute without enabling broad focus logs.
     */
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.zmxPersistenceViewportRefresh.resizeScheduled",
      details: [
        "debounceSeconds": Double(Self.zmxPersistenceRefreshDebounceInterval),
        "reason": reason,
        "sessionIds": sessionIds,
        "visibleCommandSessionIds": orderedVisibleCommandSessionIds(),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ],
      force: true)
    zmxPersistenceRefreshTimer = Timer.scheduledTimer(
      withTimeInterval: Self.zmxPersistenceRefreshDebounceInterval,
      repeats: false
    ) { [weak self] _ in
      Task { @MainActor in
        guard let self else { return }
        self.zmxPersistenceRefreshTimer = nil
        TerminalFocusDebugLog.append(
          event: "nativeWorkspace.zmxPersistenceViewportRefresh.resizeFired",
          details: [
            "reason": reason,
            "sessionIds": sessionIds,
            "visibleCommandSessionIds": self.orderedVisibleCommandSessionIds(),
            "visibleSessionIds": self.orderedVisibleSessionIds(),
          ],
          force: true)
        for sessionId in sessionIds {
          self.refreshZmxPersistenceTerminalIfNeeded(
            sessionId: sessionId,
            reason: "resizeDebounce.\(reason)")
        }
      }
    }
  }

  private func refreshZmxPersistenceTerminalsForSurfacedPanes(reason: String) {
    let sessionIds = zmxPersistenceTerminalSessionIdsForSurfacedPanes()
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.zmxPersistenceViewportRefresh.surfacedPanes",
      details: [
        "activeProjectEditorId": nullableString(activeProjectEditorId),
        "commandsPanelIsVisible": commandsPanelIsVisible,
        "companionSessionId": nullableString(projectEditorCompanionSessionId),
        "companionVisible": projectEditorCompanionIsVisible,
        "reason": reason,
        "sessionIds": sessionIds,
        "visibleCommandSessionIds": orderedVisibleCommandSessionIds(),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ],
      force: true)
    for sessionId in sessionIds {
      refreshZmxPersistenceTerminalIfNeeded(sessionId: sessionId, reason: "surfacedPanes.\(reason)")
    }
  }

  private func zmxPersistenceTerminalSessionIdsForSurfacedPanes() -> [String] {
    var candidates: [String] = []
    if activeProjectEditorId != nil {
      if projectEditorCompanionIsVisible, let projectEditorCompanionSessionId {
        candidates.append(projectEditorCompanionSessionId)
      }
    } else {
      candidates.append(contentsOf: orderedVisiblePaneOwnerSessionIds())
    }
    if commandsPanelIsVisible {
      candidates.append(contentsOf: orderedVisibleCommandPaneOwnerSessionIds())
    }
    return zmxPersistenceTerminalSessionIds(from: candidates, includePoppedOut: false)
  }

  private func zmxPersistenceTerminalSessionIds(
    from candidates: [String],
    includePoppedOut: Bool
  ) -> [String] {
    var seen = Set<String>()
    return candidates.compactMap { sessionId in
      guard !seen.contains(sessionId),
        (includePoppedOut || !poppedOutSessionIds.contains(sessionId)),
        sessions[sessionId]?.sessionPersistenceProvider == .zmx
      else {
        return nil
      }
      seen.insert(sessionId)
      return sessionId
    }
  }

  private func zmxPersistenceRefreshDiagnosticDetails(
    sessionId: String,
    reason: String,
    session: TerminalSession?,
    extra: [String: Any] = [:]
  ) -> [String: Any] {
    var details: [String: Any] = [
      "activeSessionIds": Array(activeSessionIds).sorted(),
      "commandsPanelActiveSessionIds": Array(commandsPanelActiveSessionIds).sorted(),
      "focusedSessionId": nullableString(focusedSessionId),
      "commandsPanelFocusedSessionId": nullableString(commandsPanelFocusedSessionId),
      "isCommandActive": commandsPanelActiveSessionIds.contains(sessionId),
      "isPoppedOut": poppedOutSessionIds.contains(sessionId),
      "isWorkspaceActive": activeSessionIds.contains(sessionId),
      "reason": reason,
      "responder": responderSnapshot(),
      "sessionId": sessionId,
      "sessionPersistenceName": nullableString(session?.sessionPersistenceName),
      "sessionPersistenceProvider": session?.sessionPersistenceProvider?.rawValue ?? "none",
      "surfaceHasGhosttySurface": session?.view.hasGhosttySurfaceForDiagnostics ?? false,
      "visibleCommandSessionIds": orderedVisibleCommandSessionIds(),
      "visibleSessionIds": orderedVisibleSessionIds(),
    ]
    for (key, value) in extra {
      details[key] = value
    }
    return details
  }

  private func focusTerminalFromContentMouseDown(
    surfaceView: GhostexGhosttySurfaceView,
    event: NSEvent
  ) {
    guard let clickedSessionId = surfaceView.ghostexSessionId else {
      return
    }
    emitAttentionAcknowledgementClickIfNeeded(
      sessionId: clickedSessionId,
      reason: "nativeTerminalContentMouseDown")
    let responderSessionId =
      event.window?.firstResponder.flatMap { sessionId(containing: $0) } ?? currentResponderSessionId()
    let isSurfaceFirstResponder = event.window?.firstResponder === surfaceView
    let localFocusedSessionId = commandsPanelActiveSessionIds.contains(clickedSessionId)
      ? commandsPanelFocusedSessionId
      : focusedSessionId
    guard localFocusedSessionId != clickedSessionId
      || responderSessionId != clickedSessionId
      || !isSurfaceFirstResponder
    else {
      return
    }
    /**
     CDXC:NativeTerminalFocus 2026-05-13-07:41
     Terminal content clicks must focus the clicked pane through the same native
     AppKit path as titlebar clicks. Relying only on NSWindow first-responder
     change notifications misses cases where AppKit keeps an old responder or
     duplicate focus suppression leaves the visible pane border stale.
     */
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.terminalContent.focusMouseDown",
      details: [
        "focusedSessionIdBefore": nullableString(focusedSessionId),
        "commandsPanelFocusedSessionIdBefore": nullableString(commandsPanelFocusedSessionId),
        "isSurfaceFirstResponder": isSurfaceFirstResponder,
        "responderSessionIdBefore": nullableString(responderSessionId),
        "sessionId": clickedSessionId,
      ])
    focusTerminal(sessionId: clickedSessionId, reason: "nativeTerminalContentMouseDown")
  }

  private func emitAttentionAcknowledgementClickIfNeeded(sessionId: String, reason: String) {
    guard attentionSessionIds.contains(sessionId) || sessionActivities[sessionId] == .attention else {
      return
    }
    /**
     CDXC:SessionAttention 2026-05-16-23:35:
     User clicks on an already-focused pane or its title bar can be suppressed as duplicate focus locally. Still emit the existing terminalFocused event while the session is green/attention so the sidebar can clear the shared attention state after its 1.5-second minimum visibility floor.
     */
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.attentionClickAcknowledgement.emitted",
      details: [
        "reason": reason,
        "sessionId": sessionId,
      ])
    sendEvent(.terminalFocused(sessionId: sessionId))
  }

  func windowFirstResponderChanged(_ responder: NSResponder?, reason: String) {
    if programmaticFocusDepth > 0 {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.windowFirstResponderChanged.programmaticSkipped",
        details: [
          "programmaticFocusDepth": programmaticFocusDepth,
          "reason": reason,
          "responder": responder.map { String(describing: type(of: $0)) } ?? "nil",
        ])
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.windowFirstResponderChangedProgrammatic",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "focusedSessionId": nullableString(focusedSessionId),
          "programmaticFocusDepth": programmaticFocusDepth,
          "reason": reason,
          "responder": responder.map { String(describing: type(of: $0)) } ?? "nil",
          "responderSessionId": nullableString(responder.flatMap { self.sessionId(containing: $0) }),
          "visibleSessionIds": orderedVisibleSessionIds(),
        ])
      return
    }
    guard let responder else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.windowFirstResponderChanged.nil",
        details: [
          "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
          "reason": reason,
        ])
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.windowFirstResponderChangedNil",
        details: [
          "focusedSessionId": nullableString(focusedSessionId),
          "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
          "reason": reason,
        ])
      return
    }
    /**
     CDXC:NativeTerminalFocus 2026-05-11-11:48
     Wrong-pane typing reports require the first-responder transition history,
     not just the later terminalFocused event. Persist each AppKit-originated
     responder handoff while debugging so key-route logs can be correlated with
     the exact responder that existed before the user typed.
     */
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.windowFirstResponderChanged",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "focusedSessionIdBeforeEmit": nullableString(focusedSessionId),
        "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
        "reason": reason,
        "responder": String(describing: type(of: responder)),
        "responderSessionId": nullableString(sessionId(containing: responder)),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    emitFocusedSessionIfNeeded(for: responder, reason: reason)
  }

  func windowKeyDownDispatch(_ event: NSEvent) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    guard focusedSessionId != nil || currentResponderSessionId() != nil else {
      return
    }
    var payload = keyboardRouteDebugPayload(surfaceSessionId: nil, event: event)
    payload["phase"] = "windowDispatch"
    /**
     CDXC:NativeTerminalFocus 2026-05-11-11:48
     The focused border is the user's visible input target. Repro logs must
     capture AppKit's keyDown dispatch target before any behavior change, so a
     later fix can be based on whether the first responder, focus ring, or
     Ghostty surface is the component that drifted.
     */
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.windowKeyDownDispatch",
      details: payload)
  }

  private func logSurfaceKeyDownProbe(
    surfaceView: GhostexGhosttySurfaceView,
    event: NSEvent,
    phase: String
  ) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    var payload = keyboardRouteDebugPayload(surfaceSessionId: surfaceView.ghostexSessionId, event: event)
    payload["firstResponderIsSurface"] = window?.firstResponder === surfaceView
    payload["phase"] = phase
    payload["searchActive"] = surfaceView.searchState != nil
    payload["surfaceFocusedFlag"] = surfaceView.focused
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.surfaceKeyDown",
      details: payload)
  }

  private func logTerminalSearchInteraction(
    _ event: String,
    session: TerminalSession,
    details: [String: Any] = [:]
  ) {
    var payload = keyboardRouteDebugPayload(surfaceSessionId: session.sessionId)
    payload["searchActive"] = session.view.searchState != nil
    payload["searchBarFrame"] = describeFrame(session.searchBarView.frame)
    payload["searchBarHidden"] = session.searchBarView.isHidden
    payload["searchNeedleLength"] = session.view.searchState?.needle.count ?? 0
    payload["sessionId"] = session.sessionId
    payload["surfaceFocusedFlag"] = session.view.focused
    for (key, value) in details {
      payload[key] = value
    }
    /**
     CDXC:NativeTerminalSearch 2026-05-19-09:02:
     Cmd+F search-box repros need AppKit routing breadcrumbs for the embedded
     Ghostty panes. Log search visibility, hit-test targets, first-responder
     state, and query length without persisting the user's typed search text.
     */
    TerminalFocusDebugLog.append(event: event, details: payload)
  }

  private func logSurfaceTextInputProbe(
    surfaceView: GhostexGhosttySurfaceView,
    text: Any,
    replacementRange: NSRange
  ) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    var payload = keyboardRouteDebugPayload(surfaceSessionId: surfaceView.ghostexSessionId)
    payload["firstResponderIsSurface"] = window?.firstResponder === surfaceView
    payload["insertTextLength"] = Self.textInputLength(text)
    payload["insertTextType"] = String(describing: type(of: text))
    payload["phase"] = "insertText"
    payload["replacementLength"] = replacementRange.length
    payload["replacementLocation"] =
      replacementRange.location == NSNotFound ? -1 : replacementRange.location
    payload["searchActive"] = surfaceView.searchState != nil
    payload["surfaceFocusedFlag"] = surfaceView.focused
    /**
     CDXC:NativeTerminalFocus 2026-05-11-11:48
     Text input probes intentionally persist only route metadata and lengths.
     Typed terminal content can include secrets, so the repro log must prove
     wrong-pane delivery without storing the characters that were typed.
     */
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.surfaceTextInput",
      details: payload)
  }

  func writeTerminalText(sessionId: String, text: String) {
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.writeTerminalText",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "requestedSessionId": sessionId,
        "responderBefore": responderSnapshot(),
        "textLength": text.count,
        "textPreview": summarizeTerminalText(text),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    sessions[sessionId]?.view.surfaceModel?.sendText(text)
  }

  func writeTerminalScript(sessionId: String, text: String) {
    do {
      let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("ghostex-restore-scripts", isDirectory: true)
      try FileManager.default.createDirectory(
        at: directory, withIntermediateDirectories: true,
        attributes: [FileAttributeKey.posixPermissions: 0o700])
      let scriptURL = directory.appendingPathComponent("restore-\(UUID().uuidString).zsh")
      try text.write(to: scriptURL, atomically: true, encoding: .utf8)
      try FileManager.default.setAttributes(
        [FileAttributeKey.posixPermissions: 0o600],
        ofItemAtPath: scriptURL.path)
      let quotedPath = shellQuote(scriptURL.path)
      /**
       CDXC:SessionRestore 2026-05-26-15:52:
       Wake restore scripts must execute in the already-open interactive shell
       so user aliases/functions such as `x` are available, but the terminal
       should only echo a short staging command. Source the private temp file
       directly from the current shell instead of spawning cat plus eval or
       launching a fresh non-interactive zsh.
       */
      let command =
        ". \(quotedPath); /bin/rm -f -- \(quotedPath)\r"
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.writeTerminalScript",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "requestedSessionId": sessionId,
          "responderBefore": responderSnapshot(),
          "scriptLength": text.count,
          "visibleSessionIds": orderedVisibleSessionIds(),
        ])
      sessions[sessionId]?.view.surfaceModel?.sendText(command)
    } catch {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.writeTerminalScript.failed",
        details: [
          "error": String(describing: error),
          "requestedSessionId": sessionId,
          "scriptLength": text.count,
        ],
        force: true)
      let message = "printf '%s\\n' \(shellQuote("Ghostex could not stage the restore script: \(error.localizedDescription)"))\r"
      sessions[sessionId]?.view.surfaceModel?.sendText(message)
    }
  }

  private func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  func readTerminalText(_ command: ReadTerminalText) {
    /**
     CDXC:CliTerminalReadback 2026-05-23-13:18:
     Ghostex CLI readback must inspect the existing visible sidebar-backed
     Ghostty surface. Do not spawn helper terminals or shell commands for this
     path, because agent coordination must never create hidden sessions.
     */
    guard let surfaceModel = sessions[command.sessionId]?.view.surfaceModel else {
      sendEvent(
        .terminalTextResult(
          requestId: command.requestId,
          sessionId: command.sessionId,
          ok: false,
          text: nil,
          error: "terminal-surface-missing"
        ))
      return
    }
    let text = surfaceModel.readText(source: command.source)
    sendEvent(
      .terminalTextResult(
        requestId: command.requestId,
        sessionId: command.sessionId,
        ok: text != nil,
        text: text,
        error: text == nil ? "terminal-text-unavailable" : nil
      ))
  }

  /**
   CDXC:SessionTitleSync 2026-04-26-10:04
   The sidebar stages `/rename <title>` as terminal text, then submits it with
   a real Return key event. Ghostty treats text carriage returns differently
   in Codex, so Enter must travel through the same key path as a user press.

   CDXC:SessionTitleSync 2026-05-16-22:19
   Rename submission must target the staged terminal command without stealing
   focus from the user's current pane. Send Enter directly to the target
   surface view and preserve the current focused session/responder state.
   */
  func sendTerminalEnter(sessionId: String) {
    guard let view = sessions[sessionId]?.view else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.sendTerminalEnter.missingSession",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "requestedSessionId": sessionId,
          "responderBefore": responderSnapshot(),
          "visibleSessionIds": orderedVisibleSessionIds(),
        ])
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.sendTerminalEnter.start",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "preserveFocus": true,
        "requestedSessionId": sessionId,
        "responderBefore": responderSnapshot(),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    guard
      let event = NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: [],
        timestamp: ProcessInfo.processInfo.systemUptime,
        windowNumber: view.window?.windowNumber ?? 0,
        context: nil,
        characters: "\r",
        charactersIgnoringModifiers: "\r",
        isARepeat: false,
        keyCode: 36
      )
    else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.sendTerminalEnter.eventCreationFailed",
        details: [
          "requestedSessionId": sessionId,
          "responderBeforeSend": responderSnapshot(),
        ])
      return
    }
    view.keyDown(with: event)
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.sendTerminalEnter.sent",
      details: [
        "requestedSessionId": sessionId,
        "responderAfter": responderSnapshot(),
      ])
  }

  func setTerminalLayout(_ nextLayout: NativeTerminalLayout) {
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.setTerminalLayout",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "responderBefore": responderSnapshot(),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    terminalLayout = nextLayout
    paneResizeRatiosByPath.removeAll()
    paneResizeDrag = nil
    needsLayout = true
  }

  func setTerminalVisibility(sessionId: String, visible: Bool) {
    guard let session = sessions[sessionId] else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.setTerminalVisibility.missingSession",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "requestedSessionId": sessionId,
          "responderBefore": responderSnapshot(),
          "visible": visible,
        ])
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.setTerminalVisibility",
      details: [
        "activeSessionIdsBefore": Array(activeSessionIds).sorted(),
        "requestedSessionId": sessionId,
        "responderBefore": responderSnapshot(),
        "visible": visible,
      ])
    if visible {
      activeSessionIds.insert(sessionId)
      session.containerView.isHidden = false
    } else {
      activeSessionIds.remove(sessionId)
      moveOffscreen(session.containerView)
    }
    session.containerView.isHidden = !visible
    session.scrollView.isHidden = false
    session.searchBarView.isHidden = !visible || session.view.searchState == nil
    session.titleBarView.isHidden = !visible
    session.borderView.isHidden = !visible
    needsLayout = true
    updateTerminalBorder(for: sessionId)
  }

  func setActiveTerminalSet(_ command: SetActiveTerminalSet) {
    let responderBefore = responderSnapshot()
    let nextActiveSessionIds = Set(command.activeSessionIds)
    let nextCommandsPanelActiveSessionIds = Set(command.commandsPanelActiveSessionIds ?? [])
    let nextActiveProjectEditorId = command.activeProjectEditorId
    let nextPoppedOutSessionIds = Set(command.poppedOutSessionIds ?? []).intersection(nextActiveSessionIds)
    let nextPaneGap = Self.clampedPaneGap(command.paneGap)
    let nextLayout = command.layout
    let previousSidebarResizeEdgeExtensionWidth = sidebarResizeEdgeExtensionWidth
    let previousDelayedSendRemainingLabels = sessionDelayedSendRemainingLabels
    let shouldRelayout =
      command.layoutChanged
      ?? (nativeLayoutSignature(
          activeSessionIds: nextActiveSessionIds,
          activeProjectEditorId: nextActiveProjectEditorId,
          layout: nextLayout,
          paneGap: nextPaneGap,
          poppedOutSessionIds: nextPoppedOutSessionIds
        )
        != nativeLayoutSignature(
          activeSessionIds: activeSessionIds,
          activeProjectEditorId: activeProjectEditorId,
          layout: terminalLayout,
          paneGap: paneGap,
          poppedOutSessionIds: poppedOutSessionIds
        ))
    let previousFocusedSessionId = focusedSessionId
    let previousCommandsPanelFocusedSessionId = commandsPanelFocusedSessionId
    let previousCommandsPanelActiveSessionIds = commandsPanelActiveSessionIds
    let previousCommandsPanelIsVisible = commandsPanelIsVisible
    let previousCommandsPanelHeightRatio = commandsPanelHeightRatio
    let previousCommandsPanelMode = commandsPanelMode
    let previousActiveProjectEditorId = activeProjectEditorId
    let previousPaneGap = paneGap
    let previousProjectEditorCompanionIsVisible = projectEditorCompanionIsVisible
    let previousProjectEditorCompanionPaneHidden = projectEditorCompanionPaneHidden
    let previousProjectEditorCompanionSessionId = projectEditorCompanionSessionId
    let previousSessionTitleBarActions = sessionTitleBarActions
    let previousSessionTitles = sessionTitles
    let responderSessionIdBefore = currentResponderSessionId()
    let passiveResponderSessionId = command.focusRequestId == nil ? responderSessionIdBefore : nil
    let passiveResponderFocusedSessionId = passiveResponderSessionId.flatMap {
      nextActiveSessionIds.contains($0) ? $0 : nil
    }
    let passiveResponderCommandSessionId = passiveResponderSessionId.flatMap {
      nextCommandsPanelActiveSessionIds.contains($0) ? $0 : nil
    }
    if command.focusRequestId != nil || previousFocusedSessionId != command.focusedSessionId {
      /**
       CDXC:NativeTerminalFocus 2026-05-09-15:30
       Active-border misses can be caused by a later setActiveTerminalSet
       repainting native focus from sidebar state. Record only focus-changing
       layout commands, including whether they are explicit focus requests or
       passive sync, so reproduction logs show the overwrite boundary.
       */
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.setActiveTerminalSetFocusInput",
        details: [
          "activeSessionIdsBefore": Array(activeSessionIds).sorted(),
          "commandFocusedSessionId": nullableString(command.focusedSessionId),
          "focusRequestId": command.focusRequestId ?? 0,
          "previousFocusedSessionId": nullableString(previousFocusedSessionId),
          "responderBefore": responderSnapshot(),
          "responderSessionIdBefore": nullableString(responderSessionIdBefore),
          "shouldRelayout": shouldRelayout,
        ])
    }
    activeSessionIds = nextActiveSessionIds
    commandsPanelActiveSessionIds = nextCommandsPanelActiveSessionIds
    /**
     CDXC:NativeTerminalFocus 2026-05-23-09:13:
     Session status updates can reorder or repaint sidebar cards while the user
     is typing in a native terminal. Passive setActiveTerminalSet sync must not
     copy a stale sidebar-focused id over the AppKit first responder; preserve
     the responder-owned workspace or command pane unless a fresh focusRequestId
     proves the user explicitly selected another session.
     */
    commandsPanelFocusedSessionId =
      passiveResponderCommandSessionId ?? command.commandsPanelFocusedSessionId
    commandsPanelHeightRatio = Self.clampedCommandsPanelHeightRatio(command.commandsPanelHeightRatio)
    commandsPanelIsVisible = command.commandsPanelIsVisible == true
    commandsPanelLayout = command.commandsPanelLayout
    commandsPanelMode = command.commandsPanelMode ?? "pinned"
    attentionSessionIds = Set(command.attentionSessionIds ?? [])
    poppedOutSessionIds = nextPoppedOutSessionIds
    sleepingSessionIds = Set(command.sleepingSessionIds ?? [])
    sessionAgentIconColors = command.sessionAgentIconColors ?? [:]
    sessionAgentIconDataUrls = command.sessionAgentIconDataUrls ?? [:]
    sessionActivities = command.sessionActivities ?? [:]
    sessionDelayedSendRemainingLabels = command.sessionDelayedSendRemainingLabels ?? [:]
    sessionFaviconDataUrls = command.sessionFaviconDataUrls ?? [:]
    sessionTitleBarActions = command.sessionTitleBarActions ?? [:]
    sessionTitles = command.sessionTitles ?? [:]
    showSessionIdInTerminalPanes = command.showSessionIdInTerminalPanes == true
    activeProjectEditorId = nextActiveProjectEditorId
    let shouldRefreshPaneTabMetadata =
      previousSessionTitleBarActions != sessionTitleBarActions || previousSessionTitles != sessionTitles
    if previousDelayedSendRemainingLabels != sessionDelayedSendRemainingLabels {
      needsLayout = true
    }
    syncProjectEditorTabBars()
    focusedSessionId = passiveResponderFocusedSessionId ?? command.focusedSessionId
    terminalLayout = nextLayout
    paneGap = nextPaneGap
    if shouldRefreshPaneTabMetadata && !shouldRelayout {
      syncPaneTabChromeFromCurrentLayout()
    }
    if abs(sidebarResizeEdgeExtensionWidth - previousSidebarResizeEdgeExtensionWidth) > 0.5 {
      /**
       CDXC:SidebarResizeRails 2026-05-15-03:59:
       The root sidebar divider owns the workspace edge gap. Active-pane and
       pane-gap changes happen inside TerminalWorkspaceView, so ask the parent
       root view to refresh the divider frame whenever that covered width changes.
       */
      superview?.needsLayout = true
    }
    applyProjectEditorCompanionPaneHiddenPreference(
      command.activeProjectEditorCompanionPaneHidden,
      reason: "setActiveTerminalSet")
    syncProjectEditorCompanionSelectionFromSidebar(reason: "setActiveTerminalSet")
    /**
     CDXC:WorkspaceLayout 2026-04-28-06:08
     The terminal workspace background is user-configurable from Settings.
     Apply the chosen color directly to the AppKit backing layer so the
     visible space created by Pane Gap uses the user's color.
    */
    applyWorkspaceBackgroundColor(command.backgroundColor)
    let isProjectEditorActive = activeProjectEditorId != nil
    for session in sessions.values {
      let isCommandActive = commandsPanelActiveSessionIds.contains(session.sessionId)
      let chromeRole: TerminalPaneChromeRole = isCommandActive ? .commands : .workspace
      session.titleBarView.setChromeRole(chromeRole)
      session.borderView.setChromeRole(chromeRole)
      session.persistenceLabelView.setEnabledBySettings(showSessionIdInTerminalPanes)
      session.persistenceLabelView.setSuppressed(isCommandActive && !commandsPanelIsVisible)
      session.borderView.setHidesInactiveCommandBorder(isCommandActive && commandsPanelMode == "pinned")
      if let title = sessionTitles[session.sessionId] {
        session.titleBarView.setTitle(
          normalizedTerminalSessionTitle(title, sessionId: session.sessionId)
        )
      }
      if let actions = sessionTitleBarActions[session.sessionId] {
        session.titleBarView.setActions(actions)
      }
      session.titleBarView.setAgentIconDataUrl(
        sessionAgentIconDataUrls[session.sessionId],
        colorHex: sessionAgentIconColors[session.sessionId])
      session.delayedSendLabelView.setRemainingLabel(
        sessionDelayedSendRemainingLabels[session.sessionId])
      if shouldRelayout {
        let isPoppedOut = poppedOutSessionIds.contains(session.sessionId)
        let isActive = (!isProjectEditorActive && activeSessionIds.contains(session.sessionId)) || isCommandActive
        session.containerView.isHidden = !isActive || isPoppedOut
        session.scrollView.isHidden = false
        session.searchBarView.isHidden = !isActive || session.view.searchState == nil
        session.titleBarView.isHidden = !isActive && !isPoppedOut
        session.borderView.isHidden = !isActive
        if !isActive && !isPoppedOut {
          moveOffscreen(session.containerView)
        }
      }
    }
    for session in webPaneSessions.values {
      if let title = sessionTitles[session.sessionId] {
        session.titleBarView.setTitle(
          normalizedTerminalSessionTitle(title, sessionId: session.sessionId)
        )
      }
      if let actions = sessionTitleBarActions[session.sessionId] {
        session.titleBarView.setActions(actions)
      }
      session.titleBarView.setAgentIconDataUrl(
        sessionAgentIconDataUrls[session.sessionId],
        colorHex: sessionAgentIconColors[session.sessionId])
      if shouldRelayout {
        let isPoppedOut = poppedOutSessionIds.contains(session.sessionId)
        let isActive = !isProjectEditorActive && activeSessionIds.contains(session.sessionId)
        session.containerView.isHidden = !isActive || isPoppedOut
        session.hostView.isHidden = !isActive && !isPoppedOut
        session.titleBarView.isHidden = !isActive && !isPoppedOut
        session.borderView.isHidden = !isActive
        if !isActive && !isPoppedOut {
          moveOffscreen(session.containerView)
        }
      }
    }
    syncPoppedOutPaneWindows(reason: "setActiveTerminalSet")
    if shouldRelayout {
      for session in projectEditorPaneSessions.values {
        let isActive = activeProjectEditorId == session.projectId
        setProjectEditorTabHostVisibility(session, isActive: isActive)
        session.titleBarView?.isHidden = !isActive
        if isActive {
          /*
           CDXC:ChromiumBrowserPanes 2026-05-17-10:05:
           Switching the active session shown in the Code/Git companion pane must not temporarily expand the editor CEF host to full workspace width before the normal layout pass restores the split.
           Use the current companion editor frame during setActiveTerminalSet relayout so Chromium sees a stable width while the companion pane retargets.
           */
          let companionLayout = projectEditorCompanionLayout(in: bounds)
          layoutProjectEditorPane(session, in: companionLayout?.editorFrame ?? bounds)
          orderProjectEditorPaneToFront(session)
        } else {
          if let titleBarView = session.titleBarView {
            moveOffscreen(titleBarView)
          }
        }
      }
      needsLayout = true
      layoutSubtreeIfNeeded()
      scheduleDeferredWebPaneLayout(sessionId: command.focusedSessionId, reason: "setActiveTerminalSet")
    }
    updateAllTerminalBorders()
    let responderSessionIdAfterBorderApply = currentResponderSessionId()
    let focusedSurfaceSessionIdsAfterActiveSet = focusedSurfaceSessionIds()
    if command.focusRequestId == nil,
      let responderSessionIdAfterBorderApply,
      activeSessionIds.contains(responderSessionIdAfterBorderApply),
      responderSessionIdAfterBorderApply != focusedSessionId
    {
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.passiveSyncResponderMismatch",
        details: [
          "commandFocusedSessionId": nullableString(command.focusedSessionId),
          "focusedSessionIdAfterApply": nullableString(focusedSessionId),
          "responderAfterBorderApply": responderSnapshot(),
          "responderSessionIdAfterBorderApply": responderSessionIdAfterBorderApply,
          "visibleSessionIds": orderedVisibleSessionIds(),
        ])
    }
    if command.focusRequestId != nil
      || shouldRelayout
      || previousFocusedSessionId != command.focusedSessionId
      || focusedSurfaceSessionIdsAfterActiveSet.count > 1
      || (responderSessionIdAfterBorderApply != nil
        && responderSessionIdAfterBorderApply != focusedSessionId)
    {
      logFocusSurfaceState(
        event: "nativeFocusTrace.setActiveTerminalSetSurfaceState",
        reason: "setActiveTerminalSet.afterBorderApply",
        details: [
          "commandFocusedSessionId": nullableString(command.focusedSessionId),
          "focusRequestId": command.focusRequestId ?? 0,
          "focusedSurfaceSessionIdsAfterActiveSet": focusedSurfaceSessionIdsAfterActiveSet,
          "layoutChanged": shouldRelayout,
          "previousFocusedSessionId": nullableString(previousFocusedSessionId),
          "responderSessionIdAfterBorderApply": nullableString(responderSessionIdAfterBorderApply),
        ])
    }
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.setActiveTerminalSet.applied",
      details: [
        "activeProjectEditorId": nullableString(activeProjectEditorId),
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "attentionSessionIds": Array(attentionSessionIds).sorted(),
        "backgroundColor": command.backgroundColor ?? "default",
        "focusRequestId": command.focusRequestId ?? 0,
        "focusedSessionId": nullableString(command.focusedSessionId),
        "layoutChanged": shouldRelayout,
        "paneGap": Double(paneGap),
        "poppedOutSessionIds": Array(poppedOutSessionIds).sorted(),
        "responderAfterLayout": responderSnapshot(),
        "responderBefore": responderBefore,
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    if previousActiveProjectEditorId != activeProjectEditorId {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:03:
       Sidebar mode switches between Agents and Code/Git/Project can reveal a zmx terminal set through layout state rather than direct terminal focus.
       Refresh every surfaced zmx terminal after the active project-editor id is applied so both directions repair persisted terminal text.
       */
      refreshZmxPersistenceTerminalsForSurfacedPanes(reason: "setActiveTerminalSet.projectEditorModeSwitch")
    }
    if abs(previousPaneGap - paneGap) > 0.5 {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
       Pane-gap preference changes resize every surfaced workspace pane through layout sync instead of a drag handler.
       Schedule the trailing zmx refresh against surfaced pane owners only.
       */
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "setActiveTerminalSet.paneGapChanged")
    }
    if previousCommandsPanelIsVisible != commandsPanelIsVisible
      || previousCommandsPanelActiveSessionIds.isEmpty != commandsPanelActiveSessionIds.isEmpty
      || previousCommandsPanelMode != commandsPanelMode
      || abs(previousCommandsPanelHeightRatio - commandsPanelHeightRatio) > 0.001
    {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
       Commands panel show/hide, collapse/expand, mode, and height-ratio sync change the workspace terminal frame without necessarily using the native resize rail.
       Refresh only the surfaced zmx workspace or command pane after layout settles.
       */
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "setActiveTerminalSet.commandsPanelSurfaceOrFrameChanged")
    }
    if previousActiveProjectEditorId == activeProjectEditorId
      && (previousProjectEditorCompanionIsVisible != projectEditorCompanionIsVisible
      || previousProjectEditorCompanionPaneHidden != projectEditorCompanionPaneHidden
      || previousProjectEditorCompanionSessionId != projectEditorCompanionSessionId)
    {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
       Companion-pane show/hide and retargeting changes the visible terminal frame inside Code/Git/Project mode without always being a pane-resize drag.
       Use the surfaced-only scheduler so hidden Agents tabs do not receive zmx refresh requests.
       */
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "setActiveTerminalSet.projectEditorCompanionChanged")
    }
    if command.focusRequestId == nil,
      commandsPanelIsVisible,
      !previousCommandsPanelIsVisible,
      let commandsPanelFocusedSessionId,
      commandsPanelActiveSessionIds.contains(commandsPanelFocusedSessionId)
    {
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-09:04:
       Command-pane surfacing can make an already-selected zmx terminal visible without an explicit focus request.
       Use the pre-layout command-panel visibility so opening the pane still refreshes the terminal viewport once it is actually on screen.
       */
      refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
        sessionId: commandsPanelFocusedSessionId,
        previousSessionId: previousCommandsPanelFocusedSessionId,
        didSurface: true,
        reason: "setActiveTerminalSet.commandPanelVisible")
    }
    guard let focusRequestId = command.focusRequestId else {
      /**
       CDXC:NativeTerminalFocus 2026-05-04-16:02
       Passive status/layout sync can mark a side pane done/green while the
       user is typing in another terminal. Do not translate focusedSessionId
       into AppKit first-responder focus unless native-sidebar attached a fresh
       explicit focus request id.
       */
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.setActiveTerminalSet.focusSkipped",
        details: [
          "focusedSessionId": nullableString(command.focusedSessionId),
          "reason": "missingFocusRequestId",
          "responder": responderSnapshot(),
        ])
      return
    }
    guard lastAppliedLayoutFocusRequestId != focusRequestId else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.setActiveTerminalSet.focusSkipped",
        details: [
          "focusRequestId": focusRequestId,
          "focusedSessionId": nullableString(command.focusedSessionId),
          "reason": "duplicateFocusRequestId",
        ])
      return
    }
    lastAppliedLayoutFocusRequestId = focusRequestId
    let requestedFocusSessionId =
      command.focusedSessionId.flatMap { activeSessionIds.contains($0) ? $0 : nil }
      ?? command.commandsPanelFocusedSessionId.flatMap {
        commandsPanelActiveSessionIds.contains($0) ? $0 : nil
      }
    if let focusedSessionId = requestedFocusSessionId
    {
      if shouldPreserveNonTerminalFirstResponder() {
        /**
         CDXC:ScratchPadFocus 2026-04-28-05:35
         Layout focus requests must not steal typing focus from the full-window
         modal host or other WKWebView controls. Explicit terminal focus should
         still preserve non-terminal first responders when a modal owns input.
         */
        TerminalFocusDebugLog.append(
          event: "nativeWorkspace.setActiveTerminalSet.focusPreserved",
          details: [
            "focusedSessionId": focusedSessionId,
            "responder": responderSnapshot(),
          ])
        return
      }
      if sessions[focusedSessionId] != nil {
        let isCommandPanelFocus = commandsPanelActiveSessionIds.contains(focusedSessionId)
        focusTerminal(sessionId: focusedSessionId, reason: "setActiveTerminalSet")
        if isCommandPanelFocus {
          /*
           CDXC:ZmxPersistenceRefresh 2026-05-18-09:04:
           Command-pane focus state is applied from sidebar layout before focusTerminal runs, so focusTerminal can no longer compare against the previous command-pane selection.
           Refresh zmx using the pre-apply command-pane state when a command tab is selected or newly surfaced.
           */
          refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
            sessionId: focusedSessionId,
            previousSessionId: previousCommandsPanelFocusedSessionId,
            didSurface: commandsPanelIsVisible
              && (!previousCommandsPanelIsVisible
                || !previousCommandsPanelActiveSessionIds.contains(focusedSessionId)),
            reason: "setActiveTerminalSet.commandPanelFocusSwitch")
        } else {
          refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
            sessionId: focusedSessionId,
            previousSessionId: previousFocusedSessionId,
            didSurface: false,
            reason: "setActiveTerminalSet.focusSwitch")
        }
      } else if webPaneSessions[focusedSessionId] != nil {
        focusWebPane(sessionId: focusedSessionId, reason: "setActiveTerminalSet")
      } else {
        /*
         CDXC:SessionSurfaceRecovery 2026-05-23-09:05:
         Active layout can still contain a session after its native Ghostty
         surface disappeared, leaving AppKit with a selectable tab that cannot
         become first responder. Tell the sidebar to reload or replace the
         stale session instead of silently keeping the broken focus target.
         */
        TerminalFocusDebugLog.append(
          event: "nativeWorkspace.setActiveTerminalSet.missingSurface",
          details: [
            "activeSessionIds": Array(activeSessionIds).sorted(),
            "focusRequestId": focusRequestId,
            "focusedSessionId": focusedSessionId,
            "reason": "focusedSessionSurfaceMissing",
          ])
        sendEvent(.nativeSessionSurfaceMissing(sessionId: focusedSessionId))
      }
    } else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.setActiveTerminalSet.focusSkipped",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "commandsPanelActiveSessionIds": Array(commandsPanelActiveSessionIds).sorted(),
          "focusRequestId": focusRequestId,
          "focusedSessionId": nullableString(command.focusedSessionId),
          "commandsPanelFocusedSessionId": nullableString(command.commandsPanelFocusedSessionId),
          "reason": "focusedSessionNotActive",
        ])
    }
  }

  func setSidebarSide(_ side: SidebarSide) {
    /**
     CDXC:NativePaneChrome 2026-05-07-15:13
     The workspace's outer rounded active/done border corner follows sidebar
     placement: left-sidebar layouts round the bottom-right workspace pane,
     while right-sidebar layouts round the bottom-left workspace pane.
     */
    guard sidebarSide != side else {
      return
    }
    sidebarSide = side
    updateOuterBottomPaneBorderCorner()
  }

  override func layout() {
    super.layout()
    defer {
      /**
       CDXC:EditorPanes 2026-05-13-23:13
       The active VS Code editor layout branch returns before the normal
       terminal/browser pane layout path. Keep CEF drag/drop monitor sync in a
       layout defer so the bridge is installed for the project editor and
       uninstalled when no Chromium interaction surface remains visible.
       */
      syncCEFNativeDragSourceReleaseMonitor(reason: "layout")
      layoutFloatingEditorOverlay()
    }
    paneResizeHits.removeAll()
    paneContentHitRegions.removeAll()
    let commandSessionIds = orderedVisibleCommandSessionIds()
    let hasCommandsPanelSessions = !commandSessionIds.isEmpty
    let shouldShowExpandedCommandsPanel = commandsPanelIsVisible && hasCommandsPanelSessions
    let shouldShowCollapsedCommandsPanel = !commandsPanelIsVisible && hasCommandsPanelSessions
    let shouldShowCommandsPanel = shouldShowExpandedCommandsPanel || shouldShowCollapsedCommandsPanel
    let shouldFloatCommandsPanel = shouldShowExpandedCommandsPanel && commandsPanelMode == "floating"
    let reservedFloatingCommandsPanelBottomBarHeight =
      shouldFloatCommandsPanel ? collapsedCommandsPanelHeight() : 0
    /**
     CDXC:CommandsPanel 2026-05-15-13:45:
     Unpinned command panes float above the workspace when expanded, but the
     minimized tab-bar footprint still needs to stay reserved as a plain black
     strip. Reserving that same bottom strip in both expanded and minimized
     states prevents workspace panes from shifting when the command pane is
     shown or hidden.
     */
    let shouldReserveCommandsPanelSpace =
      shouldShowCollapsedCommandsPanel
      || (shouldShowExpandedCommandsPanel && commandsPanelMode == "pinned")
      || reservedFloatingCommandsPanelBottomBarHeight > 0
    let commandPanelHeight = shouldShowExpandedCommandsPanel
      ? clampedCommandsPanelHeight(bounds.height * commandsPanelHeightRatio)
      : shouldShowCollapsedCommandsPanel ? collapsedCommandsPanelHeight() : 0
    let floatingCommandsPanelMargin = shouldFloatCommandsPanel ? Self.floatingCommandsPanelMargin : 0
    let commandPanelResolvedHeight = min(
      max(0, bounds.height - reservedFloatingCommandsPanelBottomBarHeight - floatingCommandsPanelMargin * 2),
      commandPanelHeight)
    let collapsedCommandsPanelLeftMargin =
      shouldShowCollapsedCommandsPanel ? Self.collapsedCommandsPanelLeftMargin : 0
    let collapsedCommandsPanelRightMargin =
      shouldShowCollapsedCommandsPanel ? Self.collapsedCommandsPanelRightMargin : 0
    let resolvedCommandPanelBounds: CGRect = shouldShowCommandsPanel
      ? CGRect(
          x: bounds.minX + floatingCommandsPanelMargin + collapsedCommandsPanelLeftMargin,
          y: bounds.minY + reservedFloatingCommandsPanelBottomBarHeight + floatingCommandsPanelMargin,
          width: max(
            0,
            bounds.width - floatingCommandsPanelMargin * 2 - collapsedCommandsPanelLeftMargin
              - collapsedCommandsPanelRightMargin),
          height: commandPanelResolvedHeight)
      : .zero
    let reservedCommandsPanelSpaceHeight: CGFloat =
      shouldFloatCommandsPanel
      ? reservedFloatingCommandsPanelBottomBarHeight
      : shouldReserveCommandsPanelSpace ? resolvedCommandPanelBounds.height : 0
    let workspaceBounds =
      shouldReserveCommandsPanelSpace
      ? CGRect(
          x: bounds.minX,
          y: bounds.minY + reservedCommandsPanelSpaceHeight,
          width: bounds.width,
          height: max(0, bounds.height - reservedCommandsPanelSpaceHeight))
      : bounds
    if let activeProjectEditorId,
      let editorSession = projectEditorPaneSessions[activeProjectEditorId]
    {
      /**
       CDXC:EditorPanes 2026-05-08-13:02
       Sidebar resize changes the workspace bounds while VS Code owns the
       visible pane. Log the active editor layout branch before touching hosted
       Chromium so crash reports can be matched to the last AppKit frame.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.layout.active", [
        "activeProjectEditorId": activeProjectEditorId,
        "hostFrameBefore": describeFrame(editorSession.hostView.frame),
        "workspaceBounds": describeFrame(bounds),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      hideSplitSessionSurfacesForActiveEditor()
      let companionLayout = projectEditorCompanionLayout(in: workspaceBounds)
      projectEditorCompanionResizeWorkspaceBounds = workspaceBounds
      setProjectEditorTabHostVisibility(editorSession, isActive: true)
      let projectEditorLiveResizeBackingRect =
        commandsPanelResizeDrag != nil
        ? projectEditorCompanionLayout(in: bounds)?.editorFrame ?? bounds
        : nil
      layoutProjectEditorPane(
        editorSession,
        in: companionLayout?.editorFrame ?? workspaceBounds,
        chromiumLiveResizeBackingRect: projectEditorLiveResizeBackingRect)
      orderProjectEditorPaneToFront(editorSession)
      syncProjectEditorCompanionPane(layout: companionLayout)
      if shouldShowCommandsPanel {
        syncCommandsPanelChrome(
          in: resolvedCommandPanelBounds,
          isExpanded: shouldShowExpandedCommandsPanel,
          isFloating: shouldShowExpandedCommandsPanel && commandsPanelMode == "floating",
          reservedBottomBarHeight: reservedFloatingCommandsPanelBottomBarHeight)
        layoutCommandsPanel(commandSessionIds, in: resolvedCommandPanelBounds)
        syncCommandsPanelResizeHandle(
          in: resolvedCommandPanelBounds,
          isExpanded: shouldShowExpandedCommandsPanel)
        syncPaneResizeHandleViews()
      } else {
        hideCommandsPanelChrome()
        hideCommandsPanelResizeHandle()
        hidePaneResizeHandleViews()
      }
      return
    }
    hideProjectEditorCompanionChrome()
    let visibleSessionIds = orderedVisibleSessionIds()
    guard !visibleSessionIds.isEmpty || shouldShowCommandsPanel else {
      hideCommandsPanelResizeHandle()
      hidePaneResizeHandleViews()
      discardCursorRects()
      return
    }
    if !visibleSessionIds.isEmpty {
      if let terminalLayout {
        layoutTree(
          terminalLayout,
          in: layoutBounds(forVisibleCount: visibleSessionIds.count, in: workspaceBounds),
          path: "root")
      } else {
        layoutGrid(visibleSessionIds, in: layoutBounds(forVisibleCount: visibleSessionIds.count, in: workspaceBounds))
      }
    }
    if shouldShowCommandsPanel {
      syncCommandsPanelChrome(
        in: resolvedCommandPanelBounds,
        isExpanded: shouldShowExpandedCommandsPanel,
        isFloating: shouldShowExpandedCommandsPanel && commandsPanelMode == "floating",
        reservedBottomBarHeight: reservedFloatingCommandsPanelBottomBarHeight)
      layoutCommandsPanel(commandSessionIds, in: resolvedCommandPanelBounds)
      syncCommandsPanelResizeHandle(
        in: resolvedCommandPanelBounds,
        isExpanded: shouldShowExpandedCommandsPanel)
    } else {
      hideCommandsPanelChrome()
      hideCommandsPanelResizeHandle()
    }
    updateOuterBottomPaneBorderCorner()
    syncPaneResizeHandleViews()
    window?.invalidateCursorRects(for: self)
  }

  private func hidePaneResizeHandleViews() {
    for handleView in paneResizeHandleViews {
      handleView.isHidden = true
      handleView.frame = .zero
    }
  }

  private func hideCommandsPanelResizeHandle() {
    commandsPanelResizeDrag = nil
    commandsPanelResizeHandleView.isHidden = true
    commandsPanelResizeHandleView.frame = .zero
  }

  private func hideCommandsPanelChrome() {
    commandsPanelChromeView.isHidden = true
    commandsPanelChromeView.frame = .zero
    commandsPanelReservedBottomBarView.isHidden = true
    commandsPanelReservedBottomBarView.frame = .zero
    commandsPanelCollapsedRightMarginView.isHidden = true
    commandsPanelCollapsedRightMarginView.frame = .zero
  }

  private func syncCommandsPanelChrome(
    in commandPanelBounds: CGRect,
    isExpanded: Bool,
    isFloating: Bool,
    reservedBottomBarHeight: CGFloat
  ) {
    commandsPanelChromeView.frame = commandPanelBounds
    commandsPanelChromeView.isHidden = false
    commandsPanelChromeView.layer?.zPosition = isFloating ? 250 : 0
    if commandsPanelChromeView.superview !== self {
      addSubview(commandsPanelChromeView, positioned: .below, relativeTo: nil)
    }
    if let firstCommandSessionId = orderedVisibleCommandSessionIds().first,
      let firstCommandSession = sessions[firstCommandSessionId],
      firstCommandSession.containerView.superview === self
    {
      addSubview(commandsPanelChromeView, positioned: .below, relativeTo: firstCommandSession.containerView)
    } else {
      addSubview(commandsPanelChromeView, positioned: .below, relativeTo: nil)
    }
    syncCommandsPanelCollapsedRightMargin(from: commandPanelBounds, isExpanded: isExpanded)
    syncCommandsPanelReservedBottomBar(height: reservedBottomBarHeight)
  }

  private func syncCommandsPanelReservedBottomBar(height: CGFloat) {
    guard height > 0 else {
      commandsPanelReservedBottomBarView.isHidden = true
      commandsPanelReservedBottomBarView.frame = .zero
      return
    }
    commandsPanelReservedBottomBarView.frame = CGRect(
      x: bounds.minX,
      y: bounds.minY,
      width: bounds.width,
      height: height)
    commandsPanelReservedBottomBarView.isHidden = false
    commandsPanelReservedBottomBarView.layer?.zPosition = 0
    if commandsPanelReservedBottomBarView.superview !== self {
      addSubview(commandsPanelReservedBottomBarView, positioned: .below, relativeTo: nil)
    }
    addSubview(commandsPanelReservedBottomBarView, positioned: .below, relativeTo: commandsPanelChromeView)
  }

  private func syncCommandsPanelCollapsedRightMargin(from commandPanelBounds: CGRect, isExpanded: Bool) {
    guard !isExpanded, commandPanelBounds.maxX < bounds.maxX else {
      commandsPanelCollapsedRightMarginView.isHidden = true
      commandsPanelCollapsedRightMarginView.frame = .zero
      return
    }
    commandsPanelCollapsedRightMarginView.frame = CGRect(
      x: commandPanelBounds.maxX,
      y: commandPanelBounds.minY,
      width: max(0, bounds.maxX - commandPanelBounds.maxX),
      height: commandPanelBounds.height)
    commandsPanelCollapsedRightMarginView.isHidden = false
    commandsPanelCollapsedRightMarginView.layer?.zPosition = commandsPanelChromeView.layer?.zPosition ?? 0
    if commandsPanelCollapsedRightMarginView.superview !== self {
      addSubview(commandsPanelCollapsedRightMarginView, positioned: .below, relativeTo: nil)
    }
    addSubview(commandsPanelCollapsedRightMarginView, positioned: .below, relativeTo: commandsPanelChromeView)
  }

  private func syncCommandsPanelResizeHandle(in commandPanelBounds: CGRect, isExpanded: Bool) {
    guard isExpanded else {
      hideCommandsPanelResizeHandle()
      return
    }
    let railHeight = max(Self.paneResizeRailWidth, 12)
    commandsPanelResizeHandleView.frame = CGRect(
      x: commandPanelBounds.minX,
      y: max(bounds.minY, commandPanelBounds.maxY - railHeight / 2),
      width: commandPanelBounds.width,
      height: min(railHeight, bounds.height)
    )
    commandsPanelResizeHandleView.configure(direction: .vertical, cursor: .resizeUpDown)
    commandsPanelResizeHandleView.isHidden = false
    commandsPanelResizeHandleView.layer?.zPosition = 10_500
    addSubview(commandsPanelResizeHandleView, positioned: .above, relativeTo: nil)
    window?.invalidateCursorRects(for: commandsPanelResizeHandleView)
  }

  private func syncPaneResizeHandleViews() {
    /**
     CDXC:NativePaneResize 2026-05-04-08:41
     AppKit layout must not remove and re-add resize handle views, so layout
     only resizes persistent handles and hides unused ones.
     CDXC:NativePaneResize 2026-05-11-08:41
     Cursor and drag must share one owner. Bind each handle's mouse-down to the
     exact split hit it represents instead of re-hit-testing the workspace point
     after the cursor has already advertised resize.
     CDXC:NativePaneResize 2026-05-11-10:40
     Match native divider behavior in AppKit: every split boundary gets one
     real rail view, and that rail alone owns cursor, mouseDown, mouseDragged,
     and mouseUp for resizing.
     CDXC:NativePaneResize 2026-05-13-07:23
     Pane split rails now follow the stable sidebar divider model: each split
     reserves one real five-pixel AppKit rail between pane containers, and that
     rail alone owns cursor rects plus direct mouse drag delivery.
     */
    while paneResizeHandleViews.count < paneResizeHits.count {
      let handleView = TerminalWorkspacePaneResizeHandleView()
      handleView.onMouseDragged = { [weak self] event in
        _ = self?.continuePaneResize(with: event)
      }
      handleView.onMouseUp = { [weak self] event in
        _ = self?.endPaneResize(with: event)
      }
      paneResizeHandleViews.append(handleView)
    }

    for (index, handleView) in paneResizeHandleViews.enumerated() {
      guard index < paneResizeHits.count else {
        handleView.isHidden = true
        handleView.frame = .zero
        continue
      }
      let hit = paneResizeHits[index]
      handleView.onMouseDown = { [weak self, hit] event in
        _ = self?.beginPaneResize(hit: hit, event: event)
      }
      handleView.frame = paneResizeHandleFrame(for: hit)
      handleView.configure(
        direction: hit.direction,
        cursor: paneResizeCursor(for: hit.direction)
      )
      handleView.isHidden = false
      handleView.layer?.zPosition = 10_000
      handleView.layer?.backgroundColor = NSColor.clear.cgColor
      addSubview(handleView, positioned: .above, relativeTo: nil)
      window?.invalidateCursorRects(for: handleView)
    }
  }

  private func mountTerminalPaneContainer(for session: TerminalSession) {
    /**
     CDXC:NativePaneResize 2026-05-11-13:38
     Native's resize reliability comes from a child/divider/child layout tree,
     not from overlay rails fighting terminal content. Mount each terminal pane
     as one AppKit leaf container so Ghostty, search, title chrome, and borders
     move as a unit while split dividers remain separate sibling views.
     */
    if session.containerView.superview !== self {
      session.containerView.removeFromSuperview()
      addSubview(session.containerView)
    }
    mount(session.titleBarView, in: session.containerView)
    mount(session.scrollView, in: session.containerView)
    mount(session.searchBarView, in: session.containerView)
    mount(session.persistenceLabelView, in: session.containerView)
    mount(session.delayedSendLabelView, in: session.containerView)
    mount(session.borderView, in: session.containerView)
  }

  private func mountWebPaneContainer(for session: WebPaneSession) {
    /**
     CDXC:NativePaneResize 2026-05-11-13:38
     Web panes follow the same native-style leaf container model as Ghostty
     panes. The split divider is a sibling of the pane container, so WKWebView
     or CEF cannot own mouse events that belong to the divider gap.
     */
    if session.containerView.superview !== self {
      session.containerView.removeFromSuperview()
      addSubview(session.containerView)
    }
    mount(session.titleBarView, in: session.containerView)
    mount(session.hostView, in: session.containerView)
    mount(session.borderView, in: session.containerView)
  }

  private func mount(_ view: NSView, in containerView: TerminalPaneLeafContainerView) {
    guard view.superview !== containerView else {
      return
    }
    view.removeFromSuperview()
    containerView.addSubview(view)
  }

  private func updateOuterBottomPaneBorderCorner() {
    /**
     CDXC:NativePaneChrome 2026-05-07-15:13
     The pane touching the workspace's visible outside bottom corner should
     preserve a rounded active/done border corner in native AppKit layout.
     Apply this after split/grid frames are assigned so the rule follows pane
     reorders, split resizing, web panes, terminal panes, and sidebar side.
     */
    var visibleBorders: [(sessionId: String, borderView: TerminalPaneBorderView)] = []
    for (sessionId, session) in sessions where activeSessionIds.contains(sessionId) {
      visibleBorders.append((sessionId: sessionId, borderView: session.borderView))
    }
    for (sessionId, session) in webPaneSessions where activeSessionIds.contains(sessionId) {
      visibleBorders.append((sessionId: sessionId, borderView: session.borderView))
    }

    let roundedSessionId = visibleBorders.max { left, right in
      let leftFrame = left.borderView.convert(left.borderView.bounds, to: self)
      let rightFrame = right.borderView.convert(right.borderView.bounds, to: self)
      let leftOuterX = sidebarSide == .left ? leftFrame.maxX : -leftFrame.minX
      let rightOuterX = sidebarSide == .left ? rightFrame.maxX : -rightFrame.minX
      if abs(leftOuterX - rightOuterX) > 0.5 {
        return leftOuterX < rightOuterX
      }
      if abs(leftFrame.minY - rightFrame.minY) > 0.5 {
        return leftFrame.minY > rightFrame.minY
      }
      return left.sessionId < right.sessionId
    }?.sessionId

    let roundedCorner: TerminalPaneRoundedBottomCorner = sidebarSide == .left ? .right : .left
    for (sessionId, borderView) in visibleBorders {
      borderView.setRoundedBottomCorner(sessionId == roundedSessionId ? roundedCorner : .none)
    }
  }

  private func layoutBounds(forVisibleCount visibleCount: Int) -> CGRect {
    layoutBounds(forVisibleCount: visibleCount, in: bounds)
  }

  private func layoutBounds(forVisibleCount visibleCount: Int, in rect: CGRect) -> CGRect {
    let inset = visibleCount <= 1 ? Self.singlePaneInset : paneGap
    return rect.insetBy(dx: inset, dy: inset)
  }

  private func layoutCommandsPanel(_ sessionIds: [String], in rect: CGRect) {
    guard !sessionIds.isEmpty else {
      return
    }
    let panelBounds = rect.insetBy(
      dx: commandPanelOuterInset(),
      dy: commandPanelVerticalOuterInset())
    if let commandsPanelLayout {
      layoutTree(commandsPanelLayout, in: panelBounds, path: "commands")
    } else {
      layoutGrid(sessionIds, in: panelBounds)
    }
    orderCommandsPanelToFront(sessionIds)
  }

  private func clampedCommandsPanelHeight(_ value: CGFloat) -> CGFloat {
    guard bounds.height > 0 else {
      return 0
    }
    let minimumHeight = bounds.height * Self.minimumCommandsPanelHeightRatio
    let maximumHeight = max(minimumHeight, bounds.height * Self.maximumCommandsPanelHeightRatio)
    return min(max(value, minimumHeight), maximumHeight)
  }

  private func collapsedCommandsPanelHeight() -> CGFloat {
    Self.commandPanelTitleBarHeight + commandPanelVerticalOuterInset() * 2
  }

  private func commandPanelOuterInset() -> CGFloat {
    2 / max(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2, 1)
  }

  private func commandPanelVerticalOuterInset() -> CGFloat {
    /**
     CDXC:PaneTabs 2026-05-15-08:29:
     The command panel keeps a horizontal inset for its side chrome, but vertical
     inset makes the visible tab bar taller than the command tabs. Use zero
     vertical inset so command tabs and their owning bar share one height.
     */
    0
  }

  private func orderCommandsPanelToFront(_ sessionIds: [String]) {
    for sessionId in sessionIds {
      guard let session = sessions[sessionId], session.containerView.superview === self else {
        continue
      }
      addSubview(session.containerView, positioned: .above, relativeTo: nil)
      session.containerView.layer?.zPosition = commandsPanelIsVisible ? 300 : 100
    }
    if commandsPanelChromeView.superview === self, commandsPanelIsVisible {
      commandsPanelChromeView.layer?.zPosition = commandsPanelMode == "floating" ? 250 : 0
    }
  }

  private func applyProjectEditorCompanionPaneHiddenPreference(_ hidden: Bool?, reason: String) {
    guard let hidden else {
      return
    }
    /**
     CDXC:ProjectEditorCompanion 2026-05-16-14:42:
     The agent side pane hidden flag is owned by the sidebar as project state
     and applies to every mode-scoped editor pane for that project. Native must
     honor it during editor creation and layout sync so Code, Git, and Project
     surfaces do not reopen the companion after the user closed it.
     */
    projectEditorCompanionPaneHidden = hidden
    if hidden {
      projectEditorCompanionIsVisible = false
      projectEditorCompanionResizeDrag = nil
      needsLayout = true
      return
    }
    if activeProjectEditorId != nil, !projectEditorCompanionIsVisible {
      openDefaultProjectEditorCompanionPane(reason: reason)
      needsLayout = true
    }
  }

  private func openDefaultProjectEditorCompanionPane(reason: String) {
    guard let sessionId = preferredProjectEditorCompanionSessionId() else {
      projectEditorCompanionSessionId = nil
      projectEditorCompanionIsVisible = false
      return
    }
    projectEditorCompanionSessionId = sessionId
    projectEditorCompanionIsVisible = true
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.companion.default", [
      "reason": reason,
      "sessionId": sessionId,
    ])
  }

  private func syncProjectEditorCompanionSelectionFromSidebar(reason: String) {
    guard activeProjectEditorId != nil else {
      return
    }
    if projectEditorCompanionPaneHidden {
      projectEditorCompanionIsVisible = false
      projectEditorCompanionResizeDrag = nil
      return
    }
    if projectEditorCompanionIsVisible {
      guard let sessionId = preferredProjectEditorCompanionSessionId() else {
        projectEditorCompanionSessionId = nil
        projectEditorCompanionIsVisible = false
        return
      }
      projectEditorCompanionSessionId = sessionId
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.companion.sync", [
        "reason": reason,
        "sessionId": sessionId,
      ])
      return
    }
    if projectEditorCompanionSessionId == nil {
      openDefaultProjectEditorCompanionPane(reason: reason)
    }
  }

  private func preferredProjectEditorCompanionSessionId() -> String? {
    let candidates = [
      focusedSessionId,
      projectEditorCompanionSessionId,
    ].compactMap { $0 }
    for candidate in candidates where isProjectEditorCompanionEligibleSession(candidate) {
      return candidate
    }
    return orderedVisibleSessionIds().first(where: isProjectEditorCompanionEligibleSession)
  }

  private func isProjectEditorCompanionEligibleSession(_ sessionId: String) -> Bool {
    activeSessionIds.contains(sessionId)
      && !commandsPanelActiveSessionIds.contains(sessionId)
      && !sleepingSessionIds.contains(sessionId)
      && !poppedOutSessionIds.contains(sessionId)
      && (sessions[sessionId] != nil || webPaneSessions[sessionId] != nil)
  }

  @discardableResult
  private func activateProjectEditorCompanionPane(
    sessionId: String,
    focus: Bool,
    reason: String
  ) -> Bool {
    let isEligible = isProjectEditorCompanionEligibleSession(sessionId)
    let previousCompanionSessionId = projectEditorCompanionSessionId
    let wasCompanionVisible = projectEditorCompanionIsVisible
    if activeProjectEditorId != nil || reason == "sidebarFocusCommand" {
      /**
       CDXC:SidebarSessionFocus 2026-05-15-17:25:
       The sidebar focus miss reproduces inside Code mode's companion-pane path.
       Log eligibility before returning so a repro shows whether focus never
       entered the companion branch or entered it but AppKit kept WebKit as first
       responder.
       */
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.projectEditorCompanionFocusAttempt",
        details: [
          "activeProjectEditorId": nullableString(activeProjectEditorId),
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "focus": focus,
          "isEligible": isEligible,
          "knownTerminalSession": sessions[sessionId] != nil,
          "knownWebPaneSession": webPaneSessions[sessionId] != nil,
          "poppedOutSessionIds": Array(poppedOutSessionIds).sorted(),
          "reason": reason,
          "requestedSessionId": sessionId,
          "responderBeforeEligibility": responderSnapshot(),
          "sleepingSessionIds": Array(sleepingSessionIds).sorted(),
          "visibleSessionIds": orderedVisibleSessionIds(),
        ])
    }
    guard activeProjectEditorId != nil, isEligible else {
      return false
    }
    /*
     CDXC:ProjectEditorCompanion 2026-05-23-13:50:
     When the Code/Git/Project companion pane is hidden, sidebar session clicks must leave the project-editor workarea and focus the clicked session in Agents view. Fall through to the normal focusTerminal/focusWebPane path instead of retargeting an invisible companion pane.
     */
    if projectEditorCompanionPaneHidden {
      return false
    }
    /**
     CDXC:ProjectEditorCompanion 2026-05-14-09:19:
     Sidebar session clicks while VS Code is active should restore or retarget
     the left companion pane instead of returning to the normal agents workarea.
     This includes clicks on the already-focused sidebar session, because the
     user may have closed the companion pane locally and wants that same session
     visible again.
     */
    projectEditorCompanionSessionId = sessionId
    projectEditorCompanionIsVisible = true
    focusedSessionId = sessionId
    needsLayout = true
    layoutSubtreeIfNeeded()
    updateAllTerminalBorders()

    guard focus else {
      return true
    }
    let targetResponder: NSResponder?
    if let session = sessions[sessionId] {
      targetResponder = session.view
    } else if let session = webPaneSessions[sessionId] {
      targetResponder = session.browserContentView
    } else {
      targetResponder = nil
    }
    let responderBeforeFocus = responderSnapshot()
    let makeFirstResponderResult: Bool
    if let targetResponder {
      makeFirstResponderResult = window?.makeFirstResponder(targetResponder) ?? false
    } else {
      makeFirstResponderResult = false
    }
    let responderAfterFocus = responderSnapshot()
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.projectEditorCompanionFocusResult",
      details: [
        "activeProjectEditorId": nullableString(activeProjectEditorId),
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "focusedSessionId": nullableString(focusedSessionId),
        "makeFirstResponderResult": makeFirstResponderResult,
        "reason": reason,
        "requestedSessionId": sessionId,
        "responderAfterFocus": responderAfterFocus,
        "responderBeforeFocus": responderBeforeFocus,
        "targetResponderClass": targetResponder.map { String(describing: type(of: $0)) } ?? "nil",
        "visibleSessionIds": orderedVisibleSessionIds(),
        "windowIsKey": window?.isKeyWindow ?? false,
      ])
    if reason == "sidebarFocusCommand" {
      scheduleDelayedProjectEditorCompanionClick(sessionId: sessionId)
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.companion.focus", [
      "reason": reason,
      "sessionId": sessionId,
    ])
    sendEvent(.terminalFocused(sessionId: sessionId))
    /*
     CDXC:ZmxPersistenceRefresh 2026-05-18-09:04:
     Code-mode sidebar session switches surface terminals through the project-editor companion branch, which returns before the ordinary focusTerminal refresh hook.
     Refresh zmx after the companion pane has been retargeted and focused so broken persisted terminal text is corrected without manual terminal input.
     */
    if sessions[sessionId] != nil {
      refreshZmxPersistenceTerminalIfFocusOrSurfaceChanged(
        sessionId: sessionId,
        previousSessionId: previousCompanionSessionId,
        didSurface: !wasCompanionVisible || previousCompanionSessionId != sessionId,
        reason: "projectEditorCompanion.\(reason)")
    }
    return true
  }

  private func scheduleDelayedProjectEditorCompanionClick(sessionId: String) {
    /**
     CDXC:SidebarSessionFocus 2026-05-15-17:41:
     WebKit can keep the sidebar as first responder after a sidebar card click
     even when the immediate companion-pane focus call succeeds. For
     sidebar-originated Code-mode session switches, replay the native equivalent
     of clicking the selected companion pane after the sidebar click has fully
     settled so typing lands in the terminal or web pane without a manual second
     click.
     Send the synthetic mouse event through the NSWindow event path so normal
     hit-testing reaches the focused terminal or web pane child view instead of
     calling the companion container directly.
     */
    DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(180)) { [weak self] in
      self?.performDelayedProjectEditorCompanionClick(sessionId: sessionId)
    }
  }

  private func performDelayedProjectEditorCompanionClick(sessionId: String) {
    guard
      activeProjectEditorId != nil,
      projectEditorCompanionIsVisible,
      projectEditorCompanionSessionId == sessionId,
      let targetView = projectEditorCompanionFocusTargetView(sessionId: sessionId),
      let targetWindow = targetView.window
    else {
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.projectEditorCompanionDelayedClickSkipped",
        details: [
          "activeProjectEditorId": nullableString(activeProjectEditorId),
          "companionSessionId": nullableString(projectEditorCompanionSessionId),
          "companionVisible": projectEditorCompanionIsVisible,
          "hasTargetView": projectEditorCompanionFocusTargetView(sessionId: sessionId) != nil,
          "requestedSessionId": sessionId,
          "responder": responderSnapshot(),
        ])
      return
    }

    let windowPoint = targetView.convert(
      CGPoint(x: targetView.bounds.midX, y: targetView.bounds.midY),
      to: nil)
    let responderBeforeClick = responderSnapshot()
    let makeFirstResponderResult = targetWindow.makeFirstResponder(targetView)
    if let mouseDown = syntheticCompanionMouseEvent(
      type: .leftMouseDown,
      location: windowPoint,
      window: targetWindow,
      clickCount: 1)
    {
      targetWindow.sendEvent(mouseDown)
    }
    if let mouseUp = syntheticCompanionMouseEvent(
      type: .leftMouseUp,
      location: windowPoint,
      window: targetWindow,
      clickCount: 1)
    {
      targetWindow.sendEvent(mouseUp)
    }
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.projectEditorCompanionDelayedClickApplied",
      details: [
        "focusedSessionId": nullableString(focusedSessionId),
        "makeFirstResponderResult": makeFirstResponderResult,
        "requestedSessionId": sessionId,
        "responderAfterClick": responderSnapshot(),
        "responderBeforeClick": responderBeforeClick,
        "targetFrame": describeFrame(targetView.frame),
        "targetViewClass": String(describing: type(of: targetView)),
        "windowIsKey": targetWindow.isKeyWindow,
        "windowPointX": Double(windowPoint.x),
        "windowPointY": Double(windowPoint.y),
      ])
  }

  private func projectEditorCompanionFocusTargetView(sessionId: String) -> NSView? {
    if let session = sessions[sessionId] {
      return session.view
    }
    if let session = webPaneSessions[sessionId] {
      return session.browserContentView
    }
    return nil
  }

  private func syntheticCompanionMouseEvent(
    type: NSEvent.EventType,
    location: CGPoint,
    window: NSWindow,
    clickCount: Int
  ) -> NSEvent? {
    NSEvent.mouseEvent(
      with: type,
      location: location,
      modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime,
      windowNumber: window.windowNumber,
      context: nil,
      eventNumber: 0,
      clickCount: clickCount,
      pressure: 1
    )
  }

  private func projectEditorCompanionLayout(in workspaceBounds: CGRect) -> ProjectEditorCompanionLayout? {
    guard
      projectEditorCompanionIsVisible,
      let sessionId = projectEditorCompanionSessionId,
      isProjectEditorCompanionEligibleSession(sessionId),
      workspaceBounds.width > Self.paneResizeRailWidth + 1,
      workspaceBounds.height > Self.terminalTitleBarHeight + 1
    else {
      return nil
    }
    let companionWidth = clampedProjectEditorCompanionWidth(
      workspaceBounds.width * projectEditorCompanionWidthRatio,
      in: workspaceBounds)
    guard companionWidth > 1 else {
      return nil
    }
    let railWidth = min(Self.paneResizeRailWidth, max(workspaceBounds.width - companionWidth, 0))
    let companionFrame = CGRect(
      x: workspaceBounds.minX,
      y: workspaceBounds.minY,
      width: companionWidth,
      height: workspaceBounds.height)
    let resizeHandleFrame = CGRect(
      x: companionFrame.maxX,
      y: workspaceBounds.minY,
      width: railWidth,
      height: workspaceBounds.height)
    let editorFrame = CGRect(
      x: resizeHandleFrame.maxX,
      y: workspaceBounds.minY,
      width: max(0, workspaceBounds.maxX - resizeHandleFrame.maxX),
      height: workspaceBounds.height)
    /**
     CDXC:ProjectEditorCompanion 2026-05-14-09:47:
     Companion controls belong in the existing session titlebar
     row. Do not reserve a separate header strip above
     the terminal content; that strip was visually detached from the actual
     clickable titlebar surface.

     CDXC:ProjectEditorCompanion 2026-05-15-15:29:
     The companion titlebar control group contains Close only; the former Back
     to Agents View button is not reserved in the code/git companion layout.
     */
    let contentFrame = companionFrame
    return ProjectEditorCompanionLayout(
      companionFrame: companionFrame,
      contentFrame: contentFrame,
      editorFrame: editorFrame,
      resizeHandleFrame: resizeHandleFrame,
      sessionId: sessionId)
  }

  private func syncProjectEditorCompanionPane(layout: ProjectEditorCompanionLayout?) {
    guard let layout else {
      hideProjectEditorCompanionChrome()
      return
    }
    syncProjectEditorCompanionTitleBarControls(activeSessionId: layout.sessionId)
    setPaneTabs([], activeSessionId: layout.sessionId, on: layout.sessionId)
    setFrame(layout.contentFrame, for: layout.sessionId)

    projectEditorCompanionResizeHandleView.frame = layout.resizeHandleFrame
    projectEditorCompanionResizeHandleView.configure(direction: .horizontal, cursor: .resizeLeftRight)
    projectEditorCompanionResizeHandleView.isHidden = false
    projectEditorCompanionResizeHandleView.layer?.zPosition = 10_600
    addSubview(projectEditorCompanionResizeHandleView, positioned: .above, relativeTo: nil)
    window?.invalidateCursorRects(for: projectEditorCompanionResizeHandleView)
  }

  private func syncProjectEditorCompanionTitleBarControls(activeSessionId: String?) {
    /**
     CDXC:ProjectEditorCompanion 2026-05-14-09:47:
     The companion pane's local controls must live inside the selected session
     titlebar, so hover and click handling use the same AppKit button path as
     existing titlebar actions.
     Clear the controls from every non-companion titlebar when the companion is
     hidden or retargeted.

     CDXC:ProjectEditorCompanion 2026-05-15-15:29:
     Code/git view companion panes should no longer show the Back to Agents View
     button. Keep only the close control in the selected companion titlebar so
     the left pane can be dismissed without offering a workarea-mode switch.
     */
    for session in sessions.values {
      configureProjectEditorCompanionTitleBarControls(
        on: session.titleBarView,
        sessionId: session.sessionId,
        activeSessionId: activeSessionId)
    }
    for session in webPaneSessions.values {
      configureProjectEditorCompanionTitleBarControls(
        on: session.titleBarView,
        sessionId: session.sessionId,
        activeSessionId: activeSessionId)
    }
  }

  private func configureProjectEditorCompanionTitleBarControls(
    on titleBarView: TerminalSessionTitleBarView,
    sessionId: String,
    activeSessionId: String?
  ) {
    guard activeSessionId == sessionId else {
      titleBarView.setProjectEditorCompanionControls(onClose: nil)
      return
    }
    titleBarView.setProjectEditorCompanionControls(
      onClose: { [weak self] in
        self?.closeProjectEditorCompanionPane()
      })
  }

  private func hideProjectEditorCompanionChrome() {
    projectEditorCompanionResizeDrag = nil
    syncProjectEditorCompanionTitleBarControls(activeSessionId: nil)
    projectEditorCompanionResizeHandleView.isHidden = true
    projectEditorCompanionResizeHandleView.frame = .zero
  }

  private func clampedProjectEditorCompanionWidth(_ value: CGFloat, in workspaceBounds: CGRect) -> CGFloat {
    let railWidth = Self.paneResizeRailWidth
    let availableWidth = max(workspaceBounds.width - railWidth, 0)
    guard availableWidth > 0 else {
      return 0
    }
    let minimumWidth = min(Self.paneResizeMinimumWidth, availableWidth)
    let maximumWidth = max(
      minimumWidth,
      availableWidth - min(Self.projectEditorMinimumWidth, availableWidth * 0.5))
    return min(max(value, minimumWidth), maximumWidth)
  }

  private static func normalizedProjectEditorCompanionWidthRatio(_ value: CGFloat?) -> CGFloat {
    guard let value, value.isFinite else {
      return defaultProjectEditorCompanionWidthRatio
    }
    return min(max(value, 0.05), 0.9)
  }

  private func closeProjectEditorCompanionPane() {
    projectEditorCompanionPaneHidden = true
    projectEditorCompanionIsVisible = false
    projectEditorCompanionResizeDrag = nil
    if let activeProjectEditorId {
      sendEvent(.projectEditorCompanionPaneHiddenChanged(projectId: activeProjectEditorId, hidden: true))
    }
    needsLayout = true
    layoutSubtreeIfNeeded()
    /*
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     Closing the Code/Git/Project companion pane resizes the editor surface and removes the visible terminal pane through local native state before sidebar sync returns.
     Schedule a surfaced-only zmx refresh so any remaining visible zmx command pane is repaired without targeting the hidden companion terminal.
     */
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "projectEditorCompanionClosed")
  }

  @discardableResult
  private func beginProjectEditorCompanionResize(with event: NSEvent) -> Bool {
    let workspaceBounds = projectEditorCompanionResizeWorkspaceBounds
    guard let layout = projectEditorCompanionLayout(in: workspaceBounds) else {
      return false
    }
    if event.clickCount >= 2 {
      projectEditorCompanionResizeDrag = nil
      projectEditorCompanionWidthRatio = Self.defaultProjectEditorCompanionWidthRatio
      needsLayout = true
      layoutSubtreeIfNeeded()
      persistProjectEditorCompanionWidthRatio(projectEditorCompanionWidthRatio)
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "projectEditorCompanionResizeReset")
      NSCursor.resizeLeftRight.set()
      return true
    }
    let point = convert(event.locationInWindow, from: nil)
    projectEditorCompanionResizeDrag = ProjectEditorCompanionResizeDrag(
      startWidth: layout.companionFrame.width,
      startX: point.x,
      workspaceBounds: workspaceBounds)
    NSCursor.resizeLeftRight.set()
    return true
  }

  @discardableResult
  private func continueProjectEditorCompanionResize(with event: NSEvent) -> Bool {
    guard let drag = projectEditorCompanionResizeDrag else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)
    let nextWidth = clampedProjectEditorCompanionWidth(
      drag.startWidth + point.x - drag.startX,
      in: drag.workspaceBounds)
    projectEditorCompanionWidthRatio = Self.normalizedProjectEditorCompanionWidthRatio(
      nextWidth / max(drag.workspaceBounds.width, 1))
    NSCursor.resizeLeftRight.set()
    needsLayout = true
    layoutSubtreeIfNeeded()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "projectEditorCompanionResizeDrag")
    return true
  }

  @discardableResult
  private func endProjectEditorCompanionResize(with event: NSEvent) -> Bool {
    guard projectEditorCompanionResizeDrag != nil else {
      return false
    }
    _ = continueProjectEditorCompanionResize(with: event)
    projectEditorCompanionResizeDrag = nil
    persistProjectEditorCompanionWidthRatio(projectEditorCompanionWidthRatio)
    NSCursor.resizeLeftRight.set()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "projectEditorCompanionResizeEnd")
    return true
  }

  private func keepCommandsPanelAboveWorkspacePanes() {
    guard commandsPanelIsVisible, !commandsPanelActiveSessionIds.isEmpty else {
      return
    }
    orderCommandsPanelToFront(orderedVisibleCommandSessionIds())
  }

  private func chromiumBackingPixelAlignedFrame(_ rect: CGRect) -> CGRect {
    guard rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0 else {
      return rect
    }
    let scale = max(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1, 1)
    func align(_ value: CGFloat) -> CGFloat {
      (value * scale).rounded() / scale
    }
    let minX = align(rect.minX)
    let maxX = align(rect.maxX)
    let minY = align(rect.minY)
    let maxY = align(rect.maxY)
    /**
     CDXC:ChromiumBrowserPanes 2026-05-15-18:35:
     The 18:29 bottom command-pane height repro showed native CEF frames staying aligned while Chromium's hosted compositor layer grew taller than the render widget after fractional AppKit y/height changes.
     Keep Chromium-hosted pane edges on backing-pixel boundaries so AppKit and CEF receive stable screen geometry during vertical splitter drags instead of subpixel pane origins.
     */
    return CGRect(
      x: minX,
      y: minY,
      width: max(0, maxX - minX),
      height: max(0, maxY - minY)
    )
  }

  private func projectEditorPaneFrames(
    _ session: ProjectEditorPaneSession,
    in rect: CGRect
  ) -> (titleBarFrame: CGRect, hostFrame: CGRect) {
    let nextFrame = chromiumBackingPixelAlignedFrame(rect)
    let titleBarHeight =
      session.showsProjectTabs && session.titleBarView != nil
      ? min(Self.terminalTitleBarHeight, max(nextFrame.height, 0))
      : 0
    let titleBarFrame = CGRect(
      x: nextFrame.minX,
      y: nextFrame.maxY - titleBarHeight,
      width: nextFrame.width,
      height: titleBarHeight
    )
    let hostFrame = CGRect(
      x: nextFrame.minX,
      y: nextFrame.minY,
      width: nextFrame.width,
      height: max(0, nextFrame.height - titleBarHeight)
    )
    return (titleBarFrame: titleBarFrame, hostFrame: hostFrame)
  }

  private func layoutProjectEditorPane(
    _ session: ProjectEditorPaneSession,
    in rect: CGRect? = nil,
    chromiumLiveResizeBackingRect: CGRect? = nil
  ) {
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.layout.start", [
      "hostFrameBefore": describeFrame(session.hostView.frame),
      "mode": session.mode,
      "projectId": session.projectId,
      "showsProjectTabs": session.showsProjectTabs,
      "url": session.url,
      "workspaceBounds": describeFrame(bounds),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    let frames = projectEditorPaneFrames(session, in: rect ?? bounds)
    let titleBarFrame = frames.titleBarFrame
    let hostFrame = frames.hostFrame
    let chromiumLiveResizeBackingHostHeight = chromiumLiveResizeBackingRect.map {
      projectEditorPaneFrames(session, in: $0).hostFrame.height
    }
    if let titleBarView = session.titleBarView {
      /**
       CDXC:GitProjectTabs 2026-05-16-07:42:
       Git project views should show the same AppKit tab strip used by the
       Agents workarea above the existing browser address toolbar. Keep this
       tab row outside WebPaneHostView so browser navigation chrome remains the
       same component normal browser panes use.
      */
      titleBarView.frame = titleBarFrame
      titleBarView.isHidden = titleBarFrame.height <= 0
    }
    /**
     CDXC:EditorPanes 2026-05-08-13:37
     Sidebar resize must not synchronously refresh or display the hosted VS
     Code CEF view from inside TerminalWorkspaceView.layout(). Resizing only
     moves the host frame; WebPaneHostView.layout owns the child Chromium frame
     on the normal AppKit pass. Do not mark the host as needing layout from
     this layout method, because that self-invalidates and can loop when the
     project editor is the active workspace surface.
     */
    for hostView in projectEditorHostViews(session) {
      hostView.chromiumLiveResizeBackingHeight = chromiumLiveResizeBackingHostHeight
      if hostView.frame != hostFrame {
        hostView.frame = hostFrame
      }
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.layout.end", [
      "hostFrameAfter": describeFrame(session.hostView.frame),
      "mode": session.mode,
      "projectId": session.projectId,
      "titleBarFrame": describeFrame(session.titleBarView?.frame ?? .zero),
      "url": session.url,
      "workspaceBounds": describeFrame(bounds),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  private func rectsMatch(_ left: CGRect, _ right: CGRect) -> Bool {
    let epsilon: CGFloat = 0.5
    return abs(left.minX - right.minX) <= epsilon
      && abs(left.minY - right.minY) <= epsilon
      && abs(left.width - right.width) <= epsilon
      && abs(left.height - right.height) <= epsilon
  }

  private func orderProjectEditorPaneToFront(_ session: ProjectEditorPaneSession) {
    if let titleBarView = session.titleBarView, titleBarView.superview !== self {
      addSubview(titleBarView)
    }
    if session.hostView.superview !== self {
      addSubview(session.hostView)
      if let titleBarView = session.titleBarView {
        addSubview(titleBarView, positioned: .above, relativeTo: session.hostView)
      }
      return
    }
    guard shouldRaiseProjectEditorHost(session.hostView) else {
      if let titleBarView = session.titleBarView, shouldRaiseProjectEditorHost(titleBarView) {
        titleBarView.removeFromSuperview()
        addSubview(titleBarView, positioned: .above, relativeTo: session.hostView)
      }
      return
    }
    /**
     CDXC:EditorPanes 2026-05-08-13:37
     Reordering the active VS Code host by remove/add invalidates AppKit
     layout. Only move it when another workspace surface is actually above it;
     doing this on every layout pass creates a self-sustaining layout loop.
     CDXC:ProjectEditorCompanion 2026-05-14-09:19:
     The companion pane chrome, resize rail, and left session pane are expected
     above or beside the editor. Do not remove/re-add the Chromium host for
     those non-overlapping AppKit siblings, because that visibly refreshes the
     VS Code embed during sidebar session switches.
     CDXC:ProjectEditorCompanion 2026-05-14-09:40:
     Switching the sidebar-selected companion session must leave the VS Code
     CEF host mounted and ordered in place. Only raise the editor when a real
     overlapping workspace surface sits above it.
    */
    session.hostView.removeFromSuperview()
    addSubview(session.hostView, positioned: .above, relativeTo: nil)
    if let titleBarView = session.titleBarView {
      titleBarView.removeFromSuperview()
      addSubview(titleBarView, positioned: .above, relativeTo: session.hostView)
    }
  }

  private func shouldRaiseProjectEditorHost(_ hostView: NSView) -> Bool {
    guard let hostIndex = subviews.firstIndex(of: hostView) else {
      return true
    }
    let hostFrame = hostView.frame
    for view in subviews[(hostIndex + 1)...] {
      guard !isExpectedProjectEditorOverlayView(view) else {
        continue
      }
      guard !view.isHidden, view.alphaValue > 0, view.frame.intersects(hostFrame) else {
        continue
      }
      return true
    }
    return false
  }

  private func isExpectedProjectEditorOverlayView(_ view: NSView) -> Bool {
    if view === projectEditorCompanionResizeHandleView
      || view === commandsPanelChromeView
      || view === commandsPanelResizeHandleView
      || view === commandsPanelCollapsedRightMarginView
      || paneResizeHandleViews.contains(where: { $0 === view })
    {
      return true
    }
    if let floatingEditorOverlayView, view === floatingEditorOverlayView {
      return true
    }
    if let companionSessionId = projectEditorCompanionSessionId {
      if sessions[companionSessionId]?.containerView === view
        || webPaneSessions[companionSessionId]?.containerView === view
      {
        return true
      }
    }
    for sessionId in commandsPanelActiveSessionIds {
      if sessions[sessionId]?.containerView === view {
        return true
      }
    }
    return false
  }

  private func hideSplitSessionSurfacesForActiveEditor() {
    paneResizeDrag = nil
    /**
     CDXC:CommandsPanel 2026-05-15-10:00
     Command-pane tab drags remain valid while a project editor is active,
     because command terminals float above the editor and keep their own
     interactive titlebar. Do not let the editor layout branch clear an
     in-flight command-tab drag before mouse-up can commit the split/drop.
     */
    if paneHeaderDrag.map({ commandsPanelActiveSessionIds.contains($0.sourceSessionId) }) != true {
      resetPaneHeaderInteractionState()
    }
    for session in sessions.values {
      if commandsPanelActiveSessionIds.contains(session.sessionId) {
        continue
      }
      moveOffscreen(session.containerView)
    }
    for session in webPaneSessions.values {
      moveOffscreen(session.containerView)
    }
    hidePaneResizeHandleViews()
    discardCursorRects()
  }

  private func handleProjectBeadsBridgeRequest(
    _ request: ProjectBeadsBridgeRequest,
    webView: WKWebView?
  ) {
    /**
     CDXC:ProjectBoard 2026-05-23-03:16:
     The Project board runs inside WKWebView and must persist work through the upstream Beads CLI, not a forked library or custom storage.
     Only expose the exact bd subcommands the board needs so the local web app can list, create, update, and comment on issues without gaining arbitrary shell access.

     CDXC:ProjectBoard 2026-05-26-10:08:
     Project board ticket deletion must route through Beads deletion so dependencies, labels, events, and deletion manifests stay consistent with bd CLI behavior.
     Keep the bridge allowlist explicit and require a concrete issue id before running the destructive command.
     */
    guard let webView else {
      return
    }
    DispatchQueue.global(qos: .userInitiated).async { [weak webView] in
      let response = Self.runProjectBeadsBridgeRequest(request)
      DispatchQueue.main.async {
        guard let webView else {
          return
        }
        Self.dispatchProjectBeadsBridgeResponse(response, to: webView)
      }
    }
  }

  private static func runProjectBeadsBridgeRequest(
    _ request: ProjectBeadsBridgeRequest
  ) -> ProjectBeadsBridgeResponse {
    do {
      let cwd = try projectBeadsWorkingDirectory(request.cwd)
      if request.action == "generateTitle" {
        let title = try projectBeadsGenerateTitle(
          cwd: cwd,
          prompt: try projectBeadsRequired(request.prompt, field: "prompt"))
        let payload = try JSONSerialization.data(withJSONObject: ["title": title])
        return ProjectBeadsBridgeResponse(
          error: nil,
          exitCode: 0,
          requestId: request.requestId,
          stderr: "",
          stdout: String(data: payload, encoding: .utf8) ?? "")
      }
      if request.action == "listIssues" {
        let issues = try projectBeadsReadIssuesJsonl(cwd: cwd)
        let payload = try JSONSerialization.data(withJSONObject: issues)
        return ProjectBeadsBridgeResponse(
          error: nil,
          exitCode: 0,
          requestId: request.requestId,
          stderr: "",
          stdout: String(data: payload, encoding: .utf8) ?? "[]")
      }
      let arguments = try projectBeadsArguments(for: request)
      if arguments.isEmpty {
        throw ProjectBeadsBridgeError.invalidRequest("Unsupported Beads action: \(request.action)")
      }
      let stdoutPipe = Pipe()
      let stderrPipe = Pipe()
      let stdoutCollector = projectBeadsPipeCollector(stdoutPipe)
      let stderrCollector = projectBeadsPipeCollector(stderrPipe)
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
      process.arguments = ["bd"] + arguments
      process.currentDirectoryURL = cwd
      var environment = projectBoardNativeProcessEnvironment()
      environment["BD_JSON_ENVELOPE"] = "1"
      process.environment = environment
      process.standardOutput = stdoutPipe
      process.standardError = stderrPipe
      try process.run()
      process.waitUntilExit()
      return ProjectBeadsBridgeResponse(
        error: nil,
        exitCode: process.terminationStatus,
        requestId: request.requestId,
        stderr: stderrCollector(),
        stdout: stdoutCollector())
    } catch {
      return ProjectBeadsBridgeResponse(
        error: error.localizedDescription,
        exitCode: 127,
        requestId: request.requestId,
        stderr: error.localizedDescription,
        stdout: "")
    }
  }

  private static func projectBeadsArguments(for request: ProjectBeadsBridgeRequest) throws -> [String] {
    switch request.action {
    case "addComment":
      return [
        "comments", "add",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        try projectBeadsRequired(request.comment, field: "comment"),
        "--json",
      ]
    case "configGet":
      return ["config", "get", "status.custom", "--json"]
    case "configSet":
      return ["config", "set", "status.custom", try projectBeadsRequired(request.value, field: "value"), "--json"]
    case "create":
      var createArguments = [
        "create",
        "--title", try projectBeadsRequired(request.title, field: "title"),
        "--description", request.description ?? "",
        "--priority", request.priority ?? "2",
        "--type", "task",
      ]
      if let estimate = request.estimate {
        createArguments.append(contentsOf: ["--estimate", String(estimate)])
      }
      if let labels = request.labels, !labels.isEmpty {
        createArguments.append(contentsOf: ["--labels", labels.joined(separator: ",")])
      }
      if let dependsOnId = request.dependsOnId?.trimmingCharacters(in: .whitespacesAndNewlines),
        !dependsOnId.isEmpty
      {
        let depType = request.depType?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "blocks"
        createArguments.append(contentsOf: ["--deps", "\(depType):\(dependsOnId)"])
      }
      createArguments.append("--json")
      return createArguments
    case "delete":
      return [
        "delete",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--force",
        "--json",
      ]
    case "list":
      return ["list", "--all", "--json"]
    case "listIssues":
      return []
    case "show":
      return ["show", try projectBeadsRequired(request.issueId, field: "issueId"), "--json"]
    case "updateDescription":
      return [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--description", request.description ?? "",
        "--json",
      ]
    case "updateStatus":
      let status = try projectBeadsRequired(request.status, field: "status")
      guard ["closed", "in_progress", "open", "review", "test"].contains(status) else {
        throw ProjectBeadsBridgeError.invalidRequest("Unsupported Beads status: \(status)")
      }
      return [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--status", status,
        "--json",
      ]
    case "updateTitle":
      return [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--title", try projectBeadsRequired(request.title, field: "title"),
        "--json",
      ]
    case "updatePriority":
      return [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--priority", try projectBeadsRequired(request.priority, field: "priority"),
        "--json",
      ]
    case "updateEstimate":
      guard let estimate = request.estimate else {
        throw ProjectBeadsBridgeError.invalidRequest("Missing required Beads field: estimate")
      }
      return [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        "--estimate", String(estimate),
        "--json",
      ]
    case "setLabels":
      var arguments = [
        "update",
        try projectBeadsRequired(request.issueId, field: "issueId"),
      ]
      if let labels = request.labels, !labels.isEmpty {
        for label in labels {
          arguments.append(contentsOf: ["--set-labels", label])
        }
      }
      arguments.append("--json")
      return arguments
    case "addLabel":
      return [
        "label", "add",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        try projectBeadsRequired(request.label, field: "label"),
        "--json",
      ]
    case "removeLabel":
      return [
        "label", "remove",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        try projectBeadsRequired(request.label, field: "label"),
        "--json",
      ]
    case "listAllLabels":
      return ["label", "list-all", "--json"]
    case "depAdd":
      let depType = request.depType?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "blocks"
      return [
        "dep", "add",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        try projectBeadsRequired(request.dependsOnId, field: "dependsOnId"),
        "--type", depType,
        "--json",
      ]
    case "depRemove":
      return [
        "dep", "remove",
        try projectBeadsRequired(request.issueId, field: "issueId"),
        try projectBeadsRequired(request.dependsOnId, field: "dependsOnId"),
        "--json",
      ]
    case "search":
      return [
        "search",
        try projectBeadsRequired(request.query, field: "query"),
        "--json",
      ]
    case "configGetIssuePrefix":
      return ["config", "get", "issue_prefix", "--json"]
    case "configSetIssuePrefix":
      return [
        "config", "set", "issue_prefix",
        try projectBeadsRequired(request.value, field: "value"),
        "--json",
      ]
    default:
      throw ProjectBeadsBridgeError.invalidRequest("Unsupported Beads action: \(request.action)")
    }
  }

  private static func projectBeadsReadIssuesJsonl(cwd: URL) throws -> [[String: Any]] {
    let jsonlURL = cwd.appendingPathComponent(".beads/issues.jsonl")
    guard FileManager.default.fileExists(atPath: jsonlURL.path) else {
      return []
    }
    let contents = try String(contentsOf: jsonlURL, encoding: .utf8)
    var issues: [[String: Any]] = []
    for line in contents.split(whereSeparator: \.isNewline) {
      let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.isEmpty,
        let data = trimmed.data(using: .utf8),
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
      else {
        continue
      }
      issues.append(object)
    }
    return issues
  }

  private static func projectBeadsGenerateTitle(cwd: URL, prompt: String) throws -> String {
    /**
     CDXC:ProjectBoard 2026-05-23-14:18:
     Ticket title autogeneration must reuse the same Codex summarization policy as native session first-prompt naming so empty ticket titles stay consistent across Ghostex surfaces.
     */
    let sourceText = String(prompt.prefix(4_000))
    let generationPrompt = """
    Write a concise session title that summarizes the user's text.
    Return plain text only.
    Rules:
    - keep it specific and scannable
    - prefer 2 to 4 words when possible
    - must be fewer than 40 characters
    - do not abbreviate with ellipses
    - do not use quotes, markdown, or commentary
    - do not end with punctuation
    - focus on the task, bug, feature, or topic

    User text:
    \(sourceText)

    Output handling:
    - Produce only the final session title.
    - Do not wrap the result in backticks.
    - Print only the final result to stdout.
    """
    let delimiter = "ghostex_SESSION_TITLE_\(Int(Date().timeIntervalSince1970))"
    let command = """
    codex exec --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort="low"' - <<'\(delimiter)'
    \(generationPrompt)
    \(delimiter)
    """
    let stdoutPipe = Pipe()
    let stderrPipe = Pipe()
    let stdoutCollector = projectBeadsPipeCollector(stdoutPipe)
    let stderrCollector = projectBeadsPipeCollector(stderrPipe)
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = ["-lc", command]
    process.currentDirectoryURL = cwd
    process.environment = projectBoardNativeProcessEnvironment()
    process.standardOutput = stdoutPipe
    process.standardError = stderrPipe
    try process.run()
    process.waitUntilExit()
    if process.terminationStatus != 0 {
      let stderr = stderrCollector().trimmingCharacters(in: .whitespacesAndNewlines)
      throw ProjectBeadsBridgeError.invalidRequest(
        stderr.isEmpty ? "Codex title generation failed." : stderr)
    }
    let stdout = stdoutCollector().trimmingCharacters(in: .whitespacesAndNewlines)
    guard let line = stdout.split(whereSeparator: \.isNewline).map(String.init).first(where: { !$0.isEmpty })
    else {
      throw ProjectBeadsBridgeError.invalidRequest("Codex title generation returned an empty title.")
    }
    let sanitized =
      line
      .trimmingCharacters(in: .whitespacesAndNewlines)
      .replacingOccurrences(of: #"^["'`]+|["'`]+$"#, with: "", options: .regularExpression)
      .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
      .replacingOccurrences(of: #"[.…]+$"#, with: "", options: .regularExpression)
    guard !sanitized.isEmpty else {
      throw ProjectBeadsBridgeError.invalidRequest("Codex title generation returned an empty title.")
    }
    return String(sanitized.prefix(39))
  }

  private static func projectBeadsRequired(_ value: String?, field: String) throws -> String {
    let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else {
      throw ProjectBeadsBridgeError.invalidRequest("Missing required Beads field: \(field)")
    }
    return trimmed
  }

  private static func projectBeadsWorkingDirectory(_ path: String) throws -> URL {
    let trimmedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedPath.isEmpty else {
      throw ProjectBeadsBridgeError.invalidRequest("No active project path is available.")
    }
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: trimmedPath, isDirectory: &isDirectory),
      isDirectory.boolValue
    else {
      throw ProjectBeadsBridgeError.invalidRequest("Project path does not exist: \(trimmedPath)")
    }
    return URL(fileURLWithPath: trimmedPath, isDirectory: true)
  }

  private static func projectBeadsPipeCollector(_ pipe: Pipe) -> () -> String {
    let queue = DispatchQueue(label: "app.ghostex.project-beads-pipe.\(UUID().uuidString)")
    var data = Data()
    let handle = pipe.fileHandleForReading
    handle.readabilityHandler = { readableHandle in
      let chunk = readableHandle.availableData
      guard !chunk.isEmpty else {
        return
      }
      queue.sync {
        data.append(chunk)
      }
    }
    return {
      handle.readabilityHandler = nil
      let remainingData = handle.readDataToEndOfFile()
      return queue.sync {
        if !remainingData.isEmpty {
          data.append(remainingData)
        }
        return String(data: data, encoding: .utf8)?
          .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
      }
    }
  }

  private static func dispatchProjectBeadsBridgeResponse(
    _ response: ProjectBeadsBridgeResponse,
    to webView: WKWebView
  ) {
    guard let data = try? JSONEncoder().encode(response),
      let json = String(data: data, encoding: .utf8)
    else {
      return
    }
    let script = """
      window.dispatchEvent(new CustomEvent('\(projectBeadsResponseEventName)', { detail: \(json) }));
      undefined;
      """
    webView.evaluateJavaScript(script, completionHandler: nil)
  }

  func dispatchProjectBoardBridgeResponse(_ response: ProjectBoardResponse) {
    let projectId = response.projectId ?? activeProjectEditorId
    let targetSession =
      projectId.flatMap { projectEditorPaneSessions[$0] }
      ?? projectEditorPaneSessions.values.first { session in
        session.mode == "tasks"
      }
    guard let webView = targetSession?.webView else {
      return
    }
    let payloadJson = response.payloadJson
    let script = """
      window.dispatchEvent(new CustomEvent('\(projectBoardResponseEventName)', { detail: \(payloadJson) }));
      undefined;
      """
    webView.evaluateJavaScript(script, completionHandler: nil)
  }

  private func loadProjectEditorPaneWhenReady(projectId: String, url: String, reason: String) {
    /**
     CDXC:EditorPanes 2026-05-09-17:24
     Report editor startup state to the sidebar. The sidebar keeps the VS Code
     row visible through loading and turns it into a retryable error row if
     code-server does not become responsive within ten seconds.
     */
    sendEvent(.projectEditorLoadState(projectId: projectId, status: "opening", message: nil))
    guard url.hasPrefix(NativeCodeServerRuntimeLauncher.origin) else {
      /**
       CDXC:ModeSwitcher 2026-05-15-14:42:
       Git and tasks-backed Project modes reuse the project-editor shell so
       they can keep the Code-style left companion session pane while loading
       non-code-server content on the right. Only VS Code URLs should wait for
       the local code-server runtime; other project-editor destinations must
       navigate directly instead of failing on a localhost readiness check.
       */
      guard let session = projectEditorPaneSessions[projectId], session.url == url else {
        return
      }
      if let chromiumView = session.chromiumView {
        chromiumView.loadURLString(url)
      } else if let webView = session.webView, let parsedURL = URL(string: url) {
        if parsedURL.isFileURL {
          webView.loadFileURL(parsedURL, allowingReadAccessTo: parsedURL.deletingLastPathComponent())
        } else {
          webView.load(URLRequest(url: parsedURL))
        }
        sendEvent(.projectEditorLoadState(projectId: projectId, status: "running", message: nil))
      }
      session.hostView.refreshHostedWebView(reason: reason)
      return
    }
    Task.detached { [weak self] in
      let isReady = NativeCodeServerRuntimeLauncher.waitUntilResponsive(timeout: 10.0)
      await MainActor.run {
        guard let self, let session = self.projectEditorPaneSessions[projectId], session.url == url
        else {
          return
        }
        NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.load", [
          "isRuntimeReady": isReady,
          "projectId": projectId,
          "reason": reason,
          "url": url,
        ])
        if !isReady {
          /**
           CDXC:EditorPanes 2026-05-13-08:44
           The editor pane must not navigate Chromium to the fixed localhost
           code-server URL until the runtime listener is responsive. Loading a
           dead 127.0.0.1 target exposes Chromium's connection-refused page when
           the titlebar or project-header editor button is clicked; keep the
           pane in ghostex's project-scoped startup error state instead.
           */
          let message = "VS Code did not finish loading within 10 seconds. Make sure Accessibility is enabled in System Settings \u{2192} Privacy & Security \u{2192} Accessibility."
          session.hostView.setInitialLoadingOverlayError(message, reason: reason)
          self.sendEvent(
            .projectEditorLoadState(
              projectId: projectId,
              status: "error",
              message: message))
          return
        }
        session.chromiumView?.loadURLString(url)
        session.hostView.refreshHostedWebView(reason: reason)
      }
    }
  }

  private func configureProjectEditorChromiumCallbacks(
    _ chromiumView: GhostexCEFBrowserView,
    projectId: String,
    reason: String
  ) {
    chromiumView.navigationStateChangedHandler = { [weak self, weak chromiumView] _, _, isLoading in
      guard let self, let chromiumView else {
        return
      }
      self.updateProjectEditorInitialLoadingOverlay(
        projectId: projectId,
        chromiumView: chromiumView,
        isLoading: isLoading,
        reason: "chromiumNavigationStateChanged")
      self.projectEditorHostView(projectId: projectId, chromiumView: chromiumView)?.refreshBrowserToolbar(
        reason: "projectEditorNavigationStateChanged")
    }
    chromiumView.titleChangedHandler = { [weak self, weak chromiumView] title in
      guard let chromiumView else { return }
      self?.updateProjectEditorActiveTabMetadata(
        projectId: projectId,
        chromiumView: chromiumView,
        title: title,
        url: nil)
      self?.projectEditorHostView(projectId: projectId, chromiumView: chromiumView)?.refreshBrowserToolbar(
        reason: "projectEditorTitleChanged")
    }
    chromiumView.urlChangedHandler = { [weak self, weak chromiumView] url in
      guard let chromiumView else { return }
      self?.updateProjectEditorActiveTabMetadata(
        projectId: projectId,
        chromiumView: chromiumView,
        title: nil,
        url: url)
      self?.projectEditorHostView(projectId: projectId, chromiumView: chromiumView)?.refreshBrowserToolbar(
        reason: "projectEditorUrlChanged")
      if let session = self?.projectEditorPaneSessions[projectId],
        session.mode == "git",
        self?.activeProjectEditorId == projectId,
        session.chromiumView.map({ $0 === chromiumView }) == true
      {
        /**
         CDXC:GitProjectTabs 2026-05-16-09:50:
         Browser Back/Forward changes the active Git tab URL after the toolbar
         has already focused the Git pane. Send the post-navigation URL so the
         sidebar stores the visible Git tab address rather than the pre-click
         address or a stale Code-mode URL for the same project.
         */
        self?.sendProjectEditorTabSelected(projectId: projectId)
      }
    }
    chromiumView.newWindowRequestedHandler = { [weak self, weak chromiumView] url in
      guard let chromiumView else { return }
      guard let self, self.projectEditorPaneSessions[projectId]?.mode == "git" else {
        chromiumView.loadURLString(url)
        return
      }
      /**
       CDXC:GitProjectTabs 2026-05-16-07:42:
       Target-blank and window.open navigations inside a Git tab should create
       a new tab in that project's Git tab strip. Normal browser panes keep
       their existing single-pane retargeting behavior in the CEF bridge.
       */
      self.addProjectEditorGitTab(
        projectId: projectId,
        url: url,
        reason: "projectEditorGitPopup")
    }
    chromiumView.loadEventHandler = { [weak self, weak chromiumView] event, url, httpStatusCode, errorCode, errorText in
      guard let self else {
        return
      }
      let session = self.projectEditorPaneSessions[projectId]
      let hostView = chromiumView.flatMap { self.projectEditorHostView(projectId: projectId, chromiumView: $0) }
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.cef.loadEvent", [
        "activeProjectEditorId": self.activeProjectEditorId ?? NSNull(),
        "currentUrl": chromiumView?.currentURLString ?? NSNull(),
        "errorCode": errorCode,
        "errorText": errorText,
        "event": event,
        "expectedUrl": session?.url ?? NSNull(),
        "hostFrame": hostView.map { self.describeFrame($0.frame) } ?? NSNull(),
        "httpStatusCode": httpStatusCode,
        "isActive": self.activeProjectEditorId == projectId,
        "isLoading": chromiumView?.isLoading ?? false,
        "projectId": projectId,
        "reason": reason,
        "title": session?.title ?? NSNull(),
        "url": url,
        "windowNumber": self.window?.windowNumber ?? NSNull(),
      ])
    }
    chromiumView.consoleMessageHandler = { [weak self] message, source, line in
      guard let self else {
        return
      }
      let session = self.projectEditorPaneSessions[projectId]
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.cef.console", [
        "activeProjectEditorId": self.activeProjectEditorId ?? NSNull(),
        "expectedUrl": session?.url ?? NSNull(),
        "line": line,
        "message": message,
        "projectId": projectId,
        "reason": reason,
        "source": source,
        "title": session?.title ?? NSNull(),
        "windowNumber": self.window?.windowNumber ?? NSNull(),
      ])
    }
    /**
     CDXC:EditorPanes 2026-05-07-05:18
     VS Code view movement depends on browser drag/drop retargeting for live
     sidebar drop indicators and hold-before-release interactions. CEF Alloy
     panes receive native mouse movement but can miss in-page `dragover`
     retargeting, so ghostex keeps code-server free of load-time injected drag
     diagnostics and uses a scoped active-drag hover bridge only while dragging.

     CDXC:EditorPanes 2026-05-07-08:29
     First editor startup should show a native loading spinner immediately while
     the existing code-server readiness wait continues in parallel. The loader is
     dismissed from CEF navigation state after the real editor URL finishes, so
     it never adds startup delay or waits on a separate timer.

     CDXC:EditorPanes 2026-05-15-18:53:
     When the embedded code editor shows "reconnecting", refreshes to page not
     found, then recovers after switching away and back, diagnostics need the
     CEF load lifecycle and VS Code console stream tagged by project editor ID.
     Log those signals without adding fallback reloads or masking the failure.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.chromiumCallbacksConfigured", [
      "projectId": projectId,
      "reason": reason,
      "url": chromiumView.currentURLString ?? NSNull(),
    ])
  }

  private func updateProjectEditorInitialLoadingOverlay(
    projectId: String,
    chromiumView: GhostexCEFBrowserView,
    isLoading: Bool,
    reason: String
  ) {
    guard let hostView = projectEditorHostView(projectId: projectId, chromiumView: chromiumView),
      !isLoading
    else {
      return
    }
    let currentURL = chromiumView.currentURLString?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !currentURL.isEmpty, currentURL != "about:blank" else {
      return
    }
    hostView.setInitialLoadingOverlayVisible(false, reason: reason)
    sendEvent(.projectEditorLoadState(projectId: projectId, status: "running", message: nil))
  }

  private func updateProjectEditorActiveTabMetadata(
    projectId: String,
    chromiumView: GhostexCEFBrowserView,
    title: String?,
    url: String?
  ) {
    guard var session = projectEditorPaneSessions[projectId],
      session.mode == "git",
      session.showsProjectTabs,
      let tabIndex = session.tabs.firstIndex(where: { tab in
        guard let tabChromiumView = tab.chromiumView else {
          return false
        }
        return tabChromiumView === chromiumView
      })
    else {
      return
    }
    let existingTab = session.tabs[tabIndex]
    let nextURL = url?.trimmingCharacters(in: .whitespacesAndNewlines)
    if isProjectEditorPlaceholderURL(nextURL) {
      /**
       CDXC:GitProjectTabs 2026-05-16-12:40:
       First-open Git tabs are created with an internal about:blank CEF
       placeholder before the project GitHub URL is loaded. That placeholder
       must never replace the visible tab URL/title or the session URL, because
       the loader uses the intended GitHub URL as the navigation contract for
       the first Git view render.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.gitTab.placeholderMetadataIgnored", [
        "activeTabId": session.activeTabId,
        "incomingTitle": title ?? NSNull(),
        "incomingUrl": nextURL ?? NSNull(),
        "projectId": projectId,
        "tabId": existingTab.tabId,
        "tabTitle": existingTab.title,
        "tabUrl": existingTab.url,
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      return
    }
    let resolvedURL = nextURL?.isEmpty == false ? nextURL! : existingTab.url
    let nextTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedNextTitle =
      isProjectEditorPlaceholderTitle(nextTitle) && !isProjectEditorPlaceholderURL(existingTab.url)
      ? nil
      : nextTitle
    let resolvedTitle =
      resolvedNextTitle?.isEmpty == false
      ? resolvedNextTitle!
      : projectEditorTabTitle(for: resolvedURL, fallback: existingTab.title)
    session.tabs[tabIndex] = ProjectEditorBrowserTab(
      tabId: existingTab.tabId,
      chromiumView: existingTab.chromiumView,
      hostView: existingTab.hostView,
      webView: existingTab.webView,
      title: resolvedTitle,
      url: resolvedURL)
    if existingTab.tabId == session.activeTabId {
      session = projectEditorSession(session, activating: existingTab.tabId)
    }
    projectEditorPaneSessions[projectId] = session
    syncProjectEditorTabBars()
  }

  private func isProjectEditorPlaceholderURL(_ value: String?) -> Bool {
    guard let value else {
      return false
    }
    return value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "about:blank"
  }

  private func isProjectEditorPlaceholderTitle(_ value: String?) -> Bool {
    guard let value else {
      return false
    }
    return value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "about:blank"
  }

  private func orderedVisibleSessionIds() -> [String] {
    let fromLayout =
      terminalLayout.map(leafSessionIds) ?? Array(sessions.keys) + Array(webPaneSessions.keys)
    return fromLayout.filter { activeSessionIds.contains($0) }
  }

  private func orderedVisibleCommandSessionIds() -> [String] {
    let fromLayout = commandsPanelLayout.map(leafSessionIds) ?? Array(sessions.keys)
    return fromLayout.filter { commandsPanelActiveSessionIds.contains($0) }
  }

  private func orderedVisibleCommandPaneOwnerSessionIds() -> [String] {
    guard let commandsPanelLayout else {
      return orderedVisibleCommandSessionIds()
    }
    return visibleCommandPaneOwnerSessionIds(in: commandsPanelLayout)
  }

  /**
   CDXC:PaneTabs 2026-05-12-10:37
   Hit testing needs the visible pane owner for each tab group, not every
   active session inside the group. Inactive tab siblings are offscreen panes;
   querying their title bars can classify a real second-tab click as right-side
   pane chrome before the visible title bar gets the mouse stream.
   */
  private func orderedVisiblePaneOwnerSessionIds() -> [String] {
    guard let terminalLayout else {
      return orderedVisibleSessionIds()
    }
    return visiblePaneOwnerSessionIds(in: terminalLayout)
  }

  private func visiblePaneOwnerSessionIds(in node: NativeTerminalLayout) -> [String] {
    switch node {
    case .leaf(let sessionId):
      return activeSessionIds.contains(sessionId) ? [sessionId] : []
    case .tabs(let activeSessionId, let sessionIds):
      let tabSessionIds = sessionIds.filter { activeSessionIds.contains($0) || sleepingSessionIds.contains($0) }
      let activeTabSessionIds = tabSessionIds.filter { activeSessionIds.contains($0) }
      guard !activeTabSessionIds.isEmpty else {
        return []
      }
      return [
        activeSessionId.flatMap { activeTabSessionIds.contains($0) ? $0 : nil }
          ?? activeTabSessionIds[0]
      ]
    case .split(_, _, let children):
      return children.flatMap { visiblePaneOwnerSessionIds(in: $0) }
    }
  }

  private func visibleCommandPaneOwnerSessionIds(in node: NativeTerminalLayout) -> [String] {
    switch node {
    case .leaf(let sessionId):
      return commandsPanelActiveSessionIds.contains(sessionId) ? [sessionId] : []
    case .tabs(let activeSessionId, let sessionIds):
      let tabSessionIds = sessionIds.filter {
        commandsPanelActiveSessionIds.contains($0) || sleepingSessionIds.contains($0)
      }
      let activeTabSessionIds = tabSessionIds.filter { commandsPanelActiveSessionIds.contains($0) }
      guard !activeTabSessionIds.isEmpty else {
        return []
      }
      return [
        activeSessionId.flatMap { activeTabSessionIds.contains($0) ? $0 : nil }
          ?? activeTabSessionIds[0]
      ]
    case .split(_, _, let children):
      return children.flatMap { visibleCommandPaneOwnerSessionIds(in: $0) }
    }
  }

  private func isPaneSessionVisible(_ sessionId: String) -> Bool {
    activeSessionIds.contains(sessionId) || commandsPanelActiveSessionIds.contains(sessionId)
  }

  private func layoutTree(_ node: NativeTerminalLayout, in rect: CGRect, path: String) {
    switch node {
    case .leaf(let sessionId):
      setPaneTabs([sessionId], activeSessionId: sessionId, on: sessionId)
      recordPaneContentHitRegion(sessionId: sessionId, paneRect: rect, path: path)
      setFrame(rect, for: sessionId)
    case .tabs(let activeSessionId, let sessionIds):
      let tabSessionIds = sessionIds.filter { isPaneSessionVisible($0) || sleepingSessionIds.contains($0) }
      let activeTabSessionIds = tabSessionIds.filter { isPaneSessionVisible($0) }
      guard !activeTabSessionIds.isEmpty else { return }
      let selectedSessionId =
        activeSessionId.flatMap { activeTabSessionIds.contains($0) ? $0 : nil } ?? activeTabSessionIds[0]
      for sessionId in activeTabSessionIds where sessionId != selectedSessionId {
        movePaneSessionOffscreen(sessionId)
      }
      setPaneTabs(tabSessionIds, activeSessionId: selectedSessionId, on: selectedSessionId)
      recordPaneContentHitRegion(sessionId: selectedSessionId, paneRect: rect, path: path)
      setFrame(rect, for: selectedSessionId)
    case .split(let direction, let ratio, let children):
      let visibleChildren = children.filter {
        !leafSessionIds($0).allSatisfy { !isPaneSessionVisible($0) }
      }
      guard !visibleChildren.isEmpty else { return }
      if visibleChildren.count == 1 {
        layoutTree(visibleChildren[0], in: rect, path: "\(path).0")
        return
      }
      /**
       CDXC:WorkspaceLayout 2026-04-28-06:01
       Native split panes must reserve real AppKit layout space between split
       siblings instead of relying on terminal/content overlays.

       CDXC:NativePaneResize 2026-05-13-07:23
       Match the stable sidebar divider shape for internal pane dividers:
       children are separated by a real five-pixel AppKit rail. The rail is the
       native cursor and drag owner, so terminal/browser/titlebar views never
       compete for the same hover strip.
       */
      let gap = splitDividerWidth(forChildCount: visibleChildren.count)
      let defaultRatios = defaultPaneResizeRatios(
        childCount: visibleChildren.count,
        firstRatio: ratio.map { CGFloat($0) })
      let ratios = normalizedPaneResizeRatios(for: path, defaultRatios: defaultRatios)
      var nextOrigin = direction == .horizontal ? rect.minX : rect.maxY
      let availableLength = max(
        (direction == .horizontal ? rect.width : rect.height)
          - gap * CGFloat(max(visibleChildren.count - 1, 0)),
        CGFloat(visibleChildren.count)
      )
      let ratioTotal = max(ratios.reduce(0, +), 1)
      var childRects: [CGRect] = []
      for (index, child) in visibleChildren.enumerated() {
        let isLast = index == visibleChildren.count - 1
        let childLength: CGFloat
        if isLast {
          childLength =
            direction == .horizontal
            ? max(rect.maxX - nextOrigin, 1)
            : max(nextOrigin - rect.minY, 1)
        } else {
          childLength = max(floor(availableLength * (ratios[index] / ratioTotal)), 1)
        }
        let childRect: CGRect
        if direction == .horizontal {
          childRect = CGRect(
            x: nextOrigin, y: rect.minY, width: childLength, height: rect.height)
          nextOrigin += childLength + gap
        } else {
          childRect = CGRect(
            x: rect.minX, y: nextOrigin - childLength, width: rect.width, height: childLength)
          nextOrigin -= childLength + gap
        }
        childRects.append(childRect)
        layoutTree(child, in: childRect, path: "\(path).\(index)")
      }
      recordPaneResizeHits(
        childRects: childRects,
        direction: direction,
        path: path,
        rect: rect
      )
      if paneResizeRatiosByPath[path]?.count != visibleChildren.count {
        paneResizeRatiosByPath[path] = ratios
      }
    }
  }

  private func recordPaneContentHitRegion(sessionId: String, paneRect: CGRect, path: String) {
    /**
     CDXC:NativeTerminalFocus 2026-05-26-04:32:
     Terminal content clicks must be owned by the pane geometry from the current
     layout pass, not by AppKit subview order. Awake-but-hidden tab siblings can
     keep live Ghostty views, so hit testing by container frames alone can focus a
     stale session and make the sidebar rewrite the visible split.
     */
    let titleBarHeight = min(titleBarHeight(for: sessionId), max(paneRect.height, 0))
    let contentRect = CGRect(
      x: paneRect.minX,
      y: paneRect.minY,
      width: paneRect.width,
      height: max(paneRect.height - titleBarHeight, 0)
    )
    guard contentRect.width > 0, contentRect.height > 0 else {
      return
    }
    paneContentHitRegions.append(
      PaneContentHitRegion(
        path: path,
        rect: contentRect,
        role: path.hasPrefix("commands") ? .commands : .workspace,
        sessionId: sessionId))
  }

  private func layoutGrid(_ sessionIds: [String], in rect: CGRect) {
    let columns = Int(ceil(sqrt(Double(sessionIds.count))))
    let rows = Int(ceil(Double(sessionIds.count) / Double(columns)))
    let gap = splitGap(forChildCount: sessionIds.count)
    let cellWidth = max((rect.width - gap * CGFloat(max(columns - 1, 0))) / CGFloat(columns), 1)
    let cellHeight = max((rect.height - gap * CGFloat(max(rows - 1, 0))) / CGFloat(rows), 1)
    for (index, sessionId) in sessionIds.enumerated() {
      let column = index % columns
      let row = index / columns
      let cell = CGRect(
        x: rect.minX + CGFloat(column) * (cellWidth + gap),
        y: rect.maxY - CGFloat(row + 1) * cellHeight - CGFloat(row) * gap,
        width: cellWidth,
        height: cellHeight
      )
      setPaneTabs([sessionId], activeSessionId: sessionId, on: sessionId)
      setFrame(cell, for: sessionId)
    }
  }

  private func splitGap(forChildCount childCount: Int) -> CGFloat {
    /**
     CDXC:NativePaneResize 2026-05-11-18:16
     Grid layouts keep the Pane Gap setting between independent panes, matching
     the outer workspace inset. Structured split layouts use
     splitDividerWidth(forChildCount:) so internal rails match the stable native
     sidebar divider: one real five-pixel AppKit cursor/drag owner.
     */
    childCount <= 1 ? 0 : paneGap
  }

  private func splitDividerWidth(forChildCount childCount: Int) -> CGFloat {
    childCount <= 1 ? 0 : Self.paneResizeRailWidth
  }

  /**
   CDXC:NativePaneResize 2026-05-02-16:44
   Native Ghostty and WKWebView panes sit above the React workspace DOM, so
   split resizing must be owned by AppKit. The workspace view records cursor
   and mouse bands around split boundaries and clamps panes to terminal-usable
   dimensions.

   CDXC:NativePaneResize 2026-05-14-07:04
   Double-clicking a split handle should restore that split to its layout-defined
   original ratio. Command panes use a 0.85 vertical split so resetting the top
   drag line returns the command pane to its intended original size instead of
   equalizing it with the main workarea.
   */
  private func defaultPaneResizeRatios(childCount: Int, firstRatio: CGFloat?) -> [CGFloat] {
    guard childCount > 0 else { return [] }
    guard childCount > 1, let firstRatio else {
      return Array(repeating: 1, count: childCount)
    }
    let first = min(max(firstRatio, 0.05), 0.95)
    let remaining = max(1 - first, 0.05)
    let trailing = remaining / CGFloat(childCount - 1)
    return [first] + Array(repeating: trailing, count: childCount - 1)
  }

  private func normalizedPaneResizeRatios(for path: String, defaultRatios: [CGFloat]) -> [CGFloat] {
    guard let current = paneResizeRatiosByPath[path], current.count == defaultRatios.count,
      current.allSatisfy({ $0 > 0 })
    else {
      return defaultRatios
    }
    return current
  }

  private func recordPaneResizeHits(
    childRects: [CGRect],
    direction: NativeTerminalLayout.SplitDirection,
    path: String,
    rect: CGRect
  ) {
    guard childRects.count > 1 else { return }
    let visualGap = splitDividerWidth(forChildCount: childRects.count)
    for boundaryIndex in 1..<childRects.count {
      let previous = childRects[boundaryIndex - 1]
      let next = childRects[boundaryIndex]
      guard isInteriorPaneResizeBoundary(
        previous: previous,
        next: next,
        direction: direction,
        container: rect
      ) else {
        continue
      }
      let dividerRect: CGRect
      switch direction {
      case .horizontal:
        let dividerWidth = max(next.minX - previous.maxX, 0)
        dividerRect = CGRect(
          x: previous.maxX,
          y: max(previous.minY, next.minY),
          width: dividerWidth,
          height: min(previous.maxY, next.maxY) - max(previous.minY, next.minY)
        )
      case .vertical:
        let dividerHeight = max(previous.minY - next.maxY, 0)
        dividerRect = CGRect(
          x: max(previous.minX, next.minX),
          y: next.maxY,
          width: min(previous.maxX, next.maxX) - max(previous.minX, next.minX),
          height: dividerHeight
        )
      }
      /**
       CDXC:NativePaneResize 2026-05-11-17:53
       Divider hit rects are derived from the reserved split gap itself. They
       no longer expand around the boundary center, so resize views are layout
       siblings between pane leaves instead of overlays on top of pane content.
       */
      if let dividerRect = validatedPaneResizeDividerRect(
        dividerRect,
        direction: direction,
        container: rect,
        previous: previous,
        next: next,
        boundaryIndex: boundaryIndex,
        path: path
      ) {
        paneResizeHits.append(
          PaneResizeHit(
            availableLength: max(
              (direction == .horizontal ? rect.width : rect.height)
                - visualGap * CGFloat(max(childRects.count - 1, 0)),
              CGFloat(childRects.count)
            ),
            boundaryIndex: boundaryIndex,
            direction: direction,
            path: path,
            rect: dividerRect,
            trackCount: childRects.count
          ))
      }
    }
  }

  private func validatedPaneResizeDividerRect(
    _ dividerRect: CGRect,
    direction: NativeTerminalLayout.SplitDirection,
    container: CGRect,
    previous: CGRect,
    next: CGRect,
    boundaryIndex: Int,
    path: String
  ) -> CGRect? {
    guard let clamped = paneResizeDividerRectExcludingOuterEdges(
      dividerRect,
      direction: direction,
      container: container
    ) else {
      logInvalidPaneResizeRailGeometry(
        reason: "emptyOrOutsideContainer",
        dividerRect: dividerRect,
        acceptedRect: nil,
        previous: previous,
        next: next,
        container: container,
        direction: direction,
        boundaryIndex: boundaryIndex,
        path: path)
      return nil
    }

    /**
     CDXC:SplitResizeRails 2026-05-11-20:25
     A split-resize rail is valid only when the accepted AppKit frame lives inside
     the split container and remains between sibling panes. Reject and log
     geometry that overlaps pane content instead of widening hit areas around it.
     */
    let expandedContainer = container.insetBy(dx: -0.5, dy: -0.5)
    let previousOverlap = paneResizeRectArea(clamped.intersection(previous))
    let nextOverlap = paneResizeRectArea(clamped.intersection(next))
    let isValid = expandedContainer.contains(clamped)
      && previousOverlap <= 0.5
      && nextOverlap <= 0.5
    guard isValid else {
      logInvalidPaneResizeRailGeometry(
        reason: "overlapsPaneOrEscapesContainer",
        dividerRect: dividerRect,
        acceptedRect: clamped,
        previous: previous,
        next: next,
        container: container,
        direction: direction,
        boundaryIndex: boundaryIndex,
        path: path)
      return nil
    }
    return clamped
  }

  private func paneResizeRectArea(_ rect: CGRect) -> CGFloat {
    guard !rect.isNull, !rect.isEmpty else {
      return 0
    }
    return max(rect.width, 0) * max(rect.height, 0)
  }

  private func logInvalidPaneResizeRailGeometry(
    reason: String,
    dividerRect: CGRect,
    acceptedRect: CGRect?,
    previous: CGRect,
    next: CGRect,
    container: CGRect,
    direction: NativeTerminalLayout.SplitDirection,
    boundaryIndex: Int,
    path: String
  ) {
    NativeT3CodePaneReproLog.append("nativeWorkspace.paneResize.invalidRailGeometry", [
      "acceptedRect": acceptedRect.map { describeFrame($0) } ?? NSNull(),
      "boundaryIndex": boundaryIndex,
      "container": describeFrame(container),
      "direction": direction.rawValue,
      "dividerRect": describeFrame(dividerRect),
      "next": describeFrame(next),
      "path": path,
      "previous": describeFrame(previous),
      "reason": reason,
    ])
  }

  private func isInteriorPaneResizeBoundary(
    previous: CGRect,
    next: CGRect,
    direction: NativeTerminalLayout.SplitDirection,
    container: CGRect
  ) -> Bool {
    /**
     CDXC:NativePaneResize 2026-05-08-12:38
     Terminal resize handles are only valid where two visible pane siblings
     share an interior boundary. Do not create edge handles along the workspace
     perimeter; the sidebar/workspace boundary is owned by ghostexRootView's
     sidebar resize handle and must not compete with terminal pane hit zones.
     */
    let epsilon: CGFloat = 0.5
    switch direction {
    case .horizontal:
      let boundaryCenter = (previous.maxX + next.minX) / 2
      let overlapHeight = min(previous.maxY, next.maxY) - max(previous.minY, next.minY)
      return boundaryCenter > container.minX + epsilon
        && boundaryCenter < container.maxX - epsilon
        && overlapHeight > epsilon
    case .vertical:
      let boundaryCenter = (previous.minY + next.maxY) / 2
      let overlapWidth = min(previous.maxX, next.maxX) - max(previous.minX, next.minX)
      return boundaryCenter > container.minY + epsilon
        && boundaryCenter < container.maxY - epsilon
        && overlapWidth > epsilon
    }
  }

  private func paneResizeDividerRectExcludingOuterEdges(
    _ dividerRect: CGRect,
    direction: NativeTerminalLayout.SplitDirection,
    container: CGRect
  ) -> CGRect? {
    /**
     CDXC:NativePaneResize 2026-05-08-12:38
     Interior split dividers should not expose draggable caps on pane sides
     that touch no sibling pane. Trim those caps so a terminal split divider
     cannot sit immediately beside the sidebar resize handle at the workspace
     edge.
     */
    let edgeInset = min(Self.paneResizeOuterEdgeExclusion, max(0, paneGap))
    let trimmed: CGRect
    switch direction {
    case .horizontal:
      trimmed = dividerRect.insetBy(dx: 0, dy: edgeInset)
    case .vertical:
      trimmed = dividerRect.insetBy(dx: edgeInset, dy: 0)
    }
    let clamped = trimmed.intersection(container)
    guard clamped.width > 0, clamped.height > 0 else {
      return nil
    }
    return clamped
  }

  private func paneResizeCursor(at point: CGPoint) -> NSCursor? {
    guard let hit = paneResizeHit(at: point) else {
      return nil
    }
    return paneResizeCursor(for: hit.direction)
  }

  private func paneResizeCursor(for direction: NativeTerminalLayout.SplitDirection) -> NSCursor {
    direction == .horizontal ? .resizeLeftRight : .resizeUpDown
  }

  private func paneResizeHandleFrame(for hit: PaneResizeHit) -> CGRect {
    /**
     CDXC:NativePaneResize 2026-05-13-07:23
     Pane split rails use the same ownership shape as the sidebar resize rail:
     the real reserved divider rect is the complete native hit target and cursor
     rect. Do not expand this frame over neighboring terminal/browser/titlebar
     views, because overlap lets those views compete for cursor settling.
     */
    return hit.rect.intersection(bounds)
  }

  var sidebarResizeEdgeExtensionWidth: CGFloat {
    /**
     CDXC:SidebarResizeRails 2026-05-15-03:59:
     The sidebar/workspace boundary has one AppKit resize owner: ghostexRootView's
     root divider. Expose the workspace edge gap width so the root divider can
     cover that visible gap without TerminalWorkspaceView installing a second
     cursor or drag target beside split panes.
     */
    guard paneGap > 0, !orderedVisibleSessionIds().isEmpty else {
      return 0
    }
    return paneGap
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  /**
   CDXC:NativePaneReorder 2026-05-06-03:04
   Pane drag-to-reorder must still start from the native title bar only, but
   embedded terminal/WebKit surfaces can sit above the title-bar mouse stream
   during AppKit hit testing. Route only exact title-bar hits to the title-bar
   view before falling back to normal pane hit testing; resize handles keep
   priority so split resizing is not converted into pane dragging.

   CDXC:NativePaneReorder 2026-05-10-14:24
   Repro logs showed a bottom-edge terminal selection hit resolving to a
   TerminalSessionTitleBarView while the registered title-bar frame was at the
   pane top. Treat any title-bar hit outside the current registered draggable
   title-bar band as invalid and reroute it to the pane content so the last
   terminal line remains selectable and cannot start pane reordering.

   CDXC:NativePaneResize 2026-05-11-17:53
   Split dividers are ordinary AppKit siblings in the reserved pane gap.

   CDXC:NativePaneResize 2026-05-13-07:35
   Pane resize rails are real AppKit divider siblings between pane containers.
   This workspace has custom titlebar/content hit routing, so check the rail
   view before pane chrome. Without this explicit native handoff, titlebar
   routing can bypass the rail and make pane resize cursors/drags disappear.

   CDXC:CommandsPanel 2026-05-14-09:31:
   The command pane remains interactive while the embedded VS Code CEF editor is
   active. Route visible command-pane and companion pane titlebar/content hits
   before yielding the rest of the workspace to the editor surface, because
   AppKit's default subview hit order can hand floating command-pane clicks to
   Chromium even when native command chrome is visually above it.

   CDXC:ProjectEditorCompanion 2026-05-15-09:01:
   Project-editor hit logs showed editor-region points falling through to
   `super.hitTest` and resolving to the companion terminal surface. When VS Code
   is active, use explicit command, companion, and editor frame routing so stale
   or overlapping terminal subviews cannot steal clicks from the editor frame.
  */
  override func hitTest(_ point: NSPoint) -> NSView? {
    if let floatingEditorHitView = floatingEditorHitView(at: point) {
      logProjectEditorHitTestDecision(
        event: "nativeWorkspace.projectEditor.hitTest.floatingEditor",
        at: point,
        hitView: floatingEditorHitView)
      return floatingEditorHitView
    }
    if isProjectEditorInteractionSurfaceActive {
      return projectEditorInteractionHitView(at: point)
    }
    if let paneResizeHandleHitView = paneResizeHandleHitView(at: point) {
      return paneResizeHandleHitView
    }
    if let titleBarHitView = paneTitleBarHitView(at: point) {
      if isPaneBottomEdgeProbePoint(point) {
        logPaneReorderProbe(
          event: "nativePaneReorder.hitTest.titleBarNearBottom",
          at: point,
          details: [
            "returnedHitView": String(describing: type(of: titleBarHitView)),
          ])
      }
      return titleBarHitView
    }
    if let contentHitView = paneContentHitView(at: point) {
      return contentHitView
    }
    /**
     CDXC:NativeTerminalFocus 2026-05-13-07:48
     Terminal content clicks must resolve through the current visible pane-owner
     layout before falling through to AppKit child z-order. Stale inactive-tab
     surfaces and recently focused pane containers can otherwise overlap a
     neighboring pane enough that clicking the left side of a pane focuses the
     pane immediately to its left. Resize rails and title bars keep priority
     above this content route.
     */
    let hitView = super.hitTest(point)
    if paneTitleBarAncestor(of: hitView) != nil {
      logPaneReorderProbe(
        event: "nativePaneReorder.hitTest.invalidTitleBarRerouted",
        at: point,
        details: [
          "originalHitView": hitView.map { String(describing: type(of: $0)) } ?? "nil",
        ])
      return paneContentHitView(at: point)
    }
    return hitView
  }

  func nativeChromeHitView(at point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      return nil
    }
    guard !suppressNativeChromeInteractivity else {
      return nil
    }
    /**
     CDXC:RootHitBoundaries 2026-05-22-22:48:
     The root React titlebar WKWebView can report interactive DOM regions below
     the fixed titlebar strip. Native pane tabs and resize handles in the
     workspace must get first chance at those points so visible right-side tabs
     do not fall through to the titlebar/sidebar web surface.
     */
    if let floatingEditorHitView = floatingEditorHitView(at: point) {
      return floatingEditorHitView
    }
    if isProjectEditorInteractionSurfaceActive {
      if let paneResizeHandleHitView = projectEditorResizeHandleHitView(at: point) {
        return paneResizeHandleHitView
      }
      if let commandTitleBarHitView = commandPaneTitleBarHitView(at: point) {
        return commandTitleBarHitView
      }
      if let projectEditorTitleBarHitView = activeProjectEditorTitleBarHitView(at: point) {
        return projectEditorTitleBarHitView
      }
      return nil
    }
    if let paneResizeHandleHitView = paneResizeHandleHitView(at: point) {
      return paneResizeHandleHitView
    }
    return paneTitleBarHitView(at: point)
  }

  func setNativeChromeInteractivitySuppressed(_ suppressed: Bool) {
    guard suppressNativeChromeInteractivity != suppressed else {
      return
    }
    /**
     CDXC:OverlayInteractivity 2026-05-25-07:02:
     App modals and titlebar dropdown portals can visually cover native pane
     tabs while the pane title bars still own AppKit tracking areas. Suppress
     the native chrome itself while the root overlay shield is active so hover
     state, tab tooltips, and clicks cannot leak through transparent overlay
     pixels.
     */
    suppressNativeChromeInteractivity = suppressed
    applyNativeChromeInteractivitySuppression()
  }

  private func applyNativeChromeInteractivitySuppression() {
    for session in sessions.values {
      session.titleBarView.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
    }
    for session in webPaneSessions.values {
      session.titleBarView.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
    }
    for session in projectEditorPaneSessions.values {
      session.titleBarView?.setOverlayInteractionSuppressed(suppressNativeChromeInteractivity)
    }
  }

  private func projectEditorInteractionHitView(at point: NSPoint) -> NSView? {
    if let paneResizeHandleHitView = projectEditorResizeHandleHitView(at: point) {
      logProjectEditorHitTestDecision(
        event: "nativeWorkspace.projectEditor.hitTest.resizeHandle",
        at: point,
        hitView: paneResizeHandleHitView)
      return paneResizeHandleHitView
    }
    if let commandTitleBarHitView = commandPaneTitleBarHitView(at: point) {
      logProjectEditorHitTestDecision(
        event: "nativeWorkspace.projectEditor.hitTest.commandTitleBar",
        at: point,
        hitView: commandTitleBarHitView)
      return commandTitleBarHitView
    }
    if let commandContentHitView = commandPaneContentHitView(at: point) {
      logProjectEditorHitTestDecision(
        event: "nativeWorkspace.projectEditor.hitTest.commandContent",
        at: point,
        hitView: commandContentHitView)
      return commandContentHitView
    }

    let workspaceBounds = projectEditorHitTestWorkspaceBounds()
    let companionLayout = projectEditorCompanionLayout(in: workspaceBounds)
    if let companionLayout {
      if companionLayout.companionFrame.contains(point) {
        if let companionHitView = projectEditorCompanionHitView(layout: companionLayout, at: point) {
          logProjectEditorHitTestDecision(
            event: "nativeWorkspace.projectEditor.hitTest.companion",
            at: point,
            hitView: companionHitView)
          return companionHitView
        }
        let hitView = projectEditorCompanionFallbackHitView(sessionId: companionLayout.sessionId)
        logProjectEditorHitTestDecision(
          event: "nativeWorkspace.projectEditor.hitTest.companionFallback",
          at: point,
          hitView: hitView)
        return hitView
      }
      if companionLayout.editorFrame.contains(point) {
        let editorHitView = activeProjectEditorHitView(at: point)
        logProjectEditorHitTestDecision(
          event: "nativeWorkspace.projectEditor.hitTest.editor",
          at: point,
          hitView: editorHitView)
        return editorHitView
      }
    } else if workspaceBounds.contains(point) {
      let editorHitView = activeProjectEditorHitView(at: point)
      logProjectEditorHitTestDecision(
        event: "nativeWorkspace.projectEditor.hitTest.editor",
        at: point,
        hitView: editorHitView)
      return editorHitView
    }

    let hitView = super.hitTest(point)
    logProjectEditorHitTestDecision(
      event: "nativeWorkspace.projectEditor.hitTest.super",
      at: point,
      hitView: hitView)
    return hitView
  }

  private func projectEditorResizeHandleHitView(at point: NSPoint) -> NSView? {
    if
      !projectEditorCompanionResizeHandleView.isHidden,
      projectEditorCompanionResizeHandleView.alphaValue > 0,
      projectEditorCompanionResizeHandleView.frame.contains(point)
    {
      let handlePoint = projectEditorCompanionResizeHandleView.convert(point, from: self)
      if let hitView = projectEditorCompanionResizeHandleView.hitTest(handlePoint) {
        return hitView
      }
    }
    if
      !commandsPanelResizeHandleView.isHidden,
      commandsPanelResizeHandleView.alphaValue > 0,
      commandsPanelResizeHandleView.frame.contains(point)
    {
      let handlePoint = commandsPanelResizeHandleView.convert(point, from: self)
      if let hitView = commandsPanelResizeHandleView.hitTest(handlePoint) {
        return hitView
      }
    }
    return nil
  }

  private func commandPaneTitleBarHitView(at point: CGPoint) -> NSView? {
    for sessionId in orderedVisibleCommandPaneOwnerSessionIds().reversed() {
      if let session = sessions[sessionId],
        let hitView = paneTitleBarHitView(session.titleBarView, at: point)
      {
        return hitView
      }
    }
    return nil
  }

  private func commandPaneContentHitView(at point: CGPoint) -> NSView? {
    for sessionId in orderedVisibleCommandPaneOwnerSessionIds().reversed() {
      if let session = sessions[sessionId],
        let hitView = terminalPaneContentHitView(session, at: point)
      {
        return hitView
      }
    }
    return nil
  }

  private func projectEditorCompanionHitView(
    layout: ProjectEditorCompanionLayout,
    at point: CGPoint
  ) -> NSView? {
    let sessionId = layout.sessionId
    if let session = sessions[sessionId] {
      return paneTitleBarHitView(session.titleBarView, at: point)
        ?? terminalPaneContentHitView(session, at: point)
    }
    if let session = webPaneSessions[sessionId] {
      return paneTitleBarHitView(session.titleBarView, at: point)
        ?? webPaneContentHitView(session, at: point)
    }
    return nil
  }

  private func projectEditorCompanionFallbackHitView(sessionId: String) -> NSView? {
    if let session = sessions[sessionId], !session.containerView.isHidden {
      return session.containerView
    }
    if let session = webPaneSessions[sessionId], !session.containerView.isHidden {
      return session.containerView
    }
    return nil
  }

  private func activeProjectEditorTitleBarHitView(at point: CGPoint) -> NSView? {
    let activeSession =
      activeProjectEditorId.flatMap { projectEditorPaneSessions[$0] }
      ?? visibleProjectEditorInteractionSessionIds.compactMap { projectEditorPaneSessions[$0] }.first
    guard let activeSession,
      let titleBarView = activeSession.titleBarView,
      !titleBarView.isHidden,
      titleBarView.frame.contains(point)
    else {
      return nil
    }
    let titleBarPoint = convert(point, to: titleBarView)
    return titleBarView.hitTest(titleBarPoint) ?? titleBarView
  }

  private func activeProjectEditorHitView(at point: CGPoint) -> NSView? {
    let activeSession =
      activeProjectEditorId.flatMap { projectEditorPaneSessions[$0] }
      ?? visibleProjectEditorInteractionSessionIds.compactMap { projectEditorPaneSessions[$0] }.first
    guard let activeSession, !activeSession.hostView.isHidden else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.hitTest.activeHostMissing", [
        "activeProjectEditorId": activeProjectEditorId ?? NSNull(),
        "knownProjectEditorIds": Array(projectEditorPaneSessions.keys).sorted(),
        "point": describePoint(point),
        "visibleProjectEditorInteractionSessionIds": Array(visibleProjectEditorInteractionSessionIds).sorted(),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      return nil
    }
    if let hitView = activeProjectEditorTitleBarHitView(at: point) {
      /**
       CDXC:GitProjectTabs 2026-05-16-07:42:
       Git tab chrome sits above the browser host inside the project-editor
       frame. Route hit testing to that native tab bar before falling back to
       the hosted Chromium view so tabs, inline buttons, and the plus control
       receive real AppKit mouse events.
      */
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.hitTest.gitTabBar", [
        "activeProjectEditorId": activeProjectEditorId ?? NSNull(),
        "hitView": String(describing: type(of: hitView)),
        "point": describePoint(point),
        "titleBarFrame": describeFrame(activeSession.titleBarView?.frame ?? .zero),
        "titleBarPoint": describePoint(convert(point, to: activeSession.titleBarView)),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      return hitView
    }
    let editorPoint = convert(point, to: activeSession.hostView)
    guard activeSession.hostView.bounds.contains(editorPoint) else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.hitTest.editorOutsideHostBounds", [
        "activeHostFrame": describeFrame(activeSession.hostView.frame),
        "activeProjectEditorId": activeProjectEditorId ?? NSNull(),
        "editorPoint": describePoint(editorPoint),
        "hostBounds": describeFrame(activeSession.hostView.bounds),
        "point": describePoint(point),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      return activeSession.hostView
    }
    let hitView = activeSession.hostView.hitTest(editorPoint) ?? activeSession.hostView
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.hitTest.editorHost", [
      "activeHostFrame": describeFrame(activeSession.hostView.frame),
      "activeProjectEditorId": activeProjectEditorId ?? NSNull(),
      "activeProjectEditorMode": activeSession.mode,
      "activeTabId": activeSession.activeTabId,
      "editorPoint": describePoint(editorPoint),
      "hitView": String(describing: type(of: hitView)),
      "hostHidden": activeSession.hostView.isHidden,
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    return hitView
  }

  private func projectEditorHitTestWorkspaceBounds() -> CGRect {
    projectEditorCompanionResizeWorkspaceBounds.width > 1
      && projectEditorCompanionResizeWorkspaceBounds.height > 1
      ? projectEditorCompanionResizeWorkspaceBounds
      : bounds
  }

  /**
   CDXC:ProjectEditorCompanion 2026-05-15-05:34:
   Clicks near the left edge of the embedded VS Code surface can be stolen by
   native pane hit routing before CEF sees them. While confirming the root
   cause, project-editor hit-test logs must record the click point, companion
   and editor frames, returned AppKit view, and resolved session id so a click
   in the editor frame can be correlated with an unexpected terminal focus.
   */
  private func logProjectEditorHitTestDecision(
    event: String,
    at point: CGPoint,
    hitView: NSView?,
    details: [String: Any] = [:]
  ) {
    guard NativeDebugLogging.isEnabled, isProjectEditorInteractionSurfaceActive else {
      return
    }
    let workspaceBounds = projectEditorHitTestWorkspaceBounds()
    let companionLayout = projectEditorCompanionLayout(in: workspaceBounds)
    var payload = details
    payload["activeProjectEditorId"] = nullableString(activeProjectEditorId)
    payload["companionContentFrame"] = companionLayout.map { describeFrame($0.contentFrame) } ?? NSNull()
    payload["companionFrame"] = companionLayout.map { describeFrame($0.companionFrame) } ?? NSNull()
    payload["companionSessionId"] = nullableString(projectEditorCompanionSessionId)
    payload["editorFrame"] = companionLayout.map { describeFrame($0.editorFrame) } ?? NSNull()
    payload["focusedSessionId"] = nullableString(focusedSessionId)
    payload["hitSessionId"] = nullableString(hitView.flatMap { sessionId(containing: $0) })
    payload["hitView"] = hitView.map { String(describing: type(of: $0)) } ?? NSNull()
    payload["point"] = describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0))
    payload["pointRegion"] = projectEditorHitTestPointRegion(point, layout: companionLayout)
    payload["resizeHandleFrame"] = companionLayout.map { describeFrame($0.resizeHandleFrame) } ?? NSNull()
    payload["visibleCommandPaneOwnerSessionIds"] = orderedVisibleCommandPaneOwnerSessionIds()
    payload["visiblePaneOwnerSessionIds"] = orderedVisiblePaneOwnerSessionIds()
    payload["workspaceBounds"] = describeFrame(workspaceBounds)
    TerminalFocusDebugLog.append(event: event, details: payload)
  }

  private func projectEditorHitTestPointRegion(
    _ point: CGPoint,
    layout: ProjectEditorCompanionLayout?
  ) -> String {
    guard let layout else {
      return "noCompanionLayout"
    }
    if layout.resizeHandleFrame.contains(point) {
      return "companionResizeHandle"
    }
    if layout.companionFrame.contains(point) {
      return "companion"
    }
    if layout.editorFrame.contains(point) {
      return "editor"
    }
    return "outside"
  }

  private func paneResizeHandleHitView(at point: NSPoint) -> NSView? {
    if
      !projectEditorCompanionResizeHandleView.isHidden,
      projectEditorCompanionResizeHandleView.alphaValue > 0,
      projectEditorCompanionResizeHandleView.frame.contains(point)
    {
      let handlePoint = projectEditorCompanionResizeHandleView.convert(point, from: self)
      if let hitView = projectEditorCompanionResizeHandleView.hitTest(handlePoint) {
        return hitView
      }
    }
    if
      !commandsPanelResizeHandleView.isHidden,
      commandsPanelResizeHandleView.alphaValue > 0,
      commandsPanelResizeHandleView.frame.contains(point)
    {
      let handlePoint = commandsPanelResizeHandleView.convert(point, from: self)
      if let hitView = commandsPanelResizeHandleView.hitTest(handlePoint) {
        return hitView
      }
    }
    for handleView in paneResizeHandleViews.reversed() {
      guard
        !handleView.isHidden,
        handleView.alphaValue > 0,
        handleView.frame.contains(point)
      else {
        continue
      }
      let handlePoint = handleView.convert(point, from: self)
      if let hitView = handleView.hitTest(handlePoint) {
        return hitView
      }
    }
    return nil
  }

  private func floatingEditorHitView(at point: NSPoint) -> NSView? {
    guard
      let overlayView = floatingEditorOverlayView,
      !overlayView.isHidden,
      overlayView.alphaValue > 0,
      overlayView.frame.contains(point)
    else {
      return nil
    }
    return overlayView.hitTest(overlayView.convert(point, from: self))
  }

  private func paneTitleBarHitView(at point: CGPoint) -> NSView? {
    guard paneResizeHit(at: point) == nil else {
      return nil
    }
    for sessionId in orderedVisibleCommandPaneOwnerSessionIds().reversed() {
      if let session = sessions[sessionId],
        let hitView = paneTitleBarHitView(session.titleBarView, at: point)
      {
        return hitView
      }
    }
    for sessionId in orderedVisiblePaneOwnerSessionIds().reversed() {
      if let session = sessions[sessionId],
        let hitView = paneTitleBarHitView(session.titleBarView, at: point)
      {
        return hitView
      }
      if let session = webPaneSessions[sessionId],
        let hitView = paneTitleBarHitView(session.titleBarView, at: point)
      {
        return hitView
      }
    }
    return nil
  }

  private func paneTitleBarHitView(
    _ titleBarView: TerminalSessionTitleBarView,
    at point: CGPoint
  ) -> NSView? {
    let titleBarPoint = convert(point, to: titleBarView)
    guard !titleBarView.isHidden, titleBarView.bounds.contains(titleBarPoint) else {
      return nil
    }
    return titleBarView.hitTest(titleBarPoint)
  }

  private func paneTitleBarAncestor(of view: NSView?) -> TerminalSessionTitleBarView? {
    var currentView = view
    while let view = currentView {
      if let titleBarView = view as? TerminalSessionTitleBarView {
        return titleBarView
      }
      currentView = view.superview
    }
    return nil
  }

  private func paneContentHitView(at point: CGPoint) -> NSView? {
    if let commandHitView = paneContentHitView(at: point, role: .commands) {
      return commandHitView
    }
    if let workspaceHitView = paneContentHitView(at: point, role: .workspace) {
      return workspaceHitView
    }
    return nil
  }

  private func paneContentHitView(at point: CGPoint, role: PaneContentHitRole) -> NSView? {
    guard let region = paneContentHitRegions.reversed().first(where: {
      $0.role == role && $0.rect.contains(point)
    }) else {
      return nil
    }
    let hitView: NSView?
    let fallbackHitView: NSView?
    if let session = sessions[region.sessionId] {
      hitView = terminalPaneContentHitView(session, at: point)
      fallbackHitView = session.scrollView
    } else if let session = webPaneSessions[region.sessionId] {
      hitView = webPaneContentHitView(session, at: point)
      fallbackHitView = session.hostView
    } else {
      hitView = nil
      fallbackHitView = nil
    }
    let returnedHitView = hitView ?? fallbackHitView
    logPaneContentHitRouteIfNeeded(
      point: point,
      region: region,
      returnedHitView: returnedHitView)
    return returnedHitView
  }

  private func logPaneContentHitRouteIfNeeded(
    point: CGPoint,
    region: PaneContentHitRegion,
    returnedHitView: NSView?
  ) {
    let legacyCandidateSessionIds = paneContentLegacyCandidateSessionIds(
      at: point,
      role: region.role)
    let legacyFirstSessionId = legacyCandidateSessionIds.first
    let returnedSessionId = returnedHitView.flatMap { sessionId(containing: $0) }
    guard legacyFirstSessionId != region.sessionId || returnedSessionId != region.sessionId else {
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.paneContentHitRoutedByLayout",
      details: [
        "legacyCandidateSessionIds": legacyCandidateSessionIds,
        "legacyFirstSessionId": nullableString(legacyFirstSessionId),
        "layoutRegionPath": region.path,
        "layoutRegionRect": describeFrame(region.rect),
        "layoutRegionRole": region.role == .commands ? "commands" : "workspace",
        "layoutRegionSessionId": region.sessionId,
        "point": describePoint(point),
        "returnedHitView": returnedHitView.map { String(describing: type(of: $0)) } ?? "nil",
        "returnedSessionId": nullableString(returnedSessionId),
      ],
      force: true)
  }

  private func paneContentLegacyCandidateSessionIds(
    at point: CGPoint,
    role: PaneContentHitRole
  ) -> [String] {
    let sessionIds =
      role == .commands
      ? orderedVisibleCommandPaneOwnerSessionIds().reversed()
      : orderedVisiblePaneOwnerSessionIds().reversed()
    return sessionIds.filter { sessionId in
      if let session = sessions[sessionId] {
        return !session.containerView.isHidden
          && session.containerView.window != nil
          && session.containerView.frame.contains(point)
      }
      if let session = webPaneSessions[sessionId] {
        return !session.containerView.isHidden
          && session.containerView.window != nil
          && session.containerView.frame.contains(point)
      }
      return false
    }
  }

  private func terminalPaneContentHitView(_ session: TerminalSession, at point: CGPoint) -> NSView? {
    guard !session.containerView.isHidden,
      session.containerView.window != nil,
      session.containerView.frame.contains(point)
    else {
      return nil
    }
    if !session.searchBarView.isHidden {
      let searchPoint = convert(point, to: session.searchBarView)
      if session.searchBarView.bounds.contains(searchPoint) {
        let hitView = session.searchBarView.hitTest(searchPoint) ?? session.searchBarView
        logTerminalSearchInteraction(
          "nativeWorkspace.terminalSearch.hitTest",
          session: session,
          details: [
            "hitView": String(describing: type(of: hitView)),
            "rootPoint": describePoint(point),
            "searchBarFrame": describeFrame(session.searchBarView.frame),
            "searchPoint": describePoint(searchPoint),
          ])
        return hitView
      }
    }
    let contentPoint = convert(point, to: session.scrollView)
    if session.scrollView.bounds.contains(contentPoint) {
      return session.scrollView.hitTest(contentPoint) ?? session.scrollView
    }
    return nil
  }

  private func webPaneContentHitView(_ session: WebPaneSession, at point: CGPoint) -> NSView? {
    guard !session.containerView.isHidden,
      session.containerView.window != nil,
      session.containerView.frame.contains(point)
    else {
      return nil
    }
    let contentPoint = convert(point, to: session.hostView)
    if session.hostView.bounds.contains(contentPoint) {
      return session.hostView.hitTest(contentPoint) ?? session.hostView
    }
    return nil
  }

  override func mouseDown(with event: NSEvent) {
    guard !isProjectEditorInteractionSurfaceActive else {
      super.mouseDown(with: event)
      return
    }
    guard beginPaneResize(with: event) else {
      super.mouseDown(with: event)
      return
    }
  }

  @discardableResult
  private func beginPaneResize(with event: NSEvent) -> Bool {
    guard !isProjectEditorInteractionSurfaceActive else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)
    guard let hit = paneResizeHit(at: point) else {
      return false
    }
    return beginPaneResize(hit: hit, event: event)
  }

  @discardableResult
  private func beginPaneResize(hit: PaneResizeHit, event: NSEvent) -> Bool {
    guard !isProjectEditorInteractionSurfaceActive else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)

    if event.clickCount >= 2 {
      resetPaneResizeRatios(for: hit)
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "paneResizeReset")
      paneResizeCursor(for: hit.direction).set()
      return true
    }

    let currentRatios =
      paneResizeRatiosByPath[hit.path]
      ?? Array(repeating: 1, count: hit.trackCount)
    paneResizeDrag = PaneResizeDrag(
      availableLength: hit.availableLength,
      boundaryIndex: hit.boundaryIndex,
      direction: hit.direction,
      minimumAfter: paneResizeMinimumLength(direction: hit.direction)
        * CGFloat(hit.trackCount - hit.boundaryIndex),
      minimumBefore: paneResizeMinimumLength(direction: hit.direction) * CGFloat(hit.boundaryIndex),
      path: hit.path,
      startCoordinate: hit.direction == .horizontal ? point.x : point.y,
      startRatios: currentRatios
    )
    paneResizeCursor(for: hit.direction).set()
    return true
  }

  override func mouseDragged(with event: NSEvent) {
    guard !isProjectEditorInteractionSurfaceActive else {
      super.mouseDragged(with: event)
      return
    }
    guard continuePaneResize(with: event) else {
      super.mouseDragged(with: event)
      return
    }
  }

  @discardableResult
  private func continuePaneResize(with event: NSEvent) -> Bool {
    guard !isProjectEditorInteractionSurfaceActive else {
      paneResizeDrag = nil
      return false
    }
    guard let drag = paneResizeDrag else {
      return false
    }

    let point = convert(event.locationInWindow, from: nil)
    paneResizeCursor(for: drag.direction).set()
    let coordinate = drag.direction == .horizontal ? point.x : point.y
    let delta = drag.direction == .horizontal
      ? coordinate - drag.startCoordinate
      : drag.startCoordinate - coordinate
    paneResizeRatiosByPath[drag.path] = resizePaneRatios(
      drag.startRatios,
      boundaryIndex: drag.boundaryIndex,
      delta: delta,
      availableLength: drag.availableLength,
      minimumBefore: drag.minimumBefore,
      minimumAfter: drag.minimumAfter)
    needsLayout = true
    layoutSubtreeIfNeeded()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "paneResizeDrag")
    return true
  }

  override func mouseUp(with event: NSEvent) {
    guard !isProjectEditorInteractionSurfaceActive else {
      super.mouseUp(with: event)
      return
    }
    if endPaneResize(with: event) {
      return
    }
    paneHeaderDrag = nil
    uninstallPaneTabDragCaptureMonitor()
    endPaneHeaderDragFeedback()
    super.mouseUp(with: event)
  }

  @discardableResult
  private func beginCommandsPanelResize(with event: NSEvent) -> Bool {
    if event.clickCount >= 2 {
      resetCommandsPanelHeightRatio()
      NSCursor.resizeUpDown.set()
      return true
    }
    let point = convert(event.locationInWindow, from: nil)
    let startHeight = clampedCommandsPanelHeight(bounds.height * commandsPanelHeightRatio)
    commandsPanelResizeDrag = CommandsPanelResizeDrag(startHeight: startHeight, startY: point.y)
    NSCursor.resizeUpDown.set()
    return true
  }

  private func resetCommandsPanelHeightRatio() {
    /**
     CDXC:CommandsPanel 2026-05-14-07:18:
     Double-clicking the command pane's top resize rail must restore the pane to its original/default height, not start a drag resize.
     Emit the existing height-ratio event after updating native layout so persisted sidebar state and AppKit geometry stay on the same path as ordinary resize drags.
     */
    commandsPanelResizeDrag = nil
    commandsPanelHeightRatio = Self.defaultCommandsPanelHeightRatio
    needsLayout = true
    layoutSubtreeIfNeeded()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "commandsPanelResizeReset")
    sendEvent(.commandsPanelHeightRatioChanged(heightRatio: Double(commandsPanelHeightRatio)))
  }

  @discardableResult
  private func continueCommandsPanelResize(with event: NSEvent) -> Bool {
    guard let drag = commandsPanelResizeDrag else {
      return false
    }
    let point = convert(event.locationInWindow, from: nil)
    let nextHeight = clampedCommandsPanelHeight(drag.startHeight + point.y - drag.startY)
    commandsPanelHeightRatio = Self.clampedCommandsPanelHeightRatio(Double(nextHeight / max(bounds.height, 1)))
    NSCursor.resizeUpDown.set()
    needsLayout = true
    layoutSubtreeIfNeeded()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "commandsPanelResizeDrag")
    return true
  }

  @discardableResult
  private func endCommandsPanelResize(with event: NSEvent) -> Bool {
    guard commandsPanelResizeDrag != nil else {
      return false
    }
    _ = continueCommandsPanelResize(with: event)
    commandsPanelResizeDrag = nil
    needsLayout = true
    layoutSubtreeIfNeeded()
    sendEvent(.commandsPanelHeightRatioChanged(heightRatio: Double(commandsPanelHeightRatio)))
    NSCursor.resizeUpDown.set()
    return true
  }

  @discardableResult
  private func endPaneResize(with event: NSEvent) -> Bool {
    guard !isProjectEditorInteractionSurfaceActive else {
      paneResizeDrag = nil
      return false
    }
    guard paneResizeDrag != nil else {
      return false
    }
    paneResizeDrag = nil
    let point = convert(event.locationInWindow, from: nil)
    paneResizeCursor(at: point)?.set()
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "paneResizeEnd")
    return true
  }

  private func resetPaneHeaderInteractionState() {
    paneHeaderDrag = nil
    uninstallPaneTabDragCaptureMonitor()
    restoreBrowserContentAfterPaneTabDrag()
    endPaneHeaderDragFeedback(restoresCursor: false)
  }

  private func installPaneTabDragCaptureMonitorIfNeeded() {
    guard paneTabDragCaptureEventMonitor == nil else {
      return
    }
    /**
     CDXC:PaneTabs 2026-05-16-11:25:
     Native tab dragging must stay owned by TerminalWorkspaceView after tab
     mouse-down, even when the active pane is a CEF browser surface. Capture
     left-drag and left-up locally for the active paneHeaderDrag so Chromium
     cannot take over the mouse stream and terminate the native tab drag after a
     short movement.
     */
    paneTabDragCaptureEventMonitor = NSEvent.addLocalMonitorForEvents(
      matching: [.leftMouseDragged, .leftMouseUp]
    ) { [weak self] event in
      self?.handleCapturedPaneTabDragEvent(event) ?? event
    }
  }

  private func uninstallPaneTabDragCaptureMonitor() {
    guard let paneTabDragCaptureEventMonitor else {
      return
    }
    NSEvent.removeMonitor(paneTabDragCaptureEventMonitor)
    self.paneTabDragCaptureEventMonitor = nil
  }

  private func handleCapturedPaneTabDragEvent(_ event: NSEvent) -> NSEvent? {
    guard window != nil else {
      uninstallPaneTabDragCaptureMonitor()
      return event
    }
    if let eventWindow = event.window, eventWindow !== window {
      return event
    }
    guard let drag = paneHeaderDrag else {
      uninstallPaneTabDragCaptureMonitor()
      return event
    }
    switch event.type {
    case .leftMouseDragged:
      handlePaneTitleBarMouseDragged(event, sessionId: drag.sourceSessionId)
      return nil
    case .leftMouseUp:
      handlePaneTabMouseUp(event, sessionId: drag.sourceSessionId)
      return nil
    default:
      return event
    }
  }

  private func syncCEFNativeDragSourceReleaseMonitor(reason: String) {
    let shouldMonitor = window != nil && !isHidden && hasVisibleCEFInteractionSurface
    if shouldMonitor {
      installCEFNativeDragSourceReleaseMonitorIfNeeded()
    } else {
      if cefNativeDragSourceReleaseEventMonitor != nil {
        NativeT3CodePaneReproLog.append("nativeWorkspace.cef.dnd.monitor.uninstallRequested", [
          "reason": reason,
          "windowNumber": window?.windowNumber ?? NSNull(),
        ])
      }
      uninstallCEFNativeDragSourceReleaseMonitor()
    }
  }

  private var hasVisibleCEFInteractionSurface: Bool {
    if let activeProjectEditorId,
      let session = projectEditorPaneSessions[activeProjectEditorId],
      session.hostView.window != nil,
      !session.hostView.isHidden
    {
      return true
    }
    return webPaneSessions.values.contains { session in
      session.chromiumView != nil && session.containerView.window != nil && !session.containerView.isHidden
    }
  }

  private func installCEFNativeDragSourceReleaseMonitorIfNeeded() {
    guard cefNativeDragSourceReleaseEventMonitor == nil else {
      return
    }
    /**
     CDXC:ChromiumBrowserPanes 2026-05-07-05:18
     CEF's renderer handles real pointer movement during in-page drags. ghostex
     observes the native mouse stream and, after a real CEF drag-length
     movement releases, asks CEF to complete its drag source so the renderer
     receives the native end.

     CDXC:ChromiumBrowserPanes 2026-05-07-05:33
     CEF Alloy panes show native mouse movement during HTML drags, but VS Code's
     composite/view drop targets do not always receive browser `dragover`
     retargeting while the pointer moves. Bridge only the in-drag hover/drop
     retargeting into the page; Chromium still owns the original drag source
     and the native source-ended completion.

     CDXC:ChromiumBrowserPanes 2026-05-07-06:32
     Chromium's native drag loop can suppress AppKit's normal
     `leftMouseDragged` delivery to local monitors, so one-shot dragover
     forwarding only reacts at drag start/release. Poll the current mouse
     location in common run-loop modes during an active CEF drag so VS Code
     drop targets keep receiving hover updates while the user holds or moves.

     CDXC:ChromiumBrowserPanes 2026-05-07-07:22
     VS Code drop indicators flicker when bridged hover events retarget every
     small DOM child under the pointer or arrive faster than the workbench's
     drop observer needs. Coalesce native hover dispatch to a stable cadence and
     let the in-page bridge stabilize dragenter/dragleave targets while still
     sending periodic dragover events for hold-to-drop workflows.

     CDXC:ChromiumBrowserPanes 2026-05-07-07:33
     A 07:31 failed editor drag had no CEF hover bridge log entries while the
     07:33 successful drag did. Chromium can start its native HTML drag before
     AppKit delivers a threshold-crossing `leftMouseDragged` event, so arm the
     poller on mouse-down and let the poller detect the drag threshold itself.
     */
    /**
     CDXC:ChromiumBrowserPanes 2026-05-11-20:24
     Keep the CEF drag monitor scoped to visible Chromium interaction surfaces.
     Terminal panes must not arm CEF drag state or keep the hover poller alive.
     */
    cefNativeDragSourceReleaseEventMonitor = NSEvent.addLocalMonitorForEvents(
      matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp]
    ) { [weak self] event in
      guard let self else {
        return event
      }
      self.handleCEFNativeDragSourceReleaseMonitorEvent(event)
      return event
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.cef.dnd.monitor.installed", [
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  private func uninstallCEFNativeDragSourceReleaseMonitor() {
    cefNativeDragSourceRelease = nil
    stopCEFNativeDragHoverTimer()
    guard let cefNativeDragSourceReleaseEventMonitor else {
      return
    }
    NSEvent.removeMonitor(cefNativeDragSourceReleaseEventMonitor)
    self.cefNativeDragSourceReleaseEventMonitor = nil
  }

  private func handleCEFNativeDragSourceReleaseMonitorEvent(_ event: NSEvent) {
    guard window != nil, !isHidden else {
      cefNativeDragSourceRelease = nil
      stopCEFNativeDragHoverTimer()
      return
    }
    guard cefNativeDragSourceRelease != nil || hasVisibleCEFInteractionSurface else {
      return
    }
    guard let windowPoint = windowPoint(forCEFNativeDragEvent: event) else {
      return
    }
    switch event.type {
    case .leftMouseDown:
      guard let chromiumView = chromiumBrowserView(atWindowPoint: windowPoint) else {
        cefNativeDragSourceRelease = nil
        stopCEFNativeDragHoverTimer()
        return
      }
      cefNativeDragSourceRelease = CEFNativeDragSourceRelease(
        chromiumView: chromiumView,
        startWindowPoint: windowPoint,
        didDrag: false,
        didStartHoverBridge: false,
        lastHoverEventTime: 0,
        lastHoverWindowPoint: nil,
        lastHoverLogEventTime: 0)
      startCEFNativeDragHoverTimerIfNeeded()
    case .leftMouseDragged:
      guard var release = cefNativeDragSourceRelease else {
        return
      }
      if !release.didDrag,
        hypot(
          windowPoint.x - release.startWindowPoint.x,
          windowPoint.y - release.startWindowPoint.y) >= Self.paneHeaderDragThreshold
      {
        release.didDrag = true
        startCEFNativeDragHoverTimerIfNeeded()
      }
      guard release.didDrag else {
        cefNativeDragSourceRelease = release
        return
      }
      if !release.didStartHoverBridge {
        dispatchCEFDragHoverUpdate(
          for: &release,
          windowPoint: windowPoint,
          eventTime: event.timestamp,
          phase: "start")
        release.didStartHoverBridge = true
      }
      cefNativeDragSourceRelease = release
    case .leftMouseUp:
      finishCEFNativeDragRelease(windowPoint: windowPoint, eventTime: event.timestamp)
    default:
      return
    }
  }

  private func startCEFNativeDragHoverTimerIfNeeded() {
    guard cefNativeDragHoverTimer == nil else {
      return
    }
    let timer = Timer(timeInterval: Self.cefNativeDragHoverInterval, repeats: true) { [weak self] _ in
      MainActor.assumeIsolated {
        self?.pumpCEFNativeDragHoverTimer()
      }
    }
    cefNativeDragHoverTimer = timer
    RunLoop.main.add(timer, forMode: .common)
  }

  private func stopCEFNativeDragHoverTimer() {
    cefNativeDragHoverTimer?.invalidate()
    cefNativeDragHoverTimer = nil
  }

  private func pumpCEFNativeDragHoverTimer() {
    guard var release = cefNativeDragSourceRelease else {
      stopCEFNativeDragHoverTimer()
      return
    }
    guard let windowPoint = currentCEFNativeDragWindowPoint() else {
      return
    }
    guard NSEvent.pressedMouseButtons & 1 == 1 else {
      guard release.didDrag else {
        cefNativeDragSourceRelease = nil
        stopCEFNativeDragHoverTimer()
        return
      }
      finishCEFNativeDragRelease(windowPoint: windowPoint, eventTime: CACurrentMediaTime())
      return
    }
    if !release.didDrag {
      guard hypot(
        windowPoint.x - release.startWindowPoint.x,
        windowPoint.y - release.startWindowPoint.y) >= Self.paneHeaderDragThreshold
      else {
        cefNativeDragSourceRelease = release
        return
      }
      release.didDrag = true
    }
    dispatchCEFDragHoverUpdate(
      for: &release,
      windowPoint: windowPoint,
      eventTime: CACurrentMediaTime(),
      phase: release.didStartHoverBridge ? "over" : "start")
    release.didStartHoverBridge = true
    cefNativeDragSourceRelease = release
  }

  private func finishCEFNativeDragRelease(windowPoint: CGPoint, eventTime: TimeInterval) {
    guard var release = cefNativeDragSourceRelease else {
      return
    }
    cefNativeDragSourceRelease = nil
    stopCEFNativeDragHoverTimer()
    guard release.didDrag else {
      return
    }
    dispatchCEFDragHoverUpdate(
      for: &release,
      windowPoint: windowPoint,
      eventTime: eventTime,
      phase: "drop",
      force: true)
    release.chromiumView.completeCurrentDrag(atWindowPoint: windowPoint)
    NativeT3CodePaneReproLog.append("nativeWorkspace.cef.dnd.nativeSourceReleaseCompleted", [
      "windowNumber": window?.windowNumber ?? NSNull(),
      "x": windowPoint.x,
      "y": windowPoint.y,
    ])
  }

  private func dispatchCEFDragHoverUpdate(
    for release: inout CEFNativeDragSourceRelease,
    windowPoint: CGPoint,
    eventTime: TimeInterval,
    phase: String,
    force: Bool = false
  ) {
    let localPoint = release.chromiumView.convert(windowPoint, from: nil)
    guard release.chromiumView.bounds.contains(localPoint) else {
      guard force || release.didStartHoverBridge else {
        return
      }
      release.chromiumView.executeJavaScript(
        Self.cefDragHoverBridgeScript(
          x: 0,
          y: 0,
          phase: "cancel",
          sourceX: nil,
          sourceY: nil))
      return
    }
    if !force, phase == "over", let lastPoint = release.lastHoverWindowPoint {
      let elapsed = eventTime - release.lastHoverEventTime
      let distance = hypot(windowPoint.x - lastPoint.x, windowPoint.y - lastPoint.y)
      if distance < Self.cefNativeDragHoverMinimumDistance {
        guard elapsed >= Self.cefNativeDragStationaryHoverInterval else {
          return
        }
      } else {
        guard elapsed >= Self.cefNativeDragHoverInterval else {
          return
        }
      }
    }
    release.lastHoverEventTime = eventTime
    release.lastHoverWindowPoint = windowPoint
    release.chromiumView.executeJavaScript(
      Self.cefDragHoverBridgeScript(
        x: localPoint.x,
        y: localPoint.y,
        phase: phase,
        sourceX: release.chromiumView.convert(release.startWindowPoint, from: nil).x,
        sourceY: release.chromiumView.convert(release.startWindowPoint, from: nil).y))
    if phase != "over" || eventTime - release.lastHoverLogEventTime >= 0.25 {
      release.lastHoverLogEventTime = eventTime
      NativeT3CodePaneReproLog.append("nativeWorkspace.cef.dnd.hoverBridgeDispatched", [
        "phase": phase,
        "windowNumber": window?.windowNumber ?? NSNull(),
        "x": windowPoint.x,
        "y": windowPoint.y,
      ])
    }
  }

  private func windowPoint(forCEFNativeDragEvent event: NSEvent) -> CGPoint? {
    if event.window === window {
      return event.locationInWindow
    }
    guard event.window == nil, let window else {
      return nil
    }
    return window.convertPoint(fromScreen: NSEvent.mouseLocation)
  }

  private func currentCEFNativeDragWindowPoint() -> CGPoint? {
    guard let window else {
      return nil
    }
    return window.convertPoint(fromScreen: NSEvent.mouseLocation)
  }

  private func chromiumBrowserView(atWindowPoint windowPoint: CGPoint) -> GhostexCEFBrowserView? {
    guard let hitView = window?.contentView?.hitTest(windowPoint) else {
      return nil
    }
    var currentView: NSView? = hitView
    while let view = currentView {
      if let chromiumView = view as? GhostexCEFBrowserView {
        return chromiumView
      }
      currentView = view.superview
    }
    return nil
  }

  private func paneResizeHit(at point: CGPoint) -> PaneResizeHit? {
    paneResizeHitRecord(at: point)?.hit
  }

  private func paneResizeHitRecord(at point: CGPoint) -> (index: Int, hit: PaneResizeHit)? {
    paneResizeHits.enumerated()
      .filter { $0.element.rect.contains(point) }
      .min { left, right in
        let leftDistance = paneResizeDistance(from: point, to: left.element)
        let rightDistance = paneResizeDistance(from: point, to: right.element)
        return leftDistance < rightDistance
      }
      .map { (index: $0.offset, hit: $0.element) }
  }

  private func paneResizeDistance(from point: CGPoint, to hit: PaneResizeHit) -> CGFloat {
    switch hit.direction {
    case .horizontal:
      abs(point.x - hit.rect.midX)
    case .vertical:
      abs(point.y - hit.rect.midY)
    }
  }

  private func paneResizeMinimumLength(direction: NativeTerminalLayout.SplitDirection) -> CGFloat {
    direction == .horizontal ? Self.paneResizeMinimumWidth : Self.paneResizeMinimumHeight
  }

  private func resizePaneRatios(
    _ ratios: [CGFloat],
    boundaryIndex: Int,
    delta: CGFloat,
    availableLength: CGFloat,
    minimumBefore: CGFloat,
    minimumAfter: CGFloat
  ) -> [CGFloat] {
    let ratioTotal = ratios.reduce(0, +)
    guard ratios.count > 1, boundaryIndex > 0, boundaryIndex < ratios.count, ratioTotal > 0,
      availableLength > minimumBefore + minimumAfter
    else {
      return ratios
    }
    let beforeRatio = ratios.prefix(boundaryIndex).reduce(0, +)
    let afterRatio = ratioTotal - beforeRatio
    guard beforeRatio > 0, afterRatio > 0 else { return ratios }
    let beforeLength = beforeRatio / ratioTotal * availableLength
    let nextBeforeLength = min(
      max(beforeLength + delta, minimumBefore),
      availableLength - minimumAfter)
    let nextBeforeRatio = nextBeforeLength / availableLength * ratioTotal
    let nextAfterRatio = ratioTotal - nextBeforeRatio
    let beforeScale = nextBeforeRatio / beforeRatio
    let afterScale = nextAfterRatio / afterRatio
    return ratios.enumerated().map { index, ratio in
      ratio * (index < boundaryIndex ? beforeScale : afterScale)
    }
  }

  private func resetPaneResizeRatios(for hit: PaneResizeHit) {
    /**
     CDXC:NativePaneResize 2026-05-14-07:04
     A double-click reset is local to the clicked divider. Removing only that
     path's live resize override lets layoutTree rebuild the split from its
     persisted default ratio while preserving independent user sizing in nested
     splits.
     */
    paneResizeRatiosByPath.removeValue(forKey: hit.path)
    needsLayout = true
    layoutSubtreeIfNeeded()
  }

  /**
   CDXC:NativePaneReorder 2026-05-02-17:33
   Native Ghostty and T3 panes are AppKit/WKWebView surfaces above the React
   workspace DOM, so pane header drag-to-reorder must be detected in AppKit and
   reported to the sidebar state owner. A short movement threshold preserves
   normal title-bar clicks for focus while drags swap the source pane with the
   pane under the release point.
   */
  private func handlePaneTitleBarMouseDown(
    _ event: NSEvent,
    sessionId: String,
    focusReason: String
  ) {
    let startPoint = convert(event.locationInWindow, from: nil)
    guard paneTitleBarFrame(for: sessionId)?.contains(startPoint) == true else {
      /**
       CDXC:NativePaneReorder 2026-05-11-01:16
       A stale or misplaced TerminalSessionTitleBarView can still receive
       AppKit mouseDown before hit-test rerouting has invalidated it. Plain
       title-bar clicks only focus panes now, but they still must match the
       current registered title-bar frame before focus changes.
       */
      logPaneReorderProbe(
        event: "nativePaneReorder.titleBar.mouseDownRejected",
        at: startPoint,
        details: [
          "focusReason": focusReason,
          "sessionId": sessionId,
          "titleBarFrame": describeFrame(paneTitleBarFrame(for: sessionId) ?? .zero),
        ])
      return
    }
    /**
     CDXC:NativePaneReorderDiagnostics 2026-05-11-01:16
     Pane reordering now starts from tabs only. A plain title-bar click still
     focuses the pane, but must not create paneHeaderDrag state; otherwise empty
     title-bar drags compete with tab dragging and terminal text selection.
     */
    TerminalFocusDebugLog.append(
      event: "nativePaneTitleBar.focusMouseDown",
      details: [
        "focusReason": focusReason,
        "sessionId": sessionId,
        "startPoint": describeFrame(
            CGRect(x: startPoint.x, y: startPoint.y, width: 0, height: 0)),
      ])
    logPaneReorderProbe(
      event: "nativePaneReorder.titleBar.mouseDown",
      at: startPoint,
      details: [
        "focusReason": focusReason,
        "sessionId": sessionId,
        "titleBarFrame": describeFrame(paneTitleBarFrame(for: sessionId) ?? .zero),
      ])
    emitAttentionAcknowledgementClickIfNeeded(sessionId: sessionId, reason: focusReason)
    if commandsPanelActiveSessionIds.contains(sessionId), !commandsPanelIsVisible {
      NativePaneTabDragReproLog.append(event: "nativeCommandsPanel.collapsedTitleBar.expandRequested", details: [
        "hitPoint": nativePaneTabsDebugFrame(CGRect(x: startPoint.x, y: startPoint.y, width: 0, height: 0)),
        "sessionId": sessionId,
      ])
      sendEvent(.paneTabSelected(sessionId: sessionId))
      return
    }
    focusSession(sessionId: sessionId, reason: focusReason)
  }

  private func handlePaneTabMouseDown(_ event: NSEvent, sessionId: String) {
    let startPoint = convert(event.locationInWindow, from: nil)
    /**
     CDXC:PaneTabs 2026-05-10-18:30
     Tab buttons can represent inactive sessions whose own title-bar view is
     offscreen. Start a pane drag from the tab's session id without requiring
     the event point to match that hidden title-bar frame; a click without drag
     selects the tab on mouse-up.
     */
    paneHeaderDrag = PaneHeaderDrag(
      isDragging: false,
      lastLoggedMoveAt: 0,
      moveEventCount: 0,
      sourceSessionId: sessionId,
      startedFromTab: true,
      startPoint: startPoint,
      targetSessionId: nil)
    installPaneTabDragCaptureMonitorIfNeeded()
    NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.mouseDown", details: [
      "sessionId": sessionId,
      "startPoint": describeFrame(CGRect(x: startPoint.x, y: startPoint.y, width: 0, height: 0)),
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
  }

  private func handlePaneTabMouseUp(_ event: NSEvent, sessionId: String) {
    guard let drag = paneHeaderDrag, drag.sourceSessionId == sessionId else {
      uninstallPaneTabDragCaptureMonitor()
      restoreBrowserContentAfterPaneTabDrag()
      NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.mouseUp.noDragState", details: [
        "sessionId": sessionId,
        "windowNumber": event.window?.windowNumber ?? NSNull(),
      ])
      sendEvent(.paneTabSelected(sessionId: sessionId))
      return
    }
    if !drag.isDragging {
      paneHeaderDrag = nil
      uninstallPaneTabDragCaptureMonitor()
      restoreBrowserContentAfterPaneTabDrag()
      if event.clickCount >= 2 {
        /**
         CDXC:SessionFocusMode 2026-05-23-09:28:
         Double-clicking a native pane tab is a reversible Focus intent, not
         another tab selection. Send a separate event after clearing drag state
         so the sidebar can zoom the tab group and switch back to Agents mode.
         */
        NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.doubleClickFocusRequested", details: [
          "sessionId": sessionId,
          "windowNumber": event.window?.windowNumber ?? NSNull(),
        ])
        sendEvent(.paneTabFocusRequested(sessionId: sessionId))
        return
      }
      NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.clickSelected", details: [
        "sessionId": sessionId,
        "windowNumber": event.window?.windowNumber ?? NSNull(),
      ])
      NativePaneTabDragReproLog.append(event: "nativePaneTabs.hostEvent.send.paneTabSelected", details: [
        "sessionId": sessionId,
        "source": "tabButtonMouseUp",
      ])
      sendEvent(.paneTabSelected(sessionId: sessionId))
      return
    }
    handlePaneTitleBarMouseUp(event, sessionId: sessionId)
  }

  private func handlePaneTitleBarMouseDragged(_ event: NSEvent, sessionId: String) {
    guard var drag = paneHeaderDrag, drag.sourceSessionId == sessionId else {
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    if !drag.isDragging,
      hypot(point.x - drag.startPoint.x, point.y - drag.startPoint.y)
        < Self.paneHeaderDragThreshold
    {
      return
    }
    if !drag.isDragging {
      drag.isDragging = true
      paneHeaderDrag = drag
      TerminalFocusDebugLog.append(
        event: "nativePaneReorder.dragStarted",
        details: [
          "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
          "sessionId": sessionId,
          "startPoint": describeFrame(
            CGRect(x: drag.startPoint.x, y: drag.startPoint.y, width: 0, height: 0)),
        ])
      logPaneReorderProbe(
        event: "nativePaneReorder.dragStarted",
        at: point,
        details: [
          "sessionId": sessionId,
          "startPoint": describeFrame(
            CGRect(x: drag.startPoint.x, y: drag.startPoint.y, width: 0, height: 0)),
        ])
      beginPaneHeaderDragFeedback(for: drag.sourceSessionId, at: point)
    }
    updatePaneHeaderDragFeedback(
      for: drag.sourceSessionId,
      at: point,
      eventTimestamp: event.timestamp)
  }

  private func handlePaneTitleBarMouseUp(_ event: NSEvent, sessionId: String) {
    guard let drag = paneHeaderDrag, drag.sourceSessionId == sessionId else {
      uninstallPaneTabDragCaptureMonitor()
      restoreBrowserContentAfterPaneTabDrag()
      return
    }
    paneHeaderDrag = nil
    uninstallPaneTabDragCaptureMonitor()
    endPaneHeaderDragFeedback()
    guard drag.isDragging else {
      /**
       CDXC:PaneTabs 2026-05-11-19:36
       Tab selection is a native title-bar mouseUp responsibility. No
       window-local monitor may release or synthesize tab clicks, because
       narrow-pane tabs and inline buttons need one reliable AppKit owner for
       mouseDown/mouseUp.
       */
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    if let tabReorderTarget = paneTabReorderDropTarget(at: point, sourceSessionId: drag.sourceSessionId) {
      TerminalFocusDebugLog.append(
        event: "nativePaneTabReorder.dropRequested",
        details: [
          "ownerSessionId": tabReorderTarget.ownerSessionId,
          "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
          "position": tabReorderTarget.position.rawValue,
          "sourceSessionId": drag.sourceSessionId,
          "targetSessionId": tabReorderTarget.targetSessionId,
        ])
      NativePaneTabDragReproLog.append(event: "nativePaneTabReorder.dropRequested", details: [
        "ownerSessionId": tabReorderTarget.ownerSessionId,
        "position": tabReorderTarget.position.rawValue,
        "sourceSessionId": drag.sourceSessionId,
        "targetSessionId": tabReorderTarget.targetSessionId,
        "windowNumber": event.window?.windowNumber ?? NSNull(),
      ])
      sendEvent(
        .paneTabReorderRequested(
          sourceSessionId: drag.sourceSessionId,
          targetSessionId: tabReorderTarget.targetSessionId,
          position: tabReorderTarget.position))
      return
    }
    if isPointInSourceTabStrip(point, sourceSessionId: drag.sourceSessionId) {
      /**
       CDXC:PaneTabs 2026-05-11-01:43
       Dropping a tab back onto its original insertion slot is a no-op. The drag
       feedback hides the insertion line in that state, and mouse-up must not
       fall through to pane split/drop handling just because the tab strip has no
       effective reorder target.
       */
      NativePaneTabDragReproLog.append(event: "nativePaneTabReorder.dropIgnoredNoop", details: [
        "sourceSessionId": drag.sourceSessionId,
        "windowNumber": event.window?.windowNumber ?? NSNull(),
      ])
      return
    }
    guard let dropTarget = paneHeaderDropTarget(at: point, sourceSessionId: drag.sourceSessionId) else {
      TerminalFocusDebugLog.append(
        event: "nativePaneReorder.dropIgnored",
        details: [
          "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
          "sourceSessionId": drag.sourceSessionId,
          "targetSessionId": paneSessionId(at: point, sourceSessionId: drag.sourceSessionId) ?? NSNull(),
        ])
      if drag.startedFromTab {
        NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.dropIgnored", details: [
          "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
          "sourceSessionId": drag.sourceSessionId,
          "targetSessionId": paneSessionId(at: point, sourceSessionId: drag.sourceSessionId) ?? NSNull(),
          "windowNumber": event.window?.windowNumber ?? NSNull(),
        ])
      }
      return
    }
    TerminalFocusDebugLog.append(
      event: "nativePaneReorder.dropRequested",
      details: [
        "placement": dropTarget.placement.rawValue,
        "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
        "sourceSessionId": drag.sourceSessionId,
        "targetSessionId": dropTarget.targetSessionId,
      ])
    if drag.startedFromTab {
      NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.dropRequested", details: [
        "feedbackSessionId": dropTarget.feedbackSessionId,
        "placement": dropTarget.placement.rawValue,
        "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
        "sourceSessionId": drag.sourceSessionId,
        "targetFrame": paneFrame(for: dropTarget.feedbackSessionId).map(describeFrame) ?? NSNull(),
        "targetSessionId": dropTarget.targetSessionId,
        "windowNumber": event.window?.windowNumber ?? NSNull(),
      ])
    }
    sendEvent(
      .paneReorderRequested(
        sourceSessionId: drag.sourceSessionId,
        targetSessionId: dropTarget.targetSessionId,
        placement: dropTarget.placement))
    if drag.startedFromTab {
      NativePaneTabDragReproLog.append(event: "nativePaneTabs.hostEvent.sent.paneReorderRequested", details: [
        "feedbackSessionId": dropTarget.feedbackSessionId,
        "placement": dropTarget.placement.rawValue,
        "sourceSessionId": drag.sourceSessionId,
        "targetSessionId": dropTarget.targetSessionId,
      ])
    }
  }

  private func paneDropPlacement(at point: CGPoint, targetSessionId: String) -> PaneDropPlacement {
    /**
     CDXC:PaneTabs 2026-05-10-18:30
     Dragging a terminal over the middle of another pane groups it as a tab;
     dragging onto an edge splits beside that pane. Keep the target bands local
     to the target pane so split intent does not depend on global workspace
     geometry.
     */
    guard let frame = paneBorderFrame(for: targetSessionId), frame.width > 1, frame.height > 1 else {
      return .center
    }
    let localX = (point.x - frame.minX) / frame.width
    let localY = (point.y - frame.minY) / frame.height
    let edgeBand: CGFloat = 0.24
    if localX <= edgeBand {
      return .left
    }
    if localX >= 1 - edgeBand {
      return .right
    }
    if localY <= edgeBand {
      return .bottom
    }
    if localY >= 1 - edgeBand {
      return .top
    }
    return .center
  }

  private func commandPaneDropPlacement(at point: CGPoint, targetSessionId: String) -> PaneDropPlacement {
    /**
     CDXC:CommandsPanel 2026-05-15-08:59
     Command-terminal tab dragging should support the same horizontal edge
     split intent as workspace tabs. The command panel does not have vertical
     split behavior, so classify only left and right edge bands and keep the
     rest of the command pane as a center tab-group drop.
     */
    guard let frame = paneBorderFrame(for: targetSessionId), frame.width > 1 else {
      return .center
    }
    let localX = (point.x - frame.minX) / frame.width
    let edgeBand: CGFloat = 0.24
    if localX <= edgeBand {
      return .left
    }
    if localX >= 1 - edgeBand {
      return .right
    }
    return .center
  }

  /**
   CDXC:NativePaneReorder 2026-05-03-03:57
   Reordering panes needs immediate native feedback because AppKit/WKWebView
   pane surfaces do not show the React session-card drag affordances. While a
   title bar is dragged, show a compact header ghost capped at 230px and outline
   the pane that will receive the drop.
   */
  private func beginPaneHeaderDragFeedback(for sessionId: String, at point: CGPoint) {
    logStalePaneDragFeedbackIfMounted(reason: "beginPaneHeaderDragFeedback")
    hideBrowserContentDuringPaneTabDragIfNeeded()
    let ghostView = paneHeaderDragGhostView ?? TerminalPaneHeaderDragGhostView()
    paneHeaderDragGhostView = ghostView
    if ghostView.superview !== self {
      addSubview(ghostView)
    }
    ghostView.configure(
      title: paneHeaderDisplayTitle(for: sessionId),
      favicon: paneHeaderFavicon(for: sessionId),
      agentIconDataUrl: sessionAgentIconDataUrls[sessionId],
      agentIconColorHex: sessionAgentIconColors[sessionId],
      maxWidth: Self.paneHeaderDragGhostMaxWidth
    )
    ghostView.layer?.zPosition = Self.paneHeaderDragFeedbackZPosition
    ghostView.alphaValue = 0.92
    ghostView.isHidden = false
    updatePaneHeaderDragFeedback(for: sessionId, at: point, eventTimestamp: nil)
  }

  private func updatePaneHeaderDragFeedback(
    for sourceSessionId: String,
    at point: CGPoint,
    eventTimestamp: TimeInterval?
  ) {
    let ghostOrigin = paneHeaderDragGhostOrigin(
      for: paneHeaderDragGhostView?.frame.size ?? .zero,
      cursorPoint: point)
    if let ghostView = paneHeaderDragGhostView {
      setPaneDragFeedbackFrame(
        CGRect(origin: ghostOrigin, size: ghostView.frame.size),
        for: ghostView)
    }
    let tabReorderTarget = paneTabReorderDropTarget(at: point, sourceSessionId: sourceSessionId)
    if let tabReorderTarget {
      updatePaneTabReorderTarget(tabReorderTarget)
      updatePaneHeaderDropTarget(feedbackSessionId: nil, placement: nil)
      if var drag = paneHeaderDrag, drag.sourceSessionId == sourceSessionId {
        let targetChanged = drag.targetSessionId != tabReorderTarget.targetSessionId
        drag.targetSessionId = tabReorderTarget.targetSessionId
        logPaneTabDragMoveIfNeeded(
          drag: &drag,
          eventTimestamp: eventTimestamp,
          ghostOrigin: ghostOrigin,
          placement: nil,
          point: point,
          targetChanged: targetChanged,
          targetSessionId: tabReorderTarget.targetSessionId)
        paneHeaderDrag = drag
      }
      return
    }
    if isPointInSourceTabStrip(point, sourceSessionId: sourceSessionId) {
      updatePaneTabReorderTarget(nil)
      updatePaneHeaderDropTarget(feedbackSessionId: nil, placement: nil)
      if var drag = paneHeaderDrag, drag.sourceSessionId == sourceSessionId {
        let targetChanged = drag.targetSessionId != nil
        drag.targetSessionId = nil
        logPaneTabDragMoveIfNeeded(
          drag: &drag,
          eventTimestamp: eventTimestamp,
          ghostOrigin: ghostOrigin,
          placement: nil,
          point: point,
          targetChanged: targetChanged,
          targetSessionId: nil)
        paneHeaderDrag = drag
      }
      return
    }
    updatePaneTabReorderTarget(nil)
    let dropTarget = paneHeaderDropTarget(at: point, sourceSessionId: sourceSessionId)
    updatePaneHeaderDropTarget(
      feedbackSessionId: dropTarget?.feedbackSessionId,
      placement: dropTarget?.placement)
    if var drag = paneHeaderDrag, drag.sourceSessionId == sourceSessionId {
      let nextTargetSessionId = dropTarget?.targetSessionId
      let targetChanged = drag.targetSessionId != nextTargetSessionId
      drag.targetSessionId = nextTargetSessionId
      logPaneTabDragMoveIfNeeded(
        drag: &drag,
        eventTimestamp: eventTimestamp,
        ghostOrigin: ghostOrigin,
        placement: dropTarget?.placement,
        point: point,
        targetChanged: targetChanged,
        targetSessionId: nextTargetSessionId)
      paneHeaderDrag = drag
    }
  }

  private func endPaneHeaderDragFeedback(restoresCursor: Bool = true) {
    restoreBrowserContentAfterPaneTabDrag()
    paneHeaderDragGhostView?.removeFromSuperview()
    paneHeaderDragGhostView = nil
    paneHeaderDragTargetView?.removeFromSuperview()
    paneHeaderDragTargetView = nil
    paneTabReorderTargetView?.removeFromSuperview()
    paneTabReorderTargetView = nil
    _ = restoresCursor
  }

  private func hideBrowserContentDuringPaneTabDragIfNeeded() {
    guard paneHeaderDrag?.startedFromTab == true else {
      return
    }
    /**
     CDXC:PaneTabs 2026-05-16-12:51:
     Browser pane tab dragging should hide CEF page content for the duration of
     the native tab drag. The tab drawer, drag ghost, insertion line, and
     split/drop target still stay native and visible, but the accelerated
     Chromium page surface is removed from compositing so dragging any tab over
     or near an active browser pane cannot flicker the browser contents.
     */
    for session in webPaneSessions.values {
      guard !session.containerView.isHidden,
        !session.hostView.isHidden,
        let chromiumView = session.chromiumView
      else {
        continue
      }
      hidePaneTabDragBrowserContentView(chromiumView)
    }
    for session in projectEditorPaneSessions.values where session.projectId == activeProjectEditorId {
      for tab in session.tabs where !tab.hostView.isHidden {
        if let chromiumView = tab.chromiumView {
          hidePaneTabDragBrowserContentView(chromiumView)
        }
      }
    }
  }

  private func hidePaneTabDragBrowserContentView(_ view: NSView) {
    guard !view.isHidden else {
      return
    }
    paneTabDragHiddenBrowserContentViews[ObjectIdentifier(view)] = view
    setHidden(true, for: view)
  }

  private func restoreBrowserContentAfterPaneTabDrag() {
    guard !paneTabDragHiddenBrowserContentViews.isEmpty else {
      return
    }
    for view in paneTabDragHiddenBrowserContentViews.values {
      setHidden(false, for: view)
    }
    paneTabDragHiddenBrowserContentViews.removeAll()
  }

  private func logStalePaneDragFeedbackIfMounted(reason: String) {
    let mountedFeedback = [
      paneHeaderDragGhostView?.superview == nil ? nil : "ghost",
      paneHeaderDragTargetView?.superview == nil ? nil : "dropTarget",
      paneTabReorderTargetView?.superview == nil ? nil : "tabReorder",
    ].compactMap { $0 }
    guard !mountedFeedback.isEmpty else {
      return
    }
    /**
     CDXC:PaneDragFeedback 2026-05-11-20:24
     Pane drag feedback is visual-only and click-through. If a new drag starts
     while old feedback remains mounted, log it so stale visual layers are
     diagnosed without giving those layers input ownership.
     */
    NativePaneTabDragReproLog.append(event: "nativePaneDragFeedback.staleMounted", details: [
      "mountedFeedback": mountedFeedback,
      "reason": reason,
    ])
  }

  private func paneTabReorderDropTarget(
    at point: CGPoint,
    sourceSessionId: String
  ) -> PaneTabReorderDropTarget? {
    for (ownerSessionId, titleBarView) in visiblePaneTitleBarViews(for: sourceSessionId) {
      let titleBarPoint = convert(point, to: titleBarView)
      guard let target = titleBarView.tabReorderTarget(
        at: titleBarPoint,
        sourceSessionId: sourceSessionId)
      else {
        continue
      }
      return PaneTabReorderDropTarget(
        lineFrame: titleBarView.convert(target.lineFrame, to: self),
        ownerSessionId: ownerSessionId,
        position: target.position,
        targetSessionId: target.targetSessionId)
    }
    return nil
  }

  private func isPointInSourceTabStrip(_ point: CGPoint, sourceSessionId: String) -> Bool {
    for (_, titleBarView) in visiblePaneTitleBarViews(for: sourceSessionId) {
      let titleBarPoint = convert(point, to: titleBarView)
      if titleBarView.containsTab(sourceSessionId) && titleBarView.isTabStripPoint(titleBarPoint) {
        return true
      }
    }
    return false
  }

  private func paneHeaderDropTarget(
    at point: CGPoint,
    sourceSessionId: String
  ) -> PaneHeaderDropTarget? {
    guard let feedbackSessionId = paneSessionId(at: point, sourceSessionId: sourceSessionId) else {
      return nil
    }
    let placement = commandsPanelActiveSessionIds.contains(sourceSessionId)
      ? commandPaneDropPlacement(at: point, targetSessionId: feedbackSessionId)
      : paneDropPlacement(at: point, targetSessionId: feedbackSessionId)
    if feedbackSessionId != sourceSessionId {
      return PaneHeaderDropTarget(
        feedbackSessionId: feedbackSessionId,
        placement: placement,
        targetSessionId: feedbackSessionId)
    }
    guard placement != .center,
      let targetSessionId = samePaneSplitAnchorSessionId(
        for: feedbackSessionId,
        sourceSessionId: sourceSessionId,
        placeAfterTarget: placement == .right || placement == .bottom)
    else {
      return nil
    }
    /**
     CDXC:PaneTabs 2026-05-12-11:08
     Dragging the active tab to the edge of its own multi-tab pane must show and
     execute split drop targets. Use the visible active pane for feedback, but
     send a remaining tab sibling as the mutation target so the sidebar can
     remove the source tab first and still find the pane to split.
     */
    return PaneHeaderDropTarget(
      feedbackSessionId: feedbackSessionId,
      placement: placement,
      targetSessionId: targetSessionId)
  }

  private func samePaneSplitAnchorSessionId(
    for feedbackSessionId: String,
    sourceSessionId: String,
    placeAfterTarget: Bool
  ) -> String? {
    for (ownerSessionId, titleBarView) in visiblePaneTitleBarViews(for: sourceSessionId)
    where ownerSessionId == feedbackSessionId && titleBarView.containsTab(sourceSessionId) {
      return titleBarView.splitAnchorSessionId(
        excluding: sourceSessionId,
        placeAfterTarget: placeAfterTarget)
    }
    return nil
  }

  private func visiblePaneTitleBarViews(
    for sourceSessionId: String? = nil
  ) -> [(ownerSessionId: String, titleBarView: TerminalSessionTitleBarView)] {
    /**
     CDXC:CommandsPanel 2026-05-15-08:59
     Command-terminal tabs use the same AppKit tab controls as workspace panes,
     but their visible pane owners live in the command-panel layout tree. Route
     command-tab drag hit testing through command-panel owners so left/right
     edge drops and tab reorders do not accidentally target workspace panes.
     */
    visiblePaneOwnerSessionIds(for: sourceSessionId).compactMap { sessionId in
      if let session = sessions[sessionId],
        !session.containerView.isHidden,
        !session.titleBarView.isHidden,
        session.titleBarView.window != nil
      {
        return (ownerSessionId: sessionId, titleBarView: session.titleBarView)
      }
      if let session = webPaneSessions[sessionId],
        !session.containerView.isHidden,
        !session.titleBarView.isHidden,
        session.titleBarView.window != nil
      {
        return (ownerSessionId: sessionId, titleBarView: session.titleBarView)
      }
      return nil
    }
  }

  private func visiblePaneOwnerSessionIds(for sourceSessionId: String?) -> [String] {
    guard let sourceSessionId,
      commandsPanelActiveSessionIds.contains(sourceSessionId)
    else {
      return orderedVisiblePaneOwnerSessionIds()
    }
    return orderedVisibleCommandPaneOwnerSessionIds()
  }

  private func updatePaneTabReorderTarget(_ target: PaneTabReorderDropTarget?) {
    guard let target else {
      paneTabReorderTargetView?.removeFromSuperview()
      paneTabReorderTargetView = nil
      return
    }
    let targetView = paneTabReorderTargetView ?? TerminalPaneTabReorderTargetView()
    paneTabReorderTargetView = targetView
    if targetView.superview !== self {
      addSubview(targetView)
    }
    targetView.layer?.zPosition = Self.paneHeaderDragFeedbackZPosition
    setPaneDragFeedbackFrame(target.lineFrame, for: targetView)
    targetView.isHidden = false
  }

  private func updatePaneHeaderDropTarget(
    feedbackSessionId: String?,
    placement: PaneDropPlacement?
  ) {
    guard let feedbackSessionId,
      let targetFrame = paneFrame(for: feedbackSessionId),
      let placement
    else {
      paneHeaderDragTargetView?.removeFromSuperview()
      paneHeaderDragTargetView = nil
      return
    }
    let targetView = paneHeaderDragTargetView ?? TerminalPaneHeaderDragTargetView()
    paneHeaderDragTargetView = targetView
    if targetView.superview !== self {
      addSubview(targetView)
    }
    targetView.layer?.zPosition = Self.paneHeaderDragFeedbackZPosition
    setPaneDragFeedbackFrame(
      paneHeaderDropTargetFrame(targetFrame: targetFrame, placement: placement),
      for: targetView)
    targetView.configure(placement: placement)
    targetView.isHidden = false
  }

  private func paneHeaderDropTargetFrame(
    targetFrame: CGRect,
    placement: PaneDropPlacement
  ) -> CGRect {
    /**
     CDXC:PaneTabs 2026-05-10-20:03
     Tab/pane drag feedback must preview the actual drop operation. Center
     drops group as tabs and fill the whole target pane; edge drops split next
     to the target and therefore fill only the half that will become the new
     split region.
     */
    let frame = targetFrame.insetBy(dx: 2, dy: 2)
    switch placement {
    case .center:
      return frame
    case .left:
      return CGRect(x: frame.minX, y: frame.minY, width: frame.width / 2, height: frame.height)
    case .right:
      return CGRect(x: frame.midX, y: frame.minY, width: frame.width / 2, height: frame.height)
    case .bottom:
      return CGRect(x: frame.minX, y: frame.minY, width: frame.width, height: frame.height / 2)
    case .top:
      return CGRect(x: frame.minX, y: frame.midY, width: frame.width, height: frame.height / 2)
    }
  }

  private func setPaneDragFeedbackFrame(_ frame: CGRect, for view: NSView) {
    NSAnimationContext.runAnimationGroup { context in
      context.duration = 0
      context.allowsImplicitAnimation = false
      view.frame = frame
    }
  }

  private func logPaneTabDragMoveIfNeeded(
    drag: inout PaneHeaderDrag,
    eventTimestamp: TimeInterval?,
    ghostOrigin: CGPoint,
    placement: PaneDropPlacement?,
    point: CGPoint,
    targetChanged: Bool,
    targetSessionId: String?
  ) {
    guard drag.startedFromTab else {
      return
    }
    drag.moveEventCount += 1
    let now = ProcessInfo.processInfo.systemUptime
    let shouldLog =
      drag.moveEventCount <= 6
      || now - drag.lastLoggedMoveAt >= 0.08
      || targetChanged
    guard shouldLog else {
      return
    }
    drag.lastLoggedMoveAt = now
    NativePaneTabDragReproLog.append(event: "nativePaneTabDrag.move", details: [
      "dispatchLagMs": eventTimestamp.map { max(0, (now - $0) * 1000) } ?? NSNull(),
      "ghostOrigin": describeFrame(CGRect(x: ghostOrigin.x, y: ghostOrigin.y, width: 0, height: 0)),
      "moveEventCount": drag.moveEventCount,
      "placement": placement?.rawValue ?? NSNull(),
      "point": describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
      "sourceSessionId": drag.sourceSessionId,
      "targetFrame": targetSessionId.flatMap { paneFrame(for: $0).map(describeFrame) } ?? NSNull(),
      "targetSessionId": targetSessionId ?? NSNull(),
    ])
  }

  private func paneHeaderDragGhostOrigin(for size: CGSize, cursorPoint point: CGPoint) -> CGPoint {
    let margin: CGFloat = 8
    let hotSpot = CGPoint(x: 10, y: size.height / 2)
    let maxX = max(margin, bounds.width - size.width - margin)
    let maxY = max(margin, bounds.height - size.height - margin)
    return CGPoint(
      x: min(max(point.x - hotSpot.x, margin), maxX),
      y: min(max(point.y - hotSpot.y, margin), maxY)
    )
  }

  private func paneFrame(for sessionId: String) -> CGRect? {
    if let session = sessions[sessionId] {
      return session.containerView.frame
    }
    if let session = webPaneSessions[sessionId] {
      return session.containerView.frame
    }
    return nil
  }

  func promptEditorSourcePaneFrame(originatingSessionId: String?) -> CGRect? {
    /**
     CDXC:PromptEditor 2026-05-13-09:48
     Ctrl+G prompt editing should open visually near the pane that launched it.
     Resolve the explicit originating native session first, then the currently
     focused pane, so the modal-host editor can appear below that terminal when
     there is enough workspace space.
     */
    let candidates = [
      originatingSessionId?.trimmingCharacters(in: .whitespacesAndNewlines),
      focusedSessionId,
    ]
    for candidate in candidates {
      guard let sessionId = candidate, !sessionId.isEmpty, let frame = paneFrame(for: sessionId)
      else {
        continue
      }
      return frame
    }
    return nil
  }

  private func paneHeaderDisplayTitle(for sessionId: String) -> String {
    if let session = sessions[sessionId] {
      return session.titleBarView.displayTitle
    }
    if let session = webPaneSessions[sessionId] {
      return session.titleBarView.displayTitle
    }
    return normalizedTerminalSessionTitle(sessionTitles[sessionId], sessionId: sessionId)
  }

  private func paneHeaderFavicon(for sessionId: String) -> NSImage? {
    webPaneSessions[sessionId]?.titleBarView.displayFavicon
  }

  private func focusSession(sessionId: String, reason: String) {
    if sessions[sessionId] != nil {
      focusTerminal(sessionId: sessionId, reason: reason)
    } else if webPaneSessions[sessionId] != nil {
      focusWebPane(sessionId: sessionId, reason: reason)
    }
  }

  private func paneSessionId(at point: CGPoint, sourceSessionId: String? = nil) -> String? {
    /**
     CDXC:NativePaneHitTarget 2026-05-13-07:48
     Pane hit ownership follows visible pane owners, not every active session.
     Active tab siblings can remain alive offscreen and may have stale frames;
     drag/drop feedback and content hit routing must ignore those inactive tab
     surfaces so spatial hits resolve to the pane the user actually sees.
     CDXC:CommandsPanel 2026-05-15-08:59
     Command-tab drags must resolve their target inside the command panel, not
     the workspace split behind it. Use command-panel pane owners whenever the
     dragged source session belongs to the command surface.
     */
    for sessionId in visiblePaneOwnerSessionIds(for: sourceSessionId).reversed() {
      if let session = sessions[sessionId],
        !session.containerView.isHidden,
        session.containerView.frame.contains(point)
      {
        return sessionId
      }
      if let session = webPaneSessions[sessionId],
        !session.containerView.isHidden,
        session.containerView.frame.contains(point)
      {
        return sessionId
      }
    }
    return nil
  }

  private func paneBorderFrame(for sessionId: String) -> CGRect? {
    if let session = sessions[sessionId] {
      return session.borderView.convert(session.borderView.bounds, to: self)
    }
    if let session = webPaneSessions[sessionId] {
      return session.borderView.convert(session.borderView.bounds, to: self)
    }
    return nil
  }

  private func paneTitleBarFrame(for sessionId: String) -> CGRect? {
    if let session = sessions[sessionId] {
      return session.titleBarView.convert(session.titleBarView.bounds, to: self)
    }
    if let session = webPaneSessions[sessionId] {
      return session.titleBarView.convert(session.titleBarView.bounds, to: self)
    }
    return nil
  }

  private func isPaneBottomEdgeProbePoint(_ point: CGPoint) -> Bool {
    guard let sessionId = paneSessionId(at: point),
      let borderFrame = paneBorderFrame(for: sessionId)
    else {
      return false
    }
    return point.y - borderFrame.minY <= 16
  }

  /**
   CDXC:NativePaneReorderDiagnostics 2026-05-10-12:32
   Bottom-edge pane drags need exact AppKit routing evidence. Log the pane,
   title-bar, resize, and hit-test state at drag lifecycle points only, so the
   repro can distinguish a false title-bar hit from stale paneHeaderDrag state.
   */
  private func logPaneReorderProbe(
    event: String,
    at point: CGPoint,
    details: [String: Any] = [:]
  ) {
    let paneSessionId = paneSessionId(at: point)
    let titleBarSessionId = paneTitleBarSessionId(at: point)
    let resizeHit = paneResizeHit(at: point)
    var payload = details
    payload["activeSessionIds"] = orderedVisibleSessionIds()
    payload["bottomEdgeDistance"] =
      paneSessionId.flatMap { id in paneBorderFrame(for: id).map { Double(point.y - $0.minY) } }
      ?? NSNull()
    payload["paneFrame"] =
      paneSessionId.flatMap { id in paneBorderFrame(for: id).map(describeFrame) } ?? NSNull()
    payload["paneSessionId"] = paneSessionId ?? NSNull()
    payload["point"] = describeFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0))
    payload["resizeHitDirection"] = resizeHit.map { String(describing: $0.direction) } ?? NSNull()
    payload["resizeHitRect"] = resizeHit.map { describeFrame($0.rect) } ?? NSNull()
    payload["titleBarFrame"] =
      titleBarSessionId.flatMap { id in paneTitleBarFrame(for: id).map(describeFrame) } ?? NSNull()
    payload["titleBarSessionId"] = titleBarSessionId ?? NSNull()
    payload["workspaceBounds"] = describeFrame(bounds)
    NativePaneReorderReproLog.append(event: event, details: payload)
  }

  private func paneTitleBarSessionId(at point: CGPoint) -> String? {
    for (sessionId, session) in sessions where activeSessionIds.contains(sessionId) {
      if paneTitleBarView(session.titleBarView, containsDraggablePoint: point) {
        return sessionId
      }
    }
    for (sessionId, session) in webPaneSessions where activeSessionIds.contains(sessionId) {
      if paneTitleBarView(session.titleBarView, containsDraggablePoint: point) {
        return sessionId
      }
    }
    return nil
  }

  private func paneTitleBarView(
    _ titleBarView: TerminalSessionTitleBarView,
    containsDraggablePoint point: CGPoint
  ) -> Bool {
    let titleBarPoint = convert(point, to: titleBarView)
    guard titleBarView.bounds.contains(titleBarPoint) else {
      return false
    }
    return titleBarView.isDraggableHeaderPoint(titleBarPoint)
  }

  private func syncPoppedOutPaneWindows(reason: String) {
    /**
     CDXC:PanePopOut 2026-05-11-09:35
     Pop-out must move the existing native surface instead of recreating a
     terminal/browser fallback. The sidebar sends presentation state; AppKit
     reparents the live surface into one ghostex-owned NSWindow and keeps the
     original split/tab slot available for a reattach placeholder.
     */
    for sessionId in Array(poppedOutPaneControllers.keys) where !poppedOutSessionIds.contains(sessionId) {
      reattachPoppedOutPane(sessionId: sessionId, reason: reason)
    }

    for sessionId in poppedOutSessionIds {
      ensurePoppedOutPaneWindow(sessionId: sessionId, reason: reason)
    }
  }

  private func ensurePoppedOutPaneWindow(sessionId: String, reason: String) {
    guard poppedOutPaneControllers[sessionId] == nil else {
      updatePoppedOutWindowTitle(sessionId: sessionId)
      showPoppedOutPlaceholderImmediately(sessionId: sessionId, reason: reason)
      return
    }
    let title = normalizedTerminalSessionTitle(sessionTitles[sessionId], sessionId: sessionId)
    let contentView: NSView
    let firstResponder: NSView
    let popOutTitleBarView = TerminalSessionTitleBarView(
      title: title,
      actions: poppedOutPaneTitleBarActions(sessionId: sessionId))
    let popOutPaneKind = sessions[sessionId] != nil ? "poppedOutTerminal" : "poppedOutWeb"
    popOutTitleBarView.setDebugContext(ownerSessionId: sessionId, paneKind: popOutPaneKind)
    /**
     CDXC:PanePopOut 2026-05-11-18:54
     Popped-out panes are their own focused pane surface and do not receive the
     main workspace hover updates that keep tab-bar actions armed. Keep the
     title-bar action strip active so Pop In and the full pane action set stay
     visible and clickable in the separate NSWindow.
     */
    popOutTitleBarView.setPaneHovered(true)
    popOutTitleBarView.setAgentIconDataUrl(
      sessionAgentIconDataUrls[sessionId],
      colorHex: sessionAgentIconColors[sessionId])
    popOutTitleBarView.onAction = { [weak self] action in
      guard let self else { return }
      self.applyOptimisticPanePopOutAction(
        sessionId: sessionId,
        action: action,
        reason: "poppedOutWindowTitleBarAction")
      self.sendEvent(.terminalTitleBarAction(sessionId: sessionId, action: action))
    }
    if let session = sessions[sessionId] {
      session.scrollView.removeFromSuperview()
      session.searchBarView.removeFromSuperview()
      contentView = PoppedOutTerminalPaneContentView(
        scrollView: session.scrollView,
        searchBarView: session.searchBarView,
        persistenceLabelView: session.persistenceLabelView,
        titleBarView: popOutTitleBarView,
        titleBarHeight: Self.terminalTitleBarHeight)
      firstResponder = session.view
    } else if let session = webPaneSessions[sessionId] {
      session.hostView.removeFromSuperview()
      popOutTitleBarView.setFavicon(nativePaneImage(fromDataUrl: sessionFaviconDataUrls[sessionId]))
      contentView = PoppedOutWebPaneContentView(
        hostView: session.hostView,
        titleBarView: popOutTitleBarView,
        titleBarHeight: Self.terminalTitleBarHeight)
      firstResponder = session.browserContentView
    } else {
      return
    }

    let frame = defaultPoppedOutWindowFrame()
    let popOutWindow = NSWindow(
      contentRect: NSWindow.contentRect(
        forFrameRect: frame,
        styleMask: [.closable, .miniaturizable, .resizable, .titled]),
      styleMask: [.closable, .miniaturizable, .resizable, .titled],
      backing: .buffered,
      defer: false)
    popOutWindow.title = title
    popOutWindow.contentView = contentView
    popOutWindow.isReleasedWhenClosed = false
    let controller = PoppedOutPaneWindowController(
      sessionId: sessionId,
      titleBarView: popOutTitleBarView,
      window: popOutWindow,
      onReattachRequested: { [weak self] sessionId in
        guard let self else { return }
        self.applyOptimisticPanePopOutAction(
          sessionId: sessionId,
          action: .restorePopOut,
          reason: "poppedOutWindowClose")
        self.sendEvent(.terminalTitleBarAction(sessionId: sessionId, action: .restorePopOut))
      },
      onResize: { [weak self] sessionId in
        guard let self else { return }
        /*
         CDXC:ZmxPersistenceRefresh 2026-05-18-15:10:
         Popped-out terminal panes resize in their own NSWindow and bypass the workspace split, command-panel, and companion-pane resize handlers.
         Route those resize events into the same trailing zmx viewport refresh so persisted terminal text repairs after the user stops resizing the pop-out window.
         */
        self.scheduleZmxPersistenceTerminalRefreshAfterResize(
          sessionIds: self.zmxPersistenceTerminalSessionIds(
            from: [sessionId],
            includePoppedOut: true),
          reason: "poppedOutWindowResize")
      })
    poppedOutPaneControllers[sessionId] = controller
    showPoppedOutPlaceholderImmediately(sessionId: sessionId, reason: reason)
    popOutWindow.makeKeyAndOrderFront(nil)
    _ = popOutWindow.makeFirstResponder(firstResponder)
    NativeT3CodePaneReproLog.append("nativeWorkspace.panePopOut.window.opened", [
      "reason": reason,
      "sessionId": sessionId,
      "title": title,
      "windowFrame": describeFrame(popOutWindow.frame),
    ])
  }

  private func reattachPoppedOutPane(sessionId: String, reason: String) {
    guard let controller = poppedOutPaneControllers.removeValue(forKey: sessionId) else {
      removePoppedOutPlaceholder(sessionId: sessionId)
      return
    }
    controller.closeProgrammatically()
    if let session = sessions[sessionId] {
      mountTerminalPaneContainer(for: session)
    } else if let session = webPaneSessions[sessionId] {
      mountWebPaneContainer(for: session)
    }
    removePoppedOutPlaceholder(sessionId: sessionId)
    needsLayout = true
    /*
     CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
     Sidebar sync can also pop a pane back into the workspace without using the local title-bar action path.
     Schedule a surfaced-only refresh so restored zmx terminals repair after their new workspace frame is applied.
     */
    scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "poppedOutWindowReattached")
    NativeT3CodePaneReproLog.append("nativeWorkspace.panePopOut.window.reattached", [
      "reason": reason,
      "sessionId": sessionId,
    ])
  }

  private func closePoppedOutPaneWindow(sessionId: String, reason: String) {
    poppedOutSessionIds.remove(sessionId)
    if let controller = poppedOutPaneControllers.removeValue(forKey: sessionId) {
      controller.closeProgrammatically()
    }
    removePoppedOutPlaceholder(sessionId: sessionId)
    NativeT3CodePaneReproLog.append("nativeWorkspace.panePopOut.window.closed", [
      "reason": reason,
      "sessionId": sessionId,
    ])
  }

  private func updatePoppedOutWindowTitle(sessionId: String) {
    guard let controller = poppedOutPaneControllers[sessionId] else {
      return
    }
    let title = normalizedTerminalSessionTitle(sessionTitles[sessionId], sessionId: sessionId)
    controller.window?.title = title
    controller.titleBarView.setTitle(title)
    controller.titleBarView.setActions(poppedOutPaneTitleBarActions(sessionId: sessionId))
    controller.titleBarView.setPaneHovered(true)
    showPoppedOutPlaceholderImmediately(sessionId: sessionId, reason: "updatePoppedOutWindowTitle")
  }

  private func poppedOutPaneTitleBarActions(sessionId: String) -> [TerminalTitleBarAction] {
    /**
     CDXC:PanePopOut 2026-05-11-18:54
     Pop-out windows must use the same right-side title-bar action model as the
     in-workspace tab bar. Reuse the synced action order when available and
     substitute Restore for Pop Out so the separate window exposes Pop In plus
     the rest of the pane actions without a parallel implementation.
     */
    let baseActions: [TerminalTitleBarAction]
    if let actions = sessionTitleBarActions[sessionId], !actions.isEmpty {
      baseActions = actions
    } else if webPaneSessions[sessionId] != nil {
      baseActions = TerminalSessionTitleBarView.webPaneCreationActions
    } else {
      baseActions = TerminalSessionTitleBarView.defaultActions
    }

    var actions = baseActions.map { action in
      action == .popOut ? .restorePopOut : action
    }
    if !actions.contains(.restorePopOut) {
      actions.append(.restorePopOut)
    }
    return actions
  }

  private func handlePaneTabActionRequested(sessionId: String, action: TerminalTitleBarAction) {
    if sessions[sessionId] != nil {
      focusTerminal(sessionId: sessionId, reason: "nativeTabContextMenuAction")
    } else if webPaneSessions[sessionId] != nil {
      focusWebPane(sessionId: sessionId, reason: "nativeTabContextMenuAction")
    }
    applyOptimisticPanePopOutAction(
      sessionId: sessionId,
      action: action,
      reason: "nativeTabContextMenuAction")
    sendEvent(.terminalTitleBarAction(sessionId: sessionId, action: action))
  }

  private func applyOptimisticPanePopOutAction(
    sessionId: String,
    action: TerminalTitleBarAction,
    reason: String
  ) {
    /**
     CDXC:PanePopOut 2026-05-11-10:24
     Pop-out/restore chrome must react immediately in AppKit. Do the native
     reparenting and placeholder update optimistically before the sidebar's
     persisted workspace sync returns, then let the next sync confirm the same
     presentation state.
     */
    switch action {
    case .popOut:
      guard activeSessionIds.contains(sessionId), !poppedOutSessionIds.contains(sessionId) else {
        return
      }
      poppedOutSessionIds.insert(sessionId)
      showPoppedOutPlaceholderImmediately(sessionId: sessionId, reason: "\(reason).popOut")
      ensurePoppedOutPaneWindow(sessionId: sessionId, reason: "\(reason).popOut")
    case .restorePopOut:
      guard poppedOutSessionIds.contains(sessionId) || poppedOutPaneControllers[sessionId] != nil else {
        return
      }
      poppedOutSessionIds.remove(sessionId)
      reattachPoppedOutPane(sessionId: sessionId, reason: "\(reason).restorePopOut")
      needsLayout = true
      layoutSubtreeIfNeeded()
      /*
       CDXC:ZmxPersistenceRefresh 2026-05-18-15:44:
       Restoring a popped-out zmx terminal reparents the live Ghostty surface into the workspace and gives it a new visible frame.
       Run the trailing surfaced-pane refresh after the pop-in layout settles.
       */
      scheduleZmxPersistenceTerminalRefreshAfterResize(reason: "\(reason).restorePopOut")
    default:
      return
    }
  }

  private func showPoppedOutPlaceholderImmediately(sessionId: String, reason: String) {
    guard poppedOutSessionIds.contains(sessionId) else {
      return
    }
    if let rect = currentPaneRectForPlaceholder(sessionId: sessionId) {
      setPoppedOutPlaceholderFrame(rect, for: sessionId)
      poppedOutPlaceholderViews[sessionId]?.needsLayout = true
      poppedOutPlaceholderViews[sessionId]?.layoutSubtreeIfNeeded()
      NativeT3CodePaneReproLog.append("nativeWorkspace.panePopOut.placeholder.immediate", [
        "reason": reason,
        "sessionId": sessionId,
        "rect": describeFrame(rect),
      ])
      return
    }
    needsLayout = true
    layoutSubtreeIfNeeded()
  }

  private func currentPaneRectForPlaceholder(sessionId: String) -> CGRect? {
    if let session = sessions[sessionId] {
      if session.containerView.frame.width > 1, session.containerView.frame.height > 1 {
        return session.containerView.frame
      }
      if session.borderView.frame.width > 1, session.borderView.frame.height > 1 {
        return session.containerView.convert(session.borderView.frame, to: self)
      }
      if session.titleBarView.frame.width > 1, session.scrollView.frame.width > 1 {
        let localRect = session.titleBarView.frame.union(session.scrollView.frame)
        return session.containerView.convert(localRect, to: self)
      }
    }
    if let session = webPaneSessions[sessionId] {
      if session.containerView.frame.width > 1, session.containerView.frame.height > 1 {
        return session.containerView.frame
      }
      if session.borderView.frame.width > 1, session.borderView.frame.height > 1 {
        return session.containerView.convert(session.borderView.frame, to: self)
      }
      if session.titleBarView.frame.width > 1, session.hostView.frame.width > 1 {
        let localRect = session.titleBarView.frame.union(session.hostView.frame)
        return session.containerView.convert(localRect, to: self)
      }
    }
    return nil
  }

  private func defaultPoppedOutWindowFrame() -> CGRect {
    let size = CGSize(width: 980, height: 680)
    guard let sourceFrame = window?.frame else {
      return CGRect(origin: CGPoint(x: 180, y: 180), size: size)
    }
    return CGRect(
      x: sourceFrame.midX - size.width / 2 + 40,
      y: sourceFrame.midY - size.height / 2 - 40,
      width: size.width,
      height: size.height)
  }

  private func setPoppedOutPlaceholderFrame(_ rect: CGRect, for sessionId: String) {
    let title = normalizedTerminalSessionTitle(sessionTitles[sessionId], sessionId: sessionId)
    let titleBarHeight = min(titleBarHeight(for: sessionId), max(rect.height, 0))
    let titleBarRect = CGRect(
      x: rect.minX,
      y: rect.maxY - titleBarHeight,
      width: rect.width,
      height: titleBarHeight
    )
    let placeholderRect = CGRect(
      x: rect.minX,
      y: rect.minY,
      width: rect.width,
      height: max(rect.height - titleBarHeight, 1)
    )
    let placeholderView =
      poppedOutPlaceholderViews[sessionId]
      ?? PoppedOutPanePlaceholderView(title: title) { [weak self] in
        guard let self else { return }
        self.applyOptimisticPanePopOutAction(
          sessionId: sessionId,
          action: .restorePopOut,
          reason: "poppedOutPlaceholderReattach")
        self.sendEvent(.terminalTitleBarAction(sessionId: sessionId, action: .restorePopOut))
      }
    poppedOutPlaceholderViews[sessionId] = placeholderView
    placeholderView.setTitle(title)
    if placeholderView.superview !== self {
      addSubview(placeholderView)
    }
    placeholderView.frame = placeholderRect
    placeholderView.isHidden = false
    if let session = sessions[sessionId] {
      session.containerView.isHidden = true
      session.titleBarView.frame = titleBarRect
      session.titleBarView.isHidden = false
      session.titleBarView.removeFromSuperview()
      addSubview(session.titleBarView)
      session.borderView.frame = rect
      session.borderView.removeFromSuperview()
      addSubview(session.borderView)
      session.borderView.isHidden = false
    } else if let session = webPaneSessions[sessionId] {
      session.containerView.isHidden = true
      session.titleBarView.frame = titleBarRect
      session.titleBarView.isHidden = false
      session.titleBarView.removeFromSuperview()
      addSubview(session.titleBarView)
      session.borderView.frame = rect
      session.borderView.removeFromSuperview()
      addSubview(session.borderView)
      session.borderView.isHidden = false
    }
    updateTerminalBorder(for: sessionId)
  }

  private func removePoppedOutPlaceholder(sessionId: String) {
    guard let placeholderView = poppedOutPlaceholderViews.removeValue(forKey: sessionId) else {
      return
    }
    placeholderView.removeFromSuperview()
  }

  private func setFrame(_ rect: CGRect, for sessionId: String) {
    if poppedOutSessionIds.contains(sessionId) {
      setPoppedOutPlaceholderFrame(rect, for: sessionId)
      return
    }
    removePoppedOutPlaceholder(sessionId: sessionId)
    if let webPane = webPaneSessions[sessionId] {
      setWebPaneFrame(rect, for: webPane)
      return
    }

    guard let session = sessions[sessionId] else {
      return
    }
    /**
     CDXC:NativeTerminals 2026-04-28-12:49
     Non-persistent native Ghostty panes must show the same per-session title
     bar that the reference workspace renders in React. The AppKit surface is
     therefore laid out below native chrome instead of covering the full pane.
     */
    let titleBarHeight = min(titleBarHeight(for: sessionId), max(rect.height, 0))
    mountTerminalPaneContainer(for: session)
    session.containerView.frame = rect
    session.containerView.isHidden = false
    let titleBarRect = CGRect(
      x: 0,
      y: rect.height - titleBarHeight,
      width: rect.width,
      height: titleBarHeight
    )
    let availableTerminalRect = CGRect(
      x: 0,
      y: 0,
      width: rect.width,
      height: max(rect.height - titleBarHeight, 1)
    )
    /**
     CDXC:NativeTerminalResize 2026-05-02-17:19
     Pane chrome and the terminal renderer must share the same body width.
     Remove the previous whole-cell stepping here because it created visible
     chrome/body width drift and did not resolve the prior terminal resize bug.
     */
    let terminalRect = availableTerminalRect
    session.titleBarView.frame = titleBarRect
    session.titleBarView.needsLayout = true
    session.titleBarView.layoutSubtreeIfNeeded()
    if commandsPanelActiveSessionIds.contains(sessionId) && !commandsPanelIsVisible {
      session.scrollView.frame = .zero
      session.scrollView.isHidden = true
      session.searchBarView.frame = .zero
      session.searchBarView.isHidden = true
      session.persistenceLabelView.setSuppressed(true)
      session.persistenceLabelView.frame = .zero
      session.delayedSendLabelView.frame = .zero
      session.borderView.frame = session.containerView.bounds
      updateTerminalBorder(for: sessionId)
      return
    }
    session.scrollView.frame = availableTerminalRect
    session.scrollView.needsLayout = true
    session.scrollView.layoutSubtreeIfNeeded()
    session.searchBarView.frame = searchBarFrame(in: terminalRect)
    if session.view.searchState != nil {
      logTerminalSearchInteraction(
        "nativeWorkspace.terminalSearch.layout",
        session: session,
        details: [
          "containerFrame": describeFrame(session.containerView.frame),
          "scrollViewFrame": describeFrame(session.scrollView.frame),
          "terminalRect": describeFrame(terminalRect),
          "titleBarRect": describeFrame(titleBarRect),
        ])
    }
    session.persistenceLabelView.setSuppressed(false)
    session.persistenceLabelView.frame = persistenceLabelFrame(
      in: terminalRect,
      labelView: session.persistenceLabelView)
    session.delayedSendLabelView.frame = delayedSendLabelFrame(
      in: terminalRect,
      labelView: session.delayedSendLabelView)
    session.borderView.frame = session.containerView.bounds
    logTerminalResizeIfNeeded(
      session: session,
      paneRect: rect,
      titleBarRect: titleBarRect,
      availableTerminalRect: availableTerminalRect,
      terminalRect: terminalRect)
    updateTerminalBorder(for: sessionId)
  }

  private func persistenceLabelFrame(
    in terminalRect: CGRect,
    labelView: TerminalPanePersistenceLabelView
  ) -> CGRect {
    guard !labelView.isHidden, terminalRect.width > 24, terminalRect.height > 18 else {
      return .zero
    }
    let labelSize = labelView.fittingSize
    let labelWidth = min(ceil(labelSize.width), max(terminalRect.width - 20, 0))
    let labelHeight = min(max(ceil(labelSize.height), 14), max(terminalRect.height - 12, 0))
    return CGRect(
      x: max(10, terminalRect.maxX - labelWidth - 10),
      y: max(6, terminalRect.maxY - labelHeight - 6),
      width: labelWidth,
      height: labelHeight
    )
  }

  private func delayedSendLabelFrame(
    in terminalRect: CGRect,
    labelView: TerminalPaneDelayedSendLabelView
  ) -> CGRect {
    guard !labelView.isHidden, terminalRect.width > 48, terminalRect.height > 32 else {
      return .zero
    }
    let labelSize = labelView.fittingSize
    let labelWidth = min(ceil(labelSize.width), max(terminalRect.width - 32, 0))
    let labelHeight = min(max(ceil(labelSize.height), 52), max(terminalRect.height - 24, 0))
    return CGRect(
      x: max(12, terminalRect.maxX - labelWidth - 12),
      y: max(8, terminalRect.maxY - labelHeight - 8),
      width: labelWidth,
      height: labelHeight
    )
  }

  private func titleBarHeight(for sessionId: String) -> CGFloat {
    commandsPanelActiveSessionIds.contains(sessionId)
      ? Self.commandPanelTitleBarHeight
      : Self.terminalTitleBarHeight
  }

  private func movePaneSessionOffscreen(_ sessionId: String) {
    if let placeholderView = poppedOutPlaceholderViews[sessionId] {
      moveOffscreen(placeholderView)
    }
    if let session = sessions[sessionId] {
      guard !poppedOutSessionIds.contains(sessionId) else {
        moveOffscreen(session.titleBarView)
        moveOffscreen(session.borderView)
        moveOffscreen(session.containerView)
        return
      }
      moveOffscreen(session.containerView)
      return
    }
    if let session = webPaneSessions[sessionId] {
      guard !poppedOutSessionIds.contains(sessionId) else {
        moveOffscreen(session.titleBarView)
        moveOffscreen(session.borderView)
        moveOffscreen(session.containerView)
        return
      }
      moveOffscreen(session.containerView)
    }
  }

  private func setPaneTabs(
    _ sessionIds: [String],
    activeSessionId: String,
    on ownerSessionId: String
  ) {
    let items = sessionIds.map { sessionId in
      TerminalSessionTitleBarView.TabItem(
        actions: sessionTitleBarActions[sessionId] ?? [],
        isSleeping: sleepingSessionIds.contains(sessionId),
        sessionId: sessionId,
        title: normalizedTerminalSessionTitle(sessionTitles[sessionId], sessionId: sessionId))
    }
    if let session = sessions[ownerSessionId] {
      session.titleBarView.setDebugContext(ownerSessionId: ownerSessionId, paneKind: "terminal")
      session.titleBarView.setTabs(items, activeSessionId: activeSessionId)
      session.titleBarView.setTabActivities(sessionActivities)
      session.titleBarView.setTabDelayedSendRemainingLabels(sessionDelayedSendRemainingLabels)
      session.titleBarView.setTabIdentityIcons(
        faviconDataUrls: sessionFaviconDataUrls,
        agentIconDataUrls: sessionAgentIconDataUrls,
        agentIconColors: sessionAgentIconColors)
    }
    if let session = webPaneSessions[ownerSessionId] {
      session.titleBarView.setDebugContext(ownerSessionId: ownerSessionId, paneKind: "web")
      session.titleBarView.setTabs(items, activeSessionId: activeSessionId)
      session.titleBarView.setTabActivities(sessionActivities)
      session.titleBarView.setTabDelayedSendRemainingLabels(sessionDelayedSendRemainingLabels)
      session.titleBarView.setTabIdentityIcons(
        faviconDataUrls: sessionFaviconDataUrls,
        agentIconDataUrls: sessionAgentIconDataUrls,
        agentIconColors: sessionAgentIconColors)
    }
  }

  /**
   CDXC:PaneTabs 2026-05-21-02:27:
   Session rename sync can change only `sessionTitles` while the split/tab tree
   is otherwise unchanged. Refresh the currently mounted native tab buttons from
   the authoritative layout immediately so the selected tab does not keep the
   old title until a later focus or layout change rebuilds the tab strip.
   */
  private func syncPaneTabChromeFromCurrentLayout() {
    if let activeProjectEditorId,
      projectEditorPaneSessions[activeProjectEditorId] != nil
    {
      if let companionLayout = projectEditorCompanionLayout(in: bounds) {
        setPaneTabs([], activeSessionId: companionLayout.sessionId, on: companionLayout.sessionId)
      }
    } else if let terminalLayout {
      syncPaneTabChrome(in: terminalLayout)
    } else {
      for sessionId in orderedVisibleSessionIds() {
        setPaneTabs([sessionId], activeSessionId: sessionId, on: sessionId)
      }
    }

    if let commandsPanelLayout {
      syncPaneTabChrome(in: commandsPanelLayout)
    } else {
      for sessionId in orderedVisibleCommandSessionIds() {
        setPaneTabs([sessionId], activeSessionId: sessionId, on: sessionId)
      }
    }
  }

  private func syncPaneTabChrome(in node: NativeTerminalLayout) {
    switch node {
    case .leaf(let sessionId):
      guard isPaneSessionVisible(sessionId) else {
        return
      }
      setPaneTabs([sessionId], activeSessionId: sessionId, on: sessionId)
    case .tabs(let activeSessionId, let sessionIds):
      let tabSessionIds = sessionIds.filter { isPaneSessionVisible($0) || sleepingSessionIds.contains($0) }
      let activeTabSessionIds = tabSessionIds.filter { isPaneSessionVisible($0) }
      guard !activeTabSessionIds.isEmpty else {
        return
      }
      let selectedSessionId =
        activeSessionId.flatMap { activeTabSessionIds.contains($0) ? $0 : nil } ?? activeTabSessionIds[0]
      setPaneTabs(tabSessionIds, activeSessionId: selectedSessionId, on: selectedSessionId)
    case .split(_, _, let children):
      for child in children {
        syncPaneTabChrome(in: child)
      }
    }
  }

  private func setWebPaneFrame(_ rect: CGRect, for session: WebPaneSession) {
    let resolvedRect: CGRect
    if rect.width <= 1 || rect.height <= Self.terminalTitleBarHeight + 1 {
      resolvedRect = layoutBounds(forVisibleCount: max(orderedVisibleSessionIds().count, 1))
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.layout.fallbackRect", [
        "inputRect": describeFrame(rect),
        "resolvedRect": describeFrame(resolvedRect),
        "sessionId": session.sessionId,
        "workspaceBounds": describeFrame(bounds),
      ])
    } else {
      resolvedRect = rect
    }
    let paneRect = session.chromiumView == nil ? resolvedRect : chromiumBackingPixelAlignedFrame(resolvedRect)
    let titleBarHeight = min(Self.terminalTitleBarHeight, max(paneRect.height, 0))
    mountWebPaneContainer(for: session)
    session.containerView.frame = paneRect
    session.containerView.isHidden = false
    let titleBarRect = CGRect(
      x: 0,
      y: paneRect.height - titleBarHeight,
      width: paneRect.width,
      height: titleBarHeight
    )
    let contentRect = CGRect(
      x: 0,
      y: 0,
      width: paneRect.width,
      height: max(paneRect.height - titleBarHeight, 1)
    )
    session.titleBarView.frame = titleBarRect
    session.titleBarView.needsLayout = true
    session.titleBarView.layoutSubtreeIfNeeded()
    session.hostView.translatesAutoresizingMaskIntoConstraints = true
    session.hostView.frame = contentRect
    session.hostView.refreshHostedWebView(reason: "setWebPaneFrame")
    session.borderView.frame = session.containerView.bounds
    if focusedSessionId == session.sessionId {
      orderWebPaneViewsToFront(session)
    }
    updateTerminalBorder(for: session.sessionId)
  }

  private func scheduleDeferredWebPaneLayout(sessionId: String?, reason: String) {
    guard let sessionId, webPaneSessions[sessionId] != nil else {
      return
    }
    DispatchQueue.main.async { [weak self] in
      guard let self, self.webPaneSessions[sessionId] != nil else {
        return
      }
      self.superview?.needsLayout = true
      self.superview?.layoutSubtreeIfNeeded()
      self.needsLayout = true
      self.layoutSubtreeIfNeeded()
      if let session = self.webPaneSessions[sessionId] {
        /**
         CDXC:T3Code 2026-05-01-14:10
         WKWebView panes can be created before the native workspace receives
         its final AppKit bounds. If split layout initially gives the web pane a
         zero rect, pin the pane host to the resolved workspace pane during the
         deferred layout pass so the web content is rendered as an inline pane,
         not an accessibility-only webview hidden behind the workspace layer.
         */
        if session.hostView.frame.width <= 1 || session.hostView.frame.height <= 1,
          self.bounds.width > 1, self.bounds.height > Self.terminalTitleBarHeight + 1
        {
          let rect = self.layoutBounds(
            forVisibleCount: max(self.orderedVisibleSessionIds().count, 1))
          NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.layout.deferredPin", [
            "hostFrame": self.describeFrame(session.hostView.frame),
            "reason": reason,
            "resolvedRect": self.describeFrame(rect),
            "sessionId": sessionId,
            "workspaceBounds": self.describeFrame(self.bounds),
          ])
          self.setWebPaneFrame(rect, for: session)
        }
        NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.layout.deferred", [
          "hostFrame": self.describeFrame(session.hostView.frame),
          "reason": reason,
          "sessionId": sessionId,
          "workspaceBounds": self.describeFrame(self.bounds),
        ])
        session.hostView.refreshHostedWebView(reason: "\(reason).deferred")
      }
    }
  }

  private func orderTerminalPaneViewsToFront(_ optionalSession: TerminalSession?) {
    guard let session = optionalSession else {
      return
    }
    guard !poppedOutSessionIds.contains(session.sessionId) else {
      return
    }
    mountTerminalPaneContainer(for: session)
    orderPaneContainerToFront(session.containerView)
  }

  /**
   CDXC:T3Code 2026-04-30-19:17
   The T3 Code pane is a native WKWebView, not React DOM inside the sidebar.
   Keep the WebKit surface inside the pane host so it participates in split
   layout and z-order like a browser pane instead of floating over the app.
   */
  private func orderWebPaneViewsToFront(_ optionalSession: WebPaneSession?) {
    guard let session = optionalSession else {
      return
    }
    guard !poppedOutSessionIds.contains(session.sessionId) else {
      return
    }
    mountWebPaneContainer(for: session)
    orderPaneContainerToFront(session.containerView)
  }

  /**
   CDXC:NativePaneResize 2026-05-11-14:44
   Native-style pane ownership raises the focused pane leaf, not terminal/web
   children separately.
   CDXC:NativePaneResize 2026-05-11-17:53
   Split dividers now occupy real layout gaps, so focused panes do not need to
   re-raise divider views above content. The pane leaf and divider are
   non-overlapping siblings.
   */
  private func orderPaneContainerToFront(_ containerView: TerminalPaneLeafContainerView) {
    guard containerView.superview === self else {
      return
    }
    if subviews.last !== containerView {
      containerView.removeFromSuperview()
      addSubview(containerView, positioned: .above, relativeTo: nil)
    }
    containerView.alphaValue = 1
    containerView.layer?.zPosition = 100
    keepCommandsPanelAboveWorkspacePanes()
  }

  private func scheduleWebPaneReload(sessionId: String, url: URL, remainingAttempts: Int) {
    guard remainingAttempts > 0 else {
      return
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.75) { [weak self] in
      guard let self, let session = self.webPaneSessions[sessionId] else {
        return
      }
      guard !self.completedWebPaneLoadSessionIds.contains(sessionId) else {
        return
      }
      guard let webView = session.webView else {
        return
      }
      if webView.isLoading {
        NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.retry.waiting", [
          "remainingAttempts": remainingAttempts,
          "sessionId": sessionId,
          "url": url.absoluteString,
        ])
        self.scheduleWebPaneReload(
          sessionId: sessionId,
          url: url,
          remainingAttempts: remainingAttempts - 1
        )
        return
      }

      /**
       CDXC:T3Code 2026-04-30-03:47
       WKWebView keeps `url` populated after a provisional localhost failure,
       so retrying only when `webView.url == nil` strands T3 Code on a gray
       pane if the first load races provider startup. Retry until navigation
       actually finishes, then stop.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.retry", [
        "currentUrl": webView.url?.absoluteString ?? NSNull(),
        "remainingAttempts": remainingAttempts,
        "sessionId": sessionId,
        "url": url.absoluteString,
      ])
      self.loadWebPane(sessionId: sessionId, url: url, reason: "retry")
    }
  }

  private func loadWebPane(sessionId: String, url: URL, reason: String) {
    guard webPaneSessions[sessionId] != nil else {
      return
    }
    guard NativeT3RuntimeLauncher.isManagedRuntimeURL(url) else {
      guard let session = webPaneSessions[sessionId] else {
        return
      }
      /**
       CDXC:ChromiumBrowserPanes 2026-05-04-16:38
       Non-T3 browser panes load through embedded Chromium/CEF, not WebKit.
       The T3 authentication/thread-route bootstrap remains gated to managed
       localhost runtime URLs so public web navigation never calls T3-only APIs.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.browserWebPane.load.start", [
        "engine": session.chromiumView == nil ? "missing-chromium" : "chromium",
        "reason": reason,
        "sessionId": sessionId,
        "url": url.absoluteString,
      ])
      session.chromiumView?.loadURLString(url.absoluteString)
      return
    }
    guard !pendingAuthenticatedWebPaneLoadSessionIds.contains(sessionId) else {
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.authPending", [
        "reason": reason,
        "sessionId": sessionId,
        "url": url.absoluteString,
      ])
      return
    }
    pendingAuthenticatedWebPaneLoadSessionIds.insert(sessionId)
    NativeT3RuntimeBrowserAuth.prepareManagedWebSession(for: url, sessionId: sessionId) {
      [weak self] in
      guard let self, let session = self.webPaneSessions[sessionId] else {
        return
      }
      self.pendingAuthenticatedWebPaneLoadSessionIds.remove(sessionId)
      self.completedWebPaneLoadSessionIds.remove(sessionId)
      NativeT3RuntimeSessionBootstrap.prepareThreadRoute(
        origin: url,
        projectId: session.projectId,
        sessionId: sessionId,
        threadId: session.threadId,
        title: session.title,
        workspaceRoot: session.workspaceRoot
      ) { [weak self] result in
        guard let self, let session = self.webPaneSessions[sessionId] else {
          return
        }
        switch result {
        case .success(let route):
          self.t3ThreadRouteRetryAttemptsBySessionId.removeValue(forKey: sessionId)
          self.sendEvent(
            .t3ThreadReady(
              sessionId: sessionId,
              projectId: route.projectId,
              threadId: route.threadId,
              serverOrigin: "\(url.scheme ?? "http")://\(url.host ?? "127.0.0.1")\(url.port.map { ":\($0)" } ?? "")",
              workspaceRoot: session.workspaceRoot ?? ""
            )
          )
          NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.start", [
            "reason": reason,
            "routeUrl": route.url.absoluteString,
            "sessionId": sessionId,
            "url": url.absoluteString,
            "workspaceRoot": session.workspaceRoot ?? NSNull(),
          ])
          session.webView?.load(URLRequest(url: route.url))
          self.scheduleWebPaneReload(sessionId: sessionId, url: route.url, remainingAttempts: 16)
        case .failure(let error):
          NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.threadRouteFailed", [
            "error": error.localizedDescription,
            "reason": reason,
            "sessionId": sessionId,
            "url": url.absoluteString,
            "workspaceRoot": session.workspaceRoot ?? NSNull(),
          ])
          if self.retryT3ThreadRouteIfStartupIsStillSettling(
            sessionId: sessionId,
            url: url,
            error: error
          ) {
            return
          }
          self.loadWebPaneError(session: session, message: error.localizedDescription)
        }
      }
    }
  }

  func reloadManagedT3WebPanes(reason: String) {
    guard let runtimeUrl = URL(string: "http://\(NativeT3RuntimeLauncher.host):\(NativeT3RuntimeLauncher.port)") else {
      return
    }
    let managedSessions = webPaneSessions.values.filter { $0.isManagedT3Pane }
    guard !managedSessions.isEmpty else {
      return
    }
    /**
     CDXC:T3Code 2026-05-24-17:38:
     Runtime liveness repair can replace the localhost server while an existing T3 WKWebView still points at the old process.
     Reload active managed panes through the native auth/thread-route bootstrap so the replacement runtime mints owner auth and reconnects without requiring the user to close and recreate the T3 agent.
     */
    for session in managedSessions {
      guard !pendingAuthenticatedWebPaneLoadSessionIds.contains(session.sessionId) else {
        NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.reloadManaged.pending", [
          "reason": reason,
          "sessionId": session.sessionId,
        ])
        continue
      }
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.reloadManaged.start", [
        "currentUrl": session.currentURLString ?? NSNull(),
        "reason": reason,
        "sessionId": session.sessionId,
      ])
      loadWebPaneStatus(
        sessionId: session.sessionId,
        title: session.title,
        message: "Loading T3 Code…",
        caption: "Preparing the embedded workspace",
        loading: true,
        reason: reason
      )
      loadWebPane(sessionId: session.sessionId, url: runtimeUrl, reason: reason)
    }
  }

  private static func browserPaneURLRequest(url: URL, reason: String) -> URLRequest {
    /**
     CDXC:BrowserPanes 2026-05-03-02:18
     Browser panes set a Safari-compatible WebKit UA, but sites can still get a
     stale disk-cached document from an older bare-WKWebView UA on app restore.
     Bypass only the local cache for initial browser-pane navigations so the
     first visible load receives markup for the current UA; user reloads and
     in-page navigations keep normal browser cache semantics.
     */
    let cachePolicy: URLRequest.CachePolicy =
      reason == "initial" ? .reloadIgnoringLocalCacheData : .useProtocolCachePolicy
    return URLRequest(url: url, cachePolicy: cachePolicy, timeoutInterval: 60)
  }

  private func retryT3ThreadRouteIfStartupIsStillSettling(
    sessionId: String,
    url: URL,
    error: Error
  ) -> Bool {
    guard NativeT3RuntimeLauncher.isManagedRuntimeURL(url) else {
      return false
    }
    let message = error.localizedDescription
    guard Self.isTransientT3ThreadRouteError(message) else {
      return false
    }
    let attempt = (t3ThreadRouteRetryAttemptsBySessionId[sessionId] ?? 0) + 1
    guard attempt <= 80 else {
      t3ThreadRouteRetryAttemptsBySessionId.removeValue(forKey: sessionId)
      NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.threadRouteRetry.exhausted", [
        "attempt": attempt,
        "error": message,
        "sessionId": sessionId,
        "url": url.absoluteString,
      ])
      return false
    }
    t3ThreadRouteRetryAttemptsBySessionId[sessionId] = attempt
    /**
     CDXC:T3Code 2026-05-02-00:55
     The native T3 pane must not paint a permanent error while the forked
     desktop server is still warming its embed API surface. During startup the
     listener can return 404 for auth/environment endpoints before the same
     process becomes ready; retry route resolution so users see the T3 Code app
     once the real APIs are available instead of a blank or white pane.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.load.threadRouteRetry.scheduled", [
      "attempt": attempt,
      "error": message,
      "sessionId": sessionId,
      "url": url.absoluteString,
    ])
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
      guard self?.webPaneSessions[sessionId] != nil else {
        return
      }
      self?.loadWebPane(sessionId: sessionId, url: url, reason: "threadRouteRetry")
    }
    return true
  }

  private static func isTransientT3ThreadRouteError(_ message: String) -> Bool {
    /**
     CDXC:T3Code 2026-05-24-17:11:
     Native T3 route resolution is allowed to retry while owner auth is still
     being minted, but it must not retry by sending stale cookies to owner-only
     APIs. Treat the explicit missing-owner-bearer guard as startup work so a
     fresh desktop bootstrap can complete and then load the real thread route.
     */
    message.contains("Could not connect to the server")
      || message.contains("timed out")
      || message.contains("returned 404")
      || message.contains("returned 503")
      || message.contains("owner bearer is not ready")
  }

  private func loadWebPaneStatus(
    sessionId: String,
    title: String,
    message: String,
    caption: String?,
    loading: Bool,
    reason: String
  ) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    /**
     CDXC:T3Code 2026-05-02-03:16
     Native T3 panes spend startup time authenticating and resolving the
     managed thread route before the real app URL can load. Show the same
     embedded-workspace loading surface as the webview implementation so users
     do not see an empty gray WKWebView while startup is still valid work.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.status.load", [
      "loading": loading,
      "message": message,
      "reason": reason,
      "sessionId": sessionId,
    ])
    session.webView?.loadHTMLString(
      Self.t3WebPaneStatusHtml(title: title, message: message, caption: caption, loading: loading),
      baseURL: nil
    )
  }

  private static func t3WebPaneStatusHtml(
    title: String,
    message: String,
    caption: String?,
    loading: Bool
  ) -> String {
    let escapedTitle = escapeHtmlText(title.isEmpty ? "T3 Code" : title)
    let escapedMessage = escapeHtmlText(message)
    let escapedCaption = caption.map(escapeHtmlText)
    let spinnerHtml = loading ? #"<div class="spinner" aria-hidden="true"></div>"# : ""
    let captionHtml = escapedCaption.map { #"<div class="caption">\#($0)</div>"# } ?? ""

    return """
      <!DOCTYPE html>
      <html lang="en">
        <head>
          <meta charset="UTF-8" />
          <meta name="viewport" content="width=device-width, initial-scale=1.0" />
          <title>\(escapedTitle)</title>
          <style>
            html, body {
              background: #101722;
              color: #d8e1ee;
              font-family: ui-sans-serif, system-ui, sans-serif;
              height: 100%;
              margin: 0;
            }

            body {
              align-items: center;
              display: flex;
              justify-content: center;
              padding: 24px;
            }

            .status {
              align-items: center;
              color: #d8e1ee;
              display: flex;
              flex-direction: column;
              font-size: 14px;
              gap: 10px;
              letter-spacing: 0.02em;
              opacity: 0.86;
              text-align: center;
            }

            .spinner {
              animation: spin 0.9s linear infinite;
              border: 2px solid rgba(216, 225, 238, 0.18);
              border-radius: 999px;
              border-top-color: rgba(216, 225, 238, 0.95);
              height: 18px;
              width: 18px;
            }

            .caption {
              font-size: 12px;
              opacity: 0.66;
            }

            @keyframes spin {
              from { transform: rotate(0deg); }
              to { transform: rotate(360deg); }
            }
          </style>
        </head>
        <body>
          <div class="status">
            \(spinnerHtml)
            <div>\(escapedMessage)</div>
            \(captionHtml)
          </div>
        </body>
      </html>
      """
  }

  private func loadWebPaneError(session: WebPaneSession, message: String) {
    let escaped = Self.escapeHtmlText(message)
    session.webView?.loadHTMLString(
      """
      <!doctype html>
      <html>
        <head>
          <meta charset="utf-8">
          <style>
            body { margin: 0; background: #1f1f1f; color: #f5f5f5; font: 13px -apple-system, BlinkMacSystemFont, sans-serif; }
            main { padding: 24px; }
            pre { white-space: pre-wrap; color: #ffb4ab; }
          </style>
        </head>
        <body><main><h1>T3 Code failed to open</h1><pre>\(escaped)</pre></main></body>
      </html>
      """,
      baseURL: nil
    )
  }

  private static func escapeHtmlText(_ value: String) -> String {
    value
      .replacingOccurrences(of: "&", with: "&amp;")
      .replacingOccurrences(of: "<", with: "&lt;")
      .replacingOccurrences(of: ">", with: "&gt;")
  }

  private func sessionId(for webView: WKWebView) -> String? {
    webPaneSessions.first { _, session in
      session.webView.map { $0 === webView } ?? false
    }?.key
  }

  private func projectEditorSessionEntry(
    for webView: WKWebView
  ) -> (projectId: String, session: ProjectEditorPaneSession)? {
    projectEditorPaneSessions.first { _, session in
      if session.webView.map({ $0 === webView }) == true {
        return true
      }
      return session.tabs.contains { tab in
        tab.webView.map { $0 === webView } == true
      }
    }.map { entry in
      (projectId: entry.key, session: entry.value)
    }
  }

  private func finishProjectEditorWebKitNavigation(for webView: WKWebView, reason: String) {
    /**
     CDXC:ProjectBoard 2026-05-23-03:36:
     Project mode uses WKWebView inside the project-editor host, but the
     existing WebKit delegate path only understood regular browser panes.
     Treat Project WKWebView navigation as project-editor loading state so the
     initial overlay is removed after the bundled board page finishes loading.
     */
    guard let entry = projectEditorSessionEntry(for: webView) else {
      return
    }
    entry.session.hostView.setInitialLoadingOverlayVisible(false, reason: reason)
    entry.session.hostView.refreshHostedWebView(reason: reason)
    sendEvent(.projectEditorLoadState(projectId: entry.projectId, status: "running", message: nil))
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.webkitNavigation.finish", [
      "projectId": entry.projectId,
      "reason": reason,
      "title": webView.title ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  private func failProjectEditorWebKitNavigation(
    for webView: WKWebView,
    error: Error,
    reason: String
  ) {
    guard let entry = projectEditorSessionEntry(for: webView) else {
      return
    }
    let message = error.localizedDescription
    entry.session.hostView.setInitialLoadingOverlayError(message, reason: reason)
    sendEvent(.projectEditorLoadState(projectId: entry.projectId, status: "error", message: message))
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.webkitNavigation.fail", [
      "error": message,
      "errorCode": (error as NSError).code,
      "errorDomain": (error as NSError).domain,
      "projectId": entry.projectId,
      "reason": reason,
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  private func updateWebPanePageMetadata(for webView: WKWebView, reason: String) {
    guard let sessionId = sessionId(for: webView),
      let session = webPaneSessions[sessionId],
      !session.isManagedT3Pane
    else {
      return
    }

    let displayTitle = webPaneDisplayTitle(for: webView, fallbackTitle: session.title)
    session.titleBarView.setTitle(normalizedTerminalSessionTitle(displayTitle, sessionId: sessionId))
    sendEvent(
      .terminalTitleChanged(
        sessionId: sessionId,
        title: displayTitle,
        sessionPersistenceName: nil))
    if let url = webView.url?.absoluteString, !url.isEmpty {
      /**
       CDXC:BrowserPanes 2026-05-03-03:41
       Browser pane restore must use the real committed WKWebView URL, not the
       initial local wrapper URL used to create the pane. Persist URL changes
       alongside title metadata so quitting and reopening ghostex restores the
       same page the user was viewing.
       */
      sendEvent(.browserUrlChanged(sessionId: sessionId, url: url))
    }
    updateWebPaneFavicon(for: session, pageURL: webView.url, reason: reason)
    NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.metadata.updated", [
      "reason": reason,
      "sessionId": sessionId,
      "title": displayTitle,
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  private func webPaneDisplayTitle(for webView: WKWebView, fallbackTitle: String) -> String {
    let title = webView.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if !title.isEmpty {
      return title
    }
    if let host = webView.url?.host, !host.isEmpty {
      return host
    }
    let fallback = fallbackTitle.trimmingCharacters(in: .whitespacesAndNewlines)
    return fallback.isEmpty ? "Browser" : fallback
  }

  private func updateWebPaneFavicon(for session: WebPaneSession, pageURL: URL?, reason: String) {
    guard let pageURL,
      pageURL.scheme == "http" || pageURL.scheme == "https"
    else {
      session.titleBarView.setFavicon(nil)
      sendEvent(.browserFaviconChanged(sessionId: session.sessionId, faviconDataUrl: nil))
      return
    }

    let sessionId = session.sessionId
    guard let webView = session.webView else {
      return
    }
    webPaneFaviconTasksBySessionId.removeValue(forKey: sessionId)?.cancel()
    webPaneFaviconTasksBySessionId[sessionId] = Task { @MainActor in
      guard self.webPaneSessions[sessionId]?.webView === webView else { return }
      let faviconURL = await self.faviconURL(for: webView, pageURL: pageURL)
      guard !Task.isCancelled, let faviconURL else {
        session.titleBarView.setFavicon(nil)
        self.sendEvent(.browserFaviconChanged(sessionId: sessionId, faviconDataUrl: nil))
        return
      }
      do {
        let (data, response) = try await URLSession.shared.data(from: faviconURL)
        guard !Task.isCancelled,
          let image = NSImage(data: data),
          self.webPaneSessions[sessionId]?.webView?.url?.host == pageURL.host
        else {
          return
        }
        session.titleBarView.setFavicon(image)
        let mimeType =
          (response as? HTTPURLResponse)?.mimeType
          ?? response.mimeType
          ?? Self.faviconMimeType(for: faviconURL)
        /**
         CDXC:BrowserPanes 2026-05-03-11:28
         The sidebar browser card should show the same tab favicon as the
         native browser pane title bar. Send the resolved favicon as a data URL
         so React can render it without re-fetching from the page origin, and
         so the browser session can persist the icon for app restore.
         */
        self.sendEvent(
          .browserFaviconChanged(
            sessionId: sessionId,
            faviconDataUrl: Self.dataUrl(for: data, mimeType: mimeType)))
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.favicon.updated", [
          "faviconUrl": faviconURL.absoluteString,
          "reason": reason,
          "sessionId": sessionId,
        ])
      } catch {
        session.titleBarView.setFavicon(nil)
        self.sendEvent(.browserFaviconChanged(sessionId: sessionId, faviconDataUrl: nil))
        NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.favicon.failed", [
          "error": error.localizedDescription,
          "faviconUrl": faviconURL.absoluteString,
          "reason": reason,
          "sessionId": sessionId,
        ])
      }
    }
  }

  private func faviconURL(for webView: WKWebView, pageURL: URL) async -> URL? {
    let script = """
      (() => {
        const links = Array.from(document.querySelectorAll('link[rel]'));
        const icon = links.find((link) => /(^|\\s)(icon|shortcut icon|apple-touch-icon|mask-icon)(\\s|$)/i.test(link.rel || ''));
        return icon?.href || '';
      })()
      """
    if let href = try? await webView.evaluateJavaScript(script) as? String,
      let resolved = resolvedFaviconURL(href: href, pageURL: pageURL)
    {
      return resolved
    }
    return fallbackFaviconURL(for: pageURL)
  }

  private func updateChromiumWebPaneMetadata(
    sessionId: String,
    title: String?,
    url: String?,
    reason: String
  ) {
    guard let session = webPaneSessions[sessionId], !session.isManagedT3Pane else {
      return
    }
    let displayTitle = chromiumWebPaneDisplayTitle(title: title, url: url, fallbackTitle: session.title)
    session.titleBarView.setTitle(normalizedTerminalSessionTitle(displayTitle, sessionId: sessionId))
    /**
     CDXC:BrowserPanes 2026-05-05-19:47
     Chromium browser panes emit title changes through the shared native title
     event, but they do not have terminal persistence names. Pass nil
     explicitly so the typed host event contract stays complete.
     */
    sendEvent(
      .terminalTitleChanged(
        sessionId: sessionId,
        title: displayTitle,
        sessionPersistenceName: nil))
    if let url, !url.isEmpty {
      sendEvent(.browserUrlChanged(sessionId: sessionId, url: url))
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.chromiumBrowserPane.metadata.updated", [
      "reason": reason,
      "sessionId": sessionId,
      "title": displayTitle,
      "url": url ?? NSNull(),
    ])
  }

  private func chromiumWebPaneDisplayTitle(title: String?, url: String?, fallbackTitle: String) -> String {
    let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if !trimmedTitle.isEmpty {
      return trimmedTitle
    }
    if let url, let host = URL(string: url)?.host, !host.isEmpty {
      return host
    }
    let fallback = fallbackTitle.trimmingCharacters(in: .whitespacesAndNewlines)
    return fallback.isEmpty ? "Browser" : fallback
  }

  private func updateChromiumWebPaneFavicon(sessionId: String, faviconURL: URL?, reason: String) {
    guard let session = webPaneSessions[sessionId] else {
      return
    }
    guard let faviconURL, faviconURL.scheme == "http" || faviconURL.scheme == "https" else {
      session.titleBarView.setFavicon(nil)
      sendEvent(.browserFaviconChanged(sessionId: sessionId, faviconDataUrl: nil))
      return
    }
    let expectedHost = URL(string: session.currentURLString ?? "")?.host
    webPaneFaviconTasksBySessionId.removeValue(forKey: sessionId)?.cancel()
    webPaneFaviconTasksBySessionId[sessionId] = Task { @MainActor in
      do {
        let (data, response) = try await URLSession.shared.data(from: faviconURL)
        guard !Task.isCancelled,
          let image = NSImage(data: data),
          self.webPaneSessions[sessionId]?.chromiumView != nil,
          expectedHost == nil || URL(string: self.webPaneSessions[sessionId]?.currentURLString ?? "")?.host == expectedHost
        else {
          return
        }
        session.titleBarView.setFavicon(image)
        let mimeType =
          (response as? HTTPURLResponse)?.mimeType
          ?? response.mimeType
          ?? Self.faviconMimeType(for: faviconURL)
        self.sendEvent(
          .browserFaviconChanged(
            sessionId: sessionId,
            faviconDataUrl: Self.dataUrl(for: data, mimeType: mimeType)))
        NativeT3CodePaneReproLog.append("nativeWorkspace.chromiumBrowserPane.favicon.updated", [
          "faviconUrl": faviconURL.absoluteString,
          "reason": reason,
          "sessionId": sessionId,
        ])
      } catch {
        session.titleBarView.setFavicon(nil)
        self.sendEvent(.browserFaviconChanged(sessionId: sessionId, faviconDataUrl: nil))
        NativeT3CodePaneReproLog.append("nativeWorkspace.chromiumBrowserPane.favicon.failed", [
          "error": error.localizedDescription,
          "faviconUrl": faviconURL.absoluteString,
          "reason": reason,
          "sessionId": sessionId,
        ])
      }
    }
  }

  private func resolvedFaviconURL(href: String, pageURL: URL) -> URL? {
    let trimmedHref = href.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedHref.isEmpty else {
      return nil
    }
    return URL(string: trimmedHref, relativeTo: pageURL)?.absoluteURL
  }

  private func fallbackFaviconURL(for pageURL: URL) -> URL? {
    guard let scheme = pageURL.scheme,
      scheme == "http" || scheme == "https",
      let host = pageURL.host
    else {
      return nil
    }
    var components = URLComponents()
    components.scheme = scheme
    components.host = host
    components.port = pageURL.port
    components.path = "/favicon.ico"
    return components.url
  }

  private static func dataUrl(for data: Data, mimeType: String?) -> String {
    "data:\(mimeType ?? "image/png");base64,\(data.base64EncodedString())"
  }

  private static func faviconMimeType(for url: URL) -> String {
    switch url.pathExtension.lowercased() {
    case "ico":
      return "image/x-icon"
    case "svg":
      return "image/svg+xml"
    case "jpg", "jpeg":
      return "image/jpeg"
    case "webp":
      return "image/webp"
    default:
      return "image/png"
    }
  }

  /**
   CDXC:T3Code 2026-04-30-15:42
   Native T3 panes must install the same desktop bridge contract before the T3
   bundle runs. Directly loading `/{projectId}/{threadId}` without this bridge
   leaves the React route waiting for environment bootstrap and the pane remains
   on the gray boot shell even though WK navigation finishes successfully.
   */
  private static func t3WebPaneBridgeScript(
    sessionId: String, title: String, workspaceRoot: String?
  ) -> String {
    let encodedSessionId = javascriptStringLiteral(sessionId)
    let encodedTitle = javascriptStringLiteral(title.isEmpty ? "T3 Code" : title)
    let encodedWorkspaceRoot = javascriptStringLiteral(workspaceRoot ?? "")
    return """
      (() => {
        const isManagedT3Origin = () => {
          try {
            return location.protocol === "http:" &&
              (location.hostname === "127.0.0.1" || location.hostname === "localhost") &&
              location.port === "3774";
          } catch {
            return false;
          }
        };
        if (!isManagedT3Origin()) {
          return;
        }
        const sessionId = \(encodedSessionId);
        const sessionTitle = \(encodedTitle);
        const workspaceRoot = \(encodedWorkspaceRoot);
        const handler = window.webkit?.messageHandlers?.\(T3CodePaneDiagnosticsBridge.messageHandlerName);
        const threadIdFromPath = () => {
          const parts = location.pathname.split("/").filter(Boolean);
          return parts.length >= 2 ? parts[1] : "";
        };
        const wsUrl = () => `${location.origin.replace(/^http/i, "ws")}/ws`;
        const currentThreadId = () => threadIdFromPath();
        let lastReportedThreadId = "";
        let lastReportedThreadTitle = "";
        const normalizeThreadTitle = (value) =>
          typeof value === "string" ? value.replace(/\\s+/g, " ").trim() : "";
        const isUsableThreadTitle = (value) => {
          const title = normalizeThreadTitle(value);
          if (!title) {
            return false;
          }
          const lower = title.toLowerCase();
          return lower !== "t3 code" &&
            lower !== "t3 code (alpha)" &&
            lower !== "no active thread" &&
            lower !== "pick a thread to continue";
        };
        const visibleThreadTitle = () => {
          const candidates = [
            window.__VSMUX_T3_ACTIVE_THREAD_TITLE__,
            document.querySelector("header h2[title]")?.getAttribute("title"),
            document.querySelector("header h2")?.textContent,
            document.querySelector("header [title]")?.getAttribute("title")
          ];
          for (const candidate of candidates) {
            if (isUsableThreadTitle(candidate)) {
              return normalizeThreadTitle(candidate);
            }
          }
          return "";
        };
        const reportThreadChange = (payload, reason) => {
          const threadId = String(payload?.threadId || "").trim();
          const title =
            (isUsableThreadTitle(payload?.title) ? normalizeThreadTitle(payload?.title) : "") ||
            visibleThreadTitle() ||
            String(document.title || "");
          const normalizedTitle = normalizeThreadTitle(title);
          if (
            !threadId ||
            (threadId === lastReportedThreadId && normalizedTitle === lastReportedThreadTitle)
          ) {
            return;
          }
          lastReportedThreadId = threadId;
          lastReportedThreadTitle = normalizedTitle;
          try {
            handler?.postMessage({
              href: String(location.href || ""),
              reason,
              threadId,
              title,
              type: "thread-changed"
            });
          } catch {}
        };
        /**
         * CDXC:T3Code 2026-05-04-03:06
         * The embedded T3 app performs client-side thread navigation, so native
         * ghostex must observe route changes inside the WKWebView and let the
         * sidebar preserve one ghostex card per T3 thread instead of silently
         * rebinding the currently visible card.
         *
         * CDXC:T3Code 2026-05-04-04:03
         * T3's own sidebar emits `vsmuxT3ThreadChanged` via postMessage when a
         * user clicks another thread. WKWebView hosts the app as the top-level
         * page, so the bridge listens for that same-window message in addition
         * to URL/history changes; sidebar-thread clicks must create/focus a
         * sibling ghostex card just like route changes.
         *
         * CDXC:T3Code 2026-05-04-04:41
         * Thread titles can arrive after the route/thread id event. De-dupe by
         * thread id plus normalized title so later same-thread title updates
         * still reach the sidebar card title sync path.
         *
         * CDXC:T3Code 2026-05-04-06:23
         * The title shown in the T3 header is the user-facing thread title. Use
         * T3's `__VSMUX_T3_ACTIVE_THREAD_TITLE__` bridge value and the visible
         * header `<h2>` as title sources because `document.title` remains the
         * generic app label and early postMessage payloads may omit the title.
         */
        const reportActiveThread = (reason) => {
          const threadId = currentThreadId();
          reportThreadChange({ threadId, title: document.title }, reason);
        };
        window.addEventListener("message", (event) => {
          const data = event?.data;
          if (!data || typeof data !== "object" || data.type !== "vsmuxT3ThreadChanged") {
            return;
          }
          reportThreadChange(data, "vsmux-message");
        });
        const wrapHistoryMethod = (method) => {
          const original = history[method];
          if (typeof original !== "function") {
            return;
          }
          history[method] = function(...args) {
            const result = original.apply(this, args);
            setTimeout(() => reportActiveThread(method), 0);
            return result;
          };
        };
        wrapHistoryMethod("pushState");
        wrapHistoryMethod("replaceState");
        window.addEventListener("popstate", () => setTimeout(() => reportActiveThread("popstate"), 0));
        window.addEventListener("hashchange", () => setTimeout(() => reportActiveThread("hashchange"), 0));
        setTimeout(() => reportActiveThread("bootstrap"), 0);
        setInterval(() => reportActiveThread("poll"), 1000);
        window.__VSMUX_T3_ACTIVE_THREAD_ID__ = currentThreadId();
        window.__VSMUX_T3_COMPOSER_FOCUS_ENABLED__ = false;
        window.__VSMUX_T3_BOOTSTRAP__ = {
          embedMode: "vsmux-mobile",
          httpOrigin: location.origin,
          sessionId,
          threadId: currentThreadId(),
          workspaceRoot,
          wsUrl: wsUrl()
        };
        const serverExposureState = {
          advertisedHost: null,
          endpointUrl: null,
          mode: "local-only"
        };
        const updateState = {
          canRetry: false,
          checkedAt: null,
          checkedVersion: null,
          downloadPercent: null,
          downloadedVersion: null,
          errorContext: null,
          message: null,
          phase: "idle"
        };
        window.desktopBridge = {
          browser: {
            close: async () => null,
            closeTab: async () => null,
            getState: async (input) => ({
              activeTabId: null,
              lastError: null,
              open: false,
              tabs: [],
              threadId: input?.threadId ?? currentThreadId()
            }),
            goBack: async () => null,
            goForward: async () => null,
            hide: async () => undefined,
            navigate: async () => null,
            newTab: async () => null,
            onState: () => () => undefined,
            open: async () => null,
            openDevTools: async () => undefined,
            reload: async () => null,
            selectTab: async () => null,
            setPanelBounds: async () => null
          },
          confirm: async (message) => window.confirm(String(message)),
          getClientSettings: async () => null,
          getLocalEnvironmentBootstrap: () => ({
            bootstrapToken: "",
            httpBaseUrl: location.origin,
            label: sessionTitle || "T3 Code",
            wsBaseUrl: wsUrl()
          }),
          getWsUrl: () => wsUrl(),
          getSavedEnvironmentRegistry: async () => [],
          getSavedEnvironmentSecret: async () => null,
          getServerExposureState: async () => serverExposureState,
          notifications: {
            isSupported: async () => false,
            show: async () => false
          },
          getUpdateState: async () => updateState,
          installUpdate: async () => ({ accepted: false, completed: false, state: updateState }),
          checkForUpdate: async () => ({ checked: false, state: updateState }),
          downloadUpdate: async () => ({ accepted: false, completed: false, state: updateState }),
          onMenuAction: () => () => undefined,
          onUpdateState: () => () => undefined,
          openExternal: async (url) => {
            try {
              window.open(String(url), "_blank", "noopener,noreferrer");
              return true;
            } catch {
              return false;
            }
          },
          pickFolder: async () => null,
          removeSavedEnvironmentSecret: async () => undefined,
          setClientSettings: async () => undefined,
          setSavedEnvironmentRegistry: async () => undefined,
          setSavedEnvironmentSecret: async () => false,
          setServerExposureMode: async () => serverExposureState,
          setTheme: async () => undefined,
          showContextMenu: async () => null
        };
      })();
      """
  }

  private static func javascriptStringLiteral(_ value: String) -> String {
    guard let data = try? JSONSerialization.data(withJSONObject: [value], options: []),
      let json = String(data: data, encoding: .utf8),
      json.hasPrefix("["),
      json.hasSuffix("]")
    else {
      return "\"\""
    }
    return String(json.dropFirst().dropLast())
  }

  private static func cefDragHoverBridgeScript(
    x: CGFloat,
    y: CGFloat,
    phase: String,
    sourceX: CGFloat?,
    sourceY: CGFloat?
  ) -> String {
    let xLiteral = String(format: "%.2f", Double(x))
    let yLiteral = String(format: "%.2f", Double(y))
    let phaseLiteral = javascriptStringLiteral(phase)
    let sourceXLiteral = sourceX.map { String(format: "%.2f", Double($0)) } ?? "null"
    let sourceYLiteral = sourceY.map { String(format: "%.2f", Double($0)) } ?? "null"
    return """
      (() => {
        const x = \(xLiteral);
        const y = \(yLiteral);
        const phase = \(phaseLiteral);
        const sourceX = \(sourceXLiteral);
        const sourceY = \(sourceYLiteral);
        const bridge = window.__ghostexCEFDragHoverBridge ||= {
          dataTransfer: null,
          lastTarget: null,
          sourceTarget: null
        };
        const stableTargetSelectors = [
          ".pane",
          ".composite",
          ".part.sidebar",
          ".part.activitybar",
          ".monaco-list-row",
          ".monaco-list",
          ".action-item",
          "[role='treeitem']",
          "[draggable='true']"
        ];
        const sourceTargetSelectors = [
          "[draggable='true']",
          ".pane-header",
          ".action-item",
          ".monaco-list-row",
          ".pane"
        ];
        const asElement = (node) => node instanceof Element ? node : node?.parentElement || null;
        const closestByPriority = (element, selectors) => {
          for (const selector of selectors) {
            const match = element.closest(selector);
            if (match) {
              return match;
            }
          }
          return null;
        };
        const stableTargetFor = (rawTarget, selectors = stableTargetSelectors) => {
          const element = asElement(rawTarget);
          if (!element) {
            return null;
          }
          return closestByPriority(element, selectors) || element;
        };
        const makeDataTransfer = () => {
          if (bridge.dataTransfer) {
            return bridge.dataTransfer;
          }
          try {
            bridge.dataTransfer = new DataTransfer();
            try {
              bridge.dataTransfer.effectAllowed = "move";
              bridge.dataTransfer.dropEffect = "move";
            } catch {}
            return bridge.dataTransfer;
          } catch {
            return null;
          }
        };
        const dispatchDragEvent = (target, type) => {
          if (!target) {
            return false;
          }
          const dataTransfer = makeDataTransfer();
          const init = {
            bubbles: true,
            cancelable: true,
            clientX: x,
            clientY: y,
            composed: true,
            dataTransfer,
            screenX: window.screenX + x,
            screenY: window.screenY + y,
            view: window
          };
          let event;
          try {
            event = new DragEvent(type, init);
          } catch {
            event = document.createEvent("DragEvent");
            event.initMouseEvent(type, true, true, window, 0, 0, 0, x, y, false, false, false, false, 0, null);
          }
          return target.dispatchEvent(event);
        };
        if (phase === "cancel") {
          dispatchDragEvent(bridge.lastTarget, "dragleave");
          dispatchDragEvent(bridge.sourceTarget || bridge.lastTarget, "dragend");
          bridge.lastTarget = null;
          bridge.sourceTarget = null;
          bridge.dataTransfer = null;
          return;
        }
        const target = document.elementFromPoint(x, y);
        const stableTarget = stableTargetFor(target);
        if (!stableTarget) {
          return;
        }
        if (phase === "start") {
          const sourceTarget = sourceX === null || sourceY === null ? null : document.elementFromPoint(sourceX, sourceY);
          bridge.sourceTarget = stableTargetFor(sourceTarget, sourceTargetSelectors) || stableTarget;
        }
        if (bridge.lastTarget !== stableTarget) {
          dispatchDragEvent(bridge.lastTarget, "dragleave");
          dispatchDragEvent(stableTarget, "dragenter");
          bridge.lastTarget = stableTarget;
        }
        dispatchDragEvent(stableTarget, "dragover");
        if (phase === "drop") {
          dispatchDragEvent(stableTarget, "drop");
          dispatchDragEvent(bridge.sourceTarget || stableTarget, "dragend");
          bridge.lastTarget = null;
          bridge.sourceTarget = null;
          bridge.dataTransfer = null;
        }
      })();
      """
  }

  private static let t3WebPaneDiagnosticsScript = """
    (() => {
      const handler = window.webkit?.messageHandlers?.\(T3CodePaneDiagnosticsBridge.messageHandlerName);
      if (!handler) {
        return;
      }
      const summarize = (value) => {
        try {
          if (value instanceof Error) {
            return value.stack || value.message || String(value);
          }
          if (typeof value === "object" && value !== null) {
            return JSON.stringify(value);
          }
          return String(value);
        } catch {
          return String(value);
        }
      };
      const post = (payload) => {
        try {
          handler.postMessage({
            href: String(location.href || ""),
            readyState: String(document.readyState || ""),
            timestamp: new Date().toISOString(),
            ...payload
          });
        } catch {}
      };
      window.addEventListener("error", (event) => {
        const target = event.target;
        const isResourceError = target && target !== window;
        post({
          column: event.colno || 0,
          line: event.lineno || 0,
          message: String(event.message || ""),
          resourceHref: isResourceError ? String(target.src || target.href || "") : "",
          source: String(event.filename || ""),
          stack: event.error && event.error.stack ? String(event.error.stack) : "",
          type: isResourceError ? "resource-error" : "error"
        });
      }, true);
      window.addEventListener("unhandledrejection", (event) => {
        post({
          message: summarize(event.reason),
          stack: event.reason && event.reason.stack ? String(event.reason.stack) : "",
          type: "unhandledrejection"
        });
      });
      for (const method of ["error", "warn"]) {
        const original = console[method]?.bind(console);
        console[method] = (...args) => {
          post({ message: args.map(summarize).join(" "), type: `console.${method}` });
          original?.(...args);
        };
      }
      post({ type: "diagnostics-ready" });
    })();
    """

  private func searchBarFrame(in terminalRect: CGRect) -> CGRect {
    let width = min(CGFloat(300), max(terminalRect.width - 16, 180))
    let height = CGFloat(34)
    return CGRect(
      x: terminalRect.maxX - width - 8,
      y: terminalRect.maxY - height - 8,
      width: width,
      height: height
    )
  }

  private func logTerminalResizeIfNeeded(
    session: TerminalSession,
    paneRect: CGRect,
    titleBarRect: CGRect,
    availableTerminalRect: CGRect,
    terminalRect: CGRect
  ) {
    /**
     CDXC:NativeTerminalResize 2026-04-29-02:22
     Narrow-pane Claude Code rendering regressions need geometry diagnostics
     from every native resize layer. Log only changed signatures so PTY size,
     Ghostty surface size, scroll content size, and visible pane dimensions can
     be compared without flooding the app log during no-op layout passes.
     */
    let nestedScrollView = firstNestedScrollView(in: session.scrollView)
    let publishedSurfaceSize = session.view.surfaceSize
    let surfaceSize = session.view.surface.map { ghostty_surface_size($0) } ?? publishedSurfaceSize
    let surfacePadding = session.view.surface.map { ghostty_surface_padding($0) }
    let cellSize = session.view.cellSize
    let estimatedColumns =
      cellSize.width > 0 ? Int(floor(terminalRect.width / cellSize.width)) : nil
    let estimatedRows =
      cellSize.height > 0 ? Int(floor(terminalRect.height / cellSize.height)) : nil
    /**
     CDXC:NativeTerminalResize 2026-04-29-07:50
     Claude Code/Ink rerenders from the PTY rows and columns that Ghostty
     reports after subtracting terminal padding. Log synchronous core size,
     published AppKit size, backing-pixel metrics, actual Ghostty padding, and
     residual non-grid space so resize bugs can distinguish stale Swift state
     from Ghostty grid math.
     */
    let coreSurfaceSize = surfaceSize.map { size in
      session.view.convertFromBacking(
        NSSize(width: Double(size.width_px), height: Double(size.height_px)))
    }
    let inferredHorizontalPaddingPx = surfaceSize.map {
      Int($0.width_px) - Int($0.columns) * Int($0.cell_width_px)
    }
    let inferredVerticalPaddingPx = surfaceSize.map {
      Int($0.height_px) - Int($0.rows) * Int($0.cell_height_px)
    }
    let inferredPadding = inferredPaddingPoints(
      view: session.view,
      horizontalPx: inferredHorizontalPaddingPx,
      verticalPx: inferredVerticalPaddingPx)
    let actualPadding = actualPaddingPoints(view: session.view, padding: surfacePadding)
    let paddingAwareEstimatedColumns = paddingAwareEstimatedCellCount(
      available: terminalRect.width,
      padding: actualPadding?.width,
      cell: cellSize.width)
    let paddingAwareEstimatedRows = paddingAwareEstimatedCellCount(
      available: terminalRect.height,
      padding: actualPadding?.height,
      cell: cellSize.height)
    var signatureParts: [String] = []
    signatureParts.append(roundedSignature(paneRect.size.width))
    signatureParts.append(roundedSignature(paneRect.size.height))
    signatureParts.append(roundedSignature(terminalRect.size.width))
    signatureParts.append(roundedSignature(terminalRect.size.height))
    signatureParts.append(roundedSignature(session.scrollView.bounds.size.width))
    signatureParts.append(roundedSignature(session.scrollView.bounds.size.height))
    signatureParts.append(roundedSignature(session.view.frame.size.width))
    signatureParts.append(roundedSignature(session.view.frame.size.height))
    signatureParts.append(String(surfaceSize?.columns ?? 0))
    signatureParts.append(String(surfaceSize?.rows ?? 0))
    signatureParts.append(String(estimatedColumns ?? 0))
    signatureParts.append(String(estimatedRows ?? 0))
    signatureParts.append(String(paddingAwareEstimatedColumns ?? 0))
    signatureParts.append(String(paddingAwareEstimatedRows ?? 0))
    signatureParts.append(String(surfaceSize?.width_px ?? 0))
    signatureParts.append(String(surfaceSize?.height_px ?? 0))
    let signature = signatureParts.joined(separator: "x")
    if resizeLogSignatureBySessionId[session.sessionId] == signature {
      return
    }
    resizeLogSignatureBySessionId[session.sessionId] = signature
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.terminalResize",
      details: [
        "cellSize": describeSize(cellSize),
        "coreSurfaceSizeLogical": coreSurfaceSize.map { describeSize($0) } ?? NSNull(),
        "coreSurfaceSizePixels": surfaceSize.map {
          ["height": Int($0.height_px), "width": Int($0.width_px)]
        } ?? NSNull(),
        "coreSurfaceCellSizePixels": surfaceSize.map {
          ["height": Int($0.cell_height_px), "width": Int($0.cell_width_px)]
        } ?? NSNull(),
        "coreSurfaceGridSizePixels": surfaceSize.map {
          [
            "height": Int($0.rows) * Int($0.cell_height_px),
            "width": Int($0.columns) * Int($0.cell_width_px),
          ]
        } ?? NSNull(),
        "estimatedColumns": nullableInt(estimatedColumns),
        "estimatedRows": nullableInt(estimatedRows),
        "focusedSessionId": nullableString(focusedSessionId),
        "inferredPaddingPixels": inferredHorizontalPaddingPx.map { horizontal in
          [
            "horizontal": horizontal,
            "vertical": inferredVerticalPaddingPx ?? 0,
          ]
        } ?? NSNull(),
        "inferredPaddingPoints": inferredPadding.map { describeSize($0) } ?? NSNull(),
        "surfacePaddingPixels": surfacePadding.map {
          [
            "bottom": Int($0.bottom_px),
            "left": Int($0.left_px),
            "right": Int($0.right_px),
            "top": Int($0.top_px),
          ]
        } ?? NSNull(),
        "surfacePaddingPoints": actualPadding.map { describeSize($0) } ?? NSNull(),
        "nestedScrollContentSize": nestedScrollView.map { describeSize($0.contentSize) }
          ?? NSNull(),
        "nestedScrollDocumentVisibleRect": nestedScrollView.map {
          describeFrame($0.contentView.documentVisibleRect)
        } ?? NSNull(),
        "paneGap": Double(paneGap),
        "paneRect": describeFrame(paneRect),
        "paddingAwareEstimatedColumns": nullableInt(paddingAwareEstimatedColumns),
        "paddingAwareEstimatedRows": nullableInt(paddingAwareEstimatedRows),
        "publishedSurfaceSizeColumns": publishedSurfaceSize.map { Int($0.columns) } ?? NSNull(),
        "publishedSurfaceSizeRows": publishedSurfaceSize.map { Int($0.rows) } ?? NSNull(),
        "rawTerminalRect": describeFrame(availableTerminalRect),
        "scrollViewBounds": describeFrame(session.scrollView.bounds),
        "scrollViewFrame": describeFrame(session.scrollView.frame),
        "sessionId": session.sessionId,
        "surfaceFrame": describeFrame(session.view.frame),
        "surfaceSizeColumns": surfaceSize.map { Int($0.columns) } ?? NSNull(),
        "surfaceSizeRows": surfaceSize.map { Int($0.rows) } ?? NSNull(),
        "terminalRect": describeFrame(terminalRect),
        "titleBarRect": describeFrame(titleBarRect),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
  }

  private func inferredPaddingPoints(
    view: NSView,
    horizontalPx: Int?,
    verticalPx: Int?
  ) -> NSSize? {
    guard let horizontalPx, let verticalPx else {
      return nil
    }
    let backingSize = NSSize(width: Double(horizontalPx), height: Double(verticalPx))
    return view.convertFromBacking(backingSize)
  }

  private func actualPaddingPoints(
    view: NSView,
    padding: ghostty_surface_padding_s?
  ) -> NSSize? {
    guard let padding else {
      return nil
    }
    let backingSize = NSSize(
      width: Double(padding.left_px + padding.right_px),
      height: Double(padding.top_px + padding.bottom_px))
    return view.convertFromBacking(backingSize)
  }

  private func paddingAwareEstimatedCellCount(
    available: CGFloat,
    padding: CGFloat?,
    cell: CGFloat
  ) -> Int? {
    guard let padding, cell > 0 else {
      return nil
    }
    return Int(floor(max(available - padding, 0) / cell))
  }

  private func firstNestedScrollView(in view: NSView) -> NSScrollView? {
    if let scrollView = view as? NSScrollView {
      return scrollView
    }
    for subview in view.subviews {
      if let scrollView = firstNestedScrollView(in: subview) {
        return scrollView
      }
    }
    return nil
  }

  private func roundedSignature(_ value: CGFloat) -> String {
    String(Int(value.rounded()))
  }

  private func nativeLayoutSignature(
    activeSessionIds: Set<String>,
    activeProjectEditorId: String?,
    layout: NativeTerminalLayout?,
    paneGap: CGFloat,
    poppedOutSessionIds: Set<String>
  ) -> String {
    [
      "active=\(activeSessionIds.sorted().joined(separator: ","))",
      "editor=\(activeProjectEditorId ?? "")",
      "gap=\(roundedSignature(paneGap * 100))",
      "layout=\(nativeLayoutNodeSignature(layout))",
      "popped=\(poppedOutSessionIds.sorted().joined(separator: ","))",
    ].joined(separator: "|")
  }

  private func nativeLayoutNodeSignature(_ layout: NativeTerminalLayout?) -> String {
    guard let layout else {
      return "none"
    }
    switch layout {
    case .leaf(let sessionId):
      return "leaf:\(sessionId)"
    case .tabs(let activeSessionId, let sessionIds):
      return "tabs:\(activeSessionId ?? "nil"):[\(sessionIds.joined(separator: ","))]"
    case .split(let direction, let ratio, let children):
      let ratioSignature = ratio.map { String(format: "%.5f", $0) } ?? "nil"
      return
        "split:\(direction.rawValue):\(ratioSignature):[\(children.map { nativeLayoutNodeSignature($0) }.joined(separator: ","))]"
    }
  }

  private func moveOffscreen(_ view: NSView) {
    let size =
      view.frame.size.width > 1 && view.frame.size.height > 1
      ? view.frame.size
      : bounds.size
    let nextFrame = CGRect(
      x: bounds.maxX + 10_000,
      y: bounds.maxY + 10_000,
      width: max(size.width, 1),
      height: max(size.height, 1)
    )
    if !rectsMatch(view.frame, nextFrame) {
      view.frame = nextFrame
    }
  }

  private func applyWorkspaceBackgroundColor(_ value: String?) {
    let nextValue = value ?? ""
    guard workspaceBackgroundColorValue != nextValue else {
      return
    }
    /**
     CDXC:NativeGpu 2026-05-08-16:45
     Passive metadata sync can arrive without a visual workspace change.
     Reassign the AppKit backing-layer color only when the configured value
     changes so repeated status updates do not dirty the whole workspace layer.
     */
    workspaceBackgroundColorValue = nextValue
    layer?.backgroundColor = Self.workspaceBackgroundColor(value).cgColor
  }

  private func setHidden(_ hidden: Bool, for view: NSView) {
    guard view.isHidden != hidden else {
      return
    }
    view.isHidden = hidden
  }

  private func updateAllTerminalBorders() {
    for sessionId in sessions.keys {
      updateTerminalBorder(for: sessionId)
    }
    for sessionId in webPaneSessions.keys {
      updateTerminalBorder(for: sessionId)
    }
  }

  private func updateTerminalBorder(for sessionId: String) {
    if let session = webPaneSessions[sessionId] {
      let isActive = activeSessionIds.contains(sessionId)
      setHidden(!isActive, for: session.hostView)
      setHidden(!isActive, for: session.titleBarView)
      setHidden(!isActive, for: session.borderView)
      session.titleBarView.setState(activity: sessionActivities[sessionId])
      session.titleBarView.setTabActivities(sessionActivities)
      session.titleBarView.setTabDelayedSendRemainingLabels(sessionDelayedSendRemainingLabels)
      session.titleBarView.setTabIdentityIcons(
        faviconDataUrls: sessionFaviconDataUrls,
        agentIconDataUrls: sessionAgentIconDataUrls,
        agentIconColors: sessionAgentIconColors)
      session.titleBarView.setFocusedPane(focusedSessionId == sessionId)
      session.borderView.setState(
        isFocused: shouldShowFocusedPaneBorder(for: sessionId),
        isAttention: attentionSessionIds.contains(sessionId)
      )
      return
    }

    guard let session = sessions[sessionId] else {
      return
    }
    let isCommandActive = commandsPanelActiveSessionIds.contains(sessionId)
    let isActive = activeSessionIds.contains(sessionId) || isCommandActive
    setHidden(!isActive, for: session.titleBarView)
    setHidden(!isActive || session.view.searchState == nil, for: session.searchBarView)
    setHidden(!isActive, for: session.borderView)
    session.titleBarView.setState(
      activity: sessionActivities[sessionId]
    )
    session.titleBarView.setTabActivities(sessionActivities)
    session.titleBarView.setTabDelayedSendRemainingLabels(sessionDelayedSendRemainingLabels)
    session.titleBarView.setTabIdentityIcons(
      faviconDataUrls: sessionFaviconDataUrls,
      agentIconDataUrls: sessionAgentIconDataUrls,
      agentIconColors: sessionAgentIconColors)
    let isCommandFocused = isCommandActive && commandPanelFocusedResponderSessionId() == sessionId
    session.titleBarView.setFocusedPane(focusedSessionId == sessionId || isCommandFocused)
    session.borderView.setState(
      isFocused: shouldShowFocusedPaneBorder(for: sessionId),
      isAttention: attentionSessionIds.contains(sessionId)
    )
  }

  private func shouldShowFocusedPaneBorder(for sessionId: String) -> Bool {
    /**
     CDXC:NativePaneResize 2026-05-11-09:48
     Focused pane borders were temporarily suppressed while diagnosing split
     resize misses from the focused pane side. That test did not change the
     failure, so keep the selected-pane border enabled and fix resize event
     ownership instead of hiding focus chrome.

     CDXC:NativePaneChrome 2026-05-15-08:08:
     Single-pane workspaces should not draw an active pane border, even when
     that pane is a tab group with multiple active sessions. Gate workspace
     focus chrome on visible pane owners so the border appears only for real
     split layouts where focus needs spatial disambiguation.
     */
    let isCommandPanelSession = commandsPanelActiveSessionIds.contains(sessionId)
    if isCommandPanelSession {
      return commandsPanelFocusedSessionId == sessionId
        && commandPanelFocusedResponderSessionId() == sessionId
    }
    return !commandPanelOwnsResponder()
      && focusedSessionId == sessionId
      && orderedVisiblePaneOwnerSessionIds().count > 1
  }

  private func commandPanelOwnsResponder() -> Bool {
    commandPanelFocusedResponderSessionId() != nil
  }

  private func commandPanelFocusedResponderSessionId() -> String? {
    guard let responderSessionId = currentResponderSessionId(),
      commandsPanelActiveSessionIds.contains(responderSessionId)
    else {
      return nil
    }
    return responderSessionId
  }

  private func keyboardRouteDebugPayload(
    surfaceSessionId: String?,
    event: NSEvent? = nil
  ) -> [String: Any] {
    let responderSessionId = currentResponderSessionId()
    var mismatchTypes: [String] = []
    if let surfaceSessionId, let focusedSessionId, surfaceSessionId != focusedSessionId {
      mismatchTypes.append("surfaceSessionDiffersFromFocusRing")
    }
    if let responderSessionId, let focusedSessionId, responderSessionId != focusedSessionId {
      mismatchTypes.append("firstResponderDiffersFromFocusRing")
    }
    if let surfaceSessionId, let responderSessionId, surfaceSessionId != responderSessionId {
      mismatchTypes.append("surfaceSessionDiffersFromFirstResponder")
    }

    var payload: [String: Any] = [
      "activeProjectEditorId": nullableString(activeProjectEditorId),
      "activeSessionIds": Array(activeSessionIds).sorted(),
      "focusedSessionId": nullableString(focusedSessionId),
      "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
      "mismatchTypes": mismatchTypes,
      "poppedOutSessionIds": Array(poppedOutSessionIds).sorted(),
      "responder": responderSnapshot(),
      "responderMatchesFocusedSession": nullableBool(
        responderSessionId.flatMap { responder in focusedSessionId.map { responder == $0 } }),
      "responderSessionId": nullableString(responderSessionId),
      "surfaceMatchesFocusedSession": nullableBool(
        surfaceSessionId.flatMap { surface in focusedSessionId.map { surface == $0 } }),
      "surfaceMatchesResponderSession": nullableBool(
        surfaceSessionId.flatMap { surface in responderSessionId.map { surface == $0 } }),
      "surfaceSessionId": nullableString(surfaceSessionId),
      "visibleSessionIds": orderedVisibleSessionIds(),
      "windowIsKey": window?.isKeyWindow ?? false,
      "windowNumber": window?.windowNumber ?? 0,
    ]

    if let event {
      payload["charactersIgnoringModifiersLength"] = event.charactersIgnoringModifiers?.count ?? 0
      payload["charactersLength"] = event.characters?.count ?? 0
      payload["eventTimestamp"] = event.timestamp
      payload["eventWindowNumber"] = event.window?.windowNumber ?? 0
      payload["isARepeat"] = event.isARepeat
      payload["keyCode"] = Int(event.keyCode)
      payload["modifierFlags"] = Self.keyboardModifierNames(event.modifierFlags)
    }

    return payload
  }

  private func focusedSurfaceSessionIds() -> [String] {
    sessions.keys.sorted().compactMap { sessionId in
      guard sessions[sessionId]?.view.focused == true else {
        return nil
      }
      return sessionId
    }
  }

  func activationDebugSnapshot() -> [String: Any] {
    /**
     CDXC:FocusStealDiagnostics 2026-05-15-10:54:
     App activation can steal focus without changing the selected terminal session.
     Expose a compact workspace snapshot for native lifecycle logs so later repros can compare selected session, first responder, visible panes, and Ghostty focused flags at the exact app activation boundary.
     */
    return [
      "activeProjectEditorId": nullableString(activeProjectEditorId),
      "activeSessionIds": Array(activeSessionIds).sorted(),
      "commandsPanelFocusedSessionId": nullableString(commandsPanelFocusedSessionId),
      "focusedSessionId": nullableString(focusedSessionId),
      "focusedSurfaceSessionIds": focusedSurfaceSessionIds(),
      "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
      "responder": responderSnapshot(),
      "responderSessionId": nullableString(currentResponderSessionId()),
      "visibleSessionIds": orderedVisibleSessionIds(),
      "windowIsKey": window?.isKeyWindow ?? false,
      "windowNumber": window?.windowNumber ?? 0,
    ]
  }

  private func focusSurfaceStateSnapshot() -> [[String: Any]] {
    sessions.keys.sorted().compactMap { sessionId in
      guard let session = sessions[sessionId] else {
        return nil
      }
      let surfaceWindow = session.view.window
      return [
        "borderFocused": shouldShowFocusedPaneBorder(for: sessionId),
        "frame": describeFrame(session.view.frame),
        "isActive": activeSessionIds.contains(sessionId),
        "isFirstResponder": surfaceWindow?.firstResponder === session.view,
        "isPoppedOut": poppedOutSessionIds.contains(sessionId),
        "scrollViewHidden": session.scrollView.isHidden,
        "sessionId": sessionId,
        "surfaceFocusedFlag": session.view.focused,
        "surfaceWindowIsKey": surfaceWindow?.isKeyWindow ?? false,
        "surfaceWindowNumber": surfaceWindow?.windowNumber ?? 0,
        "titleBarFocused": focusedSessionId == sessionId,
        "titleBarHidden": session.titleBarView.isHidden,
      ]
    }
  }

  private func logFocusSurfaceState(
    event: String,
    reason: String,
    details: [String: Any] = [:]
  ) {
    guard NativeDebugLogging.isEnabled else {
      return
    }
    let surfaceFocusIds = focusedSurfaceSessionIds()
    var payload = details
    /**
     CDXC:NativeTerminalStartupFocus 2026-05-11-12:31
     Startup focus repros need one snapshot that compares the sidebar-selected
     pane, AppKit first responder, and Ghostty's per-surface focused flag. Log
     this only through Debugging Mode so normal terminal startup remains quiet.
    */
    payload["activeSessionIds"] = Array(activeSessionIds).sorted()
    payload["focusedSessionId"] = nullableString(focusedSessionId)
    payload["focusedSurfaceCount"] = surfaceFocusIds.count
    payload["focusedSurfaceSessionIds"] = surfaceFocusIds
    payload["lastEmittedFocusedSessionId"] = nullableString(lastEmittedFocusedSessionId)
    payload["reason"] = reason
    payload["responder"] = responderSnapshot()
    payload["surfaces"] = focusSurfaceStateSnapshot()
    payload["visibleSessionIds"] = orderedVisibleSessionIds()
    payload["workspaceWindowIsKey"] = window?.isKeyWindow ?? false
    payload["workspaceWindowNumber"] = window?.windowNumber ?? 0
    TerminalFocusDebugLog.append(event: event, details: payload)
  }

  private static func keyboardModifierNames(_ flags: NSEvent.ModifierFlags) -> [String] {
    let normalizedFlags = flags.intersection(.deviceIndependentFlagsMask)
    var names: [String] = []
    if normalizedFlags.contains(.capsLock) {
      names.append("capsLock")
    }
    if normalizedFlags.contains(.shift) {
      names.append("shift")
    }
    if normalizedFlags.contains(.control) {
      names.append("control")
    }
    if normalizedFlags.contains(.option) {
      names.append("option")
    }
    if normalizedFlags.contains(.command) {
      names.append("command")
    }
    if normalizedFlags.contains(.numericPad) {
      names.append("numericPad")
    }
    if normalizedFlags.contains(.help) {
      names.append("help")
    }
    if normalizedFlags.contains(.function) {
      names.append("function")
    }
    return names
  }

  private static func textInputLength(_ value: Any) -> Int {
    if let string = value as? String {
      return string.count
    }
    if let attributedString = value as? NSAttributedString {
      return attributedString.string.count
    }
    return 0
  }

  private func responderSnapshot() -> [String: Any] {
    guard let responder = window?.firstResponder else {
      return [
        "className": "nil",
        "sessionId": NSNull(),
      ]
    }
    return [
      "className": String(describing: type(of: responder)),
      "sessionId": nullableString(sessionId(containing: responder)),
    ]
  }

  private func currentResponderSessionId() -> String? {
    guard let responder = window?.firstResponder else {
      return nil
    }
    return sessionId(containing: responder)
  }

  private func shouldPreserveNonTerminalFirstResponder() -> Bool {
    guard let responder = window?.firstResponder else {
      return false
    }
    return sessionId(containing: responder) == nil
  }

  private func sessionId(containing responder: NSResponder) -> String? {
    guard let responderView = responder as? NSView else {
      return sessions.first { _, session in responder === session.view }?.key
    }
    for (sessionId, session) in sessions {
      if responderView === session.containerView || responderView.isDescendant(of: session.containerView)
        || responderView === session.view || responderView.isDescendant(of: session.view)
      {
        return sessionId
      }
    }
    for (sessionId, session) in webPaneSessions {
      let contentView = session.browserContentView
      if responderView === session.containerView || responderView.isDescendant(of: session.containerView)
        || responderView === session.hostView || responderView.isDescendant(of: session.hostView)
        || responderView === contentView || responderView.isDescendant(of: contentView)
      {
        return sessionId
      }
    }
    return nil
  }

  private func emitFocusedSessionIfNeeded(for responder: NSResponder, reason: String) {
    /**
     CDXC:NativeTerminalFocus 2026-04-26-22:22
     Only user/AppKit-originated first-responder changes should update the
     sidebar focus store. Programmatic focus calls from setActiveTerminalSet
     already came from sidebar state; echoing them back creates a feedback
     loop where each layout sync can make another pane active.
     */
    guard let focusedSessionId = sessionId(containing: responder) else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.focusedResponderIgnored",
        details: [
          "reason": reason,
          "responder": String(describing: type(of: responder)),
        ])
      return
    }
    guard activeSessionIds.contains(focusedSessionId)
      || commandsPanelActiveSessionIds.contains(focusedSessionId)
    else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.focusedInactiveSessionIgnored",
        details: [
          "activeSessionIds": Array(activeSessionIds).sorted(),
          "commandsPanelActiveSessionIds": Array(commandsPanelActiveSessionIds).sorted(),
          "reason": reason,
          "sessionId": focusedSessionId,
        ])
      return
    }
    emitFocusedSessionSelectionIfNeeded(sessionId: focusedSessionId, reason: reason)
  }

  private func emitFocusedSessionSelectionIfNeeded(sessionId focusedSessionId: String, reason: String) {
    let localFocusedSessionId = commandsPanelActiveSessionIds.contains(focusedSessionId)
      ? commandsPanelFocusedSessionId
      : self.focusedSessionId
    if lastEmittedFocusedSessionId == focusedSessionId, localFocusedSessionId == focusedSessionId {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.terminalFocused.duplicateSkipped",
        details: [
          "reason": reason,
          "sessionId": focusedSessionId,
        ])
      return
    }
    if lastEmittedFocusedSessionId == focusedSessionId, localFocusedSessionId != focusedSessionId {
      /**
       CDXC:NativeTerminalFocus 2026-05-13-07:41
       A previous native focus emission can be followed by sidebar/layout sync
       repainting the local focused pane back to an older session. In that stale
       local state, the next matching responder focus is not a behavioral
       duplicate; re-apply and re-emit it so the border and store converge.
       */
      TerminalFocusDebugLog.append(
        event: "nativeFocusTrace.duplicateFocusWithStaleBorderState",
        details: [
          "emittedFocusedSessionId": focusedSessionId,
          "lastEmittedFocusedSessionId": nullableString(lastEmittedFocusedSessionId),
          "localFocusedSessionId": nullableString(localFocusedSessionId),
          "reason": reason,
          "responder": responderSnapshot(),
        ])
    }
    lastEmittedFocusedSessionId = focusedSessionId
    if commandsPanelActiveSessionIds.contains(focusedSessionId) {
      commandsPanelFocusedSessionId = focusedSessionId
    } else {
      self.focusedSessionId = focusedSessionId
    }
    updateAllTerminalBorders()
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.terminalFocused.emitted",
      details: [
        "reason": reason,
        "sessionId": focusedSessionId,
      ])
    TerminalFocusDebugLog.append(
      event: "nativeFocusTrace.terminalFocusedEmitted",
      details: [
        "activeSessionIds": Array(activeSessionIds).sorted(),
        "commandsPanelActiveSessionIds": Array(commandsPanelActiveSessionIds).sorted(),
        "commandsPanelFocusedSessionId": nullableString(commandsPanelFocusedSessionId),
        "focusedSessionId": focusedSessionId,
        "reason": reason,
        "responder": responderSnapshot(),
        "visibleSessionIds": orderedVisibleSessionIds(),
      ])
    sendEvent(.terminalFocused(sessionId: focusedSessionId))
  }

  private func describeFrame(_ frame: CGRect) -> [String: Double] {
    [
      "height": Double(frame.height),
      "maxX": Double(frame.maxX),
      "maxY": Double(frame.maxY),
      "minX": Double(frame.minX),
      "minY": Double(frame.minY),
      "width": Double(frame.width),
    ]
  }

  private func describePoint(_ point: CGPoint) -> [String: Double] {
    [
      "x": Double(point.x),
      "y": Double(point.y),
    ]
  }

  private func describeSize(_ size: CGSize) -> [String: Double] {
    [
      "height": Double(size.height),
      "width": Double(size.width),
    ]
  }

  private func summarizeTerminalText(_ text: String) -> String {
    String(
      text.replacingOccurrences(of: "\r", with: "\\r")
        .replacingOccurrences(of: "\n", with: "\\n")
        .prefix(160))
  }

  private func nullableString(_ value: String?) -> Any {
    value ?? NSNull()
  }

  private func nullableInt(_ value: Int?) -> Any {
    value ?? NSNull()
  }

  private func nullableBool(_ value: Bool?) -> Any {
    value ?? NSNull()
  }

  private static func clampedPaneGap(_ value: Double?) -> CGFloat {
    guard let value, value.isFinite else {
      return defaultPaneGap
    }
    return CGFloat(min(48, max(0, value)))
  }

  private static func clampedCommandsPanelHeightRatio(_ value: Double?) -> CGFloat {
    guard let value, value.isFinite else {
      return defaultCommandsPanelHeightRatio
    }
    return CGFloat(
      min(
        Double(maximumCommandsPanelHeightRatio),
        max(Double(minimumCommandsPanelHeightRatio), value)))
  }

  private static func workspaceBackgroundColor(_ value: String?) -> NSColor {
    guard let color = parseHexColor(value?.trimmingCharacters(in: .whitespacesAndNewlines)) else {
      return defaultWorkspaceBackgroundColor
    }
    return color
  }

  private static func parseHexColor(_ value: String?) -> NSColor? {
    guard let value else {
      return nil
    }
    let pattern = #"^#?([0-9a-fA-F]{6})$"#
    guard
      let match = value.range(of: pattern, options: .regularExpression),
      match == value.startIndex..<value.endIndex
    else {
      return nil
    }
    let hex = value.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
    guard let rawValue = Int(hex, radix: 16) else {
      return nil
    }
    return NSColor(
      calibratedRed: CGFloat((rawValue >> 16) & 0xff) / 255,
      green: CGFloat((rawValue >> 8) & 0xff) / 255,
      blue: CGFloat(rawValue & 0xff) / 255,
      alpha: 1
    )
  }

  private func startExitPollingIfNeeded() {
    guard exitPollTimer == nil else { return }
    exitPollTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
      MainActor.assumeIsolated {
        self?.pollExitedSurfaces()
      }
    }
  }

  private func stopExitPollingIfIdle() {
    if sessions.isEmpty {
      exitPollTimer?.invalidate()
      exitPollTimer = nil
    }
  }

  private func pollExitedSurfaces() {
    let exitedSessionIds = sessions.compactMap { sessionId, session in
      session.view.processExited ? sessionId : nil
    }
    for sessionId in exitedSessionIds {
      closeTerminal(sessionId: sessionId, requestGhosttyClose: false, reason: "processExitedPoll")
    }
  }

  private func leafSessionIds(_ node: NativeTerminalLayout) -> [String] {
    switch node {
    case .leaf(let sessionId):
      return [sessionId]
    case .tabs(_, let sessionIds):
      return sessionIds
    case .split(_, _, let children):
      return children.flatMap(leafSessionIds)
    }
  }

  private func prunedLayout(removing sessionId: String, from node: NativeTerminalLayout?)
    -> NativeTerminalLayout?
  {
    guard let node else { return nil }
    switch node {
    case .leaf(let existingSessionId):
      return existingSessionId == sessionId ? nil : node
    case .tabs(let activeSessionId, let sessionIds):
      let nextSessionIds = sessionIds.filter { $0 != sessionId }
      if nextSessionIds.isEmpty {
        return nil
      }
      if nextSessionIds.count == 1 {
        return .leaf(sessionId: nextSessionIds[0])
      }
      return .tabs(
        activeSessionId: activeSessionId.flatMap { nextSessionIds.contains($0) ? $0 : nil }
          ?? nextSessionIds[0],
        sessionIds: nextSessionIds)
    case .split(let direction, let ratio, let children):
      let nextChildren = children.compactMap { prunedLayout(removing: sessionId, from: $0) }
      if nextChildren.count == 1 {
        return nextChildren[0]
      }
      return nextChildren.isEmpty
        ? nil : .split(direction: direction, ratio: ratio, children: nextChildren)
    }
  }
}

extension TerminalWorkspaceView: WKNavigationDelegate {
  func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
    if let sessionId = sessionId(for: webView) {
      completedWebPaneLoadSessionIds.remove(sessionId)
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.start", [
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.commit", [
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    let sessionId = sessionId(for: webView)
    if let sessionId {
      completedWebPaneLoadSessionIds.insert(sessionId)
    }
    updateWebPanePageMetadata(for: webView, reason: "navigationFinish")
    finishProjectEditorWebKitNavigation(for: webView, reason: "navigationFinish")
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.finish", [
      "sessionId": sessionId ?? NSNull(),
      "title": webView.title ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
    if let sessionId = sessionId(for: webView) {
      completedWebPaneLoadSessionIds.remove(sessionId)
    }
    failProjectEditorWebKitNavigation(for: webView, error: error, reason: "navigationFail")
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.fail", [
      "error": error.localizedDescription,
      "errorCode": (error as NSError).code,
      "errorDomain": (error as NSError).domain,
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  func webView(
    _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
    withError error: Error
  ) {
    if let sessionId = sessionId(for: webView) {
      completedWebPaneLoadSessionIds.remove(sessionId)
    }
    failProjectEditorWebKitNavigation(for: webView, error: error, reason: "provisionalNavigationFail")
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.provisionalFail", [
      "error": error.localizedDescription,
      "errorCode": (error as NSError).code,
      "errorDomain": (error as NSError).domain,
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }

  func webView(
    _ webView: WKWebView,
    decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
  ) {
    let requestedUrl = navigationAction.request.url
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.action", [
      "isMainFrame": navigationAction.targetFrame?.isMainFrame ?? false,
      "method": navigationAction.request.httpMethod ?? NSNull(),
      "navigationType": String(describing: navigationAction.navigationType),
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "targetFrameMissing": navigationAction.targetFrame == nil,
      "url": requestedUrl?.absoluteString ?? NSNull(),
    ])
    if navigationAction.targetFrame == nil,
      let requestedUrl,
      requestedUrl.scheme == "http" || requestedUrl.scheme == "https"
    {
      /**
       CDXC:BrowserPanes 2026-05-03-03:59
       Embedded browser panes are single-pane browsers. Links that ask WebKit
       for a new tab/window, including many search-result links, must retarget
       into the existing WKWebView because ghostex does not create overlay windows
       for browser-pane navigation.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.navigation.retargetBlank", [
        "sessionId": sessionId(for: webView) ?? NSNull(),
        "url": requestedUrl.absoluteString,
      ])
      webView.load(navigationAction.request)
      decisionHandler(.cancel)
      return
    }
    decisionHandler(.allow)
  }

  func webView(
    _ webView: WKWebView,
    decidePolicyFor navigationResponse: WKNavigationResponse,
    decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void
  ) {
    let httpResponse = navigationResponse.response as? HTTPURLResponse
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.navigation.response", [
      "isForMainFrame": navigationResponse.isForMainFrame,
      "mimeType": navigationResponse.response.mimeType ?? NSNull(),
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "statusCode": httpResponse?.statusCode ?? 0,
      "url": navigationResponse.response.url?.absoluteString ?? NSNull(),
    ])
    decisionHandler(.allow)
  }

  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.process.terminated", [
      "sessionId": sessionId(for: webView) ?? NSNull(),
      "url": webView.url?.absoluteString ?? NSNull(),
    ])
  }
}

extension TerminalWorkspaceView: WKUIDelegate {
  func webView(
    _ webView: WKWebView,
    createWebViewWith configuration: WKWebViewConfiguration,
    for navigationAction: WKNavigationAction,
    windowFeatures: WKWindowFeatures
  ) -> WKWebView? {
    if navigationAction.targetFrame == nil,
      let requestedUrl = navigationAction.request.url,
      requestedUrl.scheme == "http" || requestedUrl.scheme == "https"
    {
      /**
       CDXC:BrowserPanes 2026-05-03-03:59
       JavaScript/window-open navigations use WKUIDelegate instead of the
       normal committed navigation path. Keep them in the same embedded pane so
       user clicks remain in-layout and never require an external overlay.
       */
      NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.navigation.uiRetargetBlank", [
        "sessionId": sessionId(for: webView) ?? NSNull(),
        "url": requestedUrl.absoluteString,
      ])
      webView.load(navigationAction.request)
    }
    return nil
  }
}

private final class ProjectBeadsBridge: NSObject, WKScriptMessageHandler {
  static let messageHandlerName = "ghostexProjectBeads"

  weak var webView: WKWebView?

  private let onRequest: (ProjectBeadsBridgeRequest, WKWebView?) -> Void

  init(onRequest: @escaping (ProjectBeadsBridgeRequest, WKWebView?) -> Void) {
    self.onRequest = onRequest
  }

  func userContentController(
    _ userContentController: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    guard let dictionary = message.body as? [String: Any],
      JSONSerialization.isValidJSONObject(dictionary),
      let data = try? JSONSerialization.data(withJSONObject: dictionary),
      let request = try? JSONDecoder().decode(ProjectBeadsBridgeRequest.self, from: data)
    else {
      return
    }
    onRequest(request, webView)
  }
}

private final class ProjectBoardBridge: NSObject, WKScriptMessageHandler {
  static let messageHandlerName = "ghostexProjectBoard"

  private let onRequest: (ProjectBoardBridgeRequest) -> Void

  init(onRequest: @escaping (ProjectBoardBridgeRequest) -> Void) {
    self.onRequest = onRequest
  }

  func userContentController(
    _ userContentController: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    guard let dictionary = message.body as? [String: Any],
      JSONSerialization.isValidJSONObject(dictionary),
      let data = try? JSONSerialization.data(withJSONObject: dictionary),
      let request = try? JSONDecoder().decode(ProjectBoardBridgeRequest.self, from: data)
    else {
      return
    }
    onRequest(request)
  }
}

private final class T3CodePaneDiagnosticsBridge: NSObject, WKScriptMessageHandler {
  static let messageHandlerName = "ghostexT3CodePaneDiagnostics"

  private let onThreadChanged: (String, String, String?) -> Void
  private let sessionId: String

  init(sessionId: String, onThreadChanged: @escaping (String, String, String?) -> Void) {
    self.onThreadChanged = onThreadChanged
    self.sessionId = sessionId
  }

  func userContentController(
    _ userContentController: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    var details = normalizeBody(message.body)
    let type = (details["type"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
    details["frameInfoIsMainFrame"] = message.frameInfo.isMainFrame
    details["sessionId"] = sessionId
    if type == "thread-changed", message.frameInfo.isMainFrame {
      let threadId = (details["threadId"] as? String)?
        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
      let title = (details["title"] as? String)?
        .trimmingCharacters(in: .whitespacesAndNewlines)
      if !threadId.isEmpty {
        onThreadChanged(sessionId, threadId, title?.isEmpty == false ? title : nil)
      }
    }
    NativeT3CodePaneReproLog.append(
      "nativeWorkspace.t3WebPane.javascript.\(type?.isEmpty == false ? type! : "message")",
      details
    )
  }

  private func normalizeBody(_ body: Any) -> [String: Any] {
    if let dictionary = body as? [String: Any] {
      return dictionary.reduce(into: [String: Any]()) { result, entry in
        result[entry.key] = normalizeValue(entry.value)
      }
    }
    return ["body": String(describing: body)]
  }

  private func normalizeValue(_ value: Any) -> Any {
    if value is NSNull {
      return NSNull()
    }
    if let string = value as? String {
      return string
    }
    if let number = value as? NSNumber {
      return number
    }
    if let bool = value as? Bool {
      return bool
    }
    if let array = value as? [Any] {
      return array.map(normalizeValue)
    }
    if let dictionary = value as? [String: Any] {
      return dictionary.reduce(into: [String: Any]()) { result, entry in
        result[entry.key] = normalizeValue(entry.value)
      }
    }
    return String(describing: value)
  }
}

private enum NativeSessionPersistenceProvider: String {
  case tmux
  case zmx
  case zellij

  static func resolve(_ command: CreateTerminal) -> NativeSessionPersistenceProvider? {
    if let provider = command.sessionPersistenceProvider,
      let resolvedProvider = NativeSessionPersistenceProvider(rawValue: provider)
    {
      return resolvedProvider
    }
    return command.tmuxMode == true ? .tmux : nil
  }
}

private enum NativeSessionPersistenceMode {
  typealias SessionExistsResult = (exists: Bool, error: String?)

  /**
   CDXC:AgentTerminalLifecycle 2026-05-25-16:26:
   Provider-backed terminals must always attach to ordinary tmux/zmx/zellij
   shells. The sidebar types agent, restore, and command startup text after
   terminalReady so exiting that process returns to the provider shell instead
   of terminating the native Ghostty surface.
   */
  private static let zellijSessionNameMaxLength = 25

  static func sessionExists(
    provider: NativeSessionPersistenceProvider,
    sessionName: String
  ) -> SessionExistsResult {
    let normalizedName = normalizedSessionName(sessionName, provider: provider) ?? sessionName
    let quotedName = shellQuote(normalizedName)
    let script: String
    switch provider {
    case .tmux:
      script = """
        if ! command -v tmux >/dev/null 2>&1; then
          printf '%s\\n' 'tmux was not found on PATH.' >&2
          exit 127
        fi
        tmux has-session -t \(quotedName) >/dev/null 2>&1
        """
    case .zmx:
      script = """
        unset ZMX_SESSION ZMX_SESSION_PREFIX
        \(zmxExecutableShellSetup())
        "$zmx_bin" list --short 2>/dev/null | grep -F -x -- \(quotedName) >/dev/null 2>&1
        """
    case .zellij:
      script = """
        if ! command -v zellij >/dev/null 2>&1; then
          printf '%s\\n' 'zellij was not found on PATH.' >&2
          exit 127
        fi
        zellij list-sessions --short --no-formatting 2>/dev/null | grep -F -x -- \(quotedName) >/dev/null 2>&1
        """
    }
    return runShellExistenceCheck(script)
  }

  private static func runShellExistenceCheck(_ script: String) -> SessionExistsResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = ["-lc", script]
    process.standardInput = FileHandle.nullDevice
    let stderrPipe = Pipe()
    process.standardError = stderrPipe
    do {
      try process.run()
      process.waitUntilExit()
      let stderr =
        String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
      if process.terminationStatus == 0 {
        return (true, nil)
      }
      if process.terminationStatus == 1 {
        return (false, nil)
      }
      return (false, stderr.isEmpty ? "exit-\(process.terminationStatus)" : stderr)
    } catch {
      return (false, error.localizedDescription)
    }
  }

  static func attachCommand(
    provider: NativeSessionPersistenceProvider,
    cwd: String,
    title: String?,
    sessionName: String
  ) -> String {
    switch provider {
    case .tmux:
      return tmuxAttachCommand(
        cwd: cwd,
        sessionName: sessionName)
    case .zmx:
      return zmxAttachCommand(
        cwd: cwd,
        title: title,
        sessionName: sessionName)
    case .zellij:
      return zellijAttachCommand(
        cwd: cwd,
        sessionName: sessionName)
    }
  }

  static func killSession(
    provider: NativeSessionPersistenceProvider?,
    sessionName: String?,
    reason: String,
    sessionId: String
  ) {
    guard let provider,
      let sessionName = normalizedSessionName(sessionName, provider: provider)
    else {
      return
    }
    let script = killCommand(provider: provider, sessionName: sessionName)
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = ["-lc", script]
    process.standardOutput = Pipe()
    process.standardError = Pipe()
    process.terminationHandler = { terminatedProcess in
      let stdout = stringFromPipe(terminatedProcess.standardOutput as? Pipe)
      let stderr = stringFromPipe(terminatedProcess.standardError as? Pipe)
      let eventName =
        terminatedProcess.terminationStatus == 0
        ? "nativeWorkspace.persistenceSessionKill.completed"
        : "nativeWorkspace.persistenceSessionKill.failed"
      DispatchQueue.main.async {
        TerminalFocusDebugLog.append(
          event: eventName,
          details: [
            "provider": provider.rawValue,
            "reason": reason,
            "sessionId": sessionId,
            "sessionName": sessionName,
            "stderr": stderr,
            "stdout": stdout,
            "terminationStatus": Int(terminatedProcess.terminationStatus),
          ])
      }
    }
    do {
      try process.run()
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.persistenceSessionKill.started",
        details: [
          "provider": provider.rawValue,
          "reason": reason,
          "sessionId": sessionId,
          "sessionName": sessionName,
        ])
    } catch {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.persistenceSessionKill.launchFailed",
        details: [
          "error": error.localizedDescription,
          "provider": provider.rawValue,
          "reason": reason,
          "sessionId": sessionId,
          "sessionName": sessionName,
        ])
    }
  }

  private static func killCommand(
    provider: NativeSessionPersistenceProvider,
    sessionName: String
  ) -> String {
    /**
     CDXC:SessionPersistence 2026-05-15-18:40:
     An explicit Ghostex sidebar close owns the full provider-backed session
     lifetime. Kill the named tmux/zmx/zellij session instead of merely closing
     the attached Ghostty client, while reload paths opt into preserving the
     provider session until the replacement terminal has attached.

     CDXC:SessionSleep 2026-05-17-01:33:
     Sleep also owns the provider runtime lifetime. It must terminate the named
     provider session so an idle sleeping agent CLI is not left running in the
     background and consuming memory.
     */
    let quotedName = shellQuote(sessionName)
    switch provider {
    case .tmux:
      return """
        if ! command -v tmux >/dev/null 2>&1; then
          printf '%s\\n' 'session persistence is set to tmux, but tmux was not found on PATH.' >&2
          exit 127
        fi
        exec tmux kill-session -t \(quotedName)
        """
    case .zmx:
      return """
        unset ZMX_SESSION ZMX_SESSION_PREFIX
        \(zmxExecutableShellSetup())
        exec "$zmx_bin" kill \(quotedName) --force
        """
    case .zellij:
      return """
        if ! command -v zellij >/dev/null 2>&1; then
          printf '%s\\n' 'session persistence is set to zellij, but zellij was not found on PATH.' >&2
          exit 127
        fi
        exec zellij delete-session --force \(quotedName)
        """
    }
  }

  private static func stringFromPipe(_ pipe: Pipe?) -> String {
    guard let pipe else {
      return ""
    }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    return String(data: data, encoding: .utf8)?
      .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
  }

  private static func tmuxAttachCommand(
    cwd: String,
    sessionName: String
  ) -> String {
    let noticeCommand = persistenceNoticeShellCommand(provider: .tmux, sessionName: sessionName)
    let script = """
      tmux_session=\(shellQuote(sessionName))
      tmux_cwd=\(shellQuote(cwd))
      tmux_notice_command=\(shellQuote(noticeCommand))
      tmux_created=0
      if ! command -v tmux >/dev/null 2>&1; then
        printf '%s\\n' 'tmux mode is enabled, but tmux was not found on PATH.'
        exit 127
      fi
      if ! tmux has-session -t "$tmux_session" 2>/dev/null; then
        tmux new-session -d -s "$tmux_session" -c "$tmux_cwd"
        tmux_created=1
      fi
      tmux set-option -t "$tmux_session" set-titles on >/dev/null
      tmux set-option -t "$tmux_session" set-titles-string '#T' >/dev/null
      if [ "$tmux_created" = "1" ]; then
        tmux send-keys -t "$tmux_session" -l "$tmux_notice_command"
        tmux send-keys -t "$tmux_session" Enter
      fi
      exec tmux attach-session -t "$tmux_session"
      """
    /**
     CDXC:TmuxMode 2026-05-05-06:06
     Ghostty accepts one command string. Run a small login-shell script that
     creates exactly one tmux session/pane for the sidebar terminal, configures
     tmux to forward the pane title to Ghostty, then execs attach so remote SSH
     clients can attach to the same named session.

     CDXC:TmuxMode 2026-05-05-06:31
     tmux startup must always create an ordinary shell pane. Sidebar-owned
     startup text is sent after terminalReady, which keeps agent exit from
     closing the pane process and still prevents restart attach from replaying
     commands.

     CDXC:SessionPersistence 2026-05-15-09:36
     New provider-backed sessions must announce their persistence manager at the
     top of the terminal. New tmux sessions receive the notice through
     send-keys; restart attach remains read-only.

     CDXC:CommandPanes 2026-05-20-22:52:
     tmux-backed command panes now share the same normal-shell startup path as
     agent panes so a completed command leaves the tmux pane attached.
     */
    return "/bin/zsh -lc \(shellQuote(script))"
  }

  private static func zmxAttachCommand(
    cwd: String,
    title: String?,
    sessionName: String
  ) -> String {
    let persistenceNoticeCommand = persistenceNoticeShellCommand(
      provider: .zmx,
      sessionName: sessionName)
    let titleNoticeCommand = sessionTitleShellCommand(title: title)
    let script = """
      zmx_session=\(shellQuote(sessionName))
      zmx_cwd=\(shellQuote(cwd))
      zmx_persistence_notice_command=\(shellQuote(persistenceNoticeCommand))
      zmx_title_notice_command=\(shellQuote(titleNoticeCommand))
      \(zmxExecutableShellSetup())
      unset ZMX_SESSION ZMX_SESSION_PREFIX
      if "$zmx_bin" list --short 2>/dev/null | grep -F -x -- "$zmx_session" >/dev/null 2>&1; then
        if [ -n "$zmx_title_notice_command" ]; then
          /bin/zsh -lc "$zmx_title_notice_command"
        fi
        exec "$zmx_bin" attach "$zmx_session"
      fi
      if [ -n "$zmx_persistence_notice_command" ]; then
        /bin/zsh -lc "$zmx_persistence_notice_command"
      fi
      cd "$zmx_cwd" || exit
      exec "$zmx_bin" attach "$zmx_session"
      """
    /**
     CDXC:SessionPersistence 2026-05-05-07:28
     zmx `attach` creates a missing session and attaches to an existing one.
     Terminals must use plain attach so the user sees a normal shell instead
     of zmx task wrapper text. The sidebar sends startup text after
     terminalReady only for the newly created native surface, so app restart
     attaches without replaying resume input into the live session.

     CDXC:SessionPersistence 2026-05-06-23:13
     Empty zmx-backed terminals must never create placeholder tasks such as
     `zmx run <name> /bin/zsh -lc :`. zmx surfaces task wrapper text such as
     `ZMX_TASK_COMPLETED` whenever a command is sent; direct attach creates the
     shell session without rendering a fake command or completion marker.

     CDXC:SessionPersistence 2026-05-06-23:31
     ghostex can itself be launched from inside zmx. Inherited ZMX_SESSION makes
     `zmx attach <target>` exit immediately, and inherited ZMX_SESSION_PREFIX
     rewrites app-managed names. Clear only those client/session variables so
     persistence still uses the user's zmx socket directory but attaches the
     exact sidebar session name.

     CDXC:SessionPersistence 2026-05-15-09:36
     A newly created zmx agent session must print a plain-language persistence
     notice before startup text is typed. Keep zmx terminals on direct attach
     so they stay normal shells without zmx task wrapper output.

     CDXC:SessionPersistence 2026-05-16-07:14:
     zmx startup should give Ghostty immediate context before the attach takes
     over: existing named sessions print the known sidebar title before attach,
     while new sessions print the persistence notice outside zmx before the
     sidebar sends one-shot startup text.
     */
    return "/bin/zsh -lc \(shellQuote(script))"
  }

  private static func zellijAttachCommand(
    cwd: String,
    sessionName: String
  ) -> String {
    let layout = zellijLayout(cwd: cwd)
    let script = """
      zellij_session=\(shellQuote(sessionName))
      if ! command -v zellij >/dev/null 2>&1; then
        printf '%s\\n' 'session persistence is set to zellij, but zellij was not found on PATH.'
        exit 127
      fi
      if zellij list-sessions --short --no-formatting 2>/dev/null | grep -F -x -- "$zellij_session" >/dev/null 2>&1; then
        exec zellij attach "$zellij_session"
      fi
      zellij_layout_file="$(mktemp "${TMPDIR:-/tmp}/ghostex-zellij-layout.XXXXXX")" || exit 1
      trap 'rm -f "$zellij_layout_file"' EXIT
      cat >"$zellij_layout_file" <<'GHOSTEX_ZELLIJ_LAYOUT'
      \(layout)
      GHOSTEX_ZELLIJ_LAYOUT
      zellij --session "$zellij_session" --new-session-with-layout "$zellij_layout_file"
      """
    /**
     CDXC:SessionPersistence 2026-05-06-03:43
     Zellij should match tmux/zmx UX: attach to a live named session when it
     exists, otherwise create the named session as a normal shell. The sidebar
     sends startup text after terminalReady so app restart never replays resume
     text into an already running session.

     CDXC:SessionPersistence 2026-05-06-22:16
     Zellij `--session --layout` does not create a missing session, and
     `attach --create` with a top-level `--layout` starts a generated-name
     session instead of the requested attach target on zellij 0.44. Write the
     generated layout to a real temporary file, then launch
     `zellij --session <name> --new-session-with-layout <file>` so new sessions
     are created under the same name that restart attach will later target.

     CDXC:SessionPersistence 2026-05-15-09:36
     New Zellij layouts must stay shell-only. Existing zellij sessions
     attach directly, and new startup commands are typed by the sidebar after
     the provider shell is ready.
     */
    return "/bin/zsh -lc \(shellQuote(script))"
  }

  static func renameTmuxSession(
    from currentName: String,
    sessionId: String,
    title: String,
    to nextName: String
  ) {
    DispatchQueue.global(qos: .utility).async {
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
      process.arguments = ["tmux", "rename-session", "-t", currentName, nextName]
      process.standardOutput = Pipe()
      process.standardError = Pipe()
      do {
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
          TerminalFocusDebugLog.append(
            event: "nativeWorkspace.tmux.renameSession.failed",
            details: [
              "currentName": currentName,
              "exitCode": process.terminationStatus,
              "nextName": nextName,
              "requestedSessionId": sessionId,
              "title": title,
            ])
        }
      } catch {
        TerminalFocusDebugLog.append(
          event: "nativeWorkspace.tmux.renameSession.failed",
          details: [
            "currentName": currentName,
            "error": error.localizedDescription,
            "nextName": nextName,
            "requestedSessionId": sessionId,
            "title": title,
          ])
      }
    }
  }

  static func sessionName(
    provider: NativeSessionPersistenceProvider,
    sessionId: String,
    title: String?
  ) -> String {
    if let compactName = compactSessionName(sessionId) {
      return compactName
    }

    guard provider == .zellij else {
      return sessionName(sessionId: sessionId, title: title)
    }

    let identitySlug = slug(sessionId) ?? "session"
    let identitySuffix = String(identitySlug.suffix(10))
    let titleSlug = slug(title) ?? "terminal"
    let maxTitleLength = max(
      1,
      zellijSessionNameMaxLength - "ghostex".count - identitySuffix.count - 2)
    let limitedTitleSlug = String(titleSlug.prefix(maxTitleLength)).trimmingCharacters(
      in: CharacterSet(charactersIn: "-_"))
    let visibleTitleSlug = limitedTitleSlug.isEmpty ? "terminal" : limitedTitleSlug
    return "ghostex-\(visibleTitleSlug)-\(identitySuffix)"
  }

  static func sessionName(sessionId: String, title: String?) -> String {
    if let compactName = compactSessionName(sessionId) {
      return compactName
    }

    let identitySlug = slug(sessionId) ?? "session"
    let identitySuffix = String(identitySlug.suffix(12))
    let titleSlug = slug(title) ?? "terminal"
    let limitedTitleSlug = String(titleSlug.prefix(48)).trimmingCharacters(
      in: CharacterSet(charactersIn: "-_"))
    let visibleTitleSlug = limitedTitleSlug.isEmpty ? "terminal" : limitedTitleSlug
    return "ghostex-\(visibleTitleSlug)-\(identitySuffix)"
  }

  static func compactSessionName(_ sessionId: String) -> String? {
    let trimmed = sessionId.trimmingCharacters(in: .whitespacesAndNewlines)
    /**
     CDXC:SessionPersistence 2026-05-15-17:33
     New sidebar session IDs already use the durable `g-MMDD-HHMMSS` identity.
     Reuse that exact value for default tmux, zmx, and zellij session names so
     provider attach targets, overlays, and persisted metadata stay short and
     consistent everywhere the session ID is exposed.

     CDXC:SessionPersistence 2026-05-15-17:49
     Native terminal surfaces receive project-scoped bridge IDs such as
     `project-id:g-0515-195810`, while the sidebar exposes `g-0515-195810` as
     the session number. Extract the trailing sidebar ID before provider-name
     generation so zmx/tmux/zellij labels match the visible session number.

     CDXC:SessionPersistence 2026-05-15-17:49
     The compact sidebar ID must outrank any stale title-derived provider name
     passed from restored sidebar state. This keeps the actual provider attach
     target and the visible Session number unified for new native surfaces.
     */
    let compactPattern = #"g-\d{4}-\d{6}$"#
    guard let range = trimmed.range(of: compactPattern, options: .regularExpression) else {
      return nil
    }
    let compactName = String(trimmed[range])
    guard trimmed == compactName || trimmed.hasSuffix(":\(compactName)") else {
      return nil
    }
    return compactName
  }

  static func normalizedSessionName(
    _ value: String?,
    provider: NativeSessionPersistenceProvider
  ) -> String? {
    let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else {
      return nil
    }
    /**
     CDXC:SessionPersistence 2026-05-05-07:28
     Persisted provider session names are trusted only when they match the
     app-generated target-safe shape. Corrupt or legacy missing names should
     regenerate from the terminal title instead of attaching to an ambiguous
     provider target.
     */
    guard trimmed.range(of: #"^[A-Za-z0-9_-]+$"#, options: .regularExpression) != nil else {
      return nil
    }
    if provider == .zellij && trimmed.count > zellijSessionNameMaxLength {
      /**
       CDXC:SessionPersistence 2026-05-06-22:30
       Zellij 0.44 rejects names at 29+ characters and launch checks did not
       reliably publish 26-28 character names. Keep zellij backing identities
       at 25 characters or less so create, list, and restart attach all target
       the same provider session instead of falling into generated-name
       sessions.
       */
      return nil
    }
    return trimmed
  }

  private static func slug(_ value: String?) -> String? {
    let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else {
      return nil
    }

    var result = ""
    var didAppendSeparator = false
    for scalar in trimmed.lowercased().unicodeScalars {
      if isAsciiAlphaNumeric(scalar) {
        result.unicodeScalars.append(scalar)
        didAppendSeparator = false
      } else if !didAppendSeparator {
        result.append("-")
        didAppendSeparator = true
      }
    }

    let normalized = result.trimmingCharacters(in: CharacterSet(charactersIn: "-_"))
    return normalized.isEmpty ? nil : normalized
  }

  private static func isAsciiAlphaNumeric(_ scalar: UnicodeScalar) -> Bool {
    (scalar.value >= 48 && scalar.value <= 57) ||
      (scalar.value >= 97 && scalar.value <= 122)
  }

  private static func persistenceNoticeShellCommand(
    provider: NativeSessionPersistenceProvider,
    sessionName: String
  ) -> String {
    "printf '%s\\n' \(shellQuote("This session is using \(provider.rawValue) persistence: \(sessionName)"))"
  }

  private static func sessionTitleShellCommand(title: String?) -> String {
    let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmedTitle.isEmpty else {
      return ""
    }
    return "printf '%s\\n' \(shellQuote(trimmedTitle))"
  }

  private static func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  private static func zmxExecutableShellSetup() -> String {
    let bundledZmxPath =
      Bundle.main.resourceURL?
      .appendingPathComponent("Web/bin/zmx", isDirectory: false)
      .path ?? ""
    /**
     CDXC:ZmxPersistence 2026-05-20-09:57:
     Ghostex-managed zmx sessions require the bundled zmx build because the pane
     refresh protocol is implemented by zmx itself. Do not fall back to an older
     PATH zmx here; using a binary without the refresh protocol can leak the
     private refresh sequence into the user's shell.
     */
    return """
      zmx_bin=\(shellQuote(bundledZmxPath))
      if [ ! -x "$zmx_bin" ]; then
        printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
        exit 127
      fi
      """
  }

  private static func zellijLayout(cwd: String) -> String {
    return """
      layout {
        cwd \(zellijKdlString(cwd))
        pane
      }
      """
  }

  private static func zellijKdlString(_ value: String) -> String {
    var result = "\""
    for scalar in value.unicodeScalars {
      switch scalar {
      case "\\":
        result += "\\\\"
      case "\"":
        result += "\\\""
      case "\n":
        result += "\\n"
      case "\r":
        result += "\\r"
      case "\t":
        result += "\\t"
      default:
        result.unicodeScalars.append(scalar)
      }
    }
    result += "\""
    return result
  }
}

private func normalizedTerminalSessionTitle(_ title: String?, sessionId: String) -> String {
  let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
  return trimmedTitle.isEmpty ? sessionId : trimmedTitle
}

private enum NativeTerminalProcessMonitor {
  /**
   CDXC:NativeTerminals 2026-04-29-09:16
   Closing a managed Ghostty surface should also clean up processes still bound
   to that terminal tty. This prevents agent helper trees from becoming
   launchd-owned orphans after the user closes or restores terminal sessions.
   */
  static func terminateSessionProcesses(ttyName: String?, foregroundPid: Int?, reason: String) {
    if let normalizedTtyName = normalizedTTYName(ttyName) {
      signalProcesses(attachedToTTY: normalizedTtyName, signal: "TERM", reason: reason)
      DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
        signalProcesses(attachedToTTY: normalizedTtyName, signal: "KILL", reason: reason)
      }
      return
    }

    guard let foregroundPid, foregroundPid > 1 else {
      return
    }
    _ = kill(pid_t(foregroundPid), SIGHUP)
    DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
      _ = kill(pid_t(foregroundPid), SIGTERM)
    }
  }

  private static func normalizedTTYName(_ ttyName: String?) -> String? {
    let trimmed = ttyName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else {
      return nil
    }
    return URL(fileURLWithPath: trimmed).lastPathComponent
  }

  private static func signalProcesses(attachedToTTY ttyName: String, signal: String, reason: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/pkill")
    process.arguments = ["-\(signal)", "-t", ttyName]
    process.standardOutput = Pipe()
    process.standardError = Pipe()
    do {
      try process.run()
    } catch {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.processMonitor.signalFailed",
        details: [
          "error": error.localizedDescription,
          "reason": reason,
          "signal": signal,
          "ttyName": ttyName,
        ])
    }
  }
}

private final class FloatingEditorOverlayView: NSView {
  let surfaceView: GhostexGhosttySurfaceView?
  let contentView: NSView
  let returnFocusSessionId: String?
  var closeHandler: (() -> Void)?
  var dragHandler: ((CGPoint) -> Void)?
  var resizeHandler: ((CGPoint) -> Void)?
  var saveHandler: (() -> Void)?
  var isUserPositioned = false

  private let titleBarView = FloatingEditorTitleBarView()
  private let titleLabel = NSTextField(labelWithString: "")
  private let closeButton = NSButton(title: "x", target: nil, action: nil)
  private let saveButton = NSButton(title: "Save", target: nil, action: nil)
  private let resizeHandleView = FloatingEditorResizeHandleView()

  init(title: String, returnFocusSessionId: String?, surfaceView: GhostexGhosttySurfaceView) {
    self.surfaceView = surfaceView
    self.returnFocusSessionId = returnFocusSessionId
    self.contentView = GhostexGhosttySurfaceHostView(surfaceView: surfaceView)
    super.init(frame: .zero)
    configure(title: title)
  }

  init(title: String, returnFocusSessionId: String?, webView: WKWebView) {
    self.surfaceView = nil
    self.returnFocusSessionId = returnFocusSessionId
    self.contentView = webView
    super.init(frame: .zero)
    configure(title: title)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  private func configure(title: String) {
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedWhite: 0.09, alpha: 0.98).cgColor
    layer?.cornerRadius = 8
    layer?.borderColor = NSColor(calibratedWhite: 1, alpha: 0.18).cgColor
    layer?.borderWidth = 1
    layer?.shadowColor = NSColor.black.cgColor
    layer?.shadowOpacity = 0.32
    layer?.shadowRadius = 18
    layer?.shadowOffset = CGSize(width: 0, height: -8)

    titleBarView.translatesAutoresizingMaskIntoConstraints = false
    titleBarView.wantsLayer = true
    titleBarView.layer?.backgroundColor = NSColor(calibratedWhite: 0.12, alpha: 1).cgColor
    titleBarView.dragHandler = { [weak self] delta in
      self?.dragHandler?(delta)
    }
    resizeHandleView.translatesAutoresizingMaskIntoConstraints = false
    resizeHandleView.resizeHandler = { [weak self] delta in
      self?.resizeHandler?(delta)
    }

    titleLabel.stringValue = title
    titleLabel.font = .systemFont(ofSize: 12, weight: .semibold)
    titleLabel.textColor = NSColor(calibratedWhite: 0.92, alpha: 1)
    titleLabel.lineBreakMode = .byTruncatingTail
    titleLabel.translatesAutoresizingMaskIntoConstraints = false

    closeButton.bezelStyle = .texturedRounded
    closeButton.font = .systemFont(ofSize: 11, weight: .semibold)
    closeButton.target = self
    closeButton.action = #selector(closeButtonPressed)
    closeButton.translatesAutoresizingMaskIntoConstraints = false

    saveButton.bezelStyle = .rounded
    saveButton.font = .systemFont(ofSize: 12, weight: .semibold)
    saveButton.target = self
    saveButton.action = #selector(saveButtonPressed)
    saveButton.translatesAutoresizingMaskIntoConstraints = false

    contentView.translatesAutoresizingMaskIntoConstraints = false

    addSubview(titleBarView)
    titleBarView.addSubview(titleLabel)
    titleBarView.addSubview(closeButton)
    addSubview(contentView)
    addSubview(saveButton)
    addSubview(resizeHandleView)

    NSLayoutConstraint.activate([
      titleBarView.leadingAnchor.constraint(equalTo: leadingAnchor),
      titleBarView.trailingAnchor.constraint(equalTo: trailingAnchor),
      titleBarView.topAnchor.constraint(equalTo: topAnchor),
      titleBarView.heightAnchor.constraint(equalToConstant: 32),

      closeButton.leadingAnchor.constraint(equalTo: titleBarView.leadingAnchor, constant: 8),
      closeButton.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),
      closeButton.widthAnchor.constraint(equalToConstant: 24),
      closeButton.heightAnchor.constraint(equalToConstant: 20),

      titleLabel.leadingAnchor.constraint(equalTo: closeButton.trailingAnchor, constant: 8),
      titleLabel.trailingAnchor.constraint(equalTo: titleBarView.trailingAnchor, constant: -12),
      titleLabel.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),

      contentView.leadingAnchor.constraint(equalTo: leadingAnchor),
      contentView.trailingAnchor.constraint(equalTo: trailingAnchor),
      contentView.topAnchor.constraint(equalTo: titleBarView.bottomAnchor),
      contentView.bottomAnchor.constraint(equalTo: bottomAnchor),

      saveButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -30),
      saveButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
      saveButton.widthAnchor.constraint(equalToConstant: 76),
      saveButton.heightAnchor.constraint(equalToConstant: 28),

      resizeHandleView.trailingAnchor.constraint(equalTo: trailingAnchor),
      resizeHandleView.bottomAnchor.constraint(equalTo: bottomAnchor),
      resizeHandleView.widthAnchor.constraint(equalToConstant: 22),
      resizeHandleView.heightAnchor.constraint(equalToConstant: 22),
    ])
  }

  @objc private func closeButtonPressed() {
    closeHandler?()
  }

  @objc private func saveButtonPressed() {
    saveHandler?()
  }

  func setSaving() {
    saveButton.title = "Saving"
    saveButton.isEnabled = false
  }

  func resetSaveButton() {
    saveButton.title = "Save"
    saveButton.isEnabled = true
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }
}

private final class FloatingEditorTitleBarView: NSView {
  var dragHandler: ((CGPoint) -> Void)?
  private var lastDragWindowPoint: CGPoint?

  override func mouseDown(with event: NSEvent) {
    lastDragWindowPoint = event.locationInWindow
  }

  override func mouseDragged(with event: NSEvent) {
    let point = event.locationInWindow
    if let lastDragWindowPoint {
      dragHandler?(CGPoint(x: point.x - lastDragWindowPoint.x, y: point.y - lastDragWindowPoint.y))
    }
    lastDragWindowPoint = point
  }

  override func mouseUp(with event: NSEvent) {
    lastDragWindowPoint = nil
  }

  override func resetCursorRects() {
    addCursorRect(bounds, cursor: .openHand)
  }
}

private final class FloatingEditorResizeHandleView: NSView {
  var resizeHandler: ((CGPoint) -> Void)?
  private var lastDragWindowPoint: CGPoint?

  override var isOpaque: Bool {
    false
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    let color = NSColor(calibratedWhite: 1, alpha: 0.34)
    color.setStroke()
    let path = NSBezierPath()
    path.lineWidth = 1
    for offset in stride(from: CGFloat(6), through: CGFloat(14), by: CGFloat(4)) {
      path.move(to: CGPoint(x: bounds.maxX - offset, y: 4))
      path.line(to: CGPoint(x: bounds.maxX - 4, y: offset))
    }
    path.stroke()
  }

  override func mouseDown(with event: NSEvent) {
    lastDragWindowPoint = event.locationInWindow
  }

  override func mouseDragged(with event: NSEvent) {
    let point = event.locationInWindow
    if let lastDragWindowPoint {
      resizeHandler?(CGPoint(x: point.x - lastDragWindowPoint.x, y: point.y - lastDragWindowPoint.y))
    }
    lastDragWindowPoint = point
  }

  override func mouseUp(with event: NSEvent) {
    lastDragWindowPoint = nil
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func resetCursorRects() {
    addCursorRect(bounds, cursor: .resizeLeftRight)
  }
}

final class GhostexGhosttySurfaceView: NSView {
  private static let zmxPersistenceRefreshSequence = "\u{001B}]1337;ZMX_REFRESH\u{0007}"
  let id: UUID
  var ghostexSessionId: String?
  var onKeyDownProbe: ((GhostexGhosttySurfaceView, NSEvent, String) -> Void)?
  var onMouseDownFocus: ((GhostexGhosttySurfaceView, NSEvent) -> Void)?
  var onTextInputProbe: ((GhostexGhosttySurfaceView, Any, NSRange) -> Void)?
  @Published private(set) var title = ""
  @Published var pwd: String?
  @Published private(set) var bell = false
  var onScrollbarChange: (() -> Void)?
  @Published var searchState: GhostexGhosttySearchState? {
    didSet {
      searchNeedleCancellable = nil
      appendSearchLog(
        event: "nativeWorkspace.terminalSearch.surfaceStateChanged",
        details: [
          "hadOldState": oldValue != nil,
          "hasNewState": searchState != nil,
          "needleLength": searchState?.needle.count ?? 0,
        ])
      if let searchState {
        searchNeedleCancellable = searchState.$needle
          .removeDuplicates()
          .sink { [weak self] needle in
            self?.appendSearchLog(
              event: "nativeWorkspace.terminalSearch.needlePublished",
              details: ["needleLength": needle.count])
            self?.performBindingAction("search:\(needle)")
          }
      } else if oldValue != nil {
        performBindingAction("end_search")
      }
    }
  }
  @Published var cellSize: NSSize = .zero
  @Published var surfaceSize: ghostty_surface_size_s?

  private(set) var surface: ghostty_surface_t?
  var scrollbar: Ghostty.Action.Scrollbar?
  var scrollbarConfiguration: Ghostty.Config.Scrollbar = .system
  var surfaceModel: GhostexGhosttySurfaceModel? {
    surface.map(GhostexGhosttySurfaceModel.init(surface:))
  }
  var focused = false
  private var processExitedOverride = false
  private var searchNeedleCancellable: AnyCancellable?
  private var markedText = ""
  private var markedTextRange = NSRange(location: NSNotFound, length: 0)
  private var selectedTextRange = NSRange(location: NSNotFound, length: 0)

  var processExited: Bool {
    if processExitedOverride {
      return true
    }
    guard let surface else { return true }
    return ghostty_surface_process_exited(surface)
  }

  var hasGhosttySurfaceForDiagnostics: Bool {
    surface != nil
  }

  init(_ app: ghostty_app_t, baseConfig: GhostexGhosttySurfaceConfiguration? = nil, uuid: UUID? = nil) {
    self.id = uuid ?? UUID()
    super.init(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
    wantsLayer = true
    layer?.backgroundColor = NSColor.black.cgColor
    let surfaceConfig = baseConfig ?? GhostexGhosttySurfaceConfiguration()
    /**
     CDXC:NativeTerminals 2026-05-11-14:01
     Direct Ghostty surfaces use the native ownership rule: the AppKit terminal
     view is the userdata and platform nsview for ghostty_surface_new. Runtime
     callbacks can therefore resolve actions to this view without wrapper
     SurfaceView lookup or recursive AppDelegate search.
     */
    surface = surfaceConfig.withCValue(view: self) { config in
      ghostty_surface_new(app, &config)
    }
    updateTrackingAreas()
    registerForDraggedTypes([.fileURL, .string])
    updateGhosttySurfaceSize()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  deinit {
    searchNeedleCancellable = nil
    if let surface {
      ghostty_surface_free(surface)
    }
  }

  override var acceptsFirstResponder: Bool { true }

  /**
   CDXC:NativeTerminals 2026-04-29-08:57
   Embedded Ghostty terminals should use the default pointer cursor instead
   of advertising a text-selection I-beam at all times. Keep this scoped to
   ghostex's SurfaceView subclass so Ghostty.app cursor behavior is unchanged.
   CDXC:NativePaneResize 2026-05-13-07:23
   Pane split rails are real AppKit divider bands with their own cursor rects.
   Do not register a full terminal-surface arrow cursor rect, because AppKit can
   re-apply that child cursor over a neighboring divider and make the pointer
   settle back to default on split lines.
   */
  override func resetCursorRects() {
    super.resetCursorRects()
  }

  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    updateGhosttySurfaceSize()
  }

  override func setFrameSize(_ newSize: NSSize) {
    super.setFrameSize(newSize)
    updateGhosttySurfaceSize()
  }

  override func viewDidChangeBackingProperties() {
    super.viewDidChangeBackingProperties()
    updateGhosttySurfaceSize()
  }

  override func becomeFirstResponder() -> Bool {
    let result = super.becomeFirstResponder()
    if result {
      focused = true
      if let surface {
        ghostty_surface_set_focus(surface, true)
      }
    }
    if searchState != nil {
      appendSearchLog(
        event: "nativeWorkspace.terminalSearch.surfaceBecomeFirstResponder",
        details: ["result": result])
    }
    return result
  }

  override func resignFirstResponder() -> Bool {
    let result = super.resignFirstResponder()
    if result {
      focused = false
      if let surface {
        ghostty_surface_set_focus(surface, false)
      }
    }
    if searchState != nil {
      appendSearchLog(
        event: "nativeWorkspace.terminalSearch.surfaceResignFirstResponder",
        details: ["result": result])
    }
    return result
  }

  override func performKeyEquivalent(with event: NSEvent) -> Bool {
    if handleCommandEditingKeyEquivalent(event) {
      return true
    }
    if handleGhostexSearchKeyEquivalent(event) {
      return true
    }
    return super.performKeyEquivalent(with: event)
  }

  /**
   CDXC:NativeTerminals 2026-04-29-08:53
   Once Cmd+F opens embedded Ghostty search, Escape should dismiss search
   before terminal programs receive the key. This mirrors normal find panels
   and keeps Escape from leaking into the shell while search is active.
   */
  override func keyDown(with event: NSEvent) {
    /**
     CDXC:NativeTerminalFocus 2026-05-11-11:48
     Wrong-pane typing must be diagnosed before changing focus behavior. Probe
     each terminal keyDown boundary with route metadata only, so the log shows
     whether this surface received a key while another pane owned the visible
     focus ring.
     */
    onKeyDownProbe?(self, event, "received")
    if event.keyCode == 53, searchState != nil {
      onKeyDownProbe?(self, event, "searchEscapeConsumed")
      searchState = nil
      return
    }
    sendKeyEvent(event, action: event.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS)
    onKeyDownProbe?(self, event, "forwarded")
  }

  override func keyUp(with event: NSEvent) {
    sendKeyEvent(event, action: GHOSTTY_ACTION_RELEASE, includeText: false)
  }

  override func flagsChanged(with event: NSEvent) {
    sendKeyEvent(event, action: GHOSTTY_ACTION_PRESS, includeText: false)
    /**
     CDXC:TerminalMouse 2026-05-23-00:24:
     gte path hover needs terminal mouse clients to see modifier-only changes
     while the pointer is already over a path. Forward the current mouse
     position on modifier transitions so Cmd/Ctrl immediately add or clear the
     clickable underline without waiting for the pointer to cross a new cell.
     */
    if let surface, ghostty_surface_mouse_captured(surface) {
      sendMousePosition(event)
    }
  }

  /**
   CDXC:NativeTerminals 2026-04-28-03:17
   Embedded Ghostty terminals must not paste text on middle click. Ghostty's
   default selection-clipboard behavior always maps middle-button events to
   paste, so ghostex consumes button 2 before the terminal core sees it.
   */
  override func otherMouseDown(with event: NSEvent) {
    if event.buttonNumber == 2 {
      return
    }
    sendMouseButton(event, action: GHOSTTY_MOUSE_PRESS, button: event.buttonNumber)
  }

  override func otherMouseUp(with event: NSEvent) {
    if event.buttonNumber == 2 {
      return
    }
    sendMouseButton(event, action: GHOSTTY_MOUSE_RELEASE, button: event.buttonNumber)
  }

  override func otherMouseDragged(with event: NSEvent) {
    if event.buttonNumber == 2 {
      return
    }
    sendMousePosition(event)
  }

  override func mouseDown(with event: NSEvent) {
    onMouseDownFocus?(self, event)
    window?.makeFirstResponder(self)
    sendMousePosition(event)
    sendMouseButton(event, action: GHOSTTY_MOUSE_PRESS, button: 0)
  }

  override func mouseUp(with event: NSEvent) {
    sendMousePosition(event)
    sendMouseButton(event, action: GHOSTTY_MOUSE_RELEASE, button: 0)
  }

  override func rightMouseDown(with event: NSEvent) {
    sendMousePosition(event)
    if !sendMouseButton(event, action: GHOSTTY_MOUSE_PRESS, button: 1) {
      super.rightMouseDown(with: event)
    }
  }

  override func rightMouseUp(with event: NSEvent) {
    sendMousePosition(event)
    if !sendMouseButton(event, action: GHOSTTY_MOUSE_RELEASE, button: 1) {
      super.rightMouseUp(with: event)
    }
  }

  override func mouseDragged(with event: NSEvent) {
    sendMousePosition(event)
  }

  override func rightMouseDragged(with event: NSEvent) {
    sendMousePosition(event)
  }

  override func mouseMoved(with event: NSEvent) {
    sendMousePosition(event)
  }

  override func scrollWheel(with event: NSEvent) {
    guard let surface else { return }
    let precision = event.hasPreciseScrollingDeltas
    /**
     CDXC:NativeTerminals 2026-05-14-09:24:
     Direct Ghostty runtime must preserve Ghostty.SurfaceView's scroll feel.
     Precise wheel devices use the same 2x delta multiplier and momentum bits
     as Ghostty's AppKit surface so trackpad inertia and scrollback movement do
     not regress when the terminal is embedded in ghostex.
     */
    let multiplier = precision ? 2.0 : 1.0
    let mods = Ghostty.Input.ScrollMods(
      precision: precision,
      momentum: Ghostty.Input.Momentum(event.momentumPhase)
    ).cScrollMods
    ghostty_surface_mouse_scroll(
      surface,
      event.scrollingDeltaX * multiplier,
      event.scrollingDeltaY * multiplier,
      mods)
  }

  /**
   CDXC:NativeTerminalContextMenu 2026-05-17-03:01:
   Embedded Ghostty surfaces need a native AppKit context menu on terminal body
   right-clicks, but ghostex should not expose Ghostty.app's split, reset,
   inspector, read-only, or title actions inside managed panes. Keep the surface
   menu limited to terminal clipboard commands and let Ghostty mouse reporting
   suppress the menu when a terminal application consumes right-click events.
   */
  override func menu(for event: NSEvent) -> NSMenu? {
    guard event.type == .rightMouseDown else {
      return nil
    }

    let menu = NSMenu()
    menu.autoenablesItems = false
    let copyItem = menu.addItem(withTitle: "Copy", action: #selector(copy(_:)), keyEquivalent: "")
    copyItem.target = self
    copyItem.isEnabled = hasSelection()

    let pasteItem = menu.addItem(withTitle: "Paste", action: #selector(paste(_:)), keyEquivalent: "")
    pasteItem.target = self
    return menu
  }

  @IBAction func copy(_ sender: Any?) {
    _ = performBindingAction("copy_to_clipboard")
  }

  @IBAction func paste(_ sender: Any?) {
    _ = performBindingAction("paste_from_clipboard")
  }

  /**
   CDXC:NativeTerminals 2026-04-28-05:13
   Embedded Ghostty surfaces do not use Ghostty's SwiftUI terminal wrapper or
   app main menu, so search shortcuts must be handled at the surface level and
   routed to Ghostty's native search actions.
   */
  private func handleGhostexSearchKeyEquivalent(_ event: NSEvent) -> Bool {
    guard event.type == .keyDown, focused else {
      return false
    }
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command), flags.isDisjoint(with: [.control, .option]) else {
      return false
    }
    switch event.charactersIgnoringModifiers?.lowercased() {
    case "f":
      appendSearchLog(
        event: "nativeWorkspace.terminalSearch.keyEquivalent",
        details: [
          "action": "start_search",
          "keyCode": Int(event.keyCode),
          "modifierFlags": Self.searchDebugModifierNames(event.modifierFlags),
        ])
      performBindingAction("start_search")
      return true
    case "g":
      appendSearchLog(
        event: "nativeWorkspace.terminalSearch.keyEquivalent",
        details: [
          "action": flags.contains(.shift) ? "goto_previous_match" : "goto_next_match",
          "keyCode": Int(event.keyCode),
          "modifierFlags": Self.searchDebugModifierNames(event.modifierFlags),
        ])
      if flags.contains(.shift) {
        _ = navigateSearchToPrevious()
      } else {
        _ = navigateSearchToNext()
      }
      return true
    default:
      return false
    }
  }

  /**
   CDXC:NativeTerminals 2026-05-11-05:24
   AppKit's Select All menu key equivalent claims Cmd-A before embedded
   Ghostty can encode it for terminal applications. When a terminal pane is
   focused, Cmd-A must reach the child program as Command/Super, while Cmd-Left
   remains on the terminal input path. Keep non-terminal text fields on the
   normal AppKit Edit menu path.

   CDXC:TerminalPromptEditing 2026-05-23-01:51:
   Cmd+G in focused Ghostty terminal panes must send Ctrl+G to child TUIs so
   Claude Code/Codex can open EDITOR and gte can save from the same shortcut.
   Cmd+F still opens Ghostty search, and Cmd+G search navigation remains scoped
   to TerminalSearchTextField when the search field owns first responder.

   CDXC:TerminalPromptEditing 2026-05-23-02:10:
   gte is the default terminal editor, so focused Ghostty panes must forward the
   main Mac editing shortcuts as Super-modified CSI-u sequences instead of
   letting AppKit's menu equivalents swallow them. Preserve native terminal-copy
   behavior when Ghostty owns an active selection, then send Cmd+C through to gte
   when the selection is inside the TUI itself.
   */
  private func handleCommandEditingKeyEquivalent(_ event: NSEvent) -> Bool {
    guard event.type == .keyDown, focused else {
      return false
    }
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command), flags.isDisjoint(with: [.control, .option]) else {
      return false
    }

    let key = event.charactersIgnoringModifiers?.lowercased()

    if flags.isDisjoint(with: [.shift]), key == "g" {
      return sendTerminalInput(Self.controlGSequence, label: "cmd-g-as-ctrl-g")
    }

    if flags.isDisjoint(with: [.shift]), key == "c", hasSelection() {
      return false
    }

    if flags.isDisjoint(with: [.shift]), let key {
      switch key {
      case "a", "c", "s", "y", "z":
        return sendTerminalInput(Self.commandSequence(for: key), label: "cmd-\(key)-as-super")
      default:
        break
      }
    }

    if flags.contains(.shift), key == "z" {
      return sendTerminalInput(Self.commandShiftZSequence, label: "cmd-shift-z-as-super-shift")
    }

    return false
  }

  private func sendTerminalInput(_ sequence: String, label: String) -> Bool {
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.commandEditingSequenceSent",
      details: ["label": label])
    insertText(sequence, replacementRange: NSRange(location: NSNotFound, length: 0))
    return true
  }

  @discardableResult
  func navigateSearchToNext() -> Bool {
    /**
     CDXC:NativeTerminalSearch 2026-05-20-22:46:
     Ghostty search navigation is a parameterized `navigate_search` binding.
     Earlier `goto_*_match` strings are not valid Ghostty actions, so the
     embedded search bar could receive Return, arrow, or button events and
     still no-op because the binding API returned false.
     */
    let action = "navigate_search:next"
    let didNavigate = performBindingAction(action)
    appendSearchLog(
      event: "nativeWorkspace.terminalSearch.navigateNext",
      details: [
        "action": action,
        "didNavigate": didNavigate,
      ])
    return didNavigate
  }

  @discardableResult
  func navigateSearchToPrevious() -> Bool {
    let action = "navigate_search:previous"
    let didNavigate = performBindingAction(action)
    appendSearchLog(
      event: "nativeWorkspace.terminalSearch.navigatePrevious",
      details: [
        "action": action,
        "didNavigate": didNavigate,
      ])
    return didNavigate
  }

  func setTerminalTitle(_ titlePointer: UnsafePointer<CChar>?) {
    guard let titlePointer else { return }
    title = String(cString: titlePointer)
  }

  func ringBell() {
    bell = true
    DispatchQueue.main.async { [weak self] in
      self?.bell = false
    }
  }

  func setCellSize(width: UInt32, height: UInt32) {
    cellSize = convertFromBacking(NSSize(width: Double(width), height: Double(height)))
  }

  func startSearch(_ action: ghostty_action_start_search_s) {
    let needle = action.needle.map { String(cString: $0) } ?? ""
    appendSearchLog(
      event: "nativeWorkspace.terminalSearch.startSearch",
      details: [
        "incomingNeedleLength": needle.count,
        "wasActive": searchState != nil,
      ])
    if let searchState {
      if !needle.isEmpty {
        searchState.needle = needle
      }
    } else {
      searchState = GhostexGhosttySearchState(needle: needle)
    }
    appendSearchLog(event: "nativeWorkspace.terminalSearch.focusNotificationPosted")
    NotificationCenter.default.post(name: .ghosttySearchFocus, object: self)
  }

  func endSearch() {
    appendSearchLog(event: "nativeWorkspace.terminalSearch.endSearch")
    searchState = nil
  }

  func setSearchTotal(_ total: Int) {
    appendSearchLog(
      event: "nativeWorkspace.terminalSearch.totalUpdated",
      details: ["total": total])
    searchState?.total = total >= 0 ? UInt(total) : nil
  }

  func setSearchSelected(_ selected: Int) {
    appendSearchLog(
      event: "nativeWorkspace.terminalSearch.selectedUpdated",
      details: ["selected": selected])
    searchState?.selected = selected >= 0 ? UInt(selected) : nil
  }

  func setScrollbar(_ nextScrollbar: Ghostty.Action.Scrollbar) {
    scrollbar = nextScrollbar
    onScrollbarChange?()
  }

  func markProcessExited() {
    processExitedOverride = true
  }

  @discardableResult
  private func performBindingAction(_ action: String) -> Bool {
    guard let surface else { return false }
    return ghostty_surface_binding_action(surface, action, UInt(action.lengthOfBytes(using: .utf8)))
  }

  func refreshZmxPersistenceViewport(reason: String) {
    /**
     CDXC:ZmxPersistence 2026-05-18-07:20:
     zmx session switches and settled pane resizes need a visible terminal
     refresh after AppKit surfaces the command-pane view.

     CDXC:ZmxPersistence 2026-05-20-09:57:
     Ghostty redraw alone does not repair stale zmx pane content. Send zmx's
     private refresh sequence to the attached zmx client so zmx consumes it and
     emits a display-only VT repaint from daemon state; the sequence must never
     be forwarded as PTY input to the user's shell.
     */
    guard surface != nil else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.zmxPersistenceViewportRefresh.surface",
        details: [
          "didRefresh": false,
          "reason": reason,
          "sessionId": ghostexSessionId ?? "",
          "skipReason": "missingSurface",
        ],
        force: true)
      return
    }
    guard window != nil, !isHidden, !bounds.isEmpty else {
      TerminalFocusDebugLog.append(
        event: "nativeWorkspace.zmxPersistenceViewportRefresh.surface",
        details: [
          "boundsHeight": bounds.height,
          "boundsWidth": bounds.width,
          "didRefresh": false,
          "isHidden": isHidden,
          "reason": reason,
          "sessionId": ghostexSessionId ?? "",
          "skipReason": window == nil ? "missingWindow" : "hiddenOrEmptyBounds",
        ],
        force: true)
      return
    }

    insertText(Self.zmxPersistenceRefreshSequence, replacementRange: NSRange(location: NSNotFound, length: 0))
    TerminalFocusDebugLog.append(
      event: "nativeWorkspace.zmxPersistenceViewportRefresh.surface",
      details: [
        "didRefresh": true,
        "reason": reason,
        "sessionId": ghostexSessionId ?? "",
      ],
      force: true)
  }

  private func updateGhosttySurfaceSize() {
    guard let surface else { return }
    let scale = Double(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2)
    /**
     CDXC:SessionRestore 2026-05-14-10:09
     Restored Ghostty panes can be created before AppKit has attached their
     backing layer to a Retina window. Keep the layer contents scale in sync
     with the scale sent to Ghostty so restored surfaces do not render a 1x
     texture that AppKit composites as oversized terminal text.
     */
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer?.contentsScale = CGFloat(scale)
    CATransaction.commit()
    ghostty_surface_set_content_scale(surface, scale, scale)
    let backingSize = convertToBacking(bounds).size
    let width = max(UInt32(floor(backingSize.width)), 1)
    let height = max(UInt32(floor(backingSize.height)), 1)
    ghostty_surface_set_size(surface, width, height)
    surfaceSize = ghostty_surface_size(surface)
    if let screen = window?.screen,
      let displayID = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? UInt32
    {
      ghostty_surface_set_display_id(surface, displayID)
    }
  }

  private func sendKeyEvent(
    _ event: NSEvent,
    action: ghostty_input_action_e,
    includeText: Bool = true
  ) {
    guard let surface else { return }
    var keyEvent = ghostty_input_key_s()
    keyEvent.action = action
    keyEvent.keycode = UInt32(event.keyCode)
    keyEvent.mods = mods(from: event)
    keyEvent.consumed_mods = GHOSTTY_MODS_NONE
    keyEvent.composing = hasMarkedText()
    keyEvent.unshifted_codepoint = unshiftedCodepoint(from: event)
    if includeText, let text = ghosttyText(from: event), !text.isEmpty {
      text.withCString { ptr in
        keyEvent.text = ptr
        _ = ghostty_surface_key(surface, keyEvent)
      }
    } else {
      keyEvent.text = nil
      _ = ghostty_surface_key(surface, keyEvent)
    }
  }

  private func ghosttyText(from event: NSEvent) -> String? {
    /**
     CDXC:NativeTerminals 2026-05-11-18:07
     AppKit represents arrow/function keys as private-use Unicode scalars
     such as U+F700..U+F703. Those values are key identifiers, not terminal
     text. Passing them through keyEvent.text makes CLI TUIs render glyphs
     instead of receiving Ghostty's encoded arrow-key sequence, so suppress
     private-use key text and let keycode/modifiers drive Ghostty encoding.
     */
    guard let text = event.characters, !text.isEmpty else {
      return nil
    }

    if text.count == 1, let scalar = text.unicodeScalars.first {
      if scalar.value < 0x20 {
        return event.characters(byApplyingModifiers: event.modifierFlags.subtracting(.control))
      }

      if (0xF700 ... 0xF8FF).contains(scalar.value) {
        TerminalFocusDebugLog.append(
          event: "nativeWorkspace.ghosttyKeyText.suppressedPrivateUse",
          details: [
            "charactersLength": text.count,
            "keyCode": Int(event.keyCode),
            "modifierFlagsRaw": event.modifierFlags.rawValue,
            "scalar": String(format: "U+%04X", scalar.value),
          ])
        return nil
      }
    }

    return text
  }

  private func sendMousePosition(_ event: NSEvent) {
    guard let surface else { return }
    let point = ghosttyMousePoint(from: event)
    ghostty_surface_mouse_pos(surface, point.x, point.y, mods(from: event))
  }

  @discardableResult
  private func sendMouseButton(
    _ event: NSEvent,
    action: ghostty_input_mouse_state_e,
    button: Int
  ) -> Bool {
    guard let surface else { return false }
    let ghosttyButton: ghostty_input_mouse_button_e =
      button == 1 ? GHOSTTY_MOUSE_RIGHT : button == 2 ? GHOSTTY_MOUSE_MIDDLE : GHOSTTY_MOUSE_LEFT
    return ghostty_surface_mouse_button(surface, action, ghosttyButton, mods(from: event))
  }

  private func hasSelection() -> Bool {
    guard let surface else { return false }
    return ghostty_surface_has_selection(surface)
  }

  private func ghosttyMousePoint(from event: NSEvent) -> NSPoint {
    let local = convert(event.locationInWindow, from: nil)
    return NSPoint(x: local.x, y: bounds.height - local.y)
  }

  private func mods(from event: NSEvent) -> ghostty_input_mods_e {
    var mods = GHOSTTY_MODS_NONE.rawValue
    let flags = event.modifierFlags
    if flags.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
    if flags.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
    if flags.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
    if flags.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
    if flags.contains(.capsLock) { mods |= GHOSTTY_MODS_CAPS.rawValue }
    return ghostty_input_mods_e(rawValue: mods)
  }

  private func appendSearchLog(event: String, details: [String: Any] = [:]) {
    var payload = details
    let firstResponder = window?.firstResponder
    payload["firstResponderClass"] = firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
    payload["firstResponderIsSurface"] = firstResponder === self
    payload["focused"] = focused
    payload["hasSurface"] = surface != nil
    payload["isHidden"] = isHidden
    payload["needleLength"] = searchState?.needle.count ?? 0
    payload["sessionId"] = ghostexSessionId ?? ""
    payload["windowIsKey"] = window?.isKeyWindow ?? false
    payload["windowNumber"] = window?.windowNumber ?? 0
    /**
     CDXC:NativeTerminalSearch 2026-05-19-09:02:
     Embedded Ghostty search failures can happen before the floating AppKit
     search bar receives input. Surface-level logs mark the command/action
     boundary and responder state without recording the search query itself.
     */
    TerminalFocusDebugLog.append(event: event, details: payload)
  }

  static func searchDebugModifierNames(_ flags: NSEvent.ModifierFlags) -> [String] {
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

  private func unshiftedCodepoint(from event: NSEvent) -> UInt32 {
    /**
     CDXC:NativeTerminals 2026-05-11-17:36
     Modifier-only flagsChanged events carry key state but no printable text.
     AppKit raises an exception if charactersIgnoringModifiers is read from
     those events, so only keyDown/keyUp events may contribute an unshifted
     Ghostty codepoint.
     */
    guard event.type == .keyDown || event.type == .keyUp else {
      return 0
    }
    guard let text = event.charactersIgnoringModifiers, let scalar = text.unicodeScalars.first else {
      return 0
    }
    return scalar.value
  }

  private static func commandSequence(for key: String) -> String {
    guard let scalar = key.unicodeScalars.first else {
      return ""
    }
    return "\u{1B}[\(scalar.value);9u"
  }

  private static let commandShiftZSequence = "\u{1B}[122;10u"
  private static let controlGSequence = "\u{07}"

}

extension GhostexGhosttySurfaceView: NSTextInputClient {
  func insertText(_ string: Any, replacementRange: NSRange) {
    onTextInputProbe?(self, string, replacementRange)
    let text = (string as? String) ?? (string as? NSAttributedString)?.string ?? ""
    guard let surface, !text.isEmpty else { return }
    markedText = ""
    markedTextRange = NSRange(location: NSNotFound, length: 0)
    text.withCString { ptr in
      var keyEvent = ghostty_input_key_s()
      keyEvent.action = GHOSTTY_ACTION_PRESS
      keyEvent.keycode = 0
      keyEvent.mods = GHOSTTY_MODS_NONE
      keyEvent.consumed_mods = GHOSTTY_MODS_NONE
      keyEvent.composing = false
      keyEvent.text = ptr
      keyEvent.unshifted_codepoint = 0
      _ = ghostty_surface_key(surface, keyEvent)
    }
  }

  func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
    markedText = (string as? String) ?? (string as? NSAttributedString)?.string ?? ""
    markedTextRange =
      markedText.isEmpty
      ? NSRange(location: NSNotFound, length: 0)
      : NSRange(location: 0, length: markedText.count)
    selectedTextRange = selectedRange
    guard let surface else { return }
    if markedText.isEmpty {
      ghostty_surface_preedit(surface, nil, 0)
    } else {
      markedText.withCString { ptr in
        ghostty_surface_preedit(surface, ptr, UInt(markedText.utf8.count))
      }
    }
  }

  func unmarkText() {
    markedText = ""
    markedTextRange = NSRange(location: NSNotFound, length: 0)
    if let surface {
      ghostty_surface_preedit(surface, nil, 0)
    }
  }

  func selectedRange() -> NSRange { selectedTextRange }

  func markedRange() -> NSRange { markedTextRange }

  func hasMarkedText() -> Bool { markedTextRange.location != NSNotFound }

  func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
    nil
  }

  func validAttributesForMarkedText() -> [NSAttributedString.Key] {
    [.underlineStyle, .backgroundColor]
  }

  func characterIndex(for point: NSPoint) -> Int {
    NSNotFound
  }

  func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
    guard let surface else { return .zero }
    var x: Double = 0
    var y: Double = 0
    var width: Double = 0
    var height: Double = 0
    ghostty_surface_ime_point(surface, &x, &y, &width, &height)
    let viewPoint = NSPoint(x: x, y: bounds.height - y)
    let screenPoint = window?.convertPoint(toScreen: convert(viewPoint, to: nil)) ?? viewPoint
    return NSRect(x: screenPoint.x, y: screenPoint.y - height, width: width, height: height)
  }
}

private final class BrowserAddressTextFieldCell: NSTextFieldCell {
  private static let verticalTextOffset: CGFloat = 1.5

  override func drawingRect(forBounds rect: NSRect) -> NSRect {
    adjustedTextFrame(super.drawingRect(forBounds: rect))
  }

  override func edit(
    withFrame rect: NSRect,
    in controlView: NSView,
    editor textObj: NSText,
    delegate: Any?,
    event: NSEvent?
  ) {
    super.edit(
      withFrame: adjustedTextFrame(rect),
      in: controlView,
      editor: textObj,
      delegate: delegate,
      event: event)
  }

  override func select(
    withFrame rect: NSRect,
    in controlView: NSView,
    editor textObj: NSText,
    delegate: Any?,
    start selStart: Int,
    length selLength: Int
  ) {
    super.select(
      withFrame: adjustedTextFrame(rect),
      in: controlView,
      editor: textObj,
      delegate: delegate,
      start: selStart,
      length: selLength)
  }

  private func adjustedTextFrame(_ frame: NSRect) -> NSRect {
    var nextFrame = frame
    /**
     CDXC:BrowserPanes 2026-05-03-02:08
     The browser address field frame aligns with toolbar controls, but AppKit
     draws the text slightly high. Offset only the cell text/edit rect down two
     pixels in AppKit field coordinates so the surrounding toolbar layout
     remains unchanged.
     */
    nextFrame.origin.y += Self.verticalTextOffset
    return nextFrame
  }
}

private final class TerminalSearchTextFieldCell: NSTextFieldCell {
  private static let verticalTextOffset: CGFloat = 3

  override func drawingRect(forBounds rect: NSRect) -> NSRect {
    adjustedTextFrame(super.drawingRect(forBounds: rect))
  }

  override func edit(
    withFrame rect: NSRect,
    in controlView: NSView,
    editor textObj: NSText,
    delegate: Any?,
    event: NSEvent?
  ) {
    super.edit(
      withFrame: adjustedTextFrame(rect),
      in: controlView,
      editor: textObj,
      delegate: delegate,
      event: event)
  }

  override func select(
    withFrame rect: NSRect,
    in controlView: NSView,
    editor textObj: NSText,
    delegate: Any?,
    start selStart: Int,
    length selLength: Int
  ) {
    super.select(
      withFrame: adjustedTextFrame(rect),
      in: controlView,
      editor: textObj,
      delegate: delegate,
      start: selStart,
      length: selLength)
  }

  private func adjustedTextFrame(_ frame: NSRect) -> NSRect {
    var nextFrame = frame
    /**
     CDXC:NativeTerminals 2026-05-10-11:58
     Embedded Ghostty search text and placeholder must be visually centered in
     the search box. Offset only the field cell's text/edit rect down three
     pixels so icon button placement and the surrounding bar layout stay fixed.
     */
    nextFrame.origin.y += Self.verticalTextOffset
    return nextFrame
  }
}

private final class TerminalSearchCountLabelCell: NSTextFieldCell {
  private static let verticalTextOffset: CGFloat = 2

  override func drawingRect(forBounds rect: NSRect) -> NSRect {
    adjustedTextFrame(super.drawingRect(forBounds: rect))
  }

  private func adjustedTextFrame(_ frame: NSRect) -> NSRect {
    var nextFrame = frame
    /**
     CDXC:NativeTerminalSearch 2026-05-20-21:38:
     The search result counter shares the compact find-box row with text and
     icon controls, but AppKit label cells draw slightly high at this size.
     Shift only the counter text rect down so current/total reads vertically
     centered without changing button or input hit targets.
     */
    nextFrame.origin.y += Self.verticalTextOffset
    return nextFrame
  }
}

private final class TerminalSearchTextField: NSTextField {
  var onClose: (() -> Void)?
  var onDiagnostic: ((String, [String: Any]) -> Void)?
  var onFindNext: (() -> Void)?
  var onFindPrevious: (() -> Void)?
  var onFocusChanged: ((Bool) -> Void)?

  override var acceptsFirstResponder: Bool {
    true
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func performKeyEquivalent(with event: NSEvent) -> Bool {
    guard event.type == .keyDown else {
      return super.performKeyEquivalent(with: event)
    }
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command),
      flags.isDisjoint(with: [.control, .option])
    else {
      return super.performKeyEquivalent(with: event)
    }

    switch event.charactersIgnoringModifiers?.lowercased() {
    case "a":
      onDiagnostic?(
        "nativeWorkspace.terminalSearch.textFieldKeyEquivalent",
        [
          "action": "selectAll",
          "keyCode": Int(event.keyCode),
          "modifierFlags": GhostexGhosttySurfaceView.searchDebugModifierNames(event.modifierFlags),
        ])
      selectSearchText()
      return true
    case "g":
      onDiagnostic?(
        "nativeWorkspace.terminalSearch.textFieldKeyEquivalent",
        [
          "action": flags.contains(.shift) ? "findPrevious" : "findNext",
          "keyCode": Int(event.keyCode),
          "modifierFlags": GhostexGhosttySurfaceView.searchDebugModifierNames(event.modifierFlags),
        ])
      if flags.contains(.shift) {
        onFindPrevious?()
      } else {
        onFindNext?()
      }
      return true
    default:
      return super.performKeyEquivalent(with: event)
    }
  }

  override func keyDown(with event: NSEvent) {
    onDiagnostic?(
      "nativeWorkspace.terminalSearch.textFieldKeyDown",
      [
        "charactersIgnoringModifiersLength": event.charactersIgnoringModifiers?.count ?? 0,
        "charactersLength": event.characters?.count ?? 0,
        "keyCode": Int(event.keyCode),
        "modifierFlags": GhostexGhosttySurfaceView.searchDebugModifierNames(event.modifierFlags),
      ])
    let navigationFlags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    if event.keyCode == 53, navigationFlags.isEmpty {
      onClose?()
      return
    }
    if event.keyCode == 36 || event.keyCode == 76 {
      guard navigationFlags.isDisjoint(with: [.command, .control, .option]) else {
        super.keyDown(with: event)
        return
      }
      if navigationFlags.contains(.shift) {
        onFindPrevious?()
      } else {
        onFindNext?()
      }
      return
    }
    if event.keyCode == 126, navigationFlags.isEmpty {
      onFindPrevious?()
      return
    }
    if event.keyCode == 125, navigationFlags.isEmpty {
      onFindNext?()
      return
    }
    super.keyDown(with: event)
  }

  override func mouseDown(with event: NSEvent) {
    onDiagnostic?(
      "nativeWorkspace.terminalSearch.textFieldMouseDown",
      [
        "clickCount": event.clickCount,
        "eventWindowNumber": event.window?.windowNumber ?? 0,
        "localPoint": Self.describeLogPoint(convert(event.locationInWindow, from: nil)),
      ])
    super.mouseDown(with: event)
  }

  override func becomeFirstResponder() -> Bool {
    let result = super.becomeFirstResponder()
    onDiagnostic?(
      "nativeWorkspace.terminalSearch.textFieldBecomeFirstResponder",
      ["result": result])
    if result {
      onFocusChanged?(true)
    }
    return result
  }

  override func resignFirstResponder() -> Bool {
    let result = super.resignFirstResponder()
    onDiagnostic?(
      "nativeWorkspace.terminalSearch.textFieldResignFirstResponder",
      ["result": result])
    if result {
      onFocusChanged?(false)
    }
    return result
  }

  override func cancelOperation(_ sender: Any?) {
    onDiagnostic?("nativeWorkspace.terminalSearch.textFieldCancelOperation", [:])
    onClose?()
  }

  private func selectSearchText() {
    if let editor = currentEditor() {
      editor.selectAll(nil)
    } else {
      selectText(nil)
    }
  }

  private static func describeLogPoint(_ point: CGPoint) -> [String: Double] {
    [
      "x": Double(point.x),
      "y": Double(point.y),
    ]
  }
}

private final class TerminalSearchButton: NSButton {
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }
}

private enum SearchNavigationAction {
  case close
  case next
  case previous

  var logName: String {
    switch self {
    case .close:
      return "close"
    case .next:
      return "next"
    case .previous:
      return "previous"
    }
  }
}

private final class TerminalSearchBarView: NSView, NSTextFieldDelegate {
  /**
   CDXC:NativeTerminals 2026-05-10-12:02
   Embedded Ghostty search should read as a neutral grey utility control, not
   a blue-tinted element. Keep the palette near-equal RGB so the floating find
   box stays visually calm over terminal content.
   */
  private static let backgroundColor = NSColor(
    calibratedRed: 0x19 / 255,
    green: 0x19 / 255,
    blue: 0x1B / 255,
    alpha: 0.96
  ).cgColor
  private static let borderColor = NSColor(
    calibratedRed: 0x83 / 255,
    green: 0x83 / 255,
    blue: 0x88 / 255,
    alpha: 0.42
  ).cgColor
  private static let focusedBorderColor = NSColor(calibratedWhite: 1, alpha: 0.94).cgColor

  private weak var surfaceView: GhostexGhosttySurfaceView?
  private var searchState: GhostexGhosttySearchState?
  private var cancellables = Set<AnyCancellable>()
  private var searchKeyMonitor: Any?
  private var searchFocusObserver: NSObjectProtocol?
  private var isSearchFieldFocused = false
  private let textField = TerminalSearchTextField()
  private let countLabel = NSTextField(labelWithString: "")
  private let previousButton = TerminalSearchButton()
  private let nextButton = TerminalSearchButton()
  private let closeButton = TerminalSearchButton()

  init(surfaceView: GhostexGhosttySurfaceView) {
    self.surfaceView = surfaceView
    super.init(frame: .zero)
    isHidden = true
    wantsLayer = true
    layer?.backgroundColor = Self.backgroundColor
    layer?.borderColor = Self.borderColor
    layer?.borderWidth = 1
    layer?.cornerRadius = 8
    layer?.masksToBounds = true

    configureTextField()
    configureCountLabel()
    configureButton(previousButton, symbolName: "chevron.up", action: #selector(findPrevious))
    configureButton(nextButton, symbolName: "chevron.down", action: #selector(findNext))
    configureButton(closeButton, symbolName: "xmark", action: #selector(closeSearch))
    addSubview(textField)
    addSubview(countLabel)
    addSubview(previousButton)
    addSubview(nextButton)
    addSubview(closeButton)

    searchFocusObserver = NotificationCenter.default.addObserver(
      forName: .ghosttySearchFocus,
      object: surfaceView,
      queue: .main
    ) { [weak self] _ in
      self?.focusSearchField(reason: "ghosttySearchFocusNotification")
    }
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  deinit {
    removeSearchKeyMonitor()
    if let searchFocusObserver {
      NotificationCenter.default.removeObserver(searchFocusObserver)
    }
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard !isHidden, alphaValue > 0, bounds.contains(point) else {
      return nil
    }
    /**
     CDXC:NativeTerminalSearch 2026-05-20-10:45:
     The floating Ghostty search bar sits above the terminal surface as a
     manually laid-out AppKit view. Child controls must be explicit hit-test
     owners so clicks on the text field and icon buttons do not stop at the
     parent bar and then fall back to terminal keyboard focus.
     */
    for child in [closeButton, nextButton, previousButton, textField] as [NSView] {
      guard !child.isHidden, child.alphaValue > 0 else {
        continue
      }
      let childPoint = convert(point, to: child)
      guard child.bounds.contains(childPoint) else {
        continue
      }
      let hitView = child.hitTest(childPoint) ?? child
      appendSearchLog(
        "nativeWorkspace.terminalSearch.childHitTest",
        details: [
          "childView": String(describing: type(of: child)),
          "hitView": String(describing: type(of: hitView)),
          "searchPoint": Self.describeLogPoint(point),
        ])
      return hitView
    }
    return self
  }

  override func mouseDown(with event: NSEvent) {
    appendSearchLog(
      "nativeWorkspace.terminalSearch.barMouseDown",
      details: ["localPoint": Self.describeLogPoint(convert(event.locationInWindow, from: nil))])
    focusSearchField(reason: "barMouseDown")
  }

  override func layout() {
    super.layout()
    /**
     CDXC:NativeTerminals 2026-04-29-02:00
     Native Ghostty search must stay a compact floating control. Manual
     AppKit frames prevent stack-view expansion from stretching the search
     input across the terminal pane and obscuring terminal content.
     */
    let inset = CGFloat(7)
    let buttonSize = CGFloat(24)
    let gap = CGFloat(4)
    let contentHeight = max(bounds.height - inset * 2, 20)
    var right = bounds.maxX - inset

    closeButton.frame = CGRect(
      x: right - buttonSize, y: inset - 1, width: buttonSize, height: contentHeight + 2)
    right = closeButton.frame.minX - gap
    nextButton.frame = CGRect(
      x: right - buttonSize, y: inset - 1, width: buttonSize, height: contentHeight + 2)
    right = nextButton.frame.minX - gap
    previousButton.frame = CGRect(
      x: right - buttonSize, y: inset - 1, width: buttonSize, height: contentHeight + 2)
    right = previousButton.frame.minX - gap
    countLabel.frame = CGRect(x: right - 50, y: inset, width: 50, height: contentHeight)
    right = countLabel.frame.minX - gap
    textField.frame = CGRect(
      x: inset + 2,
      y: inset,
      width: max(right - inset - 2, 80),
      height: contentHeight
    )
    if searchState != nil {
      appendSearchLog(
        "nativeWorkspace.terminalSearch.barLayout",
        details: [
          "closeButtonFrame": Self.describeLogFrame(closeButton.frame),
          "countLabelFrame": Self.describeLogFrame(countLabel.frame),
          "nextButtonFrame": Self.describeLogFrame(nextButton.frame),
          "previousButtonFrame": Self.describeLogFrame(previousButton.frame),
          "textFieldFrame": Self.describeLogFrame(textField.frame),
        ])
    }
  }

  func setSearchState(_ nextSearchState: GhostexGhosttySearchState?) {
    appendSearchLog(
      "nativeWorkspace.terminalSearch.barSetState",
      details: [
        "hadOldState": searchState != nil,
        "hasNewState": nextSearchState != nil,
        "incomingNeedleLength": nextSearchState?.needle.count ?? 0,
      ])
    searchState = nextSearchState
    cancellables.removeAll()
    guard let nextSearchState else {
      isHidden = true
      isSearchFieldFocused = false
      removeSearchKeyMonitor()
      updateFocusChrome()
      appendSearchLog("nativeWorkspace.terminalSearch.barHidden")
      return
    }

    isHidden = false
    installSearchKeyMonitorIfNeeded()
    updateFocusChrome()
    updateNeedle(nextSearchState.needle)
    updateCount(selected: nextSearchState.selected, total: nextSearchState.total)
    nextSearchState.$needle
      .receive(on: DispatchQueue.main)
      .sink { [weak self] needle in
        self?.updateNeedle(needle)
      }
      .store(in: &cancellables)
    nextSearchState.$selected
      .combineLatest(nextSearchState.$total)
      .receive(on: DispatchQueue.main)
      .sink { [weak self] selected, total in
        self?.updateCount(selected: selected, total: total)
      }
      .store(in: &cancellables)
    DispatchQueue.main.async { [weak self] in
      self?.focusSearchField(reason: "setSearchState")
    }
  }

  func controlTextDidChange(_ notification: Notification) {
    guard notification.object as? NSTextField === textField else {
      return
    }
    appendSearchLog(
      "nativeWorkspace.terminalSearch.controlTextDidChange",
      details: ["needleLength": textField.stringValue.count])
    searchState?.needle = textField.stringValue
  }

  func control(
    _ control: NSControl,
    textView: NSTextView,
    doCommandBy commandSelector: Selector
  ) -> Bool {
    guard control === textField else {
      return false
    }
    appendSearchLog(
      "nativeWorkspace.terminalSearch.controlCommand",
      details: ["commandSelector": NSStringFromSelector(commandSelector)])
    if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
      closeSearch()
      return true
    }
    if commandSelector == #selector(NSResponder.insertNewline(_:)) {
      /**
       CDXC:NativeTerminals 2026-05-10-11:51
       Embedded Ghostty search Return keys must navigate matches from the
       active AppKit field editor. Handling the text command directly prevents
       NSTextField from treating Return as a no-op field action.
       */
      navigateSearchFromReturn(shouldGoPrevious: false)
      return true
    }
    if commandSelector == #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)) {
      navigateSearchFromReturn(shouldGoPrevious: true)
      return true
    }
    if commandSelector == #selector(NSResponder.insertLineBreak(_:)) {
      navigateSearchFromReturn(shouldGoPrevious: true)
      return true
    }
    if commandSelector == #selector(NSResponder.moveUp(_:)) {
      navigateSearchFromArrow(shouldGoPrevious: true)
      return true
    }
    if commandSelector == #selector(NSResponder.moveDown(_:)) {
      navigateSearchFromArrow(shouldGoPrevious: false)
      return true
    }
    return false
  }

  private func configureTextField() {
    textField.delegate = self
    textField.cell = TerminalSearchTextFieldCell(textCell: "")
    textField.placeholderString = "Search"
    textField.isEditable = true
    textField.isEnabled = true
    textField.isSelectable = true
    textField.focusRingType = .none
    textField.isBezeled = false
    textField.drawsBackground = false
    textField.font = NSFont.systemFont(ofSize: 13)
    textField.textColor = NSColor(calibratedWhite: 0.94, alpha: 1)
    textField.onClose = { [weak self] in self?.closeSearch() }
    textField.onDiagnostic = { [weak self] event, details in
      self?.appendSearchLog(event, details: details)
    }
    textField.onFindNext = { [weak self] in self?.findNext() }
    textField.onFindPrevious = { [weak self] in self?.findPrevious() }
    textField.onFocusChanged = { [weak self] isFocused in
      self?.isSearchFieldFocused = isFocused
      self?.updateFocusChrome()
    }
  }

  private func configureCountLabel() {
    countLabel.cell = TerminalSearchCountLabelCell(textCell: "")
    countLabel.alignment = .right
    countLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
    countLabel.textColor = NSColor(calibratedWhite: 0.72, alpha: 1)
    countLabel.lineBreakMode = .byTruncatingMiddle
  }

  private func configureButton(_ button: NSButton, symbolName: String, action: Selector) {
    button.bezelStyle = .regularSquare
    button.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil)
    button.imagePosition = .imageOnly
    button.isBordered = false
    button.target = self
    button.action = action
  }

  private func updateNeedle(_ needle: String) {
    appendSearchLog(
      "nativeWorkspace.terminalSearch.updateNeedle",
      details: [
        "currentTextLength": textField.stringValue.count,
        "incomingNeedleLength": needle.count,
        "willAssign": textField.stringValue != needle,
      ])
    if textField.stringValue != needle {
      textField.stringValue = needle
    }
  }

  private func updateCount(selected: UInt?, total: UInt?) {
    if let selected {
      countLabel.stringValue = "\(selected + 1)/\(total.map(String.init) ?? "?")"
    } else if let total {
      countLabel.stringValue = "-/\(total)"
    } else {
      countLabel.stringValue = ""
    }
  }

  private func navigateSearchFromReturn(shouldGoPrevious: Bool) {
    let flags = NSApp.currentEvent?.modifierFlags.intersection(.deviceIndependentFlagsMask) ?? []
    appendSearchLog(
      "nativeWorkspace.terminalSearch.returnNavigation",
      details: [
        "modifierFlags": GhostexGhosttySurfaceView.searchDebugModifierNames(flags),
        "shouldGoPrevious": shouldGoPrevious,
      ])
    if shouldGoPrevious || flags.contains(.shift) {
      findPrevious()
    } else {
      findNext()
    }
  }

  private func navigateSearchFromArrow(shouldGoPrevious: Bool) {
    appendSearchLog(
      "nativeWorkspace.terminalSearch.arrowNavigation",
      details: ["shouldGoPrevious": shouldGoPrevious])
    if shouldGoPrevious {
      findPrevious()
    } else {
      findNext()
    }
  }

  private func installSearchKeyMonitorIfNeeded() {
    guard searchKeyMonitor == nil else {
      return
    }
    /**
     CDXC:NativeTerminalSearch 2026-05-20-21:38:
     Once typing uses AppKit's shared NSTextView field editor, navigation keys
     can bypass TerminalSearchTextField.keyDown and may not always arrive as
     NSTextFieldDelegate command selectors before Ghostty regains focus.
     While the search field or its field editor owns first responder, consume
     only search-navigation keys: Return/Shift+Return, Up, Down, and Escape.
     Printable input remains on the normal field-editor text path.
     */
    searchKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self, self.shouldHandleSearchNavigationEvent(event) else {
        return event
      }
      self.handleSearchNavigationEvent(event)
      return nil
    }
  }

  private func removeSearchKeyMonitor() {
    if let searchKeyMonitor {
      NSEvent.removeMonitor(searchKeyMonitor)
      self.searchKeyMonitor = nil
    }
  }

  private func shouldHandleSearchNavigationEvent(_ event: NSEvent) -> Bool {
    guard searchState != nil, !isHidden, firstResponderIsSearchFieldOrEditor() else {
      return false
    }
    return Self.searchNavigationAction(for: event) != nil
  }

  private func handleSearchNavigationEvent(_ event: NSEvent) {
    guard let action = Self.searchNavigationAction(for: event) else {
      return
    }
    appendSearchLog(
      "nativeWorkspace.terminalSearch.navigationKeyMonitor",
      details: [
        "action": action.logName,
        "keyCode": Int(event.keyCode),
        "modifierFlags": GhostexGhosttySurfaceView.searchDebugModifierNames(event.modifierFlags),
      ])
    switch action {
    case .close:
      closeSearch()
    case .next:
      findNext()
    case .previous:
      findPrevious()
    }
  }

  private static func searchNavigationAction(for event: NSEvent) -> SearchNavigationAction? {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.isDisjoint(with: [.command, .control, .option]) else {
      return nil
    }
    switch event.keyCode {
    case 36, 76:
      return flags.contains(.shift) ? .previous : .next
    case 53:
      return flags.isEmpty ? .close : nil
    case 125:
      return flags.isEmpty ? .next : nil
    case 126:
      return flags.isEmpty ? .previous : nil
    default:
      return nil
    }
  }

  @objc private func findNext() {
    let didNavigate = surfaceView?.navigateSearchToNext() ?? false
    appendSearchLog(
      "nativeWorkspace.terminalSearch.nextButtonAction",
      details: ["didNavigate": didNavigate])
  }

  @objc private func findPrevious() {
    let didNavigate = surfaceView?.navigateSearchToPrevious() ?? false
    appendSearchLog(
      "nativeWorkspace.terminalSearch.previousButtonAction",
      details: ["didNavigate": didNavigate])
  }

  @objc private func closeSearch() {
    appendSearchLog("nativeWorkspace.terminalSearch.closeAction")
    removeSearchKeyMonitor()
    surfaceView?.searchState = nil
    isSearchFieldFocused = false
    updateFocusChrome()
    if let surfaceView {
      let didFocusSurface = window?.makeFirstResponder(surfaceView) ?? false
      appendSearchLog(
        "nativeWorkspace.terminalSearch.closeFocusSurfaceRequested",
        details: ["didFocusSurface": didFocusSurface])
    }
  }

  private func focusSearchField(reason: String) {
    guard searchState != nil, !isHidden else {
      appendSearchLog(
        "nativeWorkspace.terminalSearch.textFieldFocusSkipped",
        details: ["reason": reason])
      return
    }
    /**
     CDXC:NativeTerminalSearch 2026-05-20-10:45:
     Cmd+F must immediately move keyboard ownership into the Ghostty search
     text field, including repeated Cmd+F while the bar is already open.
     While the field owns focus, ordinary typing plus editing shortcuts such
     as Cmd+A stay in the field; Return/Shift+Return navigate matches and
     Escape closes search.
     */
    let didFocus = window?.makeFirstResponder(textField) ?? false
    activateSearchFieldEditor(reason: reason)
    isSearchFieldFocused = didFocus || firstResponderIsSearchFieldOrEditor()
    updateFocusChrome()
    appendSearchLog(
      "nativeWorkspace.terminalSearch.textFieldFocusRequested",
      details: [
        "didFocus": didFocus,
        "reason": reason,
      ])
  }

  private func activateSearchFieldEditor(reason: String) {
    /**
     CDXC:NativeTerminalSearch 2026-05-20-11:35:
     Focusing an NSTextField control is not enough for Ghostty search because
     AppKit can leave the first responder as TerminalSearchTextField instead of
     installing the shared NSTextView field editor. Start editing explicitly so
     the caret appears, typed characters mutate the search string, and
     controlTextDidChange publishes the query into Ghostty.

     CDXC:NativeTerminalSearch 2026-05-20-14:38:
     Repros showed the control becoming first responder while currentEditor()
     stayed nil. Search input must be explicitly editable/selectable and must
     request AppKit's shared field editor after focus so keyDown does not stop
     at the control without an editable text session.
     */
    textField.selectText(nil)
    let editor = textField.currentEditor() ?? window?.fieldEditor(true, for: textField)
    if let editor {
      window?.makeFirstResponder(editor)
      let caretLocation = textField.stringValue.utf16.count
      editor.selectedRange = NSRange(location: caretLocation, length: 0)
      appendSearchLog(
        "nativeWorkspace.terminalSearch.textFieldEditorActivated",
        details: [
          "caretLocation": caretLocation,
          "editorClass": String(describing: type(of: editor)),
          "reason": reason,
        ])
    } else {
      appendSearchLog(
        "nativeWorkspace.terminalSearch.textFieldEditorMissing",
        details: [
          "isEditable": textField.isEditable,
          "isEnabled": textField.isEnabled,
          "isSelectable": textField.isSelectable,
          "reason": reason,
        ])
    }
  }

  private func updateFocusChrome() {
    let shouldHighlight = searchState != nil && (isSearchFieldFocused || firstResponderIsSearchFieldOrEditor())
    layer?.borderColor = shouldHighlight ? Self.focusedBorderColor : Self.borderColor
    layer?.borderWidth = shouldHighlight ? 2 : 1
  }

  private func firstResponderIsSearchFieldOrEditor() -> Bool {
    let firstResponder = window?.firstResponder
    return firstResponder === textField || firstResponder === textField.currentEditor()
  }

  private func appendSearchLog(_ event: String, details: [String: Any] = [:]) {
    var payload = details
    let firstResponder = window?.firstResponder
    payload["barFrame"] = Self.describeLogFrame(frame)
    payload["barHidden"] = isHidden
    payload["firstResponderClass"] = firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
    payload["firstResponderIsTextField"] = firstResponder === textField
    payload["firstResponderIsTextEditor"] = firstResponder === textField.currentEditor()
    payload["searchFieldFocused"] = isSearchFieldFocused
    payload["needleLength"] = searchState?.needle.count ?? 0
    payload["sessionId"] = surfaceView?.ghostexSessionId ?? ""
    payload["surfaceFocusedFlag"] = surfaceView?.focused ?? false
    payload["textFieldFrame"] = Self.describeLogFrame(textField.frame)
    payload["windowIsKey"] = window?.isKeyWindow ?? false
    payload["windowNumber"] = window?.windowNumber ?? 0
    /**
     CDXC:NativeTerminalSearch 2026-05-19-09:02:
     The floating Ghostty search box can be visible while AppKit refuses
     clicks or text input. Search-bar logs capture responder transitions,
     field-editor commands, and button action delivery without persisting the
     query text.
     */
    TerminalFocusDebugLog.append(event: event, details: payload)
  }

  private static func describeLogFrame(_ frame: CGRect) -> [String: Double] {
    [
      "height": Double(frame.height),
      "maxX": Double(frame.maxX),
      "maxY": Double(frame.maxY),
      "minX": Double(frame.minX),
      "minY": Double(frame.minY),
      "width": Double(frame.width),
    ]
  }

  private static func describeLogPoint(_ point: CGPoint) -> [String: Double] {
    [
      "x": Double(point.x),
      "y": Double(point.y),
    ]
  }
}

private final class TerminalPaneHeaderDragTargetView: NSView {
  private static let centerBackgroundColor = NSColor(
    calibratedRed: 0.18, green: 0.42, blue: 0.86, alpha: 0.12
  ).cgColor
  private static let edgeBackgroundColor = NSColor(
    calibratedRed: 0.18, green: 0.42, blue: 0.86, alpha: 0.2
  ).cgColor

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.borderWidth = 2
    layer?.cornerRadius = 6
    layer?.borderColor = NSColor(calibratedRed: 0.44, green: 0.68, blue: 1, alpha: 0.95).cgColor
    layer?.backgroundColor = Self.centerBackgroundColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    /**
     CDXC:PaneDragFeedback 2026-05-11-20:24
     Drop-target outlines are visual-only drag feedback. They can sit above pane
     content during cleanup, so AppKit must always click through to the real
     pane/titlebar owner underneath.
     */
    nil
  }

  func configure(placement: PaneDropPlacement) {
    layer?.backgroundColor =
      placement == .center ? Self.centerBackgroundColor : Self.edgeBackgroundColor
  }
}

private final class TerminalPaneTabReorderTargetView: NSView {
  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.44, green: 0.68, blue: 1, alpha: 0.98).cgColor
    layer?.cornerRadius = 1
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    /**
     CDXC:PaneDragFeedback 2026-05-11-20:24
     Tab reorder indicators are visual-only. They must never own the next click
     after a drag release, even if AppKit has not removed the layer yet.
     */
    nil
  }
}

private final class TerminalPaneHeaderDragGhostView: NSView {
  private static let height: CGFloat = 32
  private static let horizontalPadding: CGFloat = 8
  private static let iconSize: CGFloat = 16
  private static let iconGap: CGFloat = 7

  private let iconImageView = NSImageView(frame: .zero)
  private let titleLabel = NSTextField(labelWithString: "")

  override var isFlipped: Bool {
    true
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.08, green: 0.09, blue: 0.11, alpha: 0.96).cgColor
    layer?.borderColor = NSColor(calibratedWhite: 1, alpha: 0.18).cgColor
    layer?.borderWidth = 1
    layer?.cornerRadius = 7
    layer?.shadowColor = NSColor.black.cgColor
    layer?.shadowOpacity = 0.32
    layer?.shadowOffset = CGSize(width: 0, height: -5)
    layer?.shadowRadius = 12

    iconImageView.imageScaling = .scaleProportionallyDown
    iconImageView.wantsLayer = true
    iconImageView.layer?.cornerRadius = 3
    iconImageView.layer?.masksToBounds = true
    addSubview(iconImageView)

    titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    titleLabel.textColor = NSColor(calibratedWhite: 0.94, alpha: 1)
    titleLabel.lineBreakMode = .byTruncatingTail
    addSubview(titleLabel)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    /**
     CDXC:PaneDragFeedback 2026-05-11-20:24
     The floating drag ghost follows the pointer visually but is not a drag or
     click target. Keep it transparent to AppKit hit testing.
     */
    nil
  }

  func configure(
    title: String,
    favicon: NSImage?,
    agentIconDataUrl: String?,
    agentIconColorHex: String?,
    maxWidth: CGFloat
  ) {
    titleLabel.stringValue = title
    let agentIconImage = nativePaneImage(fromDataUrl: agentIconDataUrl, isTemplate: true)
    iconImageView.image = favicon ?? agentIconImage
    iconImageView.contentTintColor =
      favicon == nil && agentIconImage != nil
      ? nativePaneColor(fromHex: agentIconColorHex) ?? NSColor.white : nil
    let measuredTitleWidth = ceil(
      (title as NSString).size(withAttributes: [
        .font: titleLabel.font ?? NSFont.systemFont(ofSize: 12, weight: .semibold)
      ]).width
    )
    let width = min(
      maxWidth,
      max(96, Self.horizontalPadding * 2 + Self.iconSize + Self.iconGap + measuredTitleWidth)
    )
    frame.size = CGSize(width: width, height: Self.height)
    needsLayout = true
    layoutSubtreeIfNeeded()
  }

  override func layout() {
    super.layout()
    iconImageView.frame = CGRect(
      x: Self.horizontalPadding,
      y: floor((bounds.height - Self.iconSize) / 2),
      width: Self.iconSize,
      height: Self.iconSize
    )
    let titleX = iconImageView.frame.maxX + Self.iconGap
    titleLabel.frame = CGRect(
      x: titleX,
      y: floor((bounds.height - 16) / 2),
      width: max(0, bounds.width - titleX - Self.horizontalPadding),
      height: 16
    )
  }

}

private final class TerminalTitleBarActionButton: NSButton {
  private static let normalTintColor = NSColor(calibratedWhite: 0.88, alpha: 0.72)
  private static let hoverTintColor = NSColor(calibratedWhite: 0.96, alpha: 0.88)
  private static let activeTintColor = NSColor(calibratedWhite: 1.0, alpha: 0.96)
  private static let hoverBackgroundColor = NSColor(calibratedWhite: 1.0, alpha: 0.11).cgColor
  private static let activeBackgroundColor = NSColor(calibratedWhite: 1.0, alpha: 0.18).cgColor

  private var hoverTrackingArea: NSTrackingArea?
  private var baseToolTip: String?
  private var isOverlayInteractionSuppressed = false
  private var isPointerInside = false {
    didSet { updateActionChrome() }
  }
  override var isHighlighted: Bool {
    didSet { updateActionChrome() }
  }

  override var isEnabled: Bool {
    didSet { updateActionChrome() }
  }
  var chromeCornerRadius: CGFloat = 0 {
    didSet { needsLayout = true }
  }

  override var mouseDownCanMoveWindow: Bool {
    false
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    configureActionChrome()
  }

  required init?(coder: NSCoder) {
    super.init(coder: coder)
    configureActionChrome()
  }

  /**
   CDXC:PaneTitleBarUX 2026-05-08-16:02
   Pane title-bar actions across terminal, managed web/editor, and browser
   panes need clear interactivity without adding permanent chrome. A shared
   button subclass provides a subtle circular hover background, a slightly
   stronger pressed state, and the expected pointing-hand cursor everywhere the
   native title-bar action controls are used.

   CDXC:PaneTitleBarUX 2026-05-11-01:09
   Title-bar actions should read as compact rounded-square controls, but must
   not own custom cursor rects. AppKit was jumping between pointer, grab, and
   arrow while crossing tabs/title bars; keep these controls on the default
   cursor and express interactivity through chrome only.
   */
  private func configureActionChrome() {
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
    layer?.masksToBounds = true
    imageScaling = .scaleProportionallyDown
    contentTintColor = Self.normalTintColor
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  func setOverlayInteractionSuppressed(_ suppressed: Bool) {
    guard isOverlayInteractionSuppressed != suppressed else {
      return
    }
    isOverlayInteractionSuppressed = suppressed
    if suppressed {
      baseToolTip = toolTip
      toolTip = nil
      isPointerInside = false
      isHighlighted = false
    } else {
      toolTip = baseToolTip
    }
    updateActionChrome()
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard !isOverlayInteractionSuppressed else {
      return nil
    }
    return super.hitTest(point)
  }

  override func layout() {
    super.layout()
    layer?.cornerRadius = chromeCornerRadius
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let hoverTrackingArea {
      removeTrackingArea(hoverTrackingArea)
    }
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeInKeyWindow, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
      owner: self,
      userInfo: nil
    )
    hoverTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func mouseEntered(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      isPointerInside = false
      return
    }
    isPointerInside = true
  }

  override func mouseMoved(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      isPointerInside = false
      return
    }
    isPointerInside = true
  }

  override func mouseExited(with event: NSEvent) {
    isPointerInside = false
  }

  private func updateActionChrome() {
    guard isEnabled else {
      contentTintColor = Self.normalTintColor.withAlphaComponent(0.4)
      layer?.backgroundColor = NSColor.clear.cgColor
      return
    }
    if isHighlighted {
      contentTintColor = Self.activeTintColor
      layer?.backgroundColor = Self.activeBackgroundColor
    } else if isPointerInside {
      contentTintColor = Self.hoverTintColor
      layer?.backgroundColor = Self.hoverBackgroundColor
    } else {
      contentTintColor = Self.normalTintColor
      layer?.backgroundColor = NSColor.clear.cgColor
    }
  }
}

private func nativePaneTabsDebugFrame(_ frame: CGRect) -> [String: Double] {
  [
    "height": Double(frame.height),
    "maxX": Double(frame.maxX),
    "maxY": Double(frame.maxY),
    "minX": Double(frame.minX),
    "minY": Double(frame.minY),
    "width": Double(frame.width),
  ]
}

fileprivate enum TerminalPaneChromeRole {
  case commands
  case workspace
}

private final class TerminalTitleBarTabButton: NSButton {
  enum InlineAction {
    case close
  }

  private static let inlineButtonWidth: CGFloat = 22
  private static let inlineButtonHeight: CGFloat = 18
  private static let inlineButtonTrailingPadding: CGFloat = 4
  private static let inlineButtonBackgroundColor = NSColor(calibratedWhite: 0.16, alpha: 1).cgColor
  private static let inlineButtonHoverBackgroundColor = NSColor(calibratedWhite: 0.24, alpha: 1).cgColor
  private static let inlineButtonIconColor = NSColor(calibratedWhite: 0.94, alpha: 1).cgColor
  private static let workingIndicatorColor = NSColor(
    calibratedRed: 0xF5 / 255,
    green: 0x9E / 255,
    blue: 0x0B / 255,
    alpha: 1
  ).cgColor
  private static let attentionIndicatorColor = NSColor(
    calibratedRed: 0x65 / 255,
    green: 0xE5 / 255,
    blue: 0x8A / 255,
    alpha: 1
  ).cgColor
  private static let sleepingIconColor = NSColor(calibratedWhite: 0.86, alpha: 0.42)
  private static let delayedSendIconColor = NSColor(calibratedRed: 0xF5 / 255, green: 0x9E / 255, blue: 0x0B / 255, alpha: 0.96)
  private static let commandTabSeparatorColor = NSColor(calibratedWhite: 1, alpha: 0.10).cgColor
  private static let workspaceTabBaseRed: CGFloat = 0x05 / 255
  private static let workspaceTabBaseGreen: CGFloat = 0x06 / 255
  private static let workspaceTabBaseBlue: CGFloat = 0x08 / 255
  private static let sleepingIconSize: CGFloat = 9
  private static let delayedSendIconSize: CGFloat = 14
  private static let activityIndicatorSize: CGFloat = 8
  private static let activityIndicatorTrailingPadding: CGFloat = 9
  private static let titleLeadingPadding: CGFloat = 8
  private static let commandIdentityIconSize: CGFloat = 12
  private static let workspaceIdentityIconSize: CGFloat = 14
  private static let identityIconGap: CGFloat = 5
  private static let titleInlineActionGap: CGFloat = 4
  private static let workspaceInlineActionCornerRadius: CGFloat = 6
  /**
   CDXC:PaneTabs 2026-05-14-09:23:
   Non-command pane tab titles should be larger and lighter than the previous shared 11pt semibold style. Keep command pane tab titles on the old font so command pane chrome does not shift.

   CDXC:PaneTabs 2026-05-14-10:10:
   Non-command pane tabs should make the title font and session identity icon bigger again while command pane tabs keep their existing compact typography and icon sizing.
   */
  private static let commandTitleFont = NSFont.systemFont(ofSize: 11, weight: .semibold)
  private static let workspaceTitleFont = NSFont.systemFont(ofSize: 13, weight: .regular)
  private static let commandTitleTextHeight: CGFloat = 18
  private static let workspaceTitleTextHeight: CGFloat = 20
  private static let titleVerticalOffset: CGFloat = 2
  private static let commandTitleVerticalOffset: CGFloat = 2
  /**
   CDXC:PaneTabs 2026-05-10-18:30
   Pane tabs are native title-bar controls because Ghostty and WKWebView panes
   sit above the React workspace. Buttons forward mouse-down/drag/up with their
   session id so tab selection and tab drag/drop persist through the same
   paneLayout state as split commands.
   */
  var sessionId = ""
  var onTabMouseDown: ((NSEvent, String) -> Void)?
  var onTabMouseDragged: ((NSEvent, String) -> Void)?
  var onTabMouseUp: ((NSEvent, String) -> Void)?
  var onTabCloseRequested: ((String, PaneTabCloseScope) -> Void)?
  var onTabSleepRequested: ((String, PaneTabSleepScope) -> Void)?
  var onTabFocusRequested: ((String) -> Void)?
  var onTabActionRequested: ((String, TerminalTitleBarAction) -> Void)?
  private var contextMenuActions: [TerminalTitleBarAction] = []
  private var allowsClose = true
  private var activity: NativeTerminalActivity?
  private var hoveredInlineAction: InlineAction? {
    didSet {
      guard oldValue != hoveredInlineAction else { return }
      needsDisplay = true
    }
  }
  private var isActiveTab = false
  private var identityAgentIconColor: NSColor?
  private var identityAgentIconColorHex: String?
  private var identityAgentIconDataUrl: String?
  private var identityAgentIconImage: NSImage?
  private var identityFaviconDataUrl: String?
  private var identityFaviconImage: NSImage?
  private var isFocusedPane = true
  private var chromeRole: TerminalPaneChromeRole = .workspace
  private var delayedSendRemainingLabel: String?
  private var isSleepingTab = false
  private var showsCommandTrailingSeparator = false
  private var pendingMouseDownInlineAction: InlineAction?
  private var hoverTrackingArea: NSTrackingArea?
  private var baseToolTip: String?
  private var isOverlayInteractionSuppressed = false
  private var isTabHovered = false {
    didSet {
      guard oldValue != isTabHovered else { return }
      needsDisplay = true
    }
  }

  override var mouseDownCanMoveWindow: Bool {
    false
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    configure()
  }

  required init?(coder: NSCoder) {
    super.init(coder: coder)
    configure()
  }

  private func configure() {
    wantsLayer = true
    layer?.cornerRadius = 0
    layer?.masksToBounds = true
    bezelStyle = .texturedRounded
    isBordered = false
    font = titleFont
    lineBreakMode = .byTruncatingTail
    imagePosition = .noImage
  }

  func setActive(_ isActive: Bool) {
    isActiveTab = isActive
    updateChrome()
  }

  func setSleeping(_ isSleeping: Bool) {
    guard isSleepingTab != isSleeping else {
      return
    }
    isSleepingTab = isSleeping
    updateChrome()
  }

  func setDelayedSendRemainingLabel(_ remainingLabel: String?) {
    let normalizedLabel = remainingLabel?.trimmingCharacters(in: .whitespacesAndNewlines)
    let nextLabel = normalizedLabel?.isEmpty == false ? normalizedLabel : nil
    guard delayedSendRemainingLabel != nextLabel else {
      setTabToolTip(delayedSendToolTip())
      return
    }
    /*
     CDXC:DelayedSend 2026-05-17-03:14:
     Native pane tabs need the active Delayed Send timer beside the session
     title, using the same right-side status slot as the sleeping moon, and the
     tooltip must expose the remaining countdown.
    */
    delayedSendRemainingLabel = nextLabel
    setTabToolTip(delayedSendToolTip())
    needsDisplay = true
  }

  func setFocusedPane(_ isFocused: Bool) {
    guard isFocusedPane != isFocused else {
      return
    }
    isFocusedPane = isFocused
    updateChrome()
  }

  fileprivate func setChromeRole(_ role: TerminalPaneChromeRole) {
    guard chromeRole != role else {
      return
    }
    chromeRole = role
    layer?.cornerRadius = 0
    font = titleFont
    updateChrome()
  }

  fileprivate func setShowsCommandTrailingSeparator(_ showsSeparator: Bool) {
    guard showsCommandTrailingSeparator != showsSeparator else {
      return
    }
    showsCommandTrailingSeparator = showsSeparator
    needsDisplay = true
  }

  private func updateChrome() {
    contentTintColor = titleColor
    layer?.backgroundColor = tabBackgroundColor().cgColor
    needsDisplay = true
  }

  private func tabBackgroundColor() -> NSColor {
    /*
     CDXC:PaneTabs 2026-05-17-01:50:
     Workspace-area sleeping tabs should use the same subdued visual treatment
     as other unsurfaced tab siblings so the pane's surfaced session is the
     obvious tab. The moon remains the only sleeping-specific visual marker.
     */
    if chromeRole == .workspace {
      let isSurfacedWorkspaceTab = isActiveTab && !isSleepingTab
      let overlayAlpha =
        isSurfacedWorkspaceTab
        ? (isFocusedPane ? CGFloat(0.13) : CGFloat(0.05))
        : (isFocusedPane ? CGFloat(0.06) : CGFloat(0.024))
      return NSColor(calibratedWhite: 1, alpha: overlayAlpha)
    }
    let overlayAlpha =
      isActiveTab
      ? (isFocusedPane ? (isSleepingTab ? CGFloat(0.075) : CGFloat(0.13)) : CGFloat(0.05))
      : (isFocusedPane ? (isSleepingTab ? CGFloat(0.032) : CGFloat(0.06)) : CGFloat(0.024))
    return Self.compositedWorkspaceTabColor(overlayAlpha: overlayAlpha)
  }

  private static func compositedWorkspaceTabColor(overlayAlpha: CGFloat) -> NSColor {
    NSColor(
      calibratedRed: workspaceTabBaseRed + (1 - workspaceTabBaseRed) * overlayAlpha,
      green: workspaceTabBaseGreen + (1 - workspaceTabBaseGreen) * overlayAlpha,
      blue: workspaceTabBaseBlue + (1 - workspaceTabBaseBlue) * overlayAlpha,
      alpha: 1)
  }

  func setTabHovered(_ hovered: Bool) {
    isTabHovered = isOverlayInteractionSuppressed ? false : hovered
  }

  func setHoveredInlineAction(_ action: InlineAction?) {
    hoveredInlineAction = isOverlayInteractionSuppressed ? nil : action
  }

  func setTabToolTip(_ value: String?) {
    baseToolTip = value
    toolTip = isOverlayInteractionSuppressed ? nil : value
  }

  func setOverlayInteractionSuppressed(_ suppressed: Bool) {
    guard isOverlayInteractionSuppressed != suppressed else {
      return
    }
    isOverlayInteractionSuppressed = suppressed
    if suppressed {
      pendingMouseDownInlineAction = nil
      isTabHovered = false
      hoveredInlineAction = nil
    }
    toolTip = suppressed ? nil : baseToolTip
  }

  func setActivity(_ nextActivity: NativeTerminalActivity?) {
    guard activity != nextActivity else {
      return
    }
    activity = nextActivity
    needsDisplay = true
  }

  func setContextMenuActions(_ actions: [TerminalTitleBarAction]) {
    contextMenuActions = actions
  }

  func setAllowsClose(_ isAllowed: Bool) {
    guard allowsClose != isAllowed else {
      return
    }
    allowsClose = isAllowed
    pendingMouseDownInlineAction = nil
    setHoveredInlineAction(nil)
    needsDisplay = true
  }

  func setIdentityIconDataUrl(
    faviconDataUrl: String?,
    agentIconDataUrl: String?,
    agentIconColorHex: String?
  ) {
    guard identityFaviconDataUrl != faviconDataUrl
      || identityAgentIconDataUrl != agentIconDataUrl
      || identityAgentIconColorHex != agentIconColorHex
    else {
      return
    }
    identityFaviconDataUrl = faviconDataUrl
    identityAgentIconDataUrl = agentIconDataUrl
    identityAgentIconColorHex = agentIconColorHex
    identityFaviconImage = nativePaneImage(fromDataUrl: faviconDataUrl, isTemplate: false)
    identityAgentIconImage = nativePaneImage(fromDataUrl: agentIconDataUrl, isTemplate: true)
    identityAgentIconColor = nativePaneColor(fromHex: agentIconColorHex)
    needsDisplay = true
  }

  func inlineAction(at point: NSPoint) -> InlineAction? {
    guard allowsClose else {
      return nil
    }
    /**
     CDXC:PaneTabs 2026-05-11-11:47
     Inline Close hit testing follows the visible tab geometry, not the cached
     hover flag. Narrow panes can receive mouseDown immediately after a layout
     or scroll change, before AppKit has delivered a fresh mouseMoved.

     CDXC:PaneTabs 2026-05-11-19:36
     Narrow pane tabs put the rightmost inline action close to the clipped tab
     edge. Use the full tab-height hit band so real pointer clicks on the small
     Close icon do not miss by the 2px vertical paint inset.
     */
    if closeButtonHitFrame.contains(point) {
      return .close
    }
    return nil
  }

  override func draw(_ dirtyRect: NSRect) {
    /**
     CDXC:PaneTabs 2026-05-11-02:28
     Native tab titles are drawn manually so labels stay left-aligned and
     truncate before hover actions. AppKit's default button title is centered
     and does not reserve space for inline controls.

     CDXC:PaneTabs 2026-05-11-03:04
     Keep the title layout stable before and during hover so long tab titles do
     not visually resize or shift when Close appears. The title baseline is
     nudged up 1px to align with the native title-bar button icons.

     CDXC:PaneTabs 2026-05-11-03:15
     Tabs mirror session-card activity indicators: orange means working and
     green means attention/done. Draw the indicator on the right only while the
     tab is not hovered; hover controls float above the title instead of
     changing the title rect, so text truncation is stable while hovering.

     CDXC:PaneTabs 2026-05-11-08:32
     Main work-area tabs should show the same session identity cue as sidebar
     cards: browser favicon first, otherwise the projected agent/browser mask
     icon tinted with the sidebar's per-agent color. The icon is drawn inside
     the native tab button because these tabs are AppKit controls, not React.

     CDXC:PaneTabs 2026-05-11-03:18
     Close tab controls need hover feedback without changing layout. Draw an
     opaque control so title text cannot show through, and keep the icon
     explicitly light on dark tab chrome.

     CDXC:PaneTabs 2026-05-11-08:15
     Inline tab actions need opaque chrome, but their symbols must preserve
     glyph alpha. Draw Close as explicit strokes so the icon does not become a
     white bounding box.

     CDXC:PaneTabs 2026-05-11-08:51
     Widen only the inline action segment, not its height. The tab title bar is
     22px high, but the action chrome is intentionally 18px high so it aligns
     with the native title-bar button icons.

     CDXC:PaneTabs 2026-05-11-06:57
     Sleeping tabs use the moon itself as the right-side state marker instead
     of drawing both a leading moon and a gray indicator dot. Keep the marker in
     the same reserved right slot as working/attention so tab titles truncate
     before status UI consistently.

     CDXC:PaneTabs 2026-05-12-12:52
     Production pane tabs must not paint diagnostic hit-region colors. Keep
     narrow-pane click verification in logs and AppKit receiver ownership, not
     persistent magenta tab fills or outlines.
     */
    drawIdentityIconIfNeeded()
    drawTitle()
    if !isTabHovered {
      drawActivityIndicatorIfNeeded()
      drawCommandSeparatorIfNeeded()
      return
    }
    /**
     CDXC:PaneTabs 2026-05-11-02:02
     Per-session close controls live on the hovered tab, not on the pane
     titlebar action cluster. Draw compact inline controls inside the tab

     CDXC:PaneTabs 2026-05-11-20:24
     Inline Close hit testing belongs to the tab button that paints those
     controls. Keeping pointer ownership in the AppKit button hierarchy makes
     narrow right-side tab controls respond without workspace monitor routing.

     CDXC:PaneTabs 2026-05-14-10:10:
     Non-command pane tabs should round their inline Close control instead of drawing square segmented buttons. Preserve command pane tab controls as-is.

     CDXC:PaneTabs 2026-05-15-14:28:
     Sleep moved out of hover-only tab chrome and into the tab right-click menu. Keep hover chrome focused on Close so sleeping a tab is an intentional context-menu command instead of a neighboring inline button.
     */
    if allowsClose {
      drawInlineActionControl()
    }
    drawCommandSeparatorIfNeeded()
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let hoverTrackingArea {
      removeTrackingArea(hoverTrackingArea)
    }
    /**
     CDXC:PaneTabs 2026-05-12-10:13
     Narrow panes can clip a tab while the visible fragment still owns the
     Close control. Give each native tab button its own tracking area so hover
     chrome follows the actual AppKit button receiver instead of relying on the
     parent title bar to rediscover the same clipped geometry.
     */
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeInKeyWindow, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
      owner: self,
      userInfo: nil
    )
    hoverTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func mouseEntered(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      updateLocalHover(for: nil)
      return
    }
    updateLocalHover(for: convert(event.locationInWindow, from: nil))
  }

  override func mouseMoved(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      updateLocalHover(for: nil)
      return
    }
    updateLocalHover(for: convert(event.locationInWindow, from: nil))
  }

  override func mouseExited(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    if bounds.contains(point) {
      updateLocalHover(for: point)
      return
    }
    setTabHovered(false)
    setHoveredInlineAction(nil)
  }

  private func updateLocalHover(for point: NSPoint?) {
    guard let point, !isOverlayInteractionSuppressed else {
      setTabHovered(false)
      setHoveredInlineAction(nil)
      return
    }
    setTabHovered(bounds.contains(point))
    setHoveredInlineAction(inlineAction(at: point))
  }

  override func mouseDown(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      pendingMouseDownInlineAction = nil
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    updateLocalHover(for: point)
    if let inlineAction = inlineAction(at: point) {
      /**
       CDXC:PaneTabs 2026-05-11-19:36
       Visible tab Close controls are native AppKit button-region clicks. Handle
       them on the tab button itself instead of routing through a workspace
       monitor or title-bar coordinate router, so narrow right-side tabs keep
       one local mouseDown/mouseUp owner.
       */
      pendingMouseDownInlineAction = inlineAction
      return
    }
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.mouseDown", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    onTabMouseDown?(event, sessionId)
  }

  override func mouseDragged(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    if pendingMouseDownInlineAction != nil {
      return
    }
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.mouseDragged", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    onTabMouseDragged?(event, sessionId)
  }

  override func mouseUp(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      pendingMouseDownInlineAction = nil
      return
    }
    if let pendingInlineAction = pendingMouseDownInlineAction {
      pendingMouseDownInlineAction = nil
      let point = convert(event.locationInWindow, from: nil)
      guard inlineAction(at: point) == pendingInlineAction else {
        return
      }
      switch pendingInlineAction {
      case .close:
        onTabCloseRequested?(sessionId, .close)
      }
      return
    }
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.mouseUp", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    onTabMouseUp?(event, sessionId)
  }

  override func otherMouseDown(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    if event.buttonNumber != 2 {
      super.otherMouseDown(with: event)
      return
    }
    /**
     CDXC:PaneTabs 2026-05-11-01:25
     Middle-clicking a native pane tab is a direct tab close gesture. Consume
     button 2 on the tab button so the embedded terminal/web surface never sees
     the middle click, then close via the same scoped tab-close path as the
     context menu's Close action.
     */
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.middleMouseDown", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
  }

  override func otherMouseUp(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    if event.buttonNumber != 2 {
      super.otherMouseUp(with: event)
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.middleMouseUp", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "isInside": bounds.contains(point),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    if allowsClose, bounds.contains(point) {
      onTabCloseRequested?(sessionId, .close)
    }
  }

  override func otherMouseDragged(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    if event.buttonNumber == 2 {
      return
    }
    super.otherMouseDragged(with: event)
  }

  override func rightMouseDown(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    /**
     CDXC:PaneTabs 2026-05-11-00:45
     Native pane tabs need single-tab and group-scoped sleep/close commands
     from the tab itself. The menu reports only the clicked tab and requested
     scope; the sidebar resolves the containing tab node so scoped actions
     never affect unrelated panes or other tab groups.

     CDXC:PaneTabs 2026-05-15-14:28:
     Sleep is the first right-click menu item after leaving hover tab chrome.
     Keep the direct clicked-tab command above scoped Sleep Right/Left/Other
     options so users can sleep the intended tab without hunting through the
     broader tab-group actions.

     CDXC:PaneTabs 2026-05-15-15:43:
     Tab right-click menus should start with the primary session actions from
     the collapsed pane menu: Rename Session, Delayed Send, Fork Session,
     Reload Session, and Pop Out Pane. Keep those actions in one unseparated
     block, then place the separator before Sleep so the sleep/close tab-scope
     commands remain visually grouped below the moved actions.

     CDXC:SessionFocusMode 2026-05-23-09:28:
     Focus belongs in the native tab context menu above Pop Out Pane so users
     can enter the same reversible tab-group zoom without relying on
     double-click timing.
     */
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.contextMenu.opened", details: [
      "buttonBounds": nativePaneTabsDebugFrame(bounds),
      "buttonFrame": nativePaneTabsDebugFrame(frame),
      "sessionId": sessionId,
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    let menu = NSMenu()
    let primaryActions = primaryTabContextMenuActions()
    var didAddFocusItem = false
    for action in primaryActions {
      if action == .popOut || action == .restorePopOut {
        addTabFocusMenuItem(to: menu)
        didAddFocusItem = true
      }
      addTabActionMenuItem(action, to: menu)
    }
    if !didAddFocusItem {
      addTabFocusMenuItem(to: menu)
    }
    if !primaryActions.isEmpty {
      menu.addItem(NSMenuItem.separator())
    }
    if !isSleepingTab {
      addTabSleepMenuItem("Sleep", scope: .sleep, to: menu)
    }
    addTabSleepMenuItem("Sleep Right", scope: .sleepRight, to: menu)
    addTabSleepMenuItem("Sleep Left", scope: .sleepLeft, to: menu)
    addTabSleepMenuItem("Sleep Other Tabs", scope: .sleepOthers, to: menu)
    menu.addItem(NSMenuItem.separator())
    addTabCloseMenuItem("Close Right", scope: .closeRight, to: menu)
    addTabCloseMenuItem("Close Left", scope: .closeLeft, to: menu)
    addTabCloseMenuItem("Close Other Tabs", scope: .closeOthers, to: menu)
    NSMenu.popUpContextMenu(menu, with: event, for: self)
  }

  private func primaryTabContextMenuActions() -> [TerminalTitleBarAction] {
    let popOutAction: TerminalTitleBarAction =
      contextMenuActions.contains(.restorePopOut) ? .restorePopOut : .popOut
    return [.rename, .delayedSend, .fork, .reload, popOutAction].filter { contextMenuActions.contains($0) }
  }

  private func addTabActionMenuItem(_ action: TerminalTitleBarAction, to menu: NSMenu) {
    let item = NSMenuItem(
      title: TerminalSessionTitleBarView.actionMenuTitle(for: action),
      action: #selector(performTabActionMenuItem(_:)),
      keyEquivalent: "")
    item.representedObject = action.rawValue
    item.target = self
    item.image = TerminalSessionTitleBarView.actionMenuImage(for: action)
    menu.addItem(item)
  }

  private func addTabFocusMenuItem(to menu: NSMenu) {
    let item = NSMenuItem(title: "Focus", action: #selector(performTabFocusMenuItem(_:)), keyEquivalent: "")
    item.target = self
    item.image = NSImage(systemSymbolName: "scope", accessibilityDescription: "Focus")
    menu.addItem(item)
  }

  private func addTabSleepMenuItem(_ title: String, scope: PaneTabSleepScope, to menu: NSMenu) {
    let item = NSMenuItem(title: title, action: #selector(performTabSleepMenuItem(_:)), keyEquivalent: "")
    item.representedObject = scope.rawValue
    item.target = self
    menu.addItem(item)
  }

  private func addTabCloseMenuItem(_ title: String, scope: PaneTabCloseScope, to menu: NSMenu) {
    let item = NSMenuItem(title: title, action: #selector(performTabCloseMenuItem(_:)), keyEquivalent: "")
    item.representedObject = scope.rawValue
    item.target = self
    menu.addItem(item)
  }

  @objc private func performTabCloseMenuItem(_ sender: NSMenuItem) {
    guard let rawScope = sender.representedObject as? String,
      let scope = PaneTabCloseScope(rawValue: rawScope)
    else {
      return
    }
    onTabCloseRequested?(sessionId, scope)
  }

  @objc private func performTabSleepMenuItem(_ sender: NSMenuItem) {
    guard let rawScope = sender.representedObject as? String,
      let scope = PaneTabSleepScope(rawValue: rawScope)
    else {
      return
    }
    onTabSleepRequested?(sessionId, scope)
  }

  @objc private func performTabFocusMenuItem(_ sender: NSMenuItem) {
    onTabFocusRequested?(sessionId)
  }

  @objc private func performTabActionMenuItem(_ sender: NSMenuItem) {
    guard let rawAction = sender.representedObject as? String,
      let action = TerminalTitleBarAction(rawValue: rawAction)
    else {
      return
    }
    onTabActionRequested?(sessionId, action)
  }

  private var closeButtonFrame: CGRect {
    return CGRect(
      x: bounds.maxX - Self.inlineButtonTrailingPadding - Self.inlineButtonWidth,
      y: floor((bounds.height - Self.inlineButtonHeight) / 2),
      width: Self.inlineButtonWidth,
      height: Self.inlineButtonHeight)
  }

  private var closeButtonHitFrame: CGRect {
    closeButtonFrame.insetBy(dx: 0, dy: -floor((bounds.height - closeButtonFrame.height) / 2))
      .intersection(bounds)
  }

  private var inlineActionControlFrame: CGRect {
    return closeButtonFrame
  }

  private var titleColor: NSColor {
    let isSurfacedWorkspaceTab = chromeRole != .workspace || (isActiveTab && !isSleepingTab)
    let baseWhite: CGFloat = isSurfacedWorkspaceTab ? 0.96 : 0.78
    let baseAlpha: CGFloat = isSurfacedWorkspaceTab ? 0.98 : 0.82
    let sleepAlpha: CGFloat = isSleepingTab ? 0.48 : 1
    let resolvedSleepAlpha: CGFloat = chromeRole == .workspace ? 1 : sleepAlpha
    return NSColor(
      calibratedWhite: baseWhite,
      alpha: baseAlpha * resolvedSleepAlpha * (isFocusedPane ? 1 : 0.58))
  }

  private func drawTitle() {
    let titleLeadingInset = titleLeadingInsetForIdentity()
    let reservedTrailingWidth = titleTrailingReservedWidth()
    let resolvedTitleFont = titleFont
    let resolvedTitleTextHeight = titleTextHeight
    let titleRect = CGRect(
      x: titleLeadingInset,
      y: floor((bounds.height - resolvedTitleTextHeight) / 2) + titleVerticalOffset,
      width: max(bounds.width - titleLeadingInset - reservedTrailingWidth, 0),
      height: resolvedTitleTextHeight)
    guard titleRect.width > 0 else {
      return
    }

    let paragraphStyle = NSMutableParagraphStyle()
    paragraphStyle.alignment = .left
    paragraphStyle.lineBreakMode = .byTruncatingTail
    let attributes: [NSAttributedString.Key: Any] = [
      .font: resolvedTitleFont,
      .foregroundColor: titleColor,
      .paragraphStyle: paragraphStyle,
    ]
    (title as NSString).draw(in: titleRect, withAttributes: attributes)
  }

  private var titleFont: NSFont {
    chromeRole == .commands ? Self.commandTitleFont : Self.workspaceTitleFont
  }

  private var titleTextHeight: CGFloat {
    chromeRole == .commands ? Self.commandTitleTextHeight : Self.workspaceTitleTextHeight
  }

  private var titleVerticalOffset: CGFloat {
    chromeRole == .commands ? Self.commandTitleVerticalOffset : Self.titleVerticalOffset
  }

  private func drawCommandSeparatorIfNeeded() {
    guard chromeRole == .commands, showsCommandTrailingSeparator,
      let context = NSGraphicsContext.current?.cgContext
    else {
      return
    }
    context.saveGState()
    context.setFillColor(Self.commandTabSeparatorColor)
    context.fill(CGRect(x: bounds.maxX - 1, y: 0, width: 1, height: max(bounds.height, 1)))
    context.restoreGState()
  }

  private func titleLeadingInsetForIdentity() -> CGFloat {
    guard hasIdentityIcon else {
      return Self.titleLeadingPadding
    }
    return Self.titleLeadingPadding + identityIconSize + Self.identityIconGap
  }

  private var hasIdentityIcon: Bool {
    identityFaviconImage != nil || identityAgentIconImage != nil
  }

  private var identityIconFrame: CGRect {
    CGRect(
      x: Self.titleLeadingPadding,
      y: floor((bounds.height - identityIconSize) / 2),
      width: identityIconSize,
      height: identityIconSize)
  }

  private var identityIconSize: CGFloat {
    chromeRole == .commands ? Self.commandIdentityIconSize : Self.workspaceIdentityIconSize
  }

  private func drawIdentityIconIfNeeded() {
    if let favicon = identityFaviconImage {
      favicon.draw(
        in: identityIconFrame,
        from: .zero,
        operation: .sourceOver,
        fraction: isFocusedPane ? 1 : 0.62,
        respectFlipped: true,
        hints: nil)
      return
    }
    guard let agentIcon = identityAgentIconImage else {
      return
    }
    drawTintedSymbol(
      agentIcon,
      in: identityIconFrame,
      color: identityAgentIconColor ?? NSColor(calibratedWhite: 0.9, alpha: 1),
      rotateDegrees: 0,
      mirrorX: false)
  }

  private func titleTrailingReservedWidth() -> CGFloat {
    if hasActivityIndicator {
      return bounds.maxX - activityIndicatorFrame.minX + Self.titleInlineActionGap
    }
    return Self.titleLeadingPadding
  }

  private var hasActivityIndicator: Bool {
    delayedSendRemainingLabel != nil || isSleepingTab || activity != nil
  }

  private var activityIndicatorFrame: CGRect {
    return CGRect(
      x: bounds.maxX - Self.activityIndicatorTrailingPadding - Self.activityIndicatorSize,
      y: floor((bounds.height - Self.activityIndicatorSize) / 2),
      width: Self.activityIndicatorSize,
      height: Self.activityIndicatorSize)
  }

  private func drawActivityIndicatorIfNeeded() {
    if delayedSendRemainingLabel != nil {
      drawDelayedSendIcon()
      return
    }
    if isSleepingTab || activity == .sleeping {
      drawSleepingIcon()
      return
    }
    let fillColor: CGColor?
    switch activity {
    case .attention:
      fillColor = Self.attentionIndicatorColor
    case .working:
      fillColor = Self.workingIndicatorColor
    case .sleeping, .none:
      fillColor = nil
    }
    guard let fillColor, let context = NSGraphicsContext.current?.cgContext else {
      return
    }
    context.saveGState()
    context.setFillColor(fillColor)
    context.fillEllipse(in: activityIndicatorFrame)
    context.restoreGState()
  }

  private func drawSleepingIcon() {
    let baseRect = activityIndicatorFrame
    let iconRect = CGRect(
      x: baseRect.midX - Self.sleepingIconSize / 2,
      y: floor((bounds.height - Self.sleepingIconSize) / 2) + 1,
      width: Self.sleepingIconSize,
      height: Self.sleepingIconSize)
    guard let image = NSImage(systemSymbolName: "moon.fill", accessibilityDescription: nil) else {
      return
    }
    drawTintedSymbol(
      image,
      in: iconRect,
      color: Self.sleepingIconColor,
      rotateDegrees: 0,
      mirrorX: false)
  }

  private func drawDelayedSendIcon() {
    let baseRect = activityIndicatorFrame
    /*
     CDXC:DelayedSend 2026-05-21-12:21:
     Native pane-tab Delayed Send indicators should read as a larger orange
     timer in the same status slot as the sleeping moon. Center it slightly
     higher than the default symbol box so it aligns with the tab title text.
     */
    let iconRect = CGRect(
      x: baseRect.midX - Self.delayedSendIconSize / 2,
      y: floor((bounds.height - Self.delayedSendIconSize) / 2) - 1,
      width: Self.delayedSendIconSize,
      height: Self.delayedSendIconSize)
    guard let image = NSImage(systemSymbolName: "clock", accessibilityDescription: nil) else {
      return
    }
    drawTintedSymbol(
      image,
      in: iconRect,
      color: Self.delayedSendIconColor,
      rotateDegrees: 0,
      mirrorX: false)
  }

  private func delayedSendToolTip() -> String {
    guard let delayedSendRemainingLabel else {
      return title
    }
    return "\(title)\nDelayed Send in \(delayedSendRemainingLabel)"
  }

  private func drawInlineActionControl() {
    guard let context = NSGraphicsContext.current?.cgContext else {
      return
    }
    let controlFrame = inlineActionControlFrame
    let cornerRadius = chromeRole == .commands ? CGFloat(0) : Self.workspaceInlineActionCornerRadius
    let controlPath = CGPath(
      roundedRect: controlFrame,
      cornerWidth: cornerRadius,
      cornerHeight: cornerRadius,
      transform: nil)

    context.saveGState()
    context.addPath(controlPath)
    context.setFillColor(Self.inlineButtonBackgroundColor)
    context.fillPath()

    context.addPath(controlPath)
    context.clip()
    if hoveredInlineAction == .close {
      context.setFillColor(Self.inlineButtonHoverBackgroundColor)
      context.fill(closeButtonFrame)
    }
    context.restoreGState()

    drawInlineCloseSymbol(in: closeButtonFrame)
  }

  private func drawInlineCloseSymbol(in frame: CGRect) {
    guard let context = NSGraphicsContext.current?.cgContext else {
      return
    }
    let insetX: CGFloat = 7.8
    let insetY: CGFloat = 5.8
    context.saveGState()
    context.setStrokeColor(Self.inlineButtonIconColor)
    context.setLineWidth(1.5)
    context.setLineCap(.round)
    context.move(to: CGPoint(x: frame.minX + insetX, y: frame.minY + insetY))
    context.addLine(to: CGPoint(x: frame.maxX - insetX, y: frame.maxY - insetY))
    context.move(to: CGPoint(x: frame.maxX - insetX, y: frame.minY + insetY))
    context.addLine(to: CGPoint(x: frame.minX + insetX, y: frame.maxY - insetY))
    context.strokePath()
    context.restoreGState()
  }

  private func drawTintedSymbol(
    _ image: NSImage,
    in rect: CGRect,
    color: NSColor,
    rotateDegrees: CGFloat,
    mirrorX: Bool
  ) {
    guard let context = NSGraphicsContext.current?.cgContext else {
      return
    }
    context.saveGState()
    context.translateBy(x: rect.midX, y: rect.midY)
    if mirrorX {
      context.scaleBy(x: -1, y: 1)
    }
    context.rotate(by: rotateDegrees * .pi / 180)
    context.translateBy(x: -rect.midX, y: -rect.midY)
    /**
     CDXC:PaneTabs 2026-05-11-03:04
     AppKit template symbols drawn manually do not reliably pick up button
     tint. Draw the SF Symbol as a mask filled with an explicit light color so
     sleeping/status icons stay readable on dark tab chrome.
     */
    let maskRect = rect.insetBy(dx: 0.5, dy: 0)
    context.beginTransparencyLayer(auxiliaryInfo: nil)
    color.setFill()
    maskRect.fill()
    image.draw(
      in: maskRect,
      from: .zero,
      operation: .destinationIn,
      fraction: 1,
      respectFlipped: rotateDegrees == 0,
      hints: nil)
    context.endTransparencyLayer()
    context.restoreGState()
  }
}

private final class TerminalSessionTitleBarView: NSView {
  struct TabItem: Equatable {
    let actions: [TerminalTitleBarAction]
    let isSleeping: Bool
    let sessionId: String
    let title: String
  }

  struct TabReorderTarget {
    let lineFrame: CGRect
    let position: PaneTabReorderPosition
    let targetSessionId: String
  }

  private static let borderColor = NSColor(
    calibratedRed: 0x58 / 255,
    green: 0x6F / 255,
    blue: 0x95 / 255,
    alpha: 0.24
  ).cgColor
  private static let backgroundColor = NSColor(
    calibratedRed: 0x05 / 255,
    green: 0x06 / 255,
    blue: 0x08 / 255,
    alpha: 0.96
  ).cgColor
  private static let commandBackgroundColor = NSColor(
    calibratedWhite: 0.0,
    alpha: 1.0
  ).cgColor
  private static let commandCollapsedTrailingBackgroundColor = NSColor(
    calibratedWhite: 0.0,
    alpha: 1.0
  )
  private static let commandBorderColor = NSColor(
    calibratedWhite: 0.54,
    alpha: 0.24
  ).cgColor
  private static let titleColor = NSColor(
    calibratedRed: 0xE1 / 255,
    green: 0xE1 / 255,
    blue: 0xE1 / 255,
    alpha: 1
  )
  private static let workingIndicatorColor = NSColor(
    calibratedRed: 0xF5 / 255,
    green: 0x9E / 255,
    blue: 0x0B / 255,
    alpha: 1
  ).cgColor
  private static let attentionIndicatorColor = NSColor(
    calibratedRed: 0x65 / 255,
    green: 0xE5 / 255,
    blue: 0x8A / 255,
    alpha: 1
  ).cgColor
  private static let actionSeparatorColor = NSColor(calibratedWhite: 1, alpha: 0.16).cgColor
  private static let minimumVisibleTabViewportWidth: CGFloat = 80
  private static let minimumVisibleTabViewportWidthWithDoubleClickTarget: CGFloat = 56
  private static let preferredDoubleClickNewTerminalTargetWidth: CGFloat = 34
  private static let minimumDoubleClickNewTerminalTargetWidth: CGFloat = 24
  private static let commandActionTrailingInset: CGFloat = 8
  private static let tabViewportTrailingGap: CGFloat = 4
  private static let workspaceTabAddButtonGap: CGFloat = 2
  private static let commandTabAddButtonGap: CGFloat = 0
  private static let verticalWheelTabScrollMultiplier: CGFloat = 18
  private static let minimumDiscreteVerticalWheelTabScrollDelta: CGFloat = 96
  private static let activeTabRevealScrollMargin: CGFloat = 12
  private static let activeTabRevealMinimumVisibleWidth: CGFloat = 60
  /**
   CDXC:PaneTabs 2026-05-14-09:23:
   Non-command pane tabs need 2px more vertical reach above and below the old tab strip. Command pane tabs keep their existing full command-titlebar height.

   CDXC:PaneTabs 2026-05-14-10:10:
   Non-command pane tabs need another 2px of height above and below so the larger title and identity icon have more room without changing command pane tab height.

   CDXC:PaneTabs 2026-05-15-19:40:
   Vertical wheel gestures over the horizontal tab strip should move through tabs much faster than raw AppKit deltas, while direct horizontal wheel gestures keep their native precision.

   CDXC:PaneTabs 2026-05-15-19:50:
   The first vertical-wheel multiplier was still too slow in normal use. Boost precise vertical gestures harder and give non-precision wheel ticks a meaningful minimum horizontal step so tabs move several visible chunks per wheel action instead of creeping by raw AppKit delta size.

   CDXC:PaneTabs 2026-05-15-19:40:
   Any detected active-tab change should automatically scroll the tab strip enough to reveal the active session's tab after the next native layout pass computes tab widths.

   CDXC:PaneTabs 2026-05-15-19:53:
   Active-tab reveal must be no-op when the newly active tab is already fully visible. Only adjust the horizontal offset when activation leaves the selected tab clipped or offscreen.

   CDXC:PaneTabs 2026-05-15-20:04:
   When activation reveals a clipped right-side tab, scroll past exact edge alignment and clamp to the real maximum offset. This avoids cases where the selected session tab technically crosses the viewport boundary but does not visibly settle all the way into view.

   CDXC:PaneTabs 2026-05-22-09:14:
   Tab activation should preserve the user's current tab-strip position when the selected tab is already visibly usable. Share horizontal offsets across title-bar instances in the same tab group, and reveal only when the active tab is offscreen or clipped down below 60px of visible width so visible tab clicks and visible session switches do not move tabs around.
   */
  private static let workspaceTabButtonHeight: CGFloat = 36
  private static var tabScrollOffsetByGroupSignature: [String: CGFloat] = [:]

  private let faviconImageView = NSImageView(frame: .zero)
  private let titleLabel = NSTextField(labelWithString: "")
  private let activityIndicatorView = NSView(frame: .zero)
  private let tabClipView = NSView(frame: .zero)
  private let tabContentView = NSView(frame: .zero)
  private let tabAddButton = TerminalTitleBarActionButton(title: "", target: nil, action: nil)
  private let commandCollapsedTrailingBackgroundView = NSView(frame: .zero)
  private let bottomBorderView = NSView(frame: .zero)
  private let actionMenuButton = TerminalTitleBarActionButton(title: "", target: nil, action: nil)
  private let projectEditorCompanionCloseButton = TerminalTitleBarActionButton(title: "", target: nil, action: nil)
  private var actionButtons: [(action: TerminalTitleBarAction, button: NSButton)]
  private var actionSeparators: [NSView] = []
  private var activeTabSessionId: String?
  private var agentIconColor: NSColor?
  private var agentIconColorHex: String?
  private var agentIconDataUrl: String?
  private var agentIconImage: NSImage?
  private var activity: NativeTerminalActivity?
  private var chromeRole: TerminalPaneChromeRole = .workspace
  private var faviconImage: NSImage?
  private var isFocusedPane = false
  private var layoutHiddenActions = Set<TerminalTitleBarAction>()
  private var collapsedActionMenuActions: [TerminalTitleBarAction] = []
  private var projectEditorCompanionCloseAction: (() -> Void)?
  private var showsProjectEditorCompanionControls = false
  private var tabContentWidth: CGFloat = 0
  private var tabScrollOffsetX: CGFloat = 0
  private var tabViewportFrame: CGRect = .zero
  private var shouldScrollActiveTabIntoViewAfterLayout = false
  private var doubleClickNewTerminalFrame: CGRect = .zero
  private var tabButtons: [TerminalTitleBarTabButton] = []
  private var tabItems: [TabItem] = []
  private var allowsTabClosing = true
  private var showsTabAddButton = true
  private var debugOwnerSessionId: String?
  private var debugPaneKind = "unknown"
  private var lastLoggedPaneTabGeometrySignature: String?
  private var hoverTrackingArea: NSTrackingArea?
  private var isOverlayInteractionSuppressed = false
  private var isPaneHovered = false {
    didSet {
      updateActionButtonVisibility()
      if oldValue != isPaneHovered, let window {
        window.invalidateCursorRects(for: self)
      }
    }
  }
  private var isPointerInsideTitleBar = false {
    didSet {
      updateActionButtonVisibility()
      if oldValue != isPointerInsideTitleBar, let window {
        window.invalidateCursorRects(for: self)
      }
    }
  }
  var onMouseDown: ((NSEvent) -> Void)?
  var onAction: ((TerminalTitleBarAction) -> Void)?
  var onTabMouseDown: ((NSEvent, String) -> Void)?
  var onTabMouseDragged: ((NSEvent, String) -> Void)?
  var onTabMouseUp: ((NSEvent, String) -> Void)?
  var onTabSelected: ((String) -> Void)?
  var onTabCloseRequested: ((String, PaneTabCloseScope) -> Void)?
  var onTabSleepRequested: ((String, PaneTabSleepScope) -> Void)?
  var onTabFocusRequested: ((String) -> Void)?
  var onTabActionRequested: ((String, TerminalTitleBarAction) -> Void)?

  override var isFlipped: Bool {
    true
  }

  /**
   CDXC:PaneTitleBarUX 2026-05-11-19:10
   TerminalSessionTitleBarView owns native click dispatch for pane action
   controls. Do not let a titled popped-out NSWindow reinterpret title-bar
   pointer clicks as window-drag candidates before mouseDown/mouseUp reaches
   the local AppKit view handler.
   */
  override var mouseDownCanMoveWindow: Bool {
    false
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  var displayTitle: String {
    titleLabel.stringValue
  }

  var displayFavicon: NSImage? {
    faviconImage
  }

  func setOverlayInteractionSuppressed(_ suppressed: Bool) {
    guard isOverlayInteractionSuppressed != suppressed else {
      return
    }
    isOverlayInteractionSuppressed = suppressed
    isPaneHovered = false
    isPointerInsideTitleBar = false
    updateHoveredTab(for: nil)
    syncOverlayInteractionSuppressionForSubviews()
    updateActionButtonVisibility()
    if let window {
      window.invalidateCursorRects(for: self)
    }
  }

  private func syncOverlayInteractionSuppressionForSubviews() {
    for button in tabButtons {
      button.setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    }
    tabAddButton.setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    actionMenuButton.setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    projectEditorCompanionCloseButton.setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    for item in actionButtons {
      (item.button as? TerminalTitleBarActionButton)?
        .setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    }
  }

  func setDebugContext(ownerSessionId: String, paneKind: String) {
    guard debugOwnerSessionId != ownerSessionId || debugPaneKind != paneKind else {
      return
    }
    /**
     CDXC:PaneTabs 2026-05-15-09:37
     Native pane-tab geometry logs need stable owner metadata because a tab
     group can contain sessions whose selected tab differs from the title-bar
     view that owns the AppKit frames. Reset the dedupe signature when ownership
     changes so the next layout writes a fresh repro snapshot.
     */
    debugOwnerSessionId = ownerSessionId
    debugPaneKind = paneKind
    lastLoggedPaneTabGeometrySignature = nil
  }

  func setPaneHovered(_ isHovered: Bool) {
    isPaneHovered = isHovered
  }

  func setFocusedPane(_ isFocused: Bool) {
    guard isFocusedPane != isFocused else {
      return
    }
    /**
     CDXC:PaneTabs 2026-05-11-02:11
     Tab groups belonging to panes that are not currently focused should be
     slightly dimmed. This makes the active pane's tab group visible without
     changing individual selected-tab styling or pane layout.
     */
    isFocusedPane = isFocused
    updateTabGroupFocusAppearance()
  }

  func setTabs(_ tabs: [TabItem], activeSessionId: String) {
    let nextTabs = tabs
    let nextActiveTabSessionId = nextTabs.isEmpty ? nil : activeSessionId
    guard tabItems != nextTabs || activeTabSessionId != nextActiveTabSessionId else {
      return
    }
    let activeTabChanged = activeTabSessionId != nextActiveTabSessionId
    let nextTabGroupSignature = Self.tabScrollGroupSignature(for: nextTabs)
    if let syncedOffset = Self.tabScrollOffsetByGroupSignature[nextTabGroupSignature] {
      tabScrollOffsetX = syncedOffset
    }
    tabItems = nextTabs
    activeTabSessionId = nextActiveTabSessionId
    if activeTabChanged {
      shouldScrollActiveTabIntoViewAfterLayout = true
    }
    while tabButtons.count > nextTabs.count {
      tabButtons.removeLast().removeFromSuperview()
    }
    while tabButtons.count < nextTabs.count {
      let button = TerminalTitleBarTabButton(title: "", target: nil, action: nil)
      button.onTabMouseDown = { [weak self] event, sessionId in
        self?.onTabMouseDown?(event, sessionId)
      }
      button.onTabMouseDragged = { [weak self] event, sessionId in
        self?.onTabMouseDragged?(event, sessionId)
      }
      button.onTabMouseUp = { [weak self] event, sessionId in
        self?.onTabMouseUp?(event, sessionId)
      }
      button.onTabCloseRequested = { [weak self] sessionId, scope in
        self?.onTabCloseRequested?(sessionId, scope)
      }
      button.onTabSleepRequested = { [weak self] sessionId, scope in
        self?.onTabSleepRequested?(sessionId, scope)
      }
      button.onTabFocusRequested = { [weak self] sessionId in
        self?.onTabFocusRequested?(sessionId)
      }
      button.onTabActionRequested = { [weak self] sessionId, action in
        self?.onTabActionRequested?(sessionId, action)
      }
      tabButtons.append(button)
      tabContentView.addSubview(button)
      button.setAllowsClose(allowsTabClosing)
    }
    for (index, tab) in nextTabs.enumerated() {
      let button = tabButtons[index]
      button.sessionId = tab.sessionId
      button.title = tab.title
      button.setTabToolTip(tab.title)
      button.setChromeRole(chromeRole)
      button.setActive(tab.sessionId == activeSessionId)
      button.setSleeping(tab.isSleeping)
      button.setDelayedSendRemainingLabel(nil)
      button.setContextMenuActions(tab.actions)
      button.setOverlayInteractionSuppressed(isOverlayInteractionSuppressed)
    }
    updateTabGroupFocusAppearance()
    needsLayout = true
    window?.invalidateCursorRects(for: self)
  }

  private static func tabScrollGroupSignature(for tabs: [TabItem]) -> String {
    tabs.map(\.sessionId).joined(separator: "|")
  }

  private func syncTabScrollOffsetCache() {
    guard !tabItems.isEmpty else {
      return
    }
    Self.tabScrollOffsetByGroupSignature[Self.tabScrollGroupSignature(for: tabItems)] = tabScrollOffsetX
  }

  func setAllowsTabClosing(_ allowsClosing: Bool) {
    guard allowsTabClosing != allowsClosing else {
      return
    }
    /**
     CDXC:GitProjectTabs 2026-05-16-07:42:
     Git project tabs reuse the main native tab strip for navigation only.
     Disable inline close chrome for that strip so Git tabs cannot appear to be
     normal Agents session tabs with close/sleep lifecycle controls.
     */
    allowsTabClosing = allowsClosing
    for button in tabButtons {
      button.setAllowsClose(allowsClosing)
    }
  }

  func setShowsTabAddButton(_ isVisible: Bool) {
    guard showsTabAddButton != isVisible else {
      return
    }
    showsTabAddButton = isVisible
    needsLayout = true
  }

  fileprivate func setChromeRole(_ role: TerminalPaneChromeRole) {
    guard chromeRole != role else {
      return
    }
    chromeRole = role
    layer?.backgroundColor = backgroundColor(for: role)
    bottomBorderView.layer?.backgroundColor = borderColor(for: role)
    titleLabel.textColor = titleColor(for: role)
    for button in tabButtons {
      button.setChromeRole(role)
    }
    needsDisplay = true
  }

  func setTabActivities(_ activities: [String: NativeTerminalActivity]) {
    for button in tabButtons {
      button.setActivity(activities[button.sessionId])
    }
  }

  func setTabDelayedSendRemainingLabels(_ remainingLabels: [String: String]) {
    for button in tabButtons {
      button.setDelayedSendRemainingLabel(remainingLabels[button.sessionId])
    }
  }

  func setTabIdentityIcons(
    faviconDataUrls: [String: String],
    agentIconDataUrls: [String: String],
    agentIconColors: [String: String]
  ) {
    /**
     CDXC:PaneTabs 2026-05-11-08:32
     Native pane tabs mirror sidebar-card identity icons for every tab in the
     group, including parked and sleeping tabs that do not currently own a
     visible pane titlebar. Browser favicons win over the generic browser SVG;
     terminal/T3 tabs use the projected agent SVG and color.
     */
    for button in tabButtons {
      button.setIdentityIconDataUrl(
        faviconDataUrl: faviconDataUrls[button.sessionId],
        agentIconDataUrl: agentIconDataUrls[button.sessionId],
        agentIconColorHex: agentIconColors[button.sessionId])
    }
  }

  static let defaultActions: [TerminalTitleBarAction] = [
    .newTerminal,
    .openBrowser,
    .splitHorizontal,
    .splitVertical,
    .rotatePanesClockwise,
    .mergeAllTabs,
    .rename,
    .delayedSend,
    .fork,
    .reload,
    .popOut,
  ]

  /**
   CDXC:PaneTitleBarUX 2026-05-11-11:05
   Browser and T3 Code panes should expose the same creation/split/merge chrome
   as the sidebar sync sends: Terminal, Browser, separator, Split Right, Split
   Down, Merge all tabs. Keep the initial native titlebar aligned before the
   first layout sync.

   CDXC:PaneTitleBarUX 2026-05-15-13:51:
   Rotate Panes belongs directly below Split Downwards in the collapsed pane
   overflow menu so split layout actions stay grouped before Merge All Tabs.
   */
  static let webPaneCreationActions: [TerminalTitleBarAction] = [
    .newTerminal,
    .openBrowser,
    .splitHorizontal,
    .splitVertical,
    .rotatePanesClockwise,
    .mergeAllTabs,
  ]

  init(title: String, actions: [TerminalTitleBarAction] = TerminalSessionTitleBarView.defaultActions) {
    /**
     CDXC:BrowserPanes 2026-05-11-11:05
     Browser panes keep navigation/tooling controls in their dedicated browser
     toolbar. Their pane title bar exposes shared pane creation, split, and
     Merge All Tabs controls, while terminals keep the full session action set.
     */
    actionButtons = actions.map { action in
      (action, Self.makeActionButton(for: action))
    }
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = Self.backgroundColor
    layer?.borderColor = Self.borderColor
    layer?.borderWidth = 0

    faviconImageView.imageScaling = .scaleProportionallyDown
    faviconImageView.isHidden = true
    faviconImageView.wantsLayer = true
    faviconImageView.layer?.cornerRadius = 3
    faviconImageView.layer?.masksToBounds = true

    titleLabel.stringValue = title
    titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
    titleLabel.textColor = Self.titleColor
    titleLabel.lineBreakMode = .byTruncatingTail

    activityIndicatorView.wantsLayer = true
    activityIndicatorView.layer?.backgroundColor = NSColor.clear.cgColor
    activityIndicatorView.layer?.cornerRadius = 4
    activityIndicatorView.isHidden = true

    tabClipView.wantsLayer = true
    tabClipView.layer?.masksToBounds = true
    tabClipView.isHidden = true
    tabContentView.wantsLayer = true
    tabClipView.addSubview(tabContentView)
    configureTabAddButton()

    commandCollapsedTrailingBackgroundView.wantsLayer = true
    commandCollapsedTrailingBackgroundView.layer?.backgroundColor =
      Self.commandCollapsedTrailingBackgroundColor.cgColor
    commandCollapsedTrailingBackgroundView.isHidden = true

    bottomBorderView.wantsLayer = true
    bottomBorderView.layer?.backgroundColor = Self.borderColor

    addSubview(faviconImageView)
    addSubview(titleLabel)
    addSubview(activityIndicatorView)
    addSubview(tabClipView)
    addSubview(tabAddButton)
    addSubview(commandCollapsedTrailingBackgroundView, positioned: .below, relativeTo: tabClipView)
    /**
     CDXC:PaneTabs 2026-05-12-12:44
     Pane title bars must not paint colored debug hit-region overlays in normal
     app use. Narrow-pane click verification stays in logs and native hit-test
     ownership, leaving the visible chrome to production tab/action styling.
     */
    for item in actionButtons {
      item.button.target = self
      item.button.action = #selector(performTitleBarAction(_:))
      addSubview(item.button)
    }
    configureActionMenuButton()
    configureProjectEditorCompanionButton(
      projectEditorCompanionCloseButton,
      systemSymbolName: "xmark",
      fallbackTitle: "X",
      tooltip: "Close Session Pane",
      action: #selector(performProjectEditorCompanionCloseButton(_:)))
    addSubview(projectEditorCompanionCloseButton)
    hideProjectEditorCompanionButtons()
    syncActionSeparators()
    addSubview(bottomBorderView)
    updateActionButtonVisibility()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func mouseDown(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    isPointerInsideTitleBar = true
    if showsTabAddButton, event.clickCount >= 2, !tabItems.isEmpty, isEmptyTitleBarDoubleClickPoint(point) {
      /**
       CDXC:PaneTabs 2026-05-11-11:47
       Double-clicking unoccupied pane title-bar chrome creates a new terminal
       inside this pane's tab group. Real tab/control hits are excluded by
       isEmptyTitleBarDoubleClickPoint so activation, Sleep, Close, and menu
       clicks keep their normal single-click behavior.

       CDXC:PaneTabs 2026-05-11-20:24
       Check the reserved double-click target before right-side action dispatch.
       Hover can reveal action chrome near the reserved region, but the red
       target is explicit empty title-bar behavior and should not depend on a
       window monitor or mouse-up revalidation.
       */
      NativePaneTabDragReproLog.append(event: "nativePaneTabs.titleBar.doubleClickNewTerminal", details: [
        "doubleClickFrame": nativePaneTabsDebugFrame(doubleClickNewTerminalFrame),
        "hitPoint": nativePaneTabsDebugFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0)),
        "tabViewportFrame": nativePaneTabsDebugFrame(tabViewportFrame),
        "titleBarBounds": nativePaneTabsDebugFrame(bounds),
      ])
      onAction?(.newTerminal)
      return
    }
    onMouseDown?(event)
  }

  private func backgroundColor(for role: TerminalPaneChromeRole) -> CGColor {
    role == .commands ? Self.commandBackgroundColor : Self.backgroundColor
  }

  private func borderColor(for role: TerminalPaneChromeRole) -> CGColor {
    role == .commands ? Self.commandBorderColor : Self.borderColor
  }

  private func titleColor(for role: TerminalPaneChromeRole) -> NSColor {
    Self.titleColor
  }

  override func scrollWheel(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      return
    }
    guard !tabItems.isEmpty, tabContentWidth > tabViewportFrame.width else {
      super.scrollWheel(with: event)
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    guard tabViewportFrame.contains(point) else {
      super.scrollWheel(with: event)
      return
    }
    let isVerticalWheelGesture = abs(event.scrollingDeltaY) >= abs(event.scrollingDeltaX)
    let rawDelta =
      isVerticalWheelGesture
      ? amplifiedVerticalWheelTabDelta(for: event)
      : event.scrollingDeltaX
    let maxOffset = max(tabContentWidth - tabViewportFrame.width, 0)
    tabScrollOffsetX = min(max(tabScrollOffsetX - rawDelta, 0), maxOffset)
    syncTabScrollOffsetCache()
    needsLayout = true
  }

  private func amplifiedVerticalWheelTabDelta(for event: NSEvent) -> CGFloat {
    let scaledDelta = event.scrollingDeltaY * Self.verticalWheelTabScrollMultiplier
    guard !event.hasPreciseScrollingDeltas, scaledDelta != 0 else {
      return scaledDelta
    }
    guard abs(scaledDelta) < Self.minimumDiscreteVerticalWheelTabScrollDelta else {
      return scaledDelta
    }
    return scaledDelta < 0
      ? -Self.minimumDiscreteVerticalWheelTabScrollDelta
      : Self.minimumDiscreteVerticalWheelTabScrollDelta
  }

  func isDraggableHeaderPoint(_ point: NSPoint) -> Bool {
    false
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard !isOverlayInteractionSuppressed, bounds.contains(point) else {
      return nil
    }
    /**
     CDXC:NativePaneReorder 2026-05-03-03:42
     Pane headers used to be draggable from visible title and empty title-bar
     chrome, but tab dragging is now the only pane-reorder gesture. Keep action
     buttons as normal controls and keep tab hit testing inside the title-bar
     hierarchy so hidden/offscreen tab sessions can still be selected and
     dragged through their visible tab controls.

     CDXC:NativePaneReorder 2026-05-11-01:16
     Drag-to-reorder starts only from tabs. Non-action subviews such as the
     title label, favicon, and activity dot still should not win hit testing,
     otherwise title-bar focus and tab hit routing can disappear behind labels.

     CDXC:BrowserPanes 2026-05-03-11:06
     Action buttons remain real AppKit buttons for drawing, Accessibility, and
     pointer dispatch across terminal, browser, editor, and popped-out panes.

     CDXC:PaneTitleBarUX 2026-05-11-20:25
     When right-side pane actions are visible, return the concrete NSButton for
     its real frame. This keeps hamburger and action clicks on AppKit
     target/action dispatch.

     CDXC:PaneTabs 2026-05-12-11:59
     Tab clicks must be owned by the native TerminalTitleBarTabButton that
     paints the tab. Right-side pane actions remain higher-priority sibling
     buttons; visible tab pixels return the actual tab button so AppKit delivers
     the real mouseDown/mouseUp stream to that button, not to title-bar focus
     handling or any monitor-style click router.
     */
    layoutSubtreeIfNeeded()
    if let projectEditorCompanionButton = projectEditorCompanionButton(at: point) {
      /**
       CDXC:ProjectEditorCompanion 2026-05-14-09:47:
       Companion controls live inside the session titlebar and must be returned
       as concrete NSButton hits. This keeps hover background and click dispatch
       on the visible controls instead of a detached overlay strip.

       CDXC:ProjectEditorCompanion 2026-05-15-15:29:
       Only the companion Close button remains in this titlebar control group.
       It still returns as a concrete native button hit so AppKit owns hover and
       click dispatch.
       */
      return projectEditorCompanionButton
    }
    if let collapsedActionMenuButton = collapsedActionMenuButton(at: point) {
      logCollapsedActionMenuEvent(
        "nativePaneActionMenu.titleBar.hitTest",
        point: point)
      /**
       CDXC:PaneTitleBarUX 2026-05-11-20:24
       Collapsed action-menu clicks belong to the actual NSButton frame.
       Returning the native button keeps the menu on button target/action
       dispatch instead of title-bar click synthesis.
       */
      return collapsedActionMenuButton
    }
    if let actionButton = actionButton(at: point) {
      /**
       CDXC:PaneTitleBarUX 2026-05-11-20:24
       Titlebar action buttons own their full AppKit frames. Return the concrete
       NSButton so Pop In/Pop Out, Close, Split, Reload, and sibling actions all
       use performTitleBarAction(_:).
       */
      return actionButton
    }
    if let addButton = tabAddButton(at: point) {
      return addButton
    }
    if let tabButton = tabButton(at: point) {
      return tabButton
    }
    if let hitView = super.hitTest(point), hitView !== self {
      if hitView === faviconImageView
        || hitView === titleLabel
        || hitView === activityIndicatorView
        || hitView === tabClipView
        || hitView === tabContentView
        || hitView === bottomBorderView
        || actionSeparators.contains(where: { $0 === hitView })
      {
        return self
      }
      return hitView
    }
    return self
  }

  func tabSessionId(at point: NSPoint) -> String? {
    tabButton(at: point)?.sessionId
  }

  func tabFrame(for sessionId: String) -> CGRect? {
    guard let frame = tabButtons.first(where: { !$0.isHidden && $0.sessionId == sessionId })?.frame
    else {
      return nil
    }
    return CGRect(
      x: tabViewportFrame.minX + frame.minX - tabScrollOffsetX,
      y: tabViewportFrame.minY + frame.minY,
      width: frame.width,
      height: frame.height)
  }

  func tabInlineAction(at point: NSPoint) -> (
    action: TerminalTitleBarTabButton.InlineAction, sessionId: String
  )? {
    guard let hit = tabButtonHit(at: point),
      let action = hit.button.inlineAction(at: hit.localPoint)
    else {
      return nil
    }
    return (action, hit.button.sessionId)
  }

  private func tabButton(at point: NSPoint) -> TerminalTitleBarTabButton? {
    tabButtonHit(at: point)?.button
  }

  private func tabButtonHit(at point: NSPoint) -> (
    button: TerminalTitleBarTabButton, localPoint: NSPoint
  )? {
    guard !tabClipView.isHidden, tabViewportFrame.contains(point) else {
      return nil
    }
    let contentPoint = CGPoint(
      x: point.x - tabViewportFrame.minX + tabScrollOffsetX,
      y: point.y - tabViewportFrame.minY)
    for button in tabButtons where !button.isHidden && button.frame.contains(contentPoint) {
      return (
        button,
        CGPoint(x: contentPoint.x - button.frame.minX, y: contentPoint.y - button.frame.minY)
      )
    }
    return nil
  }

  func containsTab(_ sessionId: String) -> Bool {
    tabItems.contains { $0.sessionId == sessionId }
  }

  func splitAnchorSessionId(excluding sourceSessionId: String, placeAfterTarget: Bool) -> String? {
    let siblingSessionIds = tabItems
      .map(\.sessionId)
      .filter { $0 != sourceSessionId }
    guard !siblingSessionIds.isEmpty else {
      return nil
    }
    /**
     CDXC:PaneTabs 2026-05-12-11:08
     A same-pane active-tab split is valid only when another tab remains behind.
     Anchor before-edge splits to the first sibling and after-edge splits to the
     last sibling so the resulting split order matches the hovered edge.
     */
    return placeAfterTarget ? siblingSessionIds.last : siblingSessionIds.first
  }

  func isTabStripPoint(_ point: NSPoint) -> Bool {
    !tabClipView.isHidden && tabViewportFrame.contains(point)
  }

  func tabReorderTarget(at point: NSPoint, sourceSessionId: String) -> TabReorderTarget? {
    guard isTabStripPoint(point),
      let sourceIndex = tabItems.firstIndex(where: { $0.sessionId == sourceSessionId }),
      tabItems.count > 1
    else {
      return nil
    }
    let contentX = point.x - tabViewportFrame.minX + tabScrollOffsetX
    let insertionIndex = tabInsertionIndex(forContentX: contentX)
    let finalIndex = insertionIndex > sourceIndex ? insertionIndex - 1 : insertionIndex
    guard finalIndex != sourceIndex else {
      return nil
    }
    let remainingTabs = tabItems.filter { $0.sessionId != sourceSessionId }
    guard !remainingTabs.isEmpty else {
      return nil
    }
    let targetSessionId: String
    let position: PaneTabReorderPosition
    if finalIndex <= 0 {
      targetSessionId = remainingTabs[0].sessionId
      position = .before
    } else if finalIndex >= remainingTabs.count {
      targetSessionId = remainingTabs[remainingTabs.count - 1].sessionId
      position = .after
    } else {
      targetSessionId = remainingTabs[finalIndex].sessionId
      position = .before
    }
    /**
     CDXC:PaneTabs 2026-05-11-01:43
     Tab reorder feedback is an insertion line, not the pane drop overlay. Hide
     the line for source-adjacent no-ops so dragging a tab back to its original
     slot gives no false indication that release will change order.
     */
    let lineX = min(
      max(tabInsertionLineContentX(forInsertionIndex: insertionIndex) - tabScrollOffsetX + tabViewportFrame.minX,
        tabViewportFrame.minX),
      tabViewportFrame.maxX)
    return TabReorderTarget(
      lineFrame: CGRect(
        x: lineX - 1,
        y: tabViewportFrame.minY + 4,
        width: 2,
        height: max(tabViewportFrame.height - 8, 6)),
      position: position,
      targetSessionId: targetSessionId)
  }

  private func tabInsertionIndex(forContentX contentX: CGFloat) -> Int {
    guard !tabButtons.isEmpty else {
      return 0
    }
    for (index, button) in tabButtons.enumerated() where !button.isHidden {
      if contentX < button.frame.midX {
        return index
      }
    }
    return tabButtons.count
  }

  private func tabInsertionLineContentX(forInsertionIndex insertionIndex: Int) -> CGFloat {
    let visibleButtons = tabButtons.filter { !$0.isHidden }
    guard !visibleButtons.isEmpty else {
      return 0
    }
    if insertionIndex <= 0 {
      return visibleButtons[0].frame.minX
    }
    if insertionIndex >= visibleButtons.count {
      return visibleButtons[visibleButtons.count - 1].frame.maxX
    }
    return visibleButtons[insertionIndex].frame.minX
  }

  private func tabDebugFrames() -> [[String: Any]] {
    tabButtons.map { button in
      [
        "frame": nativePaneTabsDebugFrame(button.frame),
        "isHidden": button.isHidden,
        "sessionId": button.sessionId,
        "title": button.title,
      ]
    }
  }

  func actionButtonAction(at point: NSPoint) -> TerminalTitleBarAction? {
    actionButtonItem(at: point)?.action
  }

  func containsCollapsedActionMenuPoint(_ point: NSPoint) -> Bool {
    isCollapsedActionMenuPoint(point)
  }

  private func isCollapsedActionMenuPoint(_ point: NSPoint) -> Bool {
    collapsedActionMenuButton(at: point) != nil
  }

  private func collapsedActionMenuButton(at point: NSPoint) -> TerminalTitleBarActionButton? {
    guard !collapsedActionMenuActions.isEmpty, shouldShowCollapsedActionMenu, !actionMenuButton.frame.isEmpty
    else {
      return nil
    }
    /**
     CDXC:PaneTitleBarUX 2026-05-11-20:24
     The collapsed action menu's actual NSButton frame is the full click target.
     Do not maintain a separate invisible hamburger geometry in the titlebar.
     */
    guard actionMenuButton.frame.contains(point),
      !actionMenuButton.isHidden,
      actionMenuButton.isEnabled,
      actionMenuButton.alphaValue > 0
    else {
      return nil
    }
    return actionMenuButton
  }

  private func projectEditorCompanionButton(at point: NSPoint) -> TerminalTitleBarActionButton? {
    guard showsProjectEditorCompanionControls else {
      return nil
    }
    for button in [projectEditorCompanionCloseButton]
    where button.frame.contains(point)
      && !button.isHidden
      && button.isEnabled
      && button.alphaValue > 0 {
      return button
    }
    return nil
  }

  private func actionButton(at point: NSPoint) -> NSButton? {
    actionButtonItem(at: point)?.button
  }

  private func actionButtonItem(at point: NSPoint) -> (action: TerminalTitleBarAction, button: NSButton)? {
    for item in actionButtons
    where isActionButtonVisible(item.action)
      && item.button.frame.contains(point)
      && !item.button.isHidden
      && item.button.isEnabled
      && item.button.alphaValue > 0 {
      return item
    }
    return nil
  }

  private func tabAddButton(at point: NSPoint) -> NSButton? {
    guard tabAddButton.frame.contains(point),
      !tabAddButton.isHidden,
      tabAddButton.isEnabled,
      tabAddButton.alphaValue > 0
    else {
      return nil
    }
    return tabAddButton
  }

  private func isEmptyTitleBarDoubleClickPoint(_ point: NSPoint) -> Bool {
    /**
     CDXC:PaneTabs 2026-05-11-11:47
     Double-clicking unused pane title-bar chrome creates a new terminal in the
     same tab group. Reject real tab, inline Close, and action-control
     hits, but allow unoccupied tab-strip space so wide single-tab panes keep
     the fast "new terminal in this pane" gesture.

     CDXC:PaneTabs 2026-05-11-12:23
     Narrow panes can leave no obvious blank title-bar chrome once tabs and the
     collapsed action menu are visible. The reserved red target is explicitly
     empty chrome and must create a terminal even when the tab viewport is
     otherwise tight.
     */
    if doubleClickNewTerminalFrame.contains(point) {
      return true
    }
    if tabAddButton(at: point) != nil {
      return false
    }
    if tabInlineAction(at: point) != nil || tabSessionId(at: point) != nil {
      return false
    }
    if isCollapsedActionMenuPoint(point) || actionButtonAction(at: point) != nil {
      return false
    }
    if projectEditorCompanionButton(at: point) != nil {
      return false
    }
    return bounds.contains(point)
  }

  override func resetCursorRects() {
    super.resetCursorRects()
    /**
     CDXC:PaneTitleBarUX 2026-05-11-01:09
     Pane title bars, tabs, and buttons must keep the default cursor. Previous
     pointer/open-hand cursor rects fought with AppKit tracking areas and caused
     visible cursor jumps while moving across title-bar controls.
     */
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let hoverTrackingArea {
      removeTrackingArea(hoverTrackingArea)
    }
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeInKeyWindow, .cursorUpdate, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
      owner: self,
      userInfo: nil
    )
    hoverTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func cursorUpdate(with event: NSEvent) {
    updateCursor(for: event)
  }

  override func mouseEntered(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      isPointerInsideTitleBar = false
      updateHoveredTab(for: nil)
      return
    }
    isPointerInsideTitleBar = true
    updateHoveredTab(for: convert(event.locationInWindow, from: nil))
    updateCursor(for: event)
  }

  override func mouseMoved(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      isPointerInsideTitleBar = false
      updateHoveredTab(for: nil)
      return
    }
    isPointerInsideTitleBar = true
    updateHoveredTab(for: convert(event.locationInWindow, from: nil))
    updateCursor(for: event)
  }

  override func mouseExited(with event: NSEvent) {
    guard !isOverlayInteractionSuppressed else {
      isPointerInsideTitleBar = false
      updateHoveredTab(for: nil)
      return
    }
    let point = convert(event.locationInWindow, from: nil)
    if bounds.contains(point) {
      isPointerInsideTitleBar = true
      updateHoveredTab(for: point)
      updateCursor(for: event)
      return
    }
    isPointerInsideTitleBar = false
    updateHoveredTab(for: nil)
  }

  private func updateCursor(for event: NSEvent) {
    /**
     CDXC:NativePaneResize 2026-05-13-07:23
     Pane title bars should not participate in split cursor arbitration. Split
     rails are now real AppKit divider bands with their own cursor rects; avoid
     explicitly setting the arrow from titlebar hover tracking so titlebars
     cannot override the adjacent rail's resize cursor.
     */
  }

  private func updateHoveredTab(for point: NSPoint?) {
    guard !isOverlayInteractionSuppressed else {
      for button in tabButtons {
        button.setTabHovered(false)
        button.setHoveredInlineAction(nil)
      }
      return
    }
    let hoveredSessionId = point.flatMap { tabSessionId(at: $0) }
    for button in tabButtons {
      button.setTabHovered(button.sessionId == hoveredSessionId)
    }
    let hoveredInlineAction = point.flatMap { tabInlineAction(at: $0) }
    for button in tabButtons {
      button.setHoveredInlineAction(
        button.sessionId == hoveredInlineAction?.sessionId ? hoveredInlineAction?.action : nil)
    }
  }

  private func updateTabGroupFocusAppearance() {
    tabContentView.alphaValue = 1.0
    for button in tabButtons {
      button.setFocusedPane(isFocusedPane)
    }
  }

  override func layout() {
    super.layout()
    let isCommandChrome = chromeRole == .commands
    let insetX: CGFloat = isCommandChrome ? 0 : 8
    let buttonSize: CGFloat = isCommandChrome ? bounds.height : 28
    let tabButtonHeight: CGFloat = isCommandChrome ? buttonSize : Self.workspaceTabButtonHeight
    let buttonGap: CGFloat = 0
    let separatorGap: CGFloat = 6
    let separatorWidth: CGFloat = 1
    let separatorHeight: CGFloat = isCommandChrome ? 10 : 14
    let indicatorSize: CGFloat = 8
    let indicatorGap: CGFloat = 6
    let tabViewportTrailingGap = isCommandChrome ? CGFloat(0) : Self.tabViewportTrailingGap
    let centerY = isCommandChrome ? 0 : floor((bounds.height - buttonSize) / 2)
    let tabCenterY = isCommandChrome ? centerY : floor((bounds.height - tabButtonHeight) / 2)
    let tabAddButtonGap =
      isCommandChrome ? Self.commandTabAddButtonGap : Self.workspaceTabAddButtonGap
    let tabAddButtonSize = tabButtonHeight
    let separatorY = floor((bounds.height - separatorHeight) / 2)
    let isCollapsedCommandPanelBar =
      isCommandChrome && actionButtons.map(\.action) == [.expandCommandsPanel]
    let actionTrailingInset =
      isCommandChrome && !isCollapsedCommandPanelBar ? Self.commandActionTrailingInset : insetX
    var trailingX = bounds.width - actionTrailingInset

    /**
     CDXC:NativeTerminals 2026-04-28-05:18
     Terminal titles should not truncate before reaching the right-side action
     cluster. Keep title-bar actions compact so pane names use the available
     chrome width while still leaving a non-overlapping hit target per action.

     CDXC:BrowserPanes 2026-05-03-01:58
     Browser pane title bars keep only normal session controls on the right.
     Browser navigation/tooling belongs in the address toolbar, while the
     webpage favicon and title identify the page on the left.

     CDXC:PaneTitleBarUX 2026-05-10-17:35
     Title-bar actions stay on the right with no dead hover gaps between
     buttons. Each button gets extra internal width for icon padding, and the
     layout hides lower-priority actions instead of letting the cluster overflow
     beyond the left edge of narrow panes.

     CDXC:PaneTitleBarUX 2026-05-11-20:24
     Action NSButton frames are the real hit targets. Use larger stable frames
     with centered icons instead of compact visual buttons plus titlebar-level
     invisible hit expansion.
     */
    var nextLayoutHiddenActions = Set<TerminalTitleBarAction>()
    let nonCloseActions = actionButtons.map(\.action).filter { $0 != .close }
    /**
     CDXC:PaneTitleBarUX 2026-05-11-11:47
     Narrow tabbed panes must keep native tabs clickable and draggable. Action
     buttons are AppKit subviews above the tab strip, so when the full horizontal
     cluster would leave less than a usable tab viewport, collapse non-close
     actions into a single native hamburger menu. If even that menu would clip
     a one-tab viewport below the tab's own minimum interactive width, hide the
     menu too; tab selection plus inline Close is the primary narrow-pane
     controls.

     CDXC:PaneTitleBarUX 2026-05-12-19:06
     Pane action chrome should always use the hamburger menu, independent of pane
     width. Keep the full action-strip layout branch intact for future reuse, but
     route current layouts through the compact menu so panes do not switch back
     to the expanded icon cluster on wider widths.
     */
    let shouldCollapseActionMenu =
      !showsProjectEditorCompanionControls
      && !Self.isCommandsPanelChromeActionSet(actionButtons.map(\.action))
      && !nonCloseActions.isEmpty
    let minimumContentWidthForCollapsedControls =
      tabItems.isEmpty ? 0 : Self.minimumVisibleTabViewportWidth
    let hasCloseAction = actionButtons.contains { $0.action == .close }
    let canReserveCloseActionInCollapsedLayout =
      hasCloseAction
      && bounds.width - insetX * 2 - buttonSize - tabViewportTrailingGap
        >= minimumContentWidthForCollapsedControls
    let reservedCloseActionWidth = canReserveCloseActionInCollapsedLayout ? buttonSize : 0
    let canReserveCollapsedActionMenu =
      bounds.width - insetX * 2 - reservedCloseActionWidth - buttonSize - tabViewportTrailingGap
        >= minimumContentWidthForCollapsedControls
    var separatorIndex = 0
    if showsProjectEditorCompanionControls {
      /*
       CDXC:ProjectEditorCompanion 2026-05-16-11:46:
       The side companion pane in Code, Git, and Project views needs a minimal
       titlebar with only its local close control. Hide the normal session pane
       action buttons and collapsed overflow menu instead of leaving generic
       pane actions in this embedded-editor surface.
       */
      collapsedActionMenuActions = []
      actionMenuButton.frame = .zero
      for item in actionButtons {
        item.button.frame = .zero
        nextLayoutHiddenActions.insert(item.action)
      }
      for separator in actionSeparators {
        separator.frame = .zero
      }
    } else if shouldCollapseActionMenu {
      collapsedActionMenuActions = canReserveCollapsedActionMenu ? nonCloseActions : []
      for item in actionButtons {
        if item.action == .close && canReserveCloseActionInCollapsedLayout {
          trailingX -= buttonSize
          let buttonX = max(0, trailingX)
          item.button.frame = CGRect(
            x: buttonX,
            y: centerY,
            width: min(buttonSize + insetX, bounds.width - buttonX),
            height: buttonSize)
          trailingX -= buttonGap
        } else {
          item.button.frame = .zero
          nextLayoutHiddenActions.insert(item.action)
        }
      }
      if canReserveCollapsedActionMenu {
        trailingX -= buttonSize
        let menuX = max(0, trailingX)
        actionMenuButton.frame = CGRect(
          x: menuX,
          y: centerY,
          width: min(buttonSize + (canReserveCloseActionInCollapsedLayout ? 0 : insetX), bounds.width - menuX),
          height: buttonSize)
        trailingX -= buttonGap
      } else {
        actionMenuButton.frame = .zero
      }
      for separator in actionSeparators {
        separator.frame = .zero
      }
    } else {
      collapsedActionMenuActions = []
      actionMenuButton.frame = .zero
      var rightActionGroup: Int?
      for item in actionButtons.reversed() {
        let actionGroup = Self.actionGroup(for: item.action)
        let isRightmostActionButton = rightActionGroup == nil
        if let rightActionGroup, rightActionGroup != actionGroup {
          let separatorAndButtonWidth = separatorGap + separatorWidth + separatorGap + buttonSize
          if trailingX - separatorAndButtonWidth < insetX && item.action != .close {
            item.button.frame = .zero
            nextLayoutHiddenActions.insert(item.action)
            continue
          }
          trailingX -= separatorGap
          if separatorIndex < actionSeparators.count {
            let separator = actionSeparators[separatorIndex]
            separator.frame = CGRect(
              x: trailingX - separatorWidth,
              y: separatorY,
              width: separatorWidth,
              height: separatorHeight
            )
          }
          trailingX -= separatorWidth + separatorGap
          separatorIndex += 1
        } else if trailingX - buttonSize < insetX && item.action != .close {
          item.button.frame = .zero
          nextLayoutHiddenActions.insert(item.action)
          continue
        }
        trailingX -= buttonSize
        let buttonX = max(0, trailingX)
        item.button.frame = CGRect(
          x: buttonX,
          y: centerY,
          width: min(buttonSize + (isRightmostActionButton ? insetX : 0), bounds.width - buttonX),
          height: buttonSize)
        trailingX -= buttonGap
        rightActionGroup = actionGroup
      }
      if separatorIndex < actionSeparators.count {
        for index in separatorIndex..<actionSeparators.count {
          actionSeparators[index].frame = .zero
        }
      }
    }
    layoutProjectEditorCompanionButtons(
      trailingX: &trailingX,
      centerY: centerY,
      buttonSize: buttonSize,
      buttonGap: buttonGap,
      insetX: insetX)
    if layoutHiddenActions != nextLayoutHiddenActions {
      layoutHiddenActions = nextLayoutHiddenActions
      updateActionButtonVisibility()
    }

    if !tabItems.isEmpty {
      faviconImageView.isHidden = true
      faviconImageView.frame = .zero
      titleLabel.isHidden = true
      titleLabel.frame = .zero
      activityIndicatorView.isHidden = true
      activityIndicatorView.frame = .zero
      tabClipView.isHidden = false
      let tabAreaMaxX = max(insetX, trailingX - tabViewportTrailingGap)
      let canShowTabAddButton =
        showsTabAddButton
        && (tabAreaMaxX - insetX
          >= Self.minimumVisibleTabViewportWidthWithDoubleClickTarget + tabAddButtonGap + tabAddButtonSize)
      let tabViewportMaxX =
        canShowTabAddButton
        ? max(insetX, tabAreaMaxX - tabAddButtonGap - tabAddButtonSize)
        : reserveDoubleClickNewTerminalTarget(from: insetX, to: tabAreaMaxX)
      if canShowTabAddButton {
        doubleClickNewTerminalFrame = .zero
      }
      layoutTabButtons(
        from: insetX,
        to: tabViewportMaxX,
        centerY: tabCenterY,
        height: tabButtonHeight)
      layoutTabAddButton(
        maxX: tabAreaMaxX,
        centerY: tabCenterY,
        size: tabAddButtonSize,
        gap: tabAddButtonGap,
        isVisible: canShowTabAddButton)
      logPaneTabLayoutGeometryIfNeeded(
        canShowTabAddButton: canShowTabAddButton,
        tabAreaMaxX: tabAreaMaxX,
        tabViewportMaxX: tabViewportMaxX,
        tabButtonHeight: tabButtonHeight,
        tabAddButtonSize: tabAddButtonSize,
        tabAddButtonGap: tabAddButtonGap,
        centerY: centerY,
        tabCenterY: tabCenterY,
        buttonSize: buttonSize,
        insetX: insetX)
      if isCollapsedCommandPanelBar {
        commandCollapsedTrailingBackgroundView.isHidden = false
        commandCollapsedTrailingBackgroundView.frame = CGRect(
          x: tabViewportFrame.maxX,
          y: 0,
          width: max(0, bounds.width - tabViewportFrame.maxX),
          height: bounds.height)
      } else {
        commandCollapsedTrailingBackgroundView.isHidden = true
        commandCollapsedTrailingBackgroundView.frame = .zero
      }
      bottomBorderView.frame = CGRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1)
      window?.invalidateCursorRects(for: self)
      return
    }

    commandCollapsedTrailingBackgroundView.isHidden = true
    commandCollapsedTrailingBackgroundView.frame = .zero
    doubleClickNewTerminalFrame = .zero
    tabClipView.isHidden = true
    tabClipView.frame = .zero
    tabContentView.frame = .zero
    tabViewportFrame = .zero
    tabContentWidth = 0
    tabScrollOffsetX = 0
    hideTabAddButton()
    for button in tabButtons {
      button.frame = .zero
    }
    titleLabel.isHidden = false

    /**
     CDXC:NativeTerminals 2026-04-28-03:37
     Per-terminal title bars must not show the blue focused-session dot. Keep
     focus state visible through the pane border while preserving a small
     card-matched activity dot immediately after the title for done/working.
     */
    let faviconSize: CGFloat = 16
    let faviconGap: CGFloat = 6
    /**
     CDXC:NativePaneReorder 2026-05-03-04:52
     Pane title bars should identify terminal/T3 sessions with the same agent
     logo shown on the session card, using the existing favicon placement to
     avoid adding new chrome. Browser favicons remain higher priority because
     they identify the loaded page more specifically than the generic browser
     logo. Sidebar agent SVGs are mask assets, so native AppKit renders them as
     template images tinted with the same per-agent color as the session card
     instead of using their black source fill on dark chrome.
     */
    let identityImage = faviconImage ?? agentIconImage
    let hasIdentityImage = identityImage != nil
    faviconImageView.image = identityImage
    faviconImageView.contentTintColor =
      faviconImage == nil && agentIconImage != nil ? agentIconColor ?? Self.titleColor : nil
    if hasIdentityImage {
      faviconImageView.isHidden = false
      faviconImageView.frame = CGRect(
        x: insetX,
        y: floor((bounds.height - faviconSize) / 2),
        width: faviconSize,
        height: faviconSize
      )
    } else {
      faviconImageView.isHidden = true
      faviconImageView.frame = CGRect(
        x: insetX,
        y: floor((bounds.height - faviconSize) / 2),
        width: 0,
        height: 0
      )
    }
    let titleX = hasIdentityImage ? insetX + faviconSize + faviconGap : insetX
    let titleTrailing = trailingX
    let maxTitleWidth = max(
      titleTrailing - titleX - (activity == nil ? 2 : indicatorSize + indicatorGap + 2),
      0
    )
    /**
     CDXC:NativeTerminals 2026-05-01-02:18
     AppKit truncating text fields need a frame as wide as the title's usable
     area. Measuring the raw title is only for placing the activity dot; the
     label itself must span maxTitleWidth so native text drawing does not
     ellipsize against a stale or too-small intrinsic width.
     */
    let measuredTitleWidth = ceil(
      (titleLabel.stringValue as NSString).size(withAttributes: [
        .font: titleLabel.font ?? NSFont.systemFont(ofSize: 12, weight: .bold)
      ]).width
    )
    let titleLabelVerticalOffset: CGFloat = chromeRole == .commands ? 0 : 2
    titleLabel.frame = CGRect(
      x: titleX,
      y: floor((bounds.height - 16) / 2) + titleLabelVerticalOffset,
      width: maxTitleWidth,
      height: 16
    )
    let visibleTitleWidth = min(measuredTitleWidth, maxTitleWidth)
    activityIndicatorView.frame = CGRect(
      x: titleLabel.frame.minX + visibleTitleWidth + indicatorGap,
      y: floor((bounds.height - indicatorSize) / 2),
      width: indicatorSize,
      height: indicatorSize
    )
    bottomBorderView.frame = CGRect(x: 0, y: bounds.height - 1, width: bounds.width, height: 1)
    /**
     CDXC:NativePaneReorder 2026-05-03-04:42
     Header drag affordance must stay visible after the pointer settles, not
     only during mouse-moved events. AppKit builds cursor rectangles from the
     laid-out action-button frames, so refresh them after layout changes the
     draggable title-bar width.
     */
    window?.invalidateCursorRects(for: self)
  }

  private func reserveDoubleClickNewTerminalTarget(from minX: CGFloat, to maxX: CGFloat) -> CGFloat {
    /**
     CDXC:PaneTabs 2026-05-11-12:23
     The double-click-to-new-terminal setting needs actual blank title-bar
     chrome in narrow panes. Reserve a compact target before the right-side
     action area instead of relying on leftover tab-strip slack that can vanish
     when a single tab expands to fill the viewport.
     */
    doubleClickNewTerminalFrame = .zero
    let availableWidth = max(maxX - minX, 0)
    let maximumTargetWidth =
      availableWidth - Self.minimumVisibleTabViewportWidthWithDoubleClickTarget
    guard maximumTargetWidth >= Self.minimumDoubleClickNewTerminalTargetWidth else {
      return maxX
    }
    let targetWidth = min(Self.preferredDoubleClickNewTerminalTargetWidth, maximumTargetWidth)
    let targetMinX = maxX - targetWidth
    doubleClickNewTerminalFrame = CGRect(
      x: targetMinX,
      y: 0,
      width: targetWidth,
      height: bounds.height)
    return max(minX, targetMinX - Self.tabViewportTrailingGap)
  }

  private func layoutTabButtons(from minX: CGFloat, to maxX: CGFloat, centerY: CGFloat, height: CGFloat) {
    let availableWidth = max(maxX - minX, 0)
    guard availableWidth > 0 else {
      tabClipView.frame = .zero
      tabContentView.frame = .zero
      tabViewportFrame = .zero
      tabContentWidth = 0
      for button in tabButtons {
        button.frame = .zero
      }
      return
    }
    /**
     CDXC:PaneTabs 2026-05-11-01:09
     Native pane tabs should always be visible, including single-pane layouts.
     Use consistent tab widths up to the configured maximum, shrink evenly down
     to the configured minimum when space is tight, then keep minimum-width
     tabs inside a clipped strip that scrolls
     horizontally from either vertical or horizontal wheel gestures.

     CDXC:PaneTabs 2026-05-11-11:47
     A single-tab pane should fit the visible viewport instead of forcing an
     oversized scrollable tab. Narrow panes need the active terminal tab's inline
     Close hit area to stay inside the pane so activation and close controls
     remain usable without horizontal scrolling.

     CDXC:PaneTabs 2026-05-15-15:08:
     Workspace pane tabs need a 170px minimum width instead of the earlier
     roughly 80px compressed width, so multi-tab groups remain readable and use
     the existing horizontal scroll strip when the pane is too narrow.
     */
    let isCommandChrome = chromeRole == .commands
    let gap: CGFloat = isCommandChrome ? 0 : 2
    let maxTabWidth: CGFloat = isCommandChrome ? 160 : 175
    let minTabWidth: CGFloat = isCommandChrome ? 72 : 170
    let tabCount = max(tabButtons.count, 1)
    let totalGap = gap * CGFloat(max(tabCount - 1, 0))
    let fittedWidth = (availableWidth - totalGap) / CGFloat(tabCount)
    let tabWidth =
      tabCount == 1
      ? min(maxTabWidth, availableWidth)
      : fittedWidth >= minTabWidth ? min(maxTabWidth, fittedWidth) : minTabWidth
    tabContentWidth = tabWidth * CGFloat(tabCount) + totalGap
    tabViewportFrame = CGRect(x: minX, y: centerY, width: availableWidth, height: height)
    tabScrollOffsetX = min(max(tabScrollOffsetX, 0), max(tabContentWidth - availableWidth, 0))
    if shouldScrollActiveTabIntoViewAfterLayout {
      scrollActiveTabIntoView(tabWidth: tabWidth, gap: gap, availableWidth: availableWidth)
      shouldScrollActiveTabIntoViewAfterLayout = false
    }
    syncTabScrollOffsetCache()
    tabClipView.frame = tabViewportFrame
    tabContentView.frame = CGRect(
      x: -tabScrollOffsetX,
      y: 0,
      width: tabContentWidth,
      height: height)
    var nextX: CGFloat = 0
    for (index, button) in tabButtons.enumerated() {
      button.frame = CGRect(x: nextX, y: 0, width: tabWidth, height: height)
      button.setShowsCommandTrailingSeparator(isCommandChrome && index < tabButtons.count - 1)
      nextX += tabWidth + gap
    }
  }

  private func scrollActiveTabIntoView(tabWidth: CGFloat, gap: CGFloat, availableWidth: CGFloat) {
    guard let activeTabSessionId,
      let activeIndex = tabItems.firstIndex(where: { $0.sessionId == activeTabSessionId }),
      availableWidth > 0,
      tabContentWidth > availableWidth
    else {
      return
    }
    let activeTabMinX = CGFloat(activeIndex) * (tabWidth + gap)
    let activeTabMaxX = activeTabMinX + tabWidth
    let visibleMinX = tabScrollOffsetX
    let visibleMaxX = tabScrollOffsetX + availableWidth
    let maxOffset = max(tabContentWidth - availableWidth, 0)
    let visibleActiveTabWidth =
      max(0, min(activeTabMaxX, visibleMaxX) - max(activeTabMinX, visibleMinX))
    let minimumUsableVisibleWidth = min(tabWidth, Self.activeTabRevealMinimumVisibleWidth)
    guard visibleActiveTabWidth < minimumUsableVisibleWidth else {
      return
    }
    let nextOffset: CGFloat
    if activeTabMinX < visibleMinX {
      nextOffset = activeTabMinX - Self.activeTabRevealScrollMargin
    } else {
      nextOffset = activeTabMaxX - availableWidth + Self.activeTabRevealScrollMargin
    }
    tabScrollOffsetX = min(max(nextOffset, 0), maxOffset)
  }

  private func layoutTabAddButton(
    maxX: CGFloat,
    centerY: CGFloat,
    size: CGFloat,
    gap: CGFloat,
    isVisible: Bool
  ) {
    guard isVisible, !tabViewportFrame.isEmpty, size > 0 else {
      hideTabAddButton()
      return
    }
    let visibleContentWidth = min(
      tabViewportFrame.width,
      max(0, tabContentWidth - tabScrollOffsetX)
    )
    let visibleLastTabMaxX = tabViewportFrame.minX + visibleContentWidth
    let preferredX = visibleLastTabMaxX + gap
    let buttonX = min(maxX - size, max(tabViewportFrame.minX, preferredX))
    guard buttonX >= bounds.minX, buttonX + size <= bounds.maxX else {
      hideTabAddButton()
      return
    }
    /*
     CDXC:PaneTabs 2026-05-14-10:10:
     Earlier non-command tab-strip add button styling treated the add control as part of the inline tab-control run.

     CDXC:PaneTabs 2026-05-15-09:31:
     The non-command tab-strip add button should use square chrome like the command pane add control, while keeping its existing tab-strip placement and hit target.
     */
    tabAddButton.chromeCornerRadius = 0
    tabAddButton.frame = CGRect(x: buttonX, y: centerY, width: size, height: size)
    tabAddButton.isHidden = false
    tabAddButton.alphaValue = 1
    tabAddButton.isEnabled = true
  }

  private func logPaneTabLayoutGeometryIfNeeded(
    canShowTabAddButton: Bool,
    tabAreaMaxX: CGFloat,
    tabViewportMaxX: CGFloat,
    tabButtonHeight: CGFloat,
    tabAddButtonSize: CGFloat,
    tabAddButtonGap: CGFloat,
    centerY: CGFloat,
    tabCenterY: CGFloat,
    buttonSize: CGFloat,
    insetX: CGFloat
  ) {
    guard !tabItems.isEmpty else {
      return
    }
    let tabClipExceedsTitleBar =
      tabClipView.frame.minY < -0.5 || tabClipView.frame.maxY > bounds.height + 0.5
    let tabAddButtonExceedsTitleBar =
      !tabAddButton.isHidden
      && (tabAddButton.frame.minY < -0.5 || tabAddButton.frame.maxY > bounds.height + 0.5)
    let titleBarShorterThanTabs = bounds.height + 0.5 < tabButtonHeight
    let signature = [
      debugOwnerSessionId ?? "",
      debugPaneKind,
      chromeRoleLogValue(),
      String(tabItems.count),
      String(canShowTabAddButton),
      String(format: "%.2f", bounds.width),
      String(format: "%.2f", bounds.height),
      String(format: "%.2f", tabClipView.frame.minY),
      String(format: "%.2f", tabClipView.frame.height),
      String(format: "%.2f", tabAddButton.frame.minY),
      String(format: "%.2f", tabAddButton.frame.height),
      String(format: "%.2f", tabViewportFrame.width),
      String(format: "%.2f", actionMenuButton.frame.width),
      String(tabClipExceedsTitleBar),
      String(tabAddButtonExceedsTitleBar),
      String(titleBarShorterThanTabs),
    ].joined(separator: "|")
    guard signature != lastLoggedPaneTabGeometrySignature else {
      return
    }
    /*
     CDXC:PaneTabs 2026-05-15-09:37:
     Debug mode should capture the actual AppKit frames that decide whether
     workspace tabs extend outside their title bar. Log only changed geometry
     signatures, including anomaly flags, so reproductions show whether the fix
     should change constants, layout math, or view ownership.
     */
    lastLoggedPaneTabGeometrySignature = signature
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.geometry.layout", details: [
      "activeTabSessionId": activeTabSessionId ?? NSNull(),
      "addButtonFrame": nativePaneTabsDebugFrame(tabAddButton.frame),
      "buttonSize": Double(buttonSize),
      "canShowTabAddButton": canShowTabAddButton,
      "centerY": Double(centerY),
      "chromeRole": chromeRoleLogValue(),
      "debugPaneKind": debugPaneKind,
      "doubleClickFrame": nativePaneTabsDebugFrame(doubleClickNewTerminalFrame),
      "insetX": Double(insetX),
      "ownerSessionId": debugOwnerSessionId ?? NSNull(),
      "tabAddButtonExceedsTitleBar": tabAddButtonExceedsTitleBar,
      "tabAddButtonGap": Double(tabAddButtonGap),
      "tabAddButtonHidden": tabAddButton.isHidden,
      "tabAddButtonSize": Double(tabAddButtonSize),
      "tabAreaMaxX": Double(tabAreaMaxX),
      "tabButtonCount": tabButtons.count,
      "tabButtonFrames": tabButtons.map { nativePaneTabsDebugFrame($0.frame) },
      "tabButtonHeight": Double(tabButtonHeight),
      "tabCenterY": Double(tabCenterY),
      "tabClipExceedsTitleBar": tabClipExceedsTitleBar,
      "tabClipFrame": nativePaneTabsDebugFrame(tabClipView.frame),
      "tabContentFrame": nativePaneTabsDebugFrame(tabContentView.frame),
      "tabContentWidth": Double(tabContentWidth),
      "tabItems": tabItems.map(\.sessionId),
      "tabScrollOffsetX": Double(tabScrollOffsetX),
      "tabViewportFrame": nativePaneTabsDebugFrame(tabViewportFrame),
      "tabViewportMaxX": Double(tabViewportMaxX),
      "titleBarBounds": nativePaneTabsDebugFrame(bounds),
      "titleBarFrame": nativePaneTabsDebugFrame(frame),
      "titleBarShorterThanTabs": titleBarShorterThanTabs,
      "verticalSlack": Double(bounds.height - tabButtonHeight),
    ])
  }

  private func chromeRoleLogValue() -> String {
    switch chromeRole {
    case .commands:
      return "commands"
    case .workspace:
      return "workspace"
    }
  }

  private func hideTabAddButton() {
    tabAddButton.frame = .zero
    tabAddButton.isHidden = true
    tabAddButton.alphaValue = 0
    tabAddButton.isEnabled = false
  }

  func setTitle(_ title: String) {
    if titleLabel.stringValue != title {
      titleLabel.stringValue = title
      needsLayout = true
    }
  }

  func setProjectEditorCompanionControls(onClose: (() -> Void)?) {
    projectEditorCompanionCloseAction = onClose
    let shouldShowControls = onClose != nil
    guard showsProjectEditorCompanionControls != shouldShowControls else {
      return
    }
    /**
     CDXC:ProjectEditorCompanion 2026-05-14-09:47:
     The embedded-editor companion controls are titlebar-local actions, not
     normal pane lifecycle actions. Show them only on the selected companion
     session so Close uses the same AppKit button/hover implementation as
     native pane titlebar controls.

     CDXC:ProjectEditorCompanion 2026-05-15-15:29:
     The selected companion titlebar now exposes Close as the only companion
     action. Removing Back to Agents View from this group keeps the code/git
     companion pane chrome focused on pane dismissal.

     CDXC:ProjectEditorCompanion 2026-05-16-11:46:
     Code, Git, and Project companion panes should not show the generic pane
     overflow menu in their titlebar. Agents view panes still use the normal
     action menu because they do not enable companion controls.
     */
    showsProjectEditorCompanionControls = shouldShowControls
    updateProjectEditorCompanionButtonVisibility()
    needsLayout = true
  }

  func setActions(_ actions: [TerminalTitleBarAction]) {
    guard actionButtons.map(\.action) != actions else {
      return
    }

    for item in actionButtons {
      item.button.removeFromSuperview()
    }
    actionButtons = actions.map { action in
      (action, Self.makeActionButton(for: action))
    }
    for item in actionButtons {
      item.button.target = self
      item.button.action = #selector(performTitleBarAction(_:))
      addSubview(item.button)
    }
    if actionMenuButton.superview == nil {
      addSubview(actionMenuButton)
    }
    syncActionSeparators()
    updateActionButtonVisibility()
    needsLayout = true
    if let window {
      window.invalidateCursorRects(for: self)
    }
  }

  func setFavicon(_ image: NSImage?) {
    faviconImage = image
    needsLayout = true
  }

  func setAgentIconDataUrl(_ dataUrl: String?, colorHex: String?) {
    guard agentIconDataUrl != dataUrl || agentIconColorHex != colorHex else {
      return
    }
    /**
     CDXC:NativeGpu 2026-05-08-16:45
     Native pane chrome receives frequent sidebar metadata syncs. Decode SVG
     masks and relayout the title bar only when the icon payload actually
     changes, otherwise repeated status updates keep AppKit layers dirty.
     */
    agentIconDataUrl = dataUrl
    agentIconColorHex = colorHex
    agentIconImage = nativePaneImage(fromDataUrl: dataUrl, isTemplate: true)
    agentIconColor = nativePaneColor(fromHex: colorHex)
    needsLayout = true
  }

  func setState(activity nextActivity: NativeTerminalActivity?) {
    guard activity != nextActivity else {
      return
    }
    activity = nextActivity
    switch nextActivity {
    case .attention:
      activityIndicatorView.isHidden = false
      activityIndicatorView.layer?.backgroundColor = Self.attentionIndicatorColor
    case .working:
      activityIndicatorView.isHidden = false
      activityIndicatorView.layer?.backgroundColor = Self.workingIndicatorColor
    case .sleeping:
      activityIndicatorView.isHidden = true
      activityIndicatorView.layer?.backgroundColor = NSColor.clear.cgColor
    case .none:
      activityIndicatorView.isHidden = true
      activityIndicatorView.layer?.backgroundColor = NSColor.clear.cgColor
    }
    needsLayout = true
  }

  @objc private func performTitleBarAction(_ sender: NSButton) {
    guard let item = actionButtons.first(where: { $0.button === sender }),
      isActionButtonVisible(item.action)
    else {
      return
    }
    onAction?(item.action)
  }

  @objc private func performProjectEditorCompanionCloseButton(_ sender: NSButton) {
    guard sender === projectEditorCompanionCloseButton, showsProjectEditorCompanionControls else {
      return
    }
    projectEditorCompanionCloseAction?()
  }

  @objc private func performTabAddButton(_ sender: NSButton) {
    guard showsTabAddButton, sender === tabAddButton, !tabAddButton.isHidden, tabAddButton.isEnabled else {
      return
    }
    onAction?(.newTerminal)
  }

  @objc private func performActionMenuButton(_ sender: NSButton) {
    showCollapsedActionMenu(from: sender, source: "buttonAction")
  }

  private func showCollapsedActionMenu(from _: NSButton, source: String) {
    /**
     CDXC:PaneTitleBarUX 2026-05-15-17:54:
     The collapsed pane hamburger menu must not include New Terminal. Terminal
     creation remains available from the tab-bar plus button, so the menu starts
     with Open Browser Pane and keeps the remaining pane/session actions.
     */
    let actions = collapsedActionMenuActions.filter { $0 != .close && $0 != .newTerminal }
    logCollapsedActionMenuEvent(
      "nativePaneActionMenu.openRequested",
      point: actionMenuButton.frame.origin,
      details: [
        "actionCount": actions.count,
        "source": source,
      ])
    guard !actions.isEmpty else {
      return
    }
    let menu = NSMenu()
    var previousActionGroup: Int?
    for action in actions {
      let actionGroup = Self.actionGroup(for: action)
      if let previousActionGroup, previousActionGroup != actionGroup {
        /**
         CDXC:PaneTitleBarUX 2026-05-12-19:06
         The always-collapsed pane action menu must retain the visual grouping
         from the former full button strip: create/open, split, session actions,
         and pop-out controls remain separated even though they now live inside
         one hamburger dropdown.
         */
        menu.addItem(NSMenuItem.separator())
      }
      let item = NSMenuItem(
        title: Self.actionMenuTitle(for: action),
        action: #selector(performCollapsedActionMenuItem(_:)),
        keyEquivalent: "")
      item.target = self
      item.representedObject = action.rawValue
      item.image = Self.actionMenuImage(for: action)
      menu.addItem(item)
      previousActionGroup = actionGroup
    }
    /**
     CDXC:PaneTitleBarUX 2026-05-12-10:58
     Collapsed hamburger clicks are owned by the visible native NSButton.
     Anchor the menu in title-bar coordinates only for placement; the click
     itself reaches this path through normal AppKit target/action dispatch.

     Present the collapsed pane-action menu with AppKit's native left-click menu
     popup from the titlebar view. Context-menu presentation is for right-click
     event streams and can ignore real left clicks, which made coordinate
     testing disagree with accessibility activation.
     */
    let menuOrigin = CGPoint(x: actionMenuButton.frame.minX, y: actionMenuButton.frame.maxY + 2)
    let didOpen = menu.popUp(positioning: nil, at: menuOrigin, in: self)
    logCollapsedActionMenuEvent(
      "nativePaneActionMenu.openFinished",
      point: actionMenuButton.frame.origin,
      details: [
        "actionCount": actions.count,
        "didOpen": didOpen,
        "source": source,
      ])
  }

  @objc private func performCollapsedActionMenuItem(_ sender: NSMenuItem) {
    guard let rawValue = sender.representedObject as? String,
      let action = TerminalTitleBarAction(rawValue: rawValue)
    else {
      return
    }
    NativePaneTabDragReproLog.append(event: "nativePaneActionMenu.itemSelected", details: [
      "action": action.rawValue
    ])
    onAction?(action)
  }

  private func logCollapsedActionMenuEvent(
    _ event: String,
    point: NSPoint,
    details: [String: Any] = [:]
  ) {
    var payload = details
    payload["actions"] = collapsedActionMenuActions.map(\.rawValue)
    payload["buttonFrame"] = nativePaneTabsDebugFrame(actionMenuButton.frame)
    payload["buttonHidden"] = actionMenuButton.isHidden
    payload["buttonEnabled"] = actionMenuButton.isEnabled
    payload["buttonAlpha"] = Double(actionMenuButton.alphaValue)
    payload["hitPoint"] = nativePaneTabsDebugFrame(CGRect(x: point.x, y: point.y, width: 0, height: 0))
    payload["isPaneHovered"] = isPaneHovered
    payload["isPointerInsideTitleBar"] = isPointerInsideTitleBar
    payload["shouldShowCollapsedActionMenu"] = shouldShowCollapsedActionMenu
    payload["tabViewportFrame"] = nativePaneTabsDebugFrame(tabViewportFrame)
    payload["titleBarBounds"] = nativePaneTabsDebugFrame(bounds)
    NativePaneTabDragReproLog.append(event: event, details: payload)
  }

  private func configureTabAddButton() {
    tabAddButton.bezelStyle = .texturedRounded
    tabAddButton.isBordered = false
    tabAddButton.imagePosition = .imageOnly
    tabAddButton.toolTip = "New Terminal"
    tabAddButton.target = self
    tabAddButton.action = #selector(performTabAddButton(_:))
    /**
     CDXC:PaneTabs 2026-05-14-09:41
     Every native terminal tab bar needs a visible local add control immediately
     after the tab run. Dispatch through the same `.newTerminal` action as the
     empty-tab-bar double-click so the sidebar can choose workspace versus
     command-surface terminal creation from the source session.
     */
    tabAddButton.sendAction(on: [.leftMouseDown])
    if let image = NSImage(systemSymbolName: "plus", accessibilityDescription: "New Terminal") {
      tabAddButton.image = image
    } else {
      tabAddButton.title = "+"
      tabAddButton.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    }
    hideTabAddButton()
  }

  private func configureActionMenuButton() {
    actionMenuButton.bezelStyle = .texturedRounded
    actionMenuButton.isBordered = false
    actionMenuButton.imagePosition = .imageOnly
    actionMenuButton.toolTip = "Pane Actions"
    actionMenuButton.target = self
    actionMenuButton.action = #selector(performActionMenuButton(_:))
    /**
     CDXC:PaneTitleBarUX 2026-05-12-10:58
     The collapsed hamburger is visible whenever layout reserves its frame and
     uses AppKit's normal mouse-up button action. Do not synthesize this through
     title-bar mouse handling; the NSButton owns the click.
     */
    actionMenuButton.sendAction(on: [.leftMouseUp])
    actionMenuButton.image = NSImage(
      systemSymbolName: "line.3.horizontal",
      accessibilityDescription: "Pane Actions")
    actionMenuButton.isHidden = true
    actionMenuButton.alphaValue = 0
    addSubview(actionMenuButton)
  }

  private func configureProjectEditorCompanionButton(
    _ button: TerminalTitleBarActionButton,
    systemSymbolName: String,
    fallbackTitle: String,
    tooltip: String,
    action: Selector
  ) {
    button.bezelStyle = .texturedRounded
    button.isBordered = false
    button.imagePosition = .imageOnly
    button.toolTip = tooltip
    button.target = self
    button.action = action
    /**
     CDXC:ProjectEditorCompanion 2026-05-14-09:47:
     Companion controls are placed in the session titlebar and dispatch on left
     mouse-down, matching existing pane action buttons so a focus change during
     the click cannot swallow the action.

     CDXC:ProjectEditorCompanion 2026-05-15-15:29:
     Close is the only companion-specific button configured here; the Back to
     Agents View control is intentionally absent from code/git companion panes.
     */
    button.sendAction(on: [.leftMouseDown])
    if let image = NSImage(systemSymbolName: systemSymbolName, accessibilityDescription: tooltip) {
      button.image = image
    } else {
      button.title = fallbackTitle
      button.font = NSFont.systemFont(ofSize: 11, weight: .bold)
    }
  }

  private func syncActionSeparators() {
    let desiredCount = Self.actionSeparatorCount(for: actionButtons.map(\.action))
    while actionSeparators.count > desiredCount {
      actionSeparators.removeLast().removeFromSuperview()
    }
    while actionSeparators.count < desiredCount {
      let separator = NSView(frame: .zero)
      separator.wantsLayer = true
      separator.layer?.backgroundColor = Self.actionSeparatorColor
      actionSeparators.append(separator)
      addSubview(separator)
    }
  }

  private func updateActionButtonVisibility() {
    /**
     CDXC:PaneTitleBarUX 2026-05-11-19:36
     Title-bar action visibility is owned by TerminalSessionTitleBarView layout,
     not by a window-local monitor. Visible action frames stay native AppKit
     buttons so clicks do not depend on hover state or titlebar-level routing.
     */
    for item in actionButtons {
      let visible = isActionButtonVisible(item.action)
      item.button.isHidden = !visible
      item.button.alphaValue = visible ? 1 : 0
      item.button.isEnabled = visible
      if let window {
        window.invalidateCursorRects(for: item.button)
      }
    }
    /**
     CDXC:PaneTitleBarUX 2026-05-11-20:33
     Right-side pane actions must be visible and hittable whenever layout reserves
     their frames. Hover-gated action visibility made the first real click reveal
     chrome instead of activating the native button, especially in narrow panes.
     */
    let shouldShowSeparators = actionButtons.count > 1
    for separator in actionSeparators {
      separator.alphaValue = shouldShowSeparators && !separator.frame.isEmpty ? 1 : 0
    }
    let menuVisible = shouldShowCollapsedActionMenu
    actionMenuButton.isHidden = !menuVisible
    actionMenuButton.alphaValue = menuVisible ? 1 : 0
    actionMenuButton.isEnabled = menuVisible
    updateProjectEditorCompanionButtonVisibility()
    if let window {
      window.invalidateCursorRects(for: actionMenuButton)
    }
  }

  private func layoutProjectEditorCompanionButtons(
    trailingX: inout CGFloat,
    centerY: CGFloat,
    buttonSize: CGFloat,
    buttonGap: CGFloat,
    insetX: CGFloat
  ) {
    guard showsProjectEditorCompanionControls else {
      hideProjectEditorCompanionButtons()
      return
    }
    for button in [projectEditorCompanionCloseButton] {
      guard trailingX - buttonSize >= insetX else {
        button.frame = .zero
        continue
      }
      trailingX -= buttonSize
      let buttonX = max(0, trailingX)
      button.frame = CGRect(
        x: buttonX,
        y: centerY,
        width: min(buttonSize, bounds.width - buttonX),
        height: buttonSize)
      trailingX -= buttonGap
    }
    updateProjectEditorCompanionButtonVisibility()
  }

  private func updateProjectEditorCompanionButtonVisibility() {
    let visible = showsProjectEditorCompanionControls
    for button in [projectEditorCompanionCloseButton] {
      button.isHidden = !visible || button.frame.isEmpty
      button.alphaValue = visible && !button.frame.isEmpty ? 1 : 0
      button.isEnabled = visible && !button.frame.isEmpty
      if let window {
        window.invalidateCursorRects(for: button)
      }
    }
  }

  private func hideProjectEditorCompanionButtons() {
    for button in [projectEditorCompanionCloseButton] {
      button.frame = .zero
      button.isHidden = true
      button.alphaValue = 0
      button.isEnabled = false
    }
  }

  private func isActionButtonVisible(_ action: TerminalTitleBarAction) -> Bool {
    !layoutHiddenActions.contains(action)
  }

  private var shouldShowCollapsedActionMenu: Bool {
    !collapsedActionMenuActions.isEmpty
  }

  private static func actionSeparatorCount(for actions: [TerminalTitleBarAction]) -> Int {
    guard actions.count > 1 else {
      return 0
    }
    var count = 0
    var previousGroup = actionGroup(for: actions[0])
    for action in actions.dropFirst() {
      let group = actionGroup(for: action)
      if group != previousGroup {
        count += 1
      }
      previousGroup = group
    }
    return count
  }

  private static func isCommandsPanelChromeActionSet(_ actions: [TerminalTitleBarAction]) -> Bool {
    if actions == [.expandCommandsPanel] {
      return true
    }
    guard actions.count == 2, actions.contains(.closeCommandsPanel) else {
      return false
    }
    return actions.contains(.pinCommandsPanel) || actions.contains(.unpinCommandsPanel)
  }

  private static func actionClusterWidth(
    for actions: [TerminalTitleBarAction],
    buttonSize: CGFloat,
    separatorGap: CGFloat,
    separatorWidth: CGFloat
  ) -> CGFloat {
    guard !actions.isEmpty else {
      return 0
    }
    return CGFloat(actions.count) * buttonSize
      + CGFloat(actionSeparatorCount(for: actions)) * (separatorGap * 2 + separatorWidth)
  }

  private static func actionGroup(for action: TerminalTitleBarAction) -> Int {
    switch action {
    case .reload, .fork, .rename, .delayedSend:
      return 0
    case .splitHorizontal, .splitVertical, .rotatePanesClockwise, .mergeAllTabs:
      return 1
    case .openBrowser, .newTerminal:
      return 2
    case .popOut, .restorePopOut:
      return 3
    case .pinCommandsPanel, .unpinCommandsPanel, .closeCommandsPanel, .expandCommandsPanel:
      return 3
    case .close, .sleep:
      return 4
    }
  }

  private static func makeActionButton(for action: TerminalTitleBarAction) -> NSButton {
    switch action {
    case .newTerminal:
      return makeActionButton(systemSymbolName: "terminal", fallbackTitle: "T", tooltip: "New Terminal")
    case .openBrowser:
      return makeActionButton(systemSymbolName: "globe", fallbackTitle: "B", tooltip: "Open Browser Pane")
    case .pinCommandsPanel:
      return makeActionButton(systemSymbolName: "pin", fallbackTitle: "P", tooltip: "Pin Commands Panel")
    case .unpinCommandsPanel:
      return makeActionButton(systemSymbolName: "pin.slash", fallbackTitle: "U", tooltip: "Unpin Commands Panel")
    case .popOut:
      return makeActionButton(systemSymbolName: "arrow.up.right.square", fallbackTitle: "P", tooltip: "Pop Out Pane")
    case .restorePopOut:
      return makeActionButton(systemSymbolName: "arrow.down.left.square", fallbackTitle: "R", tooltip: "Restore Pane")
    case .splitHorizontal:
      return makeActionButton(systemSymbolName: "rectangle.split.2x1", fallbackTitle: "S", tooltip: "Split Sideways")
    case .splitVertical:
      return makeActionButton(systemSymbolName: "rectangle.split.1x2", fallbackTitle: "D", tooltip: "Split Downwards")
    case .rotatePanesClockwise:
      return makeActionButton(systemSymbolName: "arrow.clockwise", fallbackTitle: "R", tooltip: "Rotate Panes Clockwise")
    case .mergeAllTabs:
      return makeActionButton(systemSymbolName: "rectangle.stack", fallbackTitle: "M", tooltip: "Merge All Tabs")
    case .rename:
      return makeActionButton(systemSymbolName: "pencil", fallbackTitle: "R", tooltip: "Rename Session")
    case .delayedSend:
      return makeActionButton(systemSymbolName: "clock", fallbackTitle: "D", tooltip: "Delayed Send")
    case .fork:
      return makeActionButton(systemSymbolName: "arrow.triangle.branch", fallbackTitle: "F", tooltip: "Fork Session")
    case .reload:
      return makeActionButton(systemSymbolName: "arrow.clockwise", fallbackTitle: "R", tooltip: "Reload Session")
    case .sleep:
      return makeActionButton(systemSymbolName: "moon", fallbackTitle: "S", tooltip: "Sleep Session")
    case .close:
      return makeActionButton(systemSymbolName: "xmark", fallbackTitle: "X", tooltip: "Close Session")
    case .closeCommandsPanel:
      return makeActionButton(systemSymbolName: "chevron.down", fallbackTitle: "v", tooltip: "Minimize Commands Panel")
    case .expandCommandsPanel:
      return makeActionButton(systemSymbolName: "chevron.up", fallbackTitle: "^", tooltip: "Expand Commands Panel")
    }
  }

  fileprivate static func actionMenuTitle(for action: TerminalTitleBarAction) -> String {
    switch action {
    case .newTerminal:
      return "New Terminal"
    case .openBrowser:
      return "Open Browser Pane"
    case .pinCommandsPanel:
      return "Pin Commands Panel"
    case .unpinCommandsPanel:
      return "Unpin Commands Panel"
    case .popOut:
      return "Pop Out Pane"
    case .restorePopOut:
      return "Restore Pane"
    case .splitHorizontal:
      return "Split Sideways"
    case .splitVertical:
      return "Split Downwards"
    case .rotatePanesClockwise:
      return "Rotate Panes Clockwise"
    case .mergeAllTabs:
      return "Merge all tabs"
    case .rename:
      return "Rename Session"
    case .delayedSend:
      return "Delayed Send"
    case .fork:
      return "Fork Session"
    case .reload:
      return "Reload Session"
    case .sleep:
      return "Sleep Session"
    case .close:
      return "Close Session"
    case .closeCommandsPanel:
      return "Minimize Commands Panel"
    case .expandCommandsPanel:
      return "Expand Commands Panel"
    }
  }

  fileprivate static func actionMenuImage(for action: TerminalTitleBarAction) -> NSImage? {
    let symbolName: String
    switch action {
    case .newTerminal:
      symbolName = "terminal"
    case .openBrowser:
      symbolName = "globe"
    case .pinCommandsPanel:
      symbolName = "pin"
    case .unpinCommandsPanel:
      symbolName = "pin.slash"
    case .popOut:
      symbolName = "arrow.up.right.square"
    case .restorePopOut:
      symbolName = "arrow.down.left.square"
    case .splitHorizontal:
      symbolName = "rectangle.split.2x1"
    case .splitVertical:
      symbolName = "rectangle.split.1x2"
    case .rotatePanesClockwise:
      symbolName = "arrow.clockwise"
    case .mergeAllTabs:
      symbolName = "rectangle.stack"
    case .rename:
      symbolName = "pencil"
    case .delayedSend:
      symbolName = "clock"
    case .fork:
      symbolName = "arrow.triangle.branch"
    case .reload:
      symbolName = "arrow.clockwise"
    case .sleep:
      symbolName = "moon"
    case .close:
      symbolName = "xmark"
    case .closeCommandsPanel:
      symbolName = "chevron.down"
    case .expandCommandsPanel:
      symbolName = "chevron.up"
    }
    return NSImage(systemSymbolName: symbolName, accessibilityDescription: actionMenuTitle(for: action))
  }

  private static func makeActionButton(
    systemSymbolName: String,
    fallbackTitle: String,
    tooltip: String
  ) -> NSButton {
    let button = TerminalTitleBarActionButton(title: "", target: nil, action: nil)
    button.bezelStyle = .texturedRounded
    button.isBordered = false
    button.imagePosition = .imageOnly
    /**
     CDXC:PaneTitleBarUX 2026-05-11-20:30
     Right-side pane actions are transient native title-bar chrome. Dispatch their
     AppKit target/action on left mouse-down so focus, hover, or relayout during
     the same click cannot steal the later mouse-up from Pop Out, Pop In, or the
     collapsed action menu.
     */
    button.sendAction(on: [.leftMouseDown])
    button.toolTip = tooltip
    if let image = NSImage(systemSymbolName: systemSymbolName, accessibilityDescription: tooltip) {
      button.image = image
    } else {
      button.title = fallbackTitle
      button.font = NSFont.systemFont(ofSize: 11, weight: .bold)
    }
    return button
  }
}

final class ProjectEditorInitialLoadingOverlayView: NSView {
  private let spinnerContainer = NSView(frame: .zero)
  private let spinner = NSProgressIndicator(frame: .zero)
  private let titleLabel = NSTextField(labelWithString: "")
  private let messageLabel = NSTextField(wrappingLabelWithString: "")
  private var errorMessage: String?

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    translatesAutoresizingMaskIntoConstraints = true
    autoresizesSubviews = true
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.086, green: 0.086, blue: 0.086, alpha: 1)
      .cgColor

    spinnerContainer.wantsLayer = true
    spinnerContainer.layer?.backgroundColor = NSColor(calibratedWhite: 0.17, alpha: 0.92).cgColor
    spinnerContainer.layer?.cornerRadius = 8
    spinnerContainer.layer?.borderWidth = 1
    spinnerContainer.layer?.borderColor = NSColor(calibratedWhite: 0.42, alpha: 0.24).cgColor
    addSubview(spinnerContainer)

    spinner.style = .spinning
    spinner.isIndeterminate = true
    spinner.controlSize = .regular
    spinner.isDisplayedWhenStopped = false
    spinner.usesThreadedAnimation = true
    spinner.appearance = NSAppearance(named: .darkAqua)
    spinnerContainer.addSubview(spinner)

    titleLabel.alignment = .center
    titleLabel.backgroundColor = .clear
    titleLabel.font = NSFont.systemFont(ofSize: 15, weight: .semibold)
    titleLabel.isBezeled = false
    titleLabel.isEditable = false
    titleLabel.isHidden = true
    titleLabel.isSelectable = false
    titleLabel.textColor = NSColor(calibratedWhite: 0.92, alpha: 1)
    spinnerContainer.addSubview(titleLabel)

    messageLabel.alignment = .center
    messageLabel.backgroundColor = .clear
    messageLabel.font = NSFont.systemFont(ofSize: 12, weight: .regular)
    messageLabel.isBezeled = false
    messageLabel.isEditable = false
    messageLabel.isHidden = true
    messageLabel.isSelectable = true
    messageLabel.textColor = NSColor(calibratedWhite: 0.68, alpha: 1)
    spinnerContainer.addSubview(messageLabel)
    startAnimating()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let isError = errorMessage != nil
    let containerSize =
      isError
      ? CGSize(width: min(max(bounds.width - 80, 280), 520), height: 126)
      : CGSize(width: 52, height: 52)
    spinnerContainer.frame = CGRect(
      x: floor((bounds.width - containerSize.width) / 2),
      y: floor((bounds.height - containerSize.height) / 2),
      width: containerSize.width,
      height: containerSize.height)
    if isError {
      titleLabel.frame = CGRect(x: 18, y: 78, width: containerSize.width - 36, height: 22)
      messageLabel.frame = CGRect(x: 22, y: 28, width: containerSize.width - 44, height: 46)
      return
    }
    let spinnerSize: CGFloat = 24
    spinner.frame = CGRect(
      x: floor((spinnerContainer.bounds.width - spinnerSize) / 2),
      y: floor((spinnerContainer.bounds.height - spinnerSize) / 2),
      width: spinnerSize,
      height: spinnerSize)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    /**
     CDXC:VisualOverlays 2026-05-11-20:24
     The project-editor loader is visual-only startup feedback. It remains
     click-through so a stale loader cannot block the embedded editor pane.
     */
    nil
  }

  func startAnimating() {
    errorMessage = nil
    isHidden = false
    titleLabel.isHidden = true
    messageLabel.isHidden = true
    spinner.isHidden = false
    spinner.startAnimation(nil)
    needsLayout = true
  }

  func stopAnimating() {
    spinner.stopAnimation(nil)
    isHidden = true
  }

  func showError(message: String) {
    /**
     CDXC:EditorPanes 2026-05-15-13:58:
     Code startup failures should render in the project editor page area, not
     by mutating the sidebar or titlebar Code button label. Reuse the initial
     native overlay so failed startup has a visible, project-scoped diagnosis
     without navigating Chromium to a dead localhost URL.
     */
    errorMessage = message
    isHidden = false
    titleLabel.stringValue = "VS Code failed to load"
    titleLabel.isHidden = false
    messageLabel.stringValue = message
    messageLabel.isHidden = false
    spinner.stopAnimation(nil)
    spinner.isHidden = true
    needsLayout = true
  }
}

final class WebPaneHostView: NSView, NSTextFieldDelegate {
  /**
   CDXC:BrowserFeedbackTools 2026-05-26-15:36:
   The native browser toolbar must honor the Settings-selected feedback tool.
   The previous toolbar button hard-coded React Grab, so users who selected
   Agentation still launched React Grab after app restart.
   */
  private enum BrowserFeedbackTool: String {
    case agentation
    case reactGrab = "react-grab"

    static func normalized(_ value: String?) -> BrowserFeedbackTool {
      value == BrowserFeedbackTool.reactGrab.rawValue ? .reactGrab : .agentation
    }

    var fallbackTitle: String {
      switch self {
      case .agentation:
        return "AG"
      case .reactGrab:
        return "RG"
      }
    }

    var logAction: String {
      switch self {
      case .agentation:
        return "agentation"
      case .reactGrab:
        return "reactGrab"
      }
    }

    var tooltip: String {
      switch self {
      case .agentation:
        return "Agentation"
      case .reactGrab:
        return "React Grab"
      }
    }
  }

  private enum BrowserPaneThemeMode: String {
    case system
    case light
    case dark

    var title: String {
      switch self {
      case .system:
        return "System"
      case .light:
        return "Light"
      case .dark:
        return "Dark"
      }
    }

    var symbolName: String {
      switch self {
      case .system:
        return "circle.lefthalf.filled"
      case .light:
        return "sun.max"
      case .dark:
        return "moon"
      }
    }
  }

  private static let browserToolbarHeight: CGFloat = 40
  private static let toolbarButtonSize = CGSize(width: 28, height: 28)
  private static let toolbarHorizontalPadding: CGFloat = 12
  private static let toolbarItemGap: CGFloat = 10
  private static let addressMinimumWidth: CGFloat = 180

  private let browserView: NSView
  private weak var chromiumView: GhostexCEFBrowserView?
  private weak var webView: WKWebView?
  private let showsBrowserToolbar: Bool
  private let initialLoadingOverlayView: ProjectEditorInitialLoadingOverlayView?
  private let onFocus: (() -> Void)?
  private let onOpenDevTools: (() -> Void)?
  private let onInjectFeedbackTool: (() -> Void)?
  private let onShowProfilePicker: (() -> Void)?
  private let onShowImportSettings: (() -> Void)?
  private let toolbarView = NSView(frame: .zero)
  private var browserFeedbackTool: BrowserFeedbackTool
  var chromiumLiveResizeBackingHeight: CGFloat? {
    didSet {
      if chromiumLiveResizeBackingHeight != oldValue {
        needsLayout = true
      }
    }
  }
  private let backButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "chevron.left",
    fallbackTitle: "<",
    tooltip: "Back"
  )
  private let forwardButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "chevron.right",
    fallbackTitle: ">",
    tooltip: "Forward"
  )
  private let reloadButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "arrow.clockwise",
    fallbackTitle: "R",
    tooltip: "Reload"
  )
  private let securityIcon = NSImageView(frame: .zero)
  private let addressField = NSTextField(frame: .zero)
  private let devToolsButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "wrench.and.screwdriver",
    fallbackTitle: "D",
    tooltip: "Toggle DevTools"
  )
  private let reactGrabButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "cursorarrow.click.2",
    fallbackTitle: "AG",
    tooltip: "Agentation"
  )
  private let profileButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "person.crop.circle",
    fallbackTitle: "P",
    tooltip: "Browser Profile"
  )
  private let appearanceButton = WebPaneHostView.makeToolbarButton(
    systemSymbolName: "circle.lefthalf.filled",
    fallbackTitle: "A",
    tooltip: "Toggle Page Appearance"
  )
  private var navigationObservations: [NSKeyValueObservation] = []
  private var browserThemeMode: BrowserPaneThemeMode = .system
  private var isEditingAddress = false

  init(
    browserView: NSView,
    chromiumView: GhostexCEFBrowserView? = nil,
    webView: WKWebView? = nil,
    showsBrowserToolbar: Bool = false,
    showsInitialLoadingOverlay: Bool = false,
    initialAddress: String? = nil,
    browserFeedbackTool: String? = nil,
    onFocus: (() -> Void)? = nil,
    onOpenDevTools: (() -> Void)? = nil,
    onInjectFeedbackTool: (() -> Void)? = nil,
    onShowProfilePicker: (() -> Void)? = nil,
    onShowImportSettings: (() -> Void)? = nil
  ) {
    self.browserView = browserView
    self.chromiumView = chromiumView
    self.webView = webView
    self.showsBrowserToolbar = showsBrowserToolbar
    self.browserFeedbackTool = BrowserFeedbackTool.normalized(browserFeedbackTool)
    self.initialLoadingOverlayView =
      showsInitialLoadingOverlay ? ProjectEditorInitialLoadingOverlayView(frame: .zero) : nil
    self.onFocus = onFocus
    self.onOpenDevTools = onOpenDevTools
    self.onInjectFeedbackTool = onInjectFeedbackTool
    self.onShowProfilePicker = onShowProfilePicker
    self.onShowImportSettings = onShowImportSettings
    super.init(frame: .zero)
    translatesAutoresizingMaskIntoConstraints = true
    autoresizesSubviews = true
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.086, green: 0.086, blue: 0.086, alpha: 1)
      .cgColor
    layer?.masksToBounds = true
    browserView.translatesAutoresizingMaskIntoConstraints = true
    browserView.autoresizingMask = []
    browserView.frame = bounds
    if showsBrowserToolbar {
      configureBrowserToolbar(initialAddress: initialAddress)
      addSubview(toolbarView)
    }
    addSubview(browserView)
    if let initialLoadingOverlayView {
      addSubview(initialLoadingOverlayView)
    }
    updateBrowserToolbarState()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    if showsBrowserToolbar, toolbarView.superview !== self {
      toolbarView.removeFromSuperview()
      addSubview(toolbarView)
    }
    if browserView.superview !== self {
      browserView.removeFromSuperview()
      addSubview(browserView)
    }
    let toolbarHeight: CGFloat
    let webFrame: CGRect
    if showsBrowserToolbar {
      toolbarHeight = min(Self.browserToolbarHeight, max(0, bounds.height))
      toolbarView.frame = CGRect(
        x: 0,
        y: bounds.height - toolbarHeight,
        width: bounds.width,
        height: toolbarHeight
      )
      layoutBrowserToolbar()
      webFrame = CGRect(x: 0, y: 0, width: bounds.width, height: max(0, bounds.height - toolbarHeight))
    } else {
      toolbarHeight = 0
      webFrame = bounds
    }
    let browserFrame: CGRect
    if let chromiumLiveResizeBackingHeight, chromiumView != nil {
      /**
       CDXC:ChromiumBrowserPanes 2026-05-21-12:38:
       Dragging the bottom command-pane handle can emit dozens of top-anchored editor-height changes in one gesture; pinning the CEF child origin is not enough because Chromium's macOS backing layer can still crawl during continuous resizes.
       Keep the CEF viewport at a stable top-anchored backing height while the host view clips to the live command-pane boundary, then commit the final CEF height once the drag ends.
       */
      let backingWebHeight = max(webFrame.height, chromiumLiveResizeBackingHeight - toolbarHeight)
      browserFrame = CGRect(
        x: webFrame.minX,
        y: webFrame.maxY - backingWebHeight,
        width: webFrame.width,
        height: backingWebHeight)
    } else {
      browserFrame = webFrame
    }
    browserView.frame = browserFrame
    /**
     CDXC:ChromiumBrowserPanes 2026-05-21-11:09:
     Code/Git browser hosts still shrink when the bottom command pane reserves space, so no page content is hidden behind the command pane.
     Force the embedded CEF child back to the host's local origin after that resize so repeated command-pane toggles cannot accumulate an upward compositor offset.
     */
    chromiumView?.pinHostedViewToBounds()
    layoutInitialLoadingOverlay(webFrame: webFrame)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    guard bounds.contains(point) else {
      return nil
    }
    /**
     CDXC:EditorPanes 2026-05-15-10:54:
     Project-editor hit logs showed left-edge VS Code clicks reaching the editor
     region but stopping at `WebPaneHostView` when the embedded browser child
     declined that strip. The host is layout/focus chrome, not the web content
     target; visible browser pixels must resolve to the hosted browser view so
     AppKit delivers the click into VS Code.
     */
    if showsBrowserToolbar, toolbarView.frame.contains(point) {
      let toolbarPoint = convert(point, to: toolbarView)
      let hitView = browserToolbarHitView(at: toolbarPoint)
      /**
       CDXC:BrowserToolbarDiagnostics 2026-05-16-11:05:
       Browser toolbar clicks are currently failing in both Git project views
       and normal browser panes. Log toolbar-row hit routing before any action
       callback so reproduction timestamps can distinguish lost AppKit hit
       tests from controls that receive the click but do not execute.
       */
      logBrowserToolbarInteraction("hitTest.toolbar", details: [
        "addressFieldFrame": Self.describeFrame(addressField.frame),
        "browserFrame": Self.describeFrame(browserView.frame),
        "hitTarget": toolbarTargetName(at: toolbarPoint),
        "hitView": String(describing: type(of: hitView)),
        "hostBounds": Self.describeFrame(bounds),
        "hostFrame": Self.describeFrame(frame),
        "isHidden": isHidden,
        "toolbarFrame": Self.describeFrame(toolbarView.frame),
        "toolbarPoint": Self.describePoint(toolbarPoint),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      return hitView
    }
    if browserView.frame.contains(point) {
      let browserPoint = convert(point, to: browserView)
      return browserView.hitTest(browserPoint) ?? browserView
    }
    return super.hitTest(point)
  }

  func refreshBrowserToolbar(reason: String) {
    updateBrowserToolbarState()
  }

  var browserFeedbackToolRawValue: String {
    browserFeedbackTool.rawValue
  }

  func setBrowserFeedbackTool(_ value: String?) {
    browserFeedbackTool = BrowserFeedbackTool.normalized(value)
    updateBrowserFeedbackToolButton()
  }

  func refreshHostedWebView(reason: String) {
    /**
     CDXC:EditorPanes 2026-05-08-13:02
     VS Code editor crashes during sidebar resize need before/after logging
     around hosted Chromium frame refresh, including whether refresh forces
     immediate layout/display on the CEF NSView.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.host.refresh.start", [
      "browserFrameBefore": Self.describeFrame(browserView.frame),
      "hostBounds": Self.describeFrame(bounds),
      "hostFrame": Self.describeFrame(frame),
      "reason": reason,
      "showsBrowserToolbar": showsBrowserToolbar,
      "webUrl": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    if showsBrowserToolbar, toolbarView.superview !== self {
      toolbarView.removeFromSuperview()
      addSubview(toolbarView)
    }
    if browserView.superview !== self {
      browserView.removeFromSuperview()
      addSubview(browserView)
    }
    let toolbarHeight = showsBrowserToolbar ? min(Self.browserToolbarHeight, max(0, bounds.height)) : 0
    if showsBrowserToolbar {
      toolbarView.frame = CGRect(
        x: 0,
        y: bounds.height - toolbarHeight,
        width: bounds.width,
        height: toolbarHeight
      )
      layoutBrowserToolbar()
    }
    let webFrame = CGRect(x: 0, y: 0, width: bounds.width, height: max(0, bounds.height - toolbarHeight))
    browserView.frame = webFrame
    layoutInitialLoadingOverlay(webFrame: webFrame)
    updateBrowserToolbarState()
    needsLayout = true
    needsDisplay = true
    browserView.needsLayout = true
    browserView.needsDisplay = true
    /**
     CDXC:EditorPanes 2026-05-08-13:13
     Hosted Chromium frame refresh is a state update, not a synchronous paint
     command. Forcing layout/display here can re-enter CEF while AppKit is
     already processing sidebar or workspace resize, which crashes the visible
     VS Code pane. Mark the views dirty and let the run loop flush them.
     */
    NativeT3CodePaneReproLog.append("nativeWorkspace.t3WebPane.host.refresh.end", [
      "hostFrame": Self.describeFrame(frame),
      "reason": reason,
      "webFrame": Self.describeFrame(browserView.frame),
      "webUrl": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  override func mouseDown(with event: NSEvent) {
    if showsBrowserToolbar {
      let localPoint = convert(event.locationInWindow, from: nil)
      let toolbarPoint = convert(localPoint, to: toolbarView)
      logBrowserToolbarInteraction("mouseDown.host", details: [
        "clickCount": event.clickCount,
        "eventLocation": Self.describePoint(event.locationInWindow),
        "hitTarget": toolbarView.frame.contains(localPoint) ? toolbarTargetName(at: toolbarPoint) : "outside-toolbar",
        "localPoint": Self.describePoint(localPoint),
        "toolbarPoint": Self.describePoint(toolbarPoint),
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
    }
    onFocus?()
    super.mouseDown(with: event)
  }

  func setInitialLoadingOverlayVisible(_ visible: Bool, reason: String) {
    guard let initialLoadingOverlayView else {
      return
    }
    if visible {
      initialLoadingOverlayView.startAnimating()
      layoutInitialLoadingOverlay(webFrame: browserView.frame)
    } else {
      initialLoadingOverlayView.stopAnimating()
    }
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.loadingOverlay.visibilityChanged", [
      "reason": reason,
      "visible": visible,
      "webUrl": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  func setInitialLoadingOverlayError(_ message: String, reason: String) {
    guard let initialLoadingOverlayView else {
      return
    }
    initialLoadingOverlayView.showError(message: message)
    layoutInitialLoadingOverlay(webFrame: browserView.frame)
    NativeT3CodePaneReproLog.append("nativeWorkspace.projectEditor.loadingOverlay.errorShown", [
      "message": message,
      "reason": reason,
      "webUrl": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  private func layoutInitialLoadingOverlay(webFrame: CGRect) {
    guard let initialLoadingOverlayView else {
      return
    }
    /**
     CDXC:EditorPanes 2026-05-07-08:29
     The VS Code startup loader is a native overlay above the embedded Chromium
     view. It is created only for project editor panes, does not participate in
     browser layout or code-server startup, and ignores hit testing so it cannot
     add interaction or startup latency.
     */
    if initialLoadingOverlayView.superview !== self {
      addSubview(initialLoadingOverlayView)
    }
    if subviews.last !== initialLoadingOverlayView {
      initialLoadingOverlayView.removeFromSuperview()
      addSubview(initialLoadingOverlayView)
    }
    initialLoadingOverlayView.frame = webFrame
    initialLoadingOverlayView.needsLayout = true
  }

  func controlTextDidBeginEditing(_ obj: Notification) {
    isEditingAddress = true
    logBrowserToolbarInteraction("address.beginEditing", details: [
      "addressFieldFrame": Self.describeFrame(addressField.frame),
      "fieldValue": addressField.stringValue,
      "firstResponder": (window?.firstResponder).map { String(describing: type(of: $0)) } ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
  }

  func controlTextDidEndEditing(_ obj: Notification) {
    isEditingAddress = false
    logBrowserToolbarInteraction("address.endEditing", details: [
      "fieldValue": addressField.stringValue,
      "isReturnTextMovement": isReturnTextMovement(obj),
      "movement": obj.userInfo?["NSTextMovement"] ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    if isReturnTextMovement(obj) {
      /**
       CDXC:BrowserPanes 2026-05-03-04:09
       AppKit can finish NSTextField editing on Return without sending the
       target/action first. Commit here too so typed browser URLs navigate
       instead of being overwritten by the previous WKWebView URL.
       */
      commitAddress()
      return
    }
    updateBrowserToolbarState()
  }

  private func isReturnTextMovement(_ notification: Notification) -> Bool {
    guard let movement = notification.userInfo?["NSTextMovement"] as? Int else {
      return false
    }
    return movement == NSReturnTextMovement
  }

  func control(
    _ control: NSControl,
    textView: NSTextView,
    doCommandBy commandSelector: Selector
  ) -> Bool {
    guard control === addressField else {
      return false
    }
    logBrowserToolbarInteraction("address.command", details: [
      "commandSelector": NSStringFromSelector(commandSelector),
      "fieldValue": addressField.stringValue,
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    if commandSelector == #selector(NSResponder.insertNewline(_:)) {
      /**
       CDXC:BrowserPanes 2026-05-03-03:59
       Address-bar Return must always drive pane browser navigation. Handling the
       text command directly avoids AppKit swallowing the field action after a
       page focus transition or autocomplete interaction.

       CDXC:BrowserPanes 2026-05-11-20:24
       Return/keypad Enter are owned by the address field's AppKit delegate
       command path, not a local key monitor. This keeps browser address commits
       scoped to the editing field editor and prevents unrelated key events from
       being consumed outside responder dispatch.
       */
      commitAddress()
      window?.makeFirstResponder(browserView)
      return true
    }
    if commandSelector == #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)) {
      commitAddress()
      window?.makeFirstResponder(browserView)
      return true
    }
    if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
      isEditingAddress = false
      updateBrowserToolbarState()
      window?.makeFirstResponder(browserView)
      return true
    }
    return false
  }

  private func configureBrowserToolbar(initialAddress: String?) {
    toolbarView.translatesAutoresizingMaskIntoConstraints = true
    toolbarView.autoresizesSubviews = false
    toolbarView.wantsLayer = true
    toolbarView.layer?.backgroundColor = NSColor.black.cgColor

    [backButton, forwardButton, reloadButton, reactGrabButton, profileButton, appearanceButton, devToolsButton].forEach {
      button in
      button.target = self
      toolbarView.addSubview(button)
    }
    backButton.action = #selector(goBack)
    forwardButton.action = #selector(goForward)
    reloadButton.action = #selector(reloadPage)
    devToolsButton.action = #selector(openDevTools)
    reactGrabButton.action = #selector(injectFeedbackTool)
    updateBrowserFeedbackToolButton()
    profileButton.action = #selector(showProfilePicker)
    appearanceButton.action = #selector(showAppearanceMenu)

    securityIcon.image = NSImage(systemSymbolName: "lock.fill", accessibilityDescription: "Secure connection")
    securityIcon.contentTintColor = NSColor(calibratedWhite: 0.78, alpha: 0.9)
    securityIcon.imageScaling = .scaleProportionallyDown
    toolbarView.addSubview(securityIcon)

    addressField.cell = BrowserAddressTextFieldCell(textCell: "")
    addressField.stringValue = initialAddress ?? ""
    addressField.delegate = self
    addressField.target = self
    addressField.action = #selector(commitAddress)
    addressField.isBordered = false
    addressField.drawsBackground = false
    addressField.isEditable = true
    addressField.isSelectable = true
    addressField.focusRingType = .none
    addressField.font = NSFont.systemFont(ofSize: 13, weight: .medium)
    addressField.textColor = NSColor(calibratedWhite: 0.94, alpha: 0.95)
    addressField.placeholderString = "Search or enter address"
    addressField.lineBreakMode = .byTruncatingMiddle
    addressField.cell?.lineBreakMode = .byTruncatingMiddle
    addressField.cell?.usesSingleLineMode = true
    addressField.cell?.wraps = false
    toolbarView.addSubview(addressField)

    /**
     CDXC:BrowserPanes 2026-05-02-17:03
     The address bar is native AppKit chrome for embedded browser panes. It
     normalizes typed URLs/searches and drives the pane's own browser renderer,
     keeping browser navigation inside the pane instead of opening external overlays.
     */
    if let webView {
      navigationObservations = [
        webView.observe(\.url, options: [.initial, .new]) { [weak self] _, _ in
          Task { @MainActor in self?.updateBrowserToolbarState() }
        },
        webView.observe(\.canGoBack, options: [.initial, .new]) { [weak self] _, _ in
          Task { @MainActor in self?.updateBrowserToolbarState() }
        },
        webView.observe(\.canGoForward, options: [.initial, .new]) { [weak self] _, _ in
          Task { @MainActor in self?.updateBrowserToolbarState() }
        },
        webView.observe(\.isLoading, options: [.initial, .new]) { [weak self] _, _ in
          Task { @MainActor in self?.updateBrowserToolbarState() }
        },
      ]
    }
  }

  private func layoutBrowserToolbar() {
    guard showsBrowserToolbar else {
      return
    }
    let height = toolbarView.bounds.height
    var x = Self.toolbarHorizontalPadding
    let buttonY = floor((height - Self.toolbarButtonSize.height) / 2)
    for button in [backButton, forwardButton, reloadButton] {
      button.frame = CGRect(origin: CGPoint(x: x, y: buttonY), size: Self.toolbarButtonSize)
      x += Self.toolbarButtonSize.width + Self.toolbarItemGap
    }

    /**
     CDXC:BrowserPanes 2026-05-02-17:13
     The browser address row should match the reference chrome exactly: the
     selected feedback tool, profile, theme, and DevTools live to the right of the URL field.
     Import remains a profile-menu action instead of a fifth always-visible
     toolbar button so the pane chrome does not drift from the expected layout.
     */
    let rightButtons = [reactGrabButton, profileButton, appearanceButton, devToolsButton]
    var rightX = toolbarView.bounds.width - Self.toolbarHorizontalPadding
    for button in rightButtons.reversed() {
      rightX -= Self.toolbarButtonSize.width
      button.frame = CGRect(origin: CGPoint(x: rightX, y: buttonY), size: Self.toolbarButtonSize)
      rightX -= Self.toolbarItemGap
    }

    let addressX = x + 18
    let addressRight = rightX - 14
    let availableAddressWidth = max(0, addressRight - addressX)
    /**
     CDXC:BrowserPanes 2026-05-03-01:58
     The embedded browser URL text must read like toolbar chrome, not a page
     heading. Keep the field compact and vertically centered next to the lock
     icon so long URLs do not dominate the pane.
     */
    let addressHeight: CGFloat = 20
    let addressY = floor((height - addressHeight) / 2)
    securityIcon.frame = CGRect(x: addressX, y: floor((height - 14) / 2), width: 14, height: 14)
    addressField.frame = CGRect(
      x: addressX + 22,
      y: addressY,
      width: max(0, availableAddressWidth - 22),
      height: addressHeight
    )
  }

  private func updateBrowserToolbarState() {
    guard showsBrowserToolbar else {
      return
    }
    backButton.isEnabled = canGoBack()
    forwardButton.isEnabled = canGoForward()
    reloadButton.toolTip = isPageLoading() ? "Stop Loading" : "Reload"
    let lockSymbol = URL(string: currentURLString() ?? "")?.scheme == "https" ? "lock.fill" : "globe"
    securityIcon.image = NSImage(systemSymbolName: lockSymbol, accessibilityDescription: nil)
    if !isEditingAddress {
      let currentURL = currentURLString()?.trimmingCharacters(in: .whitespacesAndNewlines)
      let existingAddress = addressField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
      /**
       CDXC:GitProjectTabs 2026-05-16-12:40:
       Git panes seed the native address field with the project's GitHub URL
       before CEF finishes its initial about:blank bootstrap. Keep that seeded
       address visible until the browser reports a real URL so first-open Git
       views do not flash or settle on an about:blank address row.
       */
      if currentURL?.lowercased() == "about:blank", !existingAddress.isEmpty {
        return
      }
      addressField.stringValue = currentURL ?? addressField.stringValue
    }
  }

  private func currentURLString() -> String? {
    chromiumView?.currentURLString ?? webView?.url?.absoluteString
  }

  private func canGoBack() -> Bool {
    chromiumView?.canGoBack ?? webView?.canGoBack ?? false
  }

  private func canGoForward() -> Bool {
    chromiumView?.canGoForward ?? webView?.canGoForward ?? false
  }

  private func isPageLoading() -> Bool {
    chromiumView?.isLoading ?? webView?.isLoading ?? false
  }

  private static func browserPaneNavigationRequest(url: URL) -> URLRequest {
    /**
     CDXC:BrowserPanes 2026-05-03-02:28
     Address-bar navigations create a fresh top-level page, just like restored
     browser panes. Ignore stale local document cache here too so sites that
     vary HTML by user agent do not display old bare-WKWebView markup until the
     user manually reloads.
     */
    URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 60)
  }

  @objc private func goBack() {
    logBrowserToolbarActionDiagnostics(action: "back", phase: "before", details: [
      "canGoBack": canGoBack(),
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onFocus?()
    if chromiumView?.canGoBack == true {
      chromiumView?.goBack()
    } else if webView?.canGoBack == true {
      webView?.goBack()
    }
    logBrowserToolbarActionDiagnostics(action: "back", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "back")
  }

  @objc private func goForward() {
    logBrowserToolbarActionDiagnostics(action: "forward", phase: "before", details: [
      "canGoForward": canGoForward(),
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onFocus?()
    if chromiumView?.canGoForward == true {
      chromiumView?.goForward()
    } else if webView?.canGoForward == true {
      webView?.goForward()
    }
    logBrowserToolbarActionDiagnostics(action: "forward", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "forward")
  }

  @objc private func reloadPage() {
    let action = isPageLoading() ? "stopLoading" : "reload"
    logBrowserToolbarActionDiagnostics(action: action, phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "isPageLoading": isPageLoading(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onFocus?()
    if isPageLoading() {
      if let chromiumView {
        chromiumView.stopLoading()
      } else {
        webView?.stopLoading()
      }
    } else {
      if let chromiumView {
        chromiumView.reload()
      } else {
        webView?.reload()
      }
    }
    logBrowserToolbarActionDiagnostics(action: action, phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: action)
  }

  @objc private func commitAddress() {
    /**
     CDXC:BrowserPanes 2026-05-03-04:36
     Address-bar commits must snapshot the edited text before focusing the pane.
     Focusing can end AppKit field editing and refresh toolbar state from the
     previous WKWebView URL, which made pasted URLs appear accepted but navigate
     back to the old page when Return was pressed.
     */
    let input = addressField.stringValue
    guard let url = Self.url(fromAddressInput: input) else {
      logBrowserToolbarInteraction("address.commit.invalid", details: [
        "input": input,
        "windowNumber": window?.windowNumber ?? NSNull(),
      ])
      NSSound.beep()
      updateBrowserToolbarState()
      return
    }
    logBrowserToolbarInteraction("address.commit", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "input": input,
      "resolvedURL": url.absoluteString,
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onFocus?()
    NativeT3CodePaneReproLog.append("nativeWorkspace.browserPane.address.commit", [
      "input": input,
      "url": url.absoluteString,
    ])
    addressField.stringValue = url.absoluteString
    if let chromiumView {
      chromiumView.loadURLString(url.absoluteString)
    } else {
      webView?.load(Self.browserPaneNavigationRequest(url: url))
    }
  }

  @objc private func openDevTools() {
    logBrowserToolbarActionDiagnostics(action: "devTools", phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onOpenDevTools?()
    logBrowserToolbarActionDiagnostics(action: "devTools", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "devTools")
  }

  private func updateBrowserFeedbackToolButton() {
    reactGrabButton.toolTip = browserFeedbackTool.tooltip
    if reactGrabButton.image == nil {
      reactGrabButton.title = browserFeedbackTool.fallbackTitle
    }
  }

  @objc private func injectFeedbackTool() {
    let action = browserFeedbackTool.logAction
    NSLog(
      "Browser feedback toolbar action: %@ url=%@",
      browserFeedbackTool.rawValue,
      currentURLString() ?? "<nil>")
    logBrowserToolbarActionDiagnostics(action: action, phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "feedbackTool": browserFeedbackTool.rawValue,
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onInjectFeedbackTool?()
    logBrowserToolbarActionDiagnostics(action: action, phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: action)
  }

  @objc private func showProfilePicker() {
    logBrowserToolbarActionDiagnostics(action: "profile", phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onShowProfilePicker?()
    logBrowserToolbarActionDiagnostics(action: "profile", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "profile")
  }

  @objc private func showAppearanceMenu() {
    logBrowserToolbarActionDiagnostics(action: "appearance", phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onFocus?()
    let menu = NSMenu(title: "Browser Theme")
    for mode in [BrowserPaneThemeMode.system, .light, .dark] {
      let item = NSMenuItem(title: mode.title, action: #selector(selectAppearanceMode(_:)), keyEquivalent: "")
      item.identifier = NSUserInterfaceItemIdentifier(mode.rawValue)
      item.target = self
      item.state = mode == browserThemeMode ? .on : .off
      menu.addItem(item)
    }
    NSMenu.popUpContextMenu(
      menu,
      with: syntheticMenuEvent(),
      for: appearanceButton
    )
    logBrowserToolbarActionDiagnostics(action: "appearance", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "appearance")
  }

  @objc private func selectAppearanceMode(_ sender: NSMenuItem) {
    guard let rawValue = sender.identifier?.rawValue,
      let mode = BrowserPaneThemeMode(rawValue: rawValue)
    else {
      return
    }
    /**
     CDXC:BrowserPanes 2026-05-02-17:32
     The browser theme top-bar control mirrors the reference System/Light/Dark
     menu. Apply the choice directly to the embedded WKWebView so compatible
     pages update in place without replacing the browser pane or using overlay UI.
     */
    browserThemeMode = mode
    if let webView {
      switch mode {
      case .system:
        webView.appearance = nil
      case .light:
        webView.appearance = NSAppearance(named: .aqua)
      case .dark:
        webView.appearance = NSAppearance(named: .darkAqua)
      }
    } else {
      /**
       CDXC:ChromiumBrowserPanes 2026-05-04-16:51
       Chromium panes should not fake WebKit's per-view AppKit appearance hook.
       CEF needs explicit page/runtime theme support to do this correctly, so
       the toolbar stores the selected mode but leaves Chromium rendering alone
       until a real Chromium theme implementation is added.
       */
    }
    appearanceButton.image = NSImage(systemSymbolName: mode.symbolName, accessibilityDescription: "Browser Theme")
  }

  @objc private func showImportSettings() {
    logBrowserToolbarActionDiagnostics(action: "importSettings", phase: "before", details: [
      "currentURL": currentURLString() ?? NSNull(),
      "windowNumber": window?.windowNumber ?? NSNull(),
    ])
    onShowImportSettings?()
    logBrowserToolbarActionDiagnostics(action: "importSettings", phase: "after")
    scheduleBrowserToolbarActionDiagnostics(action: "importSettings")
  }

  private func toolbarTargetName(at point: CGPoint) -> String {
    if backButton.frame.contains(point) {
      return "backButton"
    }
    if forwardButton.frame.contains(point) {
      return "forwardButton"
    }
    if reloadButton.frame.contains(point) {
      return "reloadButton"
    }
    if securityIcon.frame.contains(point) {
      return "securityIcon"
    }
    if addressField.frame.contains(point) {
      return "addressField"
    }
    if reactGrabButton.frame.contains(point) {
      return "feedbackToolButton"
    }
    if profileButton.frame.contains(point) {
      return "profileButton"
    }
    if appearanceButton.frame.contains(point) {
      return "appearanceButton"
    }
    if devToolsButton.frame.contains(point) {
      return "devToolsButton"
    }
    if toolbarView.bounds.contains(point) {
      return "toolbarBackground"
    }
    return "outsideToolbar"
  }

  private func browserToolbarHitView(at point: CGPoint) -> NSView {
    /**
     CDXC:BrowserToolbarDiagnostics 2026-05-16-12:45:
     Repro logs at 2026-05-16 12:41 showed toolbar hit testing correctly
     classified `backButton`, `reloadButton`, and `addressField`, but
     `toolbarView.hitTest` always returned the toolbar container `NSView`.
     Route directly to the known AppKit controls so browser toolbar clicks
     execute their button actions or start address-field editing in both Git
     project views and normal browser panes.
     */
    if backButton.frame.contains(point) {
      return backButton
    }
    if forwardButton.frame.contains(point) {
      return forwardButton
    }
    if reloadButton.frame.contains(point) {
      return reloadButton
    }
    if addressField.frame.contains(point) {
      return addressField
    }
    if reactGrabButton.frame.contains(point) {
      return reactGrabButton
    }
    if profileButton.frame.contains(point) {
      return profileButton
    }
    if appearanceButton.frame.contains(point) {
      return appearanceButton
    }
    if devToolsButton.frame.contains(point) {
      return devToolsButton
    }
    return toolbarView
  }

  private func logBrowserToolbarActionDiagnostics(
    action: String,
    phase: String,
    details: [String: Any] = [:]
  ) {
    /**
     CDXC:ChromiumBrowserPanes 2026-05-16-15:48:
     A browser-toolbar click can move CEF page contents upward even when no pane resize occurs, and a full reload can restore the correct visual position.
     Log host geometry and force a CEF/page viewport snapshot before, after, and shortly after toolbar navigation actions so the repro can show whether the click changes AppKit frames, Chromium compositor layers, or only document viewport state.
     */
    var payload = details
    payload["action"] = action
    payload["addressFieldFrame"] = Self.describeFrame(addressField.frame)
    payload["browserFrame"] = Self.describeFrame(browserView.frame)
    if browserView.window != nil {
      payload["browserFrameInWindow"] = Self.describeFrame(browserView.convert(browserView.bounds, to: nil))
    } else {
      payload["browserFrameInWindow"] = NSNull()
    }
    payload["hostBounds"] = Self.describeFrame(bounds)
    payload["hostFrame"] = Self.describeFrame(frame)
    payload["phase"] = phase
    payload["toolbarFrame"] = Self.describeFrame(toolbarView.frame)
    payload["windowNumber"] = window?.windowNumber ?? NSNull()
    logBrowserToolbarInteraction("button.\(action).\(phase)", details: payload)
    chromiumView?.emitToolbarActionDiagnostics(action: action, phase: phase)
  }

  private func scheduleBrowserToolbarActionDiagnostics(action: String) {
    let delayedPhases: [(TimeInterval, String)] = [
      (0.08, "after-80ms"),
      (0.25, "after-250ms"),
      (0.75, "after-750ms"),
    ]
    for (delay, phase) in delayedPhases {
      DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
        self?.logBrowserToolbarActionDiagnostics(action: action, phase: phase)
      }
    }
  }

  private func logBrowserToolbarInteraction(_ phase: String, details: [String: Any] = [:]) {
    var payload = details
    payload["backEnabled"] = backButton.isEnabled
    payload["currentURL"] = currentURLString() ?? NSNull()
    payload["forwardEnabled"] = forwardButton.isEnabled
    payload["isEditingAddress"] = isEditingAddress
    payload["isPageLoading"] = isPageLoading()
    payload["phase"] = phase
    payload["showsBrowserToolbar"] = showsBrowserToolbar
    NativeT3CodePaneReproLog.append("nativeWorkspace.browserToolbar.interaction", payload)
  }

  private static func url(fromAddressInput value: String) -> URL? {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else {
      return nil
    }
    if let url = URL(string: trimmed), url.scheme != nil {
      return url
    }
    if trimmed == "localhost" || trimmed.hasPrefix("localhost:") || trimmed.hasPrefix("127.0.0.1") {
      return URL(string: "http://\(trimmed)")
    }
    if trimmed.contains(".") && !trimmed.contains(" ") {
      return URL(string: "https://\(trimmed)")
    }
    var components = URLComponents(string: "https://www.google.com/search")
    components?.queryItems = [URLQueryItem(name: "q", value: trimmed)]
    return components?.url
  }

  private static func makeToolbarButton(
    systemSymbolName: String,
    fallbackTitle: String,
    tooltip: String
  ) -> NSButton {
    let button = NSButton(title: "", target: nil, action: nil)
    button.bezelStyle = .texturedRounded
    button.isBordered = false
    button.imagePosition = .imageOnly
    button.toolTip = tooltip
    button.contentTintColor = NSColor(calibratedWhite: 0.86, alpha: 0.82)
    button.focusRingType = .none
    if let image = NSImage(systemSymbolName: systemSymbolName, accessibilityDescription: tooltip) {
      button.image = image
    } else {
      button.title = fallbackTitle
      button.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    }
    return button
  }

  private func syntheticMenuEvent() -> NSEvent {
    if let currentEvent = NSApp.currentEvent {
      return currentEvent
    }
    return NSEvent.mouseEvent(
      with: .rightMouseDown,
      location: NSEvent.mouseLocation,
      modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime,
      windowNumber: window?.windowNumber ?? 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1
    )!
  }

  private static func describeFrame(_ frame: CGRect) -> [String: Double] {
    [
      "height": Double(frame.height),
      "width": Double(frame.width),
      "x": Double(frame.minX),
      "y": Double(frame.minY),
    ]
  }

  private static func describePoint(_ point: CGPoint) -> [String: Double] {
    [
      "x": Double(point.x),
      "y": Double(point.y),
    ]
  }
}

private final class PoppedOutPaneWindowController: NSWindowController, NSWindowDelegate {
  let sessionId: String
  let titleBarView: TerminalSessionTitleBarView
  private let onReattachRequested: (String) -> Void
  private let onResize: (String) -> Void
  private var isClosingProgrammatically = false

  init(
    sessionId: String,
    titleBarView: TerminalSessionTitleBarView,
    window: NSWindow,
    onReattachRequested: @escaping (String) -> Void,
    onResize: @escaping (String) -> Void
  ) {
    self.sessionId = sessionId
    self.titleBarView = titleBarView
    self.onReattachRequested = onReattachRequested
    self.onResize = onResize
    super.init(window: window)
    /**
     CDXC:PanePopOut 2026-05-11-19:10
     Popped-out panes use the same TerminalSessionTitleBarView action controls
     as in-workspace panes. Pop In must be handled by the title bar's native
     AppKit mouseDown/mouseUp path instead of a window-level event monitor so
     click ownership stays local to the control hierarchy.

     CDXC:ZmxPersistenceRefresh 2026-05-18-15:10:
     Popped-out zmx terminals need the zmx repaint request after
     their standalone NSWindow resize settles, because workspace resize
     observers do not see the popped-out window's content-frame changes.
     */
    window.delegate = self
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func closeProgrammatically() {
    isClosingProgrammatically = true
    window?.delegate = nil
    window?.close()
  }

  func windowShouldClose(_ sender: NSWindow) -> Bool {
    guard !isClosingProgrammatically else {
      return true
    }
    onReattachRequested(sessionId)
    return false
  }

  func windowDidResize(_ notification: Notification) {
    onResize(sessionId)
  }
}

private final class PoppedOutTerminalPaneContentView: NSView {
  private let scrollView: NSView
  private let searchBarView: NSView
  private let persistenceLabelView: NSView
  private let titleBarView: NSView
  private let titleBarHeight: CGFloat

  init(
    scrollView: NSView,
    searchBarView: NSView,
    persistenceLabelView: NSView,
    titleBarView: NSView,
    titleBarHeight: CGFloat
  ) {
    self.scrollView = scrollView
    self.searchBarView = searchBarView
    self.persistenceLabelView = persistenceLabelView
    self.titleBarView = titleBarView
    self.titleBarHeight = titleBarHeight
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.071, green: 0.071, blue: 0.071, alpha: 1).cgColor
    addSubview(scrollView)
    addSubview(searchBarView)
    addSubview(persistenceLabelView)
    addSubview(titleBarView)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let titleHeight = min(titleBarHeight, max(bounds.height, 0))
    titleBarView.frame = CGRect(
      x: 0,
      y: bounds.maxY - titleHeight,
      width: bounds.width,
      height: titleHeight)
    scrollView.frame = CGRect(
      x: 0,
      y: 0,
      width: bounds.width,
      height: max(bounds.height - titleHeight, 1))
    searchBarView.frame = CGRect(
      x: max(12, bounds.width - 332),
      y: max(12, bounds.height - titleHeight - 48),
      width: min(320, max(bounds.width - 24, 1)),
      height: 36)
    let labelSize = persistenceLabelView.fittingSize
    let labelWidth = min(ceil(labelSize.width), max(bounds.width - 20, 0))
    let labelHeight = min(max(ceil(labelSize.height), 14), max(bounds.height - titleHeight - 12, 0))
    persistenceLabelView.frame = persistenceLabelView.isHidden
      ? .zero
      : CGRect(
        x: max(10, bounds.maxX - labelWidth - 5),
        y: max(6, bounds.height - titleHeight - labelHeight - 6),
        width: labelWidth,
        height: labelHeight)
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    if titleBarView.frame.contains(point) {
      /**
       CDXC:PanePopOut 2026-05-11-19:10
       The popped-out terminal content view must give the custom native title
       bar first ownership of its band. This keeps Pop In and sibling pane
       actions on AppKit hit testing instead of a window monitor, even when the
       embedded terminal/search views would otherwise compete for the same
       mouse stream.
       */
      return titleBarView.hitTest(convert(point, to: titleBarView))
    }
    return super.hitTest(point)
  }
}

private final class PoppedOutWebPaneContentView: NSView {
  private let hostView: NSView
  private let titleBarView: NSView
  private let titleBarHeight: CGFloat

  init(hostView: NSView, titleBarView: NSView, titleBarHeight: CGFloat) {
    self.hostView = hostView
    self.titleBarView = titleBarView
    self.titleBarHeight = titleBarHeight
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.086, green: 0.086, blue: 0.086, alpha: 1).cgColor
    addSubview(hostView)
    addSubview(titleBarView)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let titleHeight = min(titleBarHeight, max(bounds.height, 0))
    titleBarView.frame = CGRect(
      x: 0,
      y: bounds.maxY - titleHeight,
      width: bounds.width,
      height: titleHeight)
    hostView.frame = CGRect(
      x: 0,
      y: 0,
      width: bounds.width,
      height: max(bounds.height - titleHeight, 1))
    hostView.needsLayout = true
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    if titleBarView.frame.contains(point) {
      /**
       CDXC:PanePopOut 2026-05-11-19:10
       Popped-out web panes share the terminal title-bar action contract:
       title-bar clicks are routed by AppKit hit testing to the native title
       bar, while the embedded browser surface owns only the content region.
       */
      return titleBarView.hitTest(convert(point, to: titleBarView))
    }
    return super.hitTest(point)
  }
}

private final class PoppedOutPanePlaceholderView: NSView {
  private let titleLabel = NSTextField(labelWithString: "")
  private let detailLabel = NSTextField(labelWithString: "This pane is open in a separate ghostex window.")
  private let reattachButton = NSButton(title: "Bring Back", target: nil, action: nil)
  private let onReattach: () -> Void

  init(title: String, onReattach: @escaping () -> Void) {
    self.onReattach = onReattach
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedRed: 0.071, green: 0.071, blue: 0.071, alpha: 1).cgColor
    titleLabel.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
    titleLabel.textColor = NSColor(calibratedWhite: 0.92, alpha: 0.96)
    titleLabel.alignment = .center
    titleLabel.lineBreakMode = .byTruncatingTail
    detailLabel.font = NSFont.systemFont(ofSize: 12, weight: .regular)
    detailLabel.textColor = NSColor(calibratedWhite: 0.72, alpha: 0.9)
    detailLabel.alignment = .center
    detailLabel.lineBreakMode = .byTruncatingTail
    reattachButton.target = self
    reattachButton.action = #selector(reattach)
    reattachButton.bezelStyle = .rounded
    addSubview(titleLabel)
    addSubview(detailLabel)
    addSubview(reattachButton)
    setTitle(title)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func setTitle(_ title: String) {
    titleLabel.stringValue = title
  }

  override func layout() {
    super.layout()
    let contentWidth = min(max(bounds.width - 48, 1), 460)
    let centerX = bounds.midX
    let buttonSize = reattachButton.intrinsicContentSize
    let totalHeight: CGFloat = 86
    let startY = max(16, bounds.midY + totalHeight / 2 - 20)
    titleLabel.frame = CGRect(
      x: centerX - contentWidth / 2,
      y: startY,
      width: contentWidth,
      height: 20)
    detailLabel.frame = CGRect(
      x: centerX - contentWidth / 2,
      y: startY - 26,
      width: contentWidth,
      height: 18)
    reattachButton.frame = CGRect(
      x: centerX - max(buttonSize.width, 110) / 2,
      y: startY - 66,
      width: max(buttonSize.width, 110),
      height: 28)
  }

  @objc private func reattach() {
    onReattach()
  }
}

private final class TerminalPaneScrollButton: NSButton {
  enum Direction {
    case bottom
    case top
  }

  private static let normalBackgroundColor = NSColor(
    calibratedRed: 18 / 255,
    green: 20 / 255,
    blue: 26 / 255,
    alpha: 0.82
  ).cgColor
  private static let hoverBackgroundColor = NSColor(
    calibratedRed: 30 / 255,
    green: 35 / 255,
    blue: 46 / 255,
    alpha: 0.92
  ).cgColor
  private static let borderColor = NSColor(calibratedWhite: 0.88, alpha: 0.2).cgColor
  private static let hoverBorderColor = NSColor(calibratedWhite: 0.88, alpha: 0.34).cgColor
  private static let glyphColor = NSColor(calibratedWhite: 0.96, alpha: 0.96)

  let direction: Direction
  private var hoverTrackingArea: NSTrackingArea?
  private var isPointerInside = false {
    didSet { updateChrome() }
  }
  var isVisible = false {
    didSet {
      isEnabled = isVisible
      isHidden = !isVisible
      alphaValue = isVisible ? 1 : 0
    }
  }

  override var isHighlighted: Bool {
    didSet { updateChrome() }
  }

  override var mouseDownCanMoveWindow: Bool {
    false
  }

  init(direction: Direction) {
    self.direction = direction
    super.init(frame: .zero)
    configure()
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    isVisible ? super.hitTest(point) : nil
  }

  private func configure() {
    /**
     CDXC:NativeTerminalScroll 2026-05-26-13:58:
     Scroll jump controls should read as quiet terminal overlays, not title-bar
     actions. Use icon-only circular AppKit buttons with hover/pressed chrome so
     long scrollback can be navigated without adding permanent pane chrome or
     stealing keyboard focus from the Ghostty surface.

     CDXC:NativeTerminalScroll 2026-05-26-14:05:
     The overlay icons should be chevrons only. Draw direction from the button
     action, not from the visual stack position, so scroll-to-top is chevron-up
     and scroll-to-bottom is chevron-down.
     */
    title = ""
    isBordered = false
    imagePosition = .imageOnly
    wantsLayer = true
    layer?.backgroundColor = Self.normalBackgroundColor
    layer?.borderColor = Self.borderColor
    layer?.borderWidth = 1
    layer?.cornerRadius = 18.75
    layer?.masksToBounds = false
    layer?.shadowColor = NSColor.black.cgColor
    layer?.shadowOffset = CGSize(width: 0, height: -10)
    layer?.shadowOpacity = 0.32
    layer?.shadowRadius = 22
    toolTip = direction == .bottom ? "Scroll terminal to bottom" : "Scroll terminal to top"
    isEnabled = false
    isHidden = true
    alphaValue = 0
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let hoverTrackingArea {
      removeTrackingArea(hoverTrackingArea)
    }
    let trackingArea = NSTrackingArea(
      rect: .zero,
      options: [.activeInKeyWindow, .inVisibleRect, .mouseEnteredAndExited],
      owner: self,
      userInfo: nil
    )
    hoverTrackingArea = trackingArea
    addTrackingArea(trackingArea)
  }

  override func mouseEntered(with event: NSEvent) {
    isPointerInside = true
  }

  override func mouseExited(with event: NSEvent) {
    isPointerInside = false
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    drawScrollGlyph()
  }

  private func updateChrome() {
    let active = isPointerInside || isHighlighted
    layer?.backgroundColor = active ? Self.hoverBackgroundColor : Self.normalBackgroundColor
    layer?.borderColor = active ? Self.hoverBorderColor : Self.borderColor
  }

  private func drawScrollGlyph() {
    let centerX = bounds.midX
    let centerY = bounds.midY
    let path = NSBezierPath()
    path.lineWidth = 2.2
    path.lineCapStyle = .round
    path.lineJoinStyle = .round

    switch direction {
    case .bottom:
      path.move(to: CGPoint(x: centerX - 6, y: centerY + 3))
      path.line(to: CGPoint(x: centerX, y: centerY - 4))
      path.line(to: CGPoint(x: centerX + 6, y: centerY + 3))
    case .top:
      path.move(to: CGPoint(x: centerX - 6, y: centerY - 3))
      path.line(to: CGPoint(x: centerX, y: centerY + 4))
      path.line(to: CGPoint(x: centerX + 6, y: centerY - 3))
    }

    Self.glyphColor.setStroke()
    path.stroke()
  }
}

private final class TerminalPaneLeafContainerView: NSView {
  override var isOpaque: Bool {
    false
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }
}

private final class TerminalPanePersistenceLabelView: NSTextField {
  private static let labelFont = NSFont.systemFont(ofSize: 10, weight: .medium)
  private var isEnabledBySettings = false
  private var isSuppressedByPaneState = false

  override var fittingSize: NSSize {
    guard !stringValue.isEmpty else {
      return .zero
    }
    let size = (stringValue as NSString).size(withAttributes: [.font: Self.labelFont])
    return NSSize(width: ceil(size.width) + 10, height: 16)
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    isBezeled = false
    drawsBackground = false
    isEditable = false
    isSelectable = false
    isHidden = true
    lineBreakMode = .byTruncatingTail
    font = Self.labelFont
    textColor = NSColor(calibratedWhite: 0.86, alpha: 0.24)
    alignment = .left
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  func setProvider(_ provider: NativeSessionPersistenceProvider?, sessionName: String?) {
    let nextTitle: String
    if let provider, let sessionName = sessionName?.trimmingCharacters(in: .whitespacesAndNewlines),
      !sessionName.isEmpty
    {
      nextTitle = "\(provider.rawValue) - \(sessionName)"
    } else {
      nextTitle = ""
    }
    guard stringValue != nextTitle else {
      return
    }
    /*
     CDXC:SessionPersistence 2026-05-15-09:36:
     Provider-backed terminal panes should keep their durable attach identity visible in the pane itself. Render `provider - session` as a dim bottom-left overlay so tmux, zmx, and zellij sessions are identifiable without opening sidebar card details.

     CDXC:SessionPersistence 2026-05-15-09:48:
     The persistence label should stay quieter than terminal content and pane chrome. Use low opacity so it remains available as context without competing with command output.

     CDXC:SessionPersistence 2026-05-15-09:53:
     The provider/session context should sit in the bottom-right corner, away from common left-aligned shell prompts and command output.

     CDXC:SessionPersistence 2026-05-15-09:58:
     Move the provider/session context to the top-right corner so it stays visible without sitting beside prompt input at the bottom of the terminal body.

     CDXC:SessionPersistence 2026-05-16-07:14:
     Shift the top-right provider/session context 5px farther right so the floating label aligns closer to the terminal pane edge.
     */
    stringValue = nextTitle
    updateVisibility()
    needsLayout = true
    superview?.needsLayout = true
  }

  func setSuppressed(_ isSuppressed: Bool) {
    guard isSuppressedByPaneState != isSuppressed else {
      return
    }
    /*
     CDXC:SessionPersistence 2026-05-15-09:44:
     Minimized command panes only show command-tab chrome, not terminal body metadata. Suppress the dim provider/session overlay explicitly while the command pane is collapsed so reused AppKit frames cannot leave stale `zmx - session` text visible.
     */
    isSuppressedByPaneState = isSuppressed
    updateVisibility()
    needsLayout = true
    superview?.needsLayout = true
  }

  func setEnabledBySettings(_ isEnabled: Bool) {
    guard isEnabledBySettings != isEnabled else {
      return
    }
    /*
     CDXC:SessionPersistence 2026-05-23-00:50:
     The Settings preference defaults on, but this label must still require a
     non-empty provider/session title. That keeps ordinary terminals clean while
     zmx/tmux/zellij panes can show their attach identity.
     */
    isEnabledBySettings = isEnabled
    updateVisibility()
    needsLayout = true
    superview?.needsLayout = true
  }

  private func updateVisibility() {
    isHidden = !isEnabledBySettings || isSuppressedByPaneState || stringValue.isEmpty
  }
}

private final class TerminalPaneDelayedSendLabelView: NSTextField {
  private static let labelFont = NSFont.monospacedDigitSystemFont(ofSize: 23, weight: .bold)
  private static let backgroundColor = NSColor(calibratedWhite: 0.05, alpha: 0.78)
  private static let borderColor = NSColor(calibratedWhite: 1.0, alpha: 0.12)
  private static let labelColor = NSColor(
    calibratedRed: 0xF6 / 255,
    green: 0xC9 / 255,
    blue: 0x45 / 255,
    alpha: 1.0
  )

  override var fittingSize: NSSize {
    guard !stringValue.isEmpty else {
      return .zero
    }
    let size = (stringValue as NSString).size(withAttributes: [.font: Self.labelFont])
    return NSSize(width: ceil(size.width) + 54, height: 52)
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    isBezeled = false
    drawsBackground = false
    isEditable = false
    isSelectable = false
    isHidden = true
    wantsLayer = true
    layer?.backgroundColor = Self.backgroundColor.cgColor
    layer?.borderColor = Self.borderColor.cgColor
    layer?.borderWidth = 1
    layer?.cornerRadius = 12
    layer?.masksToBounds = true
    lineBreakMode = .byTruncatingTail
    font = Self.labelFont
    textColor = Self.labelColor
    alignment = .center
    usesSingleLineMode = true
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  func setRemainingLabel(_ remainingLabel: String?) {
    let normalizedLabel = remainingLabel?.trimmingCharacters(in: .whitespacesAndNewlines)
    let nextLabel: String
    if let normalizedLabel, !normalizedLabel.isEmpty {
      nextLabel = normalizedLabel
    } else {
      nextLabel = ""
    }
    guard stringValue != nextLabel else {
      return
    }
    /*
     CDXC:DelayedSend 2026-05-17-03:14:
     Terminal panes need a larger bottom-right floating countdown for active
     Delayed Send timers, formatted as hh:mm:ss only when hours exist and
     otherwise mm:ss.

     CDXC:DelayedSend 2026-05-21-12:21:
     The pane countdown should appear over the terminal pane with a
     rounded rectangle background so it reads as an intentional timer badge
     instead of terminal output or bottom-corner persistence metadata.

     CDXC:DelayedSend 2026-05-21-12:21:
     The floating timer badge needs a larger timer and more internal padding,
     with #f6c945 text, so the countdown is legible as the primary pending
     action state inside the terminal pane.

     CDXC:DelayedSend 2026-05-23-01:51:
     Increase the visible timer badge padding by another 5px on the left and
     right and 3px on the top and bottom without changing the timer text size.
     */
    stringValue = nextLabel
    isHidden = nextLabel.isEmpty
    needsLayout = true
    superview?.needsLayout = true
  }
}

private final class CommandsPanelChromeView: NSView {
  private static let backgroundColor = NSColor(
    calibratedWhite: 0.0,
    alpha: 1.0
  )

  override var isOpaque: Bool {
    false
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = Self.backgroundColor.cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }
}

private final class TerminalWorkspacePaneResizeHandleView: NSView {
  var onMouseDown: ((NSEvent) -> Void)?
  var onMouseDragged: ((NSEvent) -> Void)?
  var onMouseUp: ((NSEvent) -> Void)?
  private var cursor: NSCursor = .arrow
  private var splitDirection = ""

  override var isOpaque: Bool {
    false
  }

  override var mouseDownCanMoveWindow: Bool {
    false
  }

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func configure(
    direction: NativeTerminalLayout.SplitDirection,
    cursor: NSCursor
  ) {
    /**
     CDXC:NativePaneResize 2026-05-11-09:39
     The splitter rail owns cursor and drag for horizontal and vertical layout
     branches. It remains visually transparent so focused pane borders and the
     workspace gap provide the only visible separation.
     CDXC:NativePaneResize 2026-05-11-10:40
     Native hover ownership on the rail view itself. This keeps cursor
     ownership on the same native object that can drag, and avoids a
     window-local resize monitor competing with sidebar resize.
     CDXC:NativePaneResize 2026-05-11-14:17
     The rail must be visually transparent in production. native-style resizing is
     represented by the real pane gap; this view only owns native hit testing,
     cursor setting, and drag delivery.
     CDXC:NativePaneResize 2026-05-13-07:23
     Match the stable sidebar divider implementation for pane splits. Cursor
     feedback comes from one AppKit cursor rect on this real five-pixel rail;
     avoid hover tracking and NSCursor push/pop stacks, which can be unbalanced
     by neighboring terminal/browser/titlebar views.
     */
    splitDirection = direction.rawValue
    layer?.backgroundColor = NSColor.clear.cgColor
    if self.cursor !== cursor {
      self.cursor = cursor
    }
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    bounds.contains(point) ? self : nil
  }

  override func resetCursorRects() {
    super.resetCursorRects()
    /**
     CDXC:NativePaneResize 2026-05-13-07:23
     Keep pane split cursor ownership as simple as the sidebar divider:
     registering the rail bounds with AppKit is the complete hover behavior.
     */
    addCursorRect(bounds, cursor: cursor)
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    /**
     CDXC:NativePaneResize 2026-05-11-09:45
     Splitter rails are visually transparent. The focused pane border and
     workspace gap remain the intended production separation.
     */
  }

  override func mouseDown(with event: NSEvent) {
    NativePaneTabDragReproLog.append(event: "nativePaneTabs.resizeRail.mouseDown", details: [
      "bounds": nativePaneTabsDebugFrame(bounds),
      "direction": splitDirection,
      "frame": nativePaneTabsDebugFrame(frame),
      "locationInWindow": nativePaneTabsDebugFrame(CGRect(
        x: event.locationInWindow.x,
        y: event.locationInWindow.y,
        width: 0,
        height: 0)),
      "windowNumber": event.window?.windowNumber ?? NSNull(),
    ])
    onMouseDown?(event)
  }

  override func mouseDragged(with event: NSEvent) {
    onMouseDragged?(event)
  }

  override func mouseUp(with event: NSEvent) {
    onMouseUp?(event)
  }
}

final class TerminalPaneBorderView: NSView {
  private enum BorderState: Equatable {
    case attention
    case focused
    case none
  }

  private static let focusedBorderColor = NSColor(
    calibratedRed: 0x73 / 255,
    green: 0x73 / 255,
    blue: 0x73 / 255,
    alpha: 0.95
  ).cgColor
  private static let commandBorderColor = NSColor(
    calibratedRed: 0x11 / 255,
    green: 0x11 / 255,
    blue: 0x11 / 255,
    alpha: 1
  ).cgColor
  private static let attentionBorderColor = NSColor(
    calibratedRed: 0x65 / 255,
    green: 0xE5 / 255,
    blue: 0x8A / 255,
    alpha: 1
  ).cgColor
  private static let roundedBottomCornerRadius: CGFloat = 12
  private static let activeBorderWidth: CGFloat = 2
  private static let inactiveCommandBorderWidth: CGFloat = 2

  private var chromeRole: TerminalPaneChromeRole = .workspace
  private var hidesInactiveCommandBorder = false
  private var roundedBottomCorner: TerminalPaneRoundedBottomCorner = .none
  private var state: BorderState = .none

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
    layer?.borderWidth = 0
    layer?.cornerRadius = 0
    layer?.masksToBounds = false
    layer?.shadowRadius = 16
    layer?.shadowOffset = .zero
    layer?.shadowOpacity = 0
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    /**
     CDXC:VisualOverlays 2026-05-11-20:24
     Pane borders are status chrome, not controls. Always return nil so borders
     and shadows cannot intercept terminal/browser clicks.
     */
    nil
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    guard let borderColor = currentBorderColor() else {
      return
    }

    let path = borderPath(in: bounds)
    borderColor.setStroke()
    path.lineWidth = currentBorderWidth()
    path.stroke()
  }

  fileprivate func setRoundedBottomCorner(_ corner: TerminalPaneRoundedBottomCorner) {
    /**
     CDXC:NativePaneChrome 2026-05-07-15:13
     The active/done pane border must have a real rounded visual bottom corner
     on the workspace side opposite the sidebar. Draw the transparent native
     overlay's border path directly instead of relying on CALayer's single
     corner border masking. In this unflipped AppKit view, visible bottom is
     min-Y, so bottom-left is min-X/min-Y and bottom-right is max-X/min-Y.
     */
    guard roundedBottomCorner != corner else {
      return
    }
    roundedBottomCorner = corner
    needsDisplay = true
  }

  fileprivate func setChromeRole(_ role: TerminalPaneChromeRole) {
    guard chromeRole != role else {
      return
    }
    chromeRole = role
    switch state {
    case .attention:
      layer?.shadowColor = Self.attentionBorderColor
    case .focused:
      layer?.shadowColor = Self.focusedBorderColor
    case .none:
      layer?.shadowColor = nil
    }
    needsDisplay = true
  }

  fileprivate func setHidesInactiveCommandBorder(_ hides: Bool) {
    guard hidesInactiveCommandBorder != hides else {
      return
    }
    hidesInactiveCommandBorder = hides
    needsDisplay = true
  }

  func setState(isFocused: Bool, isAttention: Bool) {
    /**
     CDXC:NativeSessionStatus 2026-04-27-08:02
     Native Ghostty panes are outside the React workspace DOM. Mirror the
     existing workspace UX with a selected border and a green border for
     done/attention sessions, without stealing terminal input.
     */
    let nextState: BorderState = isAttention ? .attention : isFocused ? .focused : .none
    guard nextState != state else {
      return
    }
    state = nextState
    switch nextState {
    case .attention:
      layer?.shadowColor = Self.attentionBorderColor
      layer?.shadowOpacity = 0.28
    case .focused:
      layer?.shadowColor = Self.focusedBorderColor
      layer?.shadowOpacity = 0.18
    case .none:
      layer?.shadowColor = nil
      layer?.shadowOpacity = 0
    }
    needsDisplay = true
  }

  private func currentBorderColor() -> NSColor? {
    switch state {
    case .attention:
      return NSColor(cgColor: Self.attentionBorderColor)
    case .focused:
      return NSColor(cgColor: Self.focusedBorderColor)
    case .none:
      if hidesInactiveCommandBorder {
        return nil
      }
      if chromeRole == .commands {
        return NSColor(cgColor: Self.commandBorderColor)
      }
      return nil
    }
  }

  private func currentBorderWidth() -> CGFloat {
    /*
     CDXC:NativePaneChrome 2026-05-14-07:12:
     The dev build must compile after command chrome adds a distinct inactive border width.
     Keep the branch as an explicit return because Swift block methods do not implicitly return ternary expressions.
     */
    if state == .none && hidesInactiveCommandBorder {
      return 0
    }
    return state == .none && chromeRole == .commands
      ? Self.inactiveCommandBorderWidth
      : Self.activeBorderWidth
  }

  private func borderPath(in bounds: CGRect) -> NSBezierPath {
    let inset = currentBorderWidth() / 2
    let rect = bounds.insetBy(dx: inset, dy: inset)
    let radius = roundedBottomCorner == .none
      ? 0
      : min(Self.roundedBottomCornerRadius, rect.width / 2, rect.height / 2)
    switch roundedBottomCorner {
    case .left:
      return bottomLeftRoundedBorderPath(in: rect, radius: radius)
    case .none:
      return NSBezierPath(rect: rect)
    case .right:
      return bottomRightRoundedBorderPath(in: rect, radius: radius)
    }
  }

  private func bottomLeftRoundedBorderPath(in rect: CGRect, radius: CGFloat) -> NSBezierPath {
    let path = NSBezierPath()
    path.move(to: CGPoint(x: rect.minX, y: rect.minY + radius))
    if radius > 0 {
      path.curve(
        to: CGPoint(x: rect.minX + radius, y: rect.minY),
        controlPoint1: CGPoint(x: rect.minX, y: rect.minY + radius * 0.4477),
        controlPoint2: CGPoint(x: rect.minX + radius * 0.4477, y: rect.minY)
      )
    } else {
      path.line(to: CGPoint(x: rect.minX, y: rect.minY))
    }
    path.line(to: CGPoint(x: rect.maxX, y: rect.minY))
    path.line(to: CGPoint(x: rect.maxX, y: rect.maxY))
    path.line(to: CGPoint(x: rect.minX, y: rect.maxY))
    path.close()
    return path
  }

  private func bottomRightRoundedBorderPath(in rect: CGRect, radius: CGFloat) -> NSBezierPath {
    let path = NSBezierPath()
    path.move(to: CGPoint(x: rect.minX, y: rect.minY))
    path.line(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
    if radius > 0 {
      path.curve(
        to: CGPoint(x: rect.maxX, y: rect.minY + radius),
        controlPoint1: CGPoint(x: rect.maxX - radius * 0.4477, y: rect.minY),
        controlPoint2: CGPoint(x: rect.maxX, y: rect.minY + radius * 0.4477)
      )
    }
    path.line(to: CGPoint(x: rect.maxX, y: rect.maxY))
    path.line(to: CGPoint(x: rect.minX, y: rect.maxY))
    path.close()
    return path
  }
}
