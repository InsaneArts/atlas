import AppKit

/// The panel's shortcuts: ⌘O, ⌘, and ⌘Q, the same keys its ⋯ menu shows, and ⌘. and ⇧⌘P, the keys its Stop or Resume
/// and its pause button name in their tooltips, as the main menu's items have them. While the panel is open Atlas
/// has no main menu to answer them, so the panel answers them itself.
enum PanelKeys {
  /// Open Atlas: ⌘O.
  static let openAtlas = "o"
  /// Settings…: ⌘,.
  static let settings = ","
  /// Quit Atlas: ⌘Q.
  static let quit = "q"
  /// Stop Agent or Resume Agent: ⌘.
  static let stopOrResume = "."
  /// Pause Picture or Resume Picture: ⇧⌘P.
  static let pausePicture = "p"

  /// What a key press asks of the panel.
  enum Shortcut: Equatable, Sendable {
    /// Open the main window.
    case openAtlas
    /// Open Settings.
    case openSettings
    /// Quit Atlas.
    case quit
    /// Stop the agent, or resume agents once stopped.
    case stopOrResume
    /// Pause the panel's picture, or resume it.
    case pausePicture
  }

  /// The shortcut a key press is: one of the four keys with Command alone, or P with Shift and Command; nothing for
  /// anything else.
  ///
  /// - Parameters:
  ///   - characters: The key's characters, ignoring modifiers but Shift.
  ///   - modifiers: The modifier keys held.
  /// - Returns: The shortcut, or nil.
  static func shortcut(characters: String?, modifiers: NSEvent.ModifierFlags) -> Shortcut? {
    let held = modifiers.intersection(.deviceIndependentFlagsMask)
    if held == [.command, .shift] {
      return characters?.lowercased() == pausePicture ? .pausePicture : nil
    }
    guard held == .command else { return nil }
    switch characters {
    case openAtlas: return .openAtlas
    case settings: return .openSettings
    case quit: return .quit
    case stopOrResume: return .stopOrResume
    default: return nil
    }
  }
}
