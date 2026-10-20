import SwiftUI

/// Who did what, and when: a disc with the actor's initial, the action and its time.
struct ActivityRow: View {
  /// The actor's initial, shown unless the action failed.
  let initial: String
  /// What was done, by whom.
  let line: String
  /// How long ago.
  let when: String
  /// Whose action it was.
  let kind: ActivityKind

  var body: some View {
    HStack(spacing: Spacing.l) {
      Disc(content: disc, tone: discTone)
        .accessibilityHidden(true)
      VStack(alignment: .leading, spacing: Spacing.xxs) {
        Text(line)
          .font(Typography.font(.body))
          .foregroundStyle(lineTone.color)
          .fixedSize(horizontal: false, vertical: true)
        Text(when)
          .font(Typography.font(.note))
          .foregroundStyle(Color.textTertiary)
          .monospacedDigit()
      }
      Spacer(minLength: 0)
    }
    .padding(.horizontal, Spacing.xl)
    .padding(.vertical, Spacing.ml)
    .accessibilityElement(children: .combine)
  }

  /// The actor's initial, or "!" for a failure.
  private var disc: Disc.Content {
    switch kind {
    case .yours, .others: .letter(initial)
    case .failed: .letter("!")
    }
  }

  private var discTone: Disc.Tone {
    switch kind {
    case .yours: .accentSoft
    case .others: .neutral
    case .failed: .attention
    }
  }

  private var lineTone: TextTone {
    switch kind {
    case .yours, .others: .primary
    case .failed: .attention
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.activity)
  }
#endif
