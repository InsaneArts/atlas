import SwiftUI

/// A scroll view keeping its content clear of the small screen, by its pane's ``EnvironmentValues/miniClearance``.
struct MiniClearing: ViewModifier {
  @Environment(\.miniClearance) private var clearance

  /// The scroll view with its content's margin.
  func body(content: Content) -> some View {
    content
      .contentMargins(clearance.edges, clearance.length, for: .scrollContent)
  }
}
