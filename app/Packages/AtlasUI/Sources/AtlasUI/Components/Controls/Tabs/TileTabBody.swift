import SwiftUI

/// The tile a ``TileTabStyle`` draws, answering the pointer.
struct TileTabBody: View {
  /// The tab's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether it is the selected tab.
  let isSelected: Bool
  @State private var isHovered = false
  @Environment(\.isEnabled) private var enabled

  /// The tile: filled while selected, faint under the pointer.
  var body: some View {
    configuration.label
      .foregroundStyle(isSelected || isHovered ? Color.textPrimary : Color.textSecondary)
      .frame(width: Layout.tile.width, height: Layout.tile.height)
      .background(
        RoundedRectangle(cornerRadius: Radius.medium, style: .continuous)
          .fill(fill)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.medium, style: .continuous))
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

/// A tile's own measures.
private enum Layout {
  /// 74 by 50: a tile.
  static let tile = CGSize(width: 74, height: 50)
}

#if DEBUG
  #Preview("Selected and not") {
    HStack(spacing: Spacing.xs) {
      Button(String("Screen")) {}
        .buttonStyle(TileTabStyle(isSelected: true))
      Button(String("Activity")) {}
        .buttonStyle(TileTabStyle(isSelected: false))
    }
    .padding()
    .sampleSurface()
  }
#endif
