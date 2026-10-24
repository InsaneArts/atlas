import AtlasOnboardingTesting
import Foundation
import Testing

@testable import AtlasOnboarding

@Suite("OnboardingStep+Rule")
struct OnboardingStepRuleTests {
  func step(_ facts: OnboardingFacts, _ choices: OnboardingChoices = .init()) -> OnboardingStep {
    OnboardingStep(facts: facts, choices: choices)
  }

  @Test("a fresh Mac starts with the welcome, then the account choice or form")
  func freshMac() throws {
    let facts = try controller()
    var choices = OnboardingChoices()
    #expect(step(facts, choices) == .welcome)
    choices.started = true
    #expect(step(facts, choices) == .chooseHelper)
    choices.creating = true
    #expect(step(facts, choices) == .createAccount)
  }

  @Test("the switch screen stays until the helper is allowed, hidden and answering")
  func switchScreen() throws {
    #expect(step(try controller(helper(signedIn: false, answering: false, allowed: false))) == .switchToHelper)
    #expect(step(try controller(helper(hidden: false, allowed: true))) == .switchToHelper)
    #expect(step(try controller(helper())) == .finishing)
    var screenOnly = helper()
    screenOnly["permissions"] = ["screenCapture": true, "accessibility": false]
    #expect(step(try controller(screenOnly)) == .switchToHelper, "Screen Recording alone is not allowed")
  }

  @Test("a choice cut short offers the choice again")
  func cutShort() throws {
    let unpaired = try controller(helper(signedIn: false, answering: false, allowed: false, paired: false))
    #expect(step(unpaired) == .chooseHelper)
    // A helper that already answers (set up in a terminal) needs no request.
    #expect(step(try controller(helper(paired: false))) == .finishing)
  }

  @Test("a finished setup goes to the agents, then done")
  func finished() throws {
    var choices = OnboardingChoices()
    let facts = try controller(helper(), finished: true)
    #expect(step(facts, choices) == .agents)
    choices.agentsDone = true
    #expect(step(facts, choices) == .done)
  }

  @Test("a problem only the person can fix blocks the way")
  func blocked() throws {
    guard case .blocked = step(try controller(helper(admin: true))) else {
      Issue.record("an administrator helper is refused")
      return
    }
    let unsigned = try facts(["role": "controller", "app": ["installed": true, "signed": false]])
    guard case .blocked = step(unsigned) else {
      Issue.record("an unsigned app is refused")
      return
    }
  }

  @Test("a deleted account's open login leads to the close screen, whatever else is true")
  func leftoverLogins() throws {
    var started = OnboardingChoices()
    started.started = true
    #expect(step(try controller(leftovers: [502])) == .closeLeftovers, "before the welcome")
    #expect(step(try controller(helper(), finished: true, leftovers: [502]), started) == .closeLeftovers)
    #expect(step(try controller(helper(admin: true), leftovers: [502])) == .closeLeftovers)
    let unsigned = try facts([
      "role": "controller", "app": ["installed": true, "signed": false],
      "leftoverLogins": [["uid": 502, "kind": "orphan"]],
    ])
    #expect(step(unsigned) == .closeLeftovers, "closing needs no signed app")
  }

  @Test("the close screen says it closed until Continue, then setup goes on")
  func closedLeftovers() throws {
    var choices = OnboardingChoices()
    choices.closedLeftovers = true
    let facts = try controller(helper(), finished: true)
    #expect(step(facts, choices) == .closeLeftovers)
    choices.closedLeftovers = false
    #expect(step(facts, choices) == .agents)
  }

  @Test("the helper's own window never offers to close another account's login")
  func helperNeverCloses() throws {
    var closed = OnboardingChoices()
    closed.closedLeftovers = true
    let facts = try asker(installed: true, running: true, ax: true, screen: true)
    #expect(step(facts, closed) == .helperDone)
  }

  @Test("a helper whose account was deleted is chosen again")
  func deletedHelper() throws {
    var choices = OnboardingChoices()
    let facts = try controller(["exists": false])
    #expect(step(facts, choices) == .chooseHelper, "no welcome again")
    choices.creating = true
    #expect(step(facts, choices) == .createAccount)
  }

  @Test("the helper's side allows, then asks for both permissions")
  func helperSide() throws {
    #expect(step(try asker(installed: false, running: false, ax: false, screen: false)) == .allow)
    #expect(step(try asker(installed: true, running: true, ax: false, screen: false)) == .permissions)
    #expect(step(try asker(installed: true, running: true, ax: true, screen: false)) == .permissions)
    #expect(step(try asker(installed: true, running: true, ax: false, screen: true)) == .permissions)
    #expect(step(try asker(installed: true, running: true, ax: true, screen: true)) == .helperDone)
  }

  @Test("recorded Allow waits for the requesting Atlas instead of asking again")
  func acceptedHelperWaits() throws {
    let waiting = try facts([
      "role": "helper",
      "helper": ["accepted": true, "installed": false, "running": false],
    ])
    guard case .blocked(let message) = step(waiting) else {
      Issue.record("accepted setup must explain how to finish in the requesting account")
      return
    }
    #expect(message.contains("own account"))
  }

  @Test("an interrupted accepted setup keeps its recovery problem visible")
  func acceptanceRecoveryIsVisible() throws {
    let interrupted = try facts([
      "role": "controller", "configured": true,
      "setupProblem": "Unfinished setup remains pending until recovery.",
      "helper": helper(),
    ])
    #expect(step(interrupted) == .blocked("Unfinished setup remains pending until recovery."))
  }

  @Test("a job that does not run is allowed again when an account asks")
  func stoppedJob() throws {
    // An old Atlas's job, installed but not running: allowing again fixes it.
    #expect(step(try asker(installed: true, running: false, ax: true, screen: true)) == .allow)
  }

  @Test("a helper set up in a terminal still checks its permissions")
  func terminalSetup() throws {
    #expect(step(try asker(name: nil, installed: true, running: true, ax: false, screen: false)) == .permissions)
    #expect(step(try asker(name: nil, installed: true, running: true, ax: true, screen: true)) == .helperDone)
    // Not running, and no one asked: nothing to allow here.
    guard case .blocked = step(try asker(name: nil, installed: true, running: false, ax: false, screen: false))
    else {
      Issue.record("a job that does not run, with no one asking, is explained")
      return
    }
  }
}
