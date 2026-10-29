import AppKit
import AtlasUI

extension MainMenu {
  /// The Screen and the Library's lists, ⌘1 to ⌘4 in the sidebar's order; Hide Sidebar (⌃⌘S), Pause Picture, Actual
  /// Size and Picture Updates, ticking the rate pictures arrive at; and Enter Full Screen.
  ///
  /// - Returns: The menu.
  static func view() -> NSMenu {
    let view = NSMenu(title: MenuWords.view)
    for place in ViewerPlace.allCases {
      let item = view.addItem(
        withTitle: ViewerWords.place(place),
        action: #selector(ViewerMenuActions.showPlace(_:)),
        keyEquivalent: String(place.menuTag + 1)
      )
      item.tag = place.menuTag
    }
    view.addItem(.separator())
    let sidebar = view.addItem(
      withTitle: ViewerWords.hideSidebarItem,
      action: #selector(ViewerMenuActions.toggleSidebar(_:)),
      keyEquivalent: "s"
    )
    sidebar.keyEquivalentModifierMask = [.command, .control]
    let pause = view.addItem(
      withTitle: Words.pausePictureItem,
      action: #selector(ViewerMenuActions.togglePicturePause(_:)),
      keyEquivalent: "p"
    )
    pause.keyEquivalentModifierMask = [.command, .shift]
    view.addItem(
      withTitle: ViewerWords.actualSizeItem,
      action: #selector(ViewerMenuActions.toggleActualSize(_:)),
      keyEquivalent: "0"
    )
    let updates = view.addItem(withTitle: ViewerWords.updatesItem, action: nil, keyEquivalent: "")
    updates.submenu = pictureUpdates()
    view.addItem(.separator())
    let fullScreen = view.addItem(
      withTitle: ViewerWords.fullScreenItem,
      action: #selector(NSWindow.toggleFullScreen(_:)),
      keyEquivalent: "f"
    )
    fullScreen.keyEquivalentModifierMask = [.command, .control]
    return view
  }

  /// Picture Updates: Live and Every 5 Seconds, the one the picture arrives at, or resumes at, ticked.
  ///
  /// - Returns: The submenu.
  private static func pictureUpdates() -> NSMenu {
    let updates = NSMenu(title: ViewerWords.updatesItem)
    for rate in ViewerPictureRate.allCases {
      let item = updates.addItem(
        withTitle: ViewerWords.rate(rate),
        action: #selector(ViewerMenuActions.chooseRate(_:)),
        keyEquivalent: ""
      )
      item.tag = rate.menuTag
    }
    return updates
  }
}
