/// What fills the panel under its header.
package enum PanelStage: Equatable, Sendable {
  /// The helper's desktop, live.
  case picture(DesktopPicture)
  /// No desktop to show, and why.
  case placeholder(String)
  /// The helper is logged out: the way to log in takes the desktop's place.
  ///
  /// - Parameters:
  ///   - text: What logging in involves.
  ///   - isEnabled: Whether Log In… can be pressed now.
  case loggedOut(text: String, isEnabled: Bool)
}
