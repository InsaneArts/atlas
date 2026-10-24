import Foundation

/// Where a screenshot dragged out of the Screenshots pane comes from: the app's, which reads the picture as taken once
/// it is dropped. The app sets one for the window's whole life, so any two compare equal and a redraw never
/// invalidates what reads it.
public struct ShotDragSource: Equatable, Sendable {
  /// The item the system carries for a screenshot, by its file name.
  let item: @MainActor @Sendable (String) -> NSItemProvider

  /// Creates the source.
  ///
  /// - Parameter item: The item the system carries for a screenshot, by its file name.
  public init(item: @escaping @MainActor @Sendable (String) -> NSItemProvider) {
    self.item = item
  }

  /// Whether two sources are the same: always, as the window sets one for its life.
  ///
  /// - Parameters:
  ///   - lhs: One source.
  ///   - rhs: The other.
  /// - Returns: True.
  public static func == (lhs: ShotDragSource, rhs: ShotDragSource) -> Bool { true }
}
