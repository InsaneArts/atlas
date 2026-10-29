import SwiftUI

/// A tab in a small capsule track, as a pane's filters: its word bright on a raised thumb while selected, quiet
/// otherwise and brightening while held; it never shrinks.
struct SegmentTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  @Environment(\.isEnabled) private var enabled

  /// The tab's word, on the thumb while selected.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(.callout, weight: .medium))
      .foregroundStyle(isSelected || configuration.isPressed ? Color.textPrimary : Color.textSecondary)
      .lineLimit(1)
      .padding(.horizontal, Spacing.sPlus)
      .frame(height: ControlHeight.xs)
      .background {
        if isSelected {
          TabsSegmentThumb()
        }
      }
      .transaction(value: isSelected) { $0.animation = nil }
      .contentShape(Capsule())
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }
}
