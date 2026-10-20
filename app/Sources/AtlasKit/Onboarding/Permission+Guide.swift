extension Permission {
  /// Whether the guide panel beside System Settings stays open: only on the permissions step, until its switch is on.
  ///
  /// - Parameters:
  ///   - step: The step the facts and choices lead to now.
  ///   - asker: This account's state, in the helper.
  /// - Returns: Whether the guide stays; once false, it steps aside.
  package func guideStays(on step: OnboardingStep, asker: OnboardingFacts.Asker) -> Bool {
    step == .permissions && !granted(to: asker)
  }
}
