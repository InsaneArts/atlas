import AppKit
import AtlasKit
import SwiftUI

/// Atlas's menu bar item and its panel. The glyph follows the helper's
/// mode; the panel reads the desktop only while it is open.
@MainActor
final class MenuBarController: NSObject, NSPopoverDelegate {
    let model: PanelModel
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let popover = NSPopover()

    init(model: PanelModel) {
        self.model = model
        super.init()
        let content = NSHostingController(rootView: PanelView(model: model))
        // The popover follows the panel as rows and messages come and go.
        content.sizingOptions = [.preferredContentSize]
        popover.contentViewController = content
        popover.behavior = .transient
        // Atlas's panel is always dark, like its other windows.
        popover.appearance = NSAppearance(named: .darkAqua)
        popover.delegate = self
        item.button?.target = self
        item.button?.action = #selector(toggle)
        track({ [weak model] in model?.mode }) { [weak self] mode in
            self?.item.button?.image = statusImage(mode)
            self?.item.button?.toolTip = "Atlas — \(mode.label)"
        }
        model.watch(open: false)
        Task { await model.findHelper() }
    }

    @objc private func toggle() {
        guard let button = item.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            // No row is focused until the person presses Tab, as in menus.
            let window = popover.contentViewController?.view.window
            window?.makeFirstResponder(nil)
            Task { window?.makeFirstResponder(nil) }
            model.watch(open: true)
        }
    }

    func closePanel() {
        if popover.isShown { popover.performClose(nil) }
    }

    func popoverDidClose(_ notification: Notification) {
        model.watch(open: false)
    }
}
