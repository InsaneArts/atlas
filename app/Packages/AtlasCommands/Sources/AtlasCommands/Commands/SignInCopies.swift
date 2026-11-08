import AtlasService

/// Whether agents may copy sign-ins and secrets into the helper, as `atlas onboarding sign-ins` says.
public struct SignInCopies: Equatable, Sendable, Decodable {
  /// Whether agents may copy sign-ins in.
  public let signInCopies: Bool

  /// What `atlas onboarding sign-ins` said, or nil when it did not work or said something else.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: Whether copies are allowed now, or nil.
  public static func of(_ outcome: Outcome) -> Bool? {
    guard outcome.ok else { return nil }
    return outcome.decode(SignInCopies.self)?.signInCopies
  }
}
