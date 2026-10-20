import SwiftUI

extension KeyboardShortcut {
  /// The shortcut as a menu writes it, such as ⌘Q: its modifiers in the system's order, ⌃⌥⇧⌘, then its key.
  var symbols: String {
    let marks: [(modifier: EventModifiers, symbol: String)] = [
      (.control, "⌃"), (.option, "⌥"), (.shift, "⇧"), (.command, "⌘"),
    ]
    let held = marks.filter { modifiers.contains($0.modifier) }
      .map(\.symbol)
    let letter = String(key.character)
      .uppercased()
    return held.joined() + letter
  }
}
