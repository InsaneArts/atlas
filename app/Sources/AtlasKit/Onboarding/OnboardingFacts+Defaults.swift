extension OnboardingFacts {
  /// The account picked before the person picks one: the helper already chosen, else a new account when no standard
  /// account could be one, else none, so the person chooses.
  package var defaultPick: String? {
    if helper.exists { return helper.name }
    return candidates.isEmpty ? "" : nil
  }

  /// Open at Login's first value in the setup window: the person's earlier choice stands, and the first setup turns
  /// it on.
  ///
  /// - Parameter enabledNow: Whether Atlas opens at login now.
  /// - Returns: Whether the Done screen's switch starts on.
  package func opensAtLogin(enabledNow: Bool) -> Bool {
    loginDecided ? enabledNow : true
  }
}
