import SwiftUI

/// The ring round Atlas's mark while something connects: an oval 14 points outside the mark, along which a bright
/// quarter travels, one linear turn every ``Motion/Duration/trace``. It shows only once ``Motion/appear``'s delay has
/// passed, so a connection that is quick never flashes it. Under Reduce Motion, and in a render, it holds still as a
/// faint whole oval instead.
struct OrbitRing: View {
  /// Where along the oval the bright quarter starts, a fraction of one turn.
  @State private var phase = OrbitRingLayout.start
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// The travelling quarter, or the still faint oval.
  var body: some View {
    ZStack {
      if isStill {
        Ellipse()
          .stroke(Color.strokeHighlight, lineWidth: Stroke.emphasis)
      } else {
        TracingArc(phase: phase, arc: OrbitRingLayout.arc)
          .stroke(
            Color.textPrimary
              .opacity(Opacity.muted),
            style: StrokeStyle(lineWidth: Stroke.emphasis, lineCap: .round)
          )
      }
    }
    .padding(-OrbitRingLayout.reach)
    .appearsAfterDelay()
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
    withTransaction(still) { phase = OrbitRingLayout.start }
    guard !isStill else { return }
    withAnimation(Motion.trace) { phase = OrbitRingLayout.start + 1 }
  }
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
