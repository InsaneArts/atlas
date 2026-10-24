import SwiftUI

/// A group of facts in an inspector (v3 V3, `.vdetails .sec`): its name in small print over its rows, which stand
/// straight on the inspector with a hairline above each.
struct FactGroup<Content: View>: View {
  /// The group's name, such as "Helper".
  private let title: String
  /// The rows, usually ``FactRow``s.
  private let content: Content

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - title: The group's name.
  ///   - content: The rows.
  init(_ title: String, @ViewBuilder content: () -> Content) {
    self.title = title
    self.content = content()
  }

  /// The group's name over its rows.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      GroupLabel(text: title)
        .padding(.bottom, Spacing.xs)
      content
    }
    .accessibilityElement(children: .contain)
  }
}

#if DEBUG
  #Preview("Samples") {
    FactGroup(String("Picture")) {
      FactRow(String("Updates"), value: String("Live"))
      FactRow(String("Newest"), value: String("just now"))
    }
    .frame(width: SampleWidth.narrow)
    .sampleSurface()
  }
#endif
