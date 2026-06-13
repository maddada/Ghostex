import AppKit
import CommonCrypto
import Foundation
import WebKit

struct NativeBrowserProfileDefinition: Codable, Hashable, Identifiable, Sendable {
  let id: UUID
  var displayName: String
  let createdAt: Date
  let isBuiltInDefault: Bool
}

@MainActor
final class NativeBrowserProfileStore {
  static let shared = NativeBrowserProfileStore()

  private static let profilesDefaultsKey = "browserProfiles.v1"
  private static let lastUsedProfileDefaultsKey = "browserProfiles.lastUsed"
  private static let builtInDefaultProfileID = UUID(uuidString: "52B43C05-4A1D-45D3-8FD5-9EF94952E445")!

  private let defaults: UserDefaults
  private var dataStores: [UUID: WKWebsiteDataStore] = [:]
  private(set) var profiles: [NativeBrowserProfileDefinition] = []
  private(set) var lastUsedProfileID: UUID = builtInDefaultProfileID

  private init(defaults: UserDefaults = .standard) {
    self.defaults = defaults
    load()
  }

  var effectiveLastUsedProfileID: UUID {
    profileDefinition(id: lastUsedProfileID) != nil
      ? lastUsedProfileID
      : Self.builtInDefaultProfileID
  }

  func profileDefinition(id: UUID) -> NativeBrowserProfileDefinition? {
    profiles.first(where: { $0.id == id })
  }

  func displayName(for id: UUID) -> String {
    profileDefinition(id: id)?.displayName ?? "Default"
  }

  func noteUsed(_ id: UUID) {
    guard profileDefinition(id: id) != nil else { return }
    lastUsedProfileID = id
    defaults.set(id.uuidString, forKey: Self.lastUsedProfileDefaultsKey)
  }

  func createProfile(named rawName: String) -> NativeBrowserProfileDefinition? {
    let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else { return nil }
    let profile = NativeBrowserProfileDefinition(
      id: UUID(),
      displayName: name,
      createdAt: Date(),
      isBuiltInDefault: false
    )
    profiles.append(profile)
    sortAndPersist()
    noteUsed(profile.id)
    return profile
  }

  func websiteDataStore(for profileID: UUID) -> WKWebsiteDataStore {
    if profileID == Self.builtInDefaultProfileID {
      return .default()
    }
    if let existing = dataStores[profileID] {
      return existing
    }
    if #available(macOS 14.0, *) {
      let store = WKWebsiteDataStore(forIdentifier: profileID)
      dataStores[profileID] = store
      return store
    }
    return .default()
  }

  private func load() {
    if let data = defaults.data(forKey: Self.profilesDefaultsKey),
      let decoded = try? JSONDecoder().decode([NativeBrowserProfileDefinition].self, from: data)
    {
      profiles = decoded
    }

    if !profiles.contains(where: \.isBuiltInDefault) {
      profiles.append(
        NativeBrowserProfileDefinition(
          id: Self.builtInDefaultProfileID,
          displayName: "Default",
          createdAt: Date(timeIntervalSince1970: 0),
          isBuiltInDefault: true
        ))
    }

    if let rawID = defaults.string(forKey: Self.lastUsedProfileDefaultsKey),
      let id = UUID(uuidString: rawID),
      profileDefinition(id: id) != nil
    {
      lastUsedProfileID = id
    }

    sortAndPersist()
  }

  private func sortAndPersist() {
    profiles.sort {
      if $0.isBuiltInDefault != $1.isBuiltInDefault {
        return $0.isBuiltInDefault && !$1.isBuiltInDefault
      }
      return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
    }
    if let data = try? JSONEncoder().encode(profiles) {
      defaults.set(data, forKey: Self.profilesDefaultsKey)
    }
  }
}

/**
 CDXC:BrowserPanes 2026-05-02-06:35
 Profile selection is native macOS UI and persists the chosen browser profile
 for future browser panes. Existing panes keep their current WebKit data store;
 new panes use the selected profile's WKWebsiteDataStore.
 */
