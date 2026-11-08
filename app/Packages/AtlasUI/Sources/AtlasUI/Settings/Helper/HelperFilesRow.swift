import SwiftUI

/// One row of the Helper pane's "Your files": a 16-point symbol in its tone where a check's mark leads, so its words
/// line up with "Ready to act" above, the title over its detail, and its control at the end, at a compact row's height
/// and insets. When `swap` changes, the symbol and the words cross-fade in place over ``Motion/textSwap``, the row
/// keeping its height.
struct HelperFilesRow<Trailing: View>: View {
  /// The symbol leading the row.
  let symbol: SymbolName
  /// The symbol's colour.
  let tone: Color
  /// The title.
  let title: String
  /// The line under it.
  let detail: String
  /// What changes the symbol and the words together, which cross-fade when it does.
  let swap: Bool
  /// The control at the row's end.
  @ViewBuilder let trailing: Trailing
  @Environment(\.drawsListSeparator) private var drawsSeparator

  /// The symbol, the words and the control, with the separator above.
  var body: some View {
    HStack(spacing: ListRow.gap) {
      Image(systemName: symbol.rawValue)
        .font(Typography.font(size: IconSize.s))
        .foregroundStyle(tone)
        .frame(width: IconSize.s, height: IconSize.s)
        .swapsText(on: swap)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(title)
          .textStyle(.body, weight: ListRow.titleWeight(.check(.done), size: .compact))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(2)
        Text(detail)
          .textStyle(ListRow.Size.compact.detailStyle)
          .foregroundStyle(Color.textSecondary)
      }
      .swapsText(on: swap)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      trailing
    }
    .padding(.horizontal, ListRow.Size.compact.inset)
    .padding(.vertical, ListRow.Size.compact.verticalInset)
    .frame(minHeight: ListRow.Size.compact.height)
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, ListRow.separatorInset(.check(.done), size: .compact))
          .padding(.trailing, ListRow.Size.compact.inset)
      }
    }
    .accessibilityElement(children: .contain)
  }
}
