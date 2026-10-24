import AtlasCommands
import AtlasUI
import CoreGraphics
import Foundation

/// The agents' own app icons, where their apps are on this Mac: the Claude app, and the Codex app, which is not the
/// Codex command line but is the same agent. Claude Code has no app, so it keeps its tile. The icons are read through
/// Settings' dependencies, never the workspace directly (A-17).
enum AgentIcons {
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
}
