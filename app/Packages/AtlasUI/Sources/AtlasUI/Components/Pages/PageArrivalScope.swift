import SwiftUI

/// Holds one page's arrival for every piece under it (``SwiftUICore/EnvironmentValues/pageArrived``): not arrived as
/// the page appears, then arrived, which fades its pieces in one after another (``PageArrival``). A layout that
/// switches between two arrangements of the same pieces, as ``ScrollColumn`` does once a long problem makes it scroll,
/// keeps them shown, as the page has arrived already; so does a piece that appears on the page later, which draws in
/// place.
struct PageArrivalScope: ViewModifier {
  @State private var arrived = false

  /// The page, its arrival set for the pieces under it.
  ///
  /// - Parameter content: The page.
  /// - Returns: The page, arriving once.
  func body(content: Content) -> some View {
    content
      .environment(\.pageArrived, arrived)
      .onAppear { arrived = true }
  }
}
