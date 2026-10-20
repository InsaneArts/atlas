import AtlasUI

extension OnboardingAccountPick {
  /// The pick AtlasKit names: an empty short name is a new account, as `OnboardingFacts.defaultPick` gives it.
  ///
  /// - Parameter name: A short name, or empty for a new account.
  init(name: String) {
    self = name.isEmpty ? .newAccount : .existing(name)
  }
}
