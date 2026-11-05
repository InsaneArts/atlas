import AtlasUI
import Foundation

/// The words of pictures of agent actions: what a mark says, an action's details shown large, and a replay's lines.
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

  /// An action's details line naming what it did.
  static let whatDetail = "What"
  /// The details line naming where.
  static let whereDetail = "Where"
  /// The details line naming who did it.
  static let agentDetail = "Agent"
  /// The details line naming who did it and the task it named.
  static let agentAndTaskDetail = "Agent and task"
  /// The details line saying how it went.
  static let outcomeDetail = "Outcome"
  /// The details line saying when the picture before was taken.
  static let beforeDetail = "Before"
  /// The details line saying when the picture after was taken.
  static let afterDetail = "After"
  /// An action that worked.
  static let worked = "Worked"
  /// An action that failed.
  static let failed = "Failed"
  /// An action that may not have finished.
  static let mayNotHaveFinished = "May not have finished"
  /// An action that sent nothing.
  static let nothingSent = "Nothing sent"

  /// How an action went, and why when it did not work.
  ///
  /// - Parameters:
  ///   - tone: How it went.
  ///   - reason: Why, when it did not work.
  /// - Returns: "Worked", or "Failed: No “Save” in this window".
  static func outcome(_ tone: ViewerMarkTone, reason: String?) -> String {
    let word =
      switch tone {
      case .worked: worked
      case .failed: failed
      case .uncertain: mayNotHaveFinished
      case .notSent: nothingSent
      }
    guard tone != .worked, let reason, !reason.isEmpty else { return word }
    return "\(word): \(reason)"
  }

  /// When the picture before was taken.
  ///
  /// - Parameter time: Its time, to the hundredth.
  /// - Returns: "10:31:09.21, just before the action".
  static func beforeTaken(_ time: String) -> String {
    "\(time), just before the action"
  }

  /// When the picture after was taken, and where it came from.
  ///
  /// - Parameters:
  ///   - time: Its time, to the hundredth; nil when unknown, which leaves it out.
  ///   - ownPicture: Whether Atlas took it after the action, rather than it being the next step's picture before.
  ///   - delay: How long after the picture before it was taken; nil when unknown.
  /// - Returns: "10:31:10.02, a second later", "10:31:10.80, 1.6 seconds later", "Taken after the action" or
  ///   "10:31:10.02, the next step's picture before it".
  static func afterTaken(_ time: String?, ownPicture: Bool, delay: TimeInterval?) -> String {
    let how = ownPicture ? delay.map(later) ?? "taken after the action" : "the next step's picture before it"
    guard let time else { return Words.leadingCapital(how) }
    return "\(time), \(how)"
  }

  /// How long after the picture before one was taken, to the tenth of a second.
  ///
  /// - Parameter delay: The time between them.
  /// - Returns: "a second later", or "1.6 seconds later".
  static func later(_ delay: TimeInterval) -> String {
    let tenths = (max(delay, 0) * 10)
      .rounded()
    guard tenths != 10 else { return "a second later" }
    return "\(Int(tenths) / 10).\(Int(tenths) % 10) seconds later"
  }

  /// A side's label over a large picture.
  ///
  /// - Parameters:
  ///   - side: "Before" or "After".
  ///   - time: When it was taken, to the hundredth; nil when unknown, which leaves it out.
  /// - Returns: "Before · 10:31:09.21", or "After".
  static func sideLabel(_ side: String, _ time: String?) -> String {
    time.map { "\(side) · \($0)" } ?? side
  }

  /// Which step of how many an action shown large is.
  ///
  /// - Parameters:
  ///   - index: Its place, from 0.
  ///   - count: How many.
  /// - Returns: "step 5 of 5".
  static func stepOf(_ index: Int, _ count: Int) -> String {
    "step \(index + 1) of \(count)"
  }

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
  /// - Returns: "Claude Code · Slack · step 5 of 5".
  static func dotted(_ parts: [String]) -> String {
    parts.filter { !$0.isEmpty }
      .joined(separator: " · ")
  }
}
