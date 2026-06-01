import AppKit
import Darwin
import GhosttyKit

private func terminalCliArguments() -> [String] {
  CommandLine.arguments.dropFirst().filter { argument in
    !argument.hasPrefix("-psn_")
  }
}

private func isTerminalCliInvocation() -> Bool {
  isatty(STDIN_FILENO) == 1 || isatty(STDOUT_FILENO) == 1 || isatty(STDERR_FILENO) == 1
}

private func runBundledCli(arguments: [String]) -> Never {
  guard
    let cliScriptPath = Bundle.main.resourceURL?
      .appendingPathComponent("Web/cli/ghostex-cli.mjs").path,
    FileManager.default.fileExists(atPath: cliScriptPath)
  else {
    fputs("Ghostex CLI is missing from this app bundle. Rebuild or reinstall Ghostex.\n", stderr)
    exit(1)
  }

  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
  process.arguments = ["node", cliScriptPath] + arguments
  process.currentDirectoryURL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
  var environment = ProcessInfo.processInfo.environment
  /**
   CDXC:DevAppFlavor 2026-05-11-12:10
   LaunchServices does not preserve the shell environment that built ghostex-dev.
   Pass the bundle-derived dev home and bridge port into the bundled CLI so
   `ghostex-dev sessions` uses ~/.ghostex-dev and the dev WebSocket bridge instead
   of production state.
   */
  environment["GHOSTEX_HOME"] = GhostexAppStorage.sharedRootDirectory.path
  if Bundle.main.bundleIdentifier == "com.madda.ghostex-dev.host" {
    environment["GHOSTEX_APP_VARIANT"] = "dev"
    environment["GHOSTEX_CLI_PORT"] = "58744"
  }
  process.environment = environment
  process.standardInput = FileHandle.standardInput
  process.standardOutput = FileHandle.standardOutput
  process.standardError = FileHandle.standardError
  do {
    try process.run()
    process.waitUntilExit()
    exit(process.terminationStatus)
  } catch {
    fputs("Ghostex CLI failed to start node: \(error.localizedDescription)\n", stderr)
    exit(1)
  }
}

let cliArguments = terminalCliArguments()
if !cliArguments.isEmpty || isTerminalCliInvocation() {
  /**
   CDXC:CliSessions 2026-05-10-03:28
   The installed executable is also what shells resolve from PATH. When users
   run a CLI command, treat argv as CLI intent and proxy to the bundled Node
   CLI before AppKit, CEF, or Ghostty can launch the GUI/browser path.
   CDXC:CliBranding 2026-05-26-15:11
   Public CLI commands are `ghostex` and `gx`; the older `gtx` short alias is
   no longer preserved because setup should install only the current concise
   command when that binary name is available.
   LaunchServices `-psn_*` arguments are ignored above so Dock and Finder
   launches still start the app normally.
   CDXC:CliEntrypoint 2026-05-30-21:37
   If an install resolves `ghostex` to the app executable instead of the bundled
   shell launcher, bare terminal `ghostex` still means CLI/TUI intent. Keep the
   shell working directory intact so path commands such as `ghostex ./file`
   resolve exactly as they do through the normal launcher.
   */
  runBundledCli(arguments: cliArguments)
}

/**
 CDXC:ChromiumBrowserPanes 2026-05-04-16:38
 Chromium browser panes require CEF's NSApplication subclass before AppKit
 creates the shared application. Prepare it first; the app still reports a
 browser-pane error instead of using WebKit if the CEF runtime is not bundled.
 */
let preparedCEFApplication = GhostexCEFPrepareApplication()

/**
 CDXC:NativeTerminals 2026-04-26-07:21
 Ghostty resolves bundled themes from global runtime state created during
 `ghostty_init`. Set the embedded app's resource directory first so named
 themes from the user's Ghostty config, such as GitHub Dark Default, load
 from Ghostex.app/Contents/Resources/ghostty.
 */
if let resourcesPath = Bundle.main.resourceURL?.appendingPathComponent("ghostty").path {
  setenv("GHOSTTY_RESOURCES_DIR", resourcesPath, 1)
}

if ghostty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv) != GHOSTTY_SUCCESS {
  Ghostty.logger.critical("ghostty_init failed")
  exit(1)
}

private let app = NSApplication.shared
private let delegate = AppDelegate()

app.delegate = delegate
let runsCEFMessageLoop =
  preparedCEFApplication && GhostexCEFInitialize(Int32(CommandLine.argc), CommandLine.unsafeArgv)
if runsCEFMessageLoop {
  app.finishLaunching()
  GhostexCEFRunMessageLoop()
  GhostexCEFShutdown()
} else {
  app.run()
}