@MainActor
enum NativeBrowserProfileUI {
  static func showPicker(parentWindow: NSWindow?, currentProfileID: UUID?) {
    let store = NativeBrowserProfileStore.shared
    let menu = NSMenu(title: "Profiles")
    for profile in store.profiles {
      let item = NSMenuItem(title: profile.displayName, action: #selector(ProfileMenuTarget.pick(_:)), keyEquivalent: "")
      item.identifier = NSUserInterfaceItemIdentifier(profile.id.uuidString)
      item.state = profile.id == (currentProfileID ?? store.effectiveLastUsedProfileID) ? .on : .off
      item.target = ProfileMenuTarget.shared
      menu.addItem(item)
    }
    menu.addItem(.separator())
    let newItem = NSMenuItem(title: "New Profile...", action: #selector(ProfileMenuTarget.create(_:)), keyEquivalent: "")
    newItem.target = ProfileMenuTarget.shared
    menu.addItem(newItem)
    let importItem = NSMenuItem(
      title: "Import Browser Data...",
      action: #selector(ProfileMenuTarget.importBrowserData(_:)),
      keyEquivalent: ""
    )
    importItem.target = ProfileMenuTarget.shared
    menu.addItem(importItem)

    let location = NSEvent.mouseLocation
    if let event = NSEvent.mouseEvent(
      with: .rightMouseDown,
      location: location,
      modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime,
      windowNumber: parentWindow?.windowNumber ?? 0,
      context: nil,
      eventNumber: 0,
      clickCount: 1,
      pressure: 1
    ) {
      NSMenu.popUpContextMenu(menu, with: event, for: parentWindow?.contentView ?? NSView())
    }
  }

  static func showImportSettings(parentWindow: NSWindow?) {
    NativeBrowserImportDialogController.present(parentWindow: parentWindow)
  }

  private final class ProfileMenuTarget: NSObject {
    static let shared = ProfileMenuTarget()

    @MainActor
    @objc func pick(_ sender: NSMenuItem) {
      guard let rawID = sender.identifier?.rawValue,
        let profileID = UUID(uuidString: rawID)
      else { return }
      NativeBrowserProfileStore.shared.noteUsed(profileID)
    }

    @MainActor
    @objc func create(_ sender: NSMenuItem) {
      let alert = NSAlert()
      alert.messageText = "New Browser Profile"
      alert.informativeText = "Enter a name for the new browser profile."
      alert.addButton(withTitle: "Create")
      alert.addButton(withTitle: "Cancel")
      let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
      input.placeholderString = "Profile name"
      alert.accessoryView = input
      guard alert.runModal() == .alertFirstButtonReturn else { return }
      _ = NativeBrowserProfileStore.shared.createProfile(named: input.stringValue)
    }

    @MainActor
    @objc func importBrowserData(_ sender: NSMenuItem) {
      /**
       CDXC:BrowserImport 2026-06-04-11:40:
       Browser data import belongs in the profile menu rather than as a permanent extra address-bar button. Keep the toolbar visually stable while still giving users a profile-scoped path to import compatible browser cookies.
       */
      NativeBrowserProfileUI.showImportSettings(parentWindow: NSApp.keyWindow ?? NSApp.mainWindow)
    }
  }
}

private enum NativeBrowserImportFamily: Sendable {
  case chromium
  case firefox
}

private struct NativeBrowserImportDescriptor: Sendable {
  let displayName: String
  let family: NativeBrowserImportFamily
  let keychainServices: [String]
  let relativeDataRoots: [String]
}

private struct NativeBrowserImportSourceProfile: Hashable, Sendable {
  let displayName: String
  let rootURL: URL
  let isDefault: Bool
}

private struct NativeBrowserImportCandidate: Sendable {
  let descriptor: NativeBrowserImportDescriptor
  let rootURL: URL
  let profiles: [NativeBrowserImportSourceProfile]
}

private struct NativeBrowserCookieImportOutcome: Sendable {
  let browserName: String
  let sourceProfileName: String
  let destinationProfileName: String
  let importedCookies: Int
  let skippedCookies: Int
  let warnings: [String]
}

private struct NativeChromiumCookieReadStats {
  var rowCount = 0
  var missingNameRows = 0
  var domainFilteredRows = 0
  var plaintextRows = 0
  var encryptedRows = 0
  var decryptedRows = 0
  var skippedEncryptedRows = 0
  var emptyValueRows = 0
  var cookieRejectedRows = 0
  var encryptedMinBytes: Int?
  var encryptedMaxBytes: Int?
  var encryptedPrefixCounts: [String: Int] = [:]
  var decryptFailureReasonCounts: [String: Int] = [:]
  var decryptStatusCounts: [String: Int] = [:]

  mutating func noteEncryptedValue(encryptedHex: String) {
    encryptedRows += 1
    let byteCount = encryptedHex.trimmingCharacters(in: .whitespacesAndNewlines).count / 2
    encryptedMinBytes = min(encryptedMinBytes ?? byteCount, byteCount)
    encryptedMaxBytes = max(encryptedMaxBytes ?? byteCount, byteCount)
    encryptedPrefixCounts[Self.encryptedPrefixKind(encryptedHex), default: 0] += 1
  }

  mutating func noteDecryptFailure(reason: String, status: Int?) {
    skippedEncryptedRows += 1
    decryptFailureReasonCounts[reason, default: 0] += 1
    if let status {
      decryptStatusCounts[String(status), default: 0] += 1
    }
  }

  func logPayload(
    family: NativeBrowserImportFamily,
    sourceProfile: NativeBrowserImportSourceProfile,
    domainFilterCount: Int,
    candidateCookieCount: Int,
    dedupedCookieCount: Int
  ) -> [String: Any] {
    [
      "browserFamily": family.logValue,
      "sourceProfileKind": sourceProfile.logKind,
      "sourceProfileIsDefault": sourceProfile.isDefault,
      "domainFilterCount": domainFilterCount,
      "rowCount": rowCount,
      "missingNameRows": missingNameRows,
      "domainFilteredRows": domainFilteredRows,
      "plaintextRows": plaintextRows,
      "encryptedRows": encryptedRows,
      "decryptedRows": decryptedRows,
      "skippedEncryptedRows": skippedEncryptedRows,
      "emptyValueRows": emptyValueRows,
      "cookieRejectedRows": cookieRejectedRows,
      "candidateCookieCount": candidateCookieCount,
      "dedupedCookieCount": dedupedCookieCount,
      "encryptedMinBytes": encryptedMinBytes ?? NSNull(),
      "encryptedMaxBytes": encryptedMaxBytes ?? NSNull(),
      "encryptedPrefixCounts": encryptedPrefixCounts,
      "decryptFailureReasonCounts": decryptFailureReasonCounts,
      "decryptStatusCounts": decryptStatusCounts,
    ]
  }

  private static func encryptedPrefixKind(_ encryptedHex: String) -> String {
    let normalized = encryptedHex.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    guard normalized.count >= 6 else { return "short" }
    switch normalized.prefix(6) {
    case "763130":
      return "v10"
    case "763131":
      return "v11"
    case "763230":
      return "v20"
    default:
      return "other"
    }
  }
}

private extension NativeBrowserImportFamily {
  var logValue: String {
    switch self {
    case .chromium:
      return "chromium"
    case .firefox:
      return "firefox"
    }
  }
}

private extension NativeBrowserImportSourceProfile {
  var logKind: String {
    let directoryName = rootURL.lastPathComponent
    if directoryName == "Default" {
      return "default"
    }
    if directoryName.range(of: #"^Profile [0-9]+$"#, options: .regularExpression) != nil {
      return "numbered"
    }
    if isDefault {
      return "defaultLike"
    }
    return "other"
  }
}

private enum NativeBrowserDataImporter {
  private static let descriptors: [NativeBrowserImportDescriptor] = [
    NativeBrowserImportDescriptor(
      displayName: "Google Chrome",
      family: .chromium,
      keychainServices: ["Chrome Safe Storage", "Google Chrome Safe Storage"],
      relativeDataRoots: ["Library/Application Support/Google/Chrome"]
    ),
    NativeBrowserImportDescriptor(
      displayName: "Brave",
      family: .chromium,
      keychainServices: ["Brave Safe Storage", "Brave Browser Safe Storage"],
      relativeDataRoots: ["Library/Application Support/BraveSoftware/Brave-Browser"]
    ),
    NativeBrowserImportDescriptor(
      displayName: "Microsoft Edge",
      family: .chromium,
      keychainServices: ["Microsoft Edge Safe Storage"],
      relativeDataRoots: ["Library/Application Support/Microsoft Edge"]
    ),
    NativeBrowserImportDescriptor(
      displayName: "Arc",
      family: .chromium,
      keychainServices: ["Arc Safe Storage"],
      relativeDataRoots: ["Library/Application Support/Arc"]
    ),
    NativeBrowserImportDescriptor(
      displayName: "Chromium",
      family: .chromium,
      keychainServices: ["Chromium Safe Storage"],
      relativeDataRoots: ["Library/Application Support/Chromium"]
    ),
    NativeBrowserImportDescriptor(
      displayName: "Firefox",
      family: .firefox,
      keychainServices: [],
      relativeDataRoots: ["Library/Application Support/Firefox"]
    ),
  ]

  static func detectCandidates(
    homeDirectoryURL: URL = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
  ) -> [NativeBrowserImportCandidate] {
    let fileManager = FileManager.default
    return descriptors.compactMap { descriptor in
      for relativeRoot in descriptor.relativeDataRoots {
        let rootURL = homeDirectoryURL.appendingPathComponent(relativeRoot, isDirectory: true)
        guard fileManager.fileExists(atPath: rootURL.path) else { continue }
        let profiles: [NativeBrowserImportSourceProfile]
        switch descriptor.family {
        case .chromium:
          profiles = chromiumProfiles(rootURL: rootURL)
        case .firefox:
          profiles = firefoxProfiles(rootURL: rootURL)
        }
        if !profiles.isEmpty {
          return NativeBrowserImportCandidate(
            descriptor: descriptor,
            rootURL: rootURL,
            profiles: profiles
          )
        }
      }
      return nil
    }.sorted {
      if $0.profiles.count != $1.profiles.count {
        return $0.profiles.count > $1.profiles.count
      }
      return $0.descriptor.displayName.localizedCaseInsensitiveCompare($1.descriptor.displayName) == .orderedAscending
    }
  }

  static func importCookies(
    from candidate: NativeBrowserImportCandidate,
    sourceProfile: NativeBrowserImportSourceProfile,
    destinationProfile: NativeBrowserProfileDefinition,
    domainFilters: [String]
  ) async -> NativeBrowserCookieImportOutcome {
    let startedAt = Date()
    NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.import.start", details: [
      "browserFamily": candidate.descriptor.family.logValue,
      "sourceProfileKind": sourceProfile.logKind,
      "sourceProfileIsDefault": sourceProfile.isDefault,
      "destinationProfileID": destinationProfile.id.uuidString,
      "destinationProfileIsBuiltInDefault": destinationProfile.isBuiltInDefault,
      "domainFilterCount": domainFilters.count,
    ])
    let result: (cookies: [HTTPCookie], skipped: Int, warnings: [String])
    switch candidate.descriptor.family {
    case .chromium:
      result = readChromiumCookies(
        from: sourceProfile,
        descriptor: candidate.descriptor,
        domainFilters: domainFilters
      )
    case .firefox:
      result = readFirefoxCookies(from: sourceProfile, domainFilters: domainFilters)
    }
    let imported = await importCookiesIntoCEFProfile(
      result.cookies,
      destinationProfileID: destinationProfile.id
    )
    let skippedCookies = max(0, result.cookies.count - imported) + result.skipped
    NativeBrowserImportDebugLog.append(
      event: imported == 0 && !result.cookies.isEmpty
        ? "nativeBrowserImport.cefImport.warningZeroImported"
        : "nativeBrowserImport.import.finish",
      details: [
        "browserFamily": candidate.descriptor.family.logValue,
        "sourceProfileKind": sourceProfile.logKind,
        "sourceProfileIsDefault": sourceProfile.isDefault,
        "destinationProfileID": destinationProfile.id.uuidString,
        "destinationProfileIsBuiltInDefault": destinationProfile.isBuiltInDefault,
        "candidateCookieCount": result.cookies.count,
        "importedCookieCount": imported,
        "skippedCookieCount": skippedCookies,
        "warningCount": result.warnings.count,
        "elapsedMs": Int(Date().timeIntervalSince(startedAt) * 1000),
      ])
    return NativeBrowserCookieImportOutcome(
      browserName: candidate.descriptor.displayName,
      sourceProfileName: sourceProfile.displayName,
      destinationProfileName: destinationProfile.displayName,
      importedCookies: imported,
      skippedCookies: skippedCookies,
      warnings: result.warnings
    )
  }

