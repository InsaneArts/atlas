import SwiftUI

/// A section of a form, as System Settings sets one (v3 G3, U-17): its name above the group, the rows in a compact
/// ``GroupedList``, and its notes under it, each the group's width. Sections stack in a ``FormColumn``.
struct GroupedSection<Items: RandomAccessCollection, Row: View>: View where Items.Element: Identifiable {
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
    VStack(alignment: .leading, spacing: Layout.gap) {
      if !title.isEmpty {
        GroupLabel(text: title)
          .padding(.horizontal, Spacing.xs)
      }
      GroupedList(items, style: .compact, row: row)
      ForEach(notes.indices, id: \.self) { index in
        GroupedSectionNoteView(note: notes[index])
          .padding(.horizontal, Spacing.xs)
      }
    }
  }
}

/// One line under a section's group.
private struct GroupedSectionNoteView: View {
  /// The line.
  let note: GroupedSectionNote

  var body: some View {
    switch note {
    case .text(let text):
      Text(text)
        .textStyle(.caption)
        .foregroundStyle(Color.textTertiary)
        .fixedSize(horizontal: false, vertical: true)
    case .status(let kind, let text):
      StatusLine(kind: kind, text: text, size: .small)
    }
  }
}

/// A section's own measures (v3 `.spane`'s 6 gap plus `.sghead`'s and `.sgfoot`'s 4).
private enum Layout {
  /// 10: between the name, the group and each note.
  static let gap = Spacing.sPlus
}

#if DEBUG
  #Preview("Samples") {
    GroupedSectionSamples()
  }
#endif
