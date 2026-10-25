import SwiftUI

/// About's one update button, whatever the check's state: glass Check for Updates, then blue Install Update… once a
/// newer version is found. It holds no spinner: while its check or install runs it waits, plain, and the line under
/// it says what runs. It lays out at its widest word, so it never changes width.
struct AboutUpdateButton: View {
  /// Where the check stands.
  let update: SettingsUpdateButton
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    SettingsAlertButton(opensAlert: update.event == .askInstall) {
      if let event = update.event { send(event) }
    } label: {
      ZStack {
        // Untracked, the widest any look sets them, so the glass and the blue button share one width.
        ForEach(SettingsUpdateButton.titles, id: \.self) { title in
          Text(title)
            .tracking(LetterSpacing.natural)
            .hidden()
        }
        Text(update.title)
          .swapsText(on: update.title)
      }
    }
    .buttonStyle(.capsule(update.look, size: .mini))
    .disabled(update.isWorking)
  }
}
