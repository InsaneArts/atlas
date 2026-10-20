import SwiftUI

/// Atlas's colours, as code literals: the app ships no resource bundle, so there is no asset catalog to hold them.
/// One dark ground, quiet fills, white for the one thing to do next, and system blue for what is happening now.
/// The window backgrounds read ``ground`` through `NSColor.ground`.
extension Color {
  /// The ground every window draws on: #161618.
  package static let ground = Color(red: 0x16 / 255, green: 0x16 / 255, blue: 0x18 / 255)
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
  /// A card's ground: white at 0.045.
  static let fillCard = Color.white
    .opacity(0.045)
  /// A control's fill, such as a glass capsule, a symbol tile or a hovered menu row: white at 0.08.
  static let fillControl = Color.white
    .opacity(0.08)
  /// A control's fill while what it toggles is on, such as a round button's: white at 0.16.
  static let fillControlOn = Color.white
    .opacity(0.16)
  /// A pill's ground, such as a status badge's: white at 0.06.
  static let fillPill = Color.white
    .opacity(0.06)
  /// An account's avatar disc: an opaque grey, 0.23 of the way from black to white.
  static let fillAvatar = Color(white: 0.23)
  /// An inactive page dot: white at 0.18.
  static let fillIndicator = Color.white
    .opacity(0.18)
  /// A badge over a picture, such as Live: black at 0.55.
  static let fillBadge = Color.black
    .opacity(0.55)
  /// A chip floating over a picture, under its material: the ground at 0.75.
  static let fillChip = Color.ground
    .opacity(0.75)
  /// The shade laid over content behind an alert: the ground at 0.82.
  static let scrim = Color.ground
    .opacity(0.82)
  /// The letterbox around a picture: black.
  static let stageBackground = Color.black
  /// A divider and a card's edge: white at 0.08.
  static let separator = Color.white
    .opacity(0.08)
  /// A glass edge: white at 0.16.
  static let strokeStrong = Color.white
    .opacity(0.16)
  /// An unselected radio's ring: white at 0.30.
  static let strokeControl = Color.white
    .opacity(0.3)
}
