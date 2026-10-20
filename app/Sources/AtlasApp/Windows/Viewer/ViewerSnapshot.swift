import AtlasUI
import Foundation

/// What the viewer's controller holds, as one plain value for ``ViewerPresenter``.
struct ViewerSnapshot {
  /// What the last reading showed.
  var reading = ViewerReading()
  /// What the window is doing.
  var session = ViewerSession()
  /// Whether the person asked for the sidebar.
  var sidebarShown = false
  /// How often a picture is read while watching.
  var rate = ViewerPictureRate.everyTwoSeconds
  /// The time the header last read, in seconds of uptime.
  var now: TimeInterval = 0
  /// Whether the app can lock the screen for the helper, so Log In… shows.
  var canLogIn = false
}
