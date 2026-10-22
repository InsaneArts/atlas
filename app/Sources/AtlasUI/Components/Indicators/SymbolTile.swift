import SwiftUI

/// A small rounded tile with a symbol or a few monospaced characters on a quiet fill, such as an agent's `>_` in a
/// settings row.
struct SymbolTile: View {
  /// How large the tile is.
  enum Size: Sendable {
    /// 26, with 11-point characters: an agent's icon in a row.
    case agent
  }

  /// What the tile shows.
  let icon: TileIcon
  /// How large the tile is.
  var size = Size.agent

  var body: some View {
    glyph
      .foregroundStyle(Color.textPrimary)
      .frame(width: side, height: side)
      .background(
        RoundedRectangle(cornerRadius: Radius.compact)
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
    case .agent: IconSize.agent
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      SymbolTile(icon: .text(">_"))
      SymbolTile(icon: .symbol("bubble.left"))
    }
    .sampleSurface()
  }
#endif
