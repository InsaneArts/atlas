import AtlasUI
import SwiftUI

/// The main window's content at its size, its live slot painting each picture its ``PictureColour`` by the picture's
/// id, as ``ViewerHost`` hosts it.
struct ViewerHostContent: View {
  /// What the window shows.
  let state: ViewerState

  /// The window's content, its pictures painted flat.
  var body: some View {
    ViewerView(state: state, send: { _ in }, newest: { "" }, input: { EmptyView() }) { live in
      (PictureColour(rawValue: live.picture.id) ?? .stage).color
    }
    .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }
}