  static func parseDomainFilters(_ rawValue: String) -> [String] {
    rawValue
      .components(separatedBy: CharacterSet(charactersIn: ",;\n\r\t "))
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
      .filter { !$0.isEmpty }
      .map { value in
        var normalized = value
        while normalized.hasPrefix(".") {
          normalized.removeFirst()
        }
        return normalized
      }
  }

  private static func chromiumProfiles(rootURL: URL) -> [NativeBrowserImportSourceProfile] {
    let fileManager = FileManager.default
    let nameMap = chromiumProfileNameMap(rootURL: rootURL)
    var profiles: [NativeBrowserImportSourceProfile] = []
    let children = (try? fileManager.contentsOfDirectory(
      at: rootURL,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles]
    )) ?? []
    for child in children {
      guard (try? child.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else {
        continue
      }
      let name = child.lastPathComponent
      let isLikelyProfile = name == "Default" || name.hasPrefix("Profile ") || nameMap[name] != nil
      guard isLikelyProfile else { continue }
      let cookiesURL = child.appendingPathComponent("Cookies", isDirectory: false)
      guard fileManager.fileExists(atPath: cookiesURL.path) else { continue }
      profiles.append(
        NativeBrowserImportSourceProfile(
          displayName: nameMap[name] ?? (name == "Default" ? "Default" : name),
          rootURL: child,
          isDefault: name == "Default"
        ))
    }
    return sortProfiles(profiles)
  }

