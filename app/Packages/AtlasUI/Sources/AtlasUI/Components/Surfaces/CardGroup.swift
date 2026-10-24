import SwiftUI

/// A quiet rounded group of rows, stacked with no gap: separator lines go between them.
struct CardGroup<Content: View>: View {
  /// The group's corners: `Radius.large` on its own, `Radius.medium` among groups in a column, as Details and
  /// Settings draw them.
  var radius: CGFloat = Radius.large
  /// The rows.
  @ViewBuilder let content: Content

  var body: some View {
    VStack(spacing: .zero) {
      content
    }
    .cardSurface(radius: radius)
  }
}

extension CardGroup {
  /// A card of one row per item, with a separator line above every row but the first.
  ///
  /// - Parameters:
  ///   - items: The items, in order, each with an id no other shares.
  ///   - radius: The group's corners; `Radius.large` by default.
  ///   - row: The row an item draws.
  init<Items: RandomAccessCollection, Row: View>(
    _ items: Items,
    radius: CGFloat = Radius.large,
    @ViewBuilder row: @escaping (Items.Element) -> Row
  ) where Items.Element: Identifiable, Content == SeparatedRows<Items, Row> {
    self.init(radius: radius) { SeparatedRows(items: items, row: row) }
  }
}

#if DEBUG
  #Preview("Samples") {
    CardGroup([
      Fact(name: String("Helper"), value: String("Atlas")),
      Fact(name: String("Display"), value: String("1512 × 982")),
    ]) { fact in
      DetailRow(name: fact.name, value: fact.value)
    }
    .sampleSurface()
  }
#endif
