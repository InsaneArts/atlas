import SwiftUI

/// About's one update button (v3 G9), whatever the check's state: white Check for Updates, a spinner in it while it
/// checks, blue Install Update… once a newer version is found, and a spinner in that while it installs. It is one
/// button throughout, its label morphing from one word to the next.
struct AboutUpdateButton: View {
  /// Where the check stands.
  let update: SettingsUpdateButton
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    Button {
      if let event = update.event { send(event) }
    } label: {
      WorkingLabel(title: update.title, isWorking: update.isWorking, arc: update.arc)
    }
    // A working button cannot be pressed yet stays whole, so its spinner reads as work, not as a button turned off.
    .buttonStyle(.capsule(update.look, size: .mini, isWorking: update.isWorking))
    .disabled(update.isWorking)
  }
}
