import SwiftUI

/// Recovery is needed before going on: a screen of its own over the desktop.
struct RecoveryAlert: View {
  /// What the alert says.
  let prompt: ViewerRecoveryPrompt
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The scrim, then the words and the two buttons, which wait while recovery runs.
  var body: some View {
    ZStack {
      Rectangle()
        .fill(Color.scrim)
      VStack(spacing: Spacing.xl) {
        Eyebrow(text: "Needs you", tone: .attention)
        Headline(title: prompt.title, size: .title)
        Lede(text: prompt.reason)
        Note(text: "Recover gets the helper ready again and lets go of any held keys. No action runs twice.")
          .frame(maxWidth: ViewerLayout.recoveryNoteWidth)
        HStack(spacing: Spacing.ml) {
          Button("Not Now") { send(.recoverLater) }
            .buttonStyle(.capsule(.glass))
            .keyboardShortcut(.cancelAction)
          Button(prompt.recoverTitle) { send(.recover) }
            .buttonStyle(.capsule())
            .keyboardShortcut(.defaultAction)
        }
        .disabled(prompt.isBusy)
        .padding(.top, Spacing.m)
      }
      .padding(Spacing.page)
      .accessibilityElement(children: .contain)
      .accessibilityAddTraits(.isModal)
    }
  }
}

#if DEBUG
  #Preview("Recovery") {
    if let prompt = PreviewStates.viewerRecovery.recovery {
      RecoveryAlert(prompt: prompt) { _ in }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
  }
#endif
