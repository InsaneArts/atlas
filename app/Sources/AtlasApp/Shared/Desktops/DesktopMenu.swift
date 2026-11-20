import AppKit
import AtlasUI

/// The menu that picks the desktop Atlas shows, as the main window's sidebar foot and the panel's header open it: each
/// desktop with its dot, its title and its state under it, the one shown checked, then, where it is asked for, Desktop
/// Settings…. Picking the desktop shown does nothing more.
enum DesktopMenu {
  /// 200: the menu's least width, as the panel's popup lists the desktops.
  static let width: CGFloat = 200
  /// The item that opens Settings on the desktops.
  static let settingsItem = "Desktop Settings…"

  /// The menu.
  ///
  /// - Parameters:
  ///   - items: The desktops, in the settings' order.
  ///   - view: Shows a desktop picked, by its ID.
  ///   - openSettings: Opens Settings on the desktops; nil leaves out Desktop Settings….
  /// - Returns: The menu.
  static func menu(
    _ items: [DesktopItem],
    view: @escaping @MainActor (String) -> Void,
    openSettings: (@MainActor () -> Void)? = nil
  ) -> NSMenu {
    let menu = NSMenu()
    menu.minimumWidth = width
    for desktop in items {
      menu.addItem(item(desktop, view: view))
    }
    if let openSettings {
      menu.addItem(.separator())
      menu.addItem(HandlerMenuItem(title: settingsItem, handler: openSettings))
    }
    return menu
  }

  /// One desktop's item: its dot, its title, its state under it, and a check while it is shown.
  ///
  /// - Parameters:
  ///   - desktop: The desktop.
  ///   - view: Shows it when picked, unless it is shown already.
  /// - Returns: The item.
  static func item(_ desktop: DesktopItem, view: @escaping @MainActor (String) -> Void) -> NSMenuItem {
    let item = HandlerMenuItem(title: desktop.title) {
      if !desktop.isViewed { view(desktop.id) }
    }
    // A menu item's second line is macOS 14.4's; before it, the state follows the title.
    if #available(macOS 14.4, *) {
      item.subtitle = desktop.word
    } else {
      item.title = "\(desktop.title) · \(desktop.word)"
    }
    item.image = DesktopMenuDot.image(tone: desktop.tone, mark: desktop.mark)
    item.state = desktop.isViewed ? .on : .off
    item.setAccessibilityLabel("\(desktop.title), \(desktop.word)")
    return item
  }
}
