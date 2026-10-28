import AtlasService

extension StatusReading {
  /// What a session's line says, by its state: what it does while it acts, what it waits for, which action may not
  /// have finished, what it did last while idle, and when it ended.
  ///
  /// - Parameter session: The session.
  /// - Returns: The line's kind.
  public func line(of session: ViewerStatus.HostSession) -> SessionLine {
    switch session.state {
    case .needsYou:
      return .mayNotHaveFinished(verb: session.uncertainVerb)
    case .acting:
      return .acting(says: session.latest?.says)
    case .waiting:
      let recovery = session.waitingFor == .recovery || (session.waitingFor == nil && status.overview.pending > 0)
      return recovery ? .waitingForRecovery : .waitingForHelper(seconds: session.waitingSeconds)
    case .idle where session.isLive:
      guard let says = session.latest?.says else { return .none }
      return .last(says: says, age: session.latest?.ageSeconds)
    case .idle, .ended:
      return session.endedAt == nil ? .lastSeen(at: session.lastSeenAt) : .ended(at: session.endedAt)
    }
  }
}
