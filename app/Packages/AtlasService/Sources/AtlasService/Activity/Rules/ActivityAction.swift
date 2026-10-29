/// What kind of thing a row did in the helper, read from the verb Atlas wrote for it, so the viewer can mark each
/// line with its kind. A row from a build that sent no verb reads by its words, which lead with that verb ("clicked
/// Save in TextEdit", "couldn't scroll", "opening TextEdit…"); a row that says nothing Atlas knows reads by its
/// operation.
public enum ActivityAction: Equatable, Sendable {
  /// A click, a drag or the pointer moved, and mouse-and-keyboard input with no finer verb.
  case pointer
  /// Text typed.
  case typing
  /// Keys pressed.
  case keys
  /// A scroll.
  case scroll
  /// A screenshot taken.
  case screenshot
  /// An app, a file or a link opened, apps closed, an app installed, or a task's work cleaned up.
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
    default:
      self =
        Self.verb(entry.verb)
        ?? Self.said(entry.says)
        ?? Self.operation(entry.operation)
    }
  }

  /// The kind the verb Atlas wrote for a row names.
  ///
  /// - Parameter verb: The row's verb, such as `click` or `job-start`.
  /// - Returns: The kind, or nil for no verb or one this app does not know.
  static func verb(_ verb: String?) -> ActivityAction? {
    verb.flatMap { written[$0] }
  }

  /// The kind the service's words name by their verb, after any "couldn't", "didn't" or "stopped".
  ///
  /// - Parameter says: What the row says it did, such as "couldn't click Save" or "didn't type in Notes".
  /// - Returns: The kind, or nil for words that lead with no verb Atlas writes.
  static func said(_ says: String?) -> ActivityAction? {
    guard let says else { return nil }
    let lead = leads.first { says.hasPrefix($0) } ?? ""
    guard
      let verb = says.dropFirst(lead.count)
        .split(separator: " ")
        .first
    else { return nil }
    return verbs[String(verb)]
  }

  /// The kind a row's operation names, for words that name none.
  ///
  /// - Parameter operation: The operation, such as `input` or `run`.
  /// - Returns: The pointer for input, a command for anything else.
  static func operation(_ operation: String) -> ActivityAction {
    operation == "input" ? .pointer : .command
  }

  /// How the service's words start when something could not be done, was not done as another app was in front, or
  /// was cut short.
  private static let leads = ["couldn't ", "didn't ", "stopped "]

  /// The verbs Atlas writes for rows (`src/lib/doing.rs`), by their kind.
  private static let written: [String: ActivityAction] = [
    "click": .pointer, "double-click": .pointer, "right-click": .pointer, "drag": .pointer, "move": .pointer,
    "type": .typing, "key": .keys, "scroll": .scroll, "screenshot": .screenshot, "open": .window,
    "open-link": .window, "close": .window, "install": .window, "cleanup": .window, "run": .command,
    "job-start": .command, "job-stop": .command, "collect": .command,
  ]

  /// The verbs the service's words lead with (`src/lib/doing.rs`), done, failed and under way, by their kind.
  private static let verbs: [String: ActivityAction] = [
    "clicked": .pointer, "click": .pointer, "double-clicked": .pointer, "double-click": .pointer,
    "right-clicked": .pointer, "right-click": .pointer, "dragged": .pointer, "drag": .pointer,
    "moved": .pointer, "move": .pointer, "clicking": .pointer, "dragging": .pointer, "typed": .typing,
    "type": .typing, "typing": .typing, "pressed": .keys, "press": .keys, "pressing": .keys, "scrolled": .scroll,
    "scroll": .scroll, "scrolling": .scroll, "opened": .window, "open": .window, "opening": .window,
    "closed": .window, "close": .window, "closing": .window, "installed": .window, "install": .window,
    "installing": .window, "cleaned": .window, "clean": .window, "cleaning": .window, "took": .screenshot,
    "take": .screenshot,
    "ran": .command, "run": .command, "running": .command,
  ]
}
