import SwiftUI

/// Where the person is in the steps: one long dot for the current step among short ones.
struct PageDots: View {
  /// The current step, from 0.
  let index: Int
  /// How many steps there are.
  let count: Int
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    HStack(spacing: Spacing.s) {
      // A dot is a place in the row, not an item: its position is all it is, so the position is its identity.
      ForEach(0..<count, id: \.self) { dot in
        Capsule()
          .fill(dot == index ? Color.accent : Color.fillIndicator)
          .frame(width: dot == index ? Layout.currentWidth : IconSize.dot, height: IconSize.dot)
      }
    }
    .animation(reduceMotion ? nil : Motion.indicator, value: index)
    .accessibilityElement()
    .accessibilityLabel("Step \(index + 1) of \(count)")
  }
}

/// The dots' own measures.
private enum Layout {
  /// 20: the current step's dot, long enough to read as a bar among dots.
  static let currentWidth: CGFloat = 20
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.pageDots)
  }

  #Preview("Interactive") {
    @Previewable @State var index = 0
    InteractivePreview {
      Stepper(String("Step \(index + 1)"), value: $index, in: 0...5)
    } content: {
      PageDots(index: index, count: 6)
    }
  }
#endif
