import Foundation
import Security

/// The guest account's password, made at random once for a VM and kept in
/// this Mac's Keychain, never in a file or an argument.
enum GuestPassword {
  private static let service = "com.insanearts.atlas.vm"

  /// The password of the guest of the desktop `desktop`, made and kept the
  /// first time.
  static func of(_ desktop: String) throws -> String {
    if let kept = try read(desktop) { return kept }
    let letters = Array("abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789")
    var generator = SystemRandomNumberGenerator()
    let made = String((0..<20).map { _ in letters.randomElement(using: &generator) ?? "a" })
    let item: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: desktop,
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
    [desktop, "\(desktop).admin"]
      .filter { !deleted($0) }
  }

  /// Whether the password of `account` is gone, or was never kept; never
  /// tried unless interaction is known to be off, and put back as it was.
  private static func deleted(_ account: String) -> Bool {
    var allowed: DarwinBoolean = true
    guard SecKeychainGetUserInteractionAllowed(&allowed) == errSecSuccess,
      SecKeychainSetUserInteractionAllowed(false) == errSecSuccess
    else {
      return false
    }
    defer { SecKeychainSetUserInteractionAllowed(allowed.boolValue) }
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
    ]
    let status = SecItemDelete(query as CFDictionary)
    return status == errSecSuccess || status == errSecItemNotFound
  }

  private static func read(_ desktop: String) throws -> String? {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: desktop,
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
