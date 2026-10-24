import SwiftUI

/// Atlas's colours, as code literals: the app ships no resource bundle, so there is no asset catalog to hold them.
/// One dark ground, quiet fills, white for the one thing to do next, and system blue for what is happening now.
/// The window backgrounds read ``ground`` through `NSColor.ground`.
extension Color {
  /// The ground every window draws on: #161618.
  public static let ground = Color(red: 0x16 / 255, green: 0x16 / 255, blue: 0x18 / 255)
  /// Titles and values: #F5F5F7.
  static let textPrimary = Color(red: 0xF5 / 255, green: 0xF5 / 255, blue: 0xF7 / 255)
  /// Details under a title, ledes and labels: #A1A1A6.
  static let textSecondary = Color(red: 0xA1 / 255, green: 0xA1 / 255, blue: 0xA6 / 255)
  /// Small print, hints and times: #8E8E93.
  static let textTertiary = Color(red: 0x8E / 255, green: 0x8E / 255, blue: 0x93 / 255)
  /// What is happening now, and the switches: system blue, #0A84FF.
  static let accent = Color(red: 0x0A / 255, green: 0x84 / 255, blue: 0xFF / 255)
  /// Text and glyphs on ``accent``: white.
  static let onAccent = Color.white
  /// Done and ready: system green, #30D158.
  static let success = Color(red: 0x30 / 255, green: 0xD1 / 255, blue: 0x58 / 255)
  /// Text that says something is done, such as "Allowed": #32D74B.
  static let textSuccess = Color(red: 0x32 / 255, green: 0xD7 / 255, blue: 0x4B / 255)
  /// Glyphs on ``success``, such as a check: #0B2410.
  static let onSuccess = Color(red: 0x0B / 255, green: 0x24 / 255, blue: 0x10 / 255)
  /// What needs the person: system orange, #FF9F0A.
  static let attention = Color(red: 0xFF / 255, green: 0x9F / 255, blue: 0x0A / 255)
  /// A word that needs the person, such as Needs you: #FFB340.
  static let textAttention = Color(red: 0xFF / 255, green: 0xB3 / 255, blue: 0x40 / 255)
  /// A link, and the word You have control: #4EA6FF.
  static let textLink = Color(red: 0x4E / 255, green: 0xA6 / 255, blue: 0xFF / 255)
  /// Secondary text on a glass surface, such as an alert's message under its title: rgb(235, 235, 245) at 0.7.
  static let textOnGlass = Color(red: 235 / 255, green: 235 / 255, blue: 245 / 255)
    .opacity(0.7)
  /// A destructive button's word, such as Log Out: #FF6961.
  static let textDestructive = Color(red: 0xFF / 255, green: 0x69 / 255, blue: 0x61 / 255)
  /// A card's ground: white at 0.045.
  static let fillCard = Color.white
    .opacity(0.045)
  /// A control's fill, such as a glass capsule, a symbol tile or a hovered menu row: white at 0.08.
  static let fillControl = Color.white
    .opacity(0.08)
  /// A control's fill while what it toggles is on, such as a round button's: white at 0.16.
  static let fillControlOn = Color.white
    .opacity(0.16)
  /// A control's fill raised a step, for a control on a lighter ground, such as an alert's Cancel and destructive
  /// buttons, and a selected icon tab's fill: white at 0.10.
  static let fillControlRaised = Color.white
    .opacity(0.1)
  /// A pill's ground, such as a status badge's: white at 0.06.
  static let fillPill = Color.white
    .opacity(0.06)
  /// An account's avatar disc: an opaque grey, 0.23 of the way from black to white.
  static let fillAvatar = Color(white: 0.23)
  /// An inactive page dot: white at 0.18.
  static let fillIndicator = Color.white
    .opacity(0.18)
  /// A chip floating over a picture, under its material: the ground at 0.75.
  static let fillChip = Color.ground
    .opacity(0.75)
  /// The shade laid over the whole window behind an alert: black at 0.42.
  static let scrim = Color.black
    .opacity(0.42)
  /// The letterbox around a picture: black.
  static let letterbox = Color.black
  /// Where a picture goes while there is none to show: #0D0D10.
  static let fillPlaceholder = Color(red: 0x0D / 255, green: 0x0D / 255, blue: 0x10 / 255)
  /// An overlay's ground drawn opaque, under Reduce Transparency, such as an alert's: rgb(44, 44, 48).
  static let fillOverlaySolid = Color(red: 44 / 255, green: 44 / 255, blue: 48 / 255)
  /// An overlay's ground over its material, such as an alert's: rgb(44, 44, 48) at 0.95.
  static let fillOverlay = Color.fillOverlaySolid
    .opacity(0.95)
  /// A floating bar of buttons drawn opaque, under Reduce Transparency: rgb(40, 40, 44).
  static let fillFloatingSolid = Color(red: 40 / 255, green: 40 / 255, blue: 44 / 255)
  /// A floating bar of buttons over its material, such as a large screenshot's: rgb(40, 40, 44) at 0.92.
  static let fillFloating = Color.fillFloatingSolid
    .opacity(0.92)
  /// A divider and a card's edge: white at 0.08.
  static let separator = Color.white
    .opacity(0.08)
  /// A glass edge: white at 0.16.
  static let strokeStrong = Color.white
    .opacity(0.16)
  /// A selected tab's edge: white at 0.22.
  static let strokeSelected = Color.white
    .opacity(0.22)
  /// A pressed capsule's edge, such as Keys while in control, and a quiet mark, such as an agent's dot in Activity:
  /// white at 0.28.
  static let strokePressed = Color.white
    .opacity(0.28)
  /// An unselected radio's ring: white at 0.30.
  static let strokeControl = Color.white
    .opacity(0.3)
  /// A surface's shadow, such as a raised picture's or a floating bar's: black at 0.5.
  static let dropShadow = Color.black
    .opacity(0.5)
  /// A heavier shadow, for a surface over other content, such as the mini screen's or an alert's: black at 0.55.
  static let dropShadowStrong = Color.black
    .opacity(0.55)
}
