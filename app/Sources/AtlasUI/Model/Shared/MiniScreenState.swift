/// The helper's screen, small in a window's corner: its latest picture, and whether pictures have stopped arriving.
package struct MiniScreenState: Equatable, Sendable {
  /// The latest picture.
  package var picture: DesktopPicture
  /// Whether the picture is paused, which draws it faded with "Paused" in place of "Live".
  package var isPaused: Bool

  /// Creates the small screen's state.
  ///
  /// - Parameters:
  ///   - picture: The latest picture.
  ///   - isPaused: Whether the picture is paused.
  package init(picture: DesktopPicture, isPaused: Bool = false) {
    self.picture = picture
    self.isPaused = isPaused
  }
}
