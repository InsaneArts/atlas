import SwiftUI

extension View {
  /// Asks for a screenshot's thumbnail as the view appears and as its pane shows again (``ThumbnailRequest``).
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - send: Receives the request.
  /// - Returns: The view, asking for its thumbnail.
  func asksForThumbnail(_ name: String, send: @escaping @MainActor (ViewerEvent) -> Void) -> some View {
    modifier(ThumbnailRequest(name: name, send: send))
  }
}
