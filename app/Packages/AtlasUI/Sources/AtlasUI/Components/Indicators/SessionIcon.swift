import SwiftUI

/// A host session's icon at a side from ``IconSize``: its app's logo clipped to small corners with a hairline ring, so
/// a dark logo such as Cursor's keeps its edge on dark glass, an agent's mark scaled to the side, or a tile with `>_`
/// for Terminal. One view for the panel's rows, the main window's group headers, its chip and Details, the captions
/// and the sessions menu's items. VoiceOver hides it; the name beside it is read.
public struct SessionIcon: View {
  /// 16: a menu item's image, as AppKit's menu items draw theirs.
  public static let menuSide = IconSize.s
  /// 0.42: a tile's glyph against its side, as a symbol tile's 11-point characters sit on its 26.
  private static let glyphShare: CGFloat = 0.42
  /// 0.25: the most a corner takes of the side.
  private static let cornerShare: CGFloat = 0.25

  /// What it draws.
  private let icon: RowIcon
  /// Its side, in points.
  private let side: CGFloat

  /// Creates the icon.
  ///
  /// - Parameters:
  ///   - icon: What it draws.
  ///   - side: Its side, in points, from ``IconSize`` or a window's layout.
  public init(icon: RowIcon, side: CGFloat) {
    self.icon = icon
    self.side = side
  }

  /// The logo, the mark or the tile, at its side.
  public var body: some View {
    glyph
      .frame(width: side, height: side)
      .accessibilityHidden(true)
  }

  /// The icon itself.
  @ViewBuilder private var glyph: some View {
    switch icon {
    case .app(let image):
      Image(decorative: image.image, scale: 1)
        .resizable()
        .interpolation(.high)
        .clipShape(RoundedRectangle(cornerRadius: corner, style: .continuous))
        .overlay(
          RoundedRectangle(cornerRadius: corner, style: .continuous)
            .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
        )
    case .mark(let kind):
      // The mark draws at its row size; scaled, its card, glyph and rim keep their proportions.
      AgentMark(kind: kind)
        .scaleEffect(side / IconSize.mPlus)
    case .tile(let tile):
      tileGlyph(tile)
        .foregroundStyle(Color.textSecondary)
        .frame(width: side, height: side)
        .background(
          RoundedRectangle(cornerRadius: corner, style: .continuous)
            .fill(Color.fillControl)
        )
    }
  }

  /// A tile's symbol, or its few characters in a monospaced face, sized from the side.
  ///
  /// - Parameter tile: What the tile shows.
  /// - Returns: The glyph.
  @ViewBuilder private func tileGlyph(_ tile: TileIcon) -> some View {
    switch tile {
    case .symbol(let name):
      Image(systemName: name)
        .font(Typography.font(size: side * Self.glyphShare, weight: .semibold))
    case .text(let text):
      Text(text)
        .font(Typography.font(size: side * Self.glyphShare, weight: .semibold))
        .monospaced()
        .lineLimit(1)
        .fixedSize()
    }
  }

  /// The corners: small, and smaller still on a small icon, so a caption's icon stays a rounded square.
  private var corner: CGFloat {
    min(Radius.extraSmall, side * Self.cornerShare)
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      ForEach(Array(SessionSamples.icons.enumerated()), id: \.offset) { _, icon in
        VStack(spacing: Spacing.s) {
          SessionIcon(icon: icon, side: IconSize.m)
          SessionIcon(icon: icon, side: IconSize.s)
          SessionIcon(icon: icon, side: IconSize.xsPlus)
        }
      }
    }
    .sampleSurface()
  }
#endif
