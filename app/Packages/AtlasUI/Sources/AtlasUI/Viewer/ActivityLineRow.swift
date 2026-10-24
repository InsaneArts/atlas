import SwiftUI

/// One line of the Activity pane: its time, its mark, what was done and why it failed, and "now" while it runs or the
/// screenshot it took with Open. It reads as one, with Open as its action.
struct ActivityLineRow: View {
  /// The line.
  let line: ViewerActivityLine
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    TimelineRow(time: line.time, title: line.text, reason: line.reason, isLive: line.lead == .live) {
      ActivityMark(kind: line.lead)
    } trailing: {
      trailing
    }
    .accessibilityElement(children: .combine)
    .accessibilityLabel(line.label)
    .accessibilityActions {
      if let shot = line.shot {
        Button("Open") { send(.openShot(shot.name)) }
      }
    }
  }

  /// "now" while it runs, the screenshot with Open for a screenshot's line, else nothing.
  @ViewBuilder private var trailing: some View {
    if let shot = line.shot {
      Button {
        send(.openShot(shot.name))
      } label: {
        HStack(spacing: Spacing.m) {
          PictureTile(picture: shot.thumbnail, radius: Radius.compact)
            .frame(width: ViewerLayout.lineThumbnail.width, height: ViewerLayout.lineThumbnail.height)
          Text("Open")
        }
      }
      .buttonStyle(.textLink)
      .help("Show this screenshot large")
      .onAppear { send(.thumbnailNeeded(shot.name)) }
    } else if line.lead == .live {
      Text("now")
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
    }
  }
}

#if DEBUG
  #Preview("Lines") {
    if case .activity(let activity) = PreviewStates.viewerActivity.pane {
      VStack(spacing: .zero) {
        ForEach(activity.groups.flatMap(\.lines)) { line in
          ActivityLineRow(line: line) { _ in }
        }
      }
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }
#endif
