#if DEBUG
  import SwiftUI

  /// The search field's samples: empty with its prompt, then with a query.
  struct SearchFieldSamples: View {
    /// An empty search field and one with text, on the ground.
    var body: some View {
      VStack(spacing: Spacing.m) {
        SearchField(text: "", prompt: "Search activity") { _ in }
        SearchField(text: "Opie", prompt: "Search apps") { _ in }
      }
      .sampleSurface()
    }
  }
#endif
