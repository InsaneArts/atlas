extension OnboardingFacts {
  /// Which side of setup an account is on.
  package enum Role: String, Decodable, Sendable {
    /// The person's own account, which chooses and drives the helper.
    case controller
    /// The helper's account.
    case helper
  }
}
