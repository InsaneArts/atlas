/// A command a context menu in the main window offers, as data the window can hold; the shell carries it out, as the
/// main menu's items do.
public enum ViewerCommand: Equatable, Sendable {
  /// Copy Picture: put the helper's screen on the clipboard.
  case copyPicture
  /// Save Picture…: save the helper's screen to a file.
  case savePicture
  /// Actual Size: draw the screen one view point a display point, or fit it again.
  case toggleActualSize
  /// Pause Picture or Resume Picture.
  case togglePause
  /// One of the rates pictures arrive at, by its place in the shell's list.
  ///
  /// - Parameter index: The rate's place.
  case chooseRate(index: Int)
  /// Check Again: read the helper's status now.
  case checkAgain
  /// Open Settings on Helper…, which says what the helper needs.
  case openSettings
  /// Recover…: ask before recovering the helper.
  case askRecover
  /// Switch to the Helper…: ask before switching this Mac's screen to the helper.
  case askSwitch
  /// Log Out the Helper…: ask before logging the helper out.
  case askLogOut
  /// Clean Up This Session's Work…: read what cleaning up a session's work takes, then ask before it runs.
  ///
  /// - Parameter session: The session's key.
  case askCleanup(session: String)
  /// One of Send Key's keys, by its place in the shell's list: sent to the helper while this window holds control.
  ///
  /// - Parameter index: The key's place.
  case sendKey(index: Int)
  /// Show Activity: a session's work in Activity.
  ///
  /// - Parameter session: The session's key; empty for Terminal.
  case showActivity(session: String)
  /// Show Screenshots: a session's screenshots.
  ///
  /// - Parameter session: The session's key; empty for Terminal.
  case showScreenshots(session: String)
}
