import AtlasUI

/// What the last command said, in the pane that ran it.
struct SettingsNotice: Equatable {
  /// The pane it shows in.
  var tab: SettingsTab
  /// The line.
  var text: String
  /// How the line reads: quiet, waiting on something that runs, or a problem.
  var kind = SettingsNote.Kind.quiet
}