  private static func firefoxProfiles(rootURL: URL) -> [NativeBrowserImportSourceProfile] {
    let fileManager = FileManager.default
    var profiles: [NativeBrowserImportSourceProfile] = firefoxProfilesFromINI(rootURL: rootURL)
    let profileRoot = rootURL.appendingPathComponent("Profiles", isDirectory: true)
    let children = (try? fileManager.contentsOfDirectory(
      at: profileRoot,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles]
    )) ?? []
    for child in children {
      guard (try? child.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else {
        continue
      }
      let cookiesURL = child.appendingPathComponent("cookies.sqlite", isDirectory: false)
      guard fileManager.fileExists(atPath: cookiesURL.path) else { continue }
      profiles.append(
        NativeBrowserImportSourceProfile(
          displayName: child.lastPathComponent,
          rootURL: child,
          isDefault: child.lastPathComponent.localizedCaseInsensitiveContains("default")
        ))
    }
    return sortProfiles(dedupedProfiles(profiles))
  }

  private static func firefoxProfilesFromINI(rootURL: URL) -> [NativeBrowserImportSourceProfile] {
    let iniURL = rootURL.appendingPathComponent("profiles.ini", isDirectory: false)
    guard let contents = try? String(contentsOf: iniURL, encoding: .utf8) else {
      return []
    }
    var result: [NativeBrowserImportSourceProfile] = []
    var section: [String: String] = [:]
    func flush() {
      defer { section.removeAll() }
      guard let pathValue = section["Path"], !pathValue.isEmpty else { return }
      let isRelative = section["IsRelative"] != "0"
      let profileURL = isRelative
        ? rootURL.appendingPathComponent(pathValue, isDirectory: true)
        : URL(fileURLWithPath: pathValue, isDirectory: true)
      guard FileManager.default.fileExists(
        atPath: profileURL.appendingPathComponent("cookies.sqlite", isDirectory: false).path
      ) else { return }
      result.append(
        NativeBrowserImportSourceProfile(
          displayName: section["Name"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            ? section["Name"]!
            : profileURL.lastPathComponent,
          rootURL: profileURL,
          isDefault: section["Default"] == "1"
        ))
    }
    for line in contents.components(separatedBy: .newlines) {
      let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
      if trimmed.isEmpty || trimmed.hasPrefix("#") || trimmed.hasPrefix(";") {
        continue
      }
      if trimmed.hasPrefix("[") && trimmed.hasSuffix("]") {
        flush()
        continue
      }
      guard let separator = trimmed.firstIndex(of: "=") else { continue }
      let key = String(trimmed[..<separator]).trimmingCharacters(in: .whitespacesAndNewlines)
      let value = String(trimmed[trimmed.index(after: separator)...])
        .trimmingCharacters(in: .whitespacesAndNewlines)
      section[key] = value
    }
    flush()
    return result
  }

  private static func chromiumProfileNameMap(rootURL: URL) -> [String: String] {
    let localStateURL = rootURL.appendingPathComponent("Local State", isDirectory: false)
    guard let data = try? Data(contentsOf: localStateURL),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let profile = json["profile"] as? [String: Any],
      let infoCache = profile["info_cache"] as? [String: Any]
    else {
      return [:]
    }
    var result: [String: String] = [:]
    for (directory, value) in infoCache {
      guard let item = value as? [String: Any],
        let name = item["name"] as? String,
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      else { continue }
      result[directory] = name
    }
    return result
  }

  private static func readFirefoxCookies(
    from profile: NativeBrowserImportSourceProfile,
    domainFilters: [String]
  ) -> (cookies: [HTTPCookie], skipped: Int, warnings: [String]) {
    let databaseURL = profile.rootURL.appendingPathComponent("cookies.sqlite", isDirectory: false)
    let rows = readSQLiteJSONRows(
      databaseURL: databaseURL,
      databaseKind: "firefoxCookies",
      sql: "SELECT host, name, value, path, expiry, isSecure FROM moz_cookies"
    )
    var cookies: [HTTPCookie] = []
    for row in rows {
      let host = stringValue(row["host"])
      let name = stringValue(row["name"])
      let value = stringValue(row["value"])
      guard !name.isEmpty, domainMatches(host: host, filters: domainFilters) else { continue }
      var properties: [HTTPCookiePropertyKey: Any] = [
        .domain: host,
        .path: stringValue(row["path"]).isEmpty ? "/" : stringValue(row["path"]),
        .name: name,
        .value: value,
      ]
      if intValue(row["isSecure"]) != 0 {
        properties[.secure] = "TRUE"
      }
      let expiry = intValue(row["expiry"])
      if expiry > 0 {
        properties[.expires] = Date(timeIntervalSince1970: TimeInterval(expiry))
      }
      if let cookie = HTTPCookie(properties: properties) {
        cookies.append(cookie)
      }
    }
    return (dedupeCookies(cookies), 0, rows.isEmpty ? ["No compatible Firefox cookies were found."] : [])
  }

  private static func readChromiumCookies(
    from profile: NativeBrowserImportSourceProfile,
    descriptor: NativeBrowserImportDescriptor,
    domainFilters: [String]
  ) -> (cookies: [HTTPCookie], skipped: Int, warnings: [String]) {
    let databaseURL = profile.rootURL.appendingPathComponent("Cookies", isDirectory: false)
    let rows = readSQLiteJSONRows(
      databaseURL: databaseURL,
      databaseKind: "chromiumCookies",
      sql: """
      SELECT host_key, name, value, path, expires_utc, is_secure, hex(encrypted_value) AS encrypted_hex
      FROM cookies
      """
    )
    let decryptor = NativeChromiumCookieDecryptor(descriptor: descriptor)
    var cookies: [HTTPCookie] = []
    var skippedEncrypted = 0
    var stats = NativeChromiumCookieReadStats()
    stats.rowCount = rows.count
    for row in rows {
      let host = stringValue(row["host_key"])
      let name = stringValue(row["name"])
      var value = stringValue(row["value"])
      guard !name.isEmpty else {
        stats.missingNameRows += 1
        continue
      }
      guard domainMatches(host: host, filters: domainFilters) else {
        stats.domainFilteredRows += 1
        continue
      }
      let encryptedHex = stringValue(row["encrypted_hex"])
      if value.isEmpty && !encryptedHex.isEmpty {
        stats.noteEncryptedValue(encryptedHex: encryptedHex)
        let decryptResult = decryptor.decryptWithDiagnostics(encryptedHex: encryptedHex)
        if let decryptedValue = decryptResult.value {
          value = decryptedValue
          stats.decryptedRows += 1
        } else {
          skippedEncrypted += 1
          stats.noteDecryptFailure(
            reason: decryptResult.failureReason ?? "unknown",
            status: decryptResult.failureStatus)
          continue
        }
      } else if !value.isEmpty {
        stats.plaintextRows += 1
      }
      value = value.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !value.isEmpty else {
        stats.emptyValueRows += 1
        continue
      }
      var properties: [HTTPCookiePropertyKey: Any] = [
        .domain: host,
        .path: stringValue(row["path"]).isEmpty ? "/" : stringValue(row["path"]),
        .name: name,
        .value: value,
      ]
      if intValue(row["is_secure"]) != 0 {
        properties[.secure] = "TRUE"
      }
      if let expiry = chromiumDate(fromWebKitMicroseconds: intValue(row["expires_utc"])) {
        properties[.expires] = expiry
      }
      if let cookie = HTTPCookie(properties: properties) {
        cookies.append(cookie)
      } else {
        stats.cookieRejectedRows += 1
      }
    }
    let dedupedCookies = dedupeCookies(cookies)
    var statsPayload = stats.logPayload(
      family: descriptor.family,
      sourceProfile: profile,
      domainFilterCount: domainFilters.count,
      candidateCookieCount: cookies.count,
      dedupedCookieCount: dedupedCookies.count
    )
    for (key, value) in decryptor.summaryLogPayload {
      statsPayload[key] = value
    }
    let chromiumReadEvent: String
    if skippedEncrypted > 0 {
      chromiumReadEvent = "nativeBrowserImport.chromiumRead.warningSkippedEncrypted"
    } else if rows.isEmpty {
      chromiumReadEvent = "nativeBrowserImport.chromiumRead.warningNoRows"
    } else {
      chromiumReadEvent = "nativeBrowserImport.chromiumRead.summary"
    }
    NativeBrowserImportDebugLog.append(
      event: chromiumReadEvent,
      details: statsPayload)
    var warnings: [String] = []
    if skippedEncrypted > 0 {
      warnings.append("Skipped \(skippedEncrypted) encrypted Chromium cookies that could not be decoded by the native importer.")
    }
    if rows.isEmpty {
      warnings.append("No compatible Chromium cookies were found.")
    }
    return (dedupedCookies, skippedEncrypted, warnings)
  }

  private static func readSQLiteJSONRows(databaseURL: URL, databaseKind: String, sql: String) -> [[String: Any]] {
    guard FileManager.default.fileExists(atPath: databaseURL.path) else {
      NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.sqlite.missing", details: [
        "databaseKind": databaseKind
      ])
      return []
    }
    let tempDirectory = FileManager.default.temporaryDirectory
      .appendingPathComponent("ghostex-browser-import-\(UUID().uuidString)", isDirectory: true)
    do {
      try FileManager.default.createDirectory(at: tempDirectory, withIntermediateDirectories: true)
      defer { try? FileManager.default.removeItem(at: tempDirectory) }
      let snapshotURL = tempDirectory.appendingPathComponent(databaseURL.lastPathComponent, isDirectory: false)
      try FileManager.default.copyItem(at: databaseURL, to: snapshotURL)
      var sidecars: [[String: Any]] = []
      for suffix in ["-wal", "-shm"] {
        let source = URL(fileURLWithPath: "\(databaseURL.path)\(suffix)")
        if FileManager.default.fileExists(atPath: source.path) {
          do {
            try FileManager.default.copyItem(
              at: source,
              to: URL(fileURLWithPath: "\(snapshotURL.path)\(suffix)")
            )
            sidecars.append([
              "kind": suffix == "-wal" ? "wal" : "shm",
              "copied": true,
              "bytes": fileSizeBytes(source),
            ])
          } catch {
            sidecars.append([
              "kind": suffix == "-wal" ? "wal" : "shm",
              "copied": false,
              "bytes": fileSizeBytes(source),
              "reason": "copyFailed",
            ])
          }
        }
      }
      let process = Process()
      let output = Pipe()
      let errorOutput = Pipe()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
      process.arguments = ["-readonly", "-json", snapshotURL.path, sql]
      process.standardInput = FileHandle.nullDevice
      process.standardOutput = output
      process.standardError = errorOutput
      try process.run()
      let data = output.fileHandleForReading.readDataToEndOfFile()
      let errorData = errorOutput.fileHandleForReading.readDataToEndOfFile()
      process.waitUntilExit()
      guard process.terminationStatus == 0 else {
        NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.sqlite.readFailed", details: [
          "databaseKind": databaseKind,
          "databaseBytes": fileSizeBytes(databaseURL),
          "sidecars": sidecars,
          "sqliteExitCode": Int(process.terminationStatus),
          "stdoutBytes": data.count,
          "stderrBytes": errorData.count,
        ])
        return []
      }
      guard let rows = try JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
        NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.sqlite.decodeFailed", details: [
          "databaseKind": databaseKind,
          "databaseBytes": fileSizeBytes(databaseURL),
          "sidecars": sidecars,
          "sqliteExitCode": Int(process.terminationStatus),
          "stdoutBytes": data.count,
          "stderrBytes": errorData.count,
        ])
        return []
      }
      NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.sqlite.read", details: [
        "databaseKind": databaseKind,
        "databaseBytes": fileSizeBytes(databaseURL),
        "sidecars": sidecars,
        "sqliteExitCode": Int(process.terminationStatus),
        "stdoutBytes": data.count,
        "stderrBytes": errorData.count,
        "rowCount": rows.count,
      ])
      return rows
    } catch {
      NativeBrowserImportDebugLog.append(event: "nativeBrowserImport.sqlite.exception", details: [
        "databaseKind": databaseKind,
        "databaseBytes": fileSizeBytes(databaseURL),
        "reason": "snapshotOrLaunchFailed",
      ])
      return []
    }
  }

  private static func fileSizeBytes(_ url: URL) -> Int64 {
    (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.int64Value ?? 0
  }

  private static func importCookiesIntoCEFProfile(
    _ cookies: [HTTPCookie],
    destinationProfileID: UUID
  ) async -> Int {
    guard !cookies.isEmpty else { return 0 }
    return await withCheckedContinuation { continuation in
      DispatchQueue.main.async {
        GhostexCEFImportCookiesForProfile(destinationProfileID.uuidString, cookies) { importedCount in
          continuation.resume(returning: importedCount)
        }
      }
    }
  }

  private static func chromiumDate(fromWebKitMicroseconds rawValue: Int64) -> Date? {
    guard rawValue > 0 else { return nil }
    let unixSeconds = (Double(rawValue) / 1_000_000.0) - 11_644_473_600.0
    guard unixSeconds.isFinite else { return nil }
    return Date(timeIntervalSince1970: unixSeconds)
  }

  private static func domainMatches(host: String, filters: [String]) -> Bool {
    if filters.isEmpty { return true }
    var normalizedHost = host.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    while normalizedHost.hasPrefix(".") {
      normalizedHost.removeFirst()
    }
    guard !normalizedHost.isEmpty else { return false }
    return filters.contains { filter in
      normalizedHost == filter || normalizedHost.hasSuffix(".\(filter)")
    }
  }

  private static func dedupeCookies(_ cookies: [HTTPCookie]) -> [HTTPCookie] {
    var result: [String: HTTPCookie] = [:]
    for cookie in cookies {
      let key = "\(cookie.name.lowercased())|\(cookie.domain.lowercased())|\(cookie.path)"
      let existingExpiry = result[key]?.expiresDate ?? .distantPast
      let candidateExpiry = cookie.expiresDate ?? .distantPast
      if result[key] == nil || candidateExpiry >= existingExpiry {
        result[key] = cookie
      }
    }
    return Array(result.values)
  }

  private static func dedupedProfiles(_ profiles: [NativeBrowserImportSourceProfile]) -> [NativeBrowserImportSourceProfile] {
    var seen = Set<String>()
    return profiles.filter { profile in
      seen.insert(profile.rootURL.path).inserted
    }
  }

  private static func sortProfiles(_ profiles: [NativeBrowserImportSourceProfile]) -> [NativeBrowserImportSourceProfile] {
    dedupedProfiles(profiles).sorted {
      if $0.isDefault != $1.isDefault {
        return $0.isDefault && !$1.isDefault
      }
      return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
    }
  }

  private static func stringValue(_ value: Any?) -> String {
    if let string = value as? String { return string }
    if let number = value as? NSNumber { return number.stringValue }
    return ""
  }

  private static func intValue(_ value: Any?) -> Int64 {
    if let number = value as? NSNumber { return number.int64Value }
    if let string = value as? String { return Int64(string) ?? 0 }
    return 0
  }
}

