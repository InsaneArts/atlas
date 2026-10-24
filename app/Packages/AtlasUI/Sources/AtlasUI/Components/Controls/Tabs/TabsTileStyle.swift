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

/// The tile a ``TileTabStyle`` draws, answering the pointer.
private struct TileTabBody: View {
  /// The tab's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether it is the selected tab.
  let isSelected: Bool
  @State private var isHovered = false
  @Environment(\.isEnabled) private var enabled

  var body: some View {
    configuration.label
      .foregroundStyle(isSelected || isHovered ? Color.textPrimary : Color.textSecondary)
      .frame(width: Layout.tile.width, height: Layout.tile.height)
      .background(
        RoundedRectangle(cornerRadius: Radius.medium)
          .fill(fill)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.medium))
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.tilePressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .onHover { isHovered = $0 }
      .animation(Motion.hover(isHovered), value: isHovered)
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }

  /// The tile's fill: the held fill while selected, a faint one under the pointer, none otherwise.
  private var fill: Color {
    if isSelected { return .fillControlOn }
    return isHovered ? .fillRowHover : .clear
  }
}

/// A tile's own measures (v3 G1, `.stabs`).
private enum Layout {
  /// 74 by 50: a tile.
  static let tile = CGSize(width: 74, height: 50)
  /// 0.95: how far a held tile dips (v3 `.stabs button:active`).
  static let tilePressedScale: CGFloat = 0.95
}
