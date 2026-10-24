import AtlasOnboarding
import AtlasUI

extension OnboardingAccountForm {
  /// Whether the form may be sent: every field filled, the short name one macOS takes (``AccountNameRule``), and the
  /// password fine and confirmed (``PasswordRule``).
  var canCreate: Bool {
    PasswordRule.canCreate(fullName: fullName, accountName: accountName, password: password, verify: verify)
  }
}
