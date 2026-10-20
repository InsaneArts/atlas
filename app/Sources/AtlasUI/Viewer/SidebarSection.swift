import SwiftUI

/// An eyebrow over its card, as the sidebar's sections stand.
struct SidebarSection<Content: View>: View {
  /// The section's name.
  let title: String
  /// The card under it.
  @ViewBuilder let content: Content

  /// The eyebrow, then the card.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.m) {
      Eyebrow(text: title)
        .padding(.horizontal, Spacing.xs)
        .accessibilityAddTraits(.isHeader)
      content
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    SidebarSection(title: String("Session")) {
      CardGroup {
        FactRow(name: String("Display"), value: String("1512 × 982"), size: .regular)
      }
    }
    .frame(width: SampleWidth.narrow)
    .sampleSurface()
  }
#endif
