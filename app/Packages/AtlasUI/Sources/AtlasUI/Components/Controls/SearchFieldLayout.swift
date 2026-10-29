import SwiftUI

/// The field's own measures, from the pane header's search.
enum SearchFieldLayout {
  /// 200: the field's width where there is room.
  static let width: CGFloat = 200
  /// 120: the least it narrows to, still holding a short query beside its magnifier.
  static let leastWidth: CGFloat = 120
  /// 3: the focus ring's soft halo, as a form's field draws it.
  static let haloWidth: CGFloat = 3
}
