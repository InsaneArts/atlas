/// The Settings window: where it stands and where it has been, its sidebar of pages, each page's state, and the alert
/// or sheet over it.
public struct SettingsState: Equatable, Sendable {
  /// The route shown, with Back's and Forward's routes.
  public var navigation: SettingsNavigation
  /// The sidebar: its rows and the search over them.
  public var sidebar: SettingsSidebarState
  /// The values of the General and Activity pages, which one binding edits.
  public var general: SettingsGeneralState
  /// The Helper page.
  public var helper: SettingsHelperState
  /// The Accounts page.
  public var accounts: SettingsAccountsState
  /// The page of the account shown; nil while no account's page shows.
  public var account: SettingsAccountPageState?
  /// The Privacy page; nil until Atlas has looked at the person's files.
  public var privacy: SettingsFilesState?
  /// The Integrations page.
  public var agents: SettingsAgentsState
  /// The About page.
  public var about: SettingsAboutState
  /// The alert over the whole window, if one is asked.
  public var alert: AlertCardState<AlertChoice>?
  /// Sign In to a Site…'s sheet over the whole window, while it shows.
  public var signIn: SignInSheetState?

  /// Creates the window's state.
  ///
  /// - Parameters:
  ///   - navigation: The route shown, with Back's and Forward's.
  ///   - sidebar: The sidebar.
  ///   - general: The values of the General and Activity pages.
  ///   - helper: The Helper page.
  ///   - accounts: The Accounts page.
  ///   - account: The page of the account shown; none by default.
  ///   - privacy: The Privacy page; nil until Atlas has looked.
  ///   - agents: The Integrations page.
  ///   - about: The About page.
  ///   - alert: The alert over the whole window, if one is asked.
  ///   - signIn: Sign In to a Site…'s sheet, while it shows; none by default.
  public init(
    navigation: SettingsNavigation,
    sidebar: SettingsSidebarState,
    general: SettingsGeneralState,
    helper: SettingsHelperState,
    accounts: SettingsAccountsState,
    account: SettingsAccountPageState? = nil,
    privacy: SettingsFilesState?,
    agents: SettingsAgentsState,
    about: SettingsAboutState,
    alert: AlertCardState<AlertChoice>? = nil,
    signIn: SignInSheetState? = nil
  ) {
    self.navigation = navigation
    self.sidebar = sidebar
    self.general = general
    self.helper = helper
    self.accounts = accounts
    self.account = account
    self.privacy = privacy
    self.agents = agents
    self.about = about
    self.alert = alert
    self.signIn = signIn
  }

  /// The route shown.
  public var route: SettingsRoute { navigation.current }

  /// The title of the route shown, which the bar and the window's title show: its page's, or its account's name.
  public var title: String {
    switch route {
    case .page(let page): sidebar.title(of: page)
    case .account(let id):
      account.map(\.title) ?? accounts.rows?.first { $0.id == id }?.title ?? sidebar.title(of: .accounts)
    }
  }
}
