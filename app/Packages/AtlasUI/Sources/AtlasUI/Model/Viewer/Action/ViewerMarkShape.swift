import CoreGraphics

/// What an action's mark draws, one shape per kind, so a click never looks like a keystroke.
public enum ViewerMarkShape: Equatable, Sendable {
  /// A ring on the spot and a dot where the pointer landed, two for a double click.
  ///
  /// - Parameter clicks: How many times it clicked.
  case click(clicks: Int)
  /// A ring on the field with what was typed in a bubble above it.
  ///
  /// - Parameters:
  ///   - text: What was typed, or eight dots for a password.
  ///   - detail: The bubble's quiet line, such as "Typed · 70 characters" or "A password".
  case typing(text: String, detail: String)
  /// The keys as keycaps near the spot, with the key's name after them, and a soft ring where they went.
  ///
  /// - Parameters:
  ///   - keys: One chord's caps each, such as `[["⌘", "S"]]`.
  ///   - name: The key's name, such as "Return"; empty for keys that need none.
  case keys([[String]], name: String = "")
  /// A pill with an arrow the way the page moved, and how far, such as "Down ×12".
  ///
  /// - Parameters:
  ///   - arrow: The way the page moved.
  ///   - label: The direction and the count.
  case scroll(arrow: ViewerScrollArrow, label: String)
  /// A dot where it started, a dashed path, and a ring where it was dropped.
  ///
  /// - Parameter to: Where it was dropped, as a share of the picture's width and height.
  case drag(to: CGPoint)
  /// A ring with no dot, as the pointer never moved; with no spot, a plain tag that says it was pressed by name.
  case press
}
