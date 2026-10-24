/// A status at a glance: its word, how it reads, its mark and the line beside it.
public struct StatusSummary: Equatable, Sendable {
  /// The status in words, such as "Working".
  public var label: String
  /// How it reads at a glance, which its mark's and its word's colours show.
  public var tone: StatusTone
  /// The mark beside the word.
  public var mark: StatusMark
  /// The line beside the word, such as "Turning Show Center off in Opie"; empty for none.
  public var line: String

  /// Creates a status at a glance.
  ///
  /// - Parameters:
  ///   - label: The status in words, such as "Working".
  ///   - tone: How it reads at a glance, which its mark's and its word's colours show.
  ///   - mark: The mark beside the word.
  ///   - line: The line beside the word; empty for none.
  public init(label: String, tone: StatusTone, mark: StatusMark, line: String = "") {
    self.label = label
    self.tone = tone
    self.mark = mark
    self.line = line
  }
}
