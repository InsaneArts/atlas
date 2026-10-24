#if DEBUG
  import SwiftUI

  /// The pane header's samples: one with a search field at its end, and one with nothing there.
  struct PaneHeaderSamples: View {
    /// A pane header with a search field, and one without tools.
    var body: some View {
      VStack(spacing: Spacing.m) {
        PaneHeader(title: "Apps", meta: "5 apps · 6 windows") {
          SearchField(text: "", prompt: "Search apps") { _ in }
        }
        PaneHeader(title: "Activity", meta: "38 actions today")
      }
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }
#endif
