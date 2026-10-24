import SwiftUI

/// A tab as a tile (v3 G1, `.stabs button`): its symbol over its word, both quiet; under the pointer a faint fill
/// and the word brightens; selected, the tile filled and the symbol blue. Held, it dips a little (v3 X9: 0.95).
struct TileTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool

  /// The tile, answering the pointer.
  func makeBody(configuration: Configuration) -> some View {
    TileTabBody(configuration: configuration, isSelected: isSelected)
  }
}
