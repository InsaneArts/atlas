import SwiftUI

/// The Activity scroll's content, reading its actual position below CardScroll's environment boundary.
struct ActivityListContent: View {
  /// What the list shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// 48: above and below an empty filtered list's message.
  private static let emptyInset: CGFloat = 48

  /// The filters and moments inside the shared scroll.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      if state.showsFilters {
        filters
      }
      if let empty = state.empty {
        EmptyMessage(state: empty, size: .regular, ground: .pane, symbol: .list)
          .frame(maxWidth: .infinity)
          .padding(.vertical, Self.emptyInset)
      } else {
        rows
      }
    }
    .padding(.horizontal, ViewerLayout.swapInset)
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
