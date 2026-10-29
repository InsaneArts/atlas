import CoreGraphics

/// The Screenshots pane: how many were taken today, the screenshots in their groups or why there are none, and the one
/// shown large.
public struct ViewerShotsState: Equatable, Sendable {
  /// What VoiceOver says of the pane, such as "6 today"; empty while none was taken.
  public var meta: String
  /// The screenshots in their groups, newest first.
  public var days: [ViewerShotDay]
  /// Why no screenshot shows, in place of them; nil while some show or before the first reading.
  public var empty: EmptyMessageState?
  /// The screenshot shown large over the whole window; nil shows none.
  public var large: ViewerLargeShot?

  /// Keyboard moves made; a change asks the grid to reveal its selected tile.
  public var scrollRequest: Int
  /// How many tiles the grid keeps to a row while the window is being resized, its tiles stretching within them; nil
  /// fits as many as the width holds.
  public var heldColumns: Int?

  /// Creates the Screenshots pane's state.
  ///
  /// - Parameters:
  ///   - meta: What VoiceOver says of the pane.
  ///   - days: The screenshots in their groups.
  ///   - empty: Why no screenshot shows.
  ///   - large: The screenshot shown large.
  ///   - scrollRequest: Keyboard moves made.
  ///   - heldColumns: How many tiles the grid keeps to a row while the window is resized; nil fits the width.
  public init(
    meta: String,
    days: [ViewerShotDay],
    empty: EmptyMessageState? = nil,
    large: ViewerLargeShot? = nil,
    scrollRequest: Int = 0,
    heldColumns: Int? = nil
  ) {
    self.meta = meta
    self.days = days
    self.empty = empty
    self.large = large
    self.scrollRequest = scrollRequest
    self.heldColumns = heldColumns
  }

  /// The shape every tile's place in the grid takes, so the rows stand even: the shape most screenshots listed share,
  /// to the hundredth, the newest first among equals, kept between 4:3 and 16:9; 16:10 while none says its shape. A
  /// screenshot of that shape fills its place, and any other stands whole inside it.
  public var tileShape: CGFloat {
    let shapes =
      days
      .flatMap(\.shots)
      .compactMap(\.aspect)
      .filter { $0.isFinite && $0 > 0 }
      .map(Self.grained)
    var counts: [CGFloat: Int] = [:]
    for shape in shapes { counts[shape, default: 0] += 1 }
    let most = shapes.first { counts[$0] == counts.values.max() } ?? ShotShape.standard
    return min(max(most, Self.tileShapes.lowerBound), Self.tileShapes.upperBound)
  }

  /// A shape to the hundredth (``shapeGrain``).
  ///
  /// - Parameter shape: A screenshot's width over its height.
  /// - Returns: The shape, rounded.
  static func grained(_ shape: CGFloat) -> CGFloat {
    let scaled = shape * shapeGrain
    return scaled.rounded() / shapeGrain
  }

  /// 4:3 to 16:9: the shapes a tile's place takes, so an odd screenshot never makes every row a sliver.
  public static let tileShapes: ClosedRange<CGFloat> = (4.0 / 3.0)...(16.0 / 9.0)
  /// 100: screenshots' shapes are compared to the hundredth, so a display's few pixels of rounding count as one shape.
  static let shapeGrain: CGFloat = 100
}
