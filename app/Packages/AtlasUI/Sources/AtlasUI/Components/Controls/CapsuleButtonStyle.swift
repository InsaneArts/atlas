import SwiftUI

/// Atlas's capsule buttons: the text's colour for the next step, a quiet fill beside it, plain for the quiet way out,
/// the accent, tinted and destructive looks an alert's buttons wear, and a solid dark circle over a picture; a filled
/// circle for a symbol alone. A style, not a view, so the control stays a `Button` with its own label, role, shortcut
/// and disabled state. Each is a fill alone: nothing is glass or a material.
///
/// Each answers the pointer: a secondary fill steps from 0.08 at rest to 0.12 under the pointer and 0.16 while held,
/// the main capsule steps off its colour, in the dark to pure white and then #E3E3E8, and the accent darkens a tenth
/// while held. The held fill lands at once on mouse-down and eases back over 0.12 seconds once let go; the button never changes size.
struct CapsuleButtonStyle: ButtonStyle {
  /// How the capsule is painted.
  enum Look: Sendable {
    /// The text's colour with the ground's words, white in the dark and near black in the light: the one thing to do
    /// next.
    case primary
    /// A quiet fill: the actions beside it.
    case secondary
    /// Text alone: the quiet way out, such as Back.
    case plain
    /// The held fill: a capsule whose state is on, such as Keys while in control.
    case pressed
    /// The accent with white text: a default action, such as an alert's; brighter under the pointer.
    case accent
    /// A raised quiet fill with plain text: another action beside the default, such as an alert's Not Now; brighter
    /// under the pointer.
    case tinted
    /// The tinted capsule with red text: a destructive action, such as an alert's Log Out.
    case destructive
    /// A symbol alone, unfilled at rest: a title bar's quiet toggle, such as the sidebar's. A faint fill under the pointer,
    /// and the held fill, its symbol brightening, while held or while what it toggles is on.
    case glyph
    /// An opaque dark fill with a light label, the same in either appearance, for a control resting on a picture,
    /// such as the panel picture's pause circle; a step lighter under the pointer, and again while held or on.
    case overPicture
  }

  /// How large the capsule is.
  enum Size: Sendable {
    /// 26 high, 13-point text: in a settings row.
    case mini
    /// 28 high, 13-point text: an alert's buttons.
    case compact
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
    /// A capsule around a symbol and its title, a little tighter before the symbol, whose own room reads as part of
    /// the inset, such as the footer's Stop and Take Control.
    case symbolPill
    /// A circle the capsule's height across, for a button whose label is a symbol alone.
    case circle
  }

  /// How the capsule is painted.
  var look = Look.primary
  /// How large the capsule is.
  var size = Size.regular
  /// Whether it takes the whole width offered, so side-by-side buttons share a row.
  var fill = false
  /// The shape around the label.
  var outline = Outline.pill
  /// Whether what it toggles is on, which a secondary button shows with the held fill.
  var isOn = false
  /// Whether its action runs now: it cannot be pressed, yet stays whole rather than dimming, so the spinner
  /// it holds reads as work, not as a button turned off.
  var isWorking = false

  /// Draws the label on its capsule or circle.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    CapsuleButtonBody(configuration: configuration, style: self)
  }

  /// The capsule's fill: a secondary one steps 0.08 → 0.12 → 0.16 at rest, under the pointer and while held, and
  /// shows the held fill while what it toggles is on; a glyph one has none at rest, the secondary fill under the
  /// pointer and the held fill while held or on; one over a picture steps from its dark fill a shade lighter at a
  /// time; the white one goes pure white under the pointer and
  /// #E3E3E8 while held; an alert's tinted ones brighten under the pointer and while held, and its accent one
  /// brightens under the pointer and darkens while held. The other looks keep one fill. A disabled button
  /// keeps its resting fill.
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
    case .secondary:
      if isOn || pressed { return .fillControlOn }
      return hovered ? .fillControlHover : .fillControl
    case .pressed: return .fillControlOn
    case .tinted, .destructive: return hovered || pressed ? .fillControlOn : .fillControlRaised
    case .plain: return .clear
    case .glyph:
      if isOn || pressed { return .fillControlOn }
      return hovered ? .fillControl : .clear
    case .accent:
      if pressed { return .accentPressed }
      return hovered ? .accentHover : .accent
    case .overPicture:
      if isOn || pressed { return .fillOverPictureOn }
      return hovered ? .fillOverPictureHover : .fillOverPicture
    }
  }

  /// How opaque the button is: whole, or dimmed to the disabled opacity when it cannot be pressed, unless its own
  /// action is what runs.
  ///
  /// - Parameters:
  ///   - isEnabled: Whether it can be pressed.
  ///   - isWorking: Whether its action runs now.
  /// - Returns: The opacity.
  nonisolated static func opacity(isEnabled: Bool, isWorking: Bool = false) -> Double {
    isEnabled || isWorking ? 1 : Opacity.disabled
  }

  /// How a press changes the fill: at once on mouse-down, so the answer is immediate, and back over
  /// ``Motion/pressRelease`` once let go.
  ///
  /// - Parameter isPressed: Whether it is held down now.
  /// - Returns: The animation for the change, or nil for none.
  nonisolated static func pressAnimation(isPressed: Bool) -> Animation? {
    isPressed ? nil : Motion.pressRelease
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
        Text(verbatim: "Secondary")
          .tag(CapsuleButtonStyle.Look.secondary)
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
        Text(verbatim: "Glyph")
          .tag(CapsuleButtonStyle.Look.glyph)
        Text(verbatim: "Over a picture")
          .tag(CapsuleButtonStyle.Look.overPicture)
      }
      Picker(String("Size"), selection: $size) {
        Text(verbatim: "Mini")
          .tag(CapsuleButtonStyle.Size.mini)
        Text(verbatim: "Compact")
          .tag(CapsuleButtonStyle.Size.compact)
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
          .buttonStyle(.capsule(.secondary, size: size, outline: .circle))
      }
      .disabled(!isEnabled)
    }
  }
#endif
