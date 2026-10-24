import SwiftUI

/// A setup screen's column: the window's one width, its first piece at the window's one height, so moving from
/// screen to screen never shifts the column's edges or its top.
struct ScreenColumn<Content: View>: View {
  /// The screen's pieces, top to bottom.
  private let content: Content

  /// Creates a screen's column.
  ///
  /// - Parameter content: The screen's pieces, top to bottom.
  init(@ViewBuilder content: () -> Content) {
    self.content = content()
  }

  /// The pieces in the window's column.
  var body: some View {
    PageColumn(width: OnboardingLayout.column, top: OnboardingLayout.columnTop) {
      content
    }
  }
}
