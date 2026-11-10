import AtlasUI
import Foundation

/// The words of pictures of agent actions: what a mark says, and a replay's lines.
extension ViewerWords {
  /// What typing into a password field says in place of what was typed.
  static let aPassword = "a password"
  /// What a password shows in its bubble, whatever its length.
  static let passwordDots = "••••••••"
  /// A password bubble's quiet line.
  static let passwordDetail = "A password"
  /// 36: the most characters of typed text a step's line shows.
  static let quoteLength = 36
  /// The last frame of a replay.
  static let afterFrame = "After"
  /// The last frame of a replay's words.
  static let screenAfter = "The screen after the last step"

  /// What was typed, on one line and shortened at a word to ``quoteLength`` characters with an ellipsis.
  ///
  /// - Parameter text: What was typed.
  /// - Returns: "Release 0.1.1 is out: faster setup…".
  static func shortened(_ text: String) -> String {
    let line = text.split(whereSeparator: \.isWhitespace)
      .joined(separator: " ")
    guard line.count > quoteLength else { return line }
    let cut = line.prefix(quoteLength)
    let space = cut.lastIndex(of: " ")
    let word = space.map { cut[..<$0] } ?? cut
    return "\(word.trimmingCharacters(in: .punctuationCharacters))…"
  }

  /// The names of the keys a step's cap does not say by itself, by their caps.
  static let keyNames = [
    "↩": "Return", "⏎": "Return", "⌤": "Return", "⇥": "Tab", "⎋": "Esc", "⌫": "Delete", "␣": "Space",
    "↑": "Up Arrow", "↓": "Down Arrow", "←": "Left Arrow", "→": "Right Arrow",
  ]

  /// The name of a lone key whose cap is a symbol, such as Return for ↩; none for a shortcut or a key whose cap says
  /// its name, since Atlas does not know what a shortcut does.
  ///
  /// - Parameter keys: One chord's caps each.
  /// - Returns: "Return", or empty.
  static func keyName(_ keys: [[String]]) -> String {
    guard keys.count == 1, let chord = keys.first, chord.count == 1, let cap = chord.first else { return "" }
    return keyNames[cap] ?? ""
  }

  /// A typing bubble's quiet line.
  ///
  /// - Parameter length: How many characters were typed, when known.
  /// - Returns: "Typed · 70 characters", or "Typed".
  static func typedDetail(_ length: Int?) -> String {
    guard let length else { return "Typed" }
    return "Typed · \(length) \(counted(length, "character"))"
  }

  /// A scroll mark's tag: the way the page moved and how many times.
  ///
  /// - Parameters:
  ///   - arrow: The way.
  ///   - count: How many scrolls; one says none.
  /// - Returns: "Down ×12", or "Down".
  static func scrollTag(_ arrow: ViewerScrollArrow, count: Int) -> String {
    let way =
      switch arrow {
      case .up: "Up"
      case .down: "Down"
      case .left: "Left"
      case .right: "Right"
      }
    return count > 1 ? "\(way) ×\(count)" : way
  }

  /// An action that may not have finished.
  static let mayNotHaveFinished = "May not have finished"

  /// How many steps a replay holds.
  ///
  /// - Parameter count: How many.
  /// - Returns: "5 steps".
  static func steps(_ count: Int) -> String {
    "\(count) \(counted(count, "step"))"
  }

  /// From when to when.
  ///
  /// - Parameters:
  ///   - start: The first time.
  ///   - end: The last time.
  /// - Returns: "10:30:41 to 10:31:10", or the one time when they are the same.
  static func span(_ start: String, _ end: String) -> String {
    start == end ? start : "\(start) to \(end)"
  }

  /// How many repeats a folded step holds.
  ///
  /// - Parameter count: How many.
  /// - Returns: "12 in one step".
  static func folded(_ count: Int) -> String {
    "\(count) in one step"
  }

  /// Words joined by middle dots, the empty ones left out.
  ///
  /// - Parameter parts: The words.
  /// - Returns: "Claude Code · Slack · 5 steps".
  static func dotted(_ parts: [String]) -> String {
    parts.filter { !$0.isEmpty }
      .joined(separator: " · ")
  }
}
