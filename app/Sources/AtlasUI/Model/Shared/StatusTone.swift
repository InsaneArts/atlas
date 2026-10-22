/// How a status reads at a glance, which its mark's colour and its word's colour show.
package enum StatusTone: Sendable {
  /// Done and ready, such as a picture that is live.
  case ready
  /// Something is working now.
  case active
  /// The person holds control: a blue mark and a word in the link colour.
  case link
  /// The person is needed.
  case attention
  /// Nothing to say: a grey mark.
  case quiet
}