private struct NativeChromiumCookieDecryptor {
  private let key: Data?
  private let selectedServiceIndex: Int?
  private let keyMaterialAvailable: Bool
  private let keyDerivationStatus: Int
  private let keychainAttemptCount: Int

  init(descriptor: NativeBrowserImportDescriptor) {
    let keychain = Self.safeStoragePassword(services: descriptor.keychainServices)
    let derivation = Self.deriveKey(password: keychain.password)
    self.key = derivation.key
    self.selectedServiceIndex = keychain.selectedServiceIndex
    self.keyMaterialAvailable = keychain.password != nil
    self.keyDerivationStatus = derivation.status
    self.keychainAttemptCount = keychain.attempts.count
    NativeBrowserImportDebugLog.append(
      event: self.key == nil
        ? "nativeBrowserImport.chromiumDecryptor.keyMissing"
        : "nativeBrowserImport.chromiumDecryptor.ready",
      details: [
        "serviceCount": descriptor.keychainServices.count,
        "selectedServiceIndex": keychain.selectedServiceIndex ?? NSNull(),
        "keyMaterialAvailable": keyMaterialAvailable,
        "keyAvailable": self.key != nil,
        "keyDerivationStatus": keyDerivationStatus,
        "keychainAttempts": keychain.attempts,
      ])
  }

