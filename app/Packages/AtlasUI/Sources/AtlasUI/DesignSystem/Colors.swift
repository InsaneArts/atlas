import SwiftUI

/// Atlas's colours, as code literals: the app ships no resource bundle, so there is no asset catalog to hold them.
/// One dark ground, quiet fills, white for the one thing to do next, and system blue for what is happening now.
/// The window backgrounds read ``ground`` through `NSColor.ground`.
extension Color {
  /// opencode's graphite card, at its top edge.
  static let brandOpencodeTop = Color(red: 0x2E / 255, green: 0x2A / 255, blue: 0x2A / 255)
  /// opencode's graphite card, at its bottom edge.
  static let brandOpencodeBottom = Color(red: 0x13 / 255, green: 0x10 / 255, blue: 0x10 / 255)
  /// The grey square inside opencode's original frame.
  static let brandOpencodeInner = Color(red: 0x5A / 255, green: 0x58 / 255, blue: 0x58 / 255)
  /// The white silhouette on a dark brand card.
  static let brandWhite = Color.white
  /// Pi's graphite card, at its top edge.
  static let brandPiTop = Color(red: 0x2B / 255, green: 0x2B / 255, blue: 0x2E / 255)
  /// Pi's graphite card, at its bottom edge.
  static let brandPiBottom = Color(red: 0x11 / 255, green: 0x11 / 255, blue: 0x13 / 255)
  /// The coral part of Pi's original mark.
  static let brandPiCoral = Color(red: 0xF0 / 255, green: 0x90 / 255, blue: 0x82 / 255)
  /// The blue part of Pi's original mark.
  static let brandPiBlue = Color(red: 0x4D / 255, green: 0x9A / 255, blue: 0xBF / 255)
  /// The gold part of Pi's original mark.
  static let brandPiGold = Color(red: 0xF1 / 255, green: 0xBE / 255, blue: 0x58 / 255)
  /// The light across a dark brand card's top.
  static let brandCardHighlight = Color.white
    .opacity(0.16)
  /// The inner light rim of a dark brand card.
  static let brandCardRim = Color.white
    .opacity(0.16)
  /// The dark outside edge of a brand card.
  static let brandCardEdge = Color.black
    .opacity(0.35)
  /// The soft shade under a brand card.
  static let brandCardShadow = Color.black
    .opacity(0.18)
  /// The ground every window draws on: #161618.
  public static let ground = Color(red: 0x16 / 255, green: 0x16 / 255, blue: 0x18 / 255)
  /// Titles and values: #F5F5F7.
  static let textPrimary = Color(red: 0xF5 / 255, green: 0xF5 / 255, blue: 0xF7 / 255)
  /// Details under a title, ledes and labels: #A1A1A6.
  static let textSecondary = Color(red: 0xA1 / 255, green: 0xA1 / 255, blue: 0xA6 / 255)
  /// Small print, hints and times: #8E8E93.
  static let textTertiary = Color(red: 0x8E / 255, green: 0x8E / 255, blue: 0x93 / 255)
  /// What an empty field shows until something is typed, such as Required: white at 0.28.
  static let textPlaceholder = Color.white
    .opacity(0.28)
  /// What is happening now, and the switches: system blue, #0A84FF.
  static let accent = Color(red: 0x0A / 255, green: 0x84 / 255, blue: 0xFF / 255)
  /// The accent under the pointer, a step brighter, such as an alert's default button:
  /// #1A8CFF.
  static let accentHover = Color(red: 0x1A / 255, green: 0x8C / 255, blue: 0xFF / 255)
  /// The accent while it is held down, a tenth darker, as AppKit's default button darkens on mouse-down: #0977E6.
  static let accentPressed = Color(red: 0x09 / 255, green: 0x77 / 255, blue: 0xE6 / 255)
  /// Text and glyphs on ``accent``: white.
  static let onAccent = Color.white
  /// The accent laid under the current step's row, as a wash: the accent at 0.09.
  static let accentWash = Color.accent
    .opacity(0.09)
  /// The accent tinting a bar while the person holds control: the accent at 0.07.
  static let accentTint = Color.accent
    .opacity(0.07)
  /// A ring in the accent: a rim, a focus ring or a selection's edge: the accent at 0.20.
  static let accentRing = Color.accent
    .opacity(0.2)
  /// A tile behind the person's own symbol, such as their line's in Activity, beside ``attentionFill``: the
  /// accent at 0.16.
  static let accentFill = Color.accent
    .opacity(0.16)
  /// The fine edge of a focus ring, inside its ``accentRing`` halo, such as a focused field's: the accent at
  /// 0.7.
  static let strokeFocus = Color.accent
    .opacity(0.7)
  /// Done and ready: system green, #30D158.
  static let success = Color(red: 0x30 / 255, green: 0xD1 / 255, blue: 0x58 / 255)
  /// Text that says something is done, such as "Allowed": #32D74B.
  static let textSuccess = Color(red: 0x32 / 255, green: 0xD7 / 255, blue: 0x4B / 255)
  /// Glyphs on ``success``, such as a check: #0B2410.
  static let onSuccess = Color(red: 0x0B / 255, green: 0x24 / 255, blue: 0x10 / 255)
  /// What needs the person: system orange, #FF9F0A.
  static let attention = Color(red: 0xFF / 255, green: 0x9F / 255, blue: 0x0A / 255)
  /// The attention colour laid under a flagged field, as a wash: attention at 0.09.
  static let attentionWash = Color.attention
    .opacity(0.09)
  /// The disc behind a problem's symbol, such as a problem hero's: attention at 0.16.
  static let attentionFill = Color.attention
    .opacity(0.16)
  /// Glyphs on ``attention``, such as the "!" on the panel picture's badge: #1C1203.
  static let onAttention = Color(red: 0x1C / 255, green: 0x12 / 255, blue: 0x03 / 255)
  /// A word that needs the person, such as Needs you: #FFB340.
  static let textAttention = Color(red: 0xFF / 255, green: 0xB3 / 255, blue: 0x40 / 255)
  /// A link, and the word You have control: #4EA6FF.
  static let textLink = Color(red: 0x4E / 255, green: 0xA6 / 255, blue: 0xFF / 255)
  /// Secondary text on a glass surface, such as an alert's message under its title: rgb(235, 235, 245) at 0.7.
  static let textOnGlass = Color(red: 235 / 255, green: 235 / 255, blue: 245 / 255)
    .opacity(0.7)
  /// A destructive button's word, such as Log Out: #FF6961.
  static let textDestructive = Color(red: 0xFF / 255, green: 0x69 / 255, blue: 0x61 / 255)
  /// A quiet card's ground, a step under ``fillCard``, such as the note on why setup asks for administrator
  /// access: white at 0.03.
  static let fillQuiet = Color.white
    .opacity(0.03)
  /// A card's ground: white at 0.045.
  static let fillCard = Color.white
    .opacity(0.045)
  /// A grouped list's ground (a step over ``fillCard``): white at 0.05.
  static let fillGroup = Color.white
    .opacity(0.05)
  /// A row under the pointer: white at 0.035.
  static let fillRowHover = Color.white
    .opacity(0.035)
  /// A section's header pinned over a list scrolling under it, such as Activity's: rgb(28, 28, 31) at 0.9.
  static let fillPinned = Color(red: 28 / 255, green: 28 / 255, blue: 31 / 255)
    .opacity(0.9)
  /// A control's fill, such as a glass capsule, a symbol tile or a hovered menu row: white at 0.08.
  static let fillControl = Color.white
    .opacity(0.08)
  /// A control's fill under the pointer, the step between ``fillControl`` and the pressed ``fillControlOn``:
  /// white at 0.12.
  static let fillControlHover = Color.white
    .opacity(0.12)
  /// A control's fill while what it toggles is on, such as a round button's: white at 0.16.
  static let fillControlOn = Color.white
    .opacity(0.16)
  /// A control's fill raised a step, for a control on a lighter ground, such as an alert's Cancel and destructive
  /// buttons, and a selected icon tab's fill: white at 0.10.
  static let fillControlRaised = Color.white
    .opacity(0.1)
  /// The white capsule under the pointer, brighter than its #F5F5F7: white.
  static let fillPrimaryHover = Color.white
  /// The white capsule while it is held: #E3E3E8.
  static let fillPrimaryPressed = Color(red: 0xE3 / 255, green: 0xE3 / 255, blue: 0xE8 / 255)
  /// A pill's ground, such as a status badge's: white at 0.06.
  static let fillPill = Color.white
    .opacity(0.06)
  /// An account's avatar disc: an opaque grey, 0.23 of the way from black to white.
  static let fillAvatar = Color(white: 0.23)
  /// A step still to come on a progress track: white at 0.14.
  static let fillTrack = Color.white
    .opacity(0.14)
  /// A finished step on a progress track, light grey: white at 0.42.
  static let fillTrackDone = Color.white
    .opacity(0.42)
  /// A chip floating over a picture, under its material: the ground at 0.75.
  static let fillChip = Color.ground
    .opacity(0.75)
  /// The shade laid over the whole window behind an alert: black at 0.5, so what lies behind recedes rather than
  /// competing with the alert's words.
  static let scrim = Color.black
    .opacity(0.5)
  /// The fill of the hand carrying the drag ghost, dark under its light edge as the pointer is, so it shows over light
  /// and dark lists alike: black at 0.42. Its own, so a darker alert scrim never darkens the hand.
  static let fillHand = Color.black
    .opacity(0.42)
  /// The letterbox around a picture: black.
  static let letterbox = Color.black
  /// Where a picture goes while there is none to show: #0D0D10.
  static let fillPlaceholder = Color(red: 0x0D / 255, green: 0x0D / 255, blue: 0x10 / 255)
  /// A modal surface's ground drawn opaque, under Reduce Transparency, such as an alert's: rgb(44, 44, 48).
  static let fillModalSolid = Color(red: 44 / 255, green: 44 / 255, blue: 48 / 255)
  /// A floating bar of buttons drawn opaque, under Reduce Transparency: rgb(40, 40, 44).
  static let fillFloatingSolid = Color(red: 40 / 255, green: 40 / 255, blue: 44 / 255)
  /// The tint over a regular glass's material before macOS 26, such as a chip's or the tab track's: the
  /// ground at 0.66.
  static let fillGlass = Color.ground
    .opacity(0.66)
  /// The tint of a modal surface, such as an alert's, over its material: rgb(38, 38, 42) at 0.94, near opaque, as a
  /// system alert's is, so nothing behind it reads through its paragraphs.
  static let fillGlassThick = Color(red: 38 / 255, green: 38 / 255, blue: 42 / 255)
    .opacity(0.94)
  /// A divider and a card's edge: white at 0.08.
  static let separator = Color.white
    .opacity(0.08)
  /// The rail joining a list's steps: white at 0.10.
  static let strokeRail = Color.white
    .opacity(0.1)
  /// The rail below a finished step: system green at 0.45.
  static let strokeRailDone = Color.success
    .opacity(0.45)
  /// A glass edge: white at 0.16.
  static let strokeStrong = Color.white
    .opacity(0.16)
  /// The 0.5-point highlight along a glass surface's top edge, such as the panel's: white at 0.12.
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
  /// The ring a thumbnail shows under the pointer, in place of lifting: white at 0.30.
  static let strokeHover = Color.white
    .opacity(0.3)
  /// The shade along a keycap's bottom edge, so it reads as a key: black at 0.3.
  static let strokeShade = Color.black
    .opacity(0.3)
  /// The faint shadow outside a chip on the picture: black at 0.25.
  static let dropShadowFaint = Color.black
    .opacity(0.25)
  /// A soft shadow under a surface resting on the ground, such as the live picture's: black at 0.4.
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
