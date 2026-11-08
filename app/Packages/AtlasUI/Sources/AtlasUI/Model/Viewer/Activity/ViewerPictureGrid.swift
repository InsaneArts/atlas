/// A run's pictures opened in place: a grid of one framed size, newest first, then "Show N more" while it holds more,
/// and Show fewer.
public struct ViewerPictureGrid: Equatable, Sendable {
  /// The run whose pictures they are.
  public var run: String
  /// The pictures shown, newest first.
  public var tiles: [ViewerPictureTile]
  /// How many more it holds than it shows.
  public var more: Int

  /// Creates a grid.
  ///
  /// - Parameters:
  ///   - run: The run whose pictures they are.
  ///   - tiles: The pictures shown.
  ///   - more: How many more it holds; none by default.
  public init(run: String, tiles: [ViewerPictureTile], more: Int = 0) {
    self.run = run
    self.tiles = tiles
    self.more = more
  }

  /// "Show 112 more".
  public var moreTitle: String { "Show \(more) more" }
}
