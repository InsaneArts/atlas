import CoreGraphics

/// The design system names its own values: literals are at home here.
enum Spacing {
  /// 12: a row's inset.
  static let m: CGFloat = 12
}

extension Spacing {
  /// A design-system helper may write a literal into a call.
  static var doubled: CGFloat { max(m * 2, 24) }
}
