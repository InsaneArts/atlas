/// What kind of thing a row did in the helper, read from what the service says it did and its operation, so
/// the viewer can mark each line with its kind. The service's words lead with the verb Atlas wrote for the
/// row ("clicked Save in TextEdit", "couldn't scroll", "opening TextEdit…"), so the verb decides; a row that
/// says nothing Atlas knows reads by its operation.
public enum ActivityAction: Equatable, Sendable {
  /// A click, a drag or the pointer moved, and mouse-and-keyboard input with no finer verb.
  case pointer
  /// Text typed or keys pressed.
  case keys
  /// A scroll.
  case scroll
  /// A screenshot taken.
  case screenshot
  /// An app, a file or a link opened, or apps closed.
  case window
  /// Control of the helper changing hands, such as given back.
  case control
  /// A command run, or anything else Atlas did in the helper.
  case command

  /// The kind of thing `entry` did.
  ///
  /// - Parameter entry: The row.
  public init(_ entry: ActivityLog.Entry) {
    switch entry.operation {
    case "screenshot": self = .screenshot
    case "acquire", "release": self = .control
    default: self = Self.said(entry.says) ?? Self.operation(entry.operation)
    }
  }

  /// The kind the service's words name by their verb, after any "couldn't".
  ///
  /// - Parameter says: What the row says it did, such as "couldn't click Save".
  /// - Returns: The kind, or nil for words that lead with no verb Atlas writes.
  static func said(_ says: String?) -> ActivityAction? {
    guard let says else { return nil }
    let words = says.hasPrefix(failedLead) ? says.dropFirst(failedLead.count) : Substring(says)
    guard let verb = words.split(separator: " ").first else { return nil }
    return verbs[String(verb)]
  }

  /// The kind a row's operation names, for words that name none.
  ///
  /// - Parameter operation: The operation, such as `input` or `run`.
  /// - Returns: The pointer for input, a command for anything else.
  static func operation(_ operation: String) -> ActivityAction {
    operation == "input" ? .pointer : .command
  }

  /// How the service's words start when something could not be done.
  private static let failedLead = "couldn't "

  /// The verbs the service's words lead with (`src/lib/doing.rs`), done, failed and under way, by their kind.
  private static let verbs: [String: ActivityAction] = [
    "clicked": .pointer, "click": .pointer, "double-clicked": .pointer, "double-click": .pointer,
    "right-clicked": .pointer, "right-click": .pointer, "dragged": .pointer, "drag": .pointer,
    "moved": .pointer, "move": .pointer, "typed": .keys, "type": .keys, "pressed": .keys, "press": .keys,
    "scrolled": .scroll, "scroll": .scroll, "opened": .window, "open": .window, "opening": .window,
    "closed": .window, "close": .window, "closing": .window, "took": .screenshot, "take": .screenshot,
    "ran": .command, "run": .command, "running": .command,
  ]
}
