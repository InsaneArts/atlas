import AppKit

/// The ⋯ menu's helper commands: each asks first, in an alert ``WindowCoordinator`` places. The menu enables them from the
/// presenter's value, so they need no validator.
extension PanelPopover: HelperMenuActions {
  /// Recover…: ask before recovering the helper.
  ///
  /// - Parameter sender: The menu item.
  @objc func askRecover(_ sender: Any?) {
    controller.send(.askRecover)
  }

  /// Restart the Helper…, which the panel's menu does not list.
  ///
  /// - Parameter sender: The menu item.
  @objc func askRestart(_ sender: Any?) {}

  /// Switch to the Helper…: ask before switching this Mac's screen to it.
  ///
  /// - Parameter sender: The menu item.
  @objc func askSwitch(_ sender: Any?) {
    controller.send(.askSwitch)
  }

  /// Log Out the Helper…: ask before logging it out.
  ///
  /// - Parameter sender: The menu item.
  @objc func askLogOut(_ sender: Any?) {
    controller.askLogOut()
  }
}
