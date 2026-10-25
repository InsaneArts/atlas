import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import CoreGraphics

@testable import AtlasApp

extension SettingsController {
  /// A Settings controller over a scripted command line and service, with its choices kept in memory and the other
  /// windows standing still, so a test never writes the real defaults or reaches another window.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - statusReads: Counts each time it asks for the shared status to be read again.
  ///   - installs: The agent installs it shares with another window; its own over `commands` when nil.
  ///   - clock: The clock its waits run on, such as a done Recover's hold.
  ///   - actions: The helper's commands it shares with another window; its own over `commands` when nil.
  ///   - fileIcon: The icon a file shows, by its path; none by default.
  ///   - status: The shared status it reads; an empty one by default.
  /// - Returns: The controller.
  static func scripted(
    commands: ScriptedCommands,
    statusReads: CallCount = CallCount(),
    installs: AgentInstalls? = nil,
    clock: ManualClock = ManualClock(),
    actions: HelperActions? = nil,
    fileIcon: @escaping @MainActor (String) -> CGImage? = { _ in nil },
    status: ViewerStatus = ViewerStatus()
  ) -> SettingsController {
    let service = ScriptedService()
    return SettingsController(
      dependencies: SettingsDependencies(
        commands: commands,
        actions: actions
          ?? HelperActions(
            commands: commands,
            service: service,
            barrier: ControlBarrier(service: service, clock: ManualClock())
          ),
        installs: installs ?? AgentInstalls(commands: commands),
        preferences: SettingsPreferences(
          loadTab: { nil },
          saveTab: { _ in },
          loadNotify: { false },
          saveNotify: { _ in }
        ),
        fileIcon: fileIcon,
        version: "0.1.0",
        links: SettingsLinks(
          status: { (status, false) },
          readStatus: { statusReads.count += 1 },
          opensAtLogin: { OpenAtLogin.State(isAvailable: true, isEnabled: false) },
          setOpenAtLogin: { _ in },
          picture: { .live },
          canPause: { false },
          choosePicture: { _ in },
          switchToHelper: { "" },
          gettingStarted: {}
        ),
        clock: clock
      )
    )
  }
}
