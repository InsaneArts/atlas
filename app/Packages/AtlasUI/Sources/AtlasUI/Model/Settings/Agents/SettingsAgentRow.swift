/// One agent in the Agents pane: its tile, name and what Atlas does in it, then whether it is connected; and, when a
/// Connect failed, why, in the row itself.
public struct SettingsAgentRow: Equatable, Sendable, Identifiable {
  /// Where the agent stands, which the row's right side shows.
  public enum Connection: Equatable, Sendable {
    /// It uses Atlas: a green check and "Connected".
    case connected
    /// Its registration is verified: a green check and "Configured".
    case configured
    /// It is on this Mac and can be connected: Connect, waiting while another agent connects.
    case connect(isEnabled: Bool)
    /// It is being connected now.
    case connecting
    /// It isn't on this Mac: nothing at the row's end, and its icon faded.
    case missing
    /// Its state isn't known yet, saying so, such as "Checking…".
    case note(String)
  }

  /// The agent's id, such as `claude`: what Connect sends.
  public var id: String
  /// The agent's name, such as "Claude Code".
  public var title: String
  /// What Atlas does in it, such as "Live Mac view in the Codex app", or "Not installed".
  public var detail: String
  /// Its icon: its app's own where the app is on this Mac, else a tile.
  public var icon: RowIcon
  /// Where it stands.
  public var connection: Connection
  /// Why its last Connect failed, in place of the detail, such as "Couldn't connect: its settings file has a
  /// mistake"; nil for none.
  public var problem: String?
  /// Whether Show File stands beside Connect, opening the file the problem is in.
  public var showsFile: Bool
  /// Whether any connected parts can be disconnected now.
  public var canDisconnect: Bool

  /// Creates an agent's row.
  ///
  /// - Parameters:
  ///   - id: The agent's id: what Connect sends.
  ///   - title: The agent's name.
  ///   - detail: What Atlas does in it.
  ///   - icon: Its icon.
  ///   - connection: Where it stands.
  ///   - problem: Why its last Connect failed; none by default.
  ///   - showsFile: Whether Show File stands beside Connect; not by default.
  ///   - canDisconnect: Whether connected parts can be disconnected now.
  public init(
    id: String,
    title: String,
    detail: String,
    icon: RowIcon,
    connection: Connection,
    problem: String? = nil,
    showsFile: Bool = false,
    canDisconnect: Bool = false
  ) {
    self.id = id
    self.title = title
    self.detail = detail
    self.icon = icon
    self.connection = connection
    self.problem = problem
    self.showsFile = showsFile
    self.canDisconnect = canDisconnect
  }
}
