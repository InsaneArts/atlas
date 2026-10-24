import SwiftUI

/// One line of the Activity pane (v3 V8): its time, its kind's symbol in a tile tinted by how it went, what was done
/// and why it failed, and "now" while it runs or the screenshot it took with Open. The live line's tile has a blue
/// ring that cools a second after it arrives. It reads as one, with Open as its action.
struct ActivityLineRow: View {
  /// The line.
  let line: ViewerActivityLine
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.paneShows) private var paneShows

  /// The line as a timeline row, read as one.
  var body: some View {
    TimelineRow(time: line.time, title: line.text, reason: line.reason) {
      ActivityLineMark(line: line)
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
        HStack(spacing: Spacing.sPlus) {
          PictureTile(picture: shot.thumbnail, radius: Layout.thumbnailRadius)
            .frame(width: ViewerLayout.lineThumbnail.width, height: ViewerLayout.lineThumbnail.height)
          Text(verbatim: "Open")
        }
      }
      .buttonStyle(.textLink)
      .help("Show this screenshot large")
      .onAppear { send(.thumbnailNeeded(shot.name)) }
      // A kept pane's lines do not appear anew as it shows again; a thumbnail let go meanwhile is asked for again.
      .onChange(of: paneShows) { _, shows in if shows { send(.thumbnailNeeded(shot.name)) } }
    } else if line.lead == .live {
      Text(verbatim: "now")
    }
  }
}

/// The line's own measures, from v3's Activity row (`.arow`).
private enum Layout {
  /// 5: the screenshot's thumbnail corner (v3 `.thumbm`).
  static let thumbnailRadius: CGFloat = 5
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

  #Preview("Interactive") {
    @Previewable @State var attempt = 0
    InteractivePreview {
      Button(String("Arrive again")) { attempt += 1 }
    } content: {
      if case .activity(let activity) = PreviewStates.viewerActivity.pane, let line = activity.groups.first?.lines.first
      {
        ActivityLineRow(line: line) { _ in }
          .id(attempt)
          .frame(width: SampleWidth.column * 2)
      }
    }
  }
#endif
