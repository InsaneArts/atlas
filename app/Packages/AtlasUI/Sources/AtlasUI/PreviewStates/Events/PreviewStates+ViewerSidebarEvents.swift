#if DEBUG
  extension PreviewStates {
    /// The sample a sidebar event leads to: the sidebar hidden or shown, or put away from over the card, Ended opened or
    /// folded, and the search opened, typed in or closed, keeping the sessions whose name or line holds the query, and
    /// narrowing the list shown.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor static func viewerSidebar(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      var next = state
      switch event {
      case .toggleSidebar where state.sidebar.overlay, .dismissSidebarOverlay:
        next.sidebar.overlay = false
      case .toggleSidebar:
        next.sidebar.mode = state.sidebar.mode == .hidden ? .full : .hidden
      case .toggleEnded:
        next.sidebar.sessions.endedShown.toggle()
        let shown = next.sidebar.sessions.endedShown
        if let ended = next.sidebar.sessions.groups.firstIndex(where: { $0.section == .ended }) {
          next.sidebar.sessions.groups[ended].rows = shown ? [piEndedRow] : []
        }
      case .openSearch:
        next.sidebar.search.isOpen = true
        next.sidebar.search.focusRequests += 1
      case .search(let query):
        next.sidebar.search.query = query
        next.sidebar.sessions.groups = sessionGroups(matching: query)
        next = viewerPane(next, searching: query)
      case .closeSearch:
        next.sidebar.search = ViewerSearchState()
        next.sidebar.sessions.groups = allSessions.groups
        next = viewerPane(next, searching: "")
      default:
        break
      }
      return next
    }

    /// The live samples' groups, keeping the sessions whose name or line holds `query`, and no group left empty.
    ///
    /// - Parameter query: The query; empty keeps every session.
    /// - Returns: The groups.
    private static func sessionGroups(matching query: String) -> [ViewerSessionGroup] {
      guard !query.isEmpty else { return allSessions.groups }
      return allSessions.groups.compactMap { group in
        let rows = group.rows.filter { row in
          row.name.localizedCaseInsensitiveContains(query) || row.line.localizedCaseInsensitiveContains(query)
        }
        return rows.isEmpty ? nil : ViewerSessionGroup(group.section, rows: rows)
      }
    }
  }
#endif
