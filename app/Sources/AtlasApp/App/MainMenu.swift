import AppKit

/// The app's menus: Atlas · File · Edit · View · Helper · Window · Help (MacMenus). Edit makes typing and pasting work
/// in fields as in any app; File, View and Helper ask the main window through the responder chain
/// (`MainMenu+File`, `+View`, `+Helper`), so with no main window in front their items wait. Every shortcut here is
/// registered once; the window's buttons carry none.
@MainActor
enum MainMenu {
  /// The menu bar's menus.
  ///
  /// - Parameters:
  ///   - quit: What Quit Atlas does, when not simply quitting: its target and action.
  ///   - settings: What Settings… (⌘,) does: its target and action; nil leaves it out, as the setup window on its own
  ///     does.
  ///   - checkForUpdates: What Check for Updates does: its target and action; nil leaves it out.
  ///   - gettingStarted: What Getting Started… does: its target and action; nil leaves the Help menu out.
  ///   - viewer: Whether the main window can open, so View, Helper and File's Save Picture… are offered; the setup
  ///     window on its own has none, and keeps File's Close.
  /// - Returns: The main menu.
  static func make(
    quit: (target: AnyObject, action: Selector)? = nil,
    settings: (target: AnyObject, action: Selector)? = nil,
    checkForUpdates: (target: AnyObject, action: Selector)? = nil,
    gettingStarted: (target: AnyObject, action: Selector)? = nil,
    viewer: Bool = false
  ) -> NSMenu {
    let menu = NSMenu()
    let window = windowMenu()
    let atlas = app(quit: quit, settings: settings, checkForUpdates: checkForUpdates)
    var menus =
      viewer
      ? [atlas, fileMenu(viewer: true), edit(), view(), helper(), window]
      : [atlas, fileMenu(viewer: false), edit(), window]
    if let gettingStarted { menus.append(help(gettingStarted)) }
    for submenu in menus {
      let item = NSMenuItem()
      item.submenu = submenu
      menu.addItem(item)
    }
    NSApplication.shared.windowsMenu = window
    return menu
  }

  /// About Atlas, Settings…, Check for Updates, Hide Atlas and Quit Atlas.
  ///
  /// - Parameters:
  ///   - quit: What Quit Atlas does, when not simply quitting.
  ///   - settings: What Settings… does, or nil to leave it out.
  ///   - checkForUpdates: What Check for Updates does, or nil to leave it out.
  /// - Returns: The menu.
  private static func app(
    quit: (target: AnyObject, action: Selector)?,
    settings: (target: AnyObject, action: Selector)?,
    checkForUpdates: (target: AnyObject, action: Selector)?
  ) -> NSMenu {
    let app = NSMenu(title: MenuWords.app)
    app.addItem(
      withTitle: MenuWords.about,
      action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
      keyEquivalent: ""
    )
    if settings != nil || checkForUpdates != nil { app.addItem(.separator()) }
    if let settings {
      let item = app.addItem(withTitle: MenuWords.settings, action: settings.action, keyEquivalent: ",")
      item.target = settings.target
    }
    if let checkForUpdates {
      let item = app.addItem(withTitle: MenuWords.checkForUpdates, action: checkForUpdates.action, keyEquivalent: "")
      item.target = checkForUpdates.target
    }
    app.addItem(.separator())
    app.addItem(withTitle: MenuWords.hide, action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    app.addItem(.separator())
    let quitItem = app.addItem(
      withTitle: MenuWords.quit,
      action: quit?.action ?? #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q"
    )
    quitItem.target = quit?.target
    return app
  }

  /// Undo, Redo, Cut, Copy, Paste and Select All. Copy copies the main window's picture while nothing else there
  /// has a selection: the window answers `copy:` once no field does.
  ///
  /// - Returns: The menu.
  private static func edit() -> NSMenu {
    let edit = NSMenu(title: MenuWords.edit)
    edit.addItem(withTitle: MenuWords.undo, action: Selector(("undo:")), keyEquivalent: "z")
    let redo = edit.addItem(withTitle: MenuWords.redo, action: Selector(("redo:")), keyEquivalent: "z")
    redo.keyEquivalentModifierMask = [.command, .shift]
    edit.addItem(.separator())
    edit.addItem(withTitle: MenuWords.cut, action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    edit.addItem(withTitle: MenuWords.copy, action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(withTitle: MenuWords.paste, action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    edit.addItem(withTitle: MenuWords.selectAll, action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    return edit
  }

  /// Minimize, and the window list the system adds.
  ///
  /// - Returns: The menu.
  private static func windowMenu() -> NSMenu {
    let window = NSMenu(title: MenuWords.window)
    window.addItem(
      withTitle: MenuWords.minimize,
      action: #selector(NSWindow.performMiniaturize(_:)),
      keyEquivalent: "m"
    )
    return window
  }

  /// Getting Started….
  ///
  /// - Parameter gettingStarted: What it does: its target and action.
  /// - Returns: The menu.
  private static func help(_ gettingStarted: (target: AnyObject, action: Selector)) -> NSMenu {
    let help = NSMenu(title: MenuWords.help)
    let item = help.addItem(withTitle: MenuWords.gettingStarted, action: gettingStarted.action, keyEquivalent: "")
    item.target = gettingStarted.target
    NSApplication.shared.helpMenu = help
    return help
  }
}
