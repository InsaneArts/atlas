import SwiftUI

/// A glyph on a tile in an app icon's place, on the icon grid so it stands the size of the real icons beside it: a
/// quiet tile for what was done in no app, such as commands, or the person's blue mark. No edge and no shadow.
struct IconGridTile: View {
  /// The glyph takes half the tile's body, as a monogram's letter does.
  private static let glyphShare: CGFloat = 0.5
  /// How the tile is coloured.
  enum Tone: Sendable {
    /// A control's fill under a secondary glyph.
    case quiet
    /// Blue under a white glyph: the person's own.
    case accent
  }

  /// The glyph.
  let symbol: SymbolName
  /// How it is coloured.
  var tone = Tone.quiet
  /// The square's side, as a real icon's whole canvas.
  var side = IconSize.mPlus

  /// The tile, centred in its square.
  var body: some View {
    let body = side * IconGrid.bodyShare
    Image(systemName: symbol.rawValue)
      .font(Typography.font(size: body * Self.glyphShare, weight: .semibold))
      .foregroundStyle(tone == .accent ? Color.onAccent : Color.textSecondary)
      .frame(width: body, height: body)
      .background(
        RoundedRectangle(cornerRadius: body * IconGrid.cornerShare, style: .continuous)
          .fill(tone == .accent ? Color.accent : Color.fillControl)
      )
      .frame(width: side, height: side)
      .accessibilityHidden(true)
  }
}
