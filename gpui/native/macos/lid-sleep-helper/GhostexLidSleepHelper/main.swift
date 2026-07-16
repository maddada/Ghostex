import Foundation
import Security

private let helperLabel =
  Bundle.main.bundleIdentifier ?? ProcessInfo.processInfo.processName
private let helperConfigURL = URL(fileURLWithPath: "/Library/PrivilegedHelperTools/\(helperLabel).config.plist")
private let leaseInterval: TimeInterval = 45

private final class LidSleepHelper: NSObject, NSXPCListenerDelegate, GhostexLidSleepHelperProtocol {
  private let lock = NSLock()
  private var lidSleepPreventionEnabled = false
  private var leaseOwnerPID: Int32?
  private var leaseExpiresAt: Date?
  private var leaseTimer: Timer?

  override init() {
    super.init()
    /**
     CDXC:TitlebarKeepAwake 2026-05-28-19:28:
     The privileged helper starts from launchd as root and must never preserve a
     stale lid-close policy after reboot, helper restart, or app crash. Reset
     `disablesleep` before accepting leases, then only enable it while Ghostex
     holds an active keep-awake lease.
     */
    _ = applyDisableSleep(false)
    leaseTimer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
      self?.expireLeaseIfNeeded()
    }
  }

  func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
    guard isAuthorizedClient(connection) else {
      return false
    }
    connection.exportedInterface = NSXPCInterface(with: GhostexLidSleepHelperProtocol.self)
    connection.exportedObject = self
    connection.resume()
    return true
  }

  func setLidSleepPreventionEnabled(
    _ enabled: Bool,
    ownerPID: Int32,
    withReply reply: @escaping (Bool, String?) -> Void
  ) {
    lock.lock()
    defer { lock.unlock() }
    if enabled {
      let result = applyDisableSleep(true)
      if result.ok {
        lidSleepPreventionEnabled = true
        leaseOwnerPID = ownerPID
        leaseExpiresAt = Date().addingTimeInterval(leaseInterval)
      }
      reply(result.ok, result.error)
      return
    }
    let result = applyDisableSleep(false)
    if result.ok {
      lidSleepPreventionEnabled = false
      leaseOwnerPID = nil
      leaseExpiresAt = nil
    }
    reply(result.ok, result.error)
  }

  func heartbeat(ownerPID: Int32, withReply reply: @escaping (Bool, String?) -> Void) {
    lock.lock()
    defer { lock.unlock() }
    guard lidSleepPreventionEnabled, leaseOwnerPID == ownerPID else {
      reply(false, "no-active-lid-sleep-prevention-lease")
      return
    }
    leaseExpiresAt = Date().addingTimeInterval(leaseInterval)
    reply(true, nil)
  }

  func status(withReply reply: @escaping (Bool, Bool, String?) -> Void) {
    lock.lock()
    let enabled = lidSleepPreventionEnabled
    let hasLease = leaseOwnerPID != nil
    lock.unlock()
    reply(true, enabled && hasLease, nil)
  }

  private func expireLeaseIfNeeded() {
    lock.lock()
    let ownerPID = leaseOwnerPID
    let expiredByTime = leaseExpiresAt.map { Date() > $0 } ?? false
    let ownerExited = ownerPID.map { Darwin.kill($0, 0) != 0 } ?? false
    let shouldDisable = lidSleepPreventionEnabled && (expiredByTime || ownerExited)
    lock.unlock()
    guard shouldDisable else {
      return
    }
    _ = applyDisableSleep(false)
    lock.lock()
    lidSleepPreventionEnabled = false
    leaseOwnerPID = nil
    leaseExpiresAt = nil
    lock.unlock()
  }

  private func applyDisableSleep(_ enabled: Bool) -> (ok: Bool, error: String?) {
    /**
     CDXC:TitlebarKeepAwake 2026-05-28-19:28:
     The root helper must not run arbitrary shell commands. Keep the only
     privileged operation hardcoded to `/usr/bin/pmset -a disablesleep 0|1`.
     */
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/pmset")
    process.arguments = ["-a", "disablesleep", enabled ? "1" : "0"]
    let pipe = Pipe()
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = pipe
    process.standardError = pipe
    do {
      try process.run()
      process.waitUntilExit()
      if process.terminationStatus == 0 {
        return (true, nil)
      }
      let data = pipe.fileHandleForReading.readDataToEndOfFile()
      let output = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
      return (false, output?.isEmpty == false ? output : "pmset exited \(process.terminationStatus)")
    } catch {
      return (false, error.localizedDescription)
    }
  }

  private func isAuthorizedClient(_ connection: NSXPCConnection) -> Bool {
    guard let config = HelperAuthorizationConfig.read() else {
      return false
    }
    guard let code = clientCode(connection) else {
      return false
    }
    guard isValidClientCode(code, requirementString: config.authorizedClientRequirement) else {
      return false
    }
    guard config.allowedBundleIdentifiers.contains(clientBundleIdentifier(code)) else {
      return false
    }
    if let clientPath = clientExecutablePath(code),
      clientPath.hasPrefix(config.authorizedClientBundlePath + "/Contents/MacOS/")
    {
      return true
    }
    return false
  }

  private func clientBundleIdentifier(_ code: SecCode) -> String {
    signingInfo(code)[kSecCodeInfoIdentifier as String] as? String ?? ""
  }

  private func clientExecutablePath(_ code: SecCode) -> String? {
    if let url = signingInfo(code)[kSecCodeInfoMainExecutable as String] as? URL {
      return url.path
    }
    return nil
  }

  private func clientCode(_ connection: NSXPCConnection) -> SecCode? {
    var code: SecCode?
    let attributes = [kSecGuestAttributePid as String: connection.processIdentifier] as CFDictionary
    guard SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &code) == errSecSuccess,
      let code
    else {
      return nil
    }
    return code
  }

  private func isValidClientCode(_ code: SecCode, requirementString: String) -> Bool {
    /**
     CDXC:TitlebarKeepAwake 2026-05-28-20:18:
     The root helper accepts XPC clients only when their live code satisfies the
     Ghostex requirement captured during administrator-approved installation.
     Bundle id and path checks remain as additional narrowing, not the primary
     trust boundary.
     */
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(requirementString as CFString, SecCSFlags(), &requirement)
      == errSecSuccess,
      let requirement
    else {
      return false
    }
    return SecCodeCheckValidity(code, SecCSFlags(), requirement) == errSecSuccess
  }

  private func signingInfo(_ code: SecCode) -> [String: Any] {
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
      let staticCode
    else {
      return [:]
    }
    var info: CFDictionary?
    guard SecCodeCopySigningInformation(
      staticCode,
      SecCSFlags(rawValue: kSecCSSigningInformation),
      &info
    ) == errSecSuccess else {
      return [:]
    }
    return (info as? [String: Any]) ?? [:]
  }
}

private struct HelperAuthorizationConfig {
  let allowedBundleIdentifiers: Set<String>
  let authorizedClientBundlePath: String
  let authorizedClientRequirement: String

  static func read() -> HelperAuthorizationConfig? {
    guard let data = try? Data(contentsOf: helperConfigURL),
      let object = try? PropertyListSerialization.propertyList(from: data, options: [], format: nil)
        as? [String: Any],
      let bundlePath = object["AuthorizedClientBundlePath"] as? String,
      let requirement = object["AuthorizedClientRequirement"] as? String,
      let bundleIds = object["AuthorizedClientBundleIdentifiers"] as? [String],
      !bundlePath.isEmpty,
      !requirement.isEmpty,
      !bundleIds.isEmpty
    else {
      return nil
    }
    return HelperAuthorizationConfig(
      allowedBundleIdentifiers: Set(bundleIds),
      authorizedClientBundlePath: bundlePath,
      authorizedClientRequirement: requirement
    )
  }
}

private let helper = LidSleepHelper()
private let listener = NSXPCListener(machServiceName: helperLabel)
listener.delegate = helper
listener.resume()
RunLoop.main.run()
