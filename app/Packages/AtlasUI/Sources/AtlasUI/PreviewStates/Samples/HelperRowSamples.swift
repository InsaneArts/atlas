#if DEBUG
  import SwiftUI

  /// The helper's row at the sidebar's foot in each of its states, on the sidebar's ground, then the mark alone in
  /// its circle as a short window draws it, each state beside the next.
  struct HelperRowSamples: View {
    /// Each state's row, then the circles.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.s) {
        ForEach(PreviewStates.viewerHelperLines, id: \.name) { sample in
          SidebarHelperLine(helper: sample.value) { _ in }
        }
        HStack(spacing: Spacing.xs) {
          ForEach(PreviewStates.viewerHelperLines, id: \.name) { sample in
            SidebarHelperLine(helper: sample.value, isCompact: true) { _ in }
          }
        }
      }
      .padding(ViewerLayout.sidebarInset)
      .frame(width: ViewerLayout.sidebarWidth)
      .background(Color.groundRecessed)
      .sampleSurface()
    }
  }
#endif
