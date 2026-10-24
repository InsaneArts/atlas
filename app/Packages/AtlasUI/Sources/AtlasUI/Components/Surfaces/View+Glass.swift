import SwiftUI

extension View {
  /// Sets the view on glass in a shape: the system glass on macOS 26 and later, the thin material under a
  /// tint before it, and an opaque fill under Reduce Transparency.
  ///
  /// - Parameters:
  ///   - style: How thick the glass is.
  ///   - shape: The surface's outline, such as a capsule.
  /// - Returns: The view on its glass.
  func glass(_ style: GlassStyle, in shape: some Shape) -> some View {
    modifier(GlassSurface(style: style, shape: shape))
  }
}

#if DEBUG
  #Preview("Glass") {
    VStack(spacing: Spacing.l) {
      ForEach(GlassStyle.allCases, id: \.self) { style in
        Text(verbatim: "\(style)")
          .textStyle(.body)
          .padding(.horizontal, Spacing.l)
          .frame(height: ControlHeight.sPlus)
          .glass(style, in: Capsule())
      }
    }
    .padding(Spacing.xxxl)
    .sampleSurface()
  }
#endif
