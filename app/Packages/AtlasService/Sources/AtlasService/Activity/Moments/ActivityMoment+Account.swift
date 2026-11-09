extension ActivityMoment {
  /// The rows of the run that acted in an app, by name or bundle identifier: the ones an account belongs to. A
  /// screenshot or a command in no app is left out.
  private var appRows: [ActivityLog.Entry] {
    runs.flatMap(\.members)
      .filter { ActivityApp($0) != nil }
  }

  /// The one listed account the run acted in: every row of it in an app names the same one. Nil when one names
  /// none, as on a page outside the account, or two name different ones.
  public var account: ActivityLog.Account? {
    let rows = appRows
    guard let first = rows.first?.account, rows.allSatisfy({ $0.account == first }) else { return nil }
    return first
  }

  /// The site the run acted in, which titles it in place of the browser's name: its account's, when that is a site.
  public var site: String? {
    guard let account, account.key.kind == .site else { return nil }
    return account.name
  }

  /// What let the run act in its one listed account: every row of it in an app was in that account and allowed the
  /// same way. Nil when a row was outside the account, or one was allowed otherwise or needed no allowing.
  public var allowance: ActivityLog.Allowance? {
    let rows = appRows
    guard account != nil, let first = rows.first?.allowed, rows.allSatisfy({ $0.allowed == first }) else {
      return nil
    }
    return first
  }
}
