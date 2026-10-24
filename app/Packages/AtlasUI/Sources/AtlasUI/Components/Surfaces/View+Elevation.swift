import SwiftUI

extension View {
  /// Lifts the view off the ground with an elevation's shadow.
  ///
  /// - Parameter level: How far the view floats.
  /// - Returns: The view with its shadow.
  func elevation(_ level: Elevation) -> some View {
    shadow(color: level.color, radius: level.radius, y: level.y)
  }
}

#if DEBUG
  #Preview("Stage") {
    RoundedRectangle(cornerRadius: Radius.large)
      .fill(Color.fillCard)
      .frame(width: SampleWidth.narrow, height: SampleWidth.narrow / 2)
      .elevation(.stage)
      .padding(Spacing.page)
      .sampleSurface()
  }
#endif
