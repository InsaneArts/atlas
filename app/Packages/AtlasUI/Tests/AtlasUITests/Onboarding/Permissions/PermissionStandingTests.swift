import Testing

@testable import AtlasUI

@Suite("OnboardingPermissionStanding")
struct PermissionStandingTests {
  @Test("a row offers Allow… only for the permission to allow next, and only where it takes a press")
  func allowOnlyForTheNext() {
    let next = OnboardingPermissionItem(
      id: "accessibility",
      title: "Accessibility",
      isOn: false,
      status: "Off",
      allowTitle: "Allow…",
      canAllow: true
    )
    #expect(OnboardingPermissionStanding.of(next, offersAllow: true) == .allow("Allow…"))
    #expect(OnboardingPermissionStanding.of(next, offersAllow: false) == .off("Off"), "the panel only shows")
    var waiting = next
    waiting.canAllow = false
    #expect(OnboardingPermissionStanding.of(waiting, offersAllow: true) == .off("Off"))
    var on = next
    on.isOn = true
    on.status = "Allowed"
    #expect(OnboardingPermissionStanding.of(on, offersAllow: true) == .allowed("Allowed"))
  }
}
