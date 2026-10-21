import SwiftUI

/// Open Viewer, and the button beside it.
struct PanelButtons: View {
  /// How Open Viewer is painted.
  let viewerLook: CapsuleButtonStyle.Look
  /// The button beside it; nil shows none.
  let secondary: PanelSecondaryAction?
  /// Receives the buttons' events.
  let send: @MainActor (PanelEvent) -> Void

  /// The two buttons, sharing the row.
  var body: some View {
    HStack(spacing: Spacing.m) {
      Button("Open Viewer") { send(.openViewer) }
        .buttonStyle(.capsule(viewerLook, fill: true))
        // Return opens the viewer only while it is the next step.
        .keyboardShortcut(viewerLook == .primary ? .defaultAction : nil)
      switch secondary {
      case .revokeControl:
        Button("Release Control") { send(.revokeControl) }
          .buttonStyle(.capsule(.glass, fill: true))
      case .stopAgent:
        Button("Stop Agent") { send(.stopAgent) }
          .buttonStyle(.capsule(.glass, fill: true))
      case .resumeAgent(let isEnabled):
        Button("Resume Agent") { send(.resumeAgent) }
          .buttonStyle(.capsule(.glass, fill: true))
          .disabled(!isEnabled)
      case .switchToHelper(let isEnabled):
        Button("Switch to Helper…") { send(.switchToHelper) }
          .buttonStyle(.capsule(.glass, fill: true))
          .disabled(!isEnabled)
      case nil:
        EmptyView()
      }
    }
  }
}

#if DEBUG
  #Preview("In control") {
    PanelButtons(viewerLook: PreviewStates.panelInControl.viewerLook, secondary: .revokeControl) { _ in }
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }

  #Preview("Agent working") {
    PanelButtons(viewerLook: PreviewStates.panelAgentWorking.viewerLook, secondary: .stopAgent) { _ in }
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
