import AppKit

/// The ⋯ menu under its button: only what acts on this window's picture. Copy and Save, for the picture or, on the
/// Screenshots tab, the screenshot shown large or selected; Actual Size, ticked while on; then how often pictures
/// arrive under its header, ticking the rate the picture resumes at while paused, so the menu never contradicts the
/// pause button, which with ⇧⌘P alone pauses. Built from the presenter's ``ViewerMoreMenu``; each item asks the window,
/// which validates it against the state its buttons draw, so its key equivalents are only shown.
enum MoreMenu {
  /// 250: the menu's width.
  static let width: CGFloat = 250

  /// The menu.
  ///
  /// - Parameters:
  ///   - value: What the presenter decided it says.
  ///   - target: The window that answers its items.
  /// - Returns: The menu.
  static func menu(_ value: ViewerMoreMenu, target: AnyObject) -> NSMenu {
    let menu = NSMenu()
    menu.minimumWidth = width
    let copy = value.actsOnShot ? ViewerWords.copyScreenshot : ViewerWords.copyPicture
    let save = value.actsOnShot ? ViewerWords.saveScreenshot : ViewerWords.savePicture
    menu.addItem(item(copy, #selector(NSText.copy(_:)), key: "c", target: target))
    menu.addItem(item(save, #selector(ViewerMenuActions.savePicture(_:)), key: "s", target: target))
    let actualSize = item(
      ViewerWords.actualSizeItem,
      #selector(ViewerMenuActions.toggleActualSize(_:)),
      key: "0",
      target: target
    )
    actualSize.state = value.actualSize ? .on : .off
    menu.addItem(actualSize)
    menu.addItem(.separator())
    menu.addItem(.sectionHeader(title: ViewerWords.updatesHeader))
    for rate in ViewerPictureRate.allCases {
      let choice = NSMenuItem(
        title: title(rate),
        action: #selector(ViewerMenuActions.chooseRate(_:)),
        keyEquivalent: ""
      )
      choice.tag = rate.menuTag
      choice.state = rate == value.rate ? .on : .off
      choice.target = target
      menu.addItem(choice)
    }
    return menu
  }

  /// A rate's title.
  ///
  /// - Parameter rate: The rate.
  /// - Returns: "Live" or "Every 5 Seconds".
  static func title(_ rate: ViewerPictureRate) -> String {
    switch rate {
    case .live: ViewerWords.live
    case .everyFiveSeconds: ViewerWords.everyFiveSeconds
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
