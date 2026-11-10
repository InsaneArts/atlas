#if DEBUG
  extension PreviewStates {
    /// The panel while an agent works and a question waits for the person: the heading says Waiting for you, the card
    /// over the picture.
    ///
    /// - Parameter ask: The question.
    /// - Returns: The panel.
    @MainActor static func panelAsking(_ ask: AskCardState) -> PanelState {
      var state = panelWorking
      state.status = StatusSummary(label: "Waiting for you", tone: .attention, mark: .dot)
      state.ask = ask
      return state
    }

    /// Asking before a typed message is sent.
    @MainActor static let panelAskTyped = panelAsking(askTyped)
    /// Asking before a named button is clicked.
    @MainActor static let panelAskNamedClick = panelAsking(askNamedClick)
    /// Asking before a button with no name is clicked.
    @MainActor static let panelAskUnnamedClick = panelAsking(askUnnamedClick)
    /// Asking before a password is typed.
    @MainActor static let panelAskPassword = panelAsking(askPassword)
    /// Asking with Allow for This Task and Deny alone.
    @MainActor static let panelAskTaskOnly = panelAsking(askTaskOnly)

    /// An agent works under what the person allowed for its task in GitHub.
    @MainActor static let panelAllowed: PanelState = {
      var state = panelWorking
      state.allowed = [askAllowed]
      return state
    }()
  }
#endif
