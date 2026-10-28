import AtlasStatus

/// The lines beside a status's word that depend on what was done, so the panel and the window say the same.
extension StatusReading {
  /// What an agent is doing, beside Working: its newest action, else the service's reason. A status line, not a
  /// sentence, so it ends without a full stop.
  ///
  /// - Parameter withAge: Whether the action's age follows it, as the panel shows it.
  /// - Returns: "Turning Show Center off in Opie", or with " · just now" after it.
  func workingLine(withAge: Bool) -> String {
    guard let newest = newestAgentAction else {
      let reason = status.overview.reason ?? Words.connecting
      return reason.hasSuffix(".") ? String(reason.dropLast()) : reason
    }
    let first = newest.text.prefix(1)
    let action = first.uppercased() + newest.text.dropFirst()
    return withAge && !newest.when.isEmpty ? "\(action) · \(newest.when)" : action
  }

  /// What an agent did last, and when, beside Idle; before any agent acted, that the helper is ready for them, so Idle
  /// never stands alone.
  var idleLine: String {
    newestAgentAction.map { Words.idleLine(last: $0.text, ago: $0.when) } ?? Words.readyLine
  }

  /// Why the helper needs the person, beside Needs you: input Recover starts again, else an unfinished action, which
  /// recovery waits to settle while a command an agent started still runs.
  var needsYouLine: String {
    if inputStopped { return Words.inputStoppedLine }
    return status.overview.commandRuns ? Words.unfinishedBesideCommandLine : Words.unfinishedLine
  }

  /// What holding control means for agents, beside You have control: they wait, unless a command one started still
  /// runs.
  var controlLine: String {
    status.overview.commandRuns ? Words.controlBesideCommandLine : Words.controlLine
  }
}
