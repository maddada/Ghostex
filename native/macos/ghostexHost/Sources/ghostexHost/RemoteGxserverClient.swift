import Foundation
import Security

private struct RemoteGxserverConnection {
  let baseURL: String
  let localPort: Int
  let remoteMachineId: String
  let token: String
  let tunnelProcess: Process
}

private struct RemoteProcessResult {
  let exitCode: Int32
  let stderr: String
  let stdout: String
}

private struct RemoteGxserverInstallTarget {
  let arch: String
  let distribution: String?
  let os: String

  var normalizedArch: String {
    normalizeRemoteInstallArch(arch)
  }

  var normalizedOS: String {
    normalizeRemoteInstallOS(os)
  }

  var displayLabel: String {
    let osLabel = distribution?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
      ? distribution!.trimmingCharacters(in: .whitespacesAndNewlines)
      : normalizedOS
    return "\(osLabel)/\(normalizedArch)"
  }
}

private struct RemoteSshAskpassScript {
  let directory: URL
  let script: URL
}

/*
 CDXC:OnDemandAssets 2026-07-02-14:10:
 Release app bundles no longer embed the Ubuntu remote gxserver payloads.
 The build seals this manifest (asset names + SHA256 checksums) inside the
 signed app, and native downloads the matching version-pinned tarball from the
 app's own GitHub release on first remote connect, verifies it against the
 sealed checksum, caches it per app version, and uploads it over scp exactly
 like the previously bundled package. Dev builds still bundle loose packages,
 which keep taking priority over this download path.
 */
private struct OnDemandResourceAsset {
  let bytes: UInt64
  let name: String
  let sha256: String
}

private struct OnDemandResourceManifest {
  let assets: [String: OnDemandResourceAsset]
  let githubRepo: String
  let version: String
}

private struct OnDemandArchiveFailure: Error {
  let message: String
  let state: String
}

final class RemoteGxserverClient {
  static let shared = RemoteGxserverClient()

  private static let keychainService = "com.madda.ghostex.remote-gxserver-token"
  private static let sshPasswordKeychainService = "com.madda.ghostex.remote-ssh-password"
  private let lock = NSLock()
  private var connections: [String: RemoteGxserverConnection] = [:]
  private var presentationSubscriptions: [String: URLSessionWebSocketTask] = [:]

  private init() {}

  private func appendRemoteInstallDebugLog(
    _ event: String,
    command: RemoteGxserverConnect,
    details: [String: Any] = [:]
  ) {
    var payload = details
    payload["hasIdentityFile"] = command.identityFile?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    payload["hasSshPort"] = (command.sshPort ?? 0) > 0
    payload["hasSshUser"] = command.sshUser?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    payload["installApproved"] = command.installApproved == true
    payload["remoteMachineId"] = command.remoteMachineId
    payload["requestId"] = command.requestId
    RemoteGxserverInstallDebugLog.append(event: event, details: payload)
  }

  private func remoteProcessDebugSummary(_ result: RemoteProcessResult) -> [String: Any] {
    [
      "exitCode": Int(result.exitCode),
      "stderrBytes": result.stderr.lengthOfBytes(using: .utf8),
      "stdoutBytes": result.stdout.lengthOfBytes(using: .utf8),
      "timedOut": result.exitCode == 124,
    ]
  }

  private func remoteInstallTargetDebugSummary(_ target: RemoteGxserverInstallTarget) -> [String: Any] {
    /*
     CDXC:RemoteMachines 2026-06-30-04:05:
     Remote install debug summaries must keep optional platform fields JSON-compatible without relying on Swift ternary inference, because missing Linux distribution values should serialize as null and still compile in Release builds.
     */
    let distribution = target.distribution?.trimmingCharacters(in: .whitespacesAndNewlines)
    let remoteDistribution: Any
    if let distribution, !distribution.isEmpty {
      remoteDistribution = distribution
    } else {
      remoteDistribution = NSNull()
    }
    return [
      "remoteArch": target.normalizedArch,
      "remoteDistribution": remoteDistribution,
      "remoteOS": target.normalizedOS,
    ]
  }

  /*
   CDXC:RemoteMachines 2026-06-03-00:18:
   Remote connection setup is native-owned: Swift runs SSH, starts or checks
   gxserver on the remote host, reads the remote token over SSH, stores that
   token in Keychain, and keeps the local tunnel process. React receives status
   only and must not read or persist remote bearer tokens.
   */
  func connect(
    _ command: RemoteGxserverConnect,
    progress: ((HostEvent) -> Void)? = nil
  ) async -> HostEvent {
    await withCheckedContinuation { continuation in
      DispatchQueue.global(qos: .utility).async {
        let event = self.connectSynchronously(command, progress: progress)
        continuation.resume(returning: event)
      }
    }
  }

  func saveSshPassword(_ command: RemoteSshPasswordSave) async -> HostEvent {
    await withCheckedContinuation { continuation in
      DispatchQueue.global(qos: .utility).async {
        let event = self.saveSshPasswordSynchronously(command)
        continuation.resume(returning: event)
      }
    }
  }

  func request(_ command: RemoteGxserverRequest) async -> HostEvent {
    await withCheckedContinuation { continuation in
      DispatchQueue.global(qos: .utility).async {
        do {
          let connection = try self.connection(for: command.remoteMachineId)
          let response = try self.performRequest(command, connection: connection)
          continuation.resume(returning: .remoteGxserverResponse(
            remoteMachineId: command.remoteMachineId,
            requestId: command.requestId,
            path: command.path,
            ok: (200..<300).contains(response.statusCode),
            statusCode: response.statusCode,
            bodyJson: response.body,
            error: nil
          ))
        } catch {
          continuation.resume(returning: .remoteGxserverResponse(
            remoteMachineId: command.remoteMachineId,
            requestId: command.requestId,
            path: command.path,
            ok: false,
            statusCode: nil,
            bodyJson: nil,
            error: error.localizedDescription
          ))
        }
      }
    }
  }

  func subscribePresentation(
    _ command: RemoteGxserverPresentationSubscribe,
    eventHandler: @escaping (HostEvent) -> Void
  ) async -> HostEvent {
    await withCheckedContinuation { continuation in
      DispatchQueue.global(qos: .utility).async {
        do {
          let connection = try self.connection(for: command.remoteMachineId)
          try self.subscribePresentationSynchronously(
            command,
            connection: connection,
            eventHandler: eventHandler
          )
          continuation.resume(returning: .remoteGxserverStatus(
            remoteMachineId: command.remoteMachineId,
            payloadJson: self.statusPayloadJson([
              "message": "Remote presentation subscription started.",
              "ok": true,
              "requestId": command.requestId,
              "state": "presentationSubscribed",
            ])
          ))
        } catch {
          continuation.resume(returning: .remoteGxserverStatus(
            remoteMachineId: command.remoteMachineId,
            payloadJson: self.statusPayloadJson([
              "message": error.localizedDescription,
              "ok": false,
              "requestId": command.requestId,
              "state": "presentationSubscribeFailed",
            ])
          ))
        }
      }
    }
  }

  func connectingStatus(remoteMachineId: String, requestId: String) -> HostEvent {
    .remoteGxserverStatus(
      remoteMachineId: remoteMachineId,
      payloadJson: statusPayloadJson([
        "message": "Connecting to remote gxserver over SSH...",
        "ok": true,
        "requestId": requestId,
        "state": "connecting",
      ])
    )
  }

  private func saveSshPasswordSynchronously(_ command: RemoteSshPasswordSave) -> HostEvent {
    let remoteMachineId = command.remoteMachineId.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !remoteMachineId.isEmpty else {
      return .remoteSshPasswordSaveResult(
        remoteMachineId: command.remoteMachineId,
        requestId: command.requestId,
        ok: false,
        hasPassword: false,
        error: "Remote machine id is missing."
      )
    }
    do {
      let password = command.password
      /*
       CDXC:RemoteMachines 2026-06-09-18:23:
       User-provided SSH passwords are never written to Remote settings or
       passed as SSH command arguments. Save non-empty values in macOS Keychain
       under the remote machine id, and treat an empty save as credential
       removal so users can clear password auth without editing Keychain.
       */
      if password.isEmpty {
        try deleteSshPasswordFromKeychain(remoteMachineId: remoteMachineId)
        return .remoteSshPasswordSaveResult(
          remoteMachineId: remoteMachineId,
          requestId: command.requestId,
          ok: true,
          hasPassword: false,
          error: nil
        )
      }
      try storeSshPasswordInKeychain(password, remoteMachineId: remoteMachineId)
      return .remoteSshPasswordSaveResult(
        remoteMachineId: remoteMachineId,
        requestId: command.requestId,
        ok: true,
        hasPassword: true,
        error: nil
      )
    } catch {
      return .remoteSshPasswordSaveResult(
        remoteMachineId: remoteMachineId,
        requestId: command.requestId,
        ok: false,
        hasPassword: keychainHasSshPassword(remoteMachineId: remoteMachineId),
        error: "macOS Keychain could not save the SSH password."
      )
    }
  }

