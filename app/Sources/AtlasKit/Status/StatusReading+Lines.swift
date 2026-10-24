/// What the lines beside a status's word depend on, so the panel and the window say the same.
extension StatusReading {
  /// The newest thing an agent did, if any: the person's own actions are not an agent's.
  package var newestAgentAction: ActivityItem? {
    activity(limit: status.overview.activity.count)
      .first { $0.actor == .agent }
  }

  /// Why the helper needs the person, beside Needs you: its input stopped, which Recover starts again, rather than an
  /// action that may not have finished.
  package var inputStopped: Bool {
    status.overview.mode == .blocked
  }
}
