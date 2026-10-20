import AppKit

/// Keys the Mac keeps for itself (⌘⇥, ⌘Space…), sent to the helper from a menu, which leaves the window in front and
/// control where it is. An AppKit menu, so its key equivalents are only shown, never registered as this app's.
@MainActor
enum KeysMenu {
  /// Pops the menu up at the pointer, as the footer's Keys button asks.
  ///
  /// - Parameter controller: The window's controller, which each item sends its key to.
  static func show(for controller: ViewerController) {
    menu(for: controller)
      .popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
  }

  /// The menu of keys, each item sending its key to `controller`.
  ///
  /// - Parameter controller: The window's controller.
  /// - Returns: The menu.
  static func menu(for controller: ViewerController) -> NSMenu {
    let menu = NSMenu()
    menu.appearance = NSAppearance(named: .darkAqua)
    for (index, group) in HelperKey.groups.enumerated() {
      if index > 0 { menu.addItem(.separator()) }
      for key in group {
        let item = KeyItem(title: key.name) { [weak controller] in controller?.sendKey(key.sent) }
        item.keyEquivalent = key.shown
        item.keyEquivalentModifierMask = key.modifiers
        menu.addItem(item)
      }
    }
    return menu
  }
}
