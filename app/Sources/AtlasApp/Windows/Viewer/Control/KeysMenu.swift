import AppKit

/// Helper ▸ Send Key: keys this Mac would keep for itself (⌘⇥, ⌘Space…) and keys an app would keep (⌘W, ⌘Q, Escape),
/// sent to the helper while the window in front holds control, which leaves the window in front and control where it
/// is. Each item asks the window by its tag, its key's place in ``HelperKey/all``; the window enables it only while it
/// holds control. While in control the keyboard is the helper's, so the menu is chosen with the mouse.
enum KeysMenu {
  /// The submenu of keys under its two headers.
  ///
  /// - Returns: The menu.
  static func menu() -> NSMenu {
    let menu = NSMenu(title: ViewerWords.sendKeyItem)
    menu.addItem(.sectionHeader(title: ViewerWords.macKeys))
    add(HelperKey.mac, from: .zero, to: menu)
    menu.addItem(.separator())
    menu.addItem(.sectionHeader(title: ViewerWords.appKeys))
    add(HelperKey.app, from: HelperKey.mac.count, to: menu)
    return menu
  }

  /// Adds one item a key, each tagged with its key's place in ``HelperKey/all``.
  ///
  /// - Parameters:
  ///   - keys: The keys.
  ///   - start: The first key's place in ``HelperKey/all``.
  ///   - menu: The menu.
  private static func add(_ keys: [HelperKey], from start: Int, to menu: NSMenu) {
    for (offset, key) in keys.enumerated() {
      let item = menu.addItem(withTitle: key.title, action: #selector(ViewerMenuActions.sendKey(_:)), keyEquivalent: "")
      item.tag = start + offset
    }
  }
}
