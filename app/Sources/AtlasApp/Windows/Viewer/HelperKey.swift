import AppKit

/// One key the Keys menu offers: its name, the key equivalent shown beside it with its modifiers, and the key sent
/// to the helper.
struct HelperKey {
  /// The menu item's title.
  let name: String
  /// The key equivalent shown beside it.
  let shown: String
  /// The modifiers shown with it.
  let modifiers: NSEvent.ModifierFlags
  /// The key sent to the helper, as the service names it.
  let sent: String

  /// The keys the Mac keeps for itself (⌘⇥, ⌘Space…) and a few common ones, in the menu's groups.
  static let groups: [[HelperKey]] = [
    [
      HelperKey(name: "Escape", shown: "\u{1b}", modifiers: [], sent: "escape"),
      HelperKey(name: "Tab", shown: "\t", modifiers: [], sent: "tab"),
      HelperKey(name: "Return", shown: "\r", modifiers: [], sent: "return"),
    ],
    [
      HelperKey(name: "Copy", shown: "c", modifiers: .command, sent: "cmd+c"),
      HelperKey(name: "Paste", shown: "v", modifiers: .command, sent: "cmd+v"),
      HelperKey(name: "Select All", shown: "a", modifiers: .command, sent: "cmd+a"),
      HelperKey(name: "Undo", shown: "z", modifiers: .command, sent: "cmd+z"),
      HelperKey(name: "Redo", shown: "z", modifiers: [.command, .shift], sent: "cmd+shift+z"),
      HelperKey(name: "Save", shown: "s", modifiers: .command, sent: "cmd+s"),
    ],
    [
      HelperKey(name: "Switch App", shown: "\t", modifiers: .command, sent: "cmd+tab"),
      HelperKey(name: "Spotlight", shown: " ", modifiers: .command, sent: "cmd+space"),
      HelperKey(name: "Close Window", shown: "w", modifiers: .command, sent: "cmd+w"),
      HelperKey(name: "Quit App", shown: "q", modifiers: .command, sent: "cmd+q"),
    ],
  ]
}
