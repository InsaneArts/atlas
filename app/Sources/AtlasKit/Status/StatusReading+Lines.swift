/// The lines beside a status's word that depend on what was done, so the panel and the window say the same.
extension StatusReading {
  /// The newest thing an agent did, if any: the person's own actions are not an agent's.
  var newestAgentAction: ActivityItem? {
    activity(limit: status.overview.activity.count)
      .first { $0.actor == .agent }
  }

  /// What an agent is doing, beside Working: its newest action, else the service's reason. A status line, not a
  /// sentence, so it ends without a full stop.
  ///
  /// - Parameter withAge: Whether the action's age follows it, as the panel shows it.
  /// - Returns: "Turning Show Center off in Opie", or with " · just now" after it.
  package func workingLine(withAge: Bool) -> String {
    guard let newest = newestAgentAction else {
      let reason = status.overview.reason
      return reason.hasSuffix(".") ? String(reason.dropLast()) : reason
    }
    let action = newest.text.prefix(1).uppercased() + newest.text.dropFirst()
    return withAge && !newest.when.isEmpty ? "\(action) · \(newest.when)" : action
  }

  /// What an agent did last, and when, beside Idle; empty before any agent acted.
  package var idleLine: String {
    newestAgentAction.map { Words.idleLine(last: $0.text, ago: $0.when) } ?? ""
  }

  /// Why the helper needs the person, beside Needs you: input Recover starts again, else an unfinished action.
  package var needsYouLine: String {
    status.overview.mode == .blocked ? Words.inputStoppedLine : Words.unfinishedLine
  }
}
