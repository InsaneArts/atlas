/// What a step did, in parts the list sets apart: the verb, what was typed in quotes, what it acted on in semibold,
/// the keys it pressed as keycaps, and a count of repeats, such as "Clicked **Save** ×3", "Pressed ⌘ S", "Pressed ⏎ in
/// **Message**" or "Typed “Release 0.1.1…” into **Message**".
public struct ViewerStepWords: Equatable, Sendable {
  /// What was done, such as "Clicked", "Typed into" or "Took a screenshot".
  public var verb: String
  /// What was typed, shortened to fit one line; empty for none.
  public var quote: String
  /// The word between what was typed, or the keys pressed, and what it acted on, such as "into" or "in"; empty for
  /// none.
  public var joiner: String
  /// What it acted on, such as a button's or a field's name; empty for none.
  public var target: String
  /// The keys pressed, one chord's caps each, such as `[["⌘", "S"]]`; none for any other step.
  public var keys: [[String]]
  /// The key's name after its cap, such as "Return"; empty for keys that need none.
  public var keyName: String
  /// How many times in a row it was done, such as "×3"; empty for once.
  public var count: String

  /// Creates a step's words.
  ///
  /// - Parameters:
  ///   - verb: What was done.
  ///   - quote: What was typed; none by default.
  ///   - joiner: The word between what was typed or the keys and what it acted on; none by default.
  ///   - target: What it acted on; none by default.
  ///   - keys: The keys pressed; none by default.
  ///   - keyName: The key's name after its cap; none by default.
  ///   - count: How many times in a row; empty by default.
  public init(
    _ verb: String,
    quote: String = "",
    joiner: String = "",
    target: String = "",
    keys: [[String]] = [],
    keyName: String = "",
    count: String = ""
  ) {
    self.verb = verb
    self.quote = quote
    self.joiner = joiner
    self.target = target
    self.keys = keys
    self.keyName = keyName
    self.count = count
  }

  /// The words as plain text, for VoiceOver and the search: "Clicked Save ×3", "Pressed ⌘S", "Pressed ⏎ Return in
  /// Message", or "Typed “Hello” into Name".
  public var text: String {
    let chords = keys.map { $0.joined() }
      .joined(separator: " ")
    let parts =
      keys.isEmpty
      ? [verb, quoted, quote.isEmpty ? "" : joiner, target, count]
      : [verb, chords, keyName, target.isEmpty ? "" : joiner, target, count]
    return parts.filter { !$0.isEmpty }
      .joined(separator: " ")
  }

  /// What was typed in quotes; empty for none.
  public var quoted: String {
    quote.isEmpty ? "" : "“\(quote)”"
  }
}
