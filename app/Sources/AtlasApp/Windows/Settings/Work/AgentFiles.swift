import AtlasCommands
import Foundation

/// Where an agent keeps the settings Connect writes, so Show File can open the one that needs mending.
enum AgentFiles {
  /// Whether Atlas knows where an agent keeps its settings: only the Claude app's.
  ///
  /// - Parameter id: The agent's id.
  /// - Returns: Whether Show File can open its settings.
  static func knowsSettings(of id: String) -> Bool {
    id == AgentsStatus.claudeAppID
  }

  /// The settings file of an agent, where Atlas knows it.
  ///
  /// - Parameters:
  ///   - id: The agent's id, such as `claude-app`.
  ///   - home: The person's home folder.
  /// - Returns: The Claude app's settings file, or nil for another agent.
  static func settings(of id: String, home: URL) -> URL? {
    guard knowsSettings(of: id) else { return nil }
    return home.appending(path: "Library/Application Support/Claude/claude_desktop_config.json")
  }
}
