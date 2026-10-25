import AtlasCommands
import AtlasUI
import CoreGraphics
import Foundation

/// The original logo each agent shows in a list: its installed app's live icon, else its bundled logo.
/// Live icons are read through the caller's dependencies, never the workspace directly.
enum AgentRowIcons {
  /// Each agent's app, by agent id, as its bundle's name.
  static let apps = [
    AgentID.claude.rawValue: "Claude.app",
    AgentsStatus.claudeAppID: "Claude.app",
    AgentID.codex.rawValue: "Codex.app",
  ]

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

  /// An agent's original logo, preferring its installed app's live icon over the bundled image.
  ///
  /// - Parameters:
  ///   - id: The agent's id.
  ///   - icons: The icons of the agents whose apps are on this Mac.
  /// - Returns: The icon.
  static func icon(_ id: String, _ icons: [String: IconImage]) -> RowIcon {
    if let icon = icons[id] { return .app(icon) }
    return switch id {
    case AgentID.claude.rawValue, AgentsStatus.claudeAppID: .app(AgentLogos.claude)
    case AgentID.codex.rawValue: .app(AgentLogos.codex)
    case AgentID.pi.rawValue: .mark(.pi)
    case AgentID.opencode.rawValue: .mark(.opencode)
    default: .tile(.text(">_"))
    }
  }
}
