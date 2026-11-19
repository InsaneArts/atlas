import AtlasOnboardingTesting
import Foundation
import Testing

@testable import AtlasOnboarding

@Suite("VMFacts")
struct VMFactsTests {
  @Test("an add whose process ended offers Go On, and one that failed says why")
  func anEndedAddOffersGoOn() throws {
    let ended = try #require(try onVM(vm(progress: "downloading", adderRunning: false)).vm)
    let failed = try #require(try onVM(vm(adderRunning: false, failure: "The VM's disk could not be made.")).vm)
    let unknown = try #require(try onVM(vm(adderRunning: NSNull())).vm)
    #expect(VMAdd.of(ended, runningHere: false, failedHere: nil) == .stopped)
    #expect(VMAdd.of(ended, runningHere: true, failedHere: nil) == .running, "this window runs it again")
    #expect(VMAdd.of(failed, runningHere: false, failedHere: nil) == .failed("The VM's disk could not be made."))
    #expect(VMAdd.of(failed, runningHere: false, failedHere: "Refused.") == .failed("Refused."))
    #expect(
      VMAdd.of(unknown, runningHere: false, failedHere: nil) == .running,
      "a process not known never reads as ended"
    )
    #expect(VMAdd.of(nil, runningHere: false, failedHere: "Not allowed.") == .failed("Not allowed."))
  }

  @Test("the download's share is its bytes received of the whole")
  func theDownloadsShare() throws {
    let downloading = try #require(try onVM(vm(progress: "downloading")).vm)
    let share = try #require(downloading.progress?.downloaded)
    #expect(abs(share - 12.4 / 25.6) < 0.0001)
    #expect(VMFacts.Progress(phase: .checking).downloaded == nil)
    #expect(VMFacts.Progress(phase: .downloading, fraction: 1.5).downloaded == 1)
  }

  @Test("making the VM's four steps are done in order, the first not done the current one")
  func theMakingSteps() throws {
    let starting = try #require(try onVM(vm(steps: ["agent", "created"])).vm)
    #expect(starting.makingSteps == [.done, .current, .waiting, .waiting])
    let connecting = try #require(try onVM(vm(steps: ["created", "started", "enrolled", "installed"])).vm)
    #expect(connecting.makingSteps == [.done, .done, .done, .current])
  }

  @Test("Screen & System Audio Recording comes first, then Accessibility, as the guest says it allows them")
  func theAllowSteps() throws {
    var shown = vm(steps: ["created", "started", "installed", "closed", "shown"])
    #expect(try #require(try onVM(shown).vm).allowSteps == [.current, .waiting])
    shown["allowed"] = ["screenCapture": true, "accessibility": false]
    #expect(try #require(try onVM(shown).vm).allowSteps == [.done, .current])
    shown["setup"] = "resuming"
    #expect(try #require(try onVM(shown).vm).allowSteps == [.done, .done])
  }
}
