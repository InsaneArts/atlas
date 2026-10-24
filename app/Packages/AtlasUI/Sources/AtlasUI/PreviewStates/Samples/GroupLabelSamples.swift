#if DEBUG
  import SwiftUI

  /// The group label's samples: a group's name above a card's, and a longer one.
  struct GroupLabelSamples: View {
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.s) {
        GroupLabel(text: String("Ready to act"))
        GroupLabel(text: String("When something goes wrong"))
      }
      .sampleSurface()
    }
  }
#endif
