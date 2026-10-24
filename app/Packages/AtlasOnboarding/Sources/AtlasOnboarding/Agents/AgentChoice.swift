import AtlasCommands

/// One agent on the agents screen, as `atlas agents status` reported it and the person switched it.
public struct AgentChoice: Equatable, Sendable {
  /// Which agent.
  public let id: AgentID
  /// Whether `atlas agents status` has said anything of it yet.
  public var checked = false
  /// Whether it is installed on this Mac.
  public var found = false
  /// Whether it already knows how to use Atlas.
  public var connected = false
  /// Whether the person wants it connected.
  public var on = false

  /// Creates an agent nothing is known of yet.
  ///
  /// - Parameter id: Which agent.
  public init(id: AgentID) {
    self.id = id
  }
}
