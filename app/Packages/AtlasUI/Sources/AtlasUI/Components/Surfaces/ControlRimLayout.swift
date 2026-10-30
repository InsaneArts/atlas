import SwiftUI

/// The rim's own measures for its accent edge and glow.
enum ControlRimLayout {
  /// 0.9: the accent on the rim while in control.
  static let rim = 0.9
  /// 0.3: the accent glowing around the picture while in control.
  static let glow = 0.3
  /// 7: the glow's radius, half an equivalent 14-point CSS blur, as a shadow's radius spreads about twice as
  /// far.
  static let glowRadius: CGFloat = 7
}
