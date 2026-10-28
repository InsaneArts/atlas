#if DEBUG
  extension PreviewStates {
    /// Idle, with Details beside the screen.
    @MainActor public static let viewerDetails: ViewerState = {
      var state = viewerIdle
      state.top.actions.details = viewerDetailsToggle(shown: true)
      state.details = ViewerDetailsState(
        helper: [
          Fact(name: "Account", value: "Atlas"),
          Fact(name: "State", value: "Ready"),
          Fact(name: "Display", value: "1440 × 900"),
        ],
        ready: [ViewerReadyRow(id: "all", name: "Ready to act", value: "All checks pass", mark: .ready)],
        updates: "Live",
        newest: "just now",
        login: [
          Fact(name: "Logged in", value: "09:12"),
          Fact(name: "Agents", value: "Claude, Codex"),
        ]
      )
      return state
    }()

    /// Details while one check fails and one is not checked yet: only those two list, with the count of the rest.
    @MainActor public static let viewerDetailsChecks: ViewerState = {
      var state = viewerDetails
      state.details?.ready = [
        ViewerReadyRow(
          id: "Mouse and keyboard",
          name: "Mouse and keyboard",
          mark: .notReady,
          note: "Not ready. Recover… starts them again."
        ),
        ViewerReadyRow(
          id: "Save and Open dialogs",
          name: "Save and Open dialogs",
          mark: .unknown,
          note: "Not checked yet"
        ),
        ViewerReadyRow(id: "others", name: "1 more check passes", mark: .ready, isQuiet: true),
      ]
      return state
    }()

    /// Clean Up This Session's Work… asks first, over Claude Code's detail: what it quits, undoes and removes,
    /// and that the session may still be working; Cancel in blue, Clean Up in red.
    @MainActor public static let viewerCleanUp: ViewerState = {
      var state = viewerSessionDetail
      state.alert = AlertCardState(
        title: "Clean up this session's work?",
        message: """
          Quits Opie
          Undoes 1 environment change
          Removes Probe from Applications
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

    /// Claude Code's work cleaned up in part: its environment change undone and Opie quit, Probe kept as it changed
    /// since it was installed, and the copy of Opie still in use, each step's outcome above the button.
    @MainActor public static let viewerSessionCleanedUp: ViewerState = {
      var state = viewerSessionDetail
      state.details?.hostSession?.plans = []
      state.details?.hostSession?.cleanup = SessionCleanup(
        session: SessionSamples.claudeKey,
        title: "Clean Up This Session's Work…",
        stage: .finished([
          Fact(name: "Quit Opie", value: "Done"),
          Fact(name: "Undo an environment change", value: "Done"),
          Fact(name: "Remove Probe from Applications", value: "Kept: changed since it was installed"),
          Fact(name: "Remove the copy of Opie", value: "Couldn't: still in use. Try again."),
        ])
      )
      return state
    }()
  }
#endif
