import CoreGraphics

/// Where a selected picture's ring stands: outside the picture, never over it, a gap of the ground between the two,
/// and concentric with the picture's corner, so it reads as the picture lifted out rather than a second frame.
enum PictureRing {
  /// 3: the ground showing between the picture's edge and the ring.
  static let gap: CGFloat = 3
  /// 2: the ring's width.
  static let width = Stroke.thick

  /// 5: how far the ring reaches outside the picture: its gap and its width.
  static var reach: CGFloat { gap + width }

  /// The ring's outer corner around a picture with corners `radius`: the picture's and the ring's reach, so the
  /// ring's inner edge runs at the picture's corner and its gap, concentric with it.
  ///
  /// - Parameter radius: The picture's corner.
  /// - Returns: The ring's outer corner.
  static func outerRadius(around radius: CGFloat) -> CGFloat {
    radius + reach
  }

  /// The ring's inner corner around a picture with corners `radius`: the picture's and the gap.
  ///
  /// - Parameter radius: The picture's corner.
  /// - Returns: The ring's inner corner.
  static func innerRadius(around radius: CGFloat) -> CGFloat {
    outerRadius(around: radius) - width
  }
}
