import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("Appearance")
@MainActor
struct AppearanceTests {
  /// The tokens drawn on a window, each of which must change with the Mac's appearance.
  private static let adaptive: [(name: String, colour: Color)] = [
    ("ground", .ground), ("textPrimary", .textPrimary), ("textSecondary", .textSecondary),
    ("textTertiary", .textTertiary), ("textPlaceholder", .textPlaceholder), ("accent", .accent),
    ("accentHover", .accentHover), ("accentPressed", .accentPressed), ("accentWash", .accentWash),
    ("success", .success), ("textSuccess", .textSuccess), ("attention", .attention),
    ("textAttention", .textAttention), ("textLink", .textLink), ("textOnGlass", .textOnGlass),
    ("textDestructive", .textDestructive), ("fillQuiet", .fillQuiet), ("fillCard", .fillCard),
    ("fillGroup", .fillGroup), ("fillRowHover", .fillRowHover), ("fillPinned", .fillPinned),
    ("fillControl", .fillControl), ("fillControlHover", .fillControlHover), ("fillControlOn", .fillControlOn),
    ("fillControlRaised", .fillControlRaised), ("fillPrimaryHover", .fillPrimaryHover),
    ("fillPrimaryPressed", .fillPrimaryPressed), ("fillPill", .fillPill), ("fillAvatar", .fillAvatar),
    ("fillTrack", .fillTrack), ("fillTrackDone", .fillTrackDone), ("fillChip", .fillChip), ("scrim", .scrim),
    ("fillPlaceholder", .fillPlaceholder), ("fillModalSolid", .fillModalSolid),
    ("fillFloatingSolid", .fillFloatingSolid), ("fillGlass", .fillGlass), ("fillGlassThick", .fillGlassThick),
    ("separator", .separator), ("strokeRail", .strokeRail), ("strokeStrong", .strokeStrong),
    ("strokeHighlight", .strokeHighlight), ("strokeSelected", .strokeSelected), ("strokePressed", .strokePressed),
    ("strokeControl", .strokeControl), ("strokeHover", .strokeHover), ("strokeShade", .strokeShade),
    ("dropShadowFaint", .dropShadowFaint), ("dropShadowSoft", .dropShadowSoft), ("dropShadow", .dropShadow),
    ("dropShadowStrong", .dropShadowStrong), ("panelGround", PanelLayout.ground),
  ]

  /// The tokens that are the same in either appearance: the brands', the black around the helper's screen, the
  /// hand over it and the inks on the status colours.
  private static let fixed: [(name: String, colour: Color)] = [
    ("brandOpencodeTop", .brandOpencodeTop), ("brandPiCoral", .brandPiCoral), ("brandWhite", .brandWhite),
    ("letterbox", .letterbox), ("fillHand", .fillHand), ("onAccent", .onAccent), ("onSuccess", .onSuccess),
    ("onAttention", .onAttention),
  ]

  /// The text tokens, which must read at WCAG AA, 4.5 to 1, on every ground they sit on.
  private static let texts: [(name: String, colour: Color)] = [
    ("textPrimary", .textPrimary), ("textSecondary", .textSecondary), ("textTertiary", .textTertiary),
    ("textLink", .textLink), ("textAttention", .textAttention), ("textSuccess", .textSuccess),
    ("textDestructive", .textDestructive), ("textOnGlass", .textOnGlass),
  ]

  /// The grounds text sits on, each laid over the window's ground: the window, a card, a grouped list, the panel
  /// and an alert.
  private static let grounds: [(name: String, colour: Color)] = [
    ("ground", .ground), ("fillCard", .fillCard), ("fillGroup", .fillGroup), ("panelGround", PanelLayout.ground),
    ("fillGlassThick", .fillGlassThick),
  ]

  @Test("every token on a window resolves to a different colour in the light and the dark")
  func windowTokensFollowTheAppearance() {
    for (name, colour) in Self.adaptive {
      #expect(colour.resolve(in: .light) != colour.resolve(in: .dark), "\(name)")
    }
  }

  @Test("the brands, the black round the screen and the inks on status colours stay the same in either appearance")
  func fixedTokensStayTheSame() {
    for (name, colour) in Self.fixed {
      #expect(colour.resolve(in: .light) == colour.resolve(in: .dark), "\(name)")
    }
  }

  @Test("the window's ground is light under Light and dark under Dark")
  func theGroundFollowsTheAppearance() {
    #expect(Self.luminance(Color.ground.resolve(in: .light)) > 0.8)
    #expect(Self.luminance(Color.ground.resolve(in: .dark)) < 0.02)
  }

  @Test("every text token reads at WCAG AA on every ground it sits on, in both appearances")
  func textReadsInBothAppearances() {
    for environment in [EnvironmentValues.light, .dark] {
      let window = environment.colorScheme
      for (ground, fill) in Self.grounds {
        let under = Self.over(fill.resolve(in: environment), Color.ground.resolve(in: environment))
        for (text, colour) in Self.texts {
          let ratio = Self.contrast(Self.over(colour.resolve(in: environment), under), under)
          #expect(ratio >= 4.5, "\(text) on \(ground) in \(window): \(ratio)")
        }
      }
    }
  }

  @Test("the main capsule's word reads at WCAG AA on its fill in both appearances")
  func theMainCapsuleReads() {
    for environment in [EnvironmentValues.light, .dark] {
      let fill = Color.textPrimary
        .resolve(in: environment)
      let word = Color.ground
        .resolve(in: environment)
      #expect(Self.contrast(word, fill) >= 4.5, "\(environment.colorScheme)")
    }
  }

  /// `top` laid over the opaque `bottom`, by its opacity.
  ///
  /// - Parameters:
  ///   - top: The colour on top.
  ///   - bottom: The opaque colour under it.
  /// - Returns: The opaque mix.
  private static func over(_ top: Color.Resolved, _ bottom: Color.Resolved) -> Color.Resolved {
    let alpha = top.opacity
    return Color.Resolved(
      colorSpace: .sRGB,
      red: top.red * alpha + bottom.red * (1 - alpha),
      green: top.green * alpha + bottom.green * (1 - alpha),
      blue: top.blue * alpha + bottom.blue * (1 - alpha)
    )
  }

  /// A colour's relative luminance, as WCAG defines it.
  ///
  /// - Parameter colour: An opaque colour.
  /// - Returns: Its luminance, 0 for black to 1 for white.
  private static func luminance(_ colour: Color.Resolved) -> Double {
    func linear(_ channel: Float) -> Double {
      let value = Double(channel)
      return value <= 0.039_28 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
    }
    return 0.2126 * linear(colour.red) + 0.7152 * linear(colour.green) + 0.0722 * linear(colour.blue)
  }

  /// The contrast ratio between two opaque colours, as WCAG defines it.
  ///
  /// - Parameters:
  ///   - first: One colour.
  ///   - second: The other.
  /// - Returns: The ratio, 1 to 21.
  private static func contrast(_ first: Color.Resolved, _ second: Color.Resolved) -> Double {
    let lighter = max(luminance(first), luminance(second))
    let darker = min(luminance(first), luminance(second))
    return (lighter + 0.05) / (darker + 0.05)
  }
}
