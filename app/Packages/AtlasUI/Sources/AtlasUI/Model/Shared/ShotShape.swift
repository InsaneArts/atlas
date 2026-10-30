import CoreGraphics

/// The shape a screenshot's picture is drawn at wherever it shows, the Screenshots grid and an Activity step alike,
/// so the whole screenshot shows, never cut or zoomed: its thumbnail's own once read, else the shape the screenshots
/// list says, else 16:10.
public enum ShotShape {
  /// 16:10: a screenshot's shape when neither its thumbnail nor the list says it, as the helper's displays mostly are.
  public static let standard: CGFloat = 16.0 / 10.0

  /// The shape to draw a screenshot's picture at.
  ///
  /// - Parameters:
  ///   - thumbnail: Its thumbnail, once read.
  ///   - listed: Its width over its height as the list says it, if it does.
  /// - Returns: Its width over its height.
  public static func of(_ thumbnail: DesktopPicture?, listed: CGFloat?) -> CGFloat {
    [thumbnail?.aspectRatio, listed]
      .compactMap { $0 }
      .first { $0.isFinite && $0 > 0 } ?? standard
  }
}
