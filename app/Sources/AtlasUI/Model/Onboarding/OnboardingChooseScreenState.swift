/// The account choice: a new account first, then each standard account that could be the helper.
package struct OnboardingChooseScreenState: Equatable, Sendable {
  /// Whether the new account, the first row, is the one picked.
  package var isNewAccountPicked: Bool
  /// Each standard account already on this Mac, in order, after the new account.
  package var accounts: [OnboardingAccountChoice]
  /// Whether a new account is offered: not once Atlas was set up in the account chosen before, which it keeps using.
  package var offersNewAccount: Bool
  /// What approving in macOS will do for the existing account picked; nil when no approval comes.
  package var approval: String?

  /// Creates the account choice's state.
  ///
  /// - Parameters:
  ///   - isNewAccountPicked: Whether the new account, the first row, is the one picked.
  ///   - accounts: Each standard account already on this Mac, in order, after the new account.
  ///   - offersNewAccount: Whether a new account is offered.
  ///   - approval: What approving in macOS will do for the existing account picked; nil when no approval comes.
  package init(
    isNewAccountPicked: Bool,
    accounts: [OnboardingAccountChoice],
    offersNewAccount: Bool = true,
    approval: String? = nil
  ) {
    self.isNewAccountPicked = isNewAccountPicked
    self.accounts = accounts
    self.offersNewAccount = offersNewAccount
    self.approval = approval
  }
}
