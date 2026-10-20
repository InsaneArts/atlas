import SwiftUI

/// Atlas's capsule buttons: white for the next step, glass beside it, plain for the quiet way out. A style, not a
/// view, so the control stays a `Button` with its own label, role, shortcut and disabled state.
struct CapsuleButtonStyle: ButtonStyle {
  /// How the capsule is painted.
  enum Look: Sendable {
    /// White with dark text: the one thing to do next.
    case primary
    /// A glass fill with a fine edge: the actions beside it.
    case glass
    /// Text alone: the quiet way out, such as Back.
    case plain
  }

  /// How large the capsule is.
  enum Size: Sendable {
    /// 34 high, 13-point text: in a footer, a card or a panel.
    case regular
    /// 44 high, 14-point text: a screen's one main action.
    case large
  }

  /// How the capsule is painted.
  var look = Look.primary
  /// How large the capsule is.
  var size = Size.regular
  /// Whether it takes the whole width offered, so side-by-side buttons share a row.
  var fill = false
  @Environment(\.isEnabled) private var enabled

  /// Draws the label on its capsule.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(textStyle, weight: weight))
      .lineLimit(1)
      .padding(.horizontal, inset)
      .frame(maxWidth: fill ? .infinity : nil)
      .frame(height: height)
      .foregroundStyle(foreground)
      .background(
        Capsule()
          .fill(background)
      )
      .overlay(
        Capsule()
          .strokeBorder(look == .glass ? Color.strokeStrong : .clear, lineWidth: Stroke.hairline)
      )
      .contentShape(Capsule())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.press, value: configuration.isPressed)
  }

  /// The label's size: a step larger on a large capsule.
  private var textStyle: TextStyle {
    switch size {
    case .regular: .body
    case .large: .headline
    }
  }

  /// The label's weight: regular for plain text, semibold on a capsule.
  private var weight: Font.Weight {
    look == .plain ? .regular : .semibold
  }

  /// The capsule's height.
  private var height: CGFloat {
    switch size {
    case .regular: ControlHeight.button
    case .large: ControlHeight.large
    }
  }

  /// The space on each side of the label: wide for a large capsule, narrow for plain text or a capsule sharing
  /// its row.
  private var inset: CGFloat {
    if size == .large { return Layout.largeInset }
    if look == .plain { return Spacing.l }
    return fill ? Spacing.ml : Layout.regularInset
  }

  private var foreground: Color {
    switch look {
    case .primary: .ground
    case .glass: .textPrimary
    case .plain: .textSecondary
    }
  }

  private var background: Color {
    switch look {
    case .primary: .textPrimary
    case .glass: .fillControl
    case .plain: .clear
    }
  }
}

/// The capsule's own measures, off the spacing grid's steps.
private enum Layout {
  /// 26: a large capsule's label inset, so a screen's main action reads wider than its text.
  static let largeInset: CGFloat = 26
  /// 18: a regular capsule's label inset when it hugs its label.
  static let regularInset: CGFloat = 18
  /// 0.97: how far a capsule shrinks while pressed.
  static let pressedScale: CGFloat = 0.97
}

#if DEBUG
  #Preview("Looks") {
    CapsuleButtonSamples(.looks)
  }

  #Preview("Sizes") {
    CapsuleButtonSamples(.sizes)
  }

  #Preview("Sharing a row") {
    CapsuleButtonSamples(.fill)
  }

  #Preview("Interactive") {
    @Previewable @State var look = CapsuleButtonStyle.Look.primary
    @Previewable @State var size = CapsuleButtonStyle.Size.regular
    @Previewable @State var isEnabled = true
    InteractivePreview {
      Picker(String("Look"), selection: $look) {
        Text(verbatim: "Primary")
          .tag(CapsuleButtonStyle.Look.primary)
        Text(verbatim: "Glass")
          .tag(CapsuleButtonStyle.Look.glass)
        Text(verbatim: "Plain")
          .tag(CapsuleButtonStyle.Look.plain)
      }
      Picker(String("Size"), selection: $size) {
        Text(verbatim: "Regular")
          .tag(CapsuleButtonStyle.Size.regular)
        Text(verbatim: "Large")
          .tag(CapsuleButtonStyle.Size.large)
      }
      Toggle(String("Enabled"), isOn: $isEnabled)
    } content: {
      Button(String("Continue")) {}
        .buttonStyle(.capsule(look, size: size))
        .disabled(!isEnabled)
    }
  }
#endif
