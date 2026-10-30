import SwiftUI

/// A screenshot's picture as one view between its tile and the large view: both carry the screenshot's file
/// name as their geometry's identity, so opening one zooms it from its own tile to the centre and closing
/// zooms it back. Under Reduce Motion it carries none, and the large view fades in and out in place.
struct ShotZoom: ViewModifier {
  /// The screenshot's file name, the same in its tile and in the large view.
  let id: String
  /// The namespace the picture moves in.
  let namespace: Namespace.ID
  @Environment(\.motionReduced) private var reduceMotion

  /// The view, carrying the screenshot's identity unless Reduce Motion is on.
  func body(content: Content) -> some View {
    if reduceMotion {
      content
    } else {
      content
        .matchedGeometryEffect(id: id, in: namespace)
    }
  }
}
