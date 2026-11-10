import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService

@testable import AtlasApp

extension HelperActions {
  /// The helper's commands over scripted parts, on a console that is always the helper's session.
  ///
  /// - Parameters:
  ///   - service: The service every release goes to.
  ///   - commands: The command line, which by default knows no command.
  ///   - clock: The clock a lost acquisition's hold waits on; by default one nobody advances.
  ///   - endControl: What ends this app's control before a command runs; nothing by default.
  /// - Returns: The commands.
  static func scripted(
    _ service: any ViewerService,
    commands: any CommandRunning = ScriptedCommands([:]),
    clock: any Clock<Duration> = ManualClock(),
    endControl: @escaping @MainActor () -> Void = {}
  ) -> HelperActions {
    HelperActions(
      console: SessionConsole(read: { true }),
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: clock),
      endControl: endControl,
      asks: AskStore(service: service, clock: clock)
    )
  }
}
