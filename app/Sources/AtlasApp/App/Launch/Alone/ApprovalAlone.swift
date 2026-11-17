import AppKit
import AtlasCommands

/// `--approve ID`: the approval window on its own, for a request a command made of the person, as `atlas` opens it
/// through Atlas.app whether or not Atlas is set up. It needs no settings, menu bar or service, and quits once the
/// window closes.
enum ApprovalAlone {
  /// Opens the approval window for the request `id` and runs until it closes.
  ///
  /// - Parameters:
  ///   - id: The request's ID.
  ///   - commands: The `atlas` the window reads and answers the request through.
  ///   - clock: The clock the window's waits run on.
  /// - Returns: Never; the process exits.
  static func run(id: String, commands: Commands, clock: any Clock<Duration>) -> Never {
    let app = NSApplication.shared
    app.mainMenu = MainMenu.make()
    let controller = ApprovalController(
      id: id,
      commands: commands,
      links: ApprovalLinks(close: { NSApp.terminate(nil) }, clock: clock)
    )
    let window = ApprovalWindow(controller: controller)
    let delegate = ApprovalAppDelegate(controller: controller)
    app.delegate = delegate
    DockIcon.settle(windowOpen: true)
    app.finishLaunching()
    window.present()
    // Unstructured, as reading the request needs the app running below.
    Task { await controller.start() }
    withExtendedLifetime((window, delegate)) { app.run() }
    exit(0)
  }
}
