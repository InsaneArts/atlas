#if DEBUG
  extension PreviewStates {
    /// Clean Up This Session's Work… asks first, chosen from Claude Code's menu: what it quits, undoes, moves aside and
    /// removes, and that the session may still be working; Cancel in blue, Clean Up in red.
    @MainActor public static let viewerCleanUp: ViewerState = {
      var state = viewerSessionChosen
      state.sidebar.helper = viewerHelperReady(logsOut: false, alertUp: true)
      state.alert = AlertCardState(
        title: "Clean up this session's work?",
        message: """
          Quits Opie
          Undoes 1 environment change
          Moves Probe aside in the helper; removed after 7 days
          Removes 1 copy

          This session may still be working.
          """,
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Clean Up", role: .destructive),
        ]
      )
      return state
    }()

    /// Claude Code's work being cleaned up: its row says so, with a spinner in its badge's place, and its menu's
    /// Clean Up waits.
    @MainActor public static let viewerSessionCleaningUp: ViewerState = {
      var state = viewerSessionChosen
      state.sidebar.sessions.groups[0].rows[0] = viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: "Cleaning up…",
        facts: claudeRecordFacts,
        cleanup: false,
        isCleaningUp: true
      )
      return state
    }()

    /// Claude Code's work cleaned up in part: the footer says the cleanup didn't finish, and its row's tooltip lists
    /// each step: Opie quit, Probe kept as it changed since it was installed, and the copy of Opie still in use.
    @MainActor public static let viewerSessionCleanedUp: ViewerState = {
      var state = viewerSessionChosen
      let notice = "The cleanup didn't finish. The session's tooltip lists each step."
      state.footer.latest = ViewerLatestLine(id: "notice-\(notice)", words: notice, announces: true)
      state.sidebar.sessions.groups[0].rows[0] = viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: SessionSamples.claudeDoing,
        badge: .acting,
        facts: claudeRecordFacts.filter { !$0.hasPrefix("Environment plan") },
        cleanup: true,
        receipt: [
          "Quit Opie: Done",
          "Undo an environment change: Done",
          "Remove Probe from Applications: Kept: changed since it was installed",
          "Remove the copy of Opie: Couldn't: still in use. Try again.",
        ]
      )
      return state
    }()

    /// Pi · notes-sync chosen after it ended: its row stays listed under Ended, folded, its tooltip and menu saying
    /// when it started and ended.
    @MainActor public static let viewerSessionEnded: ViewerState = {
      var state = viewerSessionChosen
      state.sidebar.sessions = allSessions
      state.sidebar.sessions.chosen = .one(SessionSamples.piKey)
      // Ended stays folded, its one chosen session still listed under it.
      state.sidebar.sessions.groups.append(ViewerSessionGroup(.ended, rows: [piEndedRow]))
      state.sidebar.sessions.endedCount = 1
      // Claude Code opened the one app open, so Pi's Apps holds none.
      state.sidebar.library = viewerLibrary(counts: [.apps: 0])
      return state
    }()

    /// The helper line while one check fails and one is not checked yet: in orange, saying the helper needs setup, its
    /// tooltip naming both, with the count of the rest.
    @MainActor public static let viewerHelperNeedsSetup: ViewerState = {
      var state = viewerIdle
      state.sidebar.helper = ViewerHelperLine(
        tone: .attention,
        words: "Helper needs setup · 1 check",
        help: viewerHelperHelp(
          ready: [
            "Mouse and keyboard: Not ready. Recover… starts them again.",
            "Save and Open dialogs: Not checked yet",
            "1 more check passes",
          ],
          login: ["Logged in: 09:12", "Agents: Claude, Codex"]
        ),
        menu: viewerHelperMenu(logsOut: true)
      )
      return state
    }()
  }
#endif
