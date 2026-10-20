import SwiftUI

/// Shown when the last action's outcome is unknown: recover first.
struct RecoveryCard: View {
  /// Why recovery is needed.
  let detail: String
  /// What Recover does.
  let recover: @MainActor () -> Void

  /// The title and detail beside Recover, on an attention-tinted card.
  var body: some View {
    HStack(spacing: Spacing.ml) {
      TitleDetail(
        title: "The last action didn't finish",
        detail: detail,
        size: .compact,
        wraps: true
      )
      Spacer(minLength: Spacing.s)
      Button("Recover", action: recover)
        .buttonStyle(.capsule())
    }
    .padding(.horizontal, Spacing.l)
    .padding(.vertical, Spacing.ml)
    .cardSurface(.attention)
  }
}

#if DEBUG
  #Preview("Recovering") {
    RecoveryCard(detail: PreviewStates.panelRecovering.recovery ?? "") {}
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
