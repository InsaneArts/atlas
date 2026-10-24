import SwiftUI

/// The Activity tab, a log you can scan (v3 V8): what was done in the helper, newest first, under Now, Earlier today
/// and the days before, their headers pinned as the list scrolls, with a search over what was done and filters for
/// agents, the person and problems; or why nothing shows. A new line pushes in at the top while the others move down;
/// the list stays where the person scrolled it unless it sits at its top (X10).
struct ViewerActivityPane: View {
  /// What the pane shows.
  let state: ViewerActivityState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The line the person last scrolled to the top of the list.
  @State private var scrolledTo: String?
  /// Whether the list sits at its very top.
  @State private var isAtTop = true
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The header with the search and filters, over the lines or why there are none.
  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Activity", meta: state.meta) {
        SearchField(text: state.query, prompt: "Search activity") { send(.searchActivity($0)) }
        Tabs(state: state.filters, style: .segment) { send(.filterActivity($0)) }
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
      EmptyMessage(state: empty, size: .stage, symbol: .list)
    } else {
      ScrollView {
        lines
      }
      .scrollIndicators(.automatic)
      .scrollPosition(id: anchor, anchor: .top)
    }
  }

  /// The groups, each under its pinned header.
  private var lines: some View {
    LazyVStack(alignment: .leading, spacing: .zero, pinnedViews: .sectionHeaders) {
      ForEach(state.groups) { group in
        Section {
          ForEach(group.lines) { line in
            ActivityLineRow(line: line, send: send)
              .environment(\.drawsListSeparator, line.id != group.lines.first?.id)
              .transition(arrival)
          }
        } header: {
          PinnedHeader(title: group.title)
        }
      }
    }
    .scrollTargetLayout()
    .onGeometryChange(for: Bool.self) { proxy in
      proxy.frame(in: .scrollView).minY >= -Spacing.hairline
    } action: { atTop in
      isAtTop = atTop
    }
    // The others move down on the state spring as a new line arrives; under Reduce Motion they jump.
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: newest)
  }

  /// The list's scroll position: none at the top, so it stays there as lines arrive, else the line scrolled to.
  private var anchor: Binding<String?> {
    Binding(
      get: { ActivityScroll.anchor(scrolledTo: scrolledTo, isAtTop: isAtTop) },
      set: { scrolledTo = $0 }
    )
  }

  /// A new line pushes in from the top on a small spring; under Reduce Motion it fades in.
  private var arrival: AnyTransition {
    Motion.transition(.push(from: .top), reduceMotion: reduceMotion)
      .animation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced))
  }

  /// The newest line, whose change is a new line arriving.
  private var newest: String? {
    state.groups.first?.lines.first?.id
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
