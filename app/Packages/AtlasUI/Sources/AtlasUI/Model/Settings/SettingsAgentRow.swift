/// One agent in the Agents pane: its tile, name and what Atlas does in it, then whether it is connected.
public struct SettingsAgentRow: Equatable, Sendable, Identifiable {
  /// Where the agent stands, which the row's right side shows.
  public enum Connection: Equatable, Sendable {
    /// It uses Atlas: "Connected" in green.
    case connected
    /// It is on this Mac and can be connected: Connect, waiting while another agent connects.
    case connect(isEnabled: Bool)
    /// It is being connected now.
    case connecting
    /// It cannot be connected now, saying why, such as "Not installed".
    case note(String)
  }

  /// The agent's id, such as `claude`: what Connect sends.
  public var id: String
  /// The agent's name, such as "Claude Code".
  public var title: String
  /// What Atlas does in it, such as "Uses Atlas in every project".
  public var detail: String
  /// Its icon: its app's own where the app is on this Mac, else a tile.
  public var icon: RowIcon
  /// Where it stands.
  public var connection: Connection

  /// Creates an agent's row.
  ///
  /// - Parameters:
  ///   - id: The agent's id: what Connect sends.
  ///   - title: The agent's name.
  ///   - detail: What Atlas does in it.
  ///   - icon: Its icon.
  ///   - connection: Where it stands.
  public init(id: String, title: String, detail: String, icon: RowIcon, connection: Connection) {
    self.id = id
    self.title = title
    self.detail = detail
    self.icon = icon
    self.connection = connection
  }
}
