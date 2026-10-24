import SwiftUI

/// One row per item with a separator line above every row but the first, as a card of rows stacks them.
struct SeparatedRows<Items: RandomAccessCollection, Row: View>: View where Items.Element: Identifiable {
  /// The items, in order.
  let items: Items
  /// The row an item draws.
  @ViewBuilder let row: (Items.Element) -> Row

  var body: some View {
    ForEach(items) { item in
      if item.id != items.first?.id {
        SeparatorLine()
      }
      row(item)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.separatedRows)
  }
#endif
