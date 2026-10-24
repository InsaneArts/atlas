import SwiftUI

/// A radio, the one mark of a choice (v3 D7): off, a faint fill in a fine white ring; on, the accent with a white dot
/// that pops in. The row around it never turns blue.
struct RadioMark: View {
  /// Whether this choice is the one picked.
  let isOn: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The ring and its dot.
  var body: some View {
    ZStack {
      Circle()
        .filledOutline(fill, border: edge, lineWidth: Stroke.regular)
      Circle()
        .fill(Color.onAccent)
        .frame(width: IconSize.xxs, height: IconSize.xxs)
        .scaleEffect(dotScale)
        .opacity(isOn ? 1 : 0)
    }
    .frame(width: IconSize.s, height: IconSize.s)
    .animation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isOn)
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

  /// The dot's scale: whole when on, none when off; under Reduce Motion it stays whole and only fades.
  private var dotScale: CGFloat {
    isOn || reduceMotion ? 1 : 0
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
