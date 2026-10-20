import SwiftUI

/// A quiet rounded group of rows, stacked with no gap: separator lines go between them.
struct CardGroup<Content: View>: View {
  /// The rows.
  @ViewBuilder let content: Content

  var body: some View {
    VStack(spacing: 0) {
      content
    }
    .cardSurface()
  }
}

extension CardGroup {
  /// A card of one row per item, with a separator line above every row but the first.
  ///
  /// - Parameters:
  ///   - items: The items, in order, each with an id no other shares.
  ///   - row: The row an item draws.
  init<Items: RandomAccessCollection, Row: View>(
    _ items: Items,
    @ViewBuilder row: @escaping (Items.Element) -> Row
  ) where Items.Element: Identifiable, Content == SeparatedRows<Items, Row> {
    self.init { SeparatedRows(items: items, row: row) }
  }
}

#if DEBUG
  #Preview("Samples") {
    CardGroup(PreviewStates.panelWatching.facts) { fact in
      FactRow(name: fact.name, value: fact.value)
    }
    .sampleSurface()
  }
#endif
