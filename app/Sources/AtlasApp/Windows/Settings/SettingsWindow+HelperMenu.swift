import AppKit
import AtlasUI

/// The main menu's Helper items as Settings answers them while it is in front: Recover…, Switch and Log Out ask on the
/// Helper pane, as its own buttons do, and each item is validated against the same state those buttons draw
/// (``SettingsHelperButtons``). Settings offers no restart, so Restart Helper… waits.
extension SettingsWindow: HelperMenuActions {
  /// Asks before recovering the helper, on the Helper pane.
  ///
  /// - Parameter sender: The menu item.
  func askRecover(_ sender: Any?) { ask(.askRecover) }

  /// Settings offers no restart; the item waits.
  ///
  /// - Parameter sender: The menu item.
  func askRestart(_ sender: Any?) {}

  /// Asks before switching to the helper, on the Helper pane.
  ///
  /// - Parameter sender: The menu item.
  func askSwitch(_ sender: Any?) { ask(.askSwitch) }

  /// Asks before logging the helper out, on the Helper pane.
  ///
  /// - Parameter sender: The menu item.
  func askLogOut(_ sender: Any?) { ask(.askLogOut) }

  /// Whether a Helper item can be chosen now, as the Helper pane's button for it, with the helper's name in Switch and
  /// Log Out; every other item is the window's to validate, as for any window.
  ///
  /// - Parameter item: The item.
  /// - Returns: Whether it can be chosen.
  override func validateMenuItem(_ item: NSMenuItem) -> Bool {
    let state = controller.state
    let buttons = state.helper.buttons
    let alertUp = state.alert != nil
    let name = state.helper.account.name
    switch item.action {
    case #selector(FindMenuActions.findInTab(_:)): return !alertUp
    case #selector(HelperMenuActions.askRecover(_:)): return buttons.canRecover && !alertUp
    case #selector(HelperMenuActions.askRestart(_:)): return false
    case #selector(HelperMenuActions.askSwitch(_:)):
      item.title = MenuWords.switchTo(name)
      return buttons.canSwitch && !alertUp
    case #selector(HelperMenuActions.askLogOut(_:)):
      item.title = MenuWords.logOut(name)
      return buttons.canLogOut && !alertUp
    default: return super.validateMenuItem(item)
    }
  }

  /// Shows the Helper pane and asks there, as its own button would.
  ///
  /// - Parameter event: The Helper pane's event.
  private func ask(_ event: SettingsEvent) {
    controller.selectTab(.helper)
    controller.send(event)
  }
}
