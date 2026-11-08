/// One row a session stands as in the outline's stack, each its own element there, so the list holds the place of
/// whichever the person scrolled to as rows arrive above it.
enum ActivitySessionPart: Hashable {
  /// The hairline above it, when another session stands above it.
  case separator
  /// Its row, with its newest pictures in one box while it is folded.
  case head
  /// One of its runs, by the run's identity.
  case run(String)
}
