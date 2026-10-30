import Observation

/// Hands one retained list's place to the next only after its departure finishes.
@Observable
final class PaneHandoff {
  /// The list permitted to arrive or remain visible; nil while a departure is pending.
  private(set) var shown: ViewerPlace?
  /// The list whose fade must finish before another can arrive.
  private(set) var departing: ViewerPlace?
  /// The latest destination; replacing it cancels any earlier pending arrival.
  private var destination: ViewerPlace?

  /// Starts at the window's initial destination, without scheduling an extra navigation change after mounting.
  ///
  /// - Parameter place: The place the window first shows.
  init(_ place: ViewerPlace? = nil) {
    shown = place.flatMap { $0.isKept ? $0 : nil }
    destination = shown
  }

  /// Requests a destination, retaining an in-flight departure when navigation changes again.
  ///
  /// - Parameter place: The latest navigation choice.
  func request(_ place: ViewerPlace) {
    destination = place.isKept ? place : nil
    if let departing, departing == destination {
      // Re-admitting the same retained view reverses its existing opacity animation in place.
      shown = departing
      self.departing = nil
      return
    }
    guard shown != destination || departing != nil else { return }
    departing = shown ?? departing
    shown = departing == nil ? destination : nil
  }

  /// Admits only the latest destination once the outgoing list is fully transparent.
  ///
  /// - Parameter place: The list whose departure completed; stale completions have no effect.
  func departed(_ place: ViewerPlace) {
    guard departing == place else { return }
    departing = nil
    shown = destination
  }
}
