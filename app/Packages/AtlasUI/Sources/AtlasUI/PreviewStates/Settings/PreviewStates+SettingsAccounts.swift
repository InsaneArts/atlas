#if DEBUG
  extension PreviewStates {
    /// What the Accounts page says with no accounts listed.
    static let settingsAccountsNone = EmptyMessageState(
      headline: "No accounts yet",
      message: "Agents work freely and nothing asks you. If a task needs an account, sign the helper in here."
    )

    /// The Accounts page with a site and an app listed.
    static let settingsAccountsPane = SettingsAccountsState(
      rows: [
        SettingsAccountRow(
          id: "site\u{1F}github.com",
          letter: "G",
          title: "github.com",
          detail: "Added today",
          isSite: true
        ),
        SettingsAccountRow(id: "app\u{1F}slack", letter: "S", title: "Slack", detail: "App · added Nov 5"),
        SettingsAccountRow(
          id: "site\u{1F}staging.acme.dev",
          letter: "S",
          title: "staging.acme.dev",
          detail: "Added Nov 3",
          level: .trusted,
          isSite: true
        ),
      ],
      empty: settingsAccountsNone
    )

    /// The Accounts page, listing three accounts.
    @MainActor static let settingsAccounts: SettingsState = selecting(.accounts)

    /// The Accounts page with none listed.
    @MainActor static let settingsAccountsEmpty: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.rows = []
      return state
    }()

    /// The Accounts page just after Atlas added an account the person typed a password in.
    @MainActor static let settingsAccountsAdded: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.note = "Added github.com after you typed a password there. Agents ask you before sending there."
      state.accounts.noteAccount = "site\u{1F}github.com"
      return state
    }()

    /// The Accounts page while General keeps no pictures of agent actions: the line under the list says so.
    @MainActor static let settingsAccountsPicturesOff: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.picturesOff = "Activity won't keep pictures of what agents do here."
      return state
    }()

    /// The Accounts page with a bank agents may only look at.
    @MainActor static let settingsAccountsLookOnly: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.rows = [
        SettingsAccountRow(
          id: "site\u{1F}github.com",
          letter: "G",
          title: "github.com",
          detail: "Added today",
          isSite: true
        ),
        SettingsAccountRow(
          id: "site\u{1F}mybank.com",
          letter: "M",
          title: "mybank.com",
          detail: "Added Nov 2",
          level: .lookOnly,
          isSite: true
        ),
      ]
      return state
    }()

    /// The Accounts page when Atlas can't read its list.
    @MainActor static let settingsAccountsUnread: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.rows = nil
      return state
    }()

    /// The Accounts page with github.com opened: signing out, forgetting it, resetting the browser and removing it.
    @MainActor static let settingsAccountOpened: SettingsState = {
      var state = selecting(.accounts)
      state.accounts.open = "site\u{1F}github.com"
      return state
    }()

    /// Sign In to a Site… over the Accounts page, a site typed.
    @MainActor static let settingsSignIn: SettingsState = {
      var state = selecting(.accounts)
      state.signIn = SignInSheetState(site: "github.com")
      return state
    }()

    /// Sign In to a Site… refused, as while an agent's command runs.
    @MainActor static let settingsSignInRefused: SettingsState = {
      var state = selecting(.accounts)
      state.signIn = SignInSheetState(
        site: "github.com",
        refusal: "An agent's command is still running. Try again when it ends."
      )
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
