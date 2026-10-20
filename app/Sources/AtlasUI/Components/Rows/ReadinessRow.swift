import SwiftUI

/// One readiness check, as a checklist of readiness shows it: the mark says how it stands, and the name has the rest
/// of the row, with what to do under it unless it is ready.
struct ReadinessRow: View {
  /// What is checked.
  let name: String
  /// What it means, or what to do.
  let text: String
  /// How it stands.
  let state: ReadinessState

  var body: some View {
    HStack(alignment: alignment, spacing: Spacing.l) {
      mark
      VStack(alignment: .leading, spacing: Layout.gap) {
        Text(name)
          .font(Typography.font(.body))
          .fixedSize(horizontal: false, vertical: true)
        if state != .ready {
          Text(text)
            .font(Typography.font(.note))
            .foregroundStyle(textTone.color)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(.horizontal, Spacing.xl)
    .padding(.vertical, Spacing.l)
    .accessibilityElement(children: .combine)
    .accessibilityValue(state.spokenValue)
  }

  /// A check when ready, an orange "!" when not, an empty ring before it is checked.
  @ViewBuilder private var mark: some View {
    switch state {
    case .ready:
      CheckMark(size: IconSize.mark)
    case .notReady:
      Disc(content: .symbol("exclamationmark"), tone: .attention, size: .mark)
        .accessibilityHidden(true)
    case .unknown:
      Circle()
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.emphasis)
        .frame(width: IconSize.mark, height: IconSize.mark)
        .accessibilityHidden(true)
    }
  }

  /// A ready check's one line centres on its mark; a check with text under its name hangs from the top.
  private var alignment: VerticalAlignment {
    state == .ready ? .center : .top
  }

  /// What to do is orange while the check fails, quieter before it is checked.
  private var textTone: TextTone {
    state == .notReady ? .attention : .secondary
  }
}

/// The row's own measures.
private enum Layout {
  /// [R] 3: between the name and its text, kept exact until Phase 3b rounds it to `Spacing.xxs` (2).
  static let gap: CGFloat = 3
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.readiness)
  }
#endif
