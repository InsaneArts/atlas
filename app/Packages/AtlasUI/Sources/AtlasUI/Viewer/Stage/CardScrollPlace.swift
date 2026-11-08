import AppKit

/// Where the rows of a ``CardScroll`` stand, so new rows arriving above the one the person reads leave it exactly where
/// it was, whatever the Mac's own list does: macOS 26 moves a scrolled list on by twice a new session's height. Read and
/// written as the list lays out, never drawn.
@MainActor
final class CardScrollPlace {
  /// The list's scroll view, which a correction moves.
  weak var scroll: NSScrollView?
  /// Each built row's top in the list's visible bounds, by identity.
  private var tops: [String: CGFloat] = [:]
  /// The row read as rows began to arrive, and where its top stood.
  private var held: (row: String, top: CGFloat)?

  /// Notes where a row stands now, and brings the row held as rows arrived back to where it stood.
  ///
  /// - Parameters:
  ///   - row: The row's identity.
  ///   - top: Its top in the list's visible bounds.
  func report(_ row: String, top: CGFloat) {
    tops[row] = top
    guard let held, held.row == row, top != held.top, let scroll else { return }
    self.held = nil
    let clip = scroll.contentView
    clip.scroll(to: NSPoint(x: clip.bounds.origin.x, y: clip.bounds.origin.y + top - held.top))
    scroll.reflectScrolledClipView(clip)
  }

  /// Forgets a row the list let go.
  ///
  /// - Parameter row: The row's identity.
  func forget(_ row: String) {
    tops[row] = nil
  }

  /// Holds the row the person reads where it stands, until the rows arriving above it have been laid out: the first
  /// standing at or below the list's top, else the one crossing it, such as a long open run.
  func hold() {
    let below = tops.filter { $0.value >= .zero }
      .min { $0.value < $1.value }
    let crossing = tops.max { $0.value < $1.value }
    held = (below ?? crossing)
      .map { (row: $0.key, top: $0.value) }
    // Laid out by the next turn, the arrival's rows have reported: a later scroll by the person is theirs.
    Task { [weak self] in self?.held = nil }
  }
}
