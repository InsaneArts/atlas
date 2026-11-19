import AtlasOnboarding
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter")
struct OnboardingPresenterTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("each step shows its screen, and loading shows until the first reading")
  func eachStepShowsItsScreen() throws {
    let fresh = try Samples.person(nil)
    let loading = OnboardingPresenter.state(OnboardingSnapshot(facts: fresh))
    #expect(loading.content == .loading(problem: nil))
    #expect(loading.footer == OnboardingFooterState())
    let failed = OnboardingPresenter.state(OnboardingSnapshot(facts: fresh, problem: "unknown command"))
    #expect(failed.content == .loading(problem: "unknown command"))
    #expect(failed.footer.next?.event == .checkAgain, "Try Again reads again")
    #expect(failed.footer.back == nil && failed.footer.track == nil)
    let cases: [(OnboardingSnapshot, OnboardingScreen.ID)] = [
      (Samples.snapshot(fresh), .welcome),
      (Samples.snapshot(fresh, Samples.choices()), .choose),
      (Samples.snapshot(fresh, Samples.choices(creating: true)), .create),
      (Samples.snapshot(try Samples.person(Samples.helper(signedIn: false, answering: false))), .switchToHelper),
      (Samples.snapshot(try Samples.person(Samples.helper())), .finishing),
      (Samples.snapshot(try Samples.person(Samples.helper(), finished: true)), .agents),
      (
        Samples.snapshot(try Samples.person(Samples.helper(), finished: true), Samples.choices(agentsDone: true)), .done
      ),
      (Samples.snapshot(try Samples.helperSide(running: false)), .allow),
      (Samples.snapshot(try Samples.helperSide()), .permissions),
      (Samples.snapshot(try Samples.helperSide(accessibility: true, screen: true)), .helperDone),
      (Samples.snapshot(try Samples.helperSide(asker: "", running: false)), .blocked(Self.notRunning)),
    ]
    for (snapshot, screen) in cases {
      #expect(Samples.screen(OnboardingPresenter.state(snapshot)) == screen)
    }
    var guiding = OnboardingSnapshot(facts: try Samples.helperSide(), guiding: .accessibility)
    #expect(
      OnboardingPresenter.state(guiding).content.isPermissionPanel,
      "the panel shows even before the first reading"
    )
    guiding.loaded = true
    let footer = OnboardingPresenter.state(guiding).footer
    #expect(footer.back?.event == .closePanel, "Back to the card, at the panel's bottom left")
    #expect(footer.next == nil && footer.track == nil)
  }

  @Test(
    "the track has seven segments for Second Login and eight for a VM; the helper's three; none while closed or blocked"
  )
  func track() {
    #expect(
      OnboardingStep.welcome.track(onVM: false) == OnboardingFooterState.Track(index: 0, count: 7, label: "Step 1 of 7")
    )
    #expect(OnboardingStep.chooseDesktop.track(onVM: false)?.index == 1)
    #expect(OnboardingStep.chooseDesktop.track(onVM: true)?.count == 8, "the count follows the row picked")
    #expect(
      OnboardingStep.createAccount.track(onVM: false) == OnboardingStep.chooseHelper.track(onVM: false),
      "the form shares the choice's"
    )
    #expect(
      OnboardingStep.done.track(onVM: false) == OnboardingFooterState.Track(index: 6, count: 7, label: "Step 7 of 7")
    )
    let vm: [OnboardingStep] = [
      .welcome, .chooseDesktop, .downloadVM, .installVM, .allowVM, .finishing, .agents, .done,
    ]
    #expect(vm.map { $0.track(onVM: true)?.index } == Array(0..<8))
    #expect(vm.allSatisfy { $0.track(onVM: true)?.count == 8 })
    #expect(OnboardingStep.allowVM.track(onVM: true)?.label == "Step 5 of 8")
    #expect(
      OnboardingStep.helperDone.track(onVM: false)
        == OnboardingFooterState.Track(index: 2, count: 3, label: "Step 3 of 3")
    )
    #expect(OnboardingStep.blocked("x").track(onVM: false) == nil)
    #expect(OnboardingStep.closeLeftovers.track(onVM: true) == nil)
  }

  /// What blocks a helper whose Atlas does not run, when no account asked.
  private static let notRunning =
    "Atlas isn’t running in this account. In your own account, open Atlas and choose this account as the helper account again."
}

extension OnboardingContent {
  /// Whether this is the permission panel.
  fileprivate var isPermissionPanel: Bool {
    if case .permissionPanel = self { return true }
    return false
  }
}
