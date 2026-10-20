/// Whose action it was, which an activity row's disc shows.
package enum ActivityKind: Sendable {
  /// The person's own, in this viewer: a blue disc.
  case yours
  /// An agent's or another viewer's: a quiet disc.
  case others
  /// One that failed, whoever ran it: an orange disc with "!".
  case failed
}
