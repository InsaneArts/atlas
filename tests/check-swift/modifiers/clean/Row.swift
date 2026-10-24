import SwiftUI

/// A row whose every chained call starts its own line.
struct Row: View {
  let names: [String]

  var body: some View {
    VStack {
      Text(verbatim: names.joined(separator: ", "))
        .font(Typography.font(.body))
        .padding(Spacing.m)
      Capsule()
        .overlay(Capsule().strokeBorder(Color.separator))
    }
    .frame(maxWidth: .infinity)
  }

  /// A single call on a type or a bare name stays with it.
  var title: String { names.first ?? Typography.name(for: names.count) }
}
