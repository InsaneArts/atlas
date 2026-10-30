import AtlasControl
import AtlasUI

/// The tooltip of every button that has a menu item: the item's exact title and its shortcut, so a shortcut can be
/// found from its button, and the same button says the same thing in every window, the panel's and the main window's.
enum ShortcutWords {
  /// Stop Agent (⌘.).
  static let stopAgent = help(ViewerWords.stopAgentItem, "⌘.")
  /// Resume Agent (⌘.).
  static let resumeAgent = help(ViewerWords.resumeAgentItem, "⌘.")
  /// Pause Picture (⇧⌘P).
  static let pausePicture = help(Words.pausePictureItem, "⇧⌘P")
  /// Resume Picture (⇧⌘P).
  static let resumePicture = help(Words.resumePictureItem, "⇧⌘P")
  /// Take Control (⌃⇧⎋).
  static let takeControl = help(ViewerWords.takeControl, KeyMap.releaseSymbols)
  /// Give Back Control (⌃⇧⎋).
  static let giveBack = help(ViewerWords.giveBackItem, KeyMap.releaseSymbols)

  /// A place's tooltip: its word and its View menu shortcut.
  ///
  /// - Parameter place: The place.
  /// - Returns: "Screen (⌘1)".
  static func place(_ place: ViewerPlace) -> String {
    help(ViewerWords.place(place), "⌘\(place.menuTag + 1)")
  }

  /// A menu item's title with its shortcut after it.
  ///
  /// - Parameters:
  ///   - title: The menu item's exact title.
  ///   - shortcut: Its shortcut, as the menu draws it.
  /// - Returns: "Stop Agent (⌘.)".
  static func help(_ title: String, _ shortcut: String) -> String {
    "\(title) (\(shortcut))"
  }
}
