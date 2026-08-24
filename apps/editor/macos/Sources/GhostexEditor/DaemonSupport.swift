import Darwin
import Foundation

let ghostexEditorProtocolVersion = 1

private let daemonUsageText = """
  Usage: GhostexEditor --daemon [--socket <path>]
  """

struct LaunchConfiguration {
  let socketPath: String
}

func writeStderr(_ text: String) {
  if let data = text.data(using: .utf8) {
    FileHandle.standardError.write(data)
  }
}

func parseLaunchConfiguration(_ arguments: [String]) -> LaunchConfiguration {
  var daemonRequested = false
  var socketOverride: String?
  var index = 1

  while index < arguments.count {
    let argument = arguments[index]
    switch argument {
    case "--daemon":
      guard !daemonRequested else {
        usageExit("Duplicate --daemon option.")
      }
      daemonRequested = true
    case "--socket":
      index += 1
      guard index < arguments.count else {
        usageExit("Missing value for --socket.")
      }
      socketOverride = arguments[index]
    default:
      usageExit(
        argument.hasPrefix("--")
          ? "Unknown option: \(argument)" : "Unexpected argument: \(argument)")
    }
    index += 1
  }

  guard daemonRequested else {
    usageExit("Missing --daemon.")
  }

  do {
    return LaunchConfiguration(socketPath: try resolveSocketPath(argumentOverride: socketOverride))
  } catch {
    usageExit(error.localizedDescription)
  }
}

func usageExit(_ message: String? = nil) -> Never {
  if let message {
    writeStderr("\(message)\n")
  }
  writeStderr(daemonUsageText)
  exit(2)
}

func standardizedFileURL(_ path: String) -> URL {
  let expanded = (path as NSString).expandingTildeInPath
  return URL(fileURLWithPath: expanded).standardizedFileURL
}

func resolveSocketPath(argumentOverride: String?) throws -> String {
  let environment = ProcessInfo.processInfo.environment
  if let override = environment["GHOSTEX_EDITOR_SOCKET"], !override.isEmpty {
    return try absoluteSocketPath(override)
  }
  if let argumentOverride, !argumentOverride.isEmpty {
    return try absoluteSocketPath(argumentOverride)
  }
  if let ghostexHome = absoluteEnvironmentPath("GHOSTEX_HOME", environment: environment) {
    return
      ghostexHome
      .appendingPathComponent("runtime", isDirectory: true)
      .appendingPathComponent("ghostex-editor.sock").path
  }
  if let runtimeDirectory = absoluteEnvironmentPath("XDG_RUNTIME_DIR", environment: environment) {
    return
      runtimeDirectory
      .appendingPathComponent("ghostex", isDirectory: true)
      .appendingPathComponent("ghostex-editor.sock").path
  }
  let stateRoot =
    absoluteEnvironmentPath("XDG_STATE_HOME", environment: environment)
    ?? FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent(".local/state", isDirectory: true)
  return
    stateRoot
    .appendingPathComponent("ghostex", isDirectory: true)
    .appendingPathComponent("runtime", isDirectory: true)
    .appendingPathComponent("ghostex-editor.sock").path
}

