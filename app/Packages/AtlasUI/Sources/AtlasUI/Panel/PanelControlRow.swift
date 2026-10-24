import SwiftUI

/// The panel's buttons, the actions on the agent (v3 P2): Stop in its slot only while an agent works, growing in and
/// out as the main action makes room, then the white main action filling the row. The row moves fourth in the panel's
/// cascade (v3 P7), its slot included. Each says what it does in its tooltip; the panel registers no shortcuts.
struct PanelControlRow: View {
  /// The buttons' states.
  let controls: PanelControls
  /// Receives what the person does.
  let send: @MainActor (PanelEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Stop in its slot, then the main action filling the row.
  var body: some View {
    HStack(spacing: Spacing.s) {
      Button("Stop") { send(.stopAgent) }
        .buttonStyle(.capsule(.glass, size: .small))
        .help("Stop what the agent is doing in the helper, and let it do nothing more until you resume")
        .actionSlot(controls.showsStop)
      Button(controls.main.title) { send(controls.main.event) }
        .buttonStyle(.capsule(size: .small, fill: true))
        .disabled(!controls.main.isEnabled)
        .help(controls.main.help)
    }
    // The slot's own row animation would start at once, ahead of the cascade; the panel's waits for the buttons' turn.
    .animation(PanelLayout.slot(isShown: controls.showsStop, reduceMotion: reduceMotion), value: controls.showsStop)
  }
}

#if DEBUG
  #Preview("Working") {
    PanelControlRow(controls: PreviewStates.panelWorking.controls) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Not ready") {
    PanelControlRow(controls: PreviewStates.panelNotReady.controls) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.panelWorking
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.panel) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      PanelControlRow(controls: state.controls) { event in
        state = PreviewStates.panel(state, applying: event)
      }
      .frame(width: PanelLayout.picture.width)
    }
  }
#endif
