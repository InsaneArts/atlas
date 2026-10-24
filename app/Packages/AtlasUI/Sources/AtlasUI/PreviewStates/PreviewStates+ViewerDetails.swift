#if DEBUG
  extension PreviewStates {
    /// Idle, with Details beside the screen (MacDetails).
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
        session: [
          Fact(name: "Logged in", value: "09:12"),
          Fact(name: "Agents", value: "Claude Code, Codex"),
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
          note: "Mouse and keyboard aren't ready."
        ),
        ViewerReadyRow(
          id: "Save and Open dialogs",
          name: "Save and Open dialogs",
          mark: .unknown,
          note: "Not checked yet"
        ),
        ViewerReadyRow(id: "others", name: "1 other passes", isQuiet: true),
      ]
      return state
    }()
  }
#endif
