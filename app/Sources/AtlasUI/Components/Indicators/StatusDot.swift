import SwiftUI

/// A dot in a status's tone, which breathes while something works; steady when Reduce Motion is on.
struct StatusDot: View {
  /// How large the dot is.
  enum Size: Sendable {
    /// [R] 5: beside a badge's word, such as Live, kept exact until Phase 3b rounds it to `IconSize.dot` (6).
    case live
    /// 6: beside a caption.
    case small
    /// [R] 7: beside a status, kept exact until Phase 3b rounds it to 6 or 8.
    case regular
  }

  /// How the status reads, which the dot's colour shows.
  let tone: StatusTone
  /// Whether it breathes.
  var pulsing = false
  /// How large the dot is.
  var size = Size.regular
  @State private var dim = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Circle()
      .fill(tone.color)
      .frame(width: side, height: side)
      .opacity(pulsing && dim ? Layout.dimmed : 1)
      .onAppear(perform: breathe)
      .onChange(of: pulsing) { breathe() }
      .onChange(of: reduceMotion) { breathe() }
      .accessibilityHidden(true)
  }

  private var side: CGFloat {
    switch size {
    case .live: Layout.live
    case .small: IconSize.dot
    case .regular: Layout.regular
    }
  }

  /// Starts the breath while the dot pulses and motion is welcome, and stills it otherwise.
  private func breathe() {
    guard pulsing, !reduceMotion else {
      dim = false
      return
    }
    withAnimation(Motion.pulse) { dim = true }
  }
}

/// The dot's own measures.
private enum Layout {
  /// [R] 5: the live dot, kept exact until Phase 3b rounds it to `IconSize.dot` (6).
  static let live: CGFloat = 5
  /// [R] 7: a status dot, kept exact until Phase 3b rounds it to 6 or 8.
  static let regular: CGFloat = 7
  /// 0.35: the low point of a breath.
  static let dimmed = 0.35
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.statusDot)
  }

  #Preview("Interactive") {
    @Previewable @State var pulsing = true
    @Previewable @State var size = StatusDot.Size.regular
    @Previewable @State var tone = StatusTone.active
    InteractivePreview {
      Toggle(String("Pulsing"), isOn: $pulsing)
      Picker(String("Size"), selection: $size) {
        Text(verbatim: "Live")
          .tag(StatusDot.Size.live)
        Text(verbatim: "Small")
          .tag(StatusDot.Size.small)
        Text(verbatim: "Regular")
          .tag(StatusDot.Size.regular)
      }
      Picker(String("Tone"), selection: $tone) {
        Text(verbatim: "Ready")
          .tag(StatusTone.ready)
        Text(verbatim: "Active")
          .tag(StatusTone.active)
        Text(verbatim: "Attention")
          .tag(StatusTone.attention)
        Text(verbatim: "Quiet")
          .tag(StatusTone.quiet)
      }
    } content: {
      StatusDot(tone: tone, pulsing: pulsing, size: size)
    }
  }
#endif
