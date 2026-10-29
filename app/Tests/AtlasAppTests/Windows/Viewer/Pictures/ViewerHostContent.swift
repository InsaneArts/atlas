import AtlasUI
import SwiftUI

/// The main window's content at its size, its live slot painting each picture its ``PictureColour`` by the picture's
/// id, as ``ViewerHost`` hosts it.
struct ViewerHostContent: View {
  /// What the window shows.
  let state: ViewerState
  /// Whether a newly shown pane plays its arrival; off while the initial reference is laid out.
  var animatesArrivals = true

  /// The window's content, its pictures painted flat.
  var body: some View {
    ViewerView(state: state, send: { _ in }, input: { EmptyView() }) { live in
      (PictureColour(rawValue: live.picture.id) ?? .stage).color
    }
    .environment(\.animatesArrivals, animatesArrivals)
    .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }
}
