import AppKit
import AtlasUI

/// The sessions menu under its button: All Sessions, ticked while every session's work shows; every live session with
/// its host's icon and, on macOS 14.4 and later, its state and line under its name, as many as there are, the menu
/// scrolling past what fits; then the sessions that ended under their header. Choosing one only changes whose work the
/// window shows: nothing here stops, cleans up or acts in the helper. Built from the presenter's
/// ``ViewerSessionsMenu``; each item asks the window, by the key it represents.
enum SessionsMenu {
  /// The menu's width: the ⋯ menu's, so the bar's menus match.
  static let width = MoreMenu.width

  /// The menu.
  ///
  /// - Parameters:
  ///   - value: What the presenter decided it says.
  ///   - target: The window that answers its items.
  ///   - scale: The display's backing scale, which the icons are drawn at.
  /// - Returns: The menu.
  static func menu(_ value: ViewerSessionsMenu, target: AnyObject, scale: CGFloat) -> NSMenu {
    let menu = NSMenu()
    menu.minimumWidth = width
    let all = NSMenuItem(
      title: ViewerWords.allSessionsItem,
      action: #selector(ViewerMenuActions.chooseSession(_:)),
      keyEquivalent: ""
    )
    all.target = target
    all.state = value.chosen == .all ? .on : .off
    menu.addItem(all)
    menu.addItem(.separator())
    for session in value.live {
      menu.addItem(item(session, chosen: value.chosen, target: target, scale: scale))
    }
    if !value.ended.isEmpty {
      menu.addItem(.sectionHeader(title: ViewerWords.endedHeader))
      for session in value.ended {
        menu.addItem(item(session, chosen: value.chosen, target: target, scale: scale))
      }
    }
    return menu
  }

  /// A session's item: its name, its icon, its state and line where the system shows a subtitle, and a tick while it
  /// is the one chosen.
  ///
  /// - Parameters:
  ///   - session: The session.
  ///   - chosen: Whose work shows now.
  ///   - target: Who answers it.
  ///   - scale: The display's backing scale.
  /// - Returns: The item.
  static func item(
    _ session: ViewerSessionsMenuItem,
    chosen: ViewerSessionChoice,
    target: AnyObject,
    scale: CGFloat
  ) -> NSMenuItem {
    let item = NSMenuItem(
      title: session.title,
      action: #selector(ViewerMenuActions.chooseSession(_:)),
      keyEquivalent: ""
    )
    item.target = target
    item.representedObject = session.key
    item.image = session.icon.menuImage(scale: scale)
    item.state = chosen == .one(session.key) ? .on : .off
    if #available(macOS 14.4, *) { item.subtitle = session.subtitle }
    item.toolTip = session.subtitle
    return item
  }
}
