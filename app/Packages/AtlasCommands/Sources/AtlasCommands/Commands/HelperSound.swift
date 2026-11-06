import AtlasService

/// Whether apps in the helper can be heard on the person's speakers, and the choice the helper keeps, as
/// `atlas sound` says. A switch shows the choice; when the sound is not as chosen, the reason is its caption.
public struct HelperSound: Equatable, Sendable {
  /// What the helper's sound is now.
  public enum State: Equatable, Sendable {
    /// Apps in the helper stay silent.
    case muted
    /// Apps in the helper can be heard.
    case on
    /// The sound is not as the person chose: why, with the one step that fixes it.
    case unavailable(String)
  }

  /// What the helper's sound is now.
  public let state: State
  /// Whether the person wants apps in the helper silent, as the helper keeps it.
  public let wanted: Bool

  /// Creates what the helper says of its sound.
  ///
  /// - Parameters:
  ///   - state: What the helper's sound is now.
  ///   - wanted: Whether the person wants apps in the helper silent.
  public init(state: State, wanted: Bool) {
    self.state = state
    self.wanted = wanted
  }

  /// What the daemon said of its sound with the status, or nil when it said something unknown.
  ///
  /// - Parameter said: The status's sound.
  public init?(_ said: ViewerStatus.Sound) {
    guard let state = Self.state(said.helperSound, reason: said.reason) else { return nil }
    self.init(state: state, wanted: said.wanted)
  }

  /// The state a reply's words name, or nil for words it does not know.
  ///
  /// - Parameters:
  ///   - word: `muted`, `on` or `unavailable`.
  ///   - reason: Why the sound is not as chosen, when unavailable.
  /// - Returns: The state.
  static func state(_ word: String, reason: String?) -> State? {
    switch word {
    case "muted": .muted
    case "on": .on
    case "unavailable": .unavailable(reason ?? "Atlas could not keep the helper's apps silent.")
    default: nil
    }
  }

  /// What `atlas sound` said, or nil when it did not work or said neither.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: The sound and the choice, or nil.
  public static func of(_ outcome: Outcome) -> HelperSound? {
    guard outcome.ok, let reply = outcome.decode(ViewerStatus.Sound.self) else { return nil }
    return HelperSound(reply)
  }

  /// Why the sound is not as the person chose, with its fix; nil when it is.
  public var problem: String? {
    guard case .unavailable(let reason) = state else { return nil }
    return reason
  }
}
