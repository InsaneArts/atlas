import SwiftUI

/// A small turning arc; still when Reduce Motion is on.
struct Spinner: View {
  /// The spinner's side, in points.
  var size = IconSize.xs
  @State private var turning = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      Circle()
        .stroke(Color.strokeStrong, lineWidth: Stroke.thick)
      Circle()
        .trim(from: 0, to: Layout.arc)
        .stroke(Color.accent, style: StrokeStyle(lineWidth: Stroke.thick, lineCap: .round))
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
