import SwiftUI

/// The field row's own measures for its label and focus ring.
enum FieldRowLayout {
  /// 112: the labels' column, wide enough for "Account name" so the fields line up.
  static let labelWidth: CGFloat = 112
  /// 3: the focus ring's soft halo, half of it outside the fine edge.
  static let haloWidth: CGFloat = 3
}
