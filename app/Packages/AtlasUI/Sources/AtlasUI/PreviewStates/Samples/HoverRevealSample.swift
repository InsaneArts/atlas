#if DEBUG
  import SwiftUI

  /// A row whose button appears only under the pointer.
  struct HoverRevealSample: View {
    @State private var isHovered = false

    /// The row, revealing its action on hover.
    var body: some View {
      HStack(spacing: Spacing.m) {
        Text(verbatim: "Opie Settings")
          .frame(maxWidth: .infinity, alignment: .leading)
        Button(String("Open")) {}
          .buttonStyle(.capsule(.secondary, size: .mini))
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