  var summaryLogPayload: [String: Any] {
    [
      "keyAvailable": key != nil,
      "keyMaterialAvailable": keyMaterialAvailable,
      "keyDerivationStatus": keyDerivationStatus,
      "selectedServiceIndex": selectedServiceIndex ?? NSNull(),
      "keychainAttemptCount": keychainAttemptCount,
    ]
  }

  func decrypt(encryptedHex: String) -> String? {
    decryptWithDiagnostics(encryptedHex: encryptedHex).value
  }

  func decryptWithDiagnostics(encryptedHex: String) -> (value: String?, failureReason: String?, failureStatus: Int?) {
    guard let key else {
      return (nil, "missingKey", nil)
    }
    guard var encrypted = Self.dataFromHexString(encryptedHex) else {
      return (nil, "invalidHex", nil)
    }
    guard !encrypted.isEmpty else {
      return (nil, "emptyEncryptedValue", nil)
    }
    if encrypted.count > 3,
      String(data: encrypted.prefix(3), encoding: .utf8) == "v10"
    {
      encrypted.removeFirst(3)
    }
    let iv = Data(repeating: 0x20, count: kCCBlockSizeAES128)
    var output = Data(repeating: 0, count: encrypted.count + kCCBlockSizeAES128)
    let keyLength = key.count
    let encryptedLength = encrypted.count
    let outputCapacity = output.count
    var outputLength = 0
    let status = output.withUnsafeMutableBytes { outputBytes in
      encrypted.withUnsafeBytes { encryptedBytes in
        key.withUnsafeBytes { keyBytes in
          iv.withUnsafeBytes { ivBytes in
            CCCrypt(
              CCOperation(kCCDecrypt),
              CCAlgorithm(kCCAlgorithmAES),
              CCOptions(kCCOptionPKCS7Padding),
              keyBytes.baseAddress,
              keyLength,
              ivBytes.baseAddress,
              encryptedBytes.baseAddress,
              encryptedLength,
              outputBytes.baseAddress,
              outputCapacity,
              &outputLength
            )
          }
        }
      }
    }
    guard status == kCCSuccess else {
      return (nil, "cryptorFailed", Int(status))
    }
    output.removeSubrange(outputLength..<output.count)
    guard let value = String(data: output, encoding: .utf8) else {
      return (nil, "invalidUTF8", nil)
    }
    return (value, nil, nil)
  }

