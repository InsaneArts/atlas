/// One thing an opened account offers: its name, what it does, its button's word, and what pressing it sends.
struct AccountAction: Identifiable {
  /// Its name, such as "Forget this site".
  let title: String
  /// What it does, such as "Opens the browser's settings to delete this site's data, with agents paused".
  let detail: String
  /// Its button's word, such as "Forget…".
  let button: String
  /// What pressing it sends.
  let event: SettingsEvent

  /// The action, by its name.
  var id: String { title }

  /// What an account offers once opened: for a site, signing out, forgetting it and resetting the browser, each with
  /// agents paused; then, for any account, taking it off the list.
  ///
  /// - Parameter row: The account.
  /// - Returns: The actions, in order.
  static func of(_ row: SettingsAccountRow) -> [AccountAction] {
    let remove = AccountAction(
      title: "Remove from list",
      detail: "Atlas stops listing it. Removing it doesn't sign the helper out",
      button: "Remove…",
      event: .askRemoveAccount(row.id)
    )
    guard row.isSite else { return [remove] }
    return [
      AccountAction(
        title: "Sign out",
        detail: "Opens the site's own sign-out, with agents paused",
        button: "Sign Out…",
        event: .signOutAccount(row.id)
      ),
      AccountAction(
        title: "Forget this site",
        detail: "Opens the browser's settings to delete this site's data, with agents paused",
        button: "Forget…",
        event: .forgetAccount(row.id)
      ),
      AccountAction(
        title: "Reset the helper's browser",
        detail: "Opens the browser's settings to clear its data, with agents paused",
        button: "Reset…",
        event: .resetBrowser(row.id)
      ),
      remove,
    ]
  }
}
