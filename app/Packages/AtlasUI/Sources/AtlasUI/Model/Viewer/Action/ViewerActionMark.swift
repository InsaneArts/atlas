import CoreGraphics

/// The mark the app draws over an action's picture, never into the file: its shape, where it went and the element it
/// went to, how it went, and for one that did not work, why, on a tag.
public struct ViewerActionMark: Equatable, Sendable {
  /// What it draws.
  public var shape: ViewerMarkShape
  /// Where the action went, as a share of the picture's width and height from its top-left; nil when no spot is known,
  /// which draws keys at the picture's centre and a press as a tag.
  public var spot: CGPoint?
  /// The element it went to, as shares of the picture's width and height from its top-left, which a ring outlines; nil
  /// when unknown, which draws a ring of a fixed size on the spot.
  public var area: CGRect?
  /// How it went.
  public var tone: ViewerMarkTone
  /// Why it did not work, on a tag beside the mark; nil for none.
  public var note: String?

  /// Creates a mark.
  ///
  /// - Parameters:
  ///   - shape: What it draws.
  ///   - spot: Where the action went; none by default.
  ///   - area: The element it went to; none by default.
  ///   - tone: How it went; worked by default.
  ///   - note: Why it did not work; none by default.
  public init(
    shape: ViewerMarkShape,
    spot: CGPoint? = nil,
    area: CGRect? = nil,
    tone: ViewerMarkTone = .worked,
    note: String? = nil
  ) {
    self.shape = shape
    self.spot = spot
    self.area = area
    self.tone = tone
    self.note = note
  }

  /// Where a small picture centres its crop: the element's middle, the spot, or a drag's middle; nil shows the whole
  /// picture.
  public var focus: CGPoint? {
    if let area { return CGPoint(x: area.midX, y: area.midY) }
    guard let spot else { return nil }
    if case .drag(let to) = shape { return CGPoint(x: (spot.x + to.x) / 2, y: (spot.y + to.y) / 2) }
    return spot
  }
}
