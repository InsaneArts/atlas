import SwiftUI

/// A divider one device pixel thick, so it never drops out at any scale.
struct SeparatorLine: View {
  /// Which way the line runs.
  var axis = Axis.horizontal
  @Environment(\.displayScale) private var scale

  var body: some View {
    switch axis {
    case .horizontal:
      Rectangle()
        .fill(Color.separator)
        .frame(height: 1 / max(scale, 1))
    case .vertical:
      Rectangle()
        .fill(Color.separator)
        .frame(width: 1 / max(scale, 1))
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      VStack(spacing: Spacing.l) {
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
