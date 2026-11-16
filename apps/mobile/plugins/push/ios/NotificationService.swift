// The Notification Service Extension (ADR 0010): a push names only an account, a room and an
// event (`event_id_only`), with a generic text. Before iOS shows it, this asks the engine's
// library (bm_push_fetch, packages/matrix-ffi/src/push.rs) to open the account's store in the
// App Group container, fetch and decrypt the event, and word the notification; it shows the
// generic text when that fails or time runs out. While the app is in front the library leaves
// the push alone, and the app keeps it from showing (its own sync notifies).
//
// Copied into ios/BetterMatrixPush by plugins/withPush.js; never edit the copy.

import Foundation
import Security
import UserNotifications

final class NotificationService: UNNotificationServiceExtension {
  private var contentHandler: ((UNNotificationContent) -> Void)?
  private var fallback: UNNotificationContent?
  private let lock = NSLock()
  private var delivered = false

  override func didReceive(
    _ request: UNNotificationRequest,
    withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
  ) {
    self.contentHandler = contentHandler
    fallback = request.content
    guard
      let content = request.content.mutableCopy() as? UNMutableNotificationContent,
      let dataDir = PushEnvironment.dataDirectory,
      let userInfo = request.content.userInfo as? [String: Any],
      JSONSerialization.isValidJSONObject(userInfo),
      let payload = try? JSONSerialization.data(withJSONObject: userInfo),
      let json = String(data: payload, encoding: .utf8)
    else {
      deliver(request.content)
      return
    }
    DispatchQueue.global(qos: .userInitiated).async {
      if let shown = PushBridge.fetch(payload: json, dataDir: dataDir) {
        content.title = shown.title
        content.body = shown.body
        content.threadIdentifier = shown.thread
      }
      self.deliver(content)
    }
  }

  override func serviceExtensionTimeWillExpire() {
    if let fallback { deliver(fallback) }
  }

  /// Hands iOS the notification, once.
  private func deliver(_ content: UNNotificationContent) {
    lock.lock()
    let first = !delivered
    delivered = true
    lock.unlock()
    if first { contentHandler?(content) }
  }
}

/// What the extension shares with the app, from its Info.plist (written by the config plugin).
enum PushEnvironment {
  /// The engine's data directory in the App Group container: `<container>/matrix`.
  static var dataDirectory: String? {
    guard
      let group = Bundle.main.object(forInfoDictionaryKey: "BMAppGroup") as? String,
      let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
    else { return nil }
    return container.appendingPathComponent("matrix", isDirectory: true).path
  }

  /// The keychain access group the app's secrets live in (`<team>.chat.bettermatrix.mobile`).
  static var keychainGroup: String? {
    Bundle.main.object(forInfoDictionaryKey: "BMKeychainGroup") as? String
  }
}

/// What the library said to show.
struct Shown: Decodable {
  let title: String
  let body: String
  let thread: String
}

/// The library's answer (`matrix_ffi::push::NativePush`).
private struct NativePush: Decodable {
  let shown: Shown?
}

/// Calls the engine's library.
enum PushBridge {
  /// The notification to show for `payload`, or nil to keep the push's own text.
  static func fetch(payload: String, dataDir: String) -> Shown? {
    let secrets = BmSecrets(
      context: nil,
      get: { _, key, value in
        guard let key, let value else { return false }
        switch Keychain.get(String(cString: key)) {
        case .failed: return false
        case .missing:
          value.pointee = nil
          return true
        case .found(let text):
          value.pointee = strdup(text)
          return true
        }
      },
      set: { _, key, value in
        guard let key, let value else { return false }
        return Keychain.set(String(cString: key), String(cString: value))
      },
      delete: { _, key in
        guard let key else { return false }
        return Keychain.delete(String(cString: key))
      }
    )
    guard let answer = bm_push_fetch(payload, dataDir, secrets) else { return nil }
    defer { bm_push_free(answer) }
    let data = Data(String(cString: answer).utf8)
    return (try? JSONDecoder().decode(NativePush.self, from: data))?.shown
  }
}

/// The app's secrets as expo-secure-store keeps them (apps/mobile/src/engine/secrets.ts): generic
/// passwords under the service `chat.bettermatrix.mobile:no-auth`, the (already escaped) key as
/// account and generic attribute, readable after the first unlock, in the shared access group.
enum Keychain {
  enum Read {
    case found(String)
    case missing
    case failed
  }

  private static let service = "chat.bettermatrix.mobile"

  private static func query(_ key: String, service: String) -> [String: Any] {
    let encoded = Data(key.utf8)
    var query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrGeneric as String: encoded,
      kSecAttrAccount as String: encoded,
    ]
    if let group = PushEnvironment.keychainGroup { query[kSecAttrAccessGroup as String] = group }
    return query
  }

  static func get(_ key: String) -> Read {
    // expo-secure-store reads its current name first, then the one older versions wrote.
    for name in ["\(service):no-auth", service] {
      var search = query(key, service: name)
      search[kSecMatchLimit as String] = kSecMatchLimitOne
      search[kSecReturnData as String] = kCFBooleanTrue
      var item: CFTypeRef?
      let status = SecItemCopyMatching(search as CFDictionary, &item)
      switch status {
      case errSecSuccess:
        guard let data = item as? Data, let text = String(data: data, encoding: .utf8) else { return .failed }
        return .found(text)
      case errSecItemNotFound:
        continue
      default:
        return .failed
      }
    }
    return .missing
  }

  static func set(_ key: String, _ value: String) -> Bool {
    let name = "\(service):no-auth"
    var add = query(key, service: name)
    add[kSecValueData as String] = Data(value.utf8)
    add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    let status = SecItemAdd(add as CFDictionary, nil)
    if status == errSecSuccess { return true }
    guard status == errSecDuplicateItem else { return false }
    let update: [String: Any] = [
      kSecValueData as String: Data(value.utf8),
      kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
    ]
    return SecItemUpdate(query(key, service: name) as CFDictionary, update as CFDictionary) == errSecSuccess
  }

  static func delete(_ key: String) -> Bool {
    var ok = true
    for name in ["\(service):no-auth", service] {
      let status = SecItemDelete(query(key, service: name) as CFDictionary)
      if status != errSecSuccess && status != errSecItemNotFound { ok = false }
    }
    return ok
  }
}
