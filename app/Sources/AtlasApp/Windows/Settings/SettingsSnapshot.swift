import AtlasUI

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
  /// Whether the Dock badge is on.
  var badge = false
  /// What the rest of the app says now.
  var facts = SettingsFacts()
}
