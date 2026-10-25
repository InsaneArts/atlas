import SwiftUI

/// One line under a ``GroupedSection``'s group: quiet small print, or a small status line.
struct GroupedSectionNoteView: View {
  /// The line.
  let note: GroupedSectionNote

  /// The small print, or the status line with its mark.
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

#if DEBUG
  #Preview("Notes") {
    VStack(alignment: .leading, spacing: Spacing.sPlus) {
      GroupedSectionNoteView(note: .text("Atlas reads these from the helper."))
      GroupedSectionNoteView(note: .status(.working, "Waiting for Recover to finish"))
      GroupedSectionNoteView(note: .status(.problem, "The helper didn't answer."))
      GroupedSectionNoteView(note: .status(.done, "Recovered"))
    }
    .padding(Spacing.xxl)
    .sampleSurface()
  }
#endif
