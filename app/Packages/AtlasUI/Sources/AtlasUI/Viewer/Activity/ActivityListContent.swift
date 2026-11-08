import SwiftUI

/// The Activity scroll's content, reading its actual position below CardScroll's environment boundary.
struct ActivityListContent: View {
  /// What the list shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop
  /// The card's width as last measured, which the outline gives way to.
  @State private var width = CGFloat.infinity
  /// The days that held more sessions than they showed as last drawn, so the update that shows the rest tells them to
  /// come in with it.
  @State private var drawnFolded: Set<String> = []
  /// The sessions that arrived at the top while the person was scrolled down, which wait there until they scroll back.
  @State private var arrivals = ActivityArrivals()

  /// 48: above and below an empty filtered list's message.
  private static let emptyInset: CGFloat = 48

  /// The filters and the outline inside the shared scroll, with a line at the top for the sessions waiting there.
  var body: some View {
    let waiting = arrivals.waiting(in: state, isAtTop: isAtTop)
    return VStack(alignment: .leading, spacing: .zero) {
      if state.showsFilters {
        filters
      }
      if let empty = state.empty {
        EmptyMessage(state: empty, size: .regular, ground: .pane, symbol: .list)
          .frame(maxWidth: .infinity)
          .padding(.vertical, Self.emptyInset)
      } else {
        rows(ActivityArrivals.shown(state.days, without: waiting))
      }
    }
    .padding(.horizontal, ViewerLayout.swapInset)
    .listWaiting(ActivityArrivals.words(waiting.count))
    .onChange(of: state, initial: true) { _, state in
      arrivals.meet(state, isAtTop: isAtTop)
    }
    .onChange(of: isAtTop) { _, isAtTop in
      guard isAtTop else { return }
      withAnimation(ActivityScroll.moving(isAtTop: true, reduceMotion: reduceMotion)) {
        arrivals.release()
      }
    }
    .onGeometryChange(for: CGFloat.self) {
      $0.size.width
    } action: {
      width = $0
    }
    .environment(\.outlineWidth, width)
  }

  /// A whole capsule track: narrow panes omit visible counts and use smaller insets, keeping the counts spoken.
  /// Extremely narrow panes scroll with a visible indicator. The thumb still slides and drags in either size.
  private var filters: some View {
    ViewThatFits(in: .horizontal) {
      tabs
        .layoutFrame("activity filters")
      tabs
        .controlSize(.small)
        .layoutFrame("activity filters")
      ScrollView(.horizontal) {
        tabs
          .controlSize(.small)
          .layoutFrame("activity filters")
      }
      .scrollIndicators(.visible)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.vertical, ViewerLayout.filtersAround)
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

  /// The days holding more sessions than they show.
  private var foldedDays: Set<String> {
    Set(
      state.days
        .filter { $0.more != nil }
        .map(\.id)
    )
  }

  /// Every day's heading, its sessions with a hairline between two, and the row for the rest of a long day, each keyed
  /// by its identity so a session in two filters stays where it is, and each on the card's ground
  /// (``SwiftUI/View/outlineGround()``), so what a disclosure brings in is uncovered as the rows below move down. A
  /// session's row and runs are rows of this stack, so the list holds the place of whichever the person scrolled to.
  ///
  /// - Parameter days: The days shown, without the sessions waiting at the top.
  /// - Returns: The rows.
  private func rows(_ days: [ViewerActivityDay]) -> some View {
    LazyVStack(alignment: .leading, spacing: .zero) {
      ForEach(Array(days.enumerated()), id: \.element.id) { place, day in
        if !day.title.isEmpty {
          // On the rows' content edge, so it lines up with their chevrons; directly under the filters it needs no room.
          Text(verbatim: day.title)
            .font(Typography.font(.body, weight: .semibold))
            .foregroundStyle(Color.textSecondary)
            .lineLimit(1)
            .frame(height: ViewerLayout.dayHeight)
            .padding(.leading, ViewerLayout.outlineInset)
            .padding(.top, place == 0 && state.showsFilters ? .zero : ViewerLayout.dayAbove)
            .accessibilityAddTraits(.isHeader)
            .outlineGround()
            .id("day.\(day.id)")
            .transition(ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion))
        }
        ForEach(Array(day.sessions.enumerated()), id: \.element.id) { index, session in
          if index > 0 {
            SeparatorLine()
              .padding(.leading, ViewerLayout.outlineText(level: 0))
              .padding(.trailing, ViewerLayout.outlineInset)
              .padding(.vertical, ViewerLayout.sessionSeparatorGap)
              .outlineGround()
              .id("separator.\(session.id)")
          }
          // The block names and moves its own rows, which stand in this stack themselves.
          ActivitySessionBlock(
            session: session,
            revealing: day.more == nil && drawnFolded.contains(day.id),
            send: send
          )
        }
        if let more = day.more {
          ActivityMoreSessionsRow(more: more, send: send)
            .outlineGround()
            .id("more.\(more.day)")
        }
      }
    }
    .scrollTargetLayout()
    .onChange(of: foldedDays, initial: true) { _, days in drawnFolded = days }
    .transaction(value: state.newest) { transaction in
      // A change the person made keeps the animation it was made in, so a filter's rows wait for those leaving.
      transaction.animation = Motion.layout(
        transaction.animation ?? ActivityScroll.moving(isAtTop: isAtTop, reduceMotion: reduceMotion),
        reduceMotion: reduceMotion
      )
    }
  }
}
