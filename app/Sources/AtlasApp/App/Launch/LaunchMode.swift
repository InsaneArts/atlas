/// What one launch of the app does: one task that exits, or the menu bar app itself.
enum LaunchMode: Equatable {
  /// Tell the person something in this session, with Switch Back to an account when one is named.
  case notice(title: String, message: String, switchBack: String?)
  /// Turn Open at Login on or off, or report it, as JSON.
  case login(String)
  /// The setup window on its own.
  case onboarding
  /// Draw every setup screen to PNGs in a folder (debug builds).
  case renderOnboarding(String)
  /// Draw every viewer state to PNGs in a folder (debug builds).
  case renderViewer(String)
  /// Draw every Settings state to PNGs in a folder (debug builds).
  case renderSettings(String)
  /// Draw the menu bar item in every state, light and dark, to PNGs in a folder.
  case renderMenuBar(String)
  /// The app that reads the viewer's URL on standard input: the menu bar, or with `render`, one drawing of the
  /// panel in light or dark.
  case app(render: String?, dark: Bool)
}
