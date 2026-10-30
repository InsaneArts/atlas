import SwiftUI

/// A lead's own measures.
enum ListRowLeadLayout {
  /// 1.2: the empty ring's line.
  static let ring: CGFloat = 1.2
  /// 0.5: how small a check's new mark starts as it pops in.
  static let markArrival: CGFloat = 0.5
  /// 0.6: how small a result starts as it lands where marks swap at once.
  static let landingScale: CGFloat = 0.6
}
