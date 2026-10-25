import SwiftUI

/// The hairline above a row under an app's head, inset to where the window's words start; the group draws it
/// on every row but its first (``EnvironmentValues/drawsListSeparator``).
struct WindowRowSeparator: ViewModifier {
  @Environment(\.drawsListSeparator) private var drawsSeparator

  /// The row with its hairline, where it follows another.
  func body(content: Content) -> some View {
    content
      .overlay(alignment: .top) {
        if drawsSeparator {
          SeparatorLine()
            .padding(.leading, ViewerLayout.windowIndent)
        }
      }
  }
}
