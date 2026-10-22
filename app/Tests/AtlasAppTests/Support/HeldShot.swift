/// A screenshot request the scripted service holds until the test lets it answer.
struct HeldShot {
  /// The screenshot asked for.
  let name: String
  /// Whether it was asked for as taken, rather than as a thumbnail.
  let full: Bool
  /// Lets it answer.
  let resume: CheckedContinuation<Void, Never>
  /// Whether it was let answer already.
  var answered = false
}
