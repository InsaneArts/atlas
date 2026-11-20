import AppKit

extension MainMenu {
  /// Take Control ⌃⇧⎋ (Give Back Control while in control), Stop Agent ⌘. (Resume Agent) and Send Key, whose keys go
  /// to the helper while the window in front holds control; Check Again ⌘R and Open Settings on Helper…; Recover…,
  /// with Restart Helper… in its place while Option is held, since only someone who knows Recover cannot help should
  /// reach for it; then Switch to and Log Out, which name the helper once the window in front knows it. While the main window holds
  /// control, its screen's key monitor answers ⌃⇧⎋ first, giving control back, and sends ⌘. to the helper.
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
    let keys = helper.addItem(withTitle: ViewerWords.sendKeyItem, action: nil, keyEquivalent: "")
    keys.submenu = KeysMenu.menu()
    helper.addItem(.separator())
    helper.addItem(
      withTitle: ViewerWords.checkAgainItem,
      action: #selector(ViewerMenuActions.checkAgain(_:)),
      keyEquivalent: "r"
    )
    helper.addItem(
      withTitle: ViewerWords.openSettingsItem,
      action: #selector(ViewerMenuActions.openHelperSettings(_:)),
      keyEquivalent: ""
    )
    helper.addItem(.separator())
    let recover = helper.addItem(
      withTitle: HelperWords.recoverItem,
      action: #selector(HelperMenuActions.askRecover(_:)),
      keyEquivalent: ""
    )
    recover.keyEquivalentModifierMask = []
    let restart = helper.addItem(
      withTitle: MenuWords.restartHelper,
      action: #selector(HelperMenuActions.askRestart(_:)),
      keyEquivalent: ""
    )
    restart.keyEquivalentModifierMask = .option
    restart.isAlternate = true
    helper.addItem(.separator())
    helper.addItem(
      withTitle: MenuWords.switchTo(nil),
      action: #selector(HelperMenuActions.askSwitch(_:)),
      keyEquivalent: ""
    )
    helper.addItem(
      withTitle: MenuWords.logOut(nil),
      action: #selector(HelperMenuActions.askLogOut(_:)),
      keyEquivalent: ""
    )
    return helper
  }

  /// Names the main menu's menu of the desktop's commands for the desktop shown: Helper, or Desktop while a VM is
  /// shown; its items are named as each window validates them.
  ///
  /// - Parameters:
  ///   - noun: How the desktop shown is named.
  ///   - menu: The app's main menu; nil while it has none.
  static func name(_ noun: DesktopNoun, in menu: NSMenu?) {
    let recover = #selector(HelperMenuActions.askRecover(_:))
    let item = menu?.items.first { $0.submenu?.items.contains { $0.action == recover } == true }
    guard let item, let submenu = item.submenu, submenu.title != noun.menuTitle else { return }
    item.title = noun.menuTitle
    submenu.title = noun.menuTitle
  }
}
