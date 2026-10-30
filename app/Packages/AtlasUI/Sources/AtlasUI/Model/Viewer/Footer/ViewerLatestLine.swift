/// The footer's one line at its leading edge: the newest thing to know, such as the latest action with its session's
/// icon and its age, why Take Control was refused, a notice, what is being typed, or the helper's state.
public struct ViewerLatestLine: Equatable, Sendable {
  /// Which line this is, such as the action's id: a new id swaps the line, the same id with new words or a new age
  /// updates it in place.
  public var id: String
  /// The icon of the host session the line is about, before the words; nil for none.
  public var icon: RowIcon?
  /// What the line says.
  public var words: String
  /// How long ago, in words, after them; empty for none.
  public var age: String
  /// How the line reads: blue with a dot while this window holds control, orange with a mark when something went
  /// wrong, quiet otherwise.
  public var tone: StatusTone
  /// Whether a caret follows the words, as text being typed.
  public var caret: Bool
  /// Whether VoiceOver says the line as it arrives: a notice or a refusal, which ask to be heard; not the latest
  /// action, which comes every second or so, nor the helper's state, which the window says on its own once it holds.
  public var announces: Bool

  /// Creates the line.
  ///
  /// - Parameters:
  ///   - id: Which line this is.
  ///   - icon: The icon of the session the line is about; none by default.
  ///   - words: What the line says.
  ///   - age: How long ago, in words; none by default.
  ///   - tone: How the line reads; quiet by default.
  ///   - caret: Whether a caret follows the words; no by default.
  ///   - announces: Whether VoiceOver says the line as it arrives; no by default.
  public init(
    id: String,
    icon: RowIcon? = nil,
    words: String,
    age: String = "",
    tone: StatusTone = .quiet,
    caret: Bool = false,
    announces: Bool = false
  ) {
    self.id = id
    self.icon = icon
    self.words = words
    self.age = age
    self.tone = tone
    self.caret = caret
    self.announces = announces
  }
}
