import SwiftUI

/// The ring's own measures.
enum OrbitRingLayout {
  /// 14: how far outside the mark's box the oval lies.
  static let reach = Spacing.mPlus
  /// A quarter of the oval shines.
  static let arc = 0.25
  /// 0.625: where the quarter starts, so at rest it sits centred over the mark's top.
  static let start = 0.625
}
