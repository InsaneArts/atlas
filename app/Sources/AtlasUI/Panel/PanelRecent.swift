import SwiftUI

/// The newest actions under the picture: "Recent" with All activity on its right, then a row for each.
struct PanelRecent: View {
  /// The actions, newest first.
  let entries: [ActivityEntry]
  /// Receives All activity.
  let send: @MainActor (PanelEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      HStack {
        GroupLabel(text: "Recent")
        Spacer(minLength: Spacing.m)
        Button("All activity") { send(.openActivity) }
          .buttonStyle(.textLink(.small))
          .help("Open every action in Atlas")
      }
      .padding(.bottom, Spacing.xxs)
      // An entry's id is its place in the service's list of recent actions, the one identity the service gives.
      ForEach(entries) { entry in
        PanelRecentRow(entry: entry)
      }
    }
  }
}

#if DEBUG
  #Preview("Idle") {
    PanelRecent(entries: PreviewStates.panelIdle.recent) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
