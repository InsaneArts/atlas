import AtlasService
import AtlasUI
import CoreGraphics

extension ViewerPresenter {
  /// What an app's card shows of its windows: where they sit on the display, and its front window cut from the live
  /// picture already in memory while that window is in view (``windowPicture(_:of:among:_:)``). None for an app with
  /// no window, or before the display is known.
  ///
  /// - Parameters:
  ///   - app: The app.
  ///   - apps: Every app open in the helper, whose windows may cover its own.
  ///   - snapshot: What the controller holds.
  /// - Returns: The picture, or nil.
  static func appPicture(_ app: HelperApp, among apps: [HelperApp], _ snapshot: ViewerSnapshot) -> ViewerAppPicture? {
    guard let front = app.windows.first, let display = snapshot.reading.frame?.display, display.isValid else {
      return nil
    }
    let map = ViewerScreenMap(
      aspect: display.width / display.height,
      windows: app.windows.map { share(of: $0, on: display) }
    )
    return ViewerAppPicture(map: map, window: windowPicture(front, of: app, among: apps, snapshot))
  }

  /// An app's front window cut from the live picture with a little of the screen around it (``pictureMargin``),
  /// sharing its pixels, with no capture and no call to the helper:
  /// only while the picture is live and of the frame the display was read with, the app is not hidden, the window
  /// stood in the same place in the last two readings of the apps, and at least nine tenths of it shows past the
  /// windows in front of it (``HelperWindowVisibility``). Never kept: cut again from each picture, and named by the frame and
  /// the window, so a new frame draws anew and the same frame compares equal.
  ///
  /// - Parameters:
  ///   - window: The app's front window.
  ///   - app: The app.
  ///   - apps: Every app open in the helper.
  ///   - snapshot: What the controller holds.
  /// - Returns: The window's picture, or nil.
  static func windowPicture(
    _ window: HelperWindow,
    of app: HelperApp,
    among apps: [HelperApp],
    _ snapshot: ViewerSnapshot
  ) -> DesktopPicture? {
    guard !app.hidden, !snapshot.session.paused, !snapshot.pictureStale,
      let picture = snapshot.reading.picture, let frame = snapshot.reading.frame, picture.id == frame.id,
      frame.display.isValid, snapshot.panes.apps.steadyWindows.contains(window.id)
    else { return nil }
    let shown = apps.filter { !$0.hidden }
      .flatMap(\.windows)
    let display = HelperDisplay(width: frame.display.width, height: frame.display.height)
    guard HelperWindowVisibility.isInView(window, among: shown, display: display) else { return nil }
    var around = window
    around.x -= pictureMargin
    around.y -= pictureMargin
    around.width += pictureMargin * 2
    around.height += pictureMargin * 2
    return picture.cropped(to: share(of: around, on: frame.display), id: "\(frame.id).\(window.id)")
  }

  /// The whole display, as shares of itself.
  static let wholeDisplay = CGRect(x: 0, y: 0, width: 1, height: 1)

  /// 32: the points of the helper's screen kept around a window's picture, so a light window reads apart from the card
  /// it sits on.
  static let pictureMargin: Double = 32

  /// A window's frame as shares of the display, from its top-leading corner, kept on the display.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - display: The display's size, in points.
  /// - Returns: The shares.
  static func share(of window: HelperWindow, on display: ViewerStatus.Size) -> CGRect {
    let share = CGRect(
      x: window.x / display.width,
      y: window.y / display.height,
      width: window.width / display.width,
      height: window.height / display.height
    )
    guard !wholeDisplay.contains(share) else { return share }
    let kept = share.intersection(wholeDisplay)
    return kept.isNull ? .zero : kept
  }
}
