import SwiftUI

extension EnvironmentValues {
  /// Where a field reports its frame while it holds the keyboard, so its window can take the keyboard back on a click
  /// outside it. Nil, as in previews and renders, reports nothing.
  @Entry public var focusedFieldReport: FocusedFieldReport? = nil
}
