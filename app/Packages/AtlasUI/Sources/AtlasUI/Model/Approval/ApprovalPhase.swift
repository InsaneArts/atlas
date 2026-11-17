/// Where the approval window stands, from reading the request to the person's answer being kept.
public enum ApprovalPhase: Hashable, Sendable {
  /// The request is being read.
  case loading
  /// The request shows, waiting for the person's answer.
  case asking
  /// The answer is being kept.
  case answering(ApprovalChoice)
  /// The answer was kept: allowed or not. The window holds a moment, then closes.
  case answered(allowed: Bool)
  /// The request was answered before the window was opened again: it says what was answered, with Close alone.
  case answeredEarlier(ApprovalEarlierAnswer)
  /// The request's time ran out before an answer: nothing changes.
  case expired
  /// The request can't be shown or answered, for this reason, in Atlas's own words.
  case failed(String)
}
