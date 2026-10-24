import AppKit

/// The helper's menu items for a menu that lists them all as they are: Recover…, Switch to the Helper… and Log Out the
/// Helper…. The main menu's Helper menu builds its own, naming the helper (`MainMenu+Helper`). Each asks its window
/// (``HelperMenuActions``), which validates it against the same state its buttons draw.
enum HelperMenuItems {
  /// The three items, in order.
  ///
  /// - Parameter target: Who answers them: a window's own object, or nil for the responder chain, as the main menu's
  ///   items are.
  /// - Returns: Recover…, Switch to the Helper… and Log Out the Helper….
  static func make(target: AnyObject?) -> [NSMenuItem] {
    [
      item(HelperWords.recoverItem, #selector(HelperMenuActions.askRecover(_:)), target: target),
      item(HelperWords.switchItem, #selector(HelperMenuActions.askSwitch(_:)), target: target),
      item(HelperWords.logOutItem, #selector(HelperMenuActions.askLogOut(_:)), target: target),
    ]
  }

  /// One item with no key equivalent.
  ///
  /// - Parameters:
  ///   - title: Its title.
  ///   - action: What it asks.
  ///   - target: Who answers it.
  /// - Returns: The item.
  private static func item(_ title: String, _ action: Selector, target: AnyObject?) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
    item.target = target
    return item
  }
}
