import AtlasService
import AtlasStatus
import AtlasUI
import CoreGraphics
import Foundation

/// What the viewer's last reading showed: the helper's status and its latest picture. Pictures stay in memory only.
struct ViewerReading {
  /// The latest status the service reported.
  var status = ViewerStatus()
  /// Whether any status has been read yet.
  var hasStatus = false
  /// The latest picture's frame: its id, revision and display.
  var frame: (id: String, revision: String, display: ViewerStatus.Size)?
  /// The picture the window is laid out with, ready to draw: the first since a picture came, or came at another size,
  /// and each one while Apps shows; the panel's, until the first reading of the window's own. The live slots draw the
  /// newest in its place (``LivePictureFeed``).
  var picture: DesktopPicture?
  /// The last picture drawn, kept when the picture is lost, so the stage keeps it behind its words, blurred,
  /// while the window connects again. Only drawn: input is never mapped through it.
  var lastPicture: DesktopPicture?
  /// Whether the window has waited for a picture longer than ``Motion/Duration/appearDelay``, after which the
  /// waiting stage rings its mark; false again once a picture arrives.
  var waitedLong = false
  /// The latest picture's size in points, which input is mapped through.
  var pictureSize: CGSize?
  /// When the picture arrived, in seconds of uptime (never steps back).
  var frameAt: TimeInterval?
  /// Why the picture cannot be shown, when it cannot.
  var unavailable: String?
  /// The helper accepts input (its status said so with the last picture).
  var ready = false

  /// What the panel and viewer show of the status.
  var statusReading: StatusReading { StatusReading(status) }

  /// Whether the helper is logged out.
  var loggedOut: Bool { status.overview.mode == .loggedOut }

  /// Whether the picture is too old to aim input at, at `now`: control ends over one, and none is taken.
  ///
  /// - Parameter now: The time, in seconds of uptime.
  /// - Returns: Whether it is; true before the first picture.
  func pictureStale(at now: TimeInterval) -> Bool {
    guard let frameAt else { return true }
    return now - frameAt >= Freshness.stale
  }
}
