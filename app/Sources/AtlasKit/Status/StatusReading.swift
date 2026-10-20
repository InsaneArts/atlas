/// What the panel and viewer show of a status: one place, so they agree.
package struct StatusReading: Equatable, Sendable {
  /// The status it reads.
  package let status: ViewerStatus

  /// Creates a reading of `status`.
  ///
  /// - Parameter status: The status to read.
  package init(_ status: ViewerStatus) {
    self.status = status
  }

  /// Recent operations, newest first.
  ///
  /// - Parameter limit: The most rows. Default 6.
  /// - Returns: Who did what and when, newest first.
  package func activity(limit: Int = 6) -> [ActivityItem] {
    status.overview.activity
      .prefix(limit)
      .enumerated()
      .map { index, row in
        ActivityItem(
          id: index,
          actor: row.actor == "viewer" ? .you : .agent,
          text: Words.describe(row),
          when: row.ageSeconds.map(Words.ago) ?? "",
          ok: row.ok,
          age: row.ageSeconds
        )
      }
  }

  /// The few latest actions shown over the desktop, oldest first, while recent.
  ///
  /// - Parameters:
  ///   - limit: The most captions. Default 3.
  ///   - seconds: How old a caption may be, in seconds. Default 60.
  /// - Returns: The captions, oldest first.
  package func captions(limit: Int = 3, within seconds: Double = 60) -> [ActivityItem] {
    Array(
      activity(limit: 12)
        .filter { ($0.age ?? .infinity) <= seconds }
        .prefix(limit)
        .reversed()
    )
  }

  /// The newest action in one line, for the panel.
  package var lastAction: String {
    guard let first = activity(limit: 1).first else { return "No recent actions" }
    return [first.line, first.when]
      .filter { !$0.isEmpty }
      .joined(separator: " · ")
  }

  /// The session's facts; this viewer knows whether it holds control.
  ///
  /// - Parameter holdingControl: Whether this viewer holds control.
  /// - Returns: The display, input, control and pending actions, each in words.
  package func facts(holdingControl: Bool) -> SessionFacts {
    let overview = status.overview
    let input =
      status.session.inputReady ? "Ready" : status.session.inputReachable ? "Not ready" : "Unavailable"
    return SessionFacts(
      display: Words.size(overview.display) ?? Words.unknown,
      input: input,
      control: availability(holdingControl: holdingControl),
      pending: overview.pending > 0 ? String(overview.pending) : "None"
    )
  }

  /// Whether this viewer could take control now, and if not, why.
  func availability(holdingControl: Bool) -> String {
    let mode = status.overview.mode
    if mode.isControlled(byThisViewer: holdingControl) { return "This window" }
    return switch mode {
    case .observing: "Available"
    case .controlling: "Another window"
    case .agent: "Agent working"
    case .recovering: "After recovery"
    case .blocked: "Not ready"
    case .unavailable: "Unavailable"
    case .loggedOut: "Logged out"
    }
  }

  /// Why a viewer that does not hold control must wait to take it, if it must, as the sentence a footer shows; `nil`
  /// when nothing blocks it.
  package var controlBlocked: String? {
    switch status.overview.mode {
    case .agent: "An agent is acting in the helper."
    case .recovering: "Recover before taking control."
    case .controlling: "\(Words.anotherViewer)."
    default: nil
    }
  }

  /// The sidebar's line: why the helper is in its mode.
  ///
  /// - Parameter holdingControl: Whether this viewer holds control.
  /// - Returns: The service's reason, unless this viewer's control says more.
  package func reason(holdingControl: Bool) -> String {
    let mode = status.overview.mode
    if mode.isControlled(byThisViewer: holdingControl) {
      return "You have mouse and keyboard control"
    }
    if mode == .controlling { return Words.anotherViewer }
    return status.overview.reason.isEmpty ? Words.connecting : status.overview.reason
  }

  /// Each readiness check, then the native Save and Open check, which
  /// never counts as ready. An unknown check is unobservable, not failed.
  package var statusRows: [StatusRow] {
    // A silent helper's steps name commands; the overview says it in the app's words.
    let silent = !status.overview.mode.answers
    var rows = status.checks.map { check in
      switch check.ok {
      case false? where silent: StatusRow(name: check.name, state: .bad, text: status.overview.reason)
      case nil where silent: StatusRow(name: check.name, state: .unknown, text: "Unknown")
      case true: StatusRow(name: check.name, state: .ready, text: "Ready")
      case nil: StatusRow(name: check.name, state: .unknown, text: "Unknown. \(check.nextStep ?? "")")
      default: StatusRow(name: check.name, state: .bad, text: check.nextStep ?? "Not ready")
      }
    }
    let native = status.capabilities.nativeDialogs
    let age = native.ageSeconds.map(Words.ago) ?? ""
    switch native.state {
    case .passed:
      rows.append(StatusRow(name: "Native dialogs", state: .ready, text: "Save and Open passed \(age)"))
    case .failed:
      let stage = native.failedAt.map { " at “\($0)”" } ?? ""
      rows.append(StatusRow(name: "Native dialogs", state: .bad, text: "Failed \(age)\(stage). \(native.reason)"))
    case .unchecked:
      if !native.reason.isEmpty {
        rows.append(StatusRow(name: "Native dialogs", state: .unknown, text: native.reason))
      }
    }
    return rows
  }

  /// Recovery is offered only without control, when something needs it.
  func recoveryOffered(holdingControl: Bool) -> Bool {
    !holdingControl && (status.overview.mode == .recovering || status.overview.mode == .blocked)
  }

  /// The recovery alert, if any: while recovering, or when offered.
  ///
  /// - Parameters:
  ///   - holdingControl: Whether this viewer holds control.
  ///   - recovering: Whether this viewer is running recovery now.
  /// - Returns: The alert's title, reason and whether it is busy, or `nil` when there is none.
  package func recovery(holdingControl: Bool, recovering: Bool) -> Recovery? {
    if recovering {
      return Recovery(title: Words.recovering, reason: "Restoring the helper.", busy: true)
    }
    guard recoveryOffered(holdingControl: holdingControl) else { return nil }
    let title =
      status.overview.mode == .blocked ? "The helper isn't ready" : "The last action's outcome is unknown"
    return Recovery(title: title, reason: reason(holdingControl: false), busy: false)
  }
}
