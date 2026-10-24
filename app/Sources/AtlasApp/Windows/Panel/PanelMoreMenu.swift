import AppKit

/// The panel's ⋯ menu under its button: Open Atlas, unless the main button already is it, Settings… and Getting
/// Started…, then Quit Atlas. It holds only what applies everywhere; the helper's commands are the main button's,
/// Settings' and the Helper menu's. Its key equivalents also work while the panel is open with the menu closed
/// (``PanelKeys``).
enum PanelMoreMenu {
  /// 230: the menu's width.
  static let width: CGFloat = 230

  /// What the menu's items do.
  struct Actions {
    /// Open Atlas: the main window.
    var openAtlas: @MainActor () -> Void
    /// Settings…: the Settings window.
    var openSettings: @MainActor () -> Void
    /// Getting Started…: the setup window.
    var gettingStarted: @MainActor () -> Void
    /// Quit Atlas.
    var quit: @MainActor () -> Void
  }

  /// The menu.
  ///
  /// - Parameters:
  ///   - value: What the presenter decided it offers.
  ///   - actions: What its items do.
  /// - Returns: The menu.
  static func menu(_ value: PanelMoreMenuState, actions: Actions) -> NSMenu {
    let menu = NSMenu()
    menu.appearance = Appearance.dark
    menu.minimumWidth = width
    if value.showsOpenAtlas {
      menu.addItem(item(PanelWords.openAtlasItem, key: PanelKeys.openAtlas, run: actions.openAtlas))
    }
    menu.addItem(item(MenuWords.settings, key: PanelKeys.settings, run: actions.openSettings))
    menu.addItem(item(MenuWords.gettingStarted, key: "", run: actions.gettingStarted))
    menu.addItem(.separator())
    menu.addItem(item(MenuWords.quit, key: PanelKeys.quit, run: actions.quit))
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
