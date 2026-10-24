import SwiftUI

/// The Activity tab: what was done in the helper, newest first, under Now, Earlier today and the days before, with a
/// search over what was done and filters for agents, the person and problems; or why nothing shows.
struct ViewerActivityPane: View {
  /// What the pane shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Activity", meta: state.meta) {
        SearchField(text: state.query, prompt: "Search activity") { send(.searchActivity($0)) }
        CapsuleTabs(state: state.filters) { send(.filterActivity($0)) }
      }
      content
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .cardSurface()
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Activity")
  }

  /// The lines in their groups, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, ground: .pane)
        .padding(Spacing.xxl)
    } else {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: .zero) {
          ForEach(state.groups) { group in
            GroupLabel(text: group.title)
              .padding(.top, Spacing.xl)
              .padding(.bottom, Spacing.s)
              .padding(.horizontal, ViewerLayout.paneTextInset)
            ForEach(group.lines) { line in
              ActivityLineRow(line: line, send: send)
            }
          }
        }
      }
      .scrollIndicators(.automatic)
    }
  }
}

#if DEBUG
  #Preview("Activity") {
    if case .activity(let activity) = PreviewStates.viewerActivity.pane {
      ViewerActivityPane(state: activity) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Nothing yet") {
    if case .activity(let activity) = PreviewStates.viewerActivityEmpty.pane {
      ViewerActivityPane(state: activity) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerActivity
    InteractivePreview {
      if case .activity(let activity) = state.pane {
        ViewerActivityPane(state: activity) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
