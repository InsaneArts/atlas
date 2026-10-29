import SwiftUI

/// Clean Up This Session's Work…, the whole width of Details, last in the chosen session's part: it asks before
/// anything runs. While what it takes is read, or the cleanup runs, it holds a spinner, VoiceOver hears its working
/// title, and it takes no press; while another session's cleanup runs, it takes no press either.
struct SessionCleanupButton: View {
  /// The cleanup.
  let cleanup: SessionCleanup
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The button.
  var body: some View {
    Button(cleanup.title) {
      send(.askCleanup(session: cleanup.session))
    }
    .buttonStyle(.capsule(.glass, size: .small, fill: true, isWorking: cleanup.isWorking))
    .disabled(!cleanup.isEnabled)
    .accessibilityLabel(cleanup.isWorking ? cleanup.workingTitle : cleanup.title)
    .help("Quit, roll back and clean up what this session left, once you confirm")
  }
}

#if DEBUG
  #Preview("Samples") {
    if let cleanup = PreviewStates.viewerSessionDetail.details?.hostSession?.cleanup {
      VStack(spacing: Spacing.m) {
        SessionCleanupButton(cleanup: cleanup) { _ in }
        SessionCleanupButton(
          cleanup: {
            var working = cleanup
            working.stage = .working
            return working
          }()
        ) { _ in }
        SessionCleanupButton(
          cleanup: {
            var waiting = cleanup
            waiting.waitsForAnother = true
            return waiting
          }()
        ) { _ in }
      }
      .frame(width: ViewerLayout.detailsWidth)
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var working = false
    if let cleanup = PreviewStates.viewerSessionDetail.details?.hostSession?.cleanup {
      InteractivePreview {
        Toggle(String("Working"), isOn: $working)
      } content: {
        SessionCleanupButton(
          cleanup: {
            var shown = cleanup
            shown.stage = working ? .working : .ready
            return shown
          }()
        ) { _ in working = true }
        .frame(width: ViewerLayout.detailsWidth)
      }
    }
  }
#endif
