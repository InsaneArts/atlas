import SwiftUI

/// Atlas's colours, as code literals: the app ships no resource bundle, so there is no asset catalog to hold them.
/// Each token that sits on a window follows the Mac's appearance (``Color/init(light:dark:)``): a light ground with
/// white cards and near-black words under Light, one dark ground with quiet fills under Dark, the main capsule in the
/// ground's opposite, and system blue for what is happening now in both. The brands' colours, the black around the
/// helper's screen and the hand over it are the same in either. The window backgrounds read ``ground`` through
/// `NSColor.ground`. Each token's documentation gives its light value, then its dark one.
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
  /// The terminal prompt's graphite card, at its top edge.
  static let brandTerminalTop = Color(red: 0x3A / 255, green: 0x3A / 255, blue: 0x3C / 255)
  /// The terminal prompt's graphite card, at its bottom edge.
  static let brandTerminalBottom = Color(red: 0x1C / 255, green: 0x1C / 255, blue: 0x1E / 255)
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
  /// The ground every window draws on: #F5F5F7, or #161618.
  public static let ground = Color(light: .rgb(0xF5F5F7), dark: .rgb(0x161618))
  /// Titles and values: #1D1D1F, or #F5F5F7.
  static let textPrimary = Color(light: .rgb(0x1D1D1F), dark: .rgb(0xF5F5F7))
  /// Details under a title, ledes and labels: #515154, or #A1A1A6.
  static let textSecondary = Color(light: .rgb(0x515154), dark: .rgb(0xA1A1A6))
  /// Small print, hints and times: #636366, or #8E8E93.
  static let textTertiary = Color(light: .rgb(0x636366), dark: .rgb(0x8E8E93))
  /// What an empty field shows until something is typed, such as Required: black at 0.30, or white at 0.28.
  static let textPlaceholder = Color(light: .black(0.3), dark: .white(0.28))
  /// What is happening now, and the switches: system blue, #0071E3, or #0A84FF.
  static let accent = Color(light: .rgb(0x0071E3), dark: .rgb(0x0A84FF))
  /// The accent under the pointer, a step brighter, such as an alert's default button: #0077ED, or #1A8CFF.
  static let accentHover = Color(light: .rgb(0x0077ED), dark: .rgb(0x1A8CFF))
  /// The accent while it is held down, a tenth darker, as AppKit's default button darkens on mouse-down: #0066CC,
  /// or #0977E6.
  static let accentPressed = Color(light: .rgb(0x0066CC), dark: .rgb(0x0977E6))
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
  /// Done and ready: system green, #34C759, or #30D158.
  static let success = Color(light: .rgb(0x34C759), dark: .rgb(0x30D158))
  /// Text that says something is done, such as "Allowed": #1F7A36, or #32D74B.
  static let textSuccess = Color(light: .rgb(0x1F7A36), dark: .rgb(0x32D74B))
  /// Glyphs on ``success``, such as a check: #0B2410.
  static let onSuccess = Color(red: 0x0B / 255, green: 0x24 / 255, blue: 0x10 / 255)
  /// What needs the person: system orange, #FF9500, or #FF9F0A.
  static let attention = Color(light: .rgb(0xFF9500), dark: .rgb(0xFF9F0A))
  /// The attention colour laid under a flagged field, as a wash: attention at 0.09.
  static let attentionWash = Color.attention
    .opacity(0.09)
  /// The disc behind a problem's symbol, such as a problem hero's: attention at 0.16.
  static let attentionFill = Color.attention
    .opacity(0.16)
  /// Glyphs on ``attention``, such as the "!" on the panel picture's badge: #1C1203.
  static let onAttention = Color(red: 0x1C / 255, green: 0x12 / 255, blue: 0x03 / 255)
  /// A word that needs the person, such as Needs you: #A84B00, or #FFB340.
  static let textAttention = Color(light: .rgb(0xA84B00), dark: .rgb(0xFFB340))
  /// A link, and the word You have control: #0066CC, or #4EA6FF.
  static let textLink = Color(light: .rgb(0x0066CC), dark: .rgb(0x4EA6FF))
  /// Secondary text on a glass surface, such as an alert's message under its title: #515154, or rgb(235, 235, 245)
  /// at 0.7.
  static let textOnGlass = Color(light: .rgb(0x515154), dark: .rgb(0xEBEBF5, alpha: 0.7))
  /// A destructive button's word, such as Log Out: #C00013, or #FF6961.
  static let textDestructive = Color(light: .rgb(0xC00013), dark: .rgb(0xFF6961))
  /// A quiet card's ground, a step under ``fillCard``, such as the note on why setup asks for administrator
  /// access: white at 0.6, or white at 0.03.
  static let fillQuiet = Color(light: .white(0.6), dark: .white(0.03))
  /// A card's ground: white, or white at 0.045.
  static let fillCard = Color(light: .white(1), dark: .white(0.045))
  /// A grouped list's ground (a step over ``fillCard``): white, or white at 0.05.
  static let fillGroup = Color(light: .white(1), dark: .white(0.05))
  /// A row under the pointer: black at 0.035, or white at 0.035.
  static let fillRowHover = Color(light: .black(0.035), dark: .white(0.035))
  /// A section's header pinned over a list scrolling under it, such as Activity's: white at 0.92, or rgb(28, 28,
  /// 31) at 0.9, each the card it scrolls on.
  static let fillPinned = Color(light: .white(0.92), dark: .rgb(0x1C1C1F, alpha: 0.9))
  /// A control's fill, such as a glass capsule, a symbol tile or a hovered menu row: black at 0.05, or white at
  /// 0.08.
  static let fillControl = Color(light: .black(0.05), dark: .white(0.08))
  /// A control's fill under the pointer, the step between ``fillControl`` and the pressed ``fillControlOn``:
  /// black at 0.08, or white at 0.12.
  static let fillControlHover = Color(light: .black(0.08), dark: .white(0.12))
  /// A control's fill while what it toggles is on, such as a round button's: black at 0.11, or white at 0.16.
  static let fillControlOn = Color(light: .black(0.11), dark: .white(0.16))
  /// A control's fill raised a step, for a control on a lighter ground, such as an alert's Cancel and destructive
  /// buttons, and a selected icon tab's fill: black at 0.06, or white at 0.10.
  static let fillControlRaised = Color(light: .black(0.06), dark: .white(0.1))
  /// The main capsule under the pointer, a step off its ``textPrimary``: #333336, or white.
  static let fillPrimaryHover = Color(light: .rgb(0x333336), dark: .white(1))
  /// The main capsule while it is held: black, or #E3E3E8.
  static let fillPrimaryPressed = Color(light: .black(1), dark: .rgb(0xE3E3E8))
  /// A pill's ground, such as a status badge's: black at 0.05, or white at 0.06.
  static let fillPill = Color(light: .black(0.05), dark: .white(0.06))
  /// An account's avatar disc, opaque: #D1D1D6, or #3B3B3B.
  static let fillAvatar = Color(light: .rgb(0xD1D1D6), dark: .rgb(0x3B3B3B))
  /// A step still to come on a progress track: black at 0.10, or white at 0.14.
  static let fillTrack = Color(light: .black(0.1), dark: .white(0.14))
  /// A finished step on a progress track, mid grey: black at 0.35, or white at 0.42.
  static let fillTrackDone = Color(light: .black(0.35), dark: .white(0.42))
  /// A chip floating over a picture, under its material: the ground at 0.75.
  static let fillChip = Color.ground
    .opacity(0.75)
  /// The shade laid over the whole window behind an alert, so what lies behind recedes rather than competing with
  /// the alert's words: black at 0.25, or black at 0.5.
  static let scrim = Color(light: .black(0.25), dark: .black(0.5))
  /// The fill of the hand carrying the drag ghost, dark under its light edge as the pointer is, so it shows over light
  /// and dark lists alike: black at 0.42. Its own, so a darker alert scrim never darkens the hand.
  static let fillHand = Color.black
    .opacity(0.42)
  /// The letterbox around a picture: black, as the helper's screen is framed in either appearance.
  static let letterbox = Color.black
  /// Where a picture or a list goes while there is none to show: #E8E8ED, or #0D0D10.
  static let fillPlaceholder = Color(light: .rgb(0xE8E8ED), dark: .rgb(0x0D0D10))
  /// A modal surface's ground drawn opaque, under Reduce Transparency, such as an alert's: #FAFAFC, or rgb(44, 44,
  /// 48).
  static let fillModalSolid = Color(light: .rgb(0xFAFAFC), dark: .rgb(0x2C2C30))
  /// A floating bar of buttons drawn opaque, under Reduce Transparency: white, or rgb(40, 40, 44).
  static let fillFloatingSolid = Color(light: .white(1), dark: .rgb(0x28282C))
  /// The tint over a regular glass's material before macOS 26, such as a chip's or the tab track's: #FAFAFC at
  /// 0.82, or the ground at 0.66.
  static let fillGlass = Color(light: .rgb(0xFAFAFC, alpha: 0.82), dark: .rgb(0x161618, alpha: 0.66))
  /// The tint of a modal surface, such as an alert's, over its material, near opaque, as a system alert's is, so
  /// nothing behind it reads through its paragraphs: #FAFAFC at 0.96, or rgb(38, 38, 42) at 0.94.
  static let fillGlassThick = Color(light: .rgb(0xFAFAFC, alpha: 0.96), dark: .rgb(0x26262A, alpha: 0.94))
  /// A divider and a card's edge: black at 0.09, or white at 0.08.
  static let separator = Color(light: .black(0.09), dark: .white(0.08))
  /// The rail joining a list's steps: black at 0.10, or white at 0.10.
  static let strokeRail = Color(light: .black(0.1), dark: .white(0.1))
  /// The rail below a finished step: system green at 0.45.
  static let strokeRailDone = Color.success
    .opacity(0.45)
  /// A glass edge: black at 0.14, or white at 0.16.
  static let strokeStrong = Color(light: .black(0.14), dark: .white(0.16))
  /// The 0.5-point highlight along a glass surface's top edge, such as the panel's: white at 0.7, or white at 0.12.
  static let strokeHighlight = Color(light: .white(0.7), dark: .white(0.12))
  /// A selected tab's edge: black at 0.16, or white at 0.22.
  static let strokeSelected = Color(light: .black(0.16), dark: .white(0.22))
  /// A pressed capsule's edge, such as Keys while in control, and a quiet mark, such as an agent's dot in Activity:
  /// black at 0.25, or white at 0.28.
  static let strokePressed = Color(light: .black(0.25), dark: .white(0.28))
  /// An unselected radio's ring: black at 0.28, or white at 0.30.
  static let strokeControl = Color(light: .black(0.28), dark: .white(0.3))
  /// The ring a thumbnail shows under the pointer, in place of lifting: black at 0.25, or white at 0.30.
  static let strokeHover = Color(light: .black(0.25), dark: .white(0.3))
  /// The shade along a keycap's bottom edge, so it reads as a key: black at 0.15, or black at 0.3.
  static let strokeShade = Color(light: .black(0.15), dark: .black(0.3))
  /// The faint shadow outside a chip on the picture: black at 0.10, or black at 0.25.
  static let dropShadowFaint = Color(light: .black(0.1), dark: .black(0.25))
  /// A soft shadow under a surface resting on the ground, such as the live picture's: black at 0.12, or black at
  /// 0.4.
  static let dropShadowSoft = Color(light: .black(0.12), dark: .black(0.4))
  /// A surface's shadow, such as a raised picture's, a floating bar's or an alert's: black at 0.16, or black at 0.5.
  static let dropShadow = Color(light: .black(0.16), dark: .black(0.5))
  /// A heavier shadow, for a surface over other content, such as the mini screen's or a large screenshot's:
  /// black at 0.22, or black at 0.55.
  static let dropShadowStrong = Color(light: .black(0.22), dark: .black(0.55))
}
