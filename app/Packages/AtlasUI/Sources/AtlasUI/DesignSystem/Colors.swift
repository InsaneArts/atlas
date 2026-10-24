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
  /// The accent under the pointer, a step brighter, such as an alert's default button (v3 `.acap.accent:hover`):
  /// #1A8CFF.
  static let accentHover = Color(red: 0x1A / 255, green: 0x8C / 255, blue: 0xFF / 255)
  /// Text and glyphs on ``accent``: white.
  static let onAccent = Color.white
  /// The accent laid under the current step's row, as a wash (v3 `--accent-wash`): the accent at 0.09.
  static let accentWash = Color.accent
    .opacity(0.09)
  /// The accent tinting a bar while the person holds control (v3 `accentTop`, named for where it sat): the accent
  /// at 0.07.
  static let accentTint = Color.accent
    .opacity(0.07)
  /// A ring in the accent: a rim, a focus ring or a selection's edge (v3 `--accent-ring`): the accent at 0.20.
  static let accentRing = Color.accent
    .opacity(0.2)
  /// Done and ready: system green, #30D158.
  static let success = Color(red: 0x30 / 255, green: 0xD1 / 255, blue: 0x58 / 255)
  /// Text that says something is done, such as "Allowed": #32D74B.
  static let textSuccess = Color(red: 0x32 / 255, green: 0xD7 / 255, blue: 0x4B / 255)
  /// Glyphs on ``success``, such as a check: #0B2410.
  static let onSuccess = Color(red: 0x0B / 255, green: 0x24 / 255, blue: 0x10 / 255)
  /// What needs the person: system orange, #FF9F0A.
  static let attention = Color(red: 0xFF / 255, green: 0x9F / 255, blue: 0x0A / 255)
  /// The attention colour laid under a flagged field, as a wash (v3 `--attention-wash`): attention at 0.09.
  static let attentionWash = Color.attention
    .opacity(0.09)
  /// The disc behind a problem's symbol, such as a problem hero's (v3 `attentionDisc`, named for its shape):
  /// attention at 0.16.
  static let attentionFill = Color.attention
    .opacity(0.16)
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
  /// A grouped list's ground (v3 `--fill-group`, a step over ``fillCard``): white at 0.05.
  static let fillGroup = Color.white
    .opacity(0.05)
  /// A row under the pointer (v3 `--fill-row-hover`): white at 0.035.
  static let fillRowHover = Color.white
    .opacity(0.035)
  /// A control's fill, such as a glass capsule, a symbol tile or a hovered menu row: white at 0.08.
  static let fillControl = Color.white
    .opacity(0.08)
  /// A control's fill under the pointer, the step between ``fillControl`` and the pressed ``fillControlOn``
  /// (v3 `--fill-control-hover`, R2's 8 → 12 → 16): white at 0.12.
  static let fillControlHover = Color.white
    .opacity(0.12)
  /// A control's fill while what it toggles is on, such as a round button's: white at 0.16.
  static let fillControlOn = Color.white
    .opacity(0.16)
  /// A control's fill raised a step, for a control on a lighter ground, such as an alert's Cancel and destructive
  /// buttons, and a selected icon tab's fill: white at 0.10.
  static let fillControlRaised = Color.white
    .opacity(0.1)
  /// The white capsule under the pointer, brighter than its #F5F5F7 (v3 `--primary-hover`, P9): white.
  static let fillPrimaryHover = Color.white
  /// The white capsule while it is held (v3 `--primary-pressed`, P9): #E3E3E8.
  static let fillPrimaryPressed = Color(red: 0xE3 / 255, green: 0xE3 / 255, blue: 0xE8 / 255)
  /// A pill's ground, such as a status badge's: white at 0.06.
  static let fillPill = Color.white
    .opacity(0.06)
  /// An account's avatar disc: an opaque grey, 0.23 of the way from black to white.
  static let fillAvatar = Color(white: 0.23)
  /// An inactive page dot: white at 0.18.
  static let fillIndicator = Color.white
    .opacity(0.18)
  /// A step still to come on a progress track (v3 D8 `.track i`): white at 0.14.
  static let fillTrack = Color.white
    .opacity(0.14)
  /// A finished step on a progress track, light grey (v3 D8, S10 `.track i.done`): white at 0.42.
  static let fillTrackDone = Color.white
    .opacity(0.42)
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
  /// A modal surface's ground drawn opaque, under Reduce Transparency, such as an alert's: rgb(44, 44, 48).
  static let fillModalSolid = Color(red: 44 / 255, green: 44 / 255, blue: 48 / 255)
  /// A modal surface's ground over its material, such as an alert's: rgb(44, 44, 48) at 0.95.
  static let fillModal = Color.fillModalSolid
    .opacity(0.95)
  /// A floating bar of buttons drawn opaque, under Reduce Transparency: rgb(40, 40, 44).
  static let fillFloatingSolid = Color(red: 40 / 255, green: 40 / 255, blue: 44 / 255)
  /// A floating bar of buttons over its material, such as a large screenshot's: rgb(40, 40, 44) at 0.92.
  static let fillFloating = Color.fillFloatingSolid
    .opacity(0.92)
  /// The tint over a regular glass's material before macOS 26, such as a chip's or the tab track's (v3
  /// `--glass-chip`, D12): the ground at 0.66.
  static let fillGlass = Color.ground
    .opacity(0.66)
  /// The tint of a thick glass, such as an alert's, between the system glass or the material and its content (v3
  /// `--glass-thick`, A1): rgb(38, 38, 42) at 0.78.
  static let fillGlassThick = Color(red: 38 / 255, green: 38 / 255, blue: 42 / 255)
    .opacity(0.78)
  /// A divider and a card's edge: white at 0.08.
  static let separator = Color.white
    .opacity(0.08)
  /// A glass edge: white at 0.16.
  static let strokeStrong = Color.white
    .opacity(0.16)
  /// The 0.5-point highlight along a glass surface's top edge, such as the panel's (v3 `.panel2`'s inset; R3):
  /// white at 0.12.
  static let strokeHighlight = Color.white
    .opacity(0.12)
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
  /// The shade along a keycap's bottom edge, so it reads as a key (v3 V5 `.kc`'s inset): black at 0.3.
  static let strokeShade = Color.black
    .opacity(0.3)
  /// The faint shadow outside a chip on the picture (v3 `.gchip`'s `0 4 14`): black at 0.25.
  static let dropShadowFaint = Color.black
    .opacity(0.25)
  /// A soft shadow under a surface resting on the ground, such as the live picture's (v3 `Elevation.stage`, R3):
  /// black at 0.4.
  static let dropShadowSoft = Color.black
    .opacity(0.4)
  /// A surface's shadow, such as a raised picture's, a floating bar's or an alert's: black at 0.5.
  static let dropShadow = Color.black
    .opacity(0.5)
  /// A heavier shadow, for a surface over other content, such as the mini screen's or a large screenshot's:
  /// black at 0.55.
  static let dropShadowStrong = Color.black
    .opacity(0.55)
}
