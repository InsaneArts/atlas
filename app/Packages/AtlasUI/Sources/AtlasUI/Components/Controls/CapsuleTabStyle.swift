import SwiftUI

/// A tab's look in a capsule track: its word, bright on the selected capsule, which glides from tab to tab in `thumb`
/// (cross-fading under Reduce Motion, where `thumb` is nil); quieter otherwise, on a faint capsule while pressed, and
/// faded while disabled. A style, not a view, so the tab stays a `Button` with its own label and states.
struct CapsuleTabStyle: ButtonStyle {
  /// How closely the tabs' words sit.
  enum Size: Sendable {
    /// 14 on each side of a word (v3.py `segmented`).
    case regular
    /// 8 on each side, for a narrow window.
    case compact

    /// The space on each side of a word.
    var inset: CGFloat {
      switch self {
      case .regular: Spacing.mPlus
      case .compact: Spacing.s
      }
    }
  }

  /// Whether it is the selected tab.
  let isSelected: Bool
  /// How closely the words sit.
  var size = Size.regular
  /// The namespace the selected capsule glides in; nil cross-fades it.
  let thumb: Namespace.ID?
  @Environment(\.isEnabled) private var enabled
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Draws the tab's word on its capsule.
  ///
  /// - Parameter configuration: The tab's label and pressed state.
  /// - Returns: The styled tab.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(.body, weight: .medium))
      .tracking(LetterSpacing.control)
      .foregroundStyle(isSelected ? Color.textPrimary : Color.textSecondary)
      .lineLimit(1)
      .padding(.horizontal, size.inset)
      .frame(height: ControlHeight.s)
      .background {
        if isSelected {
          selection
        } else if configuration.isPressed {
          CapsuleEdge()
            .fill(Color.fillControl)
        }
      }
      .contentShape(Capsule())
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
  }

  /// The selected capsule behind the word, gliding in `thumb` when there is one.
  @ViewBuilder private var selection: some View {
    if let thumb {
      CapsuleTabThumb()
        .matchedGeometryEffect(id: Self.thumbID, in: thumb)
    } else {
      CapsuleTabThumb()
        .transition(.opacity)
    }
  }

  /// The selected capsule's one identity, which it keeps as it moves from tab to tab.
  private static let thumbID = "selected"
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.half) {
      Button(String("Screen")) {}
        .buttonStyle(CapsuleTabStyle(isSelected: true, thumb: nil))
      Button(String("Activity")) {}
        .buttonStyle(CapsuleTabStyle(isSelected: false, thumb: nil))
    }
    .sampleSurface()
  }
#endif
