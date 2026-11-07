import SwiftUI

/// An opened run's Replay: a small secondary capsule with `play.fill` and its word, or on a narrow card a circle with
/// the symbol alone. It plays the run's steps over the whole window.
struct ActivityReplayButton: View {
  /// The run's identity.
  let run: String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.outlineWidth) private var width

  /// The button.
  var body: some View {
    let narrow = width <= ViewerLayout.outlineNarrowest
    Button {
      send(.openReplay(run))
    } label: {
      HStack(spacing: Spacing.xs) {
        Image(systemName: SymbolName.play.rawValue)
        if !narrow {
          Text("Replay")
        }
      }
    }
    .buttonStyle(.capsule(.secondary, size: .mini, outline: narrow ? .circle : .pill))
    .help("Play this run's steps in order")
    .accessibilityLabel(Text("Replay"))
  }
}
