#if DEBUG
  import Foundation

  extension PreviewStates {
    /// The desktops the picker lists with two set up, work shown: Second Login working, work working too.
    static let desktopItems = [
      DesktopItem(id: "second-login", title: "Second Login", word: "Idle", tone: .quiet),
      DesktopItem(id: "work", title: "work", word: "Working", tone: .active, mark: .pulsing, isViewed: true),
    ]

    /// The session on Second Login while work is shown: Codex typing in Notes in notes-sync.
    static let secondLoginRow = viewerSessionRow(
      "c0de5ec0dd02",
      icon: SessionSamples.codexIcon,
      name: "notes-sync",
      line: "On Second Login · Typing in Notes",
      badge: .acting,
      facts: ["Host: Codex", "Folder: notes-sync", "Desktop: Second Login"]
    )

    /// The main window with two desktops, showing work: the foot row names the VM with its chevron, the header says
    /// the session has work, a session waits for work, a session on Second Login says so under Working, and the row's
    /// right-click menu names the desktop, with no Switch or Log Out, which are Second Login's.
    @MainActor public static let viewerTwoDesktops: ViewerState = {
      var state = viewerSessions
      state.header = ViewerHeaderState(who: SessionSamples.claudeTitle, what: "Claude Code · has work")
      state.sidebar.helper.line = "VM work · 1440 × 900"
      state.sidebar.helper.desktops = desktopItems
      state.sidebar.helper.menu = vmHelperMenu(state.sidebar.helper.menu)
      state.sidebar.sessions.liveCount += 1
      state.sidebar.sessions.groups = state.sidebar.sessions.groups.map { group in
        var waiting = group
        waiting.rows = group.rows.map(waitingForWork)
        if group.section == .working { waiting.rows.append(secondLoginRow) }
        return waiting
      }
      return state
    }()

    /// The panel with two desktops, showing work: the popup beside ⋯ names it.
    @MainActor static let panelTwoDesktops: PanelState = {
      var state = panelWorking
      state.desktop = PanelDesktopPicker(title: "work", items: desktopItems)
      return state
    }()

    /// The helper row's menu as it reads with work shown: its first fact names the desktop, Settings is Desktop
    /// Settings…, and the last section, Switch and Log Out, is left out.
    ///
    /// - Parameter menu: Second Login's menu.
    /// - Returns: The VM's.
    static func vmHelperMenu(_ menu: [ViewerMenuSection]) -> [ViewerMenuSection] {
      menu.dropLast()
        .map { section in
          var named = section
          named.items = section.items.map { item in
            var renamed = item
            if item.title == "Account: Atlas" { renamed.title = "Desktop: work" }
            if item.title == "Open Settings on Helper…" { renamed.title = "Desktop Settings…" }
            return renamed
          }
          return named
        }
    }

    /// A session row as it reads with work shown: waiting for work rather than the helper.
    ///
    /// - Parameter row: The row as Second Login's.
    /// - Returns: The row.
    static func waitingForWork(_ row: ViewerSessionRow) -> ViewerSessionRow {
      let helper = "Waiting for the helper"
      let work = "Waiting for work"
      var shown = row
      shown.line = row.line.replacingOccurrences(of: helper, with: work)
      shown.help = row.help.replacingOccurrences(of: helper, with: work)
      shown.accessibilityLabel = row.accessibilityLabel.replacingOccurrences(of: helper, with: work)
      return shown
    }
  }
#endif
