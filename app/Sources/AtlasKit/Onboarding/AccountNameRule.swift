/// What a new helper account's short name must be: the rule `atlas onboarding create` applies
/// (`valid_account_name` in `src/lib/init.rs`), checked here as the person types so the form never sends a name
/// the command line refuses.
package enum AccountNameRule {
  /// The longest short name macOS takes: 31 characters.
  static let maxLength = 31

  /// What is wrong with the short name, if anything; nothing while it is empty, which the form's button waits on.
  ///
  /// - Parameter name: The short name typed.
  /// - Returns: One line saying what to change, or `nil` when there is nothing to say yet.
  package static func problem(_ name: String) -> String? {
    if name.isEmpty { return nil }
    if name.utf8.count > maxLength { return "Use at most 31 characters in the account name." }
    let allowed = name.utf8.allSatisfy { byte in
      (byte >= 0x30 && byte <= 0x39) || (byte >= 0x41 && byte <= 0x5A) || (byte >= 0x61 && byte <= 0x7A)
        || byte == UInt8(ascii: ".") || byte == UInt8(ascii: "_") || byte == UInt8(ascii: "-")
    }
    if !allowed { return "Use only letters, digits, '.', '_' and '-' in the account name, with no spaces." }
    if name.hasPrefix(".") || name.hasPrefix("-") { return "Start the account name with a letter or a digit." }
    return nil
  }
}
