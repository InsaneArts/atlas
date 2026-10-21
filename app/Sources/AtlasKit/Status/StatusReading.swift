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
    // A helper that is not ready gives its first step, which may name a command: its failed check says it plainly.
    if mode == .blocked, let check = status.checks.first(where: { $0.ok == false }) {
      return CheckWords.failed(check)
    }
    return status.overview.reason.isEmpty ? Words.connecting : status.overview.reason
  }

  /// Each readiness check in the app's words, then the native Save and Open check, which never counts as ready. An
  /// unknown check is unobservable, not failed.
  package var statusRows: [StatusRow] {
    // A silent helper's steps name commands; the overview says it in the app's words.
    let silent = !status.overview.mode.answers
    var rows = status.checks.map { check in
      let name = CheckWords.title(check.name)
      return switch check.ok {
      case true: StatusRow(name: name, state: .ready, text: CheckWords.ready)
      case nil: StatusRow(name: name, state: .unknown, text: CheckWords.notKnown)
      case false? where silent: StatusRow(name: name, state: .bad, text: status.overview.reason)
      default: StatusRow(name: name, state: .bad, text: CheckWords.failed(check))
      }
    }
    let native = status.capabilities.nativeDialogs
    // "Worked 5 min ago", or "Worked" when its age is unknown.
    let ago = native.ageSeconds.map { " \(Words.ago($0))" } ?? ""
    let name = "Save and Open dialogs"
    switch native.state {
    case .passed:
      rows.append(StatusRow(name: name, state: .ready, text: "Worked\(ago)"))
    case .failed:
      rows.append(
        StatusRow(name: name, state: .bad, text: "Didn't work\(ago).")
      )
    case .unchecked:
      if !native.reason.isEmpty {
        rows.append(StatusRow(name: name, state: .unknown, text: "Not checked yet"))
      }
    }
    return rows
  }

  /// Recovery is offered only without control, when Recover can help: an action's outcome is unknown, or the helper
  /// is not ready for a reason Recover fixes. The panel's recovery card follows the same rule.
  func recoveryOffered(holdingControl: Bool) -> Bool {
    !holdingControl && status.overview.recoveryHelps
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
