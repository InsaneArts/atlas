import SwiftUI

/// A Library row's label: its list's symbol in grey, its word, the unseen dot after it, and how many the list holds at
/// the trailing edge once it has been read. The word keeps one weight whether chosen or not, so choosing a list never
/// shifts the dot after it; the pill says which is chosen.
struct SidebarLibraryLabel: View {
  /// The row.
  let row: ViewerLibraryRow

  /// The symbol, the word, the dot and the count.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      Image(systemName: row.place.symbol.rawValue)
        .font(Typography.symbol(.glyph))
        .foregroundStyle(Color.textSecondary)
        .frame(width: ViewerLayout.librarySymbol)
        .accessibilityHidden(true)
      HStack(spacing: ViewerLayout.sidebarIconGap) {
        Text(row.title)
          .font(Typography.font(.body, weight: .medium))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
        ViewerUnseenDot(unseen: row.unseen)
        Spacer(minLength: .zero)
        if let count = row.count {
          Text(count, format: .number)
            .font(Typography.font(.callout, weight: .medium))
            .foregroundStyle(Color.textTertiary)
            .monospacedDigit()
        }
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.s) {
      ForEach(PreviewStates.viewerActivity.sidebar.library) { row in
        SidebarLibraryLabel(row: row)
      }
    }
    .frame(width: ViewerLayout.rowContentWidth)
    .padding(Spacing.l)
    .sampleSurface()
  }
#endif
