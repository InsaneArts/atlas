/// What `atlas onboarding forget` reports: the helper it gave up, and whether that account's login is still open, which
/// must be logged out before anyone deletes the account.
public struct HelperForgotten: Decodable, Sendable {
  /// Whether the given-up helper's login is still open.
  public let loggedIn: Bool?
}
