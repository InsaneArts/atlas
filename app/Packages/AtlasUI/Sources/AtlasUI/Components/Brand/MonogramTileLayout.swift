import SwiftUI

/// The tile's measures beside the icon grid's.
enum MonogramTileLayout {
  /// 0.5: the letter's size against the body's side.
  static let letterShare: CGFloat = 0.5
  /// 0.6: how far down the body the light from its top reaches.
  static let liftEnd: CGFloat = 0.6
  /// 0.5: the most a wide letter is shrunk to stay inside the body.
  static let letterShrink: CGFloat = 0.5
}
