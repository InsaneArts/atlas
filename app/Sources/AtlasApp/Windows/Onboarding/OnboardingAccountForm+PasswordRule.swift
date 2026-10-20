import AtlasKit
import AtlasUI

extension OnboardingAccountForm {
  /// Whether the form may be sent: every field filled, and the password fine and confirmed (``PasswordRule``).
  var canCreate: Bool {
    PasswordRule.canCreate(fullName: fullName, accountName: accountName, password: password, verify: verify)
  }
}
