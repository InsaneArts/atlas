import SwiftUI

/// A tab in a small rounded track, as a pane's filters (v3 `.seg`): its word bright on the thumb while selected,
/// quiet otherwise and brightening while held, which also dips it a little.
struct SegmentTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  /// The namespace the thumb glides in.
  let thumb: Namespace.ID
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
          RoundedRectangle(cornerRadius: Radius.extraSmall)
            .fill(Color.fillControlOn)
            .shadow(color: Color.dropShadowFaint, radius: Layout.thumbShadow, y: Layout.thumbDrop)
            .matchedGeometryEffect(id: TabsStyle.thumbID, in: thumb)
        }
      }
      .contentShape(RoundedRectangle(cornerRadius: Radius.extraSmall))
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }
}

/// A segment's own measures, from v3's filters (`.seg button[aria-pressed]`).
private enum Layout {
  /// 0.96: how far a held tab dips, as the track's (v3 X9).
  static let pressedScale: CGFloat = 0.96
  /// 1: the thumb's shadow, half v3's 2-point blur.
  static let thumbShadow: CGFloat = 1
  /// 1: how far the thumb's shadow falls.
  static let thumbDrop: CGFloat = 1
}
