import AppKit
import Darwin
import Foundation

let launchConfiguration = parseLaunchConfiguration(CommandLine.arguments)
ignoreBrokenPipeSignals()

if daemonResponds(at: launchConfiguration.socketPath) {
  exit(0)
}

do {
  let listenerFileDescriptor = try createUnixListener(at: launchConfiguration.socketPath)
  let app = NSApplication.shared
  let delegate = EditorDaemon(
    socketPath: launchConfiguration.socketPath,
    listenerFileDescriptor: listenerFileDescriptor
  )
  app.delegate = delegate
  app.run()
} catch {
  writeStderr("GhostexEditor: \(error.localizedDescription)\n")
  exit(2)
}
