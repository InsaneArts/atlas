#if DEBUG
  import AppKit

  extension KeyedWindow {
    /// A key the person presses.
    enum Key {
      /// Return, which presses the default button.
      case `return`
      /// Escape, which presses the cancel button.
      case escape
      /// Command-[, which goes back, as in Setup Assistant.
      case commandBracket

      /// What the key types.
      var characters: String {
        switch self {
        case .return: "\r"
        case .escape: "\u{1b}"
        case .commandBracket: "["
        }
      }

      /// The key's code on the keyboard.
      var code: UInt16 {
        switch self {
        case .return: 36
        case .escape: 53
        case .commandBracket: 33
        }
      }

      /// The modifiers held with it.
      var modifiers: NSEvent.ModifierFlags {
        self == .commandBracket ? .command : []
      }
    }
  }
#endif
