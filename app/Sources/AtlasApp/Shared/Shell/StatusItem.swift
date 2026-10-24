import AppKit

/// Atlas's item in the menu bar: it owns the system's status item and its button, which the panel's popover opens
/// from (``PanelPopover``). Variable length, so the button is as wide as its glyph.
final class StatusItem {
  /// The system's item, kept in the menu bar while this lives.
  private let item = NSStatusBar.system
    .statusItem(withLength: NSStatusItem.variableLength)

  /// The item's button: the panel's anchor, showing the helper's mode.
  var button: NSStatusBarButton? { item.button }
}
