import SwiftUI

/// The lines under a ``GroupedSection``'s group: 8 under it and 4 apart, each inset to where the rows' words start.
struct GroupedSectionNotes: View {
  /// The lines, in order; none draws nothing.
  let notes: [GroupedSectionNote]

  /// The lines, or nothing.
  var body: some View {
    if !notes.isEmpty {
      VStack(alignment: .leading, spacing: Spacing.xs) {
        ForEach(notes.indices, id: \.self) { index in
          GroupedSectionNoteView(note: notes[index])
        }
      }
      .padding(.horizontal, ListRow.Size.compact.inset)
      .padding(.top, Spacing.s)
    }
  }
}
