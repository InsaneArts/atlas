import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasServiceTesting

@testable import AtlasApp

extension PanelController {
  /// A panel over scripted parts that opens no window.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - commands: The scripted command line, which by default knows no command.
  ///   - loginItem: The scripted login item.
  ///   - links: Stand-ins for the other windows.
  ///   - clock: The clock the pauses between readings run on; by default one nobody advances, so nothing is read
  ///     twice unasked.
  ///   - icons: The shared icon repository; by default no workspace or disk reads.
  /// - Returns: The panel's controller.
  static func scripted(
    _ service: ScriptedService = ScriptedService(),
    commands: ScriptedCommands = ScriptedCommands([:]),
    loginItem: ScriptedLoginItem = ScriptedLoginItem(),
    links: PanelLinks = PanelLinks(),
    clock: ManualClock = ManualClock(),
    icons: AppIcons = .none
  ) -> PanelController {
    PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: HelperActions(
          console: SessionConsole(read: { true }),
          commands: commands,
          service: service,
          barrier: ControlBarrier(service: service, clock: clock)
        ),
        loginItem: loginItem.seam,
        links: links,
        clock: clock,
        icons: icons
      )
    )
  }
}
