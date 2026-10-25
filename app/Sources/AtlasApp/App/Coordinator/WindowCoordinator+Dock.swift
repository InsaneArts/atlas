import AppKit

/// The Dock icon's menu and the Help menu's diagnostics: what the main window can do from the Dock while it is open,
/// and what \`atlas doctor\` says, copied for a bug report.
extension WindowCoordinator {
  /// The Dock icon's menu while the main window is open: Take Control or Give Back Control, Stop or Resume Agent, and
  /// Pause or Resume Picture, each asking the main window, which titles and validates it as its own menus' items; no
  /// line of status, which the window says. None while the main window is closed.
  ///
  /// - Returns: The menu, or nil for none.
  func dockMenu() -> NSMenu? {
    guard let viewer, viewer.window.isPresented else { return nil }
    let menu = NSMenu()
    let items: [(String, Selector)] = [
      (ViewerWords.takeControl, #selector(ViewerMenuActions.toggleControl(_:))),
      (ViewerWords.stopAgentItem, #selector(ViewerMenuActions.toggleAgent(_:))),
      (Words.pausePictureItem, #selector(ViewerMenuActions.togglePicturePause(_:))),
    ]
    for (title, action) in items {
      let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
      item.target = viewer.window
      menu.addItem(item)
    }
    return menu
  }

  /// Copy Diagnostics: what \`atlas doctor\` says goes on the clipboard; the key viewer or an app-level notice says how it went.
  func copyDiagnostics() async {
    let copied = await parts?.copyDiagnostics() ?? false
    let text = copied ? MenuWords.diagnosticsCopied : MenuWords.diagnosticsFailed
    if let viewer, viewer.window.isKeyWindow {
      viewer.controller.say(text)
    } else {
      diagnosticsNotice(text)
    }
  }

  /// Shows a result through the invoking window's sheet, or a modal notice when no window is key.
  ///
  /// - Parameter text: The result of Copy Diagnostics.
  static func showDiagnosticsNotice(_ text: String) {
    let alert = NSAlert()
    alert.messageText = text
    if let window = NSApp.keyWindow {
      alert.beginSheetModal(for: window)
    } else {
      alert.runModal()
    }
  }
}
