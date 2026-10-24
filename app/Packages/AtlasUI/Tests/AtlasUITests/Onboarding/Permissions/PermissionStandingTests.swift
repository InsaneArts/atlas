import Testing

@testable import AtlasUI

@Suite("OnboardingPermissionStanding")
struct PermissionStandingTests {
  @Test("only a permission whose turn has not come dims its row")
  func onlyNextDims() {
    let standings: [OnboardingPermissionStanding] = [
      .allowed("Allowed"), .waiting("Waiting for you…"), .yourTurn, .next("Next"), .turnedOff("Turned off"),
      .checking("Checking…"),
    ]
    #expect(standings.filter(\.dimsRow) == [.next("Next")])
  }
}
