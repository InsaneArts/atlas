/// The person's answer to a question: the rest of the task in that account, this one request, or no.
public enum AskAnswer: String, Codable, Equatable, Sendable, CaseIterable {
  /// Allow for This Task: the rest of that agent's task in that account, for up to an hour.
  case task
  /// Just This Once: this very request.
  case once
  /// Deny: nothing is sent.
  case deny
}
