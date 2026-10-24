/// What fills the panel's picture frame.
public enum PanelPictureState: Equatable, Sendable {
  /// The helper's screen, drawn with its look, and the newest action as a caption while an agent works. Clicking it
  /// opens the main window on Screen.
  ///
  /// - Parameters:
  ///   - picture: The screen; nil shows the empty stage until the first picture arrives.
  ///   - look: How it is drawn: live, dimmed, paused or inside the blue rim of control.
  ///   - caption: The newest action while an agent works; nil shows none.
  case screen(picture: DesktopPicture?, look: PictureLook, caption: ActivityEntry?)
  /// No screen to show: Atlas's orbit on the empty stage, over one line saying why or what helps.
  ///
  /// - Parameter line: The line, such as "The helper is logged out".
  case empty(String)
}
