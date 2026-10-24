import AppKit
import AtlasService

/// The menu bar item and its panel, built once at launch: the panel's controller, linked to the other windows by the
/// coordinator, Atlas's item in the menu bar, and the popover it opens; the controller's work starts in the app's
/// lifetime task group.
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
    menuBar = PanelPopover(controller: controller, item: StatusItem()) { [weak coordinator] in
      coordinator?.openSettings()
    }
    panel = controller
    // Unstructured, as nothing at launch can wait: the app's lifetime, whose task group runs the panel's work.
    lifetime = Task {
      await withDiscardingTaskGroup { group in
        group.addTask { await controller.run() }
      }
    }
  }

  /// Closes the panel, as opening a window or an alert from it does.
  func closePanel() {
    menuBar?.closePanel()
  }

  /// Quits Atlas.
  ///
  /// - Parameter sender: What asked.
  func terminate(_ sender: Any?) {
    NSApp.terminate(sender)
  }

  /// Hands the panel's latest reading to the signals.
  ///
  /// - Parameter status: The reading, or nil when it failed.
  func read(_ status: ViewerStatus?) {
    signals?.read(status)
  }
}
