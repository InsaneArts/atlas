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
        next.navigation.go(to: .page(page))
      case .openAccount(let id):
        next.navigation.go(to: .account(id))
        next.account =
          [settingsAccountSitePage, settingsAccountAppPage].first { $0.id == id } ?? settingsAccountSitePage
      case .goBack:
        next.navigation.goBack()
      case .goForward:
        next.navigation.goForward()
      case .search(let query):
        next.sidebar.query = query
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
        }
      case .disconnect(let id):
        if let index = next.agents.agents.firstIndex(where: { $0.id == id }) {
          next.agents.agents[index].connection = .connect(isEnabled: true)
          next.agents.agents[index].canDisconnect = false
        }
      case .alertConfirm:
        if next.alert == settingsForget.alert { next.agents.projects.removeFirst() }
        if next.alert == settingsInstall.alert { next.about = settingsAbout.about }
        if next.alert == settingsRemoveAccount.alert, let id = next.accounts.rows?.first?.id {
          next.accounts.rows?.removeFirst()
          next.navigation.keep(accounts: Set(next.accounts.rows?.map(\.id) ?? []))
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
        if next.account?.id == id { next.account?.level = level }
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
  }
#endif
