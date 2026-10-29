extension CleanupStep {
  /// Why a step was kept, failed or did not run, or why a Stop could not end a job, as the service's word names it. A
  /// word this app does not know reads as ``unknown``, so a newer service's reason still decodes.
  public enum Reason: String, Equatable, Sendable {
    /// An installed app changed since it was installed.
    case changed
    /// Something else is where it was.
    case replaced
    /// Another session made it.
    case anotherSession = "another-session"
    /// Atlas cannot tell whose it is.
    case labelUnreadable = "label-unreadable"
    /// The helper changed some of a plan's files since it was applied.
    case keptTargets = "kept-targets"
    /// An app did not quit in time.
    case timeout
    /// It is still in use.
    case inUse = "in-use"
    /// The file system refused.
    case io
    /// The helper could not do it.
    case helper
    /// A step before it is not done.
    case needsStep = "needs-step"
    /// Agents are stopped.
    case stopped
    /// It could not be moved aside without copying it to another volume.
    case otherVolume = "other-volume"
    /// Atlas could not read its record of it.
    case unreadable
    /// A copy collected out of the helper is missing or changed.
    case outputNotKept = "output-not-kept"
    /// A background job is still stopping.
    case stillStopping = "still-stopping"
    /// A background job is no longer the process Atlas started.
    case identity
    /// Part of a background job runs as another account.
    case otherAccount = "other-account"
    /// The helper could not be reached to stop a background job.
    case daemon
    /// A background job's first process is gone.
    case leaderGone = "leader-gone"
    /// An earlier action's outcome is unknown, so the step waits for recovery.
    case uncertain
    /// A reason this app does not know.
    case unknown

    /// The reason the service's word names, or nil when it wrote none.
    ///
    /// - Parameter word: The `reason` the service wrote, or nil when it wrote none.
    init?(word: String?) {
      guard let word, !word.isEmpty else { return nil }
      self = Self(rawValue: word) ?? .unknown
    }
  }
}
