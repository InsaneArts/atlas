import SwiftUI

/// The ring round Atlas's mark while something connects (v3 V7, `.orbitring`): an oval 14 points outside the mark,
/// along which a bright quarter travels, one linear turn every ``Motion/Duration/trace``. Under Reduce Motion, and in a
/// render, it holds still as a faint whole oval instead.
struct OrbitRing: View {
  /// Where along the oval the bright quarter starts, a fraction of one turn.
  @State private var phase = Layout.start
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The travelling quarter, or the still faint oval.
  var body: some View {
    ZStack {
      if isStill {
        Ellipse()
          .stroke(Color.strokeHighlight, lineWidth: Stroke.emphasis)
      } else {
        TracingArc(phase: phase, arc: Layout.arc)
          .stroke(
            Color.textPrimary
              .opacity(Opacity.muted),
            style: StrokeStyle(lineWidth: Stroke.emphasis, lineCap: .round)
          )
      }
    }
    .padding(-Layout.reach)
    .accessibilityHidden(true)
    .onAppear(perform: trace)
    .onChange(of: isStill) { trace() }
  }

  /// Whether the ring holds still: under Reduce Motion, and in a render, which draws one frame.
  private var isStill: Bool {
    !Motion.loops(reduceMotion: reduceMotion) || !animatesArrivals
  }

  /// Sets the quarter travelling while motion is welcome, and back to its start otherwise.
  private func trace() {
    var still = Transaction()
    still.disablesAnimations = true
    withTransaction(still) { phase = Layout.start }
    guard !isStill else { return }
    withAnimation(Motion.trace) { phase = Layout.start + 1 }
  }
}

/// The ring's own measures (v3 `.orbitring`).
private enum Layout {
  /// 14: how far outside the mark's box the oval lies.
  static let reach = Spacing.mPlus
  /// A quarter of the oval shines.
  static let arc = 0.25
  /// 0.625: where the quarter starts, so at rest it sits centred over the mark's top.
  static let start = 0.625
}

#if DEBUG
  #Preview("Samples") {
    OrbitMark(size: IconSize.xxl)
      .foregroundStyle(Color.textPrimary)
      .overlay { OrbitRing() }
      .padding(Spacing.xxxl)
      .sampleSurface()
  }
#endif
