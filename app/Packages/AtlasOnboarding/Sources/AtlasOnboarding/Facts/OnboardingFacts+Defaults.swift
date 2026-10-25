extension OnboardingFacts {
  /// The account picked before the person picks one: the helper already chosen when it is listed, else a new account
  /// when no standard account could be one and a new one is offered, else none, so the person chooses. Never an
  /// account the choice does not show.
  public var defaultPick: String? {
    if helper.exists, candidates.contains(where: { $0.name == helper.name }) { return helper.name }
    if onlyChoice { return nil }
    return candidates.isEmpty ? "" : nil
  }

  /// Whether `name` can be picked now: a listed account, or a new one (empty) while new ones are offered.
  ///
  /// - Parameter name: A short name, or empty for a new account.
  /// - Returns: Whether the choice shows it.
  public func offers(_ name: String) -> Bool {
    name.isEmpty ? !onlyChoice : candidates.contains { $0.name == name }
  }

  /// Open at Login's first value in the setup window: the person's earlier choice stands, and the first setup turns
  /// it on.
  ///
  /// - Parameter enabledNow: Whether Atlas opens at login now.
  /// - Returns: Whether the Done screen's switch starts on.
  public func opensAtLogin(enabledNow: Bool) -> Bool {
    loginDecided ? enabledNow : true
  }
}
