import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter")
struct OnboardingPresenterTests {
  /// The person's account, with a helper as `helper` describes it, or none.
  ///
  /// - Parameters:
  ///   - helper: The helper's side of the reply, or nil before one is chosen.
  ///   - finished: Whether setup has finished.
  /// - Returns: The facts.
  /// - Throws: When the reply is not a status.
  private static func person(_ helper: [String: Any]?, finished: Bool = false) throws -> OnboardingFacts {
    var reply: [String: Any] = [
      "role": "controller", "me": ["name": "me"], "app": ["installed": true, "signed": true],
      "configured": helper != nil, "finished": finished,
      "candidates": [["name": "testaccount", "realName": "Test Account"]],
    ]
    if let helper { reply["helper"] = helper }
    return try facts(reply)
  }

  /// The helper `atlas`, chosen and paired, in the state given.
  ///
  /// - Parameters:
  ///   - signedIn: Whether it is logged in.
  ///   - answering: Whether Atlas answers in it.
  ///   - hidden: Whether its session is off screen.
  ///   - allowed: Whether both permissions are on.
  /// - Returns: The helper's side of the reply.
  private static func helper(
    signedIn: Bool = true,
    answering: Bool = true,
    hidden: Bool = true,
    allowed: Bool = true
  ) -> [String: Any] {
    [
      "exists": true, "name": "atlas", "realName": "Atlas", "administrator": false, "paired": true,
      "signedIn": signedIn, "answering": answering, "onConsole": !hidden, "hidden": hidden,
      "permissions": ["screenCapture": allowed, "accessibility": allowed],
    ]
  }

  /// The helper's account, asked by `asker` (none when empty), with Atlas running as given.
  ///
  /// - Parameters:
  ///   - asker: The asking account's short name; its full name is Ismayil.
  ///   - running: Whether Atlas's job runs here.
  ///   - accessibility: Whether Accessibility is on.
  ///   - screen: Whether Screen Recording is on.
  /// - Returns: The facts.
  /// - Throws: When the reply is not a status.
  private static func helperSide(
    asker: String = "theiskaa",
    running: Bool = true,
    accessibility: Bool = false,
    screen: Bool = false
  ) throws -> OnboardingFacts {
    var controller: [String: Any] = ["uid": 501, "name": asker]
    if !asker.isEmpty { controller["realName"] = "Ismayil" }
    return try facts([
      "role": "helper", "me": ["name": "atlas", "realName": "Atlas"], "app": ["installed": true, "signed": true],
      "helper": [
        "controller": controller, "installed": running, "running": running,
        "permissions": ["screenCapture": screen, "accessibility": accessibility],
      ],
    ])
  }

  /// A snapshot that has read `facts` once.
  ///
  /// - Parameters:
  ///   - facts: The facts read.
  ///   - choices: The person's choices.
  /// - Returns: The snapshot.
  private static func snapshot(_ facts: OnboardingFacts, _ choices: OnboardingChoices = .init()) -> OnboardingSnapshot {
    OnboardingSnapshot(facts: facts, choices: choices, loaded: true)
  }

  /// Choices past the welcome, on the form when `creating`.
  ///
  /// - Parameters:
  ///   - creating: Whether the person is on the account form.
  ///   - agentsDone: Whether the agents screen is behind them.
  /// - Returns: The choices.
  private static func choices(creating: Bool = false, agentsDone: Bool = false) -> OnboardingChoices {
    var choices = OnboardingChoices()
    choices.started = true
    choices.creating = creating
    choices.agentsDone = agentsDone
    return choices
  }

  /// Which screen a state shows, if it shows one.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The screen's id, or nil for the first reading or the guide.
  private static func screen(_ state: OnboardingState) -> OnboardingScreen.ID? {
    if case .screen(let screen) = state.content { return screen.id }
    return nil
  }

