/// What an app's card shows of its windows: its front window, cut from the helper's screen while that window is in
/// view, else where its windows sit on the display.
public struct ViewerAppPicture: Equatable, Sendable {
  /// Where the app's windows sit on the display, shown while there is no window's picture.
  public var map: ViewerScreenMap
  /// The front window as the helper's screen shows it now; nil while it is covered, moving, or the picture is paused
  /// or old. Never kept: it is cut again from each picture.
  public var window: DesktopPicture?

  /// Creates a card's picture.
  ///
  /// - Parameters:
  ///   - map: Where the app's windows sit.
  ///   - window: The front window's picture, if it shows.
  public init(map: ViewerScreenMap, window: DesktopPicture? = nil) {
    self.map = map
    self.window = window
  }
}
