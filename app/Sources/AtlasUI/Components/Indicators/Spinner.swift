import SwiftUI

/// A small turning arc; still when Reduce Motion is on.
struct Spinner: View {
  /// The spinner's side, in points.
  var size = Layout.size
  @State private var turning = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      Circle()
        .stroke(Layout.track, lineWidth: Stroke.thick)
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
  /// [R] 13: the work note's spinner, kept exact until Phase 3b rounds it to `IconSize.glyph` (12).
  static let size: CGFloat = 13
  /// [R] White at 0.15: the track under the arc, kept exact until Phase 3b uses `strokeStrong` (0.16).
  static let track = Color.white
    .opacity(0.15)
  /// A quarter of the circle turns.
  static let arc: CGFloat = 0.25
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.spinner)
  }
#endif
