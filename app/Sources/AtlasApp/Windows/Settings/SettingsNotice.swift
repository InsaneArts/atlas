import AtlasUI

/// What the last command said, in the pane that ran it.
struct SettingsNotice: Equatable {
  /// The pane it shows in.
  var tab: SettingsTab
  /// The line.
  var text: String
  /// Whether it says something went wrong or could not run.
  var isProblem = false
}
