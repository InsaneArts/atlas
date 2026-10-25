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
  /// Show Details (⌥⌘I).
  static let showDetails = help(ViewerWords.showDetailsItem, "⌥⌘I")
  /// Hide Details (⌥⌘I).
  static let hideDetails = help(ViewerWords.hideDetailsItem, "⌥⌘I")
  /// Take Control (⌃⇧⎋).
  static let takeControl = help(ViewerWords.takeControl, KeyMap.releaseSymbols)
  /// Give Back Control (⌃⇧⎋).
  static let giveBack = help(ViewerWords.giveBackItem, KeyMap.releaseSymbols)

  /// A tab's tooltip: its word and its View menu shortcut.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: "Screen (⌘1)".
  static func tab(_ tab: ViewerTab) -> String {
    help(ViewerWords.tab(tab), "⌘\(tab.menuTag + 1)")
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
