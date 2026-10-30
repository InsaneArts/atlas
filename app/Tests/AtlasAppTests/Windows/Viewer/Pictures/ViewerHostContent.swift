import SwiftUI

@testable import AtlasUI

/// The main window's content at its size, its live slot painting each picture its ``PictureColour`` by the picture's
/// id, as ``ViewerHost`` hosts it.
struct ViewerHostContent: View {
  /// What the window shows.
  let state: ViewerState
  /// Whether a newly shown pane plays its arrival; off while the initial reference is laid out.
  var animatesArrivals = true
  /// Whether this host exercises the reduced-motion policy.
  var reduceMotion = false
  /// How selection placements are deferred; tests may hold them across deselection.
  var placement = CardScrollPlacement()
  /// Observes the layout under test without depending on accessibility services.
  var frameReport: LayoutFrameReport?

  /// The window's content, its pictures painted flat.
  var body: some View {
    ViewerView(state: state, send: { _ in }, input: { EmptyView() }) { live in
      (PictureColour(rawValue: live.picture.id) ?? .stage).color
        .layoutFrame("picture \(live.picture.id)")
    }
    .environment(\.animatesArrivals, animatesArrivals)
    .environment(\.motionReductionOverride, reduceMotion)
    .environment(\.cardScrollPlacement, placement)
    .environment(\.layoutFrameReport, frameReport)
    .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }
}
