import Testing

@testable import AtlasOnboarding

@Suite("Permission+Guide")
struct PermissionGuideTests {
  @Test("the window guides the permission allowed now while it is off, then the next one off, then none")
  func toGuide() {
    var asker = OnboardingFacts.Asker()
    #expect(Permission.toGuide(after: nil, asker: asker) == .accessibility, "Accessibility comes first")
    #expect(Permission.toGuide(after: .screenRecording, asker: asker) == .screenRecording, "stays while it is off")
    asker.accessibility = true
    #expect(Permission.toGuide(after: .accessibility, asker: asker) == .screenRecording, "moves on once it is on")
    asker.screenCapture = true
    #expect(Permission.toGuide(after: .screenRecording, asker: asker) == nil, "none left once both are on")
  }
}
