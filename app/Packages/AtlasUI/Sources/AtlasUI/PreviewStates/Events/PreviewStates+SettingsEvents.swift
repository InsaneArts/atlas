#if DEBUG
  extension PreviewStates {
    /// The Settings sample an event leads to, so the Interactive preview answers clicks as the app would.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor public static func settings(_ state: SettingsState, applying event: SettingsEvent) -> SettingsState {
      var next = state
      switch event {
      case .select(let page):
        next.route = .page(page)
        next.account = settingsAccount(for: next.route, in: next.accounts.rows)
      case .openAccount(let id):
        next.route = .account(id)
        next.account = settingsAccount(for: next.route, in: next.accounts.rows)
      case .askRecover:
        next.alert = settingsRecover.alert
      case .askLogOut:
        next.alert = settingsLogOut.alert
      case .askForget:
        next.alert = settingsForget.alert
      case .askInstall:
        next.alert = settingsInstall.alert
      case .askDeletePictures:
        next.alert = settingsDeletePictures.alert
      case .checkForUpdates:
        next.about = settingsAboutNewer.about
      case .connect(let id):
        if let index = next.agents.agents.firstIndex(where: { $0.id == id }) {
          next.agents.agents[index].connection = .connected
          next.agents.agents[index].canDisconnect = true
          next.agents.agents[index].hasConnection = true
        }
      case .disconnect(let id):
        if let index = next.agents.agents.firstIndex(where: { $0.id == id }) {
          next.agents.agents[index].connection = .connect(isEnabled: true)
          next.agents.agents[index].canDisconnect = false
          next.agents.agents[index].hasConnection = false
        }
      case .alertConfirm:
        if next.alert == settingsForget.alert { next.agents.projects.removeFirst() }
        if next.alert == settingsInstall.alert { next.about = settingsAbout.about }
        if next.alert == settingsRemoveAccount.alert, let id = next.accounts.rows?.first?.id {
          next.accounts.rows?.removeFirst()
          next.route = next.route.kept(accounts: Set(next.accounts.rows?.map(\.id) ?? []))
          if next.account?.id == id { next.account = nil }
        }
        if next.alert == settingsDeletePictures.alert {
          next.general.storedPictures = "No pictures"
          next.general.canDeletePictures = false
        }
        next.alert = nil
      case .alertCancel:
        next.alert = nil
      case .askSwitch:
        next.alert = settingsSwitch.alert
      case .closeLeftovers, .closeHelper:
        next.helper.closeLogin = nil
      case .makeHomePrivate:
        next.privacy = settingsFilesPrivate
      case .askOpenHome:
        next.privacy = settingsFilesOpen
      case .setSignInCopies(let on):
        next.privacy?.signInCopies = on
      case .askRemoveAccount:
        next.alert = settingsRemoveAccount.alert
      case .setAccountLevel(let id, let level):
        if let index = next.accounts.rows?.firstIndex(where: { $0.id == id }) {
          next.accounts.rows?[index].level = level
        }
        if next.account?.id == id {
          next.account?.level = level
          next.account?.levelLine = settingsLevelLines[level] ?? ""
        }
      case .dismissAccountNote:
        next.accounts.note = nil
      case .askSignIn(let site):
        next.signIn = SignInSheetState(site: site ?? "")
      case .editSignInSite(let site):
        next.signIn?.site = site
        next.signIn?.refusal = nil
      case .continueSignIn, .cancelSignIn:
        next.signIn = nil
      case .forgetAccount, .resetBrowser, .signOutAccount:
        break
      case .checkAgain, .gettingStarted, .showAppInFinder, .showAgentFile, .addApp:
        break
      }
      return next
    }

    /// The account page a route shows in the samples, built from that account's row: its letter, name, level and what
    /// it is; none for a page or an account the sample doesn't list.
    ///
    /// - Parameters:
    ///   - route: The route.
    ///   - rows: The accounts the sample lists.
    /// - Returns: The page, or nil.
    static func settingsAccount(for route: SettingsRoute, in rows: [SettingsAccountRow]?) -> SettingsAccountPageState? {
      guard let id = route.account, let row = rows?.first(where: { $0.id == id }) else { return nil }
      // A row's line is its level, then when it was added; the page names what it is in its level's place.
      let added =
        row.detail
        .split(separator: " · ", maxSplits: 1)
        .last
        .map(String.init) ?? row.detail
      let first = added.prefix(1)
        .lowercased()
      return SettingsAccountPageState(
        id: row.id,
        letter: row.letter,
        title: row.title,
        kindLine: row.isSite ? "Website · \(first)\(added.dropFirst())" : added,
        isSite: row.isSite,
        level: row.level,
        levelLine: settingsLevelLines[row.level] ?? "",
        isEnabled: true
      )
    }

    /// What each level lets agents do, as the samples' account pages say it.
    static let settingsLevelLines: [AccountLevelChoice: String] = [
      .lookOnly: "Agents only look there.",
      .ask: "Agents ask you before sending there.",
      .trusted: "Agents act there without asking.",
    ]
  }
#endif
