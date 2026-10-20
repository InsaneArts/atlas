import SwiftUI

/// The few latest actions, oldest first, over the desktop's corner.
struct CaptionList: View {
  /// The actions, oldest first.
  let entries: [ActivityEntry]

  /// One chip per action: a failure marked orange, the newest with a breathing dot, older ones faded.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.s) {
      // An entry's id is its place in the service's list of recent actions, the one identity the service gives.
      ForEach(entries) { entry in
        FloatingChip {
          switch entry.captionMark {
          case .failed:
            StatusDot(tone: .attention, size: .small)
          case .newest:
            StatusDot(tone: .active, pulsing: true, size: .small)
          case nil:
            EmptyView()
          }
          Text(entry.line)
            .font(Typography.font(.callout, weight: .medium))
          Text(entry.when)
            .font(Typography.font(.callout))
            .foregroundStyle(Color.textTertiary)
        }
        .opacity(entry.isNewest ? 1 : ViewerLayout.olderCaption)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Latest actions")
  }
}

#if DEBUG
  #Preview("Watching") {
    CaptionList(entries: PreviewStates.viewerWatching.stage.captions)
      .sampleSurface()
  }
#endif
