import AtlasUI

/// What the last command said, on the page that ran it.
struct SettingsNotice: Equatable {
  /// The route it shows on: a page, or an account's page.
  var route: SettingsRoute
  /// The line.
  var text: String
  /// How the line reads: quiet, waiting on something that runs, or a problem.
  var kind = SettingsNote.Kind.quiet
  /// Whether Check Again said it, so it shows under "Ready to Act" rather than under the page's last group.
  var fromCheck = false
}
