import SwiftUI

/// The mask that fades a scrolling list's bottom edge. A mask reads only alpha: any opaque colour, here the ground,
/// shows the rows whole, and clear hides them.
struct ScrollFade: View {
  /// The component drawn in its current state.
  var body: some View {
    LinearGradient(
      stops: [
        Gradient.Stop(color: .ground, location: 1 - GroupedListMetrics.scrollMask),
        Gradient.Stop(color: .clear, location: 1),
      ],
      startPoint: .top,
      endPoint: .bottom
    )
  }
}
