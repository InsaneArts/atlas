import SwiftUI

/// One line of a checklist: a mark for how it stands, what is checked, and a note on the right.
struct CheckRow: View {
  /// What is checked.
  let text: String
  /// How it stands.
  let state: CheckState
  /// A word on the right, such as "passed"; nil shows none.
  var note: String?

  var body: some View {
    HStack(spacing: Spacing.l) {
      mark
        .frame(width: IconSize.mark, height: IconSize.mark)
      Text(text)
        .font(Typography.font(.body))
      Spacer(minLength: Spacing.m)
      if let note {
        Text(note)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
      }
    }
    .padding(.horizontal, Spacing.xl)
    .padding(.vertical, Spacing.l)
    .accessibilityElement(children: .combine)
    .accessibilityValue(state.spokenValue)
  }

  /// A check when it passed, a spinner while it runs, an orange mark when it failed.
  @ViewBuilder private var mark: some View {
    switch state {
    case .done:
      CheckMark()
    case .busy:
      Spinner(size: IconSize.glyphLarge)
    case .failed:
      Image(systemName: "exclamationmark.circle.fill")
        .foregroundStyle(Color.attention)
        // The row's value says it failed.
        .accessibilityHidden(true)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.check)
  }
#endif
