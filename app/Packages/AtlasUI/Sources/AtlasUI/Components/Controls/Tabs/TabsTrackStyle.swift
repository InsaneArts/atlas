import SwiftUI

/// A tab in the track: its label bright on the thumb while selected, quiet otherwise and brightening while held, which
/// also dips it a little (v3 X9: tabs 0.96).
struct TrackTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  /// What the tab shows: a symbol sits closer to its edges than a word.
  let face: TabsFace
  /// The namespace the thumb glides in.
  let thumb: Namespace.ID
  @Environment(\.isEnabled) private var enabled

  /// The tab's label, on the thumb while selected.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(.body, weight: .medium))
      .foregroundStyle(isSelected || configuration.isPressed ? Color.textPrimary : Color.textSecondary)
      .lineLimit(1)
      .padding(.horizontal, face == .symbol ? Spacing.sPlus : Spacing.mPlus)
      .frame(height: TabsLayout.trackHeight)
      .background {
        if isSelected {
          TabsThumb()
            .matchedGeometryEffect(id: TabsStyle.thumbID, in: thumb)
        }
      }
      .contentShape(Capsule())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }
}

/// A track tab's own measures.
private enum Layout {
  /// 0.96: how far a held tab dips (v3 X9).
  static let pressedScale: CGFloat = 0.96
}

#if DEBUG
  #Preview("Selected and not") {
    @Previewable @Namespace var thumb
    HStack(spacing: TabsLayout.trackGap) {
      Button(String("Screen")) {}
        .buttonStyle(TrackTabStyle(isSelected: true, face: .word, thumb: thumb))
      Button(String("Activity")) {}
        .buttonStyle(TrackTabStyle(isSelected: false, face: .word, thumb: thumb))
    }
    .padding()
    .sampleSurface()
  }
#endif
