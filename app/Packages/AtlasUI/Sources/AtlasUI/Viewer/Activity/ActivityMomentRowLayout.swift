import SwiftUI

/// The header's own measures.
enum ActivityMomentRowLayout {
  /// 90°: the chevron's turn while the moment is open.
  static let turned = Angle.degrees(90)
  /// 16: the chevron's column.
  static let chevron: CGFloat = 16
  /// 7: the line under the icon, from the icon's lower edge to the header's.
  static let lineUnderIcon = (ViewerLayout.momentHeight - ViewerLayout.appIcon) / 2
}
