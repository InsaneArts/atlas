import CoreGraphics

/// The grid macOS draws its apps' icons on, which every tile Atlas draws in an icon's place follows, so it sits as one
/// beside the real icons: a body of 824 of the square's 1024 points, centred, with a continuous corner.
enum IconGrid {
  /// 824 / 1024: the body against the whole square, as an app's icon draws it.
  static let bodyShare: CGFloat = 824.0 / 1024.0
  /// 0.225: the corner against the body's side, as an app's icon is rounded.
  static let cornerShare: CGFloat = 0.225
}
