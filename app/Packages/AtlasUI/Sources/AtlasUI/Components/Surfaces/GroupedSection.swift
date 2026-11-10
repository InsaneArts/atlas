import SwiftUI

/// A section of a page of settings, as Settings' pages set one on their content card: its name above the group, 10
/// over it, and its notes 8 under it, each inset to where the rows' words start, and the rows in a ``GroupedList`` on
/// the card's quiet ground (``Color/fillGroupInset``).
struct GroupedSection<Items: RandomAccessCollection, Row: View>: View where Items.Element: Identifiable {
  /// 10: between the section's name and its group.
  static var titleGap: CGFloat { Spacing.sPlus }

  /// The rows' items, in order.
  private let items: Items
  /// The section's name; empty for none.
  private let title: String
  /// The lines under the group, in order.
  private let notes: [GroupedSectionNote]
  /// Draws one item's row, usually a compact ``ListRow``.
  private let row: (Items.Element) -> Row

  /// Creates a section.
  ///
  /// - Parameters:
  ///   - items: The rows' items, in order.
  ///   - title: The section's name; none by default.
  ///   - notes: The lines under the group, in order; none by default.
  ///   - row: Draws one item's row.
  init(
    _ items: Items,
    title: String = "",
    notes: [GroupedSectionNote] = [],
    @ViewBuilder row: @escaping (Items.Element) -> Row
  ) {
    self.items = items
    self.title = title
    self.notes = notes
    self.row = row
  }

  /// The name, the group and the notes.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      if !title.isEmpty {
        SectionTitle(text: title)
          .padding(.horizontal, ListRow.Size.compact.inset)
          .padding(.bottom, Self.titleGap)
      }
      GroupedList(items, fill: .fillGroupInset, row: row)
      GroupedSectionNotes(notes: notes)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    GroupedSectionSamples()
  }
#endif
