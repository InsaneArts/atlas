import AtlasCommands
import Foundation

/// What the app takes from its environment, read in this one place so nothing else reads it.
struct AppConfiguration {
  /// The `atlas` the app's commands run.
  let commands: Commands
  /// How a panel render differs from the live panel, for design checks.
  let renderOverrides: PanelRenderOverrides

  /// Reads the configuration.
  ///
  /// - Parameters:
  ///   - environment: The process's environment.
  ///   - executable: This app's own executable, beside which `atlas` sits in Atlas.app.
  init(environment: [String: String], executable: URL) {
    // The `atlas` that started this app (`ATLAS_CLI`), else the one beside it. Its settings arrive as
    // `ATLAS_CONFIG`, which every command inherits with the rest of the environment.
    if let path = environment["ATLAS_CLI"], !path.isEmpty {
      commands = Commands(executable: URL(fileURLWithPath: path), environment: environment)
    } else {
      commands = Commands.beside(executable, environment: environment)
    }
    renderOverrides = PanelRenderOverrides(environment: environment)
  }
}
