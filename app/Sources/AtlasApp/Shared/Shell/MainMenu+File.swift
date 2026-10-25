import AppKit

extension MainMenu {
  /// Open Atlas (⌘O), the way back to the main window, then Save Picture…, where the main window can open, and Close.
  ///
  /// - Parameters:
  ///   - viewer: Whether the main window can open, so Save Picture… is offered.
  ///   - openViewer: What Open Atlas does; nil leaves it out.
  /// - Returns: The menu.
  static func fileMenu(viewer: Bool, openViewer: Command? = nil) -> NSMenu {
    let file = NSMenu(title: MenuWords.file)
    if let openViewer {
      let open = file.addItem(withTitle: MenuWords.openAtlas, action: openViewer.action, keyEquivalent: "o")
      open.target = openViewer.target
      file.addItem(.separator())
    }
    if viewer {
      file.addItem(
        withTitle: ViewerWords.savePicture,
        action: #selector(ViewerMenuActions.savePicture(_:)),
        keyEquivalent: "s"
      )
      file.addItem(.separator())
    }
    file.addItem(withTitle: MenuWords.close, action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
    return file
  }
}