  private static func safeStoragePassword(services: [String]) -> (
    password: String?, selectedServiceIndex: Int?, attempts: [[String: Any]]
  ) {
    var attempts: [[String: Any]] = []
    for (index, service) in services.enumerated() {
      let process = Process()
      let output = Pipe()
      let errorOutput = Pipe()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/security")
      process.arguments = ["find-generic-password", "-w", "-s", service]
      process.standardInput = FileHandle.nullDevice
      process.standardOutput = output
      process.standardError = errorOutput
      do {
        try process.run()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        let errorData = errorOutput.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
          attempts.append([
            "index": index,
            "exitCode": Int(process.terminationStatus),
            "stderrBytes": errorData.count,
            "result": "nonzeroExit",
          ])
          continue
        }
        guard let password = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
          !password.isEmpty
        else {
          attempts.append([
            "index": index,
            "exitCode": Int(process.terminationStatus),
            "stderrBytes": errorData.count,
            "result": "emptyOutput",
          ])
          continue
        }
        attempts.append([
          "index": index,
          "exitCode": Int(process.terminationStatus),
          "stderrBytes": errorData.count,
          "result": "selected",
        ])
        return (password, index, attempts)
      } catch {
        attempts.append([
          "index": index,
          "exitCode": NSNull(),
          "stderrBytes": 0,
          "result": "launchFailed",
        ])
        continue
      }
    }
    return (nil, nil, attempts)
  }

  private static func deriveKey(password: String?) -> (key: Data?, status: Int) {
    guard let passwordData = password?.data(using: .utf8), !passwordData.isEmpty else {
      return (nil, Int(kCCParamError))
    }
    let salt = Data("saltysalt".utf8)
    var key = Data(repeating: 0, count: kCCKeySizeAES128)
    let passwordLength = passwordData.count
    let saltLength = salt.count
    let keyLength = key.count
    let status = key.withUnsafeMutableBytes { keyBytes in
      passwordData.withUnsafeBytes { passwordBytes in
        salt.withUnsafeBytes { saltBytes in
          CCKeyDerivationPBKDF(
            CCPBKDFAlgorithm(kCCPBKDF2),
            passwordBytes.bindMemory(to: Int8.self).baseAddress,
            passwordLength,
            saltBytes.bindMemory(to: UInt8.self).baseAddress,
            saltLength,
            CCPseudoRandomAlgorithm(kCCPRFHmacAlgSHA1),
            1003,
            keyBytes.bindMemory(to: UInt8.self).baseAddress,
            keyLength
          )
        }
      }
    }
    return status == kCCSuccess ? (key, Int(status)) : (nil, Int(status))
  }

  private static func dataFromHexString(_ value: String) -> Data? {
    let hex = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard hex.count.isMultiple(of: 2) else { return nil }
    var data = Data()
    var index = hex.startIndex
    while index < hex.endIndex {
      let next = hex.index(index, offsetBy: 2)
      guard let byte = UInt8(hex[index..<next], radix: 16) else {
        return nil
      }
      data.append(byte)
      index = next
    }
    return data
  }
}

@MainActor
private final class NativeBrowserImportDialogController: NSObject {
  private static var activeControllers: [NativeBrowserImportDialogController] = []

  private let parentWindow: NSWindow?
  private let candidates: [NativeBrowserImportCandidate]
  private let browserPopup = NSPopUpButton(frame: .zero, pullsDown: false)
  private let sourceProfilePopup = NSPopUpButton(frame: .zero, pullsDown: false)
  private let destinationProfilePopup = NSPopUpButton(frame: .zero, pullsDown: false)
  private let domainFilterField = NSTextField(frame: .zero)

