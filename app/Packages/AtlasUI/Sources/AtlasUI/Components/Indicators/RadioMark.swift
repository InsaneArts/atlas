import SwiftUI

/// A radio, the one mark of a choice: off, a faint fill in a fine white ring; on, the accent with a white dot. The fill
/// crossfades over ``Motion/markFill`` with nothing scaling, as the system's radio does. The row around it never turns
/// blue.
struct RadioMark: View {
  /// Whether this choice is the one picked.
  let isOn: Bool

  /// The ring and its dot.
  var body: some View {
    ZStack {
      Circle()
        .filledOutline(fill, border: edge, lineWidth: Stroke.regular)
      Circle()
        .fill(Color.onAccent)
        .frame(width: IconSize.xxs, height: IconSize.xxs)
        .opacity(isOn ? 1 : 0)
    }
    .frame(width: IconSize.s, height: IconSize.s)
    .animation(Motion.markFill, value: isOn)
    .accessibilityHidden(true)
  }

  /// The ring's fill: the accent when on, a faint white when off.
  private var fill: Color {
    isOn ? .accent : .fillPill
  }

  /// The ring's edge: a fine white ring when off, none on the accent.
  private var edge: Color {
    isOn ? .clear : .strokePressed
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      RadioMark(isOn: false)
      RadioMark(isOn: true)
    }
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var isOn = false
    InteractivePreview {
      Toggle(String("On"), isOn: $isOn)
    } content: {
      RadioMark(isOn: isOn)
    }
  }
#endif
