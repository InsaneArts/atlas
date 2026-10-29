import SwiftUI

/// The Activity list inside the card, with no ground of its own: the filters at the top, then what was done in the
/// helper as moments, newest first, each an app's run of work with its steps on one line under the app's icon, older
/// moments folded to one row, a quiet heading over each day before today; or why nothing shows. The sidebar's search
/// narrows it and says whose work it is. A new step fades in at the top while the rest move down, unless the person
/// has scrolled down, when it comes with no motion and what they read stays where it is (``ActivityScroll``). It scrolls in the card's one list scroll (``CardScroll``), under the small
/// screen.
struct ViewerActivityPane: View {
  /// What the pane shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// The list, or why there is nothing to list.
  var body: some View {
    Group {
      if let empty = state.empty, !state.showsFilters {
        EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .list)
      } else {
        CardScroll {
          VStack(alignment: .leading, spacing: .zero) {
            if state.showsFilters {
              filters
            }
            if let empty = state.empty {
              EmptyMessage(state: empty, size: .regular, ground: .pane, symbol: .list)
                .frame(maxWidth: .infinity)
                .padding(.vertical, Layout.emptyInset)
            } else {
              rows
            }
          }
          .padding(.horizontal, ViewerLayout.swapInset)
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.title)
    .accessibilityValue(state.meta)
  }

  /// The filters, a capsule track at the list's leading edge, scrolling sideways in a pane too narrow for it; the
  /// chosen one changes at once.
  private var filters: some View {
    ViewThatFits(in: .horizontal) {
      tabs
      ScrollView(.horizontal) {
        tabs
      }
      .scrollIndicators(.hidden)
    }
    .frame(maxWidth: .infinity, minHeight: ViewerLayout.filtersHeight, alignment: .leading)
    .padding(.top, ViewerLayout.activityTop)
  }

  /// The filters' track.
  private var tabs: some View {
    Tabs(state: state.filters, style: .segment) { filter in
      withAnimation(ActivityMotion.filter(reduceMotion: reduceMotion)) {
        send(.filterActivity(filter))
      }
    }
    .fixedSize()
  }

  /// Every day's heading and moment, keyed by its identity, so a moment in two filters stays where it is, each moment
  /// opening and folding as one (``ActivityMomentBlock``).
  private var rows: some View {
    LazyVStack(alignment: .leading, spacing: .zero) {
      ForEach(ActivityListBlock.blocks(state.items)) { block in
        self.block(block)
          .transition(ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion))
      }
    }
    .scrollTargetLayout()
    .transaction(value: state.newest) { transaction in
      // A change the person made keeps the animation it was made in, so a filter's rows wait for those leaving.
      guard transaction.animation == nil else { return }
      transaction.animation = ActivityScroll.moving(isAtTop: isAtTop, reduceMotion: reduceMotion)
    }
  }

  /// One element: a day's heading, or a moment with its steps.
  ///
  /// - Parameter block: The element.
  /// - Returns: Its view.
  @ViewBuilder private func block(_ block: ActivityListBlock) -> some View {
    switch block {
    case .day(let day):
      ListHeading(title: day.title)
        .frame(height: ViewerLayout.dayHeight)
        .padding(.top, ViewerLayout.dayAbove)
    case .moment(let moment, let rows):
      ActivityMomentBlock(moment: moment, rows: rows, send: send)
    }
  }
}

/// The pane's own measures.
private enum Layout {
  /// 48: above and below the message when a filter or a search leaves nothing.
  static let emptyInset: CGFloat = 48
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
