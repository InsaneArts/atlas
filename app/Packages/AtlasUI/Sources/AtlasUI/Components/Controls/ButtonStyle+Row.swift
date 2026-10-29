import SwiftUI

extension ButtonStyle where Self == RowButtonStyle {
  /// A whole row as a button, on small corners.
  static var row: RowButtonStyle { RowButtonStyle() }

  /// A whole row as a button, its fill's corners nested in the surface it sits in.
  ///
  /// - Parameter corner: The fill's corner, such as the panel's sessions'.
  /// - Returns: The style.
  static func row(corner: CGFloat) -> RowButtonStyle {
    RowButtonStyle(corner: corner)
  }
}
