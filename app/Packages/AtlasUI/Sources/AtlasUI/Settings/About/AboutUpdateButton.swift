import SwiftUI

/// About's native update button: Check for Updates…, then prominent Install Update… once a newer version is found.
/// While its check or install runs it waits, disabled, and the line under it says what runs. It lays out at its
/// widest title, so it never changes width.
struct AboutUpdateButton: View {
  /// Where the check stands.
  let update: SettingsUpdateButton
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    Group {
      switch update {
      case .check, .checking:
        button
          .buttonStyle(.bordered)
      case .install, .installing:
        button
          .buttonStyle(.borderedProminent)
      }
    }
    .disabled(update.isWorking)
  }

  /// The action and its title, sized once for either native style.
  private var button: some View {
    Button {
      if let event = update.event { send(event) }
    } label: {
      ZStack {
        // Reserve the widest title so checking and installing share one width.
        ForEach(SettingsUpdateButton.titles, id: \.self) { title in
          Text(title)
            .hidden()
        }
        Text(update.title)
          .swapsText(on: update.title)
      }
    }
  }
}
