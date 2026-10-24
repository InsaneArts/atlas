#if DEBUG
  import SwiftUI

  /// The search field's samples: empty with its prompt, then with a query.
  struct SearchFieldSamples: View {
    var body: some View {
      VStack(spacing: Spacing.l) {
        SearchField(text: "", prompt: "Search activity") { _ in }
        SearchField(text: "Opie", prompt: "Search apps") { _ in }
      }
      .sampleSurface()
    }
  }
#endif
