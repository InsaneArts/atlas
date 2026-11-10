/// The pictures a folded run or an ended session shows under its row: its newest few, then a link to every one of
/// them, such as "120 pictures", while it holds more than it shows.
public struct ViewerPictureStrip: Equatable, Sendable {
  /// The run whose pictures the link opens in place, or the folded session the link opens.
  public var run: String
  /// The newest pictures, newest first: three on a wide card, fewer on a narrow one while the link shows.
  public var tiles: [ViewerPictureTile]
  /// How many pictures it holds in all, which the link names; no link while it shows them all.
  public var total: Int
  /// Whether it is a folded session's, whose link opens the session rather than one run's pictures.
  public var opensSession: Bool

  /// Creates a strip.
  ///
  /// - Parameters:
  ///   - run: The run whose pictures the link opens.
  ///   - tiles: The pictures shown, newest first.
  ///   - total: How many it holds in all.
  ///   - opensSession: Whether it is a folded session's; a run's by default.
  public init(run: String, tiles: [ViewerPictureTile], total: Int, opensSession: Bool = false) {
    self.run = run
    self.tiles = tiles
    self.total = total
    self.opensSession = opensSession
  }

  /// Whether it holds more than it shows, so the link shows.
  public var hasLink: Bool { total > tiles.count }

  /// The link's words: "120 pictures".
  public var link: String { "\(total) pictures" }
}
