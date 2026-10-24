import SwiftUI

/// Atlas's capsule buttons: white for the next step, glass beside it, plain for the quiet way out, and the accent,
/// tinted and destructive looks an alert's buttons wear; a circle of glass for a symbol alone. A style, not a view, so
/// the control stays a `Button` with its own label, role, shortcut and disabled state.
///
/// Each answers the pointer (v3 P9, X9): a glass fill steps from 0.08 at rest to 0.12 under the pointer and 0.16 while
/// held, the white capsule goes pure white and then #E3E3E8; it shrinks in 0.1 seconds as it is pressed and comes back
/// on a small spring. Under Reduce Motion the fill still changes and the size does not.
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
    /// 30 high, 13-point text: in the panel, an alert and an empty message; a circle 30 across.
    case small
    /// 34 high, 13-point text: in a footer, a card or the top row; a circle 34 across.
    case regular
    /// 44 high, 14-point text: a screen's one main action.
    case large
  }

  /// The shape around the label.
  enum Outline: Sendable {
    /// A capsule around a title.
    case pill
    /// A circle the capsule's height across, for a button whose label is a symbol alone.
    case circle
  }

  /// What a glass look is laid on.
  enum Backing: Sendable {
    /// Its fill alone, over the window's ground, as every surface draws it until it is redesigned.
    case fill
    /// The system's glass under its fill (v3 D12), opaque under Reduce Transparency: a button floating over a picture.
    case glass
  }

  /// How the capsule is painted.
  var look = Look.primary
  /// How large the capsule is.
  var size = Size.regular
  /// Whether it takes the whole width offered, so side-by-side buttons share a row.
  var fill = false
  /// The shape around the label.
  var outline = Outline.pill
  /// Whether what it toggles is on, which a glass button shows with the held fill.
  var isOn = false
  /// What a glass look is laid on.
  var backing = Backing.fill

  /// Draws the label on its capsule or circle.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    CapsuleButtonBody(configuration: configuration, style: self)
  }

  /// The capsule's fill (v3 P9, R2): a glass one steps 0.08 → 0.12 → 0.16 at rest, under the pointer and while held,
  /// and shows the held fill while what it toggles is on; the white one goes pure white under the pointer and #E3E3E8
  /// while held. The other looks keep one fill. A disabled button keeps its resting fill.
  ///
  /// - Parameters:
  ///   - look: How the capsule is painted.
  ///   - isOn: Whether what it toggles is on.
  ///   - isHovered: Whether the pointer is over it.
  ///   - isPressed: Whether it is held down.
  ///   - isEnabled: Whether it can be pressed.
  /// - Returns: The fill.
  nonisolated static func fill(
    look: Look,
    isOn: Bool = false,
    isHovered: Bool,
    isPressed: Bool,
    isEnabled: Bool
  ) -> Color {
    let hovered = isEnabled && isHovered
    let pressed = isEnabled && isPressed
    switch look {
    case .primary:
      if pressed { return .fillPrimaryPressed }
      return hovered ? .fillPrimaryHover : .textPrimary
    case .glass:
      if isOn || pressed { return .fillControlOn }
      return hovered ? .fillControlHover : .fillControl
    case .pressed: return .fillControlOn
    case .tinted, .destructive: return .fillControlRaised
    case .plain: return .clear
    case .accent: return .accent
    }
  }

  /// How opaque the button is: whole, or dimmed to the disabled opacity when it cannot be pressed.
  ///
  /// - Parameter isEnabled: Whether it can be pressed.
  /// - Returns: The opacity.
  nonisolated static func opacity(isEnabled: Bool) -> Double {
    isEnabled ? 1 : Opacity.disabled
  }

  /// How a press moves the button: down over ``Motion/press``, back up on ``Motion/release``'s small spring (v3 X9).
  ///
  /// - Parameter isPressed: Whether it is held down now.
  /// - Returns: The animation for the change.
  nonisolated static func pressAnimation(isPressed: Bool) -> Animation {
    isPressed ? Motion.press : Motion.release
  }
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

  #Preview("Circles") {
    CapsuleButtonSamples(.circles)
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
      HStack(spacing: Spacing.s) {
        Button(String("Continue")) {}
          .buttonStyle(.capsule(look, size: size))
        Button(String("Pause"), systemImage: SymbolName.pause.rawValue) {}
          .buttonStyle(.capsule(.glass, size: size, outline: .circle))
      }
      .disabled(!isEnabled)
    }
  }
#endif
