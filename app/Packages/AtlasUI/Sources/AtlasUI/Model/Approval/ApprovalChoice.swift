/// What the approval window sends: the person's answer, or that they closed it.
public enum ApprovalChoice: Hashable, Sendable {
  /// Allow the request.
  case allow
  /// Refuse it: Don't Allow, or Escape while it asks.
  case refuse
  /// Close the window once nothing is left to answer.
  case close
}
