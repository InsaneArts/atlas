import SwiftUI

/// The end of the line whose action may not have finished while the helper needs the person: Show Screen, to look at
/// what it left, and Recover…, which asks before recovering, as small capsules. Both send the window's own events;
/// nothing here acts in the helper.
struct ActivityLineRecovery: View {
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// Show Screen, then Recover….
  var body: some View {
    HStack(spacing: Spacing.s) {
      Button("Show Screen") { send(.selectTab(.screen)) }
        .buttonStyle(.capsule(.glass, size: .mini))
        .help("Show what the action left on the helper's screen")
        .accessibilityHint("Shows what the action left on the helper's screen")
      Button("Recover…") { send(.askRecover) }
        .buttonStyle(.capsule(size: .mini))
        .help("Recover the helper so every session can go on")
        .accessibilityHint("Asks before recovering the helper, so every session can go on")
    }
    .fixedSize()
  }
}

#if DEBUG
  #Preview("Samples") {
    ActivityLineRecovery { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var last = String("Nothing pressed")
    InteractivePreview {
      Text(last)
    } content: {
      ActivityLineRecovery { event in last = String(describing: event) }
    }
  }
#endif
