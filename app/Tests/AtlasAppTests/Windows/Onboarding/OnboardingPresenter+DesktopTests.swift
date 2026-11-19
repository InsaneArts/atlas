import AtlasOnboarding
import AtlasOnboardingTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter desktop")
struct OnboardingPresenterDesktopTests {
  /// Facts of settings that list the VM `vm`, its add as given.
  ///
  /// - Parameter vm: The VM's part of the reply.
  /// - Returns: The facts.
  /// - Throws: When the reply is not a status.
  private func facts(vm: [String: Any]) throws -> OnboardingFacts {
    try AtlasOnboardingTesting.facts([
      "ok": true, "role": "controller", "configured": true, "candidates": [], "vm": vm,
      "desktops": ["vm": ["available": true]],
    ])
  }

  @Test("the VM row can be picked only when this Mac can run it, and says why not at its end")
  func theVMRowNeedsRoom() throws {
    let full = try AtlasOnboardingTesting.facts([
      "ok": true, "role": "controller", "configured": false, "candidates": [],
      "desktops": ["vm": ["available": false, "why": "running"]],
    ])
    var snapshot = OnboardingSamples.snapshot(full, OnboardingSamples.choices(desktop: nil))
    snapshot.pickedDesktop = .vm
    let rows = OnboardingPresenter.desktop(snapshot).items
    #expect(
      rows.map(\.standing) == [.choice(isPicked: true), .notYet("Two VMs already run"), .notYet("Not available yet")]
    )
    #expect(!snapshot.onVM, "the track stays Second Login's")
  }

  @Test("an add whose process ended offers Go On, and one that failed offers Try Again and says why")
  func aHeldAddOffersGoOnOrTryAgain() throws {
    let base: [String: Any] = ["name": "vm", "lifecycle": "adding", "steps": ["agent"]]
    var ended = base
    ended["adder"] = ["running": false]
    ended["progress"] = ["phase": "downloading", "received": 12_400_000_000, "total": 25_600_000_000]
    let stopped = OnboardingSamples.snapshot(try facts(vm: ended))
    #expect(OnboardingPresenter.state(stopped).footer.next?.event == .goOnAdding)
    #expect(
      OnboardingPresenter.vmDownload(stopped)
        == .init(
          fraction: 12.4 / 25.6,
          amount: "12.4 GB of 25.6 GB",
          isHeld: true,
          problem: "Setting up the VM stopped."
        )
    )
    var failed = ended
    failed["failure"] = "Making the VM took too long. Run atlas desktop add vm again to go on."
    let snapshot = OnboardingSamples.snapshot(try facts(vm: failed))
    #expect(OnboardingPresenter.state(snapshot).footer.next?.event == .retryAdding)
    #expect(OnboardingPresenter.vmProblem(snapshot) == "Atlas couldn’t make the VM: making the VM took too long.")
    var running = ended
    running["adder"] = ["running": true]
    let going = OnboardingSamples.snapshot(try facts(vm: running))
    #expect(OnboardingPresenter.state(going).footer.next?.isEnabled == false, "Continue waits while it goes on")
    #expect(OnboardingPresenter.state(going).footer.back == nil)
  }

  @Test("a failure's reason follows the colon in lower case, never a name or a word in capitals")
  func aReasonIsAClause() {
    #expect(OnboardingWords.clause("The VM's disk could not be made.") == "the VM's disk could not be made.")
    #expect(OnboardingWords.clause("Atlas VM did not take the VM's create") == "Atlas VM did not take the VM's create")
    #expect(OnboardingWords.clause("VM's disk is full") == "VM's disk is full")
  }
}