private func absoluteEnvironmentPath(
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

private func absoluteSocketPath(_ path: String) throws -> String {
  let expanded = (path as NSString).expandingTildeInPath
  guard (expanded as NSString).isAbsolutePath else {
    throw ghostexError("Socket path must be absolute: \(path)")
  }
  return URL(fileURLWithPath: expanded).standardizedFileURL.path
}

func ensureSocketParentDirectory(for socketPath: String) throws {
  let directory = URL(fileURLWithPath: socketPath).deletingLastPathComponent()
  var isDirectory: ObjCBool = false
  if FileManager.default.fileExists(atPath: directory.path, isDirectory: &isDirectory) {
    guard isDirectory.boolValue else {
      throw ghostexError("Socket parent exists but is not a directory: \(directory.path)")
    }
    return
  }
  try FileManager.default.createDirectory(
    at: directory,
    withIntermediateDirectories: true,
    attributes: [.posixPermissions: 0o700]
  )
}

func removeStaleSocketIfPresent(at socketPath: String) throws {
  var status = stat()
  guard lstat(socketPath, &status) == 0 else {
    if errno == ENOENT {
      return
    }
    throw posixError("Unable to inspect socket path \(socketPath)")
  }

  guard (status.st_mode & S_IFMT) == S_IFSOCK else {
    throw ghostexError("Refusing to remove non-socket file at \(socketPath)")
  }

  guard unlink(socketPath) == 0 else {
    throw posixError("Unable to remove stale socket \(socketPath)")
  }
}

func createUnixListener(at socketPath: String) throws -> Int32 {
  try ensureSocketParentDirectory(for: socketPath)
  try removeStaleSocketIfPresent(at: socketPath)

  let fileDescriptor = socket(AF_UNIX, SOCK_STREAM, 0)
  guard fileDescriptor >= 0 else {
    throw posixError("Unable to create unix socket")
  }
  disableSigPipe(fileDescriptor)

  do {
    try setCloseOnExec(fileDescriptor)
    let endpoint = try unixSocketEndpoint(path: socketPath)
    var address = endpoint.address
    let bindResult = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
        Darwin.bind(fileDescriptor, socketAddress, endpoint.length)
      }
    }
    guard bindResult == 0 else {
      throw posixError("Unable to bind socket \(socketPath)")
    }
    guard listen(fileDescriptor, 64) == 0 else {
      throw posixError("Unable to listen on socket \(socketPath)")
    }
    try setNonBlocking(fileDescriptor)
    return fileDescriptor
  } catch {
    Darwin.close(fileDescriptor)
    throw error
  }
}

func daemonResponds(at socketPath: String) -> Bool {
  let fileDescriptor = socket(AF_UNIX, SOCK_STREAM, 0)
  guard fileDescriptor >= 0 else {
    return false
  }
  disableSigPipe(fileDescriptor)
  defer {
    Darwin.close(fileDescriptor)
  }

  do {
    let endpoint = try unixSocketEndpoint(path: socketPath)
    var address = endpoint.address
    let connectResult = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
        Darwin.connect(fileDescriptor, socketAddress, endpoint.length)
      }
    }
    guard connectResult == 0 else {
      return false
    }
    guard let ping = jsonLineData(["v": ghostexEditorProtocolVersion, "type": "ping"]) else {
      return false
    }
    guard writeAll(ping, to: fileDescriptor) else {
      return false
    }
    guard let response = readLine(from: fileDescriptor, timeoutMilliseconds: 1_000),
      let object = try JSONSerialization.jsonObject(with: response) as? [String: Any],
      object["type"] as? String == "pong",
      intValue(object["v"]) == ghostexEditorProtocolVersion
    else {
      return false
    }
    return true
  } catch {
    return false
  }
}

func setNonBlocking(_ fileDescriptor: Int32) throws {
  let flags = fcntl(fileDescriptor, F_GETFL, 0)
  guard flags >= 0 else {
    throw posixError("Unable to inspect file descriptor flags")
  }
  guard fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK) == 0 else {
    throw posixError("Unable to set file descriptor non-blocking")
  }
}

func setCloseOnExec(_ fileDescriptor: Int32) throws {
  let flags = fcntl(fileDescriptor, F_GETFD, 0)
  guard flags >= 0 else {
    throw posixError("Unable to inspect file descriptor flags")
  }
  guard fcntl(fileDescriptor, F_SETFD, flags | FD_CLOEXEC) == 0 else {
    throw posixError("Unable to set close-on-exec")
  }
}

func ignoreBrokenPipeSignals() {
  signal(SIGPIPE, SIG_IGN)
}

func disableSigPipe(_ fileDescriptor: Int32) {
  var enabled: Int32 = 1
  setsockopt(
    fileDescriptor,
    SOL_SOCKET,
    SO_NOSIGPIPE,
    &enabled,
    socklen_t(MemoryLayout<Int32>.size)
  )
}

