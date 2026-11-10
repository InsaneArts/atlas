/// What the person does in the Settings window, other than the General and Activity pages' values, which their binding
/// carries.
public enum SettingsEvent: Equatable, Sendable {
  /// Shows a page from the sidebar.
  case select(SettingsPage)
  /// Shows the page of the account with this id.
  case openAccount(String)
  /// Returns to the route left last.
  case goBack
  /// Goes again to the route Back left.
  case goForward
  /// Filters the sidebar by what the person typed.
  case search(String)
  /// Reads the helper's readiness again now.
  case checkAgain
  /// Asks to switch this Mac's screen to the helper.
  case askSwitch
  /// Asks to recover the helper.
  case askRecover
  /// Asks to log the helper out.
  case askLogOut
  /// Closes the logins deleted accounts left open; macOS asks to approve.
  case closeLeftovers
  /// Logs out the helper whose Atlas is silent, as root; macOS asks to approve.
  case closeHelper
  /// Makes the home folder private from the helper.
  case makeHomePrivate
  /// Asks to let the helper open the home folder again.
  case askOpenHome
  /// Lets agents copy sign-ins into the helper, asking first, or stops them at once.
  case setSignInCopies(Bool)
  /// Asks to add an app to the accounts, choosing it in a panel.
  case addApp
  /// Asks to take the account with this id off the list.
  case askRemoveAccount(String)
  /// Sets how freely agents act in the account with this id.
  ///
  /// - Parameters:
  ///   - id: The account's id.
  ///   - level: The level.
  case setAccountLevel(String, AccountLevelChoice)
  /// The person read the note that Atlas added an account.
  case dismissAccountNote
  /// Opens Sign In to a Site…, with a site typed in already for an account's row.
  ///
  /// - Parameter site: The site to sign in to; nil for none.
  case askSignIn(String?)
  /// The site in Sign In to a Site…'s field, as the person typed it.
  case editSignInSite(String)
  /// Continue: start signing the helper in to the site.
  case continueSignIn
  /// Cancel: close Sign In to a Site….
  case cancelSignIn
  /// Opens the account with this id to show what else can be done with it, or closes it.
  case toggleAccount(String)
  /// Sign Out… on the account with this id: its site's own page, with agents paused, to sign out there.
  case signOutAccount(String)
  /// Forget This Site… on the account with this id: delete its data in the helper's browser, with agents paused.
  case forgetAccount(String)
  /// Reset the Helper's Browser… from the account with this id: clear that browser's data, with agents paused.
  case resetBrowser(String)
  /// Connects the agent with this id.
  case connect(String)
  /// Disconnects every part of the agent with this id.
  case disconnect(String)
  /// Asks to forget the project in this folder.
  case askForget(String)
  /// Asks to delete every picture of agent actions on this Mac.
  case askDeletePictures
  /// Checks whether a newer Atlas is published.
  case checkForUpdates
  /// Asks to install the newer Atlas found.
  case askInstall
  /// Opens the setup window.
  case gettingStarted
  /// Shows Atlas in the Finder, so the person can move it to the Applications folder.
  case showAppInFinder
  /// Shows the settings file of the agent with this id in the Finder, so the person can mend it.
  case showAgentFile(String)
  /// The alert's button that goes ahead.
  case alertConfirm
  /// The alert's way out.
  case alertCancel
}
