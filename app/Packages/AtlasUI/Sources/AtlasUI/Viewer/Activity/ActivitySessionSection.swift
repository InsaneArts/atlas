import SwiftUI

/// One host session's part of Activity while it shows every session's work: its header, pinned as the list scrolls,
/// over its newest lines, a hairline between each, a new line fading in as Activity's lines do.
struct ActivitySessionSection: View {
  /// The group.
  let group: ViewerActivitySessionGroup
  /// Whether the list sits at its very top, which decides how a new line arrives.
  let isAtTop: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The header over the lines.
  var body: some View {
    Section {
      ForEach(group.lines) { line in
        ActivityLineRow(line: line, send: send)
          .environment(\.drawsListSeparator, line.id != group.lines.first?.id)
          .transition(ActivityScroll.arrival(isAtTop: isAtTop))
      }
    } header: {
      ActivitySessionHeader(group: group, send: send)
    }
  }
}

#if DEBUG
  #Preview("By session") {
    if case .activity(let activity) = PreviewStates.viewerActivityBySession.pane {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: .zero, pinnedViews: .sectionHeaders) {
          ForEach(activity.sessions) { group in
            ActivitySessionSection(group: group, isAtTop: true) { _ in }
          }
        }
      }
      .frame(width: SampleWidth.column * 3, height: SampleWidth.column * 2)
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerActivityNeedsYou
    InteractivePreview {
      if case .activity(let activity) = state.pane {
        ScrollView {
          LazyVStack(alignment: .leading, spacing: .zero, pinnedViews: .sectionHeaders) {
            ForEach(activity.sessions) { group in
              ActivitySessionSection(group: group, isAtTop: true) { event in
                state = PreviewStates.viewer(state, applying: event)
              }
            }
          }
        }
        .frame(width: SampleWidth.column * 3, height: SampleWidth.column * 2)
      }
    }
  }
#endif
