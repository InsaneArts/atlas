extension Permission {
  /// The permission the window guides next: `current` while its switch is off, else the first one still off, else
  /// none, once both are on.
  ///
  /// - Parameters:
  ///   - current: The permission guided now, if any.
  ///   - asker: This account's state, in the helper.
  /// - Returns: The permission to guide, or nil when there is none left.
  public static func toGuide(after current: Permission?, asker: OnboardingFacts.Asker) -> Permission? {
    if let current, !current.granted(to: asker) { return current }
    return allCases.first { !$0.granted(to: asker) }
  }
}
