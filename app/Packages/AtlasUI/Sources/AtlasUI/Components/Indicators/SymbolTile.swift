import SwiftUI

/// A small rounded tile with a symbol or a few monospaced characters on a quiet fill, such as an agent's `>_` in a
/// settings row.
struct SymbolTile: View {
  /// How large the tile is.
  enum Size: Sendable, CaseIterable {
    /// 22, with a row's symbol: an action's kind in Activity (v3 V8).
    case small
    /// 26, with 11-point characters: an icon in a row, such as an agent's.
    case regular
    /// 30: a ``ListRow``'s tile (v3's 30-point lead, which insets the separator 56). v3 names this size regular;
    /// today's regular moves to it when its rows move to ``ListRow``.
    case large
  }

  /// What the tile shows.
  let icon: TileIcon
  /// How large the tile is.
  var size = Size.regular

  var body: some View {
    glyph
      .foregroundStyle(Color.textPrimary)
      .frame(width: side, height: side)
      .background(
        RoundedRectangle(cornerRadius: Radius.extraSmall)
          .fill(Color.fillControl)
      )
      .accessibilityHidden(true)
  }

  /// The symbol, or the characters in a monospaced face.
  @ViewBuilder private var glyph: some View {
    switch icon {
    case .symbol(let name):
      Image(systemName: name)
        .font(Typography.font(.callout))
    case .text(let text):
      Text(text)
        .font(Typography.font(.caption, design: .monospaced))
    }
  }

  /// The tile's side.
  private var side: CGFloat {
    switch size {
    case .small: Layout.smallSide
    case .regular: IconSize.mPlus
    case .large: IconSize.l
    }
  }
}

/// The tile's own measures.
private enum Layout {
  /// 22: a small tile, Activity's (v3 V8's 22 tile).
  static let smallSide: CGFloat = 22
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      SymbolTile(icon: .text(">_"))
      SymbolTile(icon: .symbol("bubble.left"))
    }
    .sampleSurface()
  }

  #Preview("Sizes") {
    IndicatorSamples(.symbolTile)
  }
#endif