  @Test("each step shows its screen, and loading shows until the first reading")
  func eachStepShowsItsScreen() throws {
    let fresh = try Self.person(nil)
    #expect(OnboardingPresenter.state(OnboardingSnapshot(facts: fresh)).content == .loading(problem: nil))
    let failed = OnboardingPresenter.state(OnboardingSnapshot(facts: fresh, problem: "unknown command"))
    #expect(failed.content == .loading(problem: "unknown command"))
    #expect(failed.footer == OnboardingFooterState())
    let cases: [(OnboardingSnapshot, OnboardingScreen.ID)] = [
      (Self.snapshot(fresh), .welcome),
      (Self.snapshot(fresh, Self.choices()), .choose),
      (Self.snapshot(fresh, Self.choices(creating: true)), .create),
      (Self.snapshot(try Self.person(Self.helper(signedIn: false, answering: false))), .switchToHelper),
      (Self.snapshot(try Self.person(Self.helper())), .finishing),
      (Self.snapshot(try Self.person(Self.helper(), finished: true)), .agents),
      (Self.snapshot(try Self.person(Self.helper(), finished: true), Self.choices(agentsDone: true)), .done),
      (Self.snapshot(try Self.helperSide(running: false)), .allow),
      (Self.snapshot(try Self.helperSide()), .permissions),
      (Self.snapshot(try Self.helperSide(accessibility: true, screen: true)), .helperDone),
      (Self.snapshot(try Self.helperSide(asker: "", running: false)), .blocked(Self.notRunning)),
    ]
    for (snapshot, screen) in cases {
      #expect(Self.screen(OnboardingPresenter.state(snapshot)) == screen)
    }
    var guiding = OnboardingSnapshot(facts: try Self.helperSide(), guiding: .accessibility)
    #expect(OnboardingPresenter.state(guiding).content.isGuide, "the guide shows even before the first reading")
    guiding.loaded = true
    #expect(OnboardingPresenter.state(guiding).footer == OnboardingFooterState())
  }

  /// What blocks a helper whose Atlas does not run, when no account asked.
  private static let notRunning =
    "Atlas isn't running in this account. In your own account, open Atlas and choose this account as the helper again."

  @Test("Back shows on the account choice before setup, on the form and on Done")
  func backShows() throws {
    let fresh = try Self.person(nil)
    #expect(OnboardingPresenter.state(Self.snapshot(fresh, Self.choices())).footer.back != nil)
    #expect(OnboardingPresenter.state(Self.snapshot(fresh, Self.choices(creating: true))).footer.back != nil)
    let done = Self.snapshot(try Self.person(Self.helper(), finished: true), Self.choices(agentsDone: true))
    #expect(OnboardingPresenter.state(done).footer.back?.event == .back)
    var gone = Self.helper()
    gone["exists"] = false
    let chosenBefore = try Self.person(gone)
    #expect(OnboardingPresenter.state(Self.snapshot(chosenBefore)).footer.back == nil, "a helper was set up before")
    #expect(OnboardingPresenter.state(Self.snapshot(fresh)).footer.back == nil)
    #expect(OnboardingPresenter.state(Self.snapshot(try Self.person(Self.helper(), finished: true))).footer.back == nil)
  }

