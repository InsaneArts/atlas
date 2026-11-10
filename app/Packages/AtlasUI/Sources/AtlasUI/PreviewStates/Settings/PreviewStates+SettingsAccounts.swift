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
          detail: "Ask before sending · Added today",
          isSite: true
        ),
        SettingsAccountRow(
          id: "app\u{1F}slack",
          letter: "S",
          title: "Slack",
          detail: "Ask before sending · App · added Nov 5"
        ),
        SettingsAccountRow(
          id: "site\u{1F}staging.acme.dev",
          letter: "S",
          title: "staging.acme.dev",
          detail: "Trusted · Added Nov 3",
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
          detail: "Ask before sending · Added today",
          isSite: true
        ),
        SettingsAccountRow(
          id: "site\u{1F}mybank.com",
          letter: "M",
          title: "mybank.com",
          detail: "Look only · Added Nov 2",
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

    /// github.com's page: what agents may do there, signing out, forgetting it, resetting the browser and removing it.
    static let settingsAccountSitePage = SettingsAccountPageState(
      id: "site\u{1F}github.com",
      letter: "G",
      title: "github.com",
      kindLine: "Website · added today",
      isSite: true,
      level: .ask,
      levelLine: "Agents ask you before sending there.",
      isEnabled: true
    )

    /// Slack's page: what agents may do there and removing it.
    static let settingsAccountAppPage = SettingsAccountPageState(
      id: "app\u{1F}slack",
      letter: "S",
      title: "Slack",
      kindLine: "App · added Nov 5",
      isSite: false,
      level: .trusted,
      levelLine: "Agents act there without asking.",
      isEnabled: true
    )

    /// github.com's page while Atlas can't read its list: its name stays, its choices wait.
    static let settingsAccountUnreadablePage = SettingsAccountPageState(
      id: "site\u{1F}github.com",
      letter: "G",
      title: "github.com",
      kindLine: "Website · added today",
      isSite: true,
      level: nil,
      levelLine: "",
      isEnabled: false
    )

    /// github.com's page after a level change failed and the reading after it couldn't be read.
    static let settingsAccountFailedUnreadablePage: SettingsAccountPageState = {
      var page = settingsAccountUnreadablePage
      page.notice = SettingsNote(text: "Atlas couldn't change its accounts list. The service stopped.", kind: .problem)
      return page
    }()

    /// github.com's page, opened from Accounts.
    @MainActor static let settingsAccountOpened: SettingsState = opening(settingsAccountSitePage)

    /// Slack's page, opened from Accounts.
    @MainActor static let settingsAccountApp: SettingsState = opening(settingsAccountAppPage)

    /// github.com's page while Atlas can't read its list.
    @MainActor static let settingsAccountUnreadable: SettingsState = {
      var state = opening(settingsAccountUnreadablePage)
      state.accounts.rows = nil
      return state
    }()

    /// github.com's page after a change failed while Atlas can't read its list.
    @MainActor static let settingsAccountFailedUnreadable: SettingsState = {
      var state = opening(settingsAccountFailedUnreadablePage)
      state.accounts.rows = nil
      return state
    }()

    /// An account's page, opened from Accounts.
    ///
    /// - Parameter page: The account's page.
    /// - Returns: The sample.
    @MainActor static func opening(_ page: SettingsAccountPageState) -> SettingsState {
      var state = selecting(.accounts)
      state.navigation.go(to: .account(page.id))
      state.account = page
      return state
    }

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
