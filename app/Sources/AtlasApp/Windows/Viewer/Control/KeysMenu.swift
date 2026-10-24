import AppKit

/// Keys this Mac would keep for itself (⌘⇥, ⌘Space…) and keys an app would keep (⌘W, ⌘Q, Escape), sent to the
/// helper from a menu under the Keys button, which leaves the window in front and control where it is. An AppKit
/// menu, so its key equivalents are only shown, never registered as this app's.
enum KeysMenu {
  /// 240: the menu's width (mac.py:152).
  static let width: CGFloat = 240

  /// The menu of keys under its two headers, each item sending its key to `controller`.
  ///
  /// - Parameter controller: The window's controller.
  /// - Returns: The menu.
  static func menu(for controller: ViewerController) -> NSMenu {
    let menu = NSMenu()
    menu.appearance = Appearance.dark
    menu.minimumWidth = width
    menu.addItem(.sectionHeader(title: ViewerWords.macKeys))
    add(HelperKey.mac, to: menu, for: controller)
    menu.addItem(.separator())
    menu.addItem(.sectionHeader(title: ViewerWords.appKeys))
    add(HelperKey.app, to: menu, for: controller)
    return menu
  }

  /// Adds one item a key, each sending its key to `controller`.
  ///
  /// - Parameters:
  ///   - keys: The keys.
  ///   - menu: The menu.
  ///   - controller: The window's controller.
  private static func add(_ keys: [HelperKey], to menu: NSMenu, for controller: ViewerController) {
    for key in keys {
      let item = HandlerMenuItem(title: key.name) { [weak controller] in controller?.control.sendKey(key.sent) }
      item.keyEquivalent = key.shown
      item.keyEquivalentModifierMask = key.modifiers
      menu.addItem(item)
    }
  }
}
