import SwiftUI

/// The panel's buttons, left to right: Stop while an agent works, the white main action filling the row, the pause
/// button and ⋯. Each says what it does in its tooltip; the panel registers no shortcuts.
struct PanelControlRow: View {
  /// The buttons' states.
  let controls: PanelControls
  /// Receives what the person does.
  let send: @MainActor (PanelEvent) -> Void
  /// Where ⋯ is in the panel, so its menu opens under it.
  @State private var moreFrame = CGRect.zero

  var body: some View {
    HStack(spacing: Spacing.m) {
      if controls.showsStop {
        Button("Stop") { send(.stopAgent) }
          .buttonStyle(.capsule(.glass, size: .small))
          .help("Stop what the agent is doing in the helper, and let it do nothing more until you resume")
      }
      Button(controls.main.title) { send(controls.main.event) }
        .buttonStyle(.capsule(size: .small, fill: true))
        .disabled(!controls.main.isEnabled)
        .help(controls.main.help)
      Button {
        send(.togglePause)
      } label: {
        PauseGlyph(isPaused: controls.pause.isPaused)
      }
      .buttonStyle(.circle(.small, isOn: controls.pause.isPaused))
      .help(controls.pause.help)
      .accessibilityLabel(controls.pause.label)
      Button {
        send(.showMore(below: moreFrame))
      } label: {
        Image(systemName: "ellipsis")
      }
      .buttonStyle(.circle(.small, isOn: controls.isMoreOpen))
      .onGeometryChange(for: CGRect.self) {
        $0.frame(in: .global)
      } action: {
        moreFrame = $0
      }
      .help("Atlas and the helper")
      .accessibilityLabel("More")
    }
  }
}

#if DEBUG
  #Preview("Working") {
    PanelControlRow(controls: PreviewStates.panelWorking.controls) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Logged out") {
    PanelControlRow(controls: PreviewStates.panelLoggedOut.controls) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
