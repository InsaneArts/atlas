import Foundation

/// Whether a press on Atlas's menu bar item opens the panel, while the panel is closed. The press that closes an open
/// panel can reach it twice: a click elsewhere, or Atlas leaving the front, closes it first, and then the item's own
/// action arrives with the panel already closed. A press that soon after a closing is that same press, so it opens
/// nothing, and the panel goes away, as a menu does when its item is pressed again.
enum PanelToggle {
  /// 0.3: how soon after a closing a press on the item still counts as the press that closed it, in seconds.
  static let sameClick: TimeInterval = 0.3

  /// Whether a press on the item opens the closed panel.
  ///
  /// - Parameters:
  ///   - closedAt: When the panel last closed, in seconds since the Mac started; nil before it first closes.
  ///   - now: The time now, on the same clock.
  /// - Returns: False within ``sameClick`` of a closing, else true.
  static func opens(closedAt: TimeInterval?, now: TimeInterval) -> Bool {
    guard let closedAt else { return true }
    return now - closedAt >= sameClick
  }
}
