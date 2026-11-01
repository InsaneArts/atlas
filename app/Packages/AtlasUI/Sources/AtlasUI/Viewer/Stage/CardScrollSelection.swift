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
  /// The requested row and its correction, retained until geometry confirms the reveal or the step is cancelled.
  private var waiting: (row: String, place: @MainActor (CGRect) -> Void)?
  /// A placement deferred until after the layout pass that reported the row.
  private var queued: (row: String, task: Task<Void, Never>)?

  /// Notes where the selected row stands now and corrects a reveal that has not yet reached its destination.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - frame: Its frame in the list's visible bounds.
  func report(_ id: String, at frame: CGRect) {
    self.id = id
    self.frame = frame
    guard let waiting, waiting.row == id else { return }
    waiting.place(frame)
  }

  /// Forgets a deselected row's geometry and cancels a step waiting for it.
  ///
  /// - Parameter id: The deselected row's identity.
  func forget(_ id: String) {
    if self.id == id {
      self.id = nil
      frame = nil
    }
    if waiting?.row == id {
      waiting = nil
    }
    if queued?.row == id {
      queued?.task.cancel()
      queued = nil
    }
  }

  /// Cancels both a step waiting for geometry and a placement queued after layout.
  func cancelPlacement() {
    waiting = nil
    queued?.task.cancel()
    queued = nil
  }

  /// Defers a placement out of the layout pass, keeping it cancellable until it runs.
  ///
  /// - Parameters:
  ///   - row: The selected row's identity.
  ///   - placement: Schedules the cancellable work after layout.
  ///   - place: What scrolls it into view after layout.
  func queue(
    _ row: String,
    using placement: CardScrollPlacement = CardScrollPlacement(),
    with place: @escaping @MainActor () -> Void
  ) {
    queued?.task.cancel()
    queued = (row, placement.schedule(place))
  }

  /// Places `row` where it stands now and on subsequent geometry reports until `cancelPlacement()` ends the reveal.
  ///
  /// - Parameters:
  ///   - row: The row's identity.
  ///   - place: What places it, given its frame in the list's visible bounds.
  /// - Returns: Whether it stood already, so it was placed at once.
  @discardableResult
  func place(_ row: String, with place: @escaping @MainActor (CGRect) -> Void) -> Bool {
    cancelPlacement()
    waiting = (row, place)
    if id == row, let frame {
      place(frame)
      return true
    }
    return false
  }
}
