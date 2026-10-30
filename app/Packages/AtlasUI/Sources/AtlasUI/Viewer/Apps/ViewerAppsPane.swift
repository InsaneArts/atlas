import SwiftUI

/// The Apps list inside the card, with no ground of its own: how many apps and windows are open, then the apps as
/// cards, under a quiet heading per group while every session's apps show, or only the chosen session's own; or why
/// none shows. The sidebar's search narrows it. Cards keep their order as the front app changes; an app that opens,
/// quits or moves between groups fades in or out in place while the others close or open the gap (``AppsMotion``). It
/// scrolls in the card's one list scroll (``CardScroll``), under the small screen.
struct ViewerAppsPane: View {
  /// What the pane shows.
  let state: ViewerAppsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The apps, or why there are none.
  var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      .accessibilityElement(children: .contain)
      .accessibilityLabel("Apps")
      .accessibilityValue(state.meta)
  }

  /// The summary and the groups, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .windows)
    } else {
      CardScroll {
        VStack(alignment: .leading, spacing: .zero) {
          summary
          ForEach(state.groups) { group in
            if let title = group.title { heading(title, count: group.cards.count) }
            AppCardGrid {
              ForEach(group.cards, id: \.scrollID) { card in
                AppCard(card: card, send: send)
                  .transition(AppsMotion.card(reduceMotion: reduceMotion))
              }
            }
            .scrollTargetLayout()
            .padding(.top, group.title == nil ? ViewerLayout.appGroupBelow : .zero)
          }
        }
        .padding(.horizontal, ViewerLayout.swapInset)
        .animation(AppsMotion.layout(reduceMotion: reduceMotion), value: state.groups.map(\.cards).map { $0.map(\.id) })
      }
    }
  }

  /// How many apps are open, and beside it how many windows they have; the words swap as a whole as they change.
  private var summary: some View {
    LineSwap([state.title, state.detail]) { words in
      HStack(alignment: .firstTextBaseline, spacing: ViewerLayout.appsSummaryGap) {
        Text(verbatim: words.first ?? "")
          .font(Typography.digits(.title3, weight: .semibold))
          .tracking(LetterSpacing.headline)
          .foregroundStyle(Color.textPrimary)
        if let detail = words.last, !detail.isEmpty {
          Text(verbatim: "· \(detail)")
            .font(Typography.digits(.callout))
            .foregroundStyle(Color.textTertiary)
        }
      }
      .lineLimit(1)
    }
    .padding(.top, ViewerLayout.appsSummaryTop)
    .padding(.bottom, ViewerLayout.appsSummaryBottom)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isHeader)
  }

  /// A group's quiet heading and its count.
  ///
  /// - Parameters:
  ///   - title: The group's heading.
  ///   - count: How many apps it holds.
  /// - Returns: The heading.
  private func heading(_ title: String, count: Int) -> some View {
    ListHeading(title: title, count: count)
      .frame(height: ViewerLayout.appGroupHeight)
      .padding(.top, ViewerLayout.appGroupAbove)
      .padding(.bottom, ViewerLayout.appGroupBelow)
  }
}

#if DEBUG
  #Preview("All sessions") {
    if case .apps(let apps) = PreviewStates.viewerApps.pane {
      ViewerAppsPane(state: apps) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("One session") {
    if case .apps(let apps) = PreviewStates.viewerAppsOneSession.pane {
      ViewerAppsPane(state: apps) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerApps
    InteractivePreview {
      Text(verbatim: "Search the sidebar to narrow the cards")
    } content: {
      if case .apps(let apps) = state.pane {
        ViewerAppsPane(state: apps) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
