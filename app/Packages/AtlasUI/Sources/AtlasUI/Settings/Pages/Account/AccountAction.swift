/// One thing an account's page offers: its name, what it does, its button's word, and what pressing it sends.
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

  /// What a site's page offers under "Sign-In": signing out, forgetting it and resetting the browser, each with agents
  /// paused.
  ///
  /// - Parameter id: The account.
  /// - Returns: The actions, in order.
  static func signIn(of id: String) -> [AccountAction] {
    [
      AccountAction(
        title: "Sign out",
        detail: "Opens the site's own sign-out, with agents paused",
        button: "Sign Out…",
        event: .signOutAccount(id)
      ),
      AccountAction(
        title: "Forget this site",
        detail: "Opens the browser's settings to delete this site's data, with agents paused",
        button: "Forget…",
        event: .forgetAccount(id)
      ),
      AccountAction(
        title: "Reset the helper's browser",
        detail: "Opens the browser's settings to clear its data, with agents paused",
        button: "Reset…",
        event: .resetBrowser(id)
      ),
    ]
  }

  /// Taking any account off the list, last on its page.
  ///
  /// - Parameter id: The account.
  /// - Returns: The action.
  static func remove(_ id: String) -> AccountAction {
    AccountAction(
      title: "Remove from list",
      detail: "Atlas stops listing it. Removing it doesn't sign the helper out",
      button: "Remove…",
      event: .askRemoveAccount(id)
    )
  }
}
