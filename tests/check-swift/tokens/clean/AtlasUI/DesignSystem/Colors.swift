import SwiftUI

/// Colours, where black and white at an opacity are at home.
extension Color {
  /// A surface's shadow: black at 0.5.
  static let dropShadow = Color.black
    .opacity(0.5)
}

/// Icon sizes by size, with a role-named list of cases.
enum IconSize {
  /// Sizes by name: an icon size names the scale, not a surface.
  case xs, s, m(Int)
  /// The scale's own name, allowed.
  static let iconSize: CGFloat = 16
}
