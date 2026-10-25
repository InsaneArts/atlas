import SwiftUI

/// About's row capsule: Check for Updates…, then accent Install Update… once a newer version is found.
/// While its check or install runs it waits, disabled, and the row's detail says what runs. It lays out at its
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
          .buttonStyle(.capsule(.glass, size: .mini))
      case .install, .installing:
        button
          .buttonStyle(.capsule(.accent, size: .mini))
      }
    }
    .fixedSize()
    .disabled(update.isWorking)
  }

  /// The action and its title, sized once for either capsule look.
  private var button: some View {
    Button {
      if let event = update.event { send(event) }
    } label: {
      ZStack {
        // Reserve the widest title without tracking, so glass and accent keep one width despite their letter spacing.
        ForEach(SettingsUpdateButton.titles, id: \.self) { title in
          Text(title)
            .tracking(.zero)
            .hidden()
        }
        Text(update.title)
          .swapsText(on: update.title)
      }
    }
  }
}
