import SwiftUI

/// Asks for a screenshot's thumbnail as its view appears, and again as a kept pane shows again: a kept pane's views do
/// not appear anew, and a thumbnail let go meanwhile is asked for again.
struct ThumbnailRequest: ViewModifier {
  /// The screenshot's file name.
  let name: String
  /// Receives the request.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.paneShows) private var paneShows

  /// The view, asking for its thumbnail.
  ///
  /// - Parameter content: The view that shows the thumbnail.
  /// - Returns: The view.
  func body(content: Content) -> some View {
    content
      .onAppear { send(.thumbnailNeeded(name)) }
      .onChange(of: paneShows) { _, shows in if shows { send(.thumbnailNeeded(name)) } }
  }
}
