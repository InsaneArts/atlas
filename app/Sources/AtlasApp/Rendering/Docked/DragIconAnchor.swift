#if DEBUG
  import SwiftUI

  /// Where the permission panel's icon sits in a docked render, so the drag ghost starts at it.
  struct DragIconAnchor: PreferenceKey {
    /// No icon until the panel lays it out.
    static let defaultValue: Anchor<CGRect>? = nil

    /// The icon laid out last.
    ///
    /// - Parameters:
    ///   - value: The anchor so far.
    ///   - nextValue: The next one.
    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
      value = nextValue() ?? value
    }
  }
#endif
