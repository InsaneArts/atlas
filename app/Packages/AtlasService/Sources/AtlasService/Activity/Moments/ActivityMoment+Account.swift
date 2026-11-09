extension ActivityMoment {
  /// The one listed account the run acted in: every row of it in an app names the same one. Nil when one names
  /// none, as on a page outside the account, or two name different ones.
  public var account: ActivityLog.Account? {
    let inApp = runs.flatMap(\.members)
      .filter { $0.app != nil }
    guard let first = inApp.first?.account, inApp.allSatisfy({ $0.account == first }) else { return nil }
    return first
  }

  /// The site the run acted in, which titles it in place of the browser's name: its account's, when that is a site.
  public var site: String? {
    guard let account, account.key.kind == .site else { return nil }
    return account.name
  }

  /// What let the run's rows act in their listed account, when every row that says agrees; nil when none says or
  /// they differ.
  public var allowance: ActivityLog.Allowance? {
    let allowed = Set(runs.flatMap(\.members).compactMap(\.allowed))
    guard allowed.count == 1 else { return nil }
    return allowed.first
  }
}
