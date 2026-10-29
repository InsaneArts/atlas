import AtlasCommands
import AtlasService
import AtlasStatus
import AtlasUI

/// A host session's icon: its host's bundled logo or mark, through ``AgentRowIcons`` with no installed icons, so the
/// panel, the main window and their renders never depend on what this Mac has installed; the `>_` tile for a host
/// Atlas does not know and for Terminal.
enum SessionIcons {
  /// The icon of a host.
  ///
  /// - Parameter host: The app a session runs in.
  /// - Returns: The host's logo or mark, or the `>_` tile.
  static func icon(_ host: SessionHost) -> RowIcon {
    let agent: AgentID? =
      switch host {
      case .claude: .claude
      case .codex: .codex
      case .cursor: .cursor
      case .opencode: .opencode
      case .pi: .pi
      case .other, .terminal: nil
      }
    return AgentRowIcons.icon(agent?.rawValue ?? "", [:])
  }

  /// The icon of the session an action came from: Terminal's `>_` tile for an action no host named, else its listed
  /// session's host. A session the status does not list gets none, since nothing says which host runs it.
  ///
  /// - Parameters:
  ///   - key: The action's session key; empty for Terminal.
  ///   - reading: The status, read.
  /// - Returns: The icon, or nil for a session the status does not list.
  static func icon(of key: String, in reading: StatusReading) -> RowIcon? {
    guard !key.isEmpty else { return icon(.terminal) }
    return reading.session(key)
      .map { icon($0.host) }
  }
}
