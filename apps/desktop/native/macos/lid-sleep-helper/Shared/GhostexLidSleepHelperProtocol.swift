import Foundation

@objc(GhostexLidSleepHelperProtocol)
protocol GhostexLidSleepHelperProtocol {
  func setLidSleepPreventionEnabled(
    _ enabled: Bool,
    ownerPID: Int32,
    withReply reply: @escaping (Bool, String?) -> Void)

  func heartbeat(ownerPID: Int32, withReply reply: @escaping (Bool, String?) -> Void)

  func status(withReply reply: @escaping (Bool, Bool, String?) -> Void)
}

