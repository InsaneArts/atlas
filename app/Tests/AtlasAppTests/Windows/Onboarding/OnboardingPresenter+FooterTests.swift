import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter footer")
struct OnboardingPresenterFooterTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("Back shows on the account choice before setup, on the form and on Done")
  func backShows() throws {
    let fresh = try Samples.person(nil)
    #expect(OnboardingPresenter.state(Samples.snapshot(fresh, Samples.choices())).footer.back != nil)
    #expect(OnboardingPresenter.state(Samples.snapshot(fresh, Samples.choices(creating: true))).footer.back != nil)
    let done = Samples.snapshot(try Samples.person(Samples.helper(), finished: true), Samples.choices(agentsDone: true))
    #expect(OnboardingPresenter.state(done).footer.back?.event == .back)
    var gone = Samples.helper()
    gone["exists"] = false
    let chosenBefore = try Samples.person(gone)
    #expect(OnboardingPresenter.state(Samples.snapshot(chosenBefore)).footer.back == nil, "a helper was set up before")
    #expect(OnboardingPresenter.state(Samples.snapshot(fresh)).footer.back == nil)
    #expect(
      OnboardingPresenter.state(Samples.snapshot(try Samples.person(Samples.helper(), finished: true))).footer.back
        == nil
    )
  }

  @Test("every footer button waits while a command runs, and each Continue waits for its own condition")
  func footerButtonsWait() throws {
    var choose = Samples.snapshot(try Samples.person(nil), Samples.choices())
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == false, "no account picked")
    choose.picked = .newAccount
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == true)
    choose.working = "Setting up qa…"
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == false)
    #expect(OnboardingPresenter.state(choose).footer.back?.isEnabled == false)
    var create = Samples.snapshot(try Samples.person(nil), Samples.choices(creating: true))
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == false, "no password yet")
    create.form.password = "secret"
    create.form.verify = "secret"
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == true)
    create.working = "Creating the account…"
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == false)
    var finishing = Samples.snapshot(try Samples.person(Samples.helper()))
    finishing.finish = .running
    #expect(OnboardingPresenter.state(finishing).footer.next?.isEnabled == false)
    finishing.finish = .passed
    #expect(OnboardingPresenter.state(finishing).footer.next?.isEnabled == false, "a moment after it passes")
    finishing.passSettled = true
    let passed = try #require(OnboardingPresenter.state(finishing).footer.next)
    #expect(passed.isEnabled && !passed.isDefault, "Return does not skip the check's result")
    var agents = Samples.snapshot(try Samples.person(Samples.helper(), finished: true))
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == false, "agents not read yet")
    agents.agentsLoaded = true
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == true)
    agents.working = "Connecting your agents…"
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == false)
    var done = Samples.snapshot(try Samples.person(Samples.helper(), finished: true), Samples.choices(agentsDone: true))
    #expect(OnboardingPresenter.state(done).footer.next?.event == .complete)
    #expect(OnboardingPresenter.state(done).footer.next?.isEnabled == true)
    done.working = "Finishing…"
    #expect(OnboardingPresenter.state(done).footer.next?.isEnabled == false)
  }

  @Test("the switch offers Choose Another Account in Back's place only while the helper asked never answered")
  func chooseAnotherWhileUnanswered() throws {
    var helper = Samples.helper(signedIn: false, answering: false, allowed: false)
    helper["forgettable"] = true
    let unanswered = OnboardingPresenter.state(Samples.snapshot(try Samples.person(helper)))
    #expect(unanswered.footer.back?.event == .chooseAnother)
    helper["forgettable"] = false
    #expect(OnboardingPresenter.state(Samples.snapshot(try Samples.person(helper))).footer.back == nil)
  }
}
