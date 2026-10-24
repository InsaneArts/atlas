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
  var frame: Frame?
  /// The latest picture, ready to draw; the panel's, until the first reading of the window's own.
  var picture: DesktopPicture?
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
}
