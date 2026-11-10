import SwiftUI

/// One choice drawn as a picture over its word, such as an account's level: a symbol in the accent on a quiet tinted
/// tile with ``Radius/medium`` corners, and, while chosen, the selected picture's ring outside it (``PictureRing``),
/// the word turning semibold. The ring comes and goes on ``Motion/state``, at once under Reduce Motion.
struct LevelTile: View {
  /// The symbol on the tile.
  let symbol: SymbolName
  /// The word under it.
  let title: String
  /// Whether it is the choice made.
  let isSelected: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The tile over its word.
  var body: some View {
    VStack(spacing: Spacing.half) {
      Image(systemName: symbol.rawValue)
        .font(Typography.font(size: IconSize.mPlus, weight: .regular))
        .foregroundStyle(Color.accent)
        .frame(maxWidth: .infinity)
        .frame(height: LevelTileLayout.height)
        .background(
          RoundedRectangle(cornerRadius: Radius.medium, style: .continuous)
            .fill(Color.fillLevelTile)
        )
        .padding(PictureRing.reach)
        .overlay {
          RoundedRectangle(cornerRadius: PictureRing.outerRadius(around: Radius.medium), style: .continuous)
            .strokeBorder(Color.accent, lineWidth: PictureRing.width)
            .opacity(isSelected ? 1 : 0)
        }
        .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: isSelected)
      Text(title)
        .textStyle(.caption, weight: isSelected ? .semibold : .medium)
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .minimumScaleFactor(LevelTileLayout.wordShrink)
    }
    .contentShape(Rectangle())
  }
}
