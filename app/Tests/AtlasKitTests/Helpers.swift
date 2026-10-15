import Foundation
import Testing

@testable import AtlasKit

/// A status reply as the viewer service writes it.
func status(_ reply: [String: Any]) throws -> ViewerStatus {
    try JSONDecoder().decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
}

/// Facts from a status reply as `atlas onboarding status` writes it.
func facts(_ reply: [String: Any]) throws -> OnboardingFacts {
    try #require(OnboardingFacts(json: JSONSerialization.data(withJSONObject: reply)))
}

/// Wait until `done` holds, for code that works in the background; after
/// two seconds the test fails with `message`.
@MainActor
func settle(_ message: String = "the condition never held", _ done: () -> Bool) async {
    let deadline = ContinuousClock.now + .seconds(2)
    while !done() {
        guard ContinuousClock.now < deadline else {
            Issue.record(Comment(rawValue: message))
            return
        }
        try? await Task.sleep(for: .milliseconds(5))
    }
}
