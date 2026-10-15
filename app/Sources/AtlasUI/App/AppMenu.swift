import AppKit

/// The app's menus: Quit and Hide, Edit (so typing and pasting work in
/// fields as in any app), and Close for the window in front. `quit` is
/// what Quit Atlas does, when not simply quitting.
@MainActor
public func appMenu(quit: (target: AnyObject, action: Selector)? = nil) -> NSMenu {
    let menu = NSMenu()
    let app = NSMenu(title: "Atlas")
    app.addItem(withTitle: "Hide Atlas", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    app.addItem(.separator())
    let quitItem = app.addItem(
        withTitle: "Quit Atlas", action: quit?.action ?? #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    quitItem.target = quit?.target
    let edit = NSMenu(title: "Edit")
    edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
    let redo = edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
    redo.keyEquivalentModifierMask = [.command, .shift]
    edit.addItem(.separator())
    edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    let window = NSMenu(title: "Window")
    window.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
    window.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
    for submenu in [app, edit, window] {
        let item = NSMenuItem()
        item.submenu = submenu
        menu.addItem(item)
    }
    return menu
}
