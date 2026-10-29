import AtlasCommands
import AtlasService
import AtlasUI

/// How a session's host looks and is named, in the panel and the main window alike.
extension SessionHost {
  /// The agent whose bundled logo or mark the host shows; nil for a host Atlas does not know and for Terminal.
  var agent: AgentID? {
    switch self {
    case .claude: .claude
    case .codex: .codex
    case .cursor: .cursor
    case .opencode: .opencode
    case .pi: .pi
    case .other, .terminal: nil
    }
  }

  /// The host's icon: its bundled logo or mark, through ``AgentRowIcons`` with no installed icons, so the panel, the
  /// main window and their renders never depend on what this Mac has installed; the `>_` tile for a host Atlas does not
  /// know and for Terminal.
  var icon: RowIcon {
    AgentRowIcons.icon(agent?.rawValue ?? "", [:])
  }

  /// The host's name: `label` when the service gives one, else the host's own, else the session's name, as for a
  /// session that named itself.
  ///
  /// - Parameters:
  ///   - label: The host's name in the service's words; empty when it gives none.
  ///   - session: The session's name.
  /// - Returns: "Claude Code", say.
  func name(label: String, session: String) -> String {
    if !label.isEmpty { return label }
    return switch self {
    case .claude: "Claude Code"
    case .codex: "Codex"
    case .cursor: "Cursor"
    case .opencode: "opencode"
    case .pi: "Pi"
    case .terminal: "Terminal"
    case .other: session
    }
  }
}