  @Test("every footer button waits while a command runs, and each Continue waits for its own condition")
  func footerButtonsWait() throws {
    var choose = Self.snapshot(try Self.person(nil), Self.choices())
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == false, "no account picked")
    choose.picked = .newAccount
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == true)
    choose.working = "Setting up qa…"
    #expect(OnboardingPresenter.state(choose).footer.next?.isEnabled == false)
    #expect(OnboardingPresenter.state(choose).footer.back?.isEnabled == false)
    var create = Self.snapshot(try Self.person(nil), Self.choices(creating: true))
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == false, "no password yet")
    create.form.password = "secret"
    create.form.verify = "secret"
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == true)
    create.working = "Creating the account…"
    #expect(OnboardingPresenter.state(create).footer.next?.isEnabled == false)
    var finishing = Self.snapshot(try Self.person(Self.helper()))
    finishing.finish = .running
    #expect(OnboardingPresenter.state(finishing).footer.next?.isEnabled == false)
    finishing.finish = .passed
    let passed = try #require(OnboardingPresenter.state(finishing).footer.next)
    #expect(passed.isEnabled && !passed.isDefault, "Return does not skip the check's result")
    var agents = Self.snapshot(try Self.person(Self.helper(), finished: true))
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == false, "agents not read yet")
    agents.agentsLoaded = true
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == true)
    agents.working = "Connecting your agents…"
    #expect(OnboardingPresenter.state(agents).footer.next?.isEnabled == false)
    var done = Self.snapshot(try Self.person(Self.helper(), finished: true), Self.choices(agentsDone: true))
    #expect(OnboardingPresenter.state(done).footer.next?.event == .complete)
    #expect(OnboardingPresenter.state(done).footer.next?.isEnabled == true)
    done.working = "Finishing…"
    #expect(OnboardingPresenter.state(done).footer.next?.isEnabled == false)
  }

  @Test(
    "the switch screen names the next step until the person switches back",
    arguments: [
      (false, false, true, false, "Next: Lock and Switch, then log in to the Atlas account."),
      (true, false, false, false, "Waiting for Atlas to open in the Atlas account…"),
      (true, false, true, false, "Next: Lock and Switch, then open Atlas there."),
      (true, true, true, false, "Waiting for the two permissions…"),
      (true, true, false, true, "Waiting for you to switch back…"),
    ] as [(Bool, Bool, Bool, Bool, String)]
  )
  func switchWaiting(signedIn: Bool, answering: Bool, hidden: Bool, allowed: Bool, waiting: String) throws {
    var snapshot = Self.snapshot(
      try Self.person(Self.helper(signedIn: signedIn, answering: answering, hidden: hidden, allowed: allowed))
    )
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == waiting)
    snapshot.working = Words.locking
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == nil, "the work note shows instead")
  }

  @Test("Lock and Switch shows until the helper is signed in on screen")
  func lockButton() throws {
    let out = try Self.person(Self.helper(signedIn: false, answering: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Self.snapshot(out)))?.showsLockButton == true)
    let hidden = try Self.person(Self.helper(answering: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Self.snapshot(hidden)))?.showsLockButton == true)
    let onScreen = try Self.person(Self.helper(answering: false, hidden: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Self.snapshot(onScreen)))?.showsLockButton == false)
  }

  /// The switch screen a state shows, if it shows it.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The switch screen, or nil.
  private static func switchScreen(_ state: OnboardingState) -> OnboardingSwitchScreenState? {
    if case .screen(.switchToHelper(let screen)) = state.content { return screen }
    return nil
  }

  @Test("the finishing check reads checking, passed or failed, with Try Again on failure")
  func finishingCheck() throws {
    var snapshot = Self.snapshot(try Self.person(Self.helper()))
    snapshot.finish = .running
    let running = OnboardingPresenter.finishing(snapshot)
    #expect(running.title == "Checking everything")
    #expect(running.loggedIn == "The Atlas account is logged in", "the default helper is named as an account")
    #expect(running.live == .busy && running.liveNote == "checking…")
    #expect(running.failure == nil)
    snapshot.finish = .passed
    let passed = OnboardingPresenter.finishing(snapshot)
    #expect(passed.title == "Everything works.")
    #expect(passed.live == .done && passed.liveNote == "passed")
    snapshot.finish = .failed("The test screenshot never arrived")
    let failed = OnboardingPresenter.finishing(snapshot)
    #expect(failed.live == .failed && failed.liveNote == "failed")
    #expect(failed.title == "Almost there.", "a failed check no longer reads as checking")
    #expect(failed.failure == "The test screenshot never arrived")
  }

  @Test("an agent reads connected, found or not installed, and only a found, unconnected one can be switched")
  func agentRows() throws {
    func agent(_ id: AgentID, found: Bool, connected: Bool) -> AgentChoice {
      var agent = AgentChoice(id: id)
      agent.checked = true
      agent.found = found
      agent.connected = connected
      return agent
    }
    var snapshot = Self.snapshot(try Self.person(Self.helper(), finished: true))
    snapshot.agents = [agent(.claude, found: true, connected: true), agent(.codex, found: true, connected: false)]
    let state = OnboardingPresenter.agents(snapshot)
    #expect(state.agents.map(\.title) == ["Claude Code", "Codex"])
    #expect(state.agents.map(\.detail) == [OnboardingWords.agentConnected, OnboardingWords.agentFound])
    #expect(state.agents.map(\.isOn) == [true, false], "a connected agent's switch is on")
    #expect(state.agents.map(\.isEnabled) == [false, true])
    snapshot.agents = [agent(.claude, found: false, connected: false)]
    let missing = try #require(OnboardingPresenter.agents(snapshot).agents.first)
    #expect(missing.detail == OnboardingWords.agentMissing)
    #expect(!missing.isEnabled)
    snapshot.agents = AgentChoice.all
    snapshot.agentsFailed = true
    let unchecked = OnboardingPresenter.state(snapshot)
    #expect(
      Self.agentsScreen(unchecked)?.agents.map(\.detail) == [
        OnboardingWords.agentNotChecked, OnboardingWords.agentNotChecked,
      ],
      "an agent the failed reading never checked reads unchecked, not missing"
    )
    #expect(unchecked.footer.next?.isEnabled == true, "Continue skips the step")
  }

  /// The agents screen a state shows, if it shows it.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The agents screen, or nil.
  private static func agentsScreen(_ state: OnboardingState) -> OnboardingAgentsScreenState? {
    if case .screen(.agents(let screen)) = state.content { return screen }
    return nil
  }

  @Test("the choice never picks an account it does not show, and offers no new account once one was set up")
  func onlyShownAccountsArePicked() throws {
    var unpaired = Self.helper(answering: false)
    unpaired["paired"] = false
    var snapshot = Self.snapshot(try Self.person(unpaired))
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

  @Test("the switch offers Choose Another Account in Back's place only while the helper asked never answered")
  func chooseAnotherWhileUnanswered() throws {
    var helper = Self.helper(signedIn: false, answering: false, allowed: false)
    helper["forgettable"] = true
    let unanswered = OnboardingPresenter.state(Self.snapshot(try Self.person(helper)))
    #expect(unanswered.footer.back?.event == .chooseAnother)
    helper["forgettable"] = false
    #expect(OnboardingPresenter.state(Self.snapshot(try Self.person(helper))).footer.back == nil)
  }

  @Test("the guide counts its permission and ticks its steps once the switch is on")
  func guideSteps() throws {
    let off = OnboardingPresenter.guide(.screenRecording, asker: try Self.helperSide(accessibility: true).asker)
    #expect(off.label == "2 of 2 · Screen Recording")
    #expect([off.turnOn, off.comeBack] == [.current, .waiting])
    #expect(off.pill == "Waiting for Screen Recording…" && !off.isOn)
    let on = OnboardingPresenter.guide(.accessibility, asker: try Self.helperSide(accessibility: true).asker)
    #expect(on.label == "1 of 2 · Accessibility")
    #expect([on.turnOn, on.comeBack] == [.done, .done])
    #expect(on.pill == "Accessibility is on" && on.isOn)
  }

  @Test("Accessibility is asked for before Screen Recording")
  func accessibilityFirst() throws {
    let neither = OnboardingPresenter.permissions(try Self.helperSide().asker)
    #expect(neither.next?.id == Permission.accessibility.id)
    #expect(neither.accessibility.status == "Off")
    let one = OnboardingPresenter.permissions(try Self.helperSide(accessibility: true).asker)
    #expect(one.next?.allowTitle == "Allow Screen Recording")
    #expect(one.accessibility.status == "Allowed")
    let both = OnboardingPresenter.permissions(try Self.helperSide(accessibility: true, screen: true).asker)
    #expect(both.next == nil)
  }

  @Test("the helper's done screen names the account to switch back to, or the person's own")
  func helperDoneTail() throws {
    let asked = OnboardingPresenter.helperDone(try Self.helperSide(accessibility: true, screen: true).asker)
    #expect(asked.tail == "Switch back to Ismayil.")
    let unasked = OnboardingPresenter.helperDone(
      try Self.helperSide(asker: "", accessibility: true, screen: true).asker
    )
    #expect(unasked.tail == "Switch back to your own account.")
  }

  @Test("a password problem replaces the work note on the form, and the work note shows once the password is right")
  func passwordProblem() throws {
    var snapshot = Self.snapshot(try Self.person(nil), Self.choices(creating: true))
    snapshot.problem = "The account could not be created."
    snapshot.form.password = "one"
    snapshot.form.verify = "two"
    #expect(OnboardingPresenter.create(snapshot).note == .problem("The two passwords are different."))
    snapshot.form.verify = "one"
    #expect(OnboardingPresenter.create(snapshot).note == .problem("The account could not be created."))
    snapshot.working = "Creating the account…"
    let working = OnboardingPresenter.create(snapshot)
    #expect(working.note == .working("Creating the account…"))
    #expect(working.tail == "Confirm in the macOS dialog." && !working.isFormEnabled)
  }

  @Test("the person's side counts six dots and the helper's three; a blocked screen shows none")
  func dots() {
    #expect(OnboardingStep.welcome.dots == OnboardingFooterState.Dots(index: 0, count: 6))
    #expect(OnboardingStep.createAccount.dots == OnboardingStep.chooseHelper.dots, "the form shares the choice's dot")
    #expect(OnboardingStep.done.dots?.index == 5)
    #expect(OnboardingStep.helperDone.dots == OnboardingFooterState.Dots(index: 2, count: 3))
    #expect(OnboardingStep.blocked("x").dots == nil)
  }
}

extension OnboardingContent {
  /// Whether this is the guide panel.
  fileprivate var isGuide: Bool {
    if case .guide = self { return true }
    return false
  }
}
