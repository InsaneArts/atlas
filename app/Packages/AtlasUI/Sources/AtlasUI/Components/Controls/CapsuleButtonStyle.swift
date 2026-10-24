import SwiftUI

/// Atlas's capsule buttons: white for the next step, glass beside it, plain for the quiet way out, and the accent,
/// tinted and destructive looks an alert's buttons wear. A style, not a view, so the control stays a `Button` with
/// its own label, role, shortcut and disabled state.
struct CapsuleButtonStyle: ButtonStyle {
  /// How the capsule is painted.
  enum Look: Sendable {
    /// White with dark text: the one thing to do next.
    case primary
    /// A glass fill with a fine edge: the actions beside it.
    case glass
    /// Text alone: the quiet way out, such as Back.
    case plain
    /// The pressed fill with a brighter edge: a capsule whose state is on, such as Keys while in control.
    case pressed
    /// The accent with white text: a default action, such as an alert's.
    case accent
    /// A raised quiet fill with plain text: another action beside the default, such as an alert's Cancel.
    case tinted
    /// A raised quiet fill with red text: a destructive action, such as an alert's Log Out.
    case destructive
  }

  /// How large the capsule is.
  enum Size: Sendable {
    /// 26 high, 13-point text: in a settings row.
    case mini
    /// 30 high, 13-point text: in the panel, an alert and an empty message.
    case small
    /// 34 high, 13-point text: in a footer, a card or the top row.
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
      .tracking(tracking)
      .lineLimit(1)
      .padding(.horizontal, inset)
      .frame(maxWidth: fill ? .infinity : nil)
      .frame(height: height)
      .foregroundStyle(foreground)
      .background(
        Capsule()
          .fill(background)
      )
      // A capsule's shape drawn as a rounded rectangle: a stroked `Capsule` leaves a hairline outside each end where
      // AppKit draws the view into an image, as the design renders do, and this one draws the same edge without it.
      .overlay(
        RoundedRectangle(cornerRadius: height / 2, style: .circular)
          .strokeBorder(edge, lineWidth: Stroke.hairline)
      )
      .contentShape(Capsule())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.press, value: configuration.isPressed)
  }

  /// The label's size: a step larger on a large capsule.
  private var textStyle: TextStyle {
    switch size {
    case .mini, .small, .regular: .body
    case .large: .headline
    }
  }

  /// The label's weight: regular for plain text, medium on an alert's buttons, semibold on a capsule.
  private var weight: Font.Weight {
    switch look {
    case .plain: .regular
    case .accent, .tinted, .destructive: .medium
    case .primary, .glass, .pressed: .semibold
    }
  }

  /// The label's letter spacing: drawn in on a capsule, as the boards set every capsule's word (v3.py `pill`); plain
  /// text and an alert's buttons keep the font's own.
  private var tracking: CGFloat {
    switch look {
    case .primary, .glass, .pressed: LetterSpacing.control
    case .plain, .accent, .tinted, .destructive: 0
    }
  }

  /// The capsule's height.
  private var height: CGFloat {
    switch size {
    case .mini: ControlHeight.xsPlus
    case .small: ControlHeight.sPlus
    case .regular: ControlHeight.mPlus
    case .large: ControlHeight.xl
    }
  }

  /// The space on each side of the label: wide for a large capsule, narrow for plain text or a capsule sharing
  /// its row.
  private var inset: CGFloat {
    if size == .large { return Layout.largeInset }
    if look == .plain { return Spacing.m }
    return fill ? Spacing.sPlus : Layout.regularInset
  }

  private var foreground: Color {
    switch look {
    case .primary: .ground
    case .glass, .pressed, .tinted: .textPrimary
    case .plain: .textSecondary
    case .accent: .onAccent
    case .destructive: .textDestructive
    }
  }

  /// The capsule's fill: near white for primary, a quiet fill for glass, pressed, tinted and destructive, none for
  /// plain, and the accent colour itself, system blue, for the accent look.
  private var background: Color {
    switch look {
    case .primary: .textPrimary
    case .glass: .fillControl
    case .tinted, .destructive: .fillControlRaised
    case .pressed: .fillControlOn
    case .plain: .clear
    case .accent: .accent
    }
  }

  /// The capsule's edge: a glass hairline, a brighter one while pressed, none otherwise.
  private var edge: Color {
    switch look {
    case .glass: .strokeStrong
    case .pressed: .strokePressed
    case .primary, .plain, .accent, .tinted, .destructive: .clear
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

  #Preview("Alert") {
    CapsuleButtonSamples(.alert)
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
        Text(verbatim: "Pressed")
          .tag(CapsuleButtonStyle.Look.pressed)
        Text(verbatim: "Accent")
          .tag(CapsuleButtonStyle.Look.accent)
        Text(verbatim: "Tinted")
          .tag(CapsuleButtonStyle.Look.tinted)
        Text(verbatim: "Destructive")
          .tag(CapsuleButtonStyle.Look.destructive)
      }
      Picker(String("Size"), selection: $size) {
        Text(verbatim: "Mini")
          .tag(CapsuleButtonStyle.Size.mini)
        Text(verbatim: "Small")
          .tag(CapsuleButtonStyle.Size.small)
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
