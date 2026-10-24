import SwiftUI

/// A divider one device pixel thick, so it never drops out at any scale.
struct SeparatorLine: View {
  /// Which way the line runs.
  var axis = Axis.horizontal
  @Environment(\.displayScale) private var scale

  /// The line along its axis.
  var body: some View {
    switch axis {
    case .horizontal:
      Rectangle()
        .fill(Color.separator)
        .frame(height: thickness)
    case .vertical:
      Rectangle()
        .fill(Color.separator)
        .frame(width: thickness)
    }
  }

  /// One device pixel, in points: a point on a standard display, half of one on Retina.
  private var thickness: CGFloat {
    1 / max(scale, 1)
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      VStack(spacing: Spacing.m) {
        Text(verbatim: "Above")
        SeparatorLine()
        Text(verbatim: "Below")
      }
      SeparatorLine(axis: .vertical)
      Text(verbatim: "Beside")
    }
    .fixedSize()
    .sampleSurface()
  }
#endif
