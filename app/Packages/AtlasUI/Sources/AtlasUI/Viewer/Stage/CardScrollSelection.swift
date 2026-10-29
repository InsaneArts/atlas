import CoreGraphics

/// Where a ``CardScroll``'s selected row stands as the list scrolls, how tall the list shows, and the step waiting for
/// a row to stand: read and written as the list scrolls and lays out, never drawn, so a scroll writes it without the
/// list being drawn again.
@MainActor
final class CardScrollSelection {
  /// The selected row's identity, as it last reported.
  private(set) var id: String?
  /// Its frame in the list's visible bounds, top left first.
  private(set) var frame: CGRect?
  /// The list's visible height.
  var viewport: CGFloat = .zero
  /// The row a step waits to place, and how to place it once it stands; nil while no step waits.
  private var waiting: (row: String, place: @MainActor (CGRect) -> Void)?

  /// Notes where the selected row stands now, and places it if a step was waiting for it to stand.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - frame: Its frame in the list's visible bounds.
  func report(_ id: String, at frame: CGRect) {
    self.id = id
    self.frame = frame
    guard let waiting, waiting.row == id else { return }
    self.waiting = nil
    waiting.place(frame)
  }

  /// Places `row` with `place` where it stands now, or as soon as it reports where it stands.
  ///
  /// - Parameters:
  ///   - row: The row's identity.
  ///   - place: What places it, given its frame in the list's visible bounds.
  /// - Returns: Whether it stood already, so it was placed at once.
  @discardableResult
  func place(_ row: String, with place: @escaping @MainActor (CGRect) -> Void) -> Bool {
    if id == row, let frame {
      waiting = nil
      place(frame)
      return true
    }
    waiting = (row, place)
    return false
  }
}
