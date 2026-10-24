import AtlasOnboarding
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter account")
struct OnboardingPresenterAccountTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("picking an existing account says what approving in macOS will do, and the account set up before needs none")
  func pickingAnExistingAccountExplainsTheApproval() throws {
    var choose = Samples.snapshot(try Samples.person(nil), Samples.choices())
    #expect(OnboardingPresenter.choose(choose).approval == nil, "nothing picked")
    choose.picked = .existing("testaccount")
    #expect(OnboardingPresenter.choose(choose).approval == OnboardingWords.approveToReady)
    #expect(OnboardingWords.approveToReady.contains("Test") == false, "the footnote names no account")
    choose.picked = .newAccount
    #expect(OnboardingPresenter.choose(choose).approval == nil, "the form explains a new account")
  }

  @Test("the choice never picks an account it does not show, and offers no new account once one was set up")
  func onlyShownAccountsArePicked() throws {
    var unpaired = Samples.helper(answering: false)
    unpaired["paired"] = false
    var snapshot = Samples.snapshot(try Samples.person(unpaired))
    snapshot.facts.candidates = [OnboardingFacts.Candidate(name: "atlas", realName: "Atlas")]
    snapshot.facts.onlyChoice = true
    snapshot.picked = .newAccount
    let state = OnboardingPresenter.state(snapshot)
    #expect(state.footer.next?.isEnabled == false, "a new account is not offered, so it is not picked")
    guard case .screen(.choose(let choose)) = state.content else {
      Issue.record("the choice is not shown")
      return
    }
    #expect(!choose.offersNewAccount && !choose.isNewAccountPicked)
    snapshot.picked = .existing("qa")
    #expect(OnboardingPresenter.state(snapshot).footer.next?.isEnabled == false, "qa is not listed")
  }

  @Test("a password problem replaces the work note on the form, and the work note shows once the password is right")
  func passwordProblem() throws {
    var snapshot = Samples.snapshot(try Samples.person(nil), Samples.choices(creating: true))
    snapshot.problem = "The account could not be created."
    snapshot.form.password = "one"
    snapshot.form.verify = "two"
    #expect(OnboardingPresenter.create(snapshot).note == .problem("The two passwords are different."))
    snapshot.form.verify = "one"
    #expect(OnboardingPresenter.create(snapshot).note == .problem("The account could not be created."))
    snapshot.working = OnboardingWords.creating
    let working = OnboardingPresenter.create(snapshot)
    #expect(working.note == .working(OnboardingWords.creating))
    #expect(working.tail == "You’ll approve it once in macOS." && !working.isFormEnabled)
  }

  @Test("a problem setting up an account shows in that account's row, not under the list")
  func aProblemWithAnAccountShowsInItsRow() throws {
    var snapshot = Samples.snapshot(try Samples.person(nil), Samples.choices())
    snapshot.picked = .existing("testaccount")
    snapshot.problem = "macOS could not say whether testaccount is an administrator."
    snapshot.problemAbout = "testaccount"
    let row = try #require(OnboardingPresenter.choose(snapshot).accounts.first)
    #expect(row.problem == snapshot.problem)
    #expect(OnboardingPresenter.note(snapshot) == .none, "the line under the list stays empty")
    snapshot.problemAbout = nil
    #expect(OnboardingPresenter.choose(snapshot).accounts.first?.problem == nil)
    #expect(OnboardingPresenter.note(snapshot) == .problem(try #require(snapshot.problem)))
  }
}
