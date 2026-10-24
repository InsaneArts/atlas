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
        TracingArc(phase: phase)
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

/// A quarter of an oval, starting `phase` of a turn along it, which SwiftUI moves round as `phase` changes.
private struct TracingArc: Shape {
  /// Where the quarter starts, a fraction of one turn; past 1 it wraps.
  var phase: Double

  var animatableData: Double {
    get { phase }
    set { phase = newValue }
  }

  /// The quarter, in two pieces when it runs past the oval's starting point.
  ///
  /// - Parameter rect: The oval's frame.
  /// - Returns: The path.
  func path(in rect: CGRect) -> Path {
    let oval = Ellipse()
      .path(in: rect)
    let start = phase - phase.rounded(.down)
    let end = start + Layout.arc
    guard end > 1 else { return oval.trimmedPath(from: start, to: end) }
    var path = oval.trimmedPath(from: start, to: 1)
    path.addPath(oval.trimmedPath(from: 0, to: end - 1))
    return path
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
