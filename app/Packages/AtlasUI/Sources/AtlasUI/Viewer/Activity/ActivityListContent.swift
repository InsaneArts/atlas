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

  /// Every day's heading, its sessions with a hairline between two, and the row for the rest of a long day, each keyed
  /// by its identity so a session in two filters stays where it is.
  private var rows: some View {
    LazyVStack(alignment: .leading, spacing: .zero) {
      ForEach(state.days) { day in
        if !day.title.isEmpty {
          Text(verbatim: day.title)
            .font(Typography.font(.body, weight: .semibold))
            .foregroundStyle(Color.textSecondary)
            .lineLimit(1)
            .frame(height: ViewerLayout.dayHeight)
            .padding(.top, ViewerLayout.dayAbove)
            .accessibilityAddTraits(.isHeader)
            .id("day.\(day.id)")
            .transition(ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion))
        }
        ForEach(Array(day.sessions.enumerated()), id: \.element.id) { index, session in
          if index > 0 {
            SeparatorLine()
              .padding(.vertical, ViewerLayout.sessionSeparatorGap)
          }
          ActivitySessionBlock(session: session, send: send)
            .id("session.\(session.id)")
            .transition(ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion))
        }
        if let more = day.more {
          ActivityMoreSessionsRow(more: more, send: send)
            .id("more.\(more.day)")
        }
      }
    }
    .scrollTargetLayout()
    .transaction(value: state.newest) { transaction in
      // A change the person made keeps the animation it was made in, so a filter's rows wait for those leaving.
      transaction.animation = Motion.layout(
        transaction.animation ?? ActivityScroll.moving(isAtTop: isAtTop, reduceMotion: reduceMotion),
        reduceMotion: reduceMotion
      )
    }
  }
}