  static func present(parentWindow: NSWindow?) {
    let candidates = NativeBrowserDataImporter.detectCandidates()
    guard !candidates.isEmpty else {
      let alert = NSAlert()
      alert.messageText = "Import Browser Data"
      alert.informativeText = "No importable browser profiles were found on this Mac."
      alert.alertStyle = .informational
      alert.addButton(withTitle: "OK")
      if let parentWindow {
        alert.beginSheetModal(for: parentWindow)
      } else {
        alert.runModal()
      }
      return
    }
    let controller = NativeBrowserImportDialogController(
      parentWindow: parentWindow,
      candidates: candidates
    )
    activeControllers.append(controller)
    controller.show()
  }

  private init(parentWindow: NSWindow?, candidates: [NativeBrowserImportCandidate]) {
    self.parentWindow = parentWindow
    self.candidates = candidates
    super.init()
  }

  private func show() {
    /*
     CDXC:BrowserImport 2026-06-04-11:40:
     Importing browser data must work from the macOS app without a terminal command. Use a compact native sheet with source browser, source profile, destination Ghostex profile, and optional domain filtering so users can import compatible cookies into the selected browser profile.
     */
    let alert = NSAlert()
    alert.messageText = "Import Browser Data"
    alert.informativeText = "Import compatible cookies into a Ghostex browser profile."
    alert.alertStyle = .informational
    alert.addButton(withTitle: "Import Cookies")
    alert.addButton(withTitle: "Cancel")
    alert.accessoryView = makeAccessoryView()
    let completion: (NSApplication.ModalResponse) -> Void = { [weak self] response in
      guard let self else { return }
      Self.activeControllers.removeAll { $0 === self }
      guard response == .alertFirstButtonReturn else { return }
      self.startImport()
    }
    if let parentWindow {
      alert.beginSheetModal(for: parentWindow, completionHandler: completion)
    } else {
      completion(alert.runModal())
    }
  }

  private func makeAccessoryView() -> NSView {
    browserPopup.target = self
    browserPopup.action = #selector(browserSelectionChanged)
    for candidate in candidates {
      browserPopup.addItem(withTitle: "\(candidate.descriptor.displayName) (\(candidate.profiles.count))")
    }
    rebuildSourceProfiles()

    for profile in NativeBrowserProfileStore.shared.profiles {
      destinationProfilePopup.addItem(withTitle: profile.displayName)
      destinationProfilePopup.lastItem?.representedObject = profile.id.uuidString
    }
    if let index = NativeBrowserProfileStore.shared.profiles.firstIndex(where: {
      $0.id == NativeBrowserProfileStore.shared.effectiveLastUsedProfileID
    }) {
      destinationProfilePopup.selectItem(at: index)
    }

    domainFilterField.placeholderString = "Optional domains, comma-separated"
    for control in [browserPopup, sourceProfilePopup, destinationProfilePopup, domainFilterField] {
      control.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    }

    let grid = NSGridView(views: [
      [label("Source Browser"), browserPopup],
      [label("Source Profile"), sourceProfilePopup],
      [label("Destination Profile"), destinationProfilePopup],
      [label("Domain Filter"), domainFilterField],
    ])
    grid.column(at: 0).xPlacement = .trailing
    grid.column(at: 1).width = 320
    grid.rowSpacing = 8
    grid.columnSpacing = 10
    /*
     CDXC:BrowserImport 2026-06-13-00:49:
     The import modal dropdowns must be fully visible inside the native alert. NSAlert lays out accessory views from their frame, so size the grid from its fitting dimensions before assignment instead of passing a zero-frame grid that AppKit can clip into the button area.
     */
    grid.layoutSubtreeIfNeeded()
    grid.setFrameSize(grid.fittingSize)
    return grid
  }

  @objc private func browserSelectionChanged() {
    rebuildSourceProfiles()
  }

  private func rebuildSourceProfiles() {
    sourceProfilePopup.removeAllItems()
    let candidate = candidates[max(0, browserPopup.indexOfSelectedItem)]
    for profile in candidate.profiles {
      sourceProfilePopup.addItem(withTitle: profile.displayName)
    }
  }

  private func startImport() {
    let candidate = candidates[max(0, browserPopup.indexOfSelectedItem)]
    let sourceProfile = candidate.profiles[max(0, sourceProfilePopup.indexOfSelectedItem)]
    let destinationIndex = max(0, destinationProfilePopup.indexOfSelectedItem)
    let destination = NativeBrowserProfileStore.shared.profiles[destinationIndex]
    NativeBrowserProfileStore.shared.noteUsed(destination.id)
    let filters = NativeBrowserDataImporter.parseDomainFilters(domainFilterField.stringValue)

    Task.detached {
      let outcome = await NativeBrowserDataImporter.importCookies(
        from: candidate,
        sourceProfile: sourceProfile,
        destinationProfile: destination,
        domainFilters: filters
      )
      await MainActor.run {
        NativeBrowserImportDialogController.presentOutcome(outcome, parentWindow: self.parentWindow)
      }
    }
  }

  private static func presentOutcome(_ outcome: NativeBrowserCookieImportOutcome, parentWindow: NSWindow?) {
    let alert = NSAlert()
    alert.messageText = "Browser Data Import Complete"
    var lines = [
      "Browser: \(outcome.browserName)",
      "Source profile: \(outcome.sourceProfileName)",
      "Destination profile: \(outcome.destinationProfileName)",
      "Imported cookies: \(outcome.importedCookies)",
      "Skipped cookies: \(outcome.skippedCookies)",
    ]
    if !outcome.warnings.isEmpty {
      lines.append("")
      lines.append("Warnings:")
      lines.append(contentsOf: outcome.warnings.map { "- \($0)" })
    }
    alert.informativeText = lines.joined(separator: "\n")
    alert.alertStyle = outcome.importedCookies > 0 ? .informational : .warning
    alert.addButton(withTitle: "OK")
    if let parentWindow {
      alert.beginSheetModal(for: parentWindow)
    } else {
      alert.runModal()
    }
  }

  private func label(_ title: String) -> NSTextField {
    let label = NSTextField(labelWithString: title)
    label.alignment = .right
    return label
  }
}
