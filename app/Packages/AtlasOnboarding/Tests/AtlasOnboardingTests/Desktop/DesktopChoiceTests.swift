import AtlasOnboardingTesting
import Foundation
import Testing

@testable import AtlasOnboarding

@Suite("DesktopChoice")
struct DesktopChoiceTests {
  @Test("the choice lists Second Login, VM and Cloud, and Cloud is not offered yet")
  func theThreeKindsInOrder() throws {
    let choices = DesktopChoice.all(try onVM(vm(), room: ["available": true]))
    #expect(choices.map(\.kind) == [.secondLogin, .vm, .cloud])
    #expect(choices.map(\.canBeChosen) == [true, true, false])
    #expect(choices.last?.lack == .notYet)
  }

  @Test(
    "the VM row can be picked only when this Mac can run it",
    arguments: [
      ("macos", DesktopChoice.Lack.macOS), ("chip", .chip), ("disk", .disk), ("running", .running),
      ("something new", .notYet),
    ]
  )
  func theVMRowNeedsRoom(why: String, lack: DesktopChoice.Lack) throws {
    let facts = try onVM(vm(), room: ["available": false, "why": why])
    let row = DesktopChoice.all(facts)[1]
    #expect(row.kind == .vm)
    #expect(row.lack == lack)
    #expect(!row.canBeChosen)
  }

  @Test("an Atlas that does not say whether this Mac can run a VM offers none yet")
  func anOlderStatusOffersNoVM() throws {
    #expect(DesktopChoice.all(try controller())[1].lack == .notYet)
  }

  @Test("Second Login is picked when the VM picked can no longer be chosen")
  func aPickThatCannotBeChosenFallsBack() throws {
    let able = try onVM(vm(), room: ["available": true])
    let full = try onVM(vm(), room: ["available": false, "why": "disk"])
    #expect(DesktopChoice.shown(.vm, facts: able) == .vm)
    #expect(DesktopChoice.shown(.vm, facts: full) == .secondLogin)
    #expect(DesktopChoice.shown(.cloud, facts: able) == .secondLogin)
    #expect(DesktopChoice.shown(.secondLogin, facts: full) == .secondLogin)
  }
}
