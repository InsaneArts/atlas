import SwiftUI

/// A small turning arc on a faint ring, at one size whatever it shows; still when Reduce Motion is on.
struct Spinner: View {
  /// The spinner's side, in points.
  var size = IconSize.xs
  /// The turning arc's colour: the accent, or white inside an accent button.
  var arc = Color.accent
  /// The faint ring the arc turns on: a fine edge (``Color/strokeStrong``) by default.
  var track = Color.strokeStrong
  @State private var turning = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// A faint ring with the arc turning on it.
  var body: some View {
    ZStack {
      Circle()
        .stroke(track, lineWidth: Stroke.thick)
      Circle()
        .trim(from: 0, to: Layout.arc)
        .stroke(arc, style: StrokeStyle(lineWidth: Stroke.thick, lineCap: .round))
        .rotationEffect(.degrees(turning ? 360 : 0))
    }
    .frame(width: size, height: size)
    .onAppear(perform: spin)
    .onChange(of: reduceMotion) { spin() }
    .accessibilityHidden(true)
  }

  /// Turns the arc while motion is welcome, and stills it once Reduce Motion is on.
  private func spin() {
    guard !reduceMotion else {
      turning = false
      return
    }
    withAnimation(Motion.spin) { turning = true }
  }
}

/// The spinner's own measures.
private enum Layout {
  /// A quarter of the circle turns.
  static let arc: CGFloat = 0.25
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.spinner)
  }
#endif
