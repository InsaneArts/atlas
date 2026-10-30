/// The callout of a step that did not work: what happened, then why, then what came next.
public struct ViewerActivityProblem: Equatable, Sendable {
  /// Why it did not work, as a sentence, such as "Another app was in front."; empty when no reason was given.
  public var why: String
  /// What came next, as a sentence, such as "The agent tried again, and it worked."; empty while nothing has.
  public var next: String

  /// Creates a callout.
  ///
  /// - Parameters:
  ///   - why: Why it did not work.
  ///   - next: What came next; nothing by default.
  public init(why: String, next: String = "") {
    self.why = why
    self.next = next
  }

  /// Why and what came next, as one line under what happened.
  public var detail: String {
    [why, next]
      .filter { !$0.isEmpty }
      .joined(separator: " ")
  }
}
