import SwiftUI

/// Rows in one group, as System Settings sets them: one quiet fill with no edge, and a separator above each row
/// but the first, inset to that row's text at its start and to the row's inset at its end. A long list can scroll inside its group, showing three
/// and a half rows with its bottom edge fading, so the header and footer around it never move.
struct GroupedList<Items: RandomAccessCollection, Row: View>: View where Items.Element: Identifiable {
  /// How the group is drawn.
  enum Style: Sendable {
    /// Radius 12: setup's grouped lists.
    case regular
    /// Radius 10: Settings' groups.
    case compact
  }

  /// The rows' items, in order.
  private let items: Items
  /// How the group is drawn.
  private let style: Style
  /// The size of its rows when it scrolls past three and a half of them, or nil when it always grows.
  private let scrolling: ListRow.Size?
  /// Draws one item's row.
  private let row: (Items.Element) -> Row

  /// Creates a group of rows.
  ///
  /// - Parameters:
  ///   - items: The rows' items, in order.
  ///   - style: How the group is drawn; regular by default.
  ///   - scrolling: The size of its rows, when it scrolls past three and a half of them; nil, by default, grows.
  ///   - row: Draws one item's row, usually a ``ListRow``.
  init(
    _ items: Items,
    style: Style = .regular,
    scrolling: ListRow.Size? = nil,
    @ViewBuilder row: @escaping (Items.Element) -> Row
  ) {
    self.items = items
    self.style = style
    self.scrolling = scrolling
    self.row = row
  }

  /// The rows on their group, scrolling where they must.
  var body: some View {
    content
      .clipShape(shape)
      .background(
        shape
          .fill(Color.fillGroup)
      )
  }

  /// The rows, in a fading scroll view when they run past three and a half.
  @ViewBuilder private var content: some View {
    if let scrolling, GroupedListMetrics.scrolls(rowCount: items.count, rowHeight: scrolling.height) {
      ScrollView {
        rows
      }
      .scrollIndicators(.never)
      .frame(height: GroupedListMetrics.scrollHeight(rowHeight: scrolling.height))
      .mask { ScrollFade() }
    } else {
      rows
    }
  }

  /// Every row, each but the first with its separator.
  private var rows: some View {
    VStack(spacing: .zero) {
      ForEach(items) { item in
        row(item)
          .environment(\.drawsListSeparator, item.id != items.first?.id)
      }
    }
  }

  /// The group's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: style == .regular ? Radius.large : Radius.medium, style: .continuous)
  }
}

#if DEBUG
  #Preview("Choices") {
    ListRowSamples(.choices)
  }

  #Preview("Scrolling") {
    ListRowSamples(.scrolling)
  }
#endif
