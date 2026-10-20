import SwiftUI

/// The root a window's hosting view draws. It builds the window's view each time SwiftUI asks, so Observation
/// tracks what the controller's state reads, and the view redraws when that changes.
struct ShellRoot<Content: View>: View {
  /// Builds the window's view from its controller's state.
  private let content: () -> Content

  /// Creates the root.
  ///
  /// - Parameter content: Builds the window's view, reading the controller's state and passing its `send`.
  init(@ViewBuilder content: @escaping () -> Content) {
    self.content = content
  }

  /// The window's view, built afresh.
  var body: some View {
    content()
  }
}
