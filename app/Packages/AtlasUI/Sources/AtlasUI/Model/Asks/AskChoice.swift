/// An answer the person gives to a question before an agent sends, in the order the answers are offered.
public enum AskChoice: String, Equatable, Hashable, Sendable, CaseIterable {
  /// Allow for This Task: the rest of that agent's task in that account, for up to an hour; the main answer.
  case task
  /// Just This Once: this very request.
  case once
  /// Deny: nothing is sent; always offered.
  case deny
}
