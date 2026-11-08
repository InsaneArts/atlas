import SwiftUI

/// The window's content with a count of its own drawing, as the window's root view draws it.
struct ViewerRedrawRoot<Content: View>: View {
  /// Counts each time the root draws.
  let counted: () -> Void
  /// The window's content.
  @ViewBuilder let content: () -> Content

  /// The content, counted.
  var body: some View {
    counted()
    return content()
  }
}
