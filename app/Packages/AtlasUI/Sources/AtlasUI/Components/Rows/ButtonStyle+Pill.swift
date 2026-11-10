import SwiftUI

extension ButtonStyle where Self == PillRowStyle {
  /// A row the whole width of its column as a button, on a capsule.
  static var pill: PillRowStyle { PillRowStyle() }

  /// A row the whole width of its column as a button, on a capsule, raised while it is the one chosen.
  ///
  /// - Parameter isSelected: Whether the row is the one chosen.
  /// - Returns: The style.
  static func pill(isSelected: Bool) -> PillRowStyle {
    PillRowStyle(isSelected: isSelected)
  }
}
