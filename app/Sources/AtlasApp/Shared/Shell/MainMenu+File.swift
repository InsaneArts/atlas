import AppKit

extension MainMenu {
  /// Save Picture…, where the main window can open, and Close.
  ///
  /// - Parameter viewer: Whether the main window can open, so Save Picture… is offered.
  /// - Returns: The menu.
  static func fileMenu(viewer: Bool) -> NSMenu {
    let file = NSMenu(title: MenuWords.file)
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
