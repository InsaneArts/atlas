#if DEBUG
  import AtlasKit
  import AtlasUI
  import Foundation
  import Testing

  @testable import AtlasApp

  /// The setup window's samples are written out by hand; these keep each one what the window would show for the same
  /// facts.
  @MainActor
  @Suite("PreviewStates+Onboarding")
  struct OnboardingPreviewStatesTests {
    /// The person's account, with one standard account that could be the helper, and the helper `atlas` as given.
    ///
    /// - Parameters:
    ///   - helper: The helper's side of the reply, or nil before one is chosen.
    ///   - finished: Whether setup has finished.
    ///   - installed: Whether Atlas.app is where it must be.
    /// - Returns: The facts.
    /// - Throws: When the reply is not a status.
    func person(_ helper: [String: Any]? = nil, finished: Bool = false, installed: Bool = true) throws
      -> OnboardingFacts
    {
      var reply: [String: Any] = [
        "ok": true, "role": "controller", "me": ["name": "me"], "app": ["installed": installed, "signed": true],
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
    ///   - allowed: Whether both permissions are on.
    /// - Returns: The helper's side of the reply.
    func helper(signedIn: Bool = true, answering: Bool = true, allowed: Bool = true) -> [String: Any] {
      [
        "exists": true, "name": "atlas", "realName": "Atlas", "administrator": false, "paired": true,
        "signedIn": signedIn, "answering": answering, "onConsole": false, "hidden": true,
        "permissions": ["screenCapture": allowed, "accessibility": allowed],
      ]
    }

    /// The helper's account, the account Atlas asked by Ismayil, with Atlas running there as given.
    ///
    /// - Parameters:
    ///   - running: Whether Atlas's job runs here.
    ///   - accessibility: Whether Accessibility is on.
    ///   - screen: Whether Screen Recording is on.
    /// - Returns: The facts.
    /// - Throws: When the reply is not a status.
    func helperSide(running: Bool = true, accessibility: Bool = false, screen: Bool = false) throws -> OnboardingFacts {
      try facts([
        "ok": true, "role": "helper", "me": ["name": "atlas", "realName": "Atlas"],
        "app": ["installed": true, "signed": true],
        "helper": [
          "controller": ["uid": 501, "name": "theiskaa", "realName": "Ismayil"], "installed": running,
          "running": running, "permissions": ["screenCapture": screen, "accessibility": accessibility],
        ],
      ])
    }

    /// A snapshot that has read `facts`, past the welcome when `started`.
    ///
    /// - Parameters:
    ///   - facts: The facts read.
    ///   - started: Whether the person pressed Get Started.
    /// - Returns: The snapshot.
    func loaded(_ facts: OnboardingFacts, started: Bool = false) -> OnboardingSnapshot {
      var snapshot = OnboardingSnapshot(facts: facts, loaded: true)
      snapshot.choices.started = started
      return snapshot
    }

    @Test("each setup window sample is what the window shows for the same facts")
    func samplesMatchThePresenter() throws {
      var choose = loaded(try person(), started: true)
      choose.picked = .newAccount
      var create = choose
      create.choices.creating = true
      var creating = create
      creating.working = OnboardingWords.creating
      var finishing = loaded(try person(helper()))
      finishing.finish = .running
      var passed = finishing
      passed.finish = .passed
      var failed = finishing
      failed.finish = .failed("The test screenshot never arrived.")
      var agents = loaded(try person(helper(), finished: true))
      agents.agents = AgentID.allCases.map { id in
        var agent = AgentChoice(id: id)
        agent.found = true
        agent.on = true
        return agent
      }
      agents.projects = [URL(fileURLWithPath: "/Users/me/dev/Opie")]
      var done = loaded(try person(helper(), finished: true))
      done.choices.agentsDone = true
      var guide = loaded(try helperSide())
      guide.guiding = .accessibility
      let presented: [String: OnboardingSnapshot] = [
        "Loading": OnboardingSnapshot(),
        "Welcome": loaded(try person()),
        "Choose": choose,
        "Create": create,
        "Create, working": creating,
        "Switch": loaded(try person(helper(signedIn: false, answering: false, allowed: false))),
        "Switch, open Atlas": loaded(try person(helper(answering: false, allowed: false))),
        "Finishing": finishing,
        "Finishing, passed": passed,
        "Finishing, failed": failed,
        "Agents": agents,
        "Done": done,
        "Helper allow": loaded(try helperSide(running: false)),
        "Helper permissions": loaded(try helperSide(accessibility: true)),
        "Helper guide": guide,
        "Helper done": loaded(try helperSide(accessibility: true, screen: true)),
        "Blocked": loaded(try person(installed: false)),
      ]
      #expect(
        Set(presented.keys) == Set(PreviewStates.onboarding.map(\.name)),
        "every sample has its facts, and no more"
      )
      for preview in PreviewStates.onboarding {
        let snapshot = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == OnboardingPresenter.state(snapshot), "\(preview.name)")
      }
    }
  }
#endif
