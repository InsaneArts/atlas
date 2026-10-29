#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The helper needing the person, the sidebar by state (``sessionsByState``), the recovery alert set aside,
    /// and the unfinished click on the footer's line.
    @MainActor public static let viewerSidebarByState: ViewerState = {
      var state = viewerNeedsYou
      state.alert = nil
      state.sidebar.helper = viewerHelperNeedsYou(alertUp: false)
      state.sidebar.sessions = sessionsByState
      state.footer.latest = viewerActivityNeedsYou.footer.latest
      return state
    }()

    /// The sidebar by state with Ended folded under its count.
    @MainActor public static let viewerSidebarEndedFolded: ViewerState = {
      var state = viewerSidebarByState
      state.sidebar.sessions.endedShown = false
      if let ended = state.sidebar.sessions.groups.firstIndex(where: { $0.section == .ended }) {
        state.sidebar.sessions.groups[ended].rows = []
      }
      return state
    }()

    /// The sidebar's search open on "opie": Claude Code, whose line names Opie, under Working, and Cursor · opie under
    /// Idle; Terminal and Codex are filtered out.
    @MainActor public static let viewerSidebarSearch: ViewerState = {
      var state = viewerSessions
      state.sidebar.search = ViewerSearchState(isOpen: true, query: "opie")
      state.sidebar.sessions.groups = [
        ViewerSessionGroup(.working, rows: [sessionRows[0]]),
        ViewerSessionGroup(.idle, rows: [sessionRows[2]]),
      ]
      return state
    }()

    /// The sidebar hidden: the card takes the window, its header past the window buttons and the toggle.
    @MainActor public static let viewerSidebarHidden: ViewerState = {
      var state = viewerSessionsEnded
      state.sidebar.mode = .hidden
      return state
    }()

    /// The sidebar shown whole over the card, as the toggle shows it in a window too narrow for it; a wide window has
    /// room for it beside the card, so only the narrow renders draw it over.
    @MainActor public static let viewerSidebarOverlay: ViewerState = {
      var state = viewerSidebarEndedFolded
      state.sidebar.overlay = true
      return state
    }()

    /// More sessions than the sidebar has room for: Claude Code under Needs you, three under Working, six under Idle
    /// and Ended open on four, so the sessions scroll in their own room above the Library.
    @MainActor public static let viewerSidebarMany: ViewerState = {
      var state = viewerSidebarByState
      let codex = SessionSamples.codexIcon
      let cursor = SessionSamples.cursorIcon
      let claude = SessionSamples.claudeIcon
      let terminal = SessionSamples.terminalIcon
      let pi = SessionSamples.piIcon
      let live: [(String, RowIcon, String, String, SessionBadge?)] = [
        ("codex-b", codex, "Codex · B", "Clicked Save · 11:08", .waiting),
        ("claude-docs", claude, "Claude Code · docs", "Typing in Notes · 12 s", .waiting),
        ("cursor-site", cursor, "Cursor · site", "Opened Safari · 2 min ago", nil),
        ("claude-api", claude, "Claude Code · api", "Took a screenshot · 5 min ago", nil),
        ("codex-c", codex, "Codex · C", "Scrolled in Opie · 9 min ago", nil),
        ("terminal-2", terminal, "Terminal", "Pressed ⌘S · 12 min ago", nil),
      ]
      let ended: [(String, RowIcon, String, String)] = [
        ("pi-2", pi, "Pi · notes", "Ended 25 min ago"),
        ("codex-d", codex, "Codex · D", "Ended 40 min ago"),
        ("cursor-old", cursor, "Cursor · old", "Ended 1 h ago"),
      ]
      let row = { (sample: (String, RowIcon, String, String, SessionBadge?)) in
        viewerSessionRow(sample.0, icon: sample.1, name: sample.2, line: sample.3, badge: sample.4, facts: [])
      }
      let gone = { (sample: (String, RowIcon, String, String)) in
        viewerSessionRow(sample.0, icon: sample.1, name: sample.2, line: sample.3, facts: [])
      }
      state.sidebar.sessions = ViewerSessionsState(
        icons: sessionsByState.icons,
        liveCount: sessionRows.count + live.count,
        groups: [
          ViewerSessionGroup(.needsYou, rows: [needsYouRows[0]]),
          ViewerSessionGroup(.working, rows: [needsYouRows[1]] + live.prefix(2).map(row)),
          ViewerSessionGroup(.idle, rows: Array(sessionRows.suffix(2)) + live.suffix(4).map(row)),
          ViewerSessionGroup(.ended, rows: [piEndedRow] + ended.map(gone)),
        ],
        endedCount: ended.count + 1,
        endedShown: true
      )
      return state
    }()

    /// `state` as a window `size` draws it, every part given way as a window first opened at that size would.
    ///
    /// - Parameters:
    ///   - state: A main window sample.
    ///   - size: The window's size.
    /// - Returns: The sample, fitted.
    @MainActor public static func viewer(_ state: ViewerState, fittedTo size: CGSize) -> ViewerState {
      state.fitted(ViewerFit().refitted(to: size, isHidden: state.sidebar.mode == .hidden))
    }
  }
#endif
