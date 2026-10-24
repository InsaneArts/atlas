/// What the panel and viewer show of a status, as kinds: one place, so they agree. The app words them.
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
  /// - Returns: Who did what and how long ago, newest first.
  package func activity(limit: Int = 6) -> [ActivityItem] {
    status.overview.activity
      .prefix(limit)
      .enumerated()
      .map { index, row in
        ActivityItem(id: row.id ?? String(index), actor: row.actor == "viewer" ? .you : .agent, row: row)
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

  /// The session's facts; this viewer knows whether it holds control.
  ///
  /// - Parameter holdingControl: Whether this viewer holds control.
  /// - Returns: The display, input, control and pending actions.
  package func facts(holdingControl: Bool) -> SessionFacts {
    let session = status.session
    let input: SessionFacts.Input =
      session.inputReady ? .ready : session.inputReachable ? .notReady : .notAnswering
    return SessionFacts(
      display: status.overview.display,
      input: input,
      control: availability(holdingControl: holdingControl),
      pending: status.overview.pending
    )
  }

  /// Whether this viewer could take control now, and if not, why.
  func availability(holdingControl: Bool) -> SessionFacts.Control {
    let mode = status.overview.mode
    if mode.isControlled(byThisViewer: holdingControl) { return .thisWindow }
    return switch mode {
    // The person may take over while an agent works: its action under way finishes first (``controlBlocked``).
    case .observing, .stopped, .agent: .available
    case .controlling: .anotherWindow
    case .recovering: .afterRecovery
    case .blocked, .unavailable: .notReady
    case .loggedOut: .loggedOut
    }
  }

  /// Why a viewer that does not hold control must wait to take it, if it must; `nil` when nothing blocks it. An agent
  /// at work never does: the person may always take over, and the service answers busy only while one of its actions
  /// runs, which the viewer asks again after.
  package var controlBlocked: ControlBlock? {
    switch status.overview.mode {
    case .recovering: .recovery
    case .controlling: .anotherWindow
    default: nil
    }
  }

  /// The sidebar's line: why the helper is in its mode, or, while idle, what an agent did last.
  ///
  /// - Parameter holdingControl: Whether this viewer holds control.
  /// - Returns: The service's reason, unless this viewer's control says more.
  package func reason(holdingControl: Bool) -> Reason {
    let mode = status.overview.mode
    if mode.isControlled(byThisViewer: holdingControl) { return .yourControl }
    if mode == .controlling { return .inUseElsewhere }
    // Idle says what an agent did last, as the line beside the status does, not the service's reason.
    if status.overview.state == .idle { return newestAgentAction.map(Reason.lastAction) ?? .nothingHappening }
    // A helper that is not ready gives its first step, which may name a command: its failed check says it plainly.
    if mode == .blocked, let check = status.checks.first(where: { $0.ok == false }) {
      return .failedCheck(check)
    }
    guard let reason = status.overview.reason, !reason.isEmpty else { return .connecting }
    return .service(reason)
  }

  /// Each readiness check, then the native Save and Open check, which never counts as ready. An unknown check is
  /// unobservable, not failed.
  package var statusRows: [StatusRow] {
    // A silent helper's steps name commands; the overview's reason stands for them.
    let silent = !status.overview.mode.answers
    var rows = status.checks.map { check in
      let subject = StatusRow.Subject.check(check.name)
      return switch check.ok {
      case true: StatusRow(subject: subject, state: .ready, detail: .passed)
      case nil: StatusRow(subject: subject, state: .unknown, detail: .notKnown)
      case false? where silent:
        StatusRow(subject: subject, state: .bad, detail: .helperSilent(status.overview.reason))
      default: StatusRow(subject: subject, state: .bad, detail: .failed(check))
      }
    }
    let native = status.capabilities.nativeDialogs
    switch native.state {
    case .passed:
      rows.append(StatusRow(subject: .nativeDialogs, state: .ready, detail: .worked(ageSeconds: native.ageSeconds)))
    case .failed:
      rows.append(StatusRow(subject: .nativeDialogs, state: .bad, detail: .didNotWork(ageSeconds: native.ageSeconds)))
    case .unchecked:
      if !native.reason.isEmpty {
        rows.append(StatusRow(subject: .nativeDialogs, state: .unknown, detail: .notChecked))
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
  /// - Returns: Running, or why recovery is offered, or `nil` when there is none.
  package func recovery(holdingControl: Bool, recovering: Bool) -> Recovery? {
    if recovering { return .running }
    guard recoveryOffered(holdingControl: holdingControl) else { return nil }
    let reason = reason(holdingControl: false)
    return status.overview.mode == .blocked ? .notReady(reason) : .unknownOutcome(reason)
  }
}
