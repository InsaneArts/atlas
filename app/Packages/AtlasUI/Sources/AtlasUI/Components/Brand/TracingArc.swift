import SwiftUI

/// A stretch of an oval, `arc` of a turn long, starting `phase` of a turn along it, which SwiftUI moves round as
/// `phase` changes: the bright quarter ``OrbitRing`` traces.
struct TracingArc: Shape {
  /// Where the stretch starts, a fraction of one turn; past 1 it wraps.
  var phase: Double
  /// How long the stretch is, a fraction of one turn.
  let arc: Double

  /// The start, which SwiftUI moves round.
  var animatableData: Double {
    get { phase }
    set { phase = newValue }
  }

  /// The stretch, in two pieces when it runs past the oval's starting point.
  ///
  /// - Parameter rect: The oval's frame.
  /// - Returns: The path.
  func path(in rect: CGRect) -> Path {
    let oval = Ellipse()
      .path(in: rect)
    let start = phase - phase.rounded(.down)
    let end = start + arc
    guard end > 1 else { return oval.trimmedPath(from: start, to: end) }
    var path = oval.trimmedPath(from: start, to: 1)
    path.addPath(oval.trimmedPath(from: 0, to: end - 1))
    return path
  }
}

#if DEBUG
  #Preview("A quarter") {
    TracingArc(phase: 0.625, arc: 0.25)
      .stroke(Color.textPrimary, lineWidth: Stroke.thick)
      .frame(width: IconSize.xxl, height: IconSize.xxl)
      .padding()
      .sampleSurface()
  }
#endif
