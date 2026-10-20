/// How many times a stand-in effect, such as quitting Atlas, was called, so no test performs the real one.
@MainActor
final class CallCount {
  /// The calls so far.
  var count = 0
}
