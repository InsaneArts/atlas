import AtlasOnboarding
import Foundation
import Testing

/// Facts from a status reply as `atlas onboarding status` writes it.
///
/// - Parameter reply: The reply's JSON fields.
/// - Returns: The facts.
/// - Throws: When the reply is not a status.
public func facts(_ reply: [String: Any]) throws -> OnboardingFacts {
  try #require(OnboardingFacts(json: JSONSerialization.data(withJSONObject: reply)))
}
