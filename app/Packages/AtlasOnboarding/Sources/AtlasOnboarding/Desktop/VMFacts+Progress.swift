extension VMFacts {
  /// How far making a VM got, as Atlas VM keeps it.
  public struct Progress: Equatable, Sendable {
    /// What it does now.
    public var phase: Phase
    /// The bytes of the download received; nil when not known.
    public var received: Int64?
    /// The bytes the download has in all; nil when not known.
    public var total: Int64?
    /// How much of the phase is done, from 0 to 1; nil when not known.
    var fraction: Double?

    /// Creates a VM's making progress.
    ///
    /// - Parameters:
    ///   - phase: What it does now.
    ///   - received: The bytes of the download received.
    ///   - total: The bytes the download has in all.
    ///   - fraction: How much of the phase is done, from 0 to 1.
    public init(phase: Phase, received: Int64? = nil, total: Int64? = nil, fraction: Double? = nil) {
      self.phase = phase
      self.received = received
      self.total = total
      self.fraction = fraction
    }

    /// How much of the download is done, from 0 to 1: the bytes received of the whole, else Atlas VM's own fraction
    /// while it downloads; nil when neither is known.
    public var downloaded: Double? {
      if let received, let total, total > 0 { return min(max(Double(received) / Double(total), 0), 1) }
      guard phase == .downloading, let fraction else { return nil }
      return min(max(fraction, 0), 1)
    }
  }
}
