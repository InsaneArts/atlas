import SwiftUI

/// The Activity list inside the card, with no ground of its own: the filters at the top, then what was done in the
/// helper as an outline, newest first, of sessions, each app's runs in them and each run's steps, with pictures shown
/// in place and a quiet heading over each day before today; or why nothing shows. The sidebar's search
/// narrows it and says whose work it is. A new step fades in at the top while the rest move down, unless the person
/// has scrolled down, when it comes with no motion and what they read stays where it is (``ActivityScroll``).
/// It scrolls in the card's one list scroll (``CardScroll``), under the small screen.
struct ViewerActivityPane: View {
  /// What the pane shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.paneShows) private var paneShows
  @Environment(\.motionReduced) private var reduceMotion

  /// The list, or why there is nothing to list; one gives way to the other as a list gives way to another
  /// (``ActivityMotion/swap(isAtTop:reduceMotion:)``), each fading in the same place.
  var body: some View {
    ZStack(alignment: .top) {
      if let empty = state.empty, !state.showsFilters {
        EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .list)
          .disabled(!paneShows)
          .transition(ActivityMotion.swap(isAtTop: true, reduceMotion: reduceMotion))
      } else {
        CardScroll(list: state.list) {
          ActivityListContent(state: state, send: send)
        }
        .transition(ActivityMotion.swap(isAtTop: true, reduceMotion: reduceMotion))
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
      Text(verbatim: "Open or fold a session or a run, or pick a filter")
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
