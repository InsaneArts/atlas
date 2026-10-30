#if DEBUG
  import SwiftUI

  /// The sidebar's motion, to watch in a preview: buttons that move Claude Code from Working to Idle to Ended and
  /// back, bring Codex under Needs you and take it away again, and open Ended, hide the sidebar and open the search,
  /// each as the window would, beside a card.
  struct SidebarMotionSample: View {
    /// Where Claude Code stands: 0 working, 1 idle, 2 ended.
    @State private var claudeStep = 0
    /// Whether Codex needs the person.
    @State private var codexNeedsYou = false
    /// The sidebar as drawn.
    @State private var sidebar = PreviewStates.viewerSessionsEnded.sidebar

    /// The buttons over the sidebar beside a card.
    var body: some View {
      InteractivePreview {
        Button(String("Move Claude Code")) {
          claudeStep = (claudeStep + 1) % 3
          regroup()
        }
        Button(String("Needs you")) {
          codexNeedsYou.toggle()
          regroup()
        }
        Button(String("Ended")) { apply(.toggleEnded) }
        Button(String("Sidebar")) { apply(.toggleSidebar) }
        Button(String("Search")) { apply(sidebar.search.isOpen ? .closeSearch : .openSearch) }
      } content: {
        SidebarSplit(sidebar: sidebar, isFullScreen: false, send: apply) { slide in
          ViewerCard(
            header: PreviewStates.viewerSessions.header,
            headerLeading: ViewerLayout.headerLeading(mode: slide.drawn, isFullScreen: false),
            footer: PreviewStates.viewerSessions.footer
          ) { _ in
          } stage: {
            Color.clear
          }
          .padding(ViewerLayout.cardInset)
        }
        .frame(width: SampleWidth.column * 4, height: SampleWidth.column * 3)
        .background(Color.groundRecessed)
      }
    }

    /// Applies a sidebar event as the window would.
    ///
    /// - Parameter event: What was clicked.
    private func apply(_ event: ViewerEvent) {
      var state = PreviewStates.viewerSessionsEnded
      state.sidebar = sidebar
      sidebar = PreviewStates.viewer(state, applying: event).sidebar
      regroup()
    }

    /// The sections as Claude Code's and Codex's states put them.
    private func regroup() {
      let rows = PreviewStates.sessionRows
      var claude = rows[0]
      var codex = rows[1]
      if codexNeedsYou {
        codex.badge = .needsYou
        codex.line = "A click may not have finished"
      }
      claude.badge = claudeStep == 0 ? .acting : nil
      let lines = ["Clicking Show Center in Opie", "Clicked Show Center in Opie · just now", "Ended just now"]
      claude.line = lines[claudeStep]
      var groups: [ViewerSessionGroup] = []
      if codexNeedsYou { groups.append(ViewerSessionGroup(.needsYou, rows: [codex])) }
      let working = (claudeStep == 0 ? [claude] : []) + (codexNeedsYou ? [] : [codex])
      if !working.isEmpty { groups.append(ViewerSessionGroup(.working, rows: working)) }
      groups.append(ViewerSessionGroup(.idle, rows: (claudeStep == 1 ? [claude] : []) + rows.suffix(2)))
      let ended = (claudeStep == 2 ? [claude] : []) + [PreviewStates.piEndedRow]
      groups.append(ViewerSessionGroup(.ended, rows: sidebar.sessions.endedShown ? ended : []))
      sidebar.sessions.groups = groups
      sidebar.sessions.endedCount = ended.count
    }
  }
#endif
