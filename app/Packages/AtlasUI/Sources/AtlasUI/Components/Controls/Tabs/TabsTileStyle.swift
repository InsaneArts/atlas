import SwiftUI

/// A tab as a tile: its symbol over its word, both quiet; under the pointer a faint fill and the word
/// brightens; selected, the tile filled and the symbol blue. Held, it scales to 0.95 for press feedback.
struct TileTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool

  /// The tile, answering the pointer.
  func makeBody(configuration: Configuration) -> some View {
    TileTabBody(configuration: configuration, isSelected: isSelected)
  }
}
