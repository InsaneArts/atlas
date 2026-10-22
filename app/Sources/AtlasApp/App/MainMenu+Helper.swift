import AppKit

extension MainMenu {
  /// Take Control ⌃⇧⎋ and Stop Agent ⌘.; Recover… and Restart the Helper…; Switch to the Helper… and Log Out the
  /// Helper…. While the main window holds control, its screen's key monitor answers ⌃⇧⎋ first, giving control back,
  /// and sends ⌘. to the helper.
  ///
  /// - Returns: The menu.
  static func helper() -> NSMenu {
    let helper = NSMenu(title: MenuWords.helper)
    let control = helper.addItem(
      withTitle: ViewerWords.takeControl,
      action: #selector(ViewerMenuActions.toggleControl(_:)),
      keyEquivalent: "\u{1b}"
    )
    control.keyEquivalentModifierMask = [.control, .shift]
    helper.addItem(
      withTitle: ViewerWords.stopAgentItem,
      action: #selector(ViewerMenuActions.toggleAgent(_:)),
      keyEquivalent: "."
    )
    helper.addItem(.separator())
    // Recover… then Restart; Switch and Log Out after a separator, as the board stands them.
    for item in HelperMenuItems.make(target: nil) {
      helper.addItem(item)
      if item.action == #selector(HelperMenuActions.askRecover(_:)) {
        helper.addItem(HelperMenuItems.restart(target: nil))
        helper.addItem(.separator())
      }
    }
    return helper
  }
}
