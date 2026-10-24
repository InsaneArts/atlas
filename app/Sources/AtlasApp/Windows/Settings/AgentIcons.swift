import AppKit
import AtlasCommands
import AtlasUI

/// The agents' own app icons, where their apps are on this Mac: the Claude app, and the Codex app, which is not the
/// Codex command line but is the same agent. Claude Code has no app, so it keeps its tile.
enum AgentIcons {
  /// 52: an icon's pixels, twice the 26-point row icon, so it stays sharp on a Retina display.
  static let pixels: CGFloat = 52

  /// Each agent's app, by agent id, as its bundle's name.
  static let apps = [AgentsStatus.claudeAppID: "Claude.app", AgentID.codex.rawValue: "Codex.app"]

  /// The icons of the agents whose apps are installed, in /Applications or the person's own Applications folder.
  ///
  /// - Parameter workspace: The workspace that reads an app's icon; the app's container hands in the shared one.
  /// - Returns: The icons, by agent id.
  static func onThisMac(_ workspace: NSWorkspace) -> [String: IconImage] {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let folders = [URL(fileURLWithPath: "/Applications"), home.appendingPathComponent("Applications")]
    var icons: [String: IconImage] = [:]
    for (agent, app) in apps {
      let found =
        folders
        .map { $0.appendingPathComponent(app) }
        .first {
          FileManager.default
            .fileExists(atPath: $0.path)
        }
      guard let found else { continue }
      var rect = CGRect(x: 0, y: 0, width: pixels, height: pixels)
      let icon =
        workspace
        .icon(forFile: found.path)
      guard let image = icon.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { continue }
      icons[agent] = IconImage(id: found.path, image: image)
    }
    return icons
  }
}
