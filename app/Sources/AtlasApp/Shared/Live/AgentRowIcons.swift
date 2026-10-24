import AtlasCommands
import AtlasUI
import CoreGraphics
import Foundation

/// The icon each agent shows in a list, wherever the app lists agents: the agent's app's own where the app is on this
/// Mac (the Claude app, and the Codex app, which is not the Codex command line but is the same agent), else a tile of
/// its own. The icons are read through the caller's dependencies, never the workspace directly.
enum AgentRowIcons {
  /// Each agent's app, by agent id, as its bundle's name.
  static let apps = [AgentsStatus.claudeAppID: "Claude.app", AgentID.codex.rawValue: "Codex.app"]

  /// The icons of the agents whose apps are installed, in /Applications or the person's own Applications folder.
  ///
  /// - Parameters:
  ///   - home: The person's home folder.
  ///   - icon: The icon a file shows, by its path, or nil where nothing is there.
  /// - Returns: The icons, by agent id.
  @MainActor static func onThisMac(home: URL, icon: (String) -> CGImage?) -> [String: IconImage] {
    let folders = [URL(fileURLWithPath: "/Applications"), home.appendingPathComponent("Applications")]
    var icons: [String: IconImage] = [:]
    for (agent, app) in apps {
      for folder in folders {
        let path =
          folder
          .appendingPathComponent(app)
          .path
        guard let image = icon(path) else { continue }
        icons[agent] = IconImage(id: path, image: image)
        break
      }
    }
    return icons
  }

  /// An agent's icon: its app's own where the app is on this Mac, else a tile of its own, Claude Code's unlike Codex's.
  ///
  /// - Parameters:
  ///   - id: The agent's id.
  ///   - icons: The icons of the agents whose apps are on this Mac.
  /// - Returns: The icon.
  static func icon(_ id: String, _ icons: [String: IconImage]) -> RowIcon {
    if let icon = icons[id] { return .app(icon) }
    return switch id {
    case AgentID.claude.rawValue: .tile(.symbol("asterisk"))
    case AgentsStatus.claudeAppID: .tile(.symbol("bubble.left"))
    default: .tile(.text(">_"))
    }
  }
}
