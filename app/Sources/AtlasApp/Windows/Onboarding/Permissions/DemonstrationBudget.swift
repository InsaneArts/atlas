import AtlasUI
import Foundation

/// How much of the drag ghost's demonstration is left for one permission: ``DragGhostTrack/loops`` loops of the time
/// it is seen (a pause while the person points at the icon or holds the mouse down does not count), ending only in a
/// loop's unseen rest, never mid-flight.
struct DemonstrationBudget {
  /// The time it was seen before it last hid.
  private var seen: TimeInterval = 0
  /// When it last appeared, while it shows.
  private var visibleSince: Date?
  /// Whether it is used up.
  private(set) var isSpent = false

  /// The seen time after which the demonstration ends at the next rest: every loop but the last whole, and the last
  /// one up to its landing.
  static var limit: TimeInterval {
    DragGhostTrack.cycle * Double(DragGhostTrack.loops - 1) + DragGhostTrack.land
  }

  /// The ghost appeared at `now`.
  ///
  /// - Parameter now: When.
  mutating func appear(at now: Date) {
    if visibleSince == nil { visibleSince = now }
  }

  /// The ghost hid at `now`.
  ///
  /// - Parameter now: When.
  mutating func disappear(at now: Date) {
    guard let since = visibleSince else { return }
    seen += now.timeIntervalSince(since)
    visibleSince = nil
  }

  /// Whether the ghost may still show at `now`, its loop having begun at `loopStart`; it is spent once its time is up
  /// and the loop has landed.
  ///
  /// - Parameters:
  ///   - now: When.
  ///   - loopStart: When its loop began.
  /// - Returns: Whether it may show.
  mutating func allows(at now: Date, loopStart: Date) -> Bool {
    guard !isSpent else { return false }
    let total = seen + (visibleSince.map { now.timeIntervalSince($0) } ?? 0)
    let moment = now.timeIntervalSince(loopStart)
      .truncatingRemainder(dividingBy: DragGhostTrack.cycle)
    if total >= Self.limit && moment >= DragGhostTrack.land { isSpent = true }
    return !isSpent
  }
}
