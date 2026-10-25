import SwiftUI

extension View {
  /// Pops the view in as it appears (``PopIn``), such as a green check beside "Atlas is up to date.".
  ///
  /// - Returns: The view, popping in.
  func popsIn() -> some View {
    modifier(PopIn())
  }
}
