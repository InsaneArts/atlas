/// How a status reads at a glance, which its dot's colour shows.
package enum StatusTone: Sendable {
  /// Ready for work.
  case ready
  /// Something is working now.
  case active
  /// The person is needed.
  case attention
  /// Nothing to say.
  case quiet
}
