/// The body of a request about a held sign-in: its lease.
struct SignInLease: Encodable, Sendable {
  /// The sign-in's lease.
  let lease: String
}
