import AtlasCommands
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
  /// Whether apps in the helper can be heard, as `atlas sound` last said; nil until it answers.
  var helperSound: HelperSound?
  /// Why the last change of the helper's sound did not reach it; nil once one did.
  var soundFailure: String?
  /// What the rest of the app says now.
  var facts = SettingsFacts()
}
