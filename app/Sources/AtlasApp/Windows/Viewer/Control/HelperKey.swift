/// One key Send Key offers: its name, its keys as a menu draws them, and the key sent to the helper.
struct HelperKey {
  /// The key's name, such as "Switch Apps".
  let name: String
  /// Its keys as a menu draws them, such as "⌘⇥".
  let glyphs: String
  /// The key sent to the helper, as the service names it.
  let sent: String

  /// Its menu item's title: its name with its keys after it. The keys are only words in the title, never the item's
  /// key equivalent, which would make them this app's shortcuts, ⌘Q and ⌘W among them.
  var title: String {
    ShortcutWords.help(name, glyphs)
  }

  /// The keys this Mac would keep for itself, typed while in control.
  static let mac = [
    HelperKey(name: "Switch Apps", glyphs: "⌘⇥", sent: "cmd+tab"),
    HelperKey(name: "Spotlight", glyphs: "⌘Space", sent: "cmd+space"),
    HelperKey(name: "Mission Control", glyphs: "⌃↑", sent: "ctrl+up"),
  ]

  /// The keys an app would keep, Atlas included, typed while in control.
  static let app = [
    HelperKey(name: "Close Window", glyphs: "⌘W", sent: "cmd+w"),
    HelperKey(name: "Quit App", glyphs: "⌘Q", sent: "cmd+q"),
    HelperKey(name: "Escape", glyphs: "⎋", sent: "escape"),
  ]

  /// Every key, the Mac's first, in the menu's order; an item's tag is its key's place here.
  static let all = mac + app

  /// The key at `index` in ``all``, as an item's tag names it.
  ///
  /// - Parameter index: The key's place.
  /// - Returns: The key, or nil for a place with none.
  static func key(at index: Int) -> HelperKey? {
    all.indices.contains(index) ? all[index] : nil
  }
}
