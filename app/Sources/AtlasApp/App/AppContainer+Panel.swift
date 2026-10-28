import AppKit
import AtlasCommands
import AtlasService

/// The menu bar item and its panel, built once at launch: the panel's controller, linked to the other windows by the
/// coordinator, Atlas's item in the menu bar, and the popover it opens; helper discovery starts in its own task.
extension AppContainer {
  /// Builds the panel and puts Atlas's item in the menu bar.
  ///
  /// - Parameter actions: The helper's own commands, shared with the main window.
  func startPanel(actions: HelperActions) {
    let controller = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: actions,
        loginItem: loginItem,
        links: coordinator.panelLinks(),
        clock: clock
      )
    )
    menuBar = PanelPopover(
      controller: controller,
      item: StatusItem(),
      openSettings: { [weak coordinator] in coordinator?.openSettings() },
      gettingStarted: { [weak coordinator] in coordinator?.showOnboarding() }
    )
    panel = controller
    // Unstructured, as nothing at launch can wait: discover the helper once; polling has its own tasks.
    Task { await controller.findHelper() }
  }

  /// Closes the panel, as opening a window or an alert from it does.
  func closePanel() {
    menuBar?.closePanel()
  }

  /// The app's control work not yet answered, shared by the main window and the helper's commands.
  var controlBarrier: ControlBarrier? { actions?.barrier }

  /// Quits Atlas.
  ///
  /// - Parameter sender: What asked.
  func terminate(_ sender: Any?) {
    NSApp.terminate(sender)
  }

  /// Puts what `atlas doctor` says on the general pasteboard, as indented
  /// text, so a bug report stays readable.
  ///
  /// - Returns: Whether it said anything to copy.
  func copyDiagnostics() async -> Bool {
    let outcome = await commands.run(.doctor)
    let report = try? JSONSerialization.jsonObject(with: outcome.reply)
    let indented = report.flatMap { report in
      try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
    }
    let text = String(decoding: indented ?? outcome.reply, as: UTF8.self)
    guard !text.isEmpty else { return false }
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    return pasteboard.setString(text, forType: .string)
  }

  /// Hands the panel's latest reading to the signals.
  ///
  /// - Parameter status: The reading, or nil when it failed.
  func read(_ status: ViewerStatus?) {
    signals?.read(status)
  }
}
