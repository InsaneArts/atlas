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

  /// The keys this Mac would keep for itself, typed while in control.
  static let mac = [
    HelperKey(name: "Switch Apps", shown: "\t", modifiers: .command, sent: "cmd+tab"),
    HelperKey(name: "Spotlight", shown: " ", modifiers: .command, sent: "cmd+space"),
    // U+F700 is AppKit's Up Arrow key (NSUpArrowFunctionKey), which a menu draws as ↑.
    HelperKey(name: "Mission Control", shown: "\u{F700}", modifiers: .control, sent: "ctrl+up"),
  ]

  /// The keys an app would keep, Atlas included, typed while in control.
  static let app = [
    HelperKey(name: "Close Window", shown: "w", modifiers: .command, sent: "cmd+w"),
    HelperKey(name: "Quit App", shown: "q", modifiers: .command, sent: "cmd+q"),
    HelperKey(name: "Escape", shown: "\u{1b}", modifiers: [], sent: "escape"),
  ]
}
