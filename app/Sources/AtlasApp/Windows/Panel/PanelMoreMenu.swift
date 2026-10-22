import AppKit

/// The panel's ⋯ menu under its button (mac.py:386): Open Atlas and Settings…, the helper's commands
/// (``HelperMenuItems``), then Open at Login and Quit Atlas. Built from the presenter's ``PanelMoreMenuState``, whose
/// fields its items are enabled and ticked by, the same fields the panel's buttons draw; its key equivalents act
/// only while it is open.
@MainActor
enum PanelMoreMenu {
  /// 230: the menu's width (mac.py:386).
  static let width: CGFloat = 230

  /// What the menu's own items do; the helper's commands go to the target's ``HelperMenuActions``.
  struct Actions {
    /// Open Atlas: the main window.
    var openAtlas: @MainActor () -> Void
    /// Settings…: the Settings window.
    var openSettings: @MainActor () -> Void
    /// Open at Login: turns it on or off.
    var setOpenAtLogin: @MainActor (Bool) -> Void
    /// Quit Atlas.
    var quit: @MainActor () -> Void
  }

  /// The menu.
  ///
  /// - Parameters:
  ///   - value: What the presenter decided it offers.
  ///   - target: Who answers the helper's commands.
  ///   - actions: What its own items do.
  /// - Returns: The menu.
  static func menu(_ value: PanelMoreMenuState, target: HelperMenuActions, actions: Actions) -> NSMenu {
    let menu = NSMenu()
    menu.appearance = NSAppearance(named: .darkAqua)
    menu.minimumWidth = width
    // Each item is enabled from the presenter's value, never by a second rule in a validator.
    menu.autoenablesItems = false
    menu.addItem(item(PanelWords.openAtlasItem, key: "o", run: actions.openAtlas))
    menu.addItem(item(MenuWords.settings, key: ",", run: actions.openSettings))
    menu.addItem(.separator())
    let helper = HelperMenuItems.make(target: target)
    let enabled = [value.helper.canRecover, value.helper.canSwitch, value.helper.canLogOut]
    for (helperItem, isEnabled) in zip(helper, enabled) {
      helperItem.isEnabled = isEnabled
      menu.addItem(helperItem)
    }
    menu.addItem(.separator())
    if let opensAtLogin = value.opensAtLogin {
      let login = item(PanelWords.openAtLoginItem, key: "") { actions.setOpenAtLogin(!opensAtLogin) }
      login.state = opensAtLogin ? .on : .off
      menu.addItem(login)
    }
    menu.addItem(item(MenuWords.quit, key: "q", run: actions.quit))
    return menu
  }

  /// An item that runs a closure, with a Command key equivalent when given one.
  ///
  /// - Parameters:
  ///   - title: Its title.
  ///   - key: Its key, with Command; empty for none.
  ///   - run: What choosing it does.
  /// - Returns: The item.
  private static func item(_ title: String, key: String, run: @escaping @MainActor () -> Void) -> NSMenuItem {
    let item = HandlerMenuItem(title: title, handler: run)
    item.keyEquivalent = key
    return item
  }
}
