import AppKit

/// The app's menus: Atlas · File · Edit · View · Helper · Window · Help. Edit makes typing and pasting work in fields as
/// in any app, and Find… focuses search in Settings or the main window's tab; File, View and Helper ask the window in front through the
/// responder chain (`MainMenu+File`, `+View`, `+Helper`), so with no window in front that answers them their items
/// wait. Every shortcut here is registered once; the window's buttons carry none.
enum MainMenu {
  /// One target and action for a menu item that the app, not the window in front, answers.
  typealias Command = (target: AnyObject, action: Selector)

  /// The menu bar's menus.
  ///
  /// - Parameters:
  ///   - quit: What Quit Atlas does, when not simply quitting.
  ///   - settings: What Settings… (⌘,) does; nil leaves it out, as the setup window on its own does.
  ///   - checkForUpdates: What Check for Updates… does; nil leaves it out.
  ///   - gettingStarted: What Getting Started… does; nil leaves the Help menu out.
  ///   - openViewer: What Open Atlas (⌘O) does; nil leaves it out.
  ///   - copyDiagnostics: What Copy Diagnostics does; nil leaves it out.
  ///   - viewer: Whether the main window can open, so View, Helper and File's Save Picture… are offered; the setup
  ///     window on its own has none, and keeps File's Close.
  /// - Returns: The main menu.
  static func make(
    quit: Command? = nil,
    settings: Command? = nil,
    checkForUpdates: Command? = nil,
    gettingStarted: Command? = nil,
    openViewer: Command? = nil,
    copyDiagnostics: Command? = nil,
    viewer: Bool = false
  ) -> NSMenu {
    let menu = NSMenu()
    let window = windowMenu()
    let atlas = app(quit: quit, settings: settings, checkForUpdates: checkForUpdates)
    var menus =
      viewer
      ? [atlas, fileMenu(viewer: true, openViewer: openViewer), edit(viewer: true), view(), helper(), window]
      : [atlas, fileMenu(viewer: false, openViewer: openViewer), edit(viewer: false), window]
    if let gettingStarted { menus.append(help(gettingStarted, copyDiagnostics: copyDiagnostics)) }
    for submenu in menus {
      let item = NSMenuItem()
      item.submenu = submenu
      menu.addItem(item)
    }
    NSApplication.shared.windowsMenu = window
    return menu
  }

  /// About Atlas · Settings… · Check for Updates… · Services · Hide Atlas · Hide Others · Show All · Quit Atlas, as every
  /// Mac app's menu reads.
  ///
  /// - Parameters:
  ///   - quit: What Quit Atlas does, when not simply quitting.
  ///   - settings: What Settings… does, or nil to leave it out.
  ///   - checkForUpdates: What Check for Updates… does, or nil to leave it out.
  /// - Returns: The menu.
  private static func app(quit: Command?, settings: Command?, checkForUpdates: Command?) -> NSMenu {
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
    let services = app.addItem(withTitle: MenuWords.services, action: nil, keyEquivalent: "")
    let servicesMenu = NSMenu(title: MenuWords.services)
    services.submenu = servicesMenu
    NSApplication.shared.servicesMenu = servicesMenu
    app.addItem(.separator())
    app.addItem(withTitle: MenuWords.hide, action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    let others = app.addItem(
      withTitle: MenuWords.hideOthers,
      action: #selector(NSApplication.hideOtherApplications(_:)),
      keyEquivalent: "h"
    )
    others.keyEquivalentModifierMask = [.command, .option]
    app.addItem(
      withTitle: MenuWords.showAll,
      action: #selector(NSApplication.unhideAllApplications(_:)),
      keyEquivalent: ""
    )
    app.addItem(.separator())
    let quitItem = app.addItem(
      withTitle: MenuWords.quit,
      action: quit?.action ?? #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q"
    )
    quitItem.target = quit?.target
    return app
  }

  /// Undo, Redo, Cut, Copy, Paste and Select All, and with the main window, Find… (⌘F), which puts the focus in the
  /// key window's search. Copy copies what the main window shows while nothing else there has a selection: the window
  /// answers `copy:` once no field does.
  ///
  /// - Parameter viewer: Whether the main window can open, so Find… is offered.
  /// - Returns: The menu.
  private static func edit(viewer: Bool) -> NSMenu {
    let edit = NSMenu(title: MenuWords.edit)
    edit.addItem(withTitle: MenuWords.undo, action: Selector(("undo:")), keyEquivalent: "z")
    let redo = edit.addItem(withTitle: MenuWords.redo, action: Selector(("redo:")), keyEquivalent: "z")
    redo.keyEquivalentModifierMask = [.command, .shift]
    edit.addItem(.separator())
    edit.addItem(withTitle: MenuWords.cut, action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    edit.addItem(withTitle: MenuWords.copy, action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(withTitle: MenuWords.paste, action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    edit.addItem(withTitle: MenuWords.selectAll, action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    if viewer {
      edit.addItem(.separator())
      edit.addItem(withTitle: MenuWords.find, action: #selector(FindMenuActions.findInTab(_:)), keyEquivalent: "f")
    }
    return edit
  }

  /// Minimize, Zoom and Bring All to Front, then the window list the system adds.
  ///
  /// - Returns: The menu.
  private static func windowMenu() -> NSMenu {
    let window = NSMenu(title: MenuWords.window)
    window.addItem(
      withTitle: MenuWords.minimize,
      action: #selector(NSWindow.performMiniaturize(_:)),
      keyEquivalent: "m"
    )
    window.addItem(withTitle: MenuWords.zoom, action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
    window.addItem(.separator())
    window.addItem(
      withTitle: MenuWords.bringAllToFront,
      action: #selector(NSApplication.arrangeInFront(_:)),
      keyEquivalent: ""
    )
    return window
  }

  /// Getting Started…, and Copy Diagnostics where the app can read them.
  ///
  /// - Parameters:
  ///   - gettingStarted: What Getting Started… does.
  ///   - copyDiagnostics: What Copy Diagnostics does; nil leaves it out.
  /// - Returns: The menu.
  private static func help(_ gettingStarted: Command, copyDiagnostics: Command?) -> NSMenu {
    let help = NSMenu(title: MenuWords.help)
    let item = help.addItem(withTitle: MenuWords.gettingStarted, action: gettingStarted.action, keyEquivalent: "")
    item.target = gettingStarted.target
    if let copyDiagnostics {
      let copy = help.addItem(withTitle: MenuWords.copyDiagnostics, action: copyDiagnostics.action, keyEquivalent: "")
      copy.target = copyDiagnostics.target
    }
    NSApplication.shared.helpMenu = help
    return help
  }
}
