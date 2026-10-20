/// The new helper account's form: the one value the setup window edits in place. The password lives here only until
/// it is handed to macOS, and never prints (see `OnboardingAccountForm+Description`).
package struct OnboardingAccountForm: Equatable, Sendable {
  /// The account's full name.
  package var fullName: String
  /// The account's short name.
  package var accountName: String
  /// The password typed.
  package var password: String
  /// The password typed again.
  package var verify: String

  /// Creates a form, its names filled with the helper's usual ones unless given.
  ///
  /// - Parameters:
  ///   - fullName: The account's full name.
  ///   - accountName: The account's short name.
  ///   - password: The password.
  ///   - verify: The password, typed again.
  package init(fullName: String = "Atlas", accountName: String = "atlas", password: String = "", verify: String = "") {
    self.fullName = fullName
    self.accountName = accountName
    self.password = password
    self.verify = verify
  }
}
