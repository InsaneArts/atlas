import SwiftUI

/// A focus-scoped marker inherited only while a screenshot or one of its controls owns keyboard focus.
struct ScreenshotFocusKey: FocusedValueKey {
  /// Whether screenshot shortcuts belong to the focused subtree.
  typealias Value = Bool
}
