extension ApprovalState {
  /// The alert the window draws: what is asked as its title, who asked and what it means as its message, Allow and
  /// Don't Allow as its buttons, and where the answer stands in place. A destructive request asks as a destructive
  /// alert does, Don't Allow its default. Once nothing is left to answer, its one way out closes the window.
  public var card: AlertCardState<ApprovalChoice> {
    switch phase {
    case .loading:
      AlertCardState(
        title: "Reading the request…",
        message: "Atlas shows what it asks in a moment.",
        buttons: [AlertCardButton(id: .refuse, title: Self.refuseWord, role: .safeCancel)]
      )
    case .asking:
      asked
    case .answering(let choice):
      AlertCardState(
        title: asked.title,
        message: asked.message,
        buttons: asked.buttons,
        phase: .running(label: choice == .allow ? "Allowing…" : "Not Allowing…")
      )
    case .answered(let allowed):
      AlertCardState(
        title: asked.title,
        message: asked.message,
        buttons: asked.buttons.map { button in
          button.id == .refuse ? AlertCardButton(id: .close, title: button.title, role: button.role) : button
        },
        phase: .done(
          title: allowed ? "Allowed" : "Not allowed",
          message: allowed ? goingOn : "Nothing changes.",
          button: "Done"
        )
      )
    case .answeredEarlier(let answer):
      AlertCardState(
        title: asked.title,
        message: Self.earlier(answer),
        buttons: [closeButton]
      )
    case .expired:
      AlertCardState(
        title: "This request expired",
        message: "Nothing changes. Ask again if you still want this.",
        buttons: [closeButton]
      )
    case .failed(let reason):
      AlertCardState(
        title: operation == nil ? "Atlas can't show this request" : asked.title,
        message: operation == nil ? "" : asked.message,
        buttons: [closeButton],
        phase: .refused(message: reason)
      )
    }
  }

  /// What an answer given before the window opened again says.
  ///
  /// - Parameter answer: The answer given.
  /// - Returns: Its sentence.
  private static func earlier(_ answer: ApprovalEarlierAnswer) -> String {
    switch answer {
    case .allowed: "You allowed this already. Nothing is left to answer."
    case .refused: "You didn't allow this. Nothing changes."
    case .used: "You allowed this already, and it has gone ahead."
    }
  }

  /// The word of the way out while the person may still answer.
  private static let refuseWord = "Don't Allow"

  /// The one button once nothing is left to answer.
  private var closeButton: AlertCardButton<ApprovalChoice> {
    AlertCardButton(id: .close, title: "Close", role: .safeCancel)
  }

  /// The alert as it asks.
  private var asked: AlertCardState<ApprovalChoice> {
    AlertCardState(title: question, message: explanation, buttons: buttons)
  }

  /// What is asked, naming the VM.
  private var question: String {
    switch operation {
    case .add: "Add the VM “\(desktop)”?"
    case .remove: "Remove the VM “\(desktop)”?"
    case .uninstall: "Uninstall Atlas and its VMs?"
    case nil: "Reading the request…"
    }
  }

  /// Who asked, what allowing it does, and that it is the person's to allow.
  private var explanation: String {
    let asked = "Asked from \(session)."
    let allow = "Allow it only if you asked for this."
    switch operation {
    case .add: return "\(asked) Atlas makes a new VM on this Mac. \(allow)"
    case .remove: return "\(asked) The VM and everything in it are deleted. \(allow)"
    case .uninstall: return "\(asked) Every VM and everything in them are deleted. \(allow)"
    case nil: return asked
    }
  }

  /// The buttons, the default first: Allow's word then Don't Allow, or for a destructive request Don't Allow then the
  /// red action.
  private var buttons: [AlertCardButton<ApprovalChoice>] {
    let action: String
    switch operation {
    case .add, nil: action = "Add VM"
    case .remove: action = "Remove VM"
    case .uninstall: action = "Uninstall"
    }
    guard operation?.isDestructive == true else {
      return [
        AlertCardButton(id: .allow, title: action, role: .action),
        AlertCardButton(id: .refuse, title: Self.refuseWord, role: .cancel),
      ]
    }
    return [
      AlertCardButton(id: .refuse, title: Self.refuseWord, role: .safeCancel),
      AlertCardButton(id: .allow, title: action, role: .destructive),
    ]
  }

  /// What happens next once allowed.
  private var goingOn: String {
    switch operation {
    case .add, nil: "Atlas goes on adding “\(desktop)”."
    case .remove: "Atlas goes on removing “\(desktop)”."
    case .uninstall: "Atlas goes on uninstalling."
    }
  }
}
