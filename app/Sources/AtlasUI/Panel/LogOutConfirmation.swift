import SwiftUI

/// Asks before logging the helper out: only the person can log it back in.
struct LogOutConfirmation: View {
  /// What Cancel does.
  let cancel: @MainActor () -> Void
  /// What Log Out does.
  let logOut: @MainActor () -> Void

  /// The question and what it means, over Cancel and Log Out.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.ml) {
      TitleDetail(
        title: "Log out the helper?",
        detail: "Its apps quit, and agents can't use it until you log in to it again.",
        size: .compact,
        wraps: true
      )
      HStack(spacing: Spacing.m) {
        Button("Cancel", action: cancel)
          .buttonStyle(.capsule(.glass, fill: true))
          .keyboardShortcut(.cancelAction)
        Button("Log Out", action: logOut)
          .buttonStyle(.capsule(fill: true))
      }
    }
    .padding(Spacing.l)
    .cardSurface()
  }
}

#if DEBUG
  #Preview("Confirm log-out") {
    LogOutConfirmation(cancel: {}, logOut: {})
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
