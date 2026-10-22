/// What a new helper account's password must be before Atlas hands it to macOS.
package enum PasswordRule {
  /// The longest password, in UTF-8 bytes, that Atlas passes on: 256, what `atlas onboarding create` reads from
  /// standard input (`src/lib/onboarding.rs`). An accented letter or a symbol takes more than one byte.
  static let maxBytes = 256

  /// What is wrong with the password, if anything; nothing while either field is empty.
  ///
  /// - Parameters:
  ///   - password: The password typed.
  ///   - verify: The password typed again.
  /// - Returns: One line saying what to change, or `nil` when there is nothing to say yet.
  package static func problem(password: String, verify: String) -> String? {
    if password.utf8.count > maxBytes { return "That password is too long. Use a shorter one." }
    if password.unicodeScalars.contains(where: { $0.properties.generalCategory == .control }) {
      return "Use a password without control characters."
    }
    if password.isEmpty || verify.isEmpty { return nil }
    return password == verify ? nil : "The two passwords are different."
  }

  /// Whether the account form may be sent: every field filled, the short name one macOS takes, and the password fine
  /// and confirmed.
  ///
  /// - Parameters:
  ///   - fullName: The account's full name.
  ///   - accountName: The account's short name.
  ///   - password: The password typed.
  ///   - verify: The password typed again.
  /// - Returns: Whether the account can be created.
  package static func canCreate(fullName: String, accountName: String, password: String, verify: String) -> Bool {
    !fullName.isEmpty && !accountName.isEmpty && !password.isEmpty && password == verify
      && problem(password: password, verify: verify) == nil && AccountNameRule.problem(accountName) == nil
  }
}
