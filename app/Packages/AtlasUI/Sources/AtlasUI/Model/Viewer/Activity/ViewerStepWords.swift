/// What a step did, in parts the list sets apart: the verb, what it acted on in semibold, the keys it pressed as
/// keycaps, and a count of repeats, such as "Clicked **Save** ×3" or "Pressed ⌘ S".
public struct ViewerStepWords: Equatable, Sendable {
  /// What was done, such as "Clicked", "Typed into" or "Took a screenshot".
  public var verb: String
  /// What it acted on, such as a button's or a field's name; empty for none.
  public var target: String
  /// The keys pressed, one chord's caps each, such as `[["⌘", "S"]]`; none for any other step.
  public var keys: [[String]]
  /// How many times in a row it was done, such as "×3"; empty for once.
  public var count: String

  /// Creates a step's words.
  ///
  /// - Parameters:
  ///   - verb: What was done.
  ///   - target: What it acted on; none by default.
  ///   - keys: The keys pressed; none by default.
  ///   - count: How many times in a row; empty by default.
  public init(_ verb: String, target: String = "", keys: [[String]] = [], count: String = "") {
    self.verb = verb
    self.target = target
    self.keys = keys
    self.count = count
  }

  /// The words as plain text, for VoiceOver and the search: "Clicked Save ×3", or "Pressed ⌘S".
  public var text: String {
    let chords = keys.map { $0.joined() }
      .joined(separator: " ")
    return [verb, target, chords, count]
      .filter { !$0.isEmpty }
      .joined(separator: " ")
  }
}