  private func connectSynchronously(
    _ command: RemoteGxserverConnect,
    progress: ((HostEvent) -> Void)? = nil
  ) -> HostEvent {
    appendRemoteInstallDebugLog("remoteGxserver.connect.start", command: command)
    guard !command.remoteMachineId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.invalid",
        command: command,
        details: ["reason": "missingRemoteMachineId"])
      return statusEvent(command, state: "invalid", ok: false, message: "Remote machine id is missing.")
    }
    guard !command.sshHost.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.invalid",
        command: command,
        details: ["reason": "missingSshHost"])
      return statusEvent(command, state: "invalid", ok: false, message: "Remote SSH host is missing.")
    }
    let hasSavedSshPassword = keychainHasSshPassword(remoteMachineId: command.remoteMachineId)

    let target = RemoteSshTarget(
      host: command.sshHost,
      identityFile: expandedLocalPath(command.identityFile),
      port: command.sshPort,
      sshPasswordAccount: hasSavedSshPassword
        ? command.remoteMachineId
        : nil,
      user: command.sshUser?.trimmingCharacters(in: .whitespacesAndNewlines)
    )

    appendRemoteInstallDebugLog(
      "remoteGxserver.connect.targetPrepared",
      command: command,
      details: ["hasSavedSshPassword": hasSavedSshPassword])
    terminateExistingConnection(remoteMachineId: command.remoteMachineId)

    let tokenResult = runSsh(
      target: target,
      remoteCommand: remoteTokenReadCommand(),
      timeoutSeconds: 18
    )
    appendRemoteInstallDebugLog(
      "remoteGxserver.connect.tokenRead.result",
      command: command,
      details: remoteProcessDebugSummary(tokenResult))
    if tokenResult.exitCode == 127 {
      if command.installApproved == true {
        /*
         CDXC:RemoteMachines 2026-06-23-09:46:
         First-time Remote install must distinguish the Mac app's local gxserver package from a remote Ubuntu target. Probe uname before upload and select only a matching bundled package so Ghostex never copies a Darwin binary to Linux.
         */
        appendRemoteInstallDebugLog("remoteGxserver.install.approved", command: command)
        let installTargetResult = probeRemoteInstallTarget(target: target)
        appendRemoteInstallDebugLog(
          "remoteGxserver.install.probe.result",
          command: command,
          details: remoteProcessDebugSummary(installTargetResult))
        guard installTargetResult.exitCode == 0,
          let installTarget = extractRemoteInstallTarget(from: installTargetResult.stdout)
        else {
          appendRemoteInstallDebugLog(
            "remoteGxserver.install.probe.failed",
            command: command,
            details: remoteProcessDebugSummary(installTargetResult))
          return statusEvent(
            command,
            state: "installFailed",
            ok: false,
            message: "Could not identify the remote operating system and CPU before installing gxserver."
          )
        }
        appendRemoteInstallDebugLog(
          "remoteGxserver.install.target.detected",
          command: command,
          details: remoteInstallTargetDebugSummary(installTarget))
        let installResult: RemoteProcessResult
        if let packageURL = bundledGxserverPackageURL(for: installTarget) {
          appendRemoteInstallDebugLog(
            "remoteGxserver.install.package.selected",
            command: command,
            details: [
              "packageResource": packageURL.lastPathComponent,
              "packageResourceExists": FileManager.default.fileExists(atPath: packageURL.path),
            ].merging(remoteInstallTargetDebugSummary(installTarget)) { current, _ in current })
          installResult = installBundledGxserverAndReadToken(command: command, target: target, packageURL: packageURL)
        } else {
          switch onDemandGxserverArchive(for: installTarget, command: command, progress: progress) {
          case .success(let archiveURL):
            appendRemoteInstallDebugLog(
              "remoteGxserver.install.package.onDemandSelected",
              command: command,
              details: [
                "archiveName": archiveURL.lastPathComponent,
              ].merging(remoteInstallTargetDebugSummary(installTarget)) { current, _ in current })
            installResult = installGxserverArchiveAndReadToken(command: command, target: target, archiveURL: archiveURL)
          case .failure(let failure):
            appendRemoteInstallDebugLog(
              "remoteGxserver.install.package.unavailable",
              command: command,
              details: ["reason": failure.state].merging(remoteInstallTargetDebugSummary(installTarget)) { current, _ in current })
            return statusEvent(
              command,
              state: failure.state,
              ok: false,
              message: failure.message
            )
          }
        }
        appendRemoteInstallDebugLog(
          "remoteGxserver.install.result",
          command: command,
          details: remoteProcessDebugSummary(installResult))
        if installResult.exitCode != 0 {
          appendRemoteInstallDebugLog(
            "remoteGxserver.install.failed",
            command: command,
            details: remoteProcessDebugSummary(installResult))
          return statusEvent(
            command,
            state: "installFailed",
            ok: false,
            message: sanitizedProcessFailure(defaultMessage: "Remote gxserver install failed.", result: installResult)
          )
        }
        return finishConnectWithTokenResult(command: command, target: target, tokenResult: installResult)
      }
      appendRemoteInstallDebugLog("remoteGxserver.install.approvalRequired", command: command)
      return statusEvent(
        command,
        state: "installApprovalRequired",
        ok: false,
        message: "gxserver is not installed on that machine. Ask before installing the remote gxserver package."
      )
    }
    if tokenResult.exitCode != 0 {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.sshFailed",
        command: command,
        details: remoteProcessDebugSummary(tokenResult))
      return statusEvent(
        command,
        state: "sshFailed",
        ok: false,
        message: sanitizedProcessFailure(defaultMessage: "Remote gxserver SSH setup failed.", result: tokenResult)
      )
    }

    return finishConnectWithTokenResult(command: command, target: target, tokenResult: tokenResult)
  }

  private func finishConnectWithTokenResult(
    command: RemoteGxserverConnect,
    target: RemoteSshTarget,
    tokenResult: RemoteProcessResult
  ) -> HostEvent {
    appendRemoteInstallDebugLog(
      "remoteGxserver.connect.token.finishStart",
      command: command,
      details: remoteProcessDebugSummary(tokenResult))
    let token = extractRemoteAuthToken(from: tokenResult.stdout)
    guard isValidAuthToken(token) else {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.token.invalid",
        command: command,
        details: [
          "hasTokenText": !token.isEmpty,
          "tokenLength": token.count,
        ])
      return statusEvent(
        command,
        state: "tokenUnavailable",
        ok: false,
        message: "Remote gxserver token was not readable after SSH start."
      )
    }

    do {
      try storeTokenInKeychain(token, remoteMachineId: command.remoteMachineId)
      appendRemoteInstallDebugLog("remoteGxserver.connect.keychain.stored", command: command)
    } catch {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.keychain.failed",
        command: command,
        details: ["errorDomain": (error as NSError).domain, "errorCode": (error as NSError).code])
      return statusEvent(
        command,
        state: "keychainFailed",
        ok: false,
        message: "Could not store the remote gxserver token in Keychain."
      )
    }

    do {
      appendRemoteInstallDebugLog("remoteGxserver.connect.tunnel.openStart", command: command)
      let connection = try openTunnel(command: command, target: target, token: token)
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.tunnel.opened",
        command: command,
        details: ["localPort": connection.localPort])
      return statusEvent(
        command,
        state: "connected",
        ok: true,
        message: "Remote gxserver is connected.",
        extra: [
          "baseUrl": connection.baseURL,
          "localPort": connection.localPort,
          "protocolVersion": GxserverClient.protocolVersion,
        ]
      )
    } catch {
      appendRemoteInstallDebugLog(
        "remoteGxserver.connect.tunnel.failed",
        command: command,
        details: ["errorDomain": (error as NSError).domain, "errorCode": (error as NSError).code])
      return statusEvent(command, state: "tunnelFailed", ok: false, message: error.localizedDescription)
    }
  }

  private func remoteTokenReadCommand() -> String {
    """
    GHOSTEX_REMOTE_TOKEN_FILE="$HOME/.ghostex/gxserver/auth/token"; \
    GXSERVER_BIN="$HOME/.ghostex/gxserver/package/bin/gxserver"; \
    if [ ! -x "$GXSERVER_BIN" ] && [ -x "$HOME/.local/bin/gxserver" ]; then GXSERVER_BIN="$HOME/.local/bin/gxserver"; fi; \
    GHOSTEX_BIN="$HOME/.ghostex/gxserver/package/bin/ghostex"; \
    if [ ! -x "$GHOSTEX_BIN" ] && [ -x "$HOME/.local/bin/ghostex" ]; then GHOSTEX_BIN="$HOME/.local/bin/ghostex"; fi; \
    GHOSTEX_REMOTE_START_FAILED=0; \
    if [ -x "$GXSERVER_BIN" ]; then \
      "$GXSERVER_BIN" start --json >/dev/null 2>&1 || "$GXSERVER_BIN" start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; \
    elif [ -x "$GHOSTEX_BIN" ]; then \
      "$GHOSTEX_BIN" server start --json >/dev/null 2>&1 || "$GHOSTEX_BIN" server start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; \
    else \
      exit 127; \
    fi; \
    if [ ! -r "$GHOSTEX_REMOTE_TOKEN_FILE" ]; then if [ "$GHOSTEX_REMOTE_START_FAILED" = "1" ]; then exit 127; fi; exit 126; fi; \
    printf '__GHOSTEX_REMOTE_TOKEN_START__\\n'; \
    cat "$GHOSTEX_REMOTE_TOKEN_FILE"; \
    printf '\\n__GHOSTEX_REMOTE_TOKEN_END__\\n'
    """
  }

  private func remoteStopStaleGxserverListenerCommand() -> String {
    """
    # CDXC:RemoteMachines 2026-06-30-03:32: Remote install scripts are passed as one SSH argv string; keep shell escape sequences textual because an embedded NUL makes Foundation's Process launch crash before SSH can return a normal install failure.
    ghostex_remote_gxserver_port=58744
    ghostex_remote_listener_pids() {
      ss -ltnp 2>/dev/null | awk -v port=":$ghostex_remote_gxserver_port" '$0 ~ port "[[:space:]]" { while (match($0, /pid=[0-9]+/)) { print substr($0, RSTART + 4, RLENGTH - 4); $0 = substr($0, RSTART + RLENGTH) } }' || true
      lsof -nP -iTCP:$ghostex_remote_gxserver_port -sTCP:LISTEN -Fp 2>/dev/null | sed -n 's/^p//p' || true
    }
    ghostex_remote_is_gxserver_pid() {
      candidate_pid="$1"
      case "$candidate_pid" in
        ''|*[!0-9]*) return 1 ;;
      esac
      [ "$candidate_pid" -gt 0 ] 2>/dev/null || return 1
      if [ -r "/proc/$candidate_pid/cmdline" ]; then
        candidate_cmdline="$(tr '\\000' ' ' < "/proc/$candidate_pid/cmdline" 2>/dev/null || true)"
        case "$candidate_cmdline" in
          *".ghostex/gxserver/"*"gxserver"*|*"gxserver --foreground"*) return 0 ;;
        esac
      fi
      candidate_command="$(ps -p "$candidate_pid" -o command= 2>/dev/null || ps -p "$candidate_pid" -o args= 2>/dev/null || true)"
      case "$candidate_command" in
        *".ghostex/gxserver/"*"gxserver"*|*"gxserver --foreground"*) return 0 ;;
      esac
      candidate_exe="$(readlink "/proc/$candidate_pid/exe" 2>/dev/null || true)"
      case "$candidate_exe" in
        *".ghostex/gxserver/"*"/gxserver"*|*"/gxserver (deleted)"*) return 0 ;;
      esac
      return 1
    }
    ghostex_remote_wait_for_pid_exit() {
      wait_pid="$1"
      wait_count=0
      while kill -0 "$wait_pid" 2>/dev/null && [ "$wait_count" -lt 30 ]; do
        sleep 0.1
        wait_count=$((wait_count + 1))
      done
      ! kill -0 "$wait_pid" 2>/dev/null
    }
    ghostex_remote_stop_existing_gxserver() {
      if [ -x "$package_link/bin/gxserver" ]; then
        "$package_link/bin/gxserver" stop --json >/dev/null 2>&1 || "$package_link/bin/gxserver" stop >/dev/null 2>&1 || true
      fi
      for listener_pid in $(ghostex_remote_listener_pids | sort -u); do
        if ghostex_remote_is_gxserver_pid "$listener_pid"; then
          kill -TERM "$listener_pid" 2>/dev/null || true
        fi
      done
      for listener_pid in $(ghostex_remote_listener_pids | sort -u); do
        if ghostex_remote_is_gxserver_pid "$listener_pid" && ! ghostex_remote_wait_for_pid_exit "$listener_pid"; then
          if ghostex_remote_is_gxserver_pid "$listener_pid"; then
            kill -KILL "$listener_pid" 2>/dev/null || true
          fi
        fi
      done
    }
    ghostex_remote_stop_existing_gxserver
    """
  }

  private func installBundledGxserverAndReadToken(
    command: RemoteGxserverConnect,
    target: RemoteSshTarget,
    packageURL: URL
  ) -> RemoteProcessResult {
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.package.archivePrepare",
      command: command,
      details: ["packageResource": packageURL.lastPathComponent])
    let tempDirectory = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-remote-gxserver-\(UUID().uuidString)", isDirectory: true)
    let archiveURL = tempDirectory.appendingPathComponent("gxserver.tar.gz")
    do {
      try FileManager.default.createDirectory(at: tempDirectory, withIntermediateDirectories: true)
    } catch {
      appendRemoteInstallDebugLog(
        "remoteGxserver.install.package.archivePrepareFailed",
        command: command,
        details: ["errorDomain": (error as NSError).domain, "errorCode": (error as NSError).code])
      return RemoteProcessResult(exitCode: 126, stderr: "Could not prepare gxserver upload archive.", stdout: "")
    }
    defer {
      try? FileManager.default.removeItem(at: tempDirectory)
    }

    /*
     CDXC:RemoteMachines 2026-06-02-23:38:
     Approved remote install uses the app-bundled gxserver package. Native
     creates a temporary archive, copies it over SSH, installs under
     ~/.ghostex/gxserver/package, and starts gxserver from that absolute path.

     CDXC:RemoteMachines 2026-06-08-19:12:
     Remote startup now runs through the user's zsh login+interactive
     environment so app-installed gxserver and public `ghostex server` installs
     can both find user-managed Node runtimes such as mise.

     CDXC:RemoteMachines 2026-06-23-09:46:
     Ubuntu remote install must be self-contained and deterministic: unpack a
     target-matched package into a release directory, atomically retarget the
     stable package symlink, expose gxserver/zmx/zehn/bd/ghostex/gx from
     ~/.local/bin, and avoid PATH probing or broad package-directory deletion.

     CDXC:RemoteMachines 2026-06-24-05:42:
     Remote attach opens through non-login /bin/sh on Ubuntu test machines, so
     install must chmod the generated ghostex wrapper before creating public
     links and archive without macOS AppleDouble files. The wrapper must also
     resolve its symlink path because users and attach flows may invoke it from
     ~/.local/bin or /usr/local/bin instead of the package bin directory.

     CDXC:RemoteMachines 2026-06-24-20:49:
     Reinstalling gxserver from Remote Settings must recover from a machine that
     has only a stale daemon process left after its install directory or token
     was removed. Stop the previous package first when possible, then terminate
     only a verified Ghostex-owned gxserver listener on the fixed API port
     before starting the freshly uploaded package so the tunnel authenticates
     against the new token instead of an orphaned old process.
     */
    let tarResult = runProcess(
      executable: "/usr/bin/tar",
      arguments: ["-czf", archiveURL.path, "-C", packageURL.path, "."],
      environment: ["COPYFILE_DISABLE": "1"],
      timeoutSeconds: 60
    )
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.archive.result",
      command: command,
      details: remoteProcessDebugSummary(tarResult))
    if tarResult.exitCode != 0 {
      return RemoteProcessResult(exitCode: tarResult.exitCode, stderr: "Could not archive bundled gxserver package.", stdout: "")
    }
    return installGxserverArchiveAndReadToken(command: command, target: target, archiveURL: archiveURL)
  }

  private func installGxserverArchiveAndReadToken(
    command: RemoteGxserverConnect,
    target: RemoteSshTarget,
    archiveURL: URL
  ) -> RemoteProcessResult {
    let archiveAttributes = try? FileManager.default.attributesOfItem(atPath: archiveURL.path)
    let archiveBytes = (archiveAttributes?[.size] as? NSNumber)?.uint64Value ?? 0
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.archive.ready",
      command: command,
      details: ["archiveBytes": archiveBytes])

    let mkdirResult = runSsh(
      target: target,
      remoteCommand: "mkdir -p \"$HOME/.ghostex/gxserver\"",
      timeoutSeconds: 12
    )
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.remoteDirectory.result",
      command: command,
      details: remoteProcessDebugSummary(mkdirResult))
    if mkdirResult.exitCode != 0 {
      return mkdirResult
    }

    let uploadResult = runScp(
      target: target,
      localPath: archiveURL.path,
      remotePath: "~/.ghostex/gxserver/gxserver-upload.tar.gz",
      timeoutSeconds: 120
    )
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.upload.result",
      command: command,
      details: remoteProcessDebugSummary(uploadResult))
    if uploadResult.exitCode != 0 {
      return RemoteProcessResult(exitCode: uploadResult.exitCode, stderr: "Could not upload gxserver package over SSH.", stdout: "")
    }

    let releaseId = "release-\(UUID().uuidString)"
    let installCommand = """
    set -eu
    install_root="$HOME/.ghostex/gxserver"
    upload_path="$install_root/gxserver-upload.tar.gz"
    release_dir="$install_root/releases/\(releaseId)"
    package_link="$install_root/package"
    mkdir -p "$install_root/releases" "$release_dir" "$HOME/.local/bin"
    \(remoteStopStaleGxserverListenerCommand())
    tar -xzf "$upload_path" -C "$release_dir"
    if [ -e "$package_link" ] && [ ! -L "$package_link" ]; then
      mv "$package_link" "$install_root/package.backup.\(releaseId)"
    fi
    ln -sfn "$release_dir" "$package_link"
    # CDXC:RemoteUbuntuTui 2026-06-25-19:33: Bare `ghostex` on Ubuntu launches the bundled TUI through the packaged CLI, so install must treat ghostex-tui as a first-class remote tool instead of leaving users with a post-install source-build error.
    for tool in gxserver zmx zehn bd ghostex-tui; do
      if [ -f "$package_link/bin/$tool" ]; then
        chmod 755 "$package_link/bin/$tool" 2>/dev/null || true
        ln -sfn "$package_link/bin/$tool" "$HOME/.local/bin/$tool" 2>/dev/null || true
      fi
    done
    if [ -f "$package_link/code-server/lib/node" ]; then
      chmod 755 "$package_link/code-server/lib/node" 2>/dev/null || true
    fi
    ghostex_cli_source=""
    if [ -f "$package_link/CLI/ghostex-cli.mjs" ]; then
      ghostex_cli_source="$package_link/CLI/ghostex-cli.mjs"
    elif [ -f "$package_link/cli/ghostex-cli.mjs" ]; then
      ghostex_cli_source="$package_link/cli/ghostex-cli.mjs"
    fi
    ghostex_cli_wrapper_written=0
    if [ -n "$ghostex_cli_source" ] && [ -x "$package_link/code-server/lib/node" ]; then
      cat > "$package_link/bin/ghostex" <<'__GHOSTEX_REMOTE_CLI__'
    #!/bin/sh
    set -eu
    SOURCE="$0"
    while [ -L "$SOURCE" ]; do
      SOURCE_DIR="$(CDPATH= cd -- "$(dirname -- "$SOURCE")" && pwd)"
      SOURCE_TARGET="$(readlink "$SOURCE")"
      case "$SOURCE_TARGET" in
        /*) SOURCE="$SOURCE_TARGET" ;;
        *) SOURCE="$SOURCE_DIR/$SOURCE_TARGET" ;;
      esac
    done
    HERE="$(CDPATH= cd -- "$(dirname -- "$SOURCE")" && pwd)"
    if [ -f "$HERE/../CLI/ghostex-cli.mjs" ]; then
      exec "$HERE/../code-server/lib/node" "$HERE/../CLI/ghostex-cli.mjs" "$@"
    fi
    exec "$HERE/../code-server/lib/node" "$HERE/../cli/ghostex-cli.mjs" "$@"
    __GHOSTEX_REMOTE_CLI__
      ghostex_cli_wrapper_written=1
    fi
    if [ -f "$package_link/bin/ghostex" ]; then
      chmod 755 "$package_link/bin/ghostex" 2>/dev/null || true
      ln -sfn "$package_link/bin/ghostex" "$HOME/.local/bin/ghostex" 2>/dev/null || true
      if [ "$ghostex_cli_wrapper_written" = "1" ] || [ ! -x "$package_link/bin/gx" ]; then
        ln -sfn "$package_link/bin/ghostex" "$package_link/bin/gx" 2>/dev/null || true
      fi
    fi
    if [ -x "$package_link/bin/gx" ]; then
      chmod 755 "$package_link/bin/gx" 2>/dev/null || true
      ln -sfn "$package_link/bin/gx" "$HOME/.local/bin/gx" 2>/dev/null || true
    fi
    rm -f "$upload_path"
    \(remoteTokenReadCommand())
    """
    let installResult = runSsh(target: target, remoteCommand: installCommand, timeoutSeconds: 45)
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.remoteCommand.result",
      command: command,
      details: remoteProcessDebugSummary(installResult))
    return installResult
  }

  private func probeRemoteInstallTarget(target: RemoteSshTarget) -> RemoteProcessResult {
    runSsh(
      target: target,
      remoteCommand: remoteInstallTargetProbeCommand(),
      timeoutSeconds: 12
    )
  }

  private func remoteInstallTargetProbeCommand() -> String {
    """
    GHOSTEX_REMOTE_OS="$(uname -s 2>/dev/null || true)"; \
    GHOSTEX_REMOTE_ARCH="$(uname -m 2>/dev/null || true)"; \
    GHOSTEX_REMOTE_DIST=""; \
    if [ -r /etc/os-release ]; then \
      GHOSTEX_REMOTE_DIST="$(sed -n 's/^ID=//p' /etc/os-release 2>/dev/null | head -n 1 | tr -d '"' || true)"; \
    fi; \
    printf '__GHOSTEX_REMOTE_PLATFORM_START__\\n'; \
    printf '%s\\n' "$GHOSTEX_REMOTE_OS"; \
    printf '%s\\n' "$GHOSTEX_REMOTE_ARCH"; \
    printf '%s\\n' "$GHOSTEX_REMOTE_DIST"; \
    printf '__GHOSTEX_REMOTE_PLATFORM_END__\\n'
    """
  }

  private func extractRemoteInstallTarget(from stdout: String) -> RemoteGxserverInstallTarget? {
    let payload: String
    if
      let start = stdout.range(of: "__GHOSTEX_REMOTE_PLATFORM_START__"),
      let end = stdout.range(of: "__GHOSTEX_REMOTE_PLATFORM_END__", range: start.upperBound..<stdout.endIndex)
    {
      payload = String(stdout[start.upperBound..<end.lowerBound])
    } else {
      payload = stdout
    }

    let lines = payload
      .split(whereSeparator: \.isNewline)
      .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
    guard lines.count >= 2, !lines[0].isEmpty, !lines[1].isEmpty else {
      return nil
    }
    return RemoteGxserverInstallTarget(
      arch: lines[1],
      distribution: lines.count >= 3 && !lines[2].isEmpty ? lines[2] : nil,
      os: lines[0]
    )
  }

  private func bundledGxserverPackageURL(for target: RemoteGxserverInstallTarget) -> URL? {
    for resourceName in bundledGxserverPackageResourceNames(for: target) {
      guard let packageURL = bundledResourceDirectory(named: resourceName) else {
        continue
      }
      if bundledGxserverPackageIsCompatible(packageURL, for: target) {
        return packageURL
      }
    }
    return nil
  }

  private func bundledGxserverPackageIsCompatible(_ packageURL: URL, for target: RemoteGxserverInstallTarget) -> Bool {
    let requiredPaths = ["bin/gxserver", "bin/zmx", "bin/zehn", "bin/bd"]
    guard requiredPaths.allSatisfy({ FileManager.default.fileExists(atPath: packageURL.appendingPathComponent($0).path) }) else {
      return false
    }
    guard target.normalizedOS == "linux" else {
      return true
    }

    /*
     CDXC:RemoteMachines 2026-06-23-09:46:
     Ubuntu packages must include the Linux server tools, bundled Node runtime,
     Portless CLI, and Ghostex CLI entrypoint before native will upload them.
     Require native ELF payloads and reject Mach-O binaries so a macOS app
     package or host-only shell wrapper cannot pass as a Linux remote package
     just because the file names match. Match ELF machine architecture too so
     x64 and arm64 remote packages cannot be staged under the wrong resource
     name and fail only after upload.
     */
    // CDXC:RemoteMinimalDeps 2026-07-13: Portless and the standalone Node
    // runtime are no longer staged in Linux remote packages.
    // CDXC:GhostexRustCli 2026-07-13: packages ship the native bin/ghostex CLI
    // (legacy CLI/ghostex-cli.mjs bundles remain accepted for old payloads).
    let hasGhostexCli = ["bin/ghostex", "CLI/ghostex-cli.mjs", "cli/ghostex-cli.mjs"].contains { relativePath in
      FileManager.default.fileExists(atPath: packageURL.appendingPathComponent(relativePath).path)
    }
    guard hasGhostexCli else {
      return false
    }
    let nativePayloadPaths = [
      "bin/gxserver",
      "bin/zmx",
      "bin/zehn",
      "bin/bd",
    ]
    guard !nativePayloadPaths.contains(where: { relativePath in
      isMachOBinary(packageURL.appendingPathComponent(relativePath))
    }) else {
      return false
    }
    return nativePayloadPaths.allSatisfy { relativePath in
      isELFBinary(packageURL.appendingPathComponent(relativePath), arch: target.normalizedArch)
    }
  }

  private func bundledGxserverPackageResourceNames(for target: RemoteGxserverInstallTarget) -> [String] {
    let os = target.normalizedOS
    let arch = target.normalizedArch
    if os == "linux" && arch == "x64" {
      return ["Web/gxserver-linux-x64", "Web/gxserver-linux-amd64"]
    }
    if os == "linux" && arch == "arm64" {
      return ["Web/gxserver-linux-arm64", "Web/gxserver-linux-aarch64"]
    }
    if os == "darwin" && arch == "arm64" {
      return bundledHostGxserverPackageArch() == "arm64"
        ? ["Web/gxserver-darwin-arm64", "Web/gxserver"]
        : ["Web/gxserver-darwin-arm64"]
    }
    if os == "darwin" && arch == "x64" {
      return bundledHostGxserverPackageArch() == "x64"
        ? ["Web/gxserver-darwin-x64", "Web/gxserver"]
        : ["Web/gxserver-darwin-x64"]
    }
    return []
  }

  private func bundledResourceDirectory(named resourceName: String) -> URL? {
    var resourceURL = Bundle.main.resourceURL
    for component in resourceName.split(separator: "/") {
      resourceURL = resourceURL?.appendingPathComponent(String(component), isDirectory: true)
    }
    return resourceURL
  }

  private func bundledHostGxserverPackageArch() -> String {
    #if arch(arm64)
      return "arm64"
    #elseif arch(x86_64)
      return "x64"
    #else
      return "unknown"
    #endif
  }

  private func onDemandResourceManifest() -> OnDemandResourceManifest? {
    guard
      let manifestURL = Bundle.main.resourceURL?
        .appendingPathComponent("Web", isDirectory: true)
        .appendingPathComponent("on-demand-resources.json", isDirectory: false),
      let data = try? Data(contentsOf: manifestURL),
      let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let version = payload["version"] as? String,
      let githubRepo = payload["githubRepo"] as? String,
      let rawAssets = payload["assets"] as? [String: [String: Any]]
    else {
      return nil
    }
    var assets: [String: OnDemandResourceAsset] = [:]
    for (key, rawAsset) in rawAssets {
      guard
        let name = rawAsset["name"] as? String,
        let sha256 = rawAsset["sha256"] as? String,
        sha256.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil,
        !name.contains("/"), !name.contains("..")
      else {
        return nil
      }
      let bytes = (rawAsset["bytes"] as? NSNumber)?.uint64Value ?? 0
      assets[key] = OnDemandResourceAsset(bytes: bytes, name: name, sha256: sha256)
    }
    return OnDemandResourceManifest(assets: assets, githubRepo: githubRepo, version: version)
  }

  private func onDemandGxserverAssetKey(for target: RemoteGxserverInstallTarget) -> String? {
    guard target.normalizedOS == "linux" else {
      return nil
    }
    switch target.normalizedArch {
    case "x64":
      return "gxserver-linux-x64"
    case "arm64":
      return "gxserver-linux-arm64"
    default:
      return nil
    }
  }

  private func sha256OfFile(atPath path: String) -> String? {
    let result = runProcess(executable: "/usr/bin/shasum", arguments: ["-a", "256", path], timeoutSeconds: 120)
    guard result.exitCode == 0 else {
      return nil
    }
    return result.stdout.split(separator: " ").first.map(String.init)
  }

  private func onDemandGxserverArchive(
    for target: RemoteGxserverInstallTarget,
    command: RemoteGxserverConnect,
    progress: ((HostEvent) -> Void)?
  ) -> Result<URL, OnDemandArchiveFailure> {
    guard
      let assetKey = onDemandGxserverAssetKey(for: target),
      let manifest = onDemandResourceManifest(),
      let asset = manifest.assets[assetKey]
    else {
      return .failure(OnDemandArchiveFailure(
        message: unsupportedRemotePackageMessage(for: target),
        state: "unsupportedRemotePlatform"
      ))
    }

    let fileManager = FileManager.default
    guard let supportRoot = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
      return .failure(OnDemandArchiveFailure(
        message: "Could not resolve the Application Support directory for the remote server package cache.",
        state: "installFailed"
      ))
    }
    let cacheDirectory = supportRoot
      .appendingPathComponent("Ghostex", isDirectory: true)
      .appendingPathComponent("on-demand", isDirectory: true)
      .appendingPathComponent(manifest.version, isDirectory: true)
    let archiveURL = cacheDirectory.appendingPathComponent(asset.name, isDirectory: false)

    if fileManager.fileExists(atPath: archiveURL.path), sha256OfFile(atPath: archiveURL.path) == asset.sha256 {
      appendRemoteInstallDebugLog(
        "remoteGxserver.install.onDemand.cacheHit",
        command: command,
        details: ["asset": asset.name])
      return .success(archiveURL)
    }

    do {
      try fileManager.createDirectory(at: cacheDirectory, withIntermediateDirectories: true)
    } catch {
      return .failure(OnDemandArchiveFailure(
        message: "Could not create the remote server package cache directory.",
        state: "installFailed"
      ))
    }

    let downloadMB = max(1, Int(asset.bytes / 1_048_576))
    progress?(statusEvent(
      command,
      state: "downloadingRemoteServerPackage",
      ok: true,
      message: "Downloading the Ghostex remote server package (\(asset.name), ~\(downloadMB) MB). This happens once per app version."
    ))
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.onDemand.downloadStart",
      command: command,
      details: ["asset": asset.name, "assetBytes": asset.bytes])

    let downloadURL = "https://github.com/\(manifest.githubRepo)/releases/download/v\(manifest.version)/\(asset.name)"
    let temporaryURL = cacheDirectory.appendingPathComponent(".download-\(UUID().uuidString)", isDirectory: false)
    defer {
      try? fileManager.removeItem(at: temporaryURL)
    }
    let curlResult = runProcess(
      executable: "/usr/bin/curl",
      arguments: ["-fsSL", "--retry", "2", "-o", temporaryURL.path, downloadURL],
      timeoutSeconds: 900
    )
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.onDemand.downloadResult",
      command: command,
      details: remoteProcessDebugSummary(curlResult))
    if curlResult.exitCode != 0 {
      return .failure(OnDemandArchiveFailure(
        message: "Could not download the remote server package from \(downloadURL). First-time remote setup needs one download from github.com per app version.",
        state: "installFailed"
      ))
    }
    guard sha256OfFile(atPath: temporaryURL.path) == asset.sha256 else {
      return .failure(OnDemandArchiveFailure(
        message: "The downloaded remote server package failed checksum verification against the app's sealed manifest and was discarded. Try connecting again.",
        state: "installFailed"
      ))
    }
    _ = runProcess(
      executable: "/usr/bin/xattr",
      arguments: ["-d", "com.apple.quarantine", temporaryURL.path],
      timeoutSeconds: 15
    )
    do {
      if fileManager.fileExists(atPath: archiveURL.path) {
        try fileManager.removeItem(at: archiveURL)
      }
      try fileManager.moveItem(at: temporaryURL, to: archiveURL)
    } catch {
      return .failure(OnDemandArchiveFailure(
        message: "Could not store the verified remote server package in the cache.",
        state: "installFailed"
      ))
    }
    appendRemoteInstallDebugLog(
      "remoteGxserver.install.onDemand.ready",
      command: command,
      details: ["asset": asset.name])
    return .success(archiveURL)
  }

  private func unsupportedRemotePackageMessage(for target: RemoteGxserverInstallTarget) -> String {
    "This Ghostex app bundle does not include a gxserver package for \(target.displayLabel). Install a Ghostex build that includes a matching remote gxserver package, then retry."
  }

  private func openTunnel(command: RemoteGxserverConnect, target: RemoteSshTarget, token: String) throws -> RemoteGxserverConnection {
    var lastError: Error?
    for attemptIndex in 0..<8 {
      let localPort = Int.random(in: 42000...58999)
      appendRemoteInstallDebugLog(
        "remoteGxserver.tunnel.attempt",
        command: command,
        details: ["attempt": attemptIndex + 1, "localPort": localPort])
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
      var arguments = ["-N"]
      arguments.append(contentsOf: sshClientOptions(target))
      arguments.append(contentsOf: [
        "-o", "ExitOnForwardFailure=yes",
        "-L", "\(localPort):127.0.0.1:58744",
      ])
      arguments.append(contentsOf: sshTargetArguments(target))
      process.arguments = arguments
      process.standardInput = FileHandle.nullDevice
      process.standardOutput = Pipe()
      process.standardError = Pipe()
      let askpass: RemoteSshAskpassScript?
      do {
        askpass = try makeSshAskpassScript(target: target)
      } catch {
        appendRemoteInstallDebugLog(
          "remoteGxserver.tunnel.askpass.failed",
          command: command,
          details: ["attempt": attemptIndex + 1, "errorDomain": (error as NSError).domain])
        lastError = error
        continue
      }
      process.environment = sshAskpassEnvironment(askpass)
      defer {
        removeSshAskpassScript(askpass)
      }

      do {
        try process.run()
      } catch {
        appendRemoteInstallDebugLog(
          "remoteGxserver.tunnel.processRun.failed",
          command: command,
          details: ["attempt": attemptIndex + 1, "errorDomain": (error as NSError).domain])
        lastError = error
        continue
      }

      Thread.sleep(forTimeInterval: 0.35)
      if !process.isRunning {
        appendRemoteInstallDebugLog(
          "remoteGxserver.tunnel.processExited",
          command: command,
          details: ["attempt": attemptIndex + 1, "exitCode": Int(process.terminationStatus)])
        lastError = NSError(
          domain: "RemoteGxserverTunnel",
          code: 1,
          userInfo: [NSLocalizedDescriptionKey: "SSH tunnel exited before remote gxserver became reachable."]
        )
        continue
      }

      let baseURL = "http://127.0.0.1:\(localPort)"
      if waitForAuthenticatedHealth(baseURL: baseURL, token: token) {
        appendRemoteInstallDebugLog(
          "remoteGxserver.tunnel.health.ok",
          command: command,
          details: ["attempt": attemptIndex + 1, "localPort": localPort])
        let connection = RemoteGxserverConnection(
          baseURL: baseURL,
          localPort: localPort,
          remoteMachineId: command.remoteMachineId,
          token: token,
          tunnelProcess: process
        )
        lock.lock()
        connections[command.remoteMachineId] = connection
        lock.unlock()
        return connection
      }

      process.terminate()
      appendRemoteInstallDebugLog(
        "remoteGxserver.tunnel.health.failed",
        command: command,
        details: ["attempt": attemptIndex + 1, "localPort": localPort])
      lastError = NSError(
        domain: "RemoteGxserverTunnel",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "SSH tunnel opened, but remote gxserver health did not become reachable."]
      )
    }

    throw lastError ?? NSError(
      domain: "RemoteGxserverTunnel",
      code: 3,
      userInfo: [NSLocalizedDescriptionKey: "Could not open an SSH tunnel to remote gxserver."]
    )
  }

  private func subscribePresentationSynchronously(
    _ command: RemoteGxserverPresentationSubscribe,
    connection: RemoteGxserverConnection,
    eventHandler: @escaping (HostEvent) -> Void
  ) throws {
    guard var components = URLComponents(string: "\(connection.baseURL)/api/events") else {
      throw NSError(
        domain: "RemoteGxserverPresentation",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Invalid remote gxserver event URL."]
      )
    }
    components.scheme = components.scheme == "https" ? "wss" : "ws"
    components.queryItems = [
      URLQueryItem(name: "protocolVersion", value: String(GxserverClient.protocolVersion)),
      URLQueryItem(name: "authToken", value: connection.token),
    ]
    guard let url = components.url else {
      throw NSError(
        domain: "RemoteGxserverPresentation",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "Invalid remote gxserver event URL."]
      )
    }

    let task = URLSession.shared.webSocketTask(with: url)
    lock.lock()
    let previous = presentationSubscriptions[command.remoteMachineId]
    presentationSubscriptions[command.remoteMachineId] = task
    lock.unlock()
    previous?.cancel(with: .goingAway, reason: nil)

    task.resume()
    var subscribePayload: [String: Any] = [
      "clientId": command.clientId ?? "macos-remote-sidebar-\(command.remoteMachineId)",
      "type": "subscribePresentation",
    ]
    if let lastRevision = command.lastRevision {
      subscribePayload["lastRevision"] = lastRevision
    }
    let data = try JSONSerialization.data(withJSONObject: subscribePayload)
    let message = String(data: data, encoding: .utf8) ?? #"{"type":"subscribePresentation"}"#
    task.send(.string(message)) { [weak self] error in
      if let error {
        eventHandler(.remoteGxserverStatus(
          remoteMachineId: command.remoteMachineId,
          payloadJson: self?.statusPayloadJson([
            "message": error.localizedDescription,
            "ok": false,
            "requestId": command.requestId,
            "state": "presentationSubscribeFailed",
          ]) ?? #"{"ok":false,"state":"presentationSubscribeFailed"}"#
        ))
      }
    }
    receivePresentationMessages(remoteMachineId: command.remoteMachineId, task: task, eventHandler: eventHandler)
  }

  private func receivePresentationMessages(
    remoteMachineId: String,
    task: URLSessionWebSocketTask,
    eventHandler: @escaping (HostEvent) -> Void
  ) {
    task.receive { [weak self] result in
      guard let self else { return }
      self.lock.lock()
      let isCurrent = self.presentationSubscriptions[remoteMachineId] === task
      self.lock.unlock()
      guard isCurrent else { return }

      switch result {
      case .success(let message):
        let payloadJson: String?
        switch message {
        case .string(let text):
          payloadJson = text
        case .data(let data):
          payloadJson = String(data: data, encoding: .utf8)
        @unknown default:
          payloadJson = nil
        }
        if let payloadJson {
          eventHandler(.remoteGxserverPresentationEvent(
            remoteMachineId: remoteMachineId,
            payloadJson: payloadJson
          ))
        }
        self.receivePresentationMessages(remoteMachineId: remoteMachineId, task: task, eventHandler: eventHandler)
      case .failure(let error):
        eventHandler(.remoteGxserverStatus(
          remoteMachineId: remoteMachineId,
          payloadJson: self.statusPayloadJson([
            "message": error.localizedDescription,
            "ok": false,
            "state": "presentationStreamFailed",
          ])
        ))
      }
    }
  }

  private func waitForAuthenticatedHealth(baseURL: String, token: String) -> Bool {
    let deadline = Date().addingTimeInterval(7)
    while Date() < deadline {
      if let response = try? performRequest(
        path: "/api/health/server",
        method: "GET",
        paramsJson: nil,
        baseURL: baseURL,
        token: token,
        timeoutSeconds: 1
      ), (200..<300).contains(response.statusCode) {
        return true
      }
      Thread.sleep(forTimeInterval: 0.2)
    }
    return false
  }

  private func performRequest(_ command: RemoteGxserverRequest, connection: RemoteGxserverConnection) throws -> (statusCode: Int, body: String?) {
    try performRequest(
      path: command.path,
      method: command.method,
      paramsJson: command.paramsJson,
      baseURL: connection.baseURL,
      token: connection.token,
      timeoutSeconds: command.path == "/api/runBeadsAction" ? 60 : 15
    )
  }

  private func performRequest(
    path: String,
    method: String,
    paramsJson: String?,
    baseURL: String,
    token: String,
    timeoutSeconds: TimeInterval
  ) throws -> (statusCode: Int, body: String?) {
    guard path.hasPrefix("/api/") else {
      throw NSError(
        domain: "RemoteGxserverRequest",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Invalid remote gxserver API path."])
    }
    guard let url = URL(string: "\(baseURL)\(path)") else {
      throw NSError(
        domain: "RemoteGxserverRequest",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "Invalid remote gxserver API URL."])
    }
    var request = URLRequest(url: url, timeoutInterval: timeoutSeconds)
    request.httpMethod = method.uppercased()
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue(String(GxserverClient.protocolVersion), forHTTPHeaderField: "x-gxserver-protocol-version")
    if request.httpMethod == "POST" {
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      let params = paramsJson?.trimmingCharacters(in: .whitespacesAndNewlines)
      let normalizedParams = (params?.isEmpty == false) ? params! : "{}"
      request.httpBody = Data(#"{"protocolVersion":\#(GxserverClient.protocolVersion),"params":\#(normalizedParams)}"#.utf8)
    }
    return try sendSynchronousRequest(request)
  }

  private func sendSynchronousRequest(_ request: URLRequest) throws -> (statusCode: Int, body: String?) {
    let semaphore = DispatchSemaphore(value: 0)
    var result: Result<(statusCode: Int, body: String?), Error>?
    URLSession.shared.dataTask(with: request) { data, response, error in
      defer { semaphore.signal() }
      if let error {
        result = .failure(error)
        return
      }
      let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
      result = .success((statusCode, data.flatMap { String(data: $0, encoding: .utf8) }))
    }.resume()
    _ = semaphore.wait(timeout: .now() + request.timeoutInterval + 2)
    guard let result else {
      throw NSError(
        domain: "RemoteGxserverRequest",
        code: 3,
        userInfo: [NSLocalizedDescriptionKey: "Remote gxserver request timed out."])
    }
    return try result.get()
  }

  private func sshClientOptions(_ target: RemoteSshTarget) -> [String] {
    var arguments = [
      "-o", "UseKeychain=yes",
      "-o", "AddKeysToAgent=yes",
      "-o", "ConnectTimeout=8",
      "-o", "StrictHostKeyChecking=accept-new",
    ]
    if target.sshPasswordAccount?.isEmpty == false {
      /*
       CDXC:RemoteMachines 2026-06-09-18:23:
       Password-backed Remote machines cannot use SSH BatchMode because it
       suppresses password auth. Enable exactly one askpass prompt and let the
       helper read the saved credential from Keychain; key-only machines keep
       BatchMode so missing keys fail quickly without interactive prompts.
       */
      arguments.append(contentsOf: [
        "-o", "BatchMode=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "PreferredAuthentications=publickey,password,keyboard-interactive",
        "-o", "PasswordAuthentication=yes",
      ])
    } else {
      arguments.append(contentsOf: ["-o", "BatchMode=yes"])
    }
    return arguments
  }

  private func makeSshAskpassScript(target: RemoteSshTarget) throws -> RemoteSshAskpassScript? {
    guard let account = target.sshPasswordAccount, !account.isEmpty else {
      return nil
    }
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-ssh-askpass-\(UUID().uuidString)", isDirectory: true)
    let script = directory.appendingPathComponent("askpass.sh")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let contents = """
    #!/bin/sh
    exec /usr/bin/security find-generic-password -s \(shellSingleQuoted(Self.sshPasswordKeychainService)) -a \(shellSingleQuoted(account)) -w
    """
    try contents.write(to: script, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: script.path)
    return RemoteSshAskpassScript(directory: directory, script: script)
  }

  private func sshAskpassEnvironment(_ askpass: RemoteSshAskpassScript?) -> [String: String]? {
    guard let askpass else {
      return nil
    }
    var environment = ProcessInfo.processInfo.environment
    environment["DISPLAY"] = environment["DISPLAY"] ?? "localhost:0"
    environment["SSH_ASKPASS"] = askpass.script.path
    environment["SSH_ASKPASS_REQUIRE"] = "force"
    return environment
  }

  private func removeSshAskpassScript(_ askpass: RemoteSshAskpassScript?) {
    guard let askpass else {
      return
    }
    try? FileManager.default.removeItem(at: askpass.directory)
  }

  private func runSsh(target: RemoteSshTarget, remoteCommand: String, timeoutSeconds: TimeInterval) -> RemoteProcessResult {
    let askpass: RemoteSshAskpassScript?
    do {
      askpass = try makeSshAskpassScript(target: target)
    } catch {
      return RemoteProcessResult(exitCode: 126, stderr: "Could not prepare SSH password helper.", stdout: "")
    }
    defer {
      removeSshAskpassScript(askpass)
    }
    var arguments = sshClientOptions(target)
    arguments.append(contentsOf: sshTargetArguments(target))
    arguments.append(loginShellRemoteCommand(remoteCommand))
    return runProcess(
      executable: "/usr/bin/ssh",
      arguments: arguments,
      environment: sshAskpassEnvironment(askpass),
      timeoutSeconds: timeoutSeconds
    )
  }

  private func runScp(
    target: RemoteSshTarget,
    localPath: String,
    remotePath: String,
    timeoutSeconds: TimeInterval
  ) -> RemoteProcessResult {
    let askpass: RemoteSshAskpassScript?
    do {
      askpass = try makeSshAskpassScript(target: target)
    } catch {
      return RemoteProcessResult(exitCode: 126, stderr: "Could not prepare SSH password helper.", stdout: "")
    }
    defer {
      removeSshAskpassScript(askpass)
    }
    var arguments = sshClientOptions(target)
    if let identityFile = target.identityFile, !identityFile.isEmpty {
      arguments.append(contentsOf: ["-i", identityFile])
    }
    if let port = target.port, port > 0 {
      arguments.append(contentsOf: ["-P", String(port)])
    }
    arguments.append(localPath)
    arguments.append("\(remoteTargetHost(target)):\(remotePath)")
    return runProcess(
      executable: "/usr/bin/scp",
      arguments: arguments,
      environment: sshAskpassEnvironment(askpass),
      timeoutSeconds: timeoutSeconds
    )
  }

  private func runProcess(
    executable: String,
    arguments: [String],
    environment: [String: String]? = nil,
    timeoutSeconds: TimeInterval
  ) -> RemoteProcessResult {
    guard processLaunchInputIsSafe(executable: executable, arguments: arguments, environment: environment) else {
      return RemoteProcessResult(exitCode: 126, stderr: "Remote gxserver process launch input was invalid.", stdout: "")
    }
    let process = Process()
    let stdoutPipe = Pipe()
    let stderrPipe = Pipe()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    if let environment {
      process.environment = environment
    }
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = stdoutPipe
    process.standardError = stderrPipe

    do {
      try process.run()
    } catch {
      return RemoteProcessResult(exitCode: 127, stderr: error.localizedDescription, stdout: "")
    }

    let deadline = Date().addingTimeInterval(timeoutSeconds)
    while process.isRunning && Date() < deadline {
      Thread.sleep(forTimeInterval: 0.05)
    }
    if process.isRunning {
      process.terminate()
      return RemoteProcessResult(exitCode: 124, stderr: "Remote SSH command timed out.", stdout: "")
    }
    let stdout = String(data: stdoutPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let stderr = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    return RemoteProcessResult(exitCode: process.terminationStatus, stderr: stderr, stdout: stdout)
  }

  private func processLaunchInputIsSafe(
    executable: String,
    arguments: [String],
    environment: [String: String]?
  ) -> Bool {
    /*
     CDXC:RemoteMachines 2026-06-30-03:32:
     Foundation's Process can raise NSInvalidArgumentException, not a Swift
     Error, when executable, argv, or environment strings contain NUL bytes.
     Remote setup must reject those inputs before launch so Install reports a
     controlled failure instead of terminating Ghostex.
     */
    guard !executable.contains("\u{0}"), !arguments.contains(where: { $0.contains("\u{0}") }) else {
      return false
    }
    if let environment {
      for (key, value) in environment where key.contains("\u{0}") || value.contains("\u{0}") {
        return false
      }
    }
    return true
  }

  private func sshTargetArguments(_ target: RemoteSshTarget) -> [String] {
    var args: [String] = []
    if let identityFile = target.identityFile, !identityFile.isEmpty {
      args.append(contentsOf: ["-i", identityFile])
    }
    if let port = target.port, port > 0 {
      args.append(contentsOf: ["-p", String(port)])
    }
    let host = target.user?.isEmpty == false ? "\(target.user!)@\(target.host)" : target.host
    args.append(host)
    return args
  }

  private func expandedLocalPath(_ path: String?) -> String? {
    let trimmed = path?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else { return nil }
    return (trimmed as NSString).expandingTildeInPath
  }

  private func loginShellRemoteCommand(_ command: String) -> String {
    /*
     CDXC:RemoteMachines 2026-06-08-19:12:
     Remote macOS hosts can have Ghostex, gxserver, and Node installed through
     Homebrew or mise in user shell startup files. Native SSH setup must execute
     daemon checks through the user's zsh login+interactive environment and
     still resolve the app-installed ~/.ghostex package path, otherwise a
     running `ghostex server` appears missing from non-interactive SSH.
     */
    let quotedCommand = shellSingleQuoted(command)
    return """
    if [ -x /bin/zsh ]; then exec /bin/zsh -lic \(quotedCommand); \
    elif command -v zsh >/dev/null 2>&1; then exec zsh -lic \(quotedCommand); \
    else exec /bin/sh -lc \(quotedCommand); fi
    """
  }

  private func shellSingleQuoted(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  private func remoteTargetHost(_ target: RemoteSshTarget) -> String {
    target.user?.isEmpty == false ? "\(target.user!)@\(target.host)" : target.host
  }

  private func connection(for remoteMachineId: String) throws -> RemoteGxserverConnection {
    lock.lock()
    let connection = connections[remoteMachineId]
    lock.unlock()
    guard let connection, connection.tunnelProcess.isRunning else {
      throw NSError(
        domain: "RemoteGxserverConnection",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Remote gxserver is not connected."])
    }
    return connection
  }

  private func terminateExistingConnection(remoteMachineId: String) {
    lock.lock()
    let existing = connections.removeValue(forKey: remoteMachineId)
    let subscription = presentationSubscriptions.removeValue(forKey: remoteMachineId)
    lock.unlock()
    subscription?.cancel(with: .goingAway, reason: nil)
    if existing?.tunnelProcess.isRunning == true {
      existing?.tunnelProcess.terminate()
    }
  }

  private func storeTokenInKeychain(_ token: String, remoteMachineId: String) throws {
    guard let tokenData = token.data(using: .utf8) else {
      throw NSError(domain: "RemoteGxserverKeychain", code: 1)
    }
    let query: [String: Any] = [
      kSecAttrAccount as String: remoteMachineId,
      kSecAttrService as String: Self.keychainService,
      kSecClass as String: kSecClassGenericPassword,
    ]
    SecItemDelete(query as CFDictionary)
    var addQuery = query
    addQuery[kSecValueData as String] = tokenData
    let status = SecItemAdd(addQuery as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw NSError(domain: "RemoteGxserverKeychain", code: Int(status))
    }
  }

  private func storeSshPasswordInKeychain(_ password: String, remoteMachineId: String) throws {
    guard let passwordData = password.data(using: .utf8) else {
      throw NSError(domain: "RemoteSshPasswordKeychain", code: 1)
    }
    let query = sshPasswordKeychainQuery(remoteMachineId: remoteMachineId)
    SecItemDelete(query as CFDictionary)
    var addQuery = query
    addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    addQuery[kSecValueData as String] = passwordData
    let status = SecItemAdd(addQuery as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw NSError(domain: "RemoteSshPasswordKeychain", code: Int(status))
    }
  }

  private func deleteSshPasswordFromKeychain(remoteMachineId: String) throws {
    let status = SecItemDelete(sshPasswordKeychainQuery(remoteMachineId: remoteMachineId) as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
      throw NSError(domain: "RemoteSshPasswordKeychain", code: Int(status))
    }
  }

  private func keychainHasSshPassword(remoteMachineId: String) -> Bool {
    var query = sshPasswordKeychainQuery(remoteMachineId: remoteMachineId)
    query[kSecReturnData as String] = false
    query[kSecMatchLimit as String] = kSecMatchLimitOne
    return SecItemCopyMatching(query as CFDictionary, nil) == errSecSuccess
  }

  private func sshPasswordKeychainQuery(remoteMachineId: String) -> [String: Any] {
    [
      kSecAttrAccount as String: remoteMachineId,
      kSecAttrService as String: Self.sshPasswordKeychainService,
      kSecClass as String: kSecClassGenericPassword,
    ]
  }

  private func statusEvent(
    _ command: RemoteGxserverConnect,
    state: String,
    ok: Bool,
    message: String,
    extra: [String: Any] = [:]
  ) -> HostEvent {
    var payload: [String: Any] = [
      "message": message,
      "ok": ok,
      "protocolVersion": GxserverClient.protocolVersion,
      "requestId": command.requestId,
      "state": state,
    ]
    for (key, value) in extra {
      payload[key] = value
    }
    return .remoteGxserverStatus(
      remoteMachineId: command.remoteMachineId,
      payloadJson: statusPayloadJson(payload)
    )
  }

  private func statusPayloadJson(_ payload: [String: Any]) -> String {
    guard
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let payloadJson = String(data: data, encoding: .utf8)
    else {
      return #"{"ok":false,"state":"invalid","message":"Could not encode remote gxserver status."}"#
    }
    return payloadJson
  }

  private func isValidAuthToken(_ token: String) -> Bool {
    token.range(of: #"^[A-Za-z0-9_-]{32,}$"#, options: .regularExpression) != nil
  }

  private func extractRemoteAuthToken(from stdout: String) -> String {
    if
      let start = stdout.range(of: "__GHOSTEX_REMOTE_TOKEN_START__"),
      let end = stdout.range(of: "__GHOSTEX_REMOTE_TOKEN_END__", range: start.upperBound..<stdout.endIndex)
    {
      return String(stdout[start.upperBound..<end.lowerBound]).trimmingCharacters(in: .whitespacesAndNewlines)
    }
    let matchRange = stdout.range(of: #"[A-Za-z0-9_-]{32,}"#, options: .regularExpression)
    return matchRange.map { String(stdout[$0]) } ?? stdout.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private func sanitizedProcessFailure(defaultMessage: String, result: RemoteProcessResult) -> String {
    let stderr = result.stderr.trimmingCharacters(in: .whitespacesAndNewlines)
    if stderr.isEmpty {
      return defaultMessage
    }
    /*
     CDXC:RemoteMachines 2026-07-14-00:00:
     Remote connect always runs SSH non-interactively from the GUI helper, so
     the raw failure is opaque to users. Map the common causes to specific,
     actionable messages. This is the main diagnosability path (issue #61) for
     connecting to another account on the same Mac: a terminal succeeds by
     prompting for that account's password, but Ghostex only sends a password
     when one is saved for the machine, so a key-only machine fails fast with
     "Permission denied" and no prompt.
     */
    if stderr.localizedCaseInsensitiveContains("remote host identification has changed") {
      return "The remote host key changed since Ghostex last connected. Verify the host is trusted, remove its old entry from ~/.ssh/known_hosts, then reconnect."
    }
    if stderr.localizedCaseInsensitiveContains("host key verification failed") {
      return "SSH could not verify the remote host key. Connect once from a terminal to record the host in ~/.ssh/known_hosts, then reconnect."
    }
    if stderr.localizedCaseInsensitiveContains("permission denied") {
      return "SSH authentication was rejected. If this machine uses a password (common when connecting to another account on the same Mac), save its SSH password in Remote settings; otherwise add a key the remote account accepts."
    }
    if stderr.localizedCaseInsensitiveContains("connection refused") {
      return "The remote host refused the SSH connection. Enable Remote Login (System Settings → General → Sharing) on it and confirm the port."
    }
    if stderr.localizedCaseInsensitiveContains("could not resolve hostname") {
      return "SSH could not resolve the remote host. Check the host name or IP address."
    }
    if stderr.localizedCaseInsensitiveContains("no route to host") ||
      stderr.localizedCaseInsensitiveContains("network is unreachable") {
      return "SSH could not reach the remote host over the network. Check connectivity and the host address."
    }
    if stderr.localizedCaseInsensitiveContains("operation timed out") ||
      stderr.localizedCaseInsensitiveContains("connection timed out") {
      return "SSH connection to the remote machine timed out."
    }
    return defaultMessage
  }
}

private func normalizeRemoteInstallOS(_ value: String) -> String {
  let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
  if normalized.contains("darwin") {
    return "darwin"
  }
  if normalized.contains("linux") {
    return "linux"
  }
  return normalized.isEmpty ? "unknown" : normalized
}

private func normalizeRemoteInstallArch(_ value: String) -> String {
  switch value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
  case "amd64", "x86_64":
    return "x64"
  case "aarch64", "arm64":
    return "arm64"
  default:
    let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return normalized.isEmpty ? "unknown" : normalized
  }
}

private func isMachOBinary(_ url: URL) -> Bool {
  guard
    let data = try? Data(contentsOf: url, options: [.mappedIfSafe]),
    data.count >= 4
  else {
    return false
  }
  let prefix = Array(data.prefix(4))
  return prefix == [0xfe, 0xed, 0xfa, 0xce] ||
    prefix == [0xce, 0xfa, 0xed, 0xfe] ||
    prefix == [0xfe, 0xed, 0xfa, 0xcf] ||
    prefix == [0xcf, 0xfa, 0xed, 0xfe] ||
    prefix == [0xca, 0xfe, 0xba, 0xbe] ||
    prefix == [0xbe, 0xba, 0xfe, 0xca]
}

private func isELFBinary(_ url: URL, arch: String? = nil) -> Bool {
  guard
    let data = try? Data(contentsOf: url, options: [.mappedIfSafe]),
    data.count >= 4
  else {
    return false
  }
  guard Array(data.prefix(4)) == [0x7f, 0x45, 0x4c, 0x46] else {
    return false
  }
  guard let arch else {
    return true
  }
  return elfMachine(data) == expectedELFMachine(for: arch)
}

private func expectedELFMachine(for arch: String) -> UInt16? {
  switch normalizeRemoteInstallArch(arch) {
  case "x64":
    return 0x3e
  case "arm64":
    return 0xb7
  default:
    return nil
  }
}

private func elfMachine(_ data: Data) -> UInt16? {
  guard data.count >= 20 else {
    return nil
  }
  let machineRange = 18..<20
  let machineBytes = [UInt8](data[machineRange])
  switch data[5] {
  case 1:
    return UInt16(machineBytes[0]) | (UInt16(machineBytes[1]) << 8)
  case 2:
    return (UInt16(machineBytes[0]) << 8) | UInt16(machineBytes[1])
  default:
    return nil
  }
}

private struct RemoteSshTarget {
  let host: String
  let identityFile: String?
  let port: Int?
  let sshPasswordAccount: String?
  let user: String?
}
