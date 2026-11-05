import SwiftUI

/// A replay's controls: Previous, Play or Pause, Next, a bar to scrub through the frames, and where it stands.
struct ReplayControls: View {
  /// The replay.
  let replay: ViewerReplayState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The buttons, the bar and the position.
  var body: some View {
    HStack(spacing: Spacing.s) {
      round(.back, label: "Previous step", look: .secondary, enabled: replay.index > 0) { send(.previousFrame) }
      round(
        replay.isPlaying ? .pause : .play,
        label: replay.isPlaying ? "Pause" : "Play",
        look: .accent,
        enabled: replay.frames.count > 1
      ) { send(.togglePlay) }
      round(.next, label: "Next step", look: .secondary, enabled: replay.index < replay.frames.count - 1) {
        send(.nextFrame)
      }
      if replay.frames.count > 1 {
        Slider(
          value: Binding(
            get: { Double(replay.index) },
            set: { send(.replayFrame(Int($0.rounded()))) }
          ),
          in: 0...Double(replay.frames.count - 1),
          step: 1
        )
        .controlSize(.small)
        .tint(Color.accent)
        .accessibilityLabel("Scrub through the steps")
        .accessibilityValue(replay.position)
      } else {
        Spacer()
      }
      Text(verbatim: replay.position)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
        .frame(minWidth: ViewerLayout.replayPosition, alignment: .trailing)
        .accessibilityHidden(true)
    }
  }

  /// A round button.
  ///
  /// - Parameters:
  ///   - symbol: Its symbol.
  ///   - label: What VoiceOver calls it.
  ///   - look: How it is painted.
  ///   - enabled: Whether it goes anywhere.
  ///   - action: What it does.
  /// - Returns: The button.
  private func round(
    _ symbol: SymbolName,
    label: String,
    look: CapsuleButtonStyle.Look,
    enabled: Bool,
    action: @escaping @MainActor () -> Void
  ) -> some View {
    Button(action: action) {
      Image(systemName: symbol.rawValue)
    }
    .buttonStyle(.capsule(look, size: .small, outline: .circle))
    .disabled(!enabled)
    .help(label)
    .accessibilityLabel(label)
  }
}
