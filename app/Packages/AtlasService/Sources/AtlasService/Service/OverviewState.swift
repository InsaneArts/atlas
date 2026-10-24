/// What the helper is doing, as every agent app shows it: the overview's `state`, which the service decides once
/// (`src/lib/overview.rs`, `kind()`) so the panel, the window and the agent hosts all say the same.
public enum OverviewState: Equatable, Sendable, Decodable {
  /// An agent is acting in the helper.
  case working
  /// Nothing is happening in the helper.
  case idle
  /// A viewer holds mouse and keyboard control; the agent waits.
  case control
  /// Something only the person or recovery can settle: an action's outcome is unknown, or input Recover starts.
  case needsYou
  /// The person stopped agents; they wait until the person resumes them.
  case stopped
  /// Agents can't use the helper yet.
  case notReady
  /// A state this app does not know, by its name; it reads as ``notReady``.
  case unknown(String)

  /// The state the service names `name`.
  ///
  /// - Parameter name: The state's name, as `status.json` gives it.
  init(name: String) {
    self =
      switch name {
      case "working": .working
      case "idle": .idle
      case "control": .control
      case "needsYou": .needsYou
      case "stopped": .stopped
      case "notReady": .notReady
      default: .unknown(name)
      }
  }

  /// Decodes a state from its name.
  ///
  /// - Parameter decoder: The decoder reading the `state` string.
  /// - Throws: `DecodingError` when it is not a string.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.singleValueContainer()
    self.init(name: try container.decode(String.self))
  }
}
