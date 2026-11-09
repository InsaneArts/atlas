import AtlasUI
import Foundation

/// Everything the Settings controller holds and reads, as one value, for the presenter.
struct SettingsSnapshot {
  /// What the window is doing.
  var session = SettingsSession()
  /// The agents as last reported.
  var agents = SettingsAgents()
  /// The icons of the agents whose apps are on this Mac, by agent id.
  var icons: [String: IconImage] = [:]
  /// Whether Notify me is on.
  var notify = false
  /// The helper's sound, as every window shows it.
  var sound = HelperSoundButton()
  /// The person's files and the helper, as last read.
  var files = SettingsFiles()
  /// The pictures of agent actions.
  var pictures = SettingsPictures()
  /// What the rest of the app says now.
  var facts = SettingsFacts()
  /// When the account whose note the person read was added, in milliseconds since 1970.
  var accountNoteRead: Double = 0
  /// Sign In to a Site…'s sheet, while it shows.
  var signIn: SignInSheetState?
  /// The account whose row is open; nil for none.
  var openAccount: String?
  /// Now, which tells the day an account was added.
  var now = Date()
  /// The calendar days are told in.
  var calendar = Calendar.current
}
