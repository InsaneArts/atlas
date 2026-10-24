#if DEBUG
  import SwiftUI

  /// A window row whose Show on Screen appears only under the pointer (v3 R7, V9).
  struct HoverRevealSample: View {
    @State private var isHovered = false

    /// The row, revealing its action on hover.
    var body: some View {
      HStack(spacing: Spacing.m) {
        Text(verbatim: "Opie Settings")
          .frame(maxWidth: .infinity, alignment: .leading)
        Button(String("Show on Screen")) {}
          .buttonStyle(.capsule(.glass, size: .mini))
          .revealsOnHover(isHovered)
      }
      .padding(.horizontal, Spacing.mPlus)
      .frame(width: SampleWidth.column, height: ControlHeight.mPlus)
      .contentShape(Rectangle())
      .onHover { isHovered = $0 }
      .sampleSurface()
    }
  }
#endif
