/// Which account the person picks to be the helper: a new one, or a standard account already on this Mac.
public enum OnboardingAccountPick: Hashable, Sendable {
  /// A new account, made with the form.
  case newAccount
  /// A standard account already on this Mac.
  ///
  /// - Parameter name: Its short name.
  case existing(String)
}
