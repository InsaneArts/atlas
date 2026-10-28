import SwiftUI

/// The Activity tab, a log you can scan: what was done in the helper, newest first, under Now, Earlier today and the
/// days before, or under each host session while it shows every session's work, their headers pinned as the list
/// scrolls, with a search over what was done and filters once the log holds anything; or why nothing shows, on the
/// pane's own ground. With one session chosen, its name and icon are the pane's title. A new line fades in
/// at the top while the others move down, unless the person has scrolled down, when it comes with no motion and what
/// they read stays where it is (``ActivityScroll``).
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
  @Environment(\.paneSettled) private var paneSettled

  /// The header with the search and filters, over the lines or why there are none.
  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: state.title, meta: state.meta, icon: state.titleIcon) {
        if state.isSearchable {
          SearchField(text: state.query, prompt: "Search activity", focusRequests: state.findRequests) {
            send(.searchActivity($0))
          }
          Tabs(state: state.filters, style: .segment) { send(.filterActivity($0)) }
        }
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
      EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .list)
    } else {
      ScrollView {
        lines
      }
      .scrollIndicators(paneSettled ? .automatic : .hidden)
      .scrollPosition(id: anchor, anchor: .top)
      .clearsMiniScreen()
    }
  }

  /// The groups, by session or by time, each under its pinned header.
  private var lines: some View {
    LazyVStack(alignment: .leading, spacing: .zero, pinnedViews: .sectionHeaders) {
      if state.sessions.isEmpty {
        ForEach(state.groups) { group in
          Section {
            ForEach(group.lines) { line in
              ActivityLineRow(line: line, send: send)
                .environment(\.drawsListSeparator, line.id != group.lines.first?.id)
                .transition(ActivityScroll.arrival(isAtTop: isAtTop))
            }
          } header: {
            PinnedHeader(title: group.title)
          }
        }
      } else {
        ForEach(state.sessions) { group in
          ActivitySessionSection(group: group, isAtTop: isAtTop, send: send)
        }
      }
    }
    .scrollTargetLayout()
    .onGeometryChange(for: Bool.self) { proxy in
      proxy.frame(in: .scrollView).minY >= -Spacing.hairline
    } action: { atTop in
      isAtTop = atTop
    }
    .animation(ActivityScroll.moving(isAtTop: isAtTop, reduceMotion: reduceMotion), value: newest)
  }

  /// The list's scroll position: none at the top, so it stays there as lines arrive, else the line scrolled to.
  private var anchor: Binding<String?> {
    Binding(
      get: { ActivityScroll.anchor(scrolledTo: scrolledTo, isAtTop: isAtTop) },
      set: { scrolledTo = $0 }
    )
  }

  /// The newest line, whose change is a new line arriving: the first session's while the pane lists sessions.
  private var newest: String? {
    state.sessions.isEmpty ? state.groups.first?.lines.first?.id : state.sessions.first?.lines.first?.id
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

  #Preview("By session") {
    if case .activity(let activity) = PreviewStates.viewerActivityBySession.pane {
      ViewerActivityPane(state: activity) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("One session") {
    if case .activity(let activity) = PreviewStates.viewerActivityOneSession.pane {
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
