#if DEBUG
  extension PreviewStates {
    /// What the Accounts pane says with no accounts listed.
    static let settingsAccountsNone = EmptyMessageState(
      headline: "No accounts yet",
      message: "Agents work freely and nothing asks you. If a task needs an account, sign the helper in here.",
      action: EmptyMessageAction(title: "Add an App…", isPrimary: false)
    )

    /// The Accounts pane with a site and an app listed.
    static let settingsAccountsPane = SettingsAccountsState(
      rows: [
        SettingsAccountRow(id: "site\u{1F}github.com", letter: "G", title: "github.com", detail: "Added today"),
        SettingsAccountRow(id: "app\u{1F}slack", letter: "S", title: "Slack", detail: "App · added Nov 5"),
        SettingsAccountRow(id: "site\u{1F}acme.dev", letter: "A", title: "acme.dev", detail: "Added Nov 3"),
      ],
      empty: settingsAccountsNone
    )

    /// The Accounts pane, listing three accounts.
    @MainActor static let settingsAccounts: SettingsState = selecting(.accounts)

    /// The Accounts pane with none listed.
    @MainActor static let settingsAccountsEmpty: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.rows = []
      return state
    }()

    /// The Accounts pane just after Atlas added an account the person typed a password in.
    @MainActor static let settingsAccountsAdded: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.note = "Added github.com after you typed a password there."
      return state
    }()

    /// The Accounts pane when Atlas can't read its list.
    @MainActor static let settingsAccountsUnread: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.rows = nil
      return state
    }()

    /// Remove asked for an account: it stays listed until the person confirms.
    @MainActor static let settingsRemoveAccount: SettingsState = {
      var state = selecting(.accounts)
      state.alert = AlertCardState(
        title: "Remove github.com from Accounts?",
        message: "Atlas stops listing it. Removing it from the list doesn't sign the helper out.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Remove", role: .destructive),
        ]
      )
      return state
    }()
  }
#endif
