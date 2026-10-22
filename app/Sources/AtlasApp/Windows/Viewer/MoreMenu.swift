import AppKit

/// The ⋯ menu under its button (mac.py:347): the picture's choices, Copy and Save Picture, the helper's commands
/// (``HelperMenuItems``) and a note on the helper. Built from the presenter's ``ViewerMoreMenu``; each item asks the
/// window, which validates it against the state its buttons draw, so its key equivalents are only shown.
@MainActor
enum MoreMenu {
  /// 250: the menu's width (mac.py:347).
  static let width: CGFloat = 250

  /// The menu.
  ///
  /// - Parameters:
  ///   - value: What the presenter decided it says.
  ///   - target: The window that answers its items.
  /// - Returns: The menu.
  static func menu(_ value: ViewerMoreMenu, target: AnyObject) -> NSMenu {
    let menu = NSMenu()
    menu.appearance = NSAppearance(named: .darkAqua)
    menu.minimumWidth = width
    menu.addItem(.sectionHeader(title: ViewerWords.pictureHeader))
    for choice in ViewerPictureChoice.allCases {
      let item = NSMenuItem(
        title: title(choice),
        action: #selector(ViewerMenuActions.choosePicture(_:)),
        keyEquivalent: choice == .paused ? "p" : ""
      )
      item.keyEquivalentModifierMask = [.command, .shift]
      item.tag = choice.rawValue
      item.state = choice == value.picture ? .on : .off
      item.target = target
      menu.addItem(item)
    }
    menu.addItem(.separator())
    menu.addItem(item(ViewerWords.copyPicture, #selector(NSText.copy(_:)), key: "c", target: target))
    menu.addItem(item(ViewerWords.savePicture, #selector(ViewerMenuActions.savePicture(_:)), key: "s", target: target))
    menu.addItem(.separator())
    for helperItem in HelperMenuItems.make(target: target) {
      menu.addItem(helperItem)
    }
    menu.addItem(.separator())
    let note = NSMenuItem(title: value.note, action: nil, keyEquivalent: "")
    note.isEnabled = false
    menu.addItem(note)
    return menu
  }

  /// A picture choice's title.
  ///
  /// - Parameter choice: The choice.
  /// - Returns: "Live", "Every 5 seconds" or "Paused".
  static func title(_ choice: ViewerPictureChoice) -> String {
    switch choice {
    case .live: ViewerWords.live
    case .everyFiveSeconds: ViewerWords.everyFiveSeconds
    case .paused: ViewerWords.paused
    }
  }

  /// An item with a Command key equivalent, shown only.
  ///
  /// - Parameters:
  ///   - title: Its title.
  ///   - action: What it asks.
  ///   - key: Its key, with Command.
  ///   - target: Who answers it.
  /// - Returns: The item.
  private static func item(_ title: String, _ action: Selector, key: String, target: AnyObject) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
    item.target = target
    return item
  }
}
