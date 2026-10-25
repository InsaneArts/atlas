import AppKit
import AtlasUI

extension MainMenu {
  /// The tabs, ⌘1 to ⌘4; Show Details, Pause Picture and Actual Size; and Enter Full Screen.
  ///
  /// - Returns: The menu.
  static func view() -> NSMenu {
    let view = NSMenu(title: MenuWords.view)
    for tab in ViewerTab.allCases {
      let item = view.addItem(
        withTitle: ViewerWords.tab(tab),
        action: #selector(ViewerMenuActions.selectTab(_:)),
        keyEquivalent: String(tab.menuTag + 1)
      )
      item.tag = tab.menuTag
    }
    view.addItem(.separator())
    let details = view.addItem(
      withTitle: ViewerWords.showDetailsItem,
      action: #selector(ViewerMenuActions.toggleDetails(_:)),
      keyEquivalent: "i"
    )
    details.keyEquivalentModifierMask = [.command, .option]
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
    view.addItem(.separator())
    let fullScreen = view.addItem(
      withTitle: ViewerWords.fullScreenItem,
      action: #selector(NSWindow.toggleFullScreen(_:)),
      keyEquivalent: "f"
    )
    fullScreen.keyEquivalentModifierMask = [.command, .control]
    return view
  }
}
