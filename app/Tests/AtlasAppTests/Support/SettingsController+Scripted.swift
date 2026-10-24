import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI

@testable import AtlasApp

extension SettingsController {
  /// A Settings controller over a scripted command line and service, with its choices kept in memory and the other
  /// windows standing still, so a test never writes the real defaults or reaches another window.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - statusReads: Counts each time it asks for the shared status to be read again.
  ///   - installs: The agent installs it shares with another window; its own over `commands` when nil.
  /// - Returns: The controller.
  static func scripted(
    commands: ScriptedCommands,
    statusReads: CallCount = CallCount(),
    installs: AgentInstalls? = nil
  ) -> SettingsController {
    let service = ScriptedService()
    return SettingsController(
      dependencies: SettingsDependencies(
        commands: commands,
        actions: HelperActions(commands: commands, service: service, barrier: ControlBarrier(service: service)),
        installs: installs ?? AgentInstalls(commands: commands),
        preferences: SettingsPreferences(
          loadTab: { nil },
          saveTab: { _ in },
          loadNotify: { false },
          saveNotify: { _ in },
          loadBadge: { false },
          saveBadge: { _ in }
        ),
        agentIcons: { [:] },
        version: "0.1.0",
        links: SettingsLinks(
          status: { (ViewerStatus(), false) },
          readStatus: { statusReads.count += 1 },
          opensAtLogin: { OpenAtLogin.State(isAvailable: true, isEnabled: false) },
          setOpenAtLogin: { _ in },
          picture: { .live },
          canPause: { false },
          choosePicture: { _ in },
          switchToHelper: { "" },
          gettingStarted: {}
        )
      )
    )
  }
}
