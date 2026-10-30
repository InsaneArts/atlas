import SwiftUI

/// The panel's buttons, the actions on the agent: Stop while an agent works, or Resume once the person stopped agents,
/// in a secondary slot that grows in and out as the main action makes room, then the white main action filling the row.
/// Each names its menu item and shortcut in its tooltip, as the main window's buttons do.
struct PanelControlRow: View {
  /// The buttons' states.
  let controls: PanelControls
  /// Receives what the person does.
  let send: @MainActor (PanelEvent) -> Void

  /// Stop or Resume in its slot, then the main action filling the row.
  var body: some View {
    HStack(spacing: Spacing.s) {
      LineSwap(agentTitle) { shown, isDrawn, isReady in
        Button {
          if isReady && agentIsEnabled { send(agentEvent) }
        } label: {
          // Reserve Resume's width so changing the action never resizes either button.
          ZStack {
            Text(verbatim: "Resume")
              .hidden()
            Text(shown)
              .opacity(isDrawn ? 1 : 0)
          }
        }
        .buttonStyle(.capsule(.secondary, size: .small))
        .disabled(!agentIsEnabled || !isReady)
        .help(controls.agentHelp)
        .accessibilityLabel(shown)
        .layoutFrame("panel agent action")
        .actionSlot(controls.agent != nil)
      }
      LineSwap(controls.main.title) { shown, isDrawn, isReady in
        Button {
          if isReady && controls.main.isEnabled { send(controls.main.event) }
        } label: {
          Text(shown)
            .opacity(isDrawn ? 1 : 0)
        }
        .buttonStyle(.capsule(size: .small, fill: true))
        .disabled(!controls.main.isEnabled || !isReady)
        .help(controls.main.help)
        .accessibilityLabel(shown)
        .layoutFrame("panel main action")
      }
    }
    .animatesActionSlots(controls.agent != nil)
  }

  /// The secondary button's word: Resume once agents are stopped, else Stop.
  private var agentTitle: String {
    isResume ? "Resume" : "Stop"
  }

  /// What the secondary button sends.
  private var agentEvent: PanelEvent {
    isResume ? .resumeAgent : .stopAgent
  }

  /// Whether the secondary button can be pressed: Stop always, Resume once nothing else is asked of the service.
  private var agentIsEnabled: Bool {
    guard controls.agent != nil else { return false }
    guard case .resume(let isEnabled) = controls.agent else { return true }
    return isEnabled
  }

  /// Whether the secondary button is Resume.
  private var isResume: Bool {
    guard case .resume = controls.agent else { return false }
    return true
  }
}

#if DEBUG
  #Preview("Working") {
    PanelControlRow(controls: PreviewStates.panelWorking.controls) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Stopped") {
    PanelControlRow(controls: PreviewStates.panelStopped.controls) { _ in }
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
