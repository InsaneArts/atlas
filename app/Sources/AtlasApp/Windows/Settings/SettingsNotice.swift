import AtlasUI

/// What the last command said, in the pane that ran it.
struct SettingsNotice: Equatable {
  /// The pane it shows in.
  var tab: SettingsTab
  /// The line.
  var text: String
  /// How the line reads: quiet, waiting on something that runs, or a problem.
  var kind = SettingsNote.Kind.quiet
  /// Whether Check Again said it, so it shows under "Ready to act" rather than under the pane's last group.
  var fromCheck = false
}
