import AtlasVMKit
import Foundation
import Security

/// The guest's passwords, made at random once for a VM and kept in this
/// Mac's Keychain, never in a file, an argument or a log: its own account's,
/// and its second administrator's.
enum GuestPassword {
  private static let service = "com.insanearts.atlas.vm"

  /// The password of the guest of the desktop `desktop`, made and kept the
  /// first time, never shown.
  static func of(_ desktop: String) throws -> String {
    try kept(desktop) {
      let letters = Array("abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789")
      var generator = SystemRandomNumberGenerator()
      return String((0..<20).map { _ in letters.randomElement(using: &generator) ?? "a" })
    }
  }

  /// The password of the second administrator of the guest of the desktop
  /// `desktop`: kept already for a VM made before, else made the first time
  /// as a passphrase the person can type (``AdminPassphrase``), which its
  /// setup window shows.
  static func admin(_ desktop: String) throws -> String {
    try kept(account(admin: desktop)) {
      var generator = SystemRandomNumberGenerator()
      return AdminPassphrase.make(using: &generator)
    }
  }

  /// The second administrator's password of the guest of `desktop`, as its
  /// creation kept it; none when none is kept or it cannot be read. Read
  /// only: it is never made here.
  static func shownAdmin(_ desktop: String) -> String? {
    (try? read(account(admin: desktop))) ?? nil
  }

  /// The Keychain account the second administrator's password of the guest
  /// of `desktop` is kept under.
  private static func account(admin desktop: String) -> String { "\(desktop).admin" }

  /// The password kept for `account`, or made by `make` and kept the first
  /// time.
  private static func kept(_ account: String, made make: () -> String) throws -> String {
    if let kept = try read(account) { return kept }
    let made = make()
    let item: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecValueData as String: Data(made.utf8),
      kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
    ]
    guard SecItemAdd(item as CFDictionary, nil) == errSecSuccess else { throw AtlasVMError.password }
    return made
  }

  /// Delete the passwords of the guest of `desktop` and of its second
  /// administrator, Keychain interaction turned off around each call, so a
  /// locked Keychain refuses rather than asks; the accounts left.
  static func forget(_ desktop: String) -> [String] {
    [desktop, account(admin: desktop)]
      .filter { delete($0) == .left }
  }

  /// Delete the password of `account`: what became of it. Never tried
  /// unless interaction is known to be off, and put back as it was.
  static func delete(_ account: String) -> PasswordFate {
    var allowed: DarwinBoolean = true
    guard SecKeychainGetUserInteractionAllowed(&allowed) == errSecSuccess,
      SecKeychainSetUserInteractionAllowed(false) == errSecSuccess
    else {
      return .left
    }
    defer { SecKeychainSetUserInteractionAllowed(allowed.boolValue) }
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
    ]
    switch SecItemDelete(query as CFDictionary) {
    case errSecSuccess: return .deleted
    case errSecItemNotFound: return .absent
    default: return .left
    }
  }

  private static func read(_ account: String) throws -> String? {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecReturnData as String: true,
    ]
    var found: CFTypeRef?
    switch SecItemCopyMatching(query as CFDictionary, &found) {
    case errSecSuccess:
      guard let data = found as? Data else { throw AtlasVMError.password }
      return String(decoding: data, as: UTF8.self)
    case errSecItemNotFound:
      return nil
    default:
      throw AtlasVMError.password
    }
  }
}
