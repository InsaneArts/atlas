import AtlasUI

/// The sessions menu as the presenter decides it: whose work shows, every live session from the status, and the
/// sessions that ended from the last read of the list, the newest first.
struct ViewerSessionsMenu: Equatable {
  /// 20: the most ended sessions the menu lists.
  static let mostEnded = 20

  /// Whose work shows now.
  var chosen: ViewerSessionChoice
  /// Every live session, in the service's order.
  var live: [ViewerSessionsMenuItem]
  /// The sessions that ended, newest first, at most ``mostEnded``.
  var ended: [ViewerSessionsMenuItem]
}
