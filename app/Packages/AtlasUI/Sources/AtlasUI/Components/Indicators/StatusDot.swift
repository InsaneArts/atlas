import SwiftUI

/// A status's mark in its tone: a dot that glows, one that breathes while something works, a stop's square, or a
/// small quiet dot. The breath holds still under Reduce Motion.
struct StatusDot: View {
  /// How large a dot is; the square and the small dot keep their own sizes.
  enum Size: Sendable {
    /// 6: beside a caption or a badge's word, such as Live.
    case small
    /// 7: beside a status's word.
    case regular
  }

  /// Which mark it draws.
  let mark: StatusMark
  /// How the status reads, which the mark's colour shows.
  let tone: StatusTone
  /// How large a dot or a breathing dot is.
  var size = Size.regular
  @State private var dim = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    shape
      .frame(width: side, height: side)
      .shadow(color: glows ? tone.color : .clear, radius: Layout.glow)
      .opacity(breathes && dim ? Layout.dimmed : 1)
      .onAppear(perform: breathe)
      .onChange(of: breathes) { breathe() }
      .onChange(of: reduceMotion) { breathe() }
      .accessibilityHidden(true)
  }

  /// The mark's shape, filled in its tone.
  @ViewBuilder private var shape: some View {
    switch mark {
    case .square:
      RoundedRectangle(cornerRadius: Layout.squareCorner)
        .fill(tone.color)
    case .dot, .pulsing, .small:
      Circle()
        .fill(tone.color)
    }
  }

  /// Whether the mark breathes.
  private var breathes: Bool { mark == .pulsing }

  /// Whether the mark glows: a dot in any tone but the quiet one; the square and the small dot never do.
  private var glows: Bool {
    (mark == .dot || mark == .pulsing) && tone.glows
  }

  /// The mark's side.
  private var side: CGFloat {
    switch (mark, size) {
    case (.square, _): Layout.square
    case (.small, _), (_, .small): IconSize.dot
    case (_, .regular): Layout.regular
    }
  }

  /// Starts the breath while the dot pulses and motion is welcome, and stills it otherwise.
  private func breathe() {
    guard breathes, !reduceMotion else {
      dim = false
      return
    }
    withAnimation(Motion.pulse) { dim = true }
  }
}

/// The marks' own measures, from the boards (v3.py `dot`, `square`).
private enum Layout {
  /// 7: a status dot.
  static let regular: CGFloat = 7
  /// 8: the stopped square.
  static let square: CGFloat = 8
  /// 2: the square's corners, softened as the system's stop buttons are.
  static let squareCorner: CGFloat = 2
  /// 4: the glow around a dot, half the board's 8-point CSS blur, since a shadow's radius spreads about twice as far.
  static let glow: CGFloat = 4
  /// 0.35: the low point of a breath.
  static let dimmed = 0.35
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.statusDot)
  }

  #Preview("Interactive") {
    @Previewable @State var mark = StatusMark.pulsing
    @Previewable @State var size = StatusDot.Size.regular
    @Previewable @State var tone = StatusTone.active
    InteractivePreview {
      Picker(String("Mark"), selection: $mark) {
        Text(verbatim: "Dot")
          .tag(StatusMark.dot)
        Text(verbatim: "Pulsing")
          .tag(StatusMark.pulsing)
        Text(verbatim: "Square")
          .tag(StatusMark.square)
        Text(verbatim: "Small")
          .tag(StatusMark.small)
      }
      Picker(String("Size"), selection: $size) {
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
        Text(verbatim: "Link")
          .tag(StatusTone.link)
        Text(verbatim: "Attention")
          .tag(StatusTone.attention)
        Text(verbatim: "Quiet")
          .tag(StatusTone.quiet)
      }
    } content: {
      StatusDot(mark: mark, tone: tone, size: size)
    }
  }
#endif
