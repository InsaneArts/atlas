/// A page of the Settings window, as its sidebar lists them, in order.
public enum SettingsPage: String, CaseIterable, Hashable, Sendable {
  /// The helper: its status, whether it is ready to act, and what to do when something goes wrong.
  case helper
  /// How Atlas starts, how the helper's screen shows and sounds, and when Atlas notifies the person.
  case general
  /// Pictures of agent actions: whether they are kept, for how long, the typed text beside them and their storage.
  case activity
  /// The accounts the helper is signed in to, as Atlas lists them, each opening its own page.
  case accounts
  /// Whether the person's home folder is private from the helper, and whether sign-ins may be copied in.
  case privacy
  /// The coding agents connected to Atlas and the projects taught.
  case integrations
  /// The app's version, updates and the setup window.
  case about

  /// The sidebar's group the page sits in; the sidebar leaves a gap between two groups.
  public var group: Int {
    switch self {
    case .helper: 0
    case .general, .activity: 1
    case .accounts, .privacy, .integrations: 2
    case .about: 3
    }
  }
}
