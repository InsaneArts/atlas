import SwiftUI

/// Shown when Recover is the next step: the last action's outcome is unknown, or the helper's input stopped.
struct RecoveryCard: View {
  /// Why recovery is needed.
  let recovery: PanelRecovery
  /// What Recover does.
  let recover: @MainActor () -> Void

  /// The title and detail beside Recover, on an attention-tinted card.
  var body: some View {
    HStack(spacing: Spacing.ml) {
      TitleDetail(
        title: title,
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

  /// What needs recovering.
  private var title: String {
    switch recovery {
    case .unfinished: "The last action didn't finish"
    case .notReady: "The helper isn't ready"
    }
  }

  /// Why, in the service's words.
  private var detail: String {
    switch recovery {
    case .unfinished(let detail), .notReady(let detail): detail
    }
  }
}

#if DEBUG
  #Preview("Recovering") {
    RecoveryCard(recovery: PreviewStates.panelRecovering.recovery ?? .unfinished("")) {}
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }

  #Preview("Input stopped") {
    RecoveryCard(recovery: PreviewStates.panelInputStopped.recovery ?? .notReady("")) {}
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
