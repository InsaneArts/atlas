#if DEBUG
  extension PreviewStates {
    /// Every approval window sample, in the order the previews and the renders list them.
    @MainActor public static var approval: [PreviewState<ApprovalState>] {
      [
        PreviewState(name: "Reading", value: .loading),
        PreviewState(name: "Add", value: approvalAdd),
        PreviewState(name: "Remove", value: approvalRemove),
        PreviewState(name: "Uninstall", value: approvalUninstall),
        PreviewState(name: "Allowing", value: approvalAllowing),
        PreviewState(name: "Allowed", value: approvalAllowed),
        PreviewState(name: "Not allowed", value: approvalRefused),
        PreviewState(name: "Answered before", value: approvalAnsweredEarlier),
        PreviewState(name: "Expired", value: approvalExpired),
        PreviewState(name: "Failed", value: approvalFailed),
      ]
    }

    /// Adding the VM `work`, asked from an agent's session.
    static let approvalAdd = ApprovalState(
      operation: .add,
      desktop: "work",
      session: "Claude Code in atlas",
      phase: .asking
    )

    /// Removing the VM `work`: a destructive alert, Don't Allow first.
    static let approvalRemove = ApprovalState(
      operation: .remove,
      desktop: "work",
      session: "a terminal",
      phase: .asking
    )

    /// Uninstalling Atlas with its VMs.
    static let approvalUninstall = ApprovalState(
      operation: .uninstall,
      desktop: "work",
      session: "a terminal",
      phase: .asking
    )

    /// Adding while the answer is kept.
    static let approvalAllowing: ApprovalState = {
      var state = approvalAdd
      state.phase = .answering(.allow)
      return state
    }()

    /// Adding once allowed, just before the window closes.
    static let approvalAllowed: ApprovalState = {
      var state = approvalAdd
      state.phase = .answered(allowed: true)
      return state
    }()

    /// Removing once refused, just before the window closes.
    static let approvalRefused: ApprovalState = {
      var state = approvalRemove
      state.phase = .answered(allowed: false)
      return state
    }()

    /// Adding opened again once it was allowed and went ahead: nothing is left to answer.
    static let approvalAnsweredEarlier: ApprovalState = {
      var state = approvalAdd
      state.phase = .answeredEarlier(.used)
      return state
    }()

    /// Adding after its time ran out.
    static let approvalExpired: ApprovalState = {
      var state = approvalAdd
      state.phase = .expired
      return state
    }()

    /// Adding whose answer the command line did not take.
    static let approvalFailed: ApprovalState = {
      var state = approvalAdd
      state.phase = .failed("This request was answered already.")
      return state
    }()
  }
#endif
