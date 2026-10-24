import SwiftUI

/// A group label over its rows, as Details stands them: the rows straight on the pane, hairlines between them.
struct DetailsGroup<Content: View>: View {
  /// The group's name.
  let title: String
  /// The rows.
  @ViewBuilder let content: Content

  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xs) {
      GroupLabel(text: title)
      VStack(spacing: .zero) {
        content
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    DetailsGroup(title: String("Picture")) {
      DetailRow(name: String("Updates"), value: String("Live"))
      SeparatorLine()
      DetailRow(name: String("Newest"), value: String("just now"))
    }
    .frame(width: SampleWidth.narrow)
    .sampleSurface()
  }
#endif
