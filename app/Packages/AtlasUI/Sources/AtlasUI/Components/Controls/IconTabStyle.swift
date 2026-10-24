import SwiftUI

/// A tab's look in a row of icon tabs, as a settings window's: its icon over its word in a 76 by 50 tile, the icon
/// blue and the word bright on a faint fill while selected, both quiet otherwise, the tile dimmed while pressed and
/// faded while disabled. The label sets its icon in the secondary style, which takes the blue. A style, not a view,
/// so the tab stays a `Button` with its own label and states.
struct IconTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  @Environment(\.isEnabled) private var enabled
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Draws the tab's icon and word on its tile.
  ///
  /// - Parameter configuration: The tab's label and pressed state.
  /// - Returns: The styled tab.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .foregroundStyle(
        isSelected ? Color.textPrimary : Color.textSecondary,
        isSelected ? Color.textLink : Color.textSecondary
      )
      .frame(width: Layout.width, height: Layout.height)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(isSelected ? Color.fillControlRaised : configuration.isPressed ? Color.fillControl : Color.clear)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.small))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
  }
}

/// The tab's own measures, from the board (mac.py `settings_window`).
private enum Layout {
  /// 76: a tab's width.
  static let width: CGFloat = 76
  /// 50: a tab's height.
  static let height: CGFloat = 50
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.iconTab)
  }
#endif
