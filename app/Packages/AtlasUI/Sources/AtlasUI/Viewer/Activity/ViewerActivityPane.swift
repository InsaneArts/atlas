import SwiftUI

/// The Activity list inside the card, with no ground of its own: the filters at the top, then what was done in the
/// helper as moments, newest first, each an app's run of work with its steps on one line under the app's icon, older
/// moments folded to one row, a quiet heading over each day before today; or why nothing shows. The sidebar's search
/// narrows it and says whose work it is. A new step fades in at the top while the rest move down, unless the person
/// has scrolled down, when it comes with no motion and what they read stays where it is (``ActivityScroll``).
/// It scrolls in the card's one list scroll (``CardScroll``), under the small screen.
struct ViewerActivityPane: View {
  /// What the pane shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The list, or why there is nothing to list.
  var body: some View {
    Group {
      if let empty = state.empty, !state.showsFilters {
        EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .list)
      } else {
        CardScroll {
          ActivityListContent(state: state, send: send)
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.title)
    .accessibilityValue(state.meta)
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

  #Preview("All sessions") {
    if case .activity(let activity) = PreviewStates.viewerActivityBySession.pane {
      ViewerActivityPane(state: activity) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Folded") {
    if case .activity(let activity) = PreviewStates.viewerActivityMany.pane {
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
      Text(verbatim: "Open or fold a moment, or pick a filter")
    } content: {
      if case .activity(let activity) = state.pane {
        ViewerActivityPane(state: activity) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
