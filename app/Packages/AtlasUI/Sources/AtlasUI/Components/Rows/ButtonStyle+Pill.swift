import SwiftUI

extension ButtonStyle where Self == PillRowStyle {
  /// A row the whole width of its column as a button, on a capsule.
  static var pill: PillRowStyle { PillRowStyle() }

  /// A row the whole width of its column as a button, on a capsule, raised while it is the one chosen.
  ///
  /// - Parameters:
  ///   - isSelected: Whether the row is the one chosen.
  ///   - hovers: Whether it lights under the pointer; it does by default.
  /// - Returns: The style.
  static func pill(isSelected: Bool, hovers: Bool = true) -> PillRowStyle {
    PillRowStyle(isSelected: isSelected, hovers: hovers)
  }
}