func jsonLineData(_ object: [String: Any]) -> Data? {
  guard JSONSerialization.isValidJSONObject(object),
    let data = try? JSONSerialization.data(withJSONObject: object)
  else {
    return nil
  }
  var line = data
  line.append(0x0A)
  return line
}

func intValue(_ value: Any?) -> Int? {
  if let value = value as? Int {
    return value
  }
  if let value = value as? NSNumber {
    return value.intValue
  }
  return nil
}

func boolValue(_ value: Any?) -> Bool? {
  if let value = value as? Bool {
    return value
  }
  if let value = value as? NSNumber {
    return value.boolValue
  }
  return nil
}

func writeAll(_ data: Data, to fileDescriptor: Int32) -> Bool {
  var bytesWritten = 0
  return data.withUnsafeBytes { rawBuffer in
    guard let baseAddress = rawBuffer.baseAddress else {
      return true
    }
    while bytesWritten < data.count {
      let result = Darwin.write(
        fileDescriptor,
        baseAddress.advanced(by: bytesWritten),
        data.count - bytesWritten
      )
      if result > 0 {
        bytesWritten += result
        continue
      }
      if result < 0 && errno == EINTR {
        continue
      }
      return false
    }
    return true
  }
}

private func readLine(from fileDescriptor: Int32, timeoutMilliseconds: Int32) -> Data? {
  var buffer = Data()
  let deadline = Date().addingTimeInterval(TimeInterval(timeoutMilliseconds) / 1_000)

  while Date() < deadline {
    var pollDescriptor = pollfd(fd: fileDescriptor, events: Int16(POLLIN), revents: 0)
    let remainingMilliseconds = max(1, Int32(deadline.timeIntervalSinceNow * 1_000))
    let pollResult = poll(&pollDescriptor, 1, remainingMilliseconds)
    if pollResult == 0 {
      return nil
    }
    if pollResult < 0 {
      if errno == EINTR {
        continue
      }
      return nil
    }
    guard (pollDescriptor.revents & Int16(POLLIN)) != 0 else {
      return nil
    }

    var bytes = [UInt8](repeating: 0, count: 4096)
    let byteCapacity = bytes.count
    let count = bytes.withUnsafeMutableBytes { rawBuffer in
      Darwin.read(fileDescriptor, rawBuffer.baseAddress, byteCapacity)
    }
    if count <= 0 {
      return nil
    }
    buffer.append(contentsOf: bytes.prefix(count))
    if let newlineIndex = buffer.firstIndex(of: 0x0A) {
      return buffer.prefix(upTo: newlineIndex)
    }
  }

  return nil
}

struct UnixSocketEndpoint {
  var address: sockaddr_un
  let length: socklen_t
}

func unixSocketEndpoint(path: String) throws -> UnixSocketEndpoint {
  var address = sockaddr_un()
  address.sun_family = sa_family_t(AF_UNIX)

  let pathBytes = Array(path.utf8CString)
  let maximumPathLength = MemoryLayout.size(ofValue: address.sun_path)
  guard pathBytes.count <= maximumPathLength else {
    throw ghostexError("Socket path is too long: \(path)")
  }

  withUnsafeMutablePointer(to: &address.sun_path) { pointer in
    pointer.withMemoryRebound(to: CChar.self, capacity: maximumPathLength) { rawPath in
      for offset in 0..<maximumPathLength {
        rawPath[offset] = 0
      }
      for (offset, byte) in pathBytes.enumerated() {
        rawPath[offset] = byte
      }
    }
  }

  let offset = MemoryLayout<sockaddr_un>.offset(of: \.sun_path) ?? 0
  return UnixSocketEndpoint(address: address, length: socklen_t(offset + pathBytes.count))
}

func ghostexError(_ message: String) -> NSError {
  NSError(
    domain: "GhostexEditor",
    code: 1,
    userInfo: [NSLocalizedDescriptionKey: message]
  )
}

func posixError(_ message: String) -> NSError {
  NSError(
    domain: NSPOSIXErrorDomain,
    code: Int(errno),
    userInfo: [NSLocalizedDescriptionKey: "\(message): \(String(cString: strerror(errno)))"]
  )
}
